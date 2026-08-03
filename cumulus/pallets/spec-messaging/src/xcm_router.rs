// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! XCM over Speculative Messaging: the [`SpecMsgRouter`].
//!
//! A [`SendXcm`] router for sibling parachains that appends XCM to the
//! designated spec-msg channel once the pair's HRMP channel is gone. It must
//! sit in the runtime's router tuple immediately BEFORE `XcmpQueue`: both
//! match the same `(1, [Parachain(id)])` destination pattern, and
//! `XcmpQueue::validate` accepts any sibling unconditionally — channel state
//! is only consulted later, in `take_outbound_messages` — so ordered after
//! it this router would never be selected.
//!
//! Transport rule: **HRMP wins while a channel exists.** `Ready` or `Full`
//! falls through to `XcmpQueue` (`Full` is backpressure, not absence); only
//! `Closed` diverts here — a set [`HrmpClosing`] cutover flag counts as
//! `Closed` (drain-before-close; see the pallet docs) — and only if the
//! outbound spec-msg channel to the destination is open. Exhausted capacity
//! is a hard [`SendError::Transport`], never `NotApplicable` — see
//! `validate`.
//!
//! Envelope: on the designated XCM channel (`domain = 0`, `num = 0`) a
//! [`SpecMsgKind::Data`](cumulus_primitives_spec_messaging::SpecMsgKind)
//! payload is exactly the SCALE-encoded `VersionedXcm` — no extra framing.
//! Demultiplexing among userspace protocols is by channel (distinct
//! `domain`/`num`), not in-band.

use crate::{Config, HrmpClosing, OutboundMessages, Pallet};
use alloc::vec::Vec;
use codec::{DecodeAll, DecodeLimit, Encode};
use core::marker::PhantomData;
use cumulus_primitives_core::{ChannelStatus, GetChannelInfo, ParaId};
use cumulus_primitives_spec_messaging::{ChannelId, SpecMsgKind, StreamId};
use polkadot_runtime_common::xcm_sender::PriceForMessageDelivery;
use xcm::{latest::prelude::*, VersionedLocation, VersionedXcm, WrapVersion, MAX_XCM_DECODE_DEPTH};
use xcm_builder::InspectMessageQueues;

/// The designated XCM channel to `peer`: `domain = 0`, `num = 0`.
pub fn xcm_channel(peer: ParaId) -> ChannelId {
	ChannelId { peer, domain: 0, num: 0 }
}

/// XCM sender routing to a sibling parachain over Speculative Messaging
/// when no HRMP channel is open.
///
/// - `T`: the [`Config`] of the runtime's spec-messaging pallet.
/// - `ChannelInfo`: HRMP channel state, supplied by `ParachainSystem` — relay state at the current
///   relay parent, so a relay-side closure is observed with at most one parachain block of lag,
///   consistently for all sends in a block.
/// - `VersionWrapper`: XCM version negotiation, usually `PolkadotXcm`. The `SupportedVersion` it
///   reads is keyed by the identical destination `Location`, so versions negotiated over HRMP carry
///   over for free.
/// - `Price`: the delivery fee, mirroring `PriceForSiblingDelivery`.
pub struct SpecMsgRouter<T, ChannelInfo, VersionWrapper, Price>(
	PhantomData<(T, ChannelInfo, VersionWrapper, Price)>,
);

impl<T, ChannelInfo, VersionWrapper, Price> SendXcm
	for SpecMsgRouter<T, ChannelInfo, VersionWrapper, Price>
where
	T: Config,
	ChannelInfo: GetChannelInfo,
	VersionWrapper: WrapVersion,
	Price: PriceForMessageDelivery<Id = ParaId>,
{
	type Ticket = (ChannelId, Vec<u8>);

	fn validate(
		dest: &mut Option<Location>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let d = dest.take().ok_or(SendError::MissingArgument)?;

		// Only sibling parachains are handled here.
		let id = match d.unpack() {
			(1, [Parachain(id)]) => ParaId::from(*id),
			_ => {
				*dest = Some(d);
				return Err(SendError::NotApplicable);
			},
		};

		// HRMP wins while a channel exists: `Full` counts as open — it is
		// backpressure, not absence — so it too falls through to
		// `XcmpQueue`, which buffers against it. Only `Closed` continues —
		// and a set `HrmpClosing(id)` cutover flag counts as `Closed`:
		// during a drain-before-close the still-open channel keeps draining
		// its queued messages via `XcmpQueue` while every NEW message
		// diverts here, ahead of the relay-side closure being observable.
		if !HrmpClosing::<T>::contains_key(id) {
			match ChannelInfo::get_channel_status(id) {
				ChannelStatus::Ready(..) | ChannelStatus::Full => {
					*dest = Some(d);
					return Err(SendError::NotApplicable);
				},
				ChannelStatus::Closed => {},
			}
		}

		// ... and spec-msg carries the XCM only over an open channel.
		let channel = xcm_channel(id);
		if !Pallet::<T>::is_outbound_channel_open(&channel) {
			*dest = Some(d);
			return Err(SendError::NotApplicable);
		}

		let xcm = msg.take().ok_or(SendError::MissingArgument)?;
		let price = Price::price_for_delivery(id, &xcm);
		let versioned = VersionWrapper::wrap_version(&d, xcm)
			.map_err(|()| SendError::DestinationUnsupported)?;
		versioned.check_is_decodable().map_err(|()| SendError::ExceedsMaxMessageSize)?;
		let encoded = versioned.encode();

		// Capacity MUST fail the send, never fall through: the HRMP channel
		// is `Closed` here, so `XcmpQueue` would accept the message and
		// `take_outbound_messages` would silently swallow it later —
		// falling back would turn backpressure into silent loss.
		Pallet::<T>::can_send(&channel, encoded.len())
			.map_err(|_| SendError::Transport("Spec-msg channel at capacity"))?;

		Ok(((channel, encoded), price))
	}

	fn deliver((channel, encoded_xcm): Self::Ticket) -> Result<XcmHash, SendError> {
		let hash = sp_io::hashing::blake2_256(&encoded_xcm);
		Pallet::<T>::send(channel, encoded_xcm)
			.map_err(|_| SendError::Transport("Spec-msg send failed"))?;
		Ok(hash)
	}
}

/// The dry-run APIs' view of this block's not-yet-committed sends, mirroring
/// `XcmpQueue`: channel data streams decoded back to the `VersionedXcm`s the
/// router delivered, keyed by the destination sibling. Payloads that are not
/// the router's `Data`-wrapped XCM envelope (in MVP none exist) are skipped.
impl<T, ChannelInfo, VersionWrapper, Price> InspectMessageQueues
	for SpecMsgRouter<T, ChannelInfo, VersionWrapper, Price>
where
	T: Config,
{
	fn clear_messages() {
		// Best effort — `OutboundMessages` only ever holds THIS block's
		// sends, so clearing isolates exactly what a dry run produces.
		let _ = OutboundMessages::<T>::clear(u32::MAX, None);
	}

	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		OutboundMessages::<T>::iter()
			.filter_map(|(stream, leaves)| {
				let StreamId::Channel { recipient, .. } = stream else { return None };
				let messages: Vec<_> = leaves
					.iter()
					.filter_map(|leaf| match SpecMsgKind::decode_all(&mut &leaf[..]).ok()? {
						SpecMsgKind::Data(data) => VersionedXcm::<()>::decode_all_with_depth_limit(
							MAX_XCM_DECODE_DEPTH,
							&mut &data[..],
						)
						.ok(),
						SpecMsgKind::Signal(_) => None,
					})
					.collect();
				(!messages.is_empty()).then(|| {
					(
						VersionedLocation::from(Location::new(1, Parachain(recipient.into()))),
						messages,
					)
				})
			})
			.collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, OutChannels, OutboundMessages};
	use cumulus_primitives_spec_messaging::{
		MessagePosition, OutChannelState, Register, SpecMsgKind, StreamId, WindowGrant,
	};
	use frame_support::assert_ok;
	use xcm::VersionedXcm;

	/// Sibling with the spec-msg channel armed and no HRMP channel.
	const SPEC_SIBLING: u32 = 2001;
	/// Sibling with neither transport set up.
	const UNKNOWN_SIBLING: u32 = 3333;

	fn sibling(id: u32) -> Location {
		Location::new(1, [Parachain(id)])
	}

	fn test_xcm() -> Xcm<()> {
		Xcm(alloc::vec![ClearOrigin])
	}

	/// Puts the designated XCM channel to `id` into phase `Open` under the
	/// given grant, as a completed open/accept/register round-trip would.
	fn open_spec_msg_channel_with_grant(id: u32, grant: WindowGrant) {
		OutChannels::<Test>::insert(
			xcm_channel(id.into()),
			OutChannelState {
				closed_by_us: false,
				announced_version: 0,
				register: Some(Register {
					version: 0,
					up_to: MessagePosition(0),
					grant,
					closed: false,
				}),
			},
		);
	}

	fn open_spec_msg_channel(id: u32) {
		open_spec_msg_channel_with_grant(id, TestGrant::get());
	}

	fn stream(id: u32) -> StreamId {
		StreamId::Channel { recipient: id.into(), domain: 0, num: 0 }
	}

	/// Runs `Router::validate` on fresh `Option`s, returning them alongside
	/// the result so fall-through tests can assert both were restored.
	fn validate(
		dest: Location,
		xcm: Xcm<()>,
	) -> (Option<Location>, Option<Xcm<()>>, SendResult<(ChannelId, Vec<u8>)>) {
		let mut dest = Some(dest);
		let mut msg = Some(xcm);
		let result = Router::validate(&mut dest, &mut msg);
		(dest, msg, result)
	}

	#[test]
	fn open_or_full_hrmp_channel_falls_through() {
		new_test_ext().execute_with(|| {
			// Even with the spec-msg channel armed, HRMP wins while its
			// channel exists — `Full` is backpressure, not absence.
			open_spec_msg_channel(SPEC_SIBLING);
			for set in [
				(|id| HrmpReady::set(alloc::vec![id])) as fn(u32),
				(|id| HrmpFull::set(alloc::vec![id])) as fn(u32),
			] {
				HrmpReady::set(Vec::new());
				HrmpFull::set(Vec::new());
				set(SPEC_SIBLING);

				let (dest, msg, result) = validate(sibling(SPEC_SIBLING), test_xcm());
				assert_eq!(result.unwrap_err(), SendError::NotApplicable);
				// Destination and message are intact for the next router
				// in the tuple (`XcmpQueue`).
				assert_eq!(dest, Some(sibling(SPEC_SIBLING)));
				assert_eq!(msg, Some(test_xcm()));
			}
		});
	}

	#[test]
	fn hrmp_closing_flag_counts_as_closed() {
		new_test_ext().execute_with(|| {
			// Mid-cutover: the pair's HRMP channel still reports `Ready`,
			// but the `HrmpClosing` flag alone diverts every new send onto
			// the spec-msg stream while the queued HRMP messages drain.
			open_spec_msg_channel(SPEC_SIBLING);
			HrmpReady::set(alloc::vec![SPEC_SIBLING]);
			assert_ok!(SpecMessaging::set_hrmp_closing(RuntimeOrigin::root(), SPEC_SIBLING.into()));

			assert!(matches!(
				MockChannelInfo::get_channel_status(SPEC_SIBLING.into()),
				ChannelStatus::Ready(..)
			));
			assert_ok!(send_xcm::<Router>(sibling(SPEC_SIBLING), test_xcm()));
			assert_eq!(OutboundMessages::<Test>::get(stream(SPEC_SIBLING)).len(), 1);

			// `Full` diverts just the same — the flag overrides both open
			// states, not only `Ready`.
			HrmpReady::set(Vec::new());
			HrmpFull::set(alloc::vec![SPEC_SIBLING]);
			assert_ok!(send_xcm::<Router>(sibling(SPEC_SIBLING), test_xcm()));
			assert_eq!(OutboundMessages::<Test>::get(stream(SPEC_SIBLING)).len(), 2);

			// The flag means "treat as `Closed`", nothing more: without the
			// open spec-msg channel the router falls through exactly as for
			// a genuinely closed pair.
			OutChannels::<Test>::remove(xcm_channel(SPEC_SIBLING.into()));
			let (dest, msg, result) = validate(sibling(SPEC_SIBLING), test_xcm());
			assert_eq!(result.unwrap_err(), SendError::NotApplicable);
			assert_eq!(dest, Some(sibling(SPEC_SIBLING)));
			assert_eq!(msg, Some(test_xcm()));

			// Rollback: clearing the flag restores the HRMP-wins rule.
			open_spec_msg_channel(SPEC_SIBLING);
			HrmpReady::set(alloc::vec![SPEC_SIBLING]);
			HrmpFull::set(Vec::new());
			assert_ok!(SpecMessaging::clear_hrmp_closing(
				RuntimeOrigin::root(),
				SPEC_SIBLING.into()
			));
			let (dest, msg, result) = validate(sibling(SPEC_SIBLING), test_xcm());
			assert_eq!(result.unwrap_err(), SendError::NotApplicable);
			assert_eq!(dest, Some(sibling(SPEC_SIBLING)));
			assert_eq!(msg, Some(test_xcm()));
		});
	}

	#[test]
	fn non_sibling_destination_is_not_applicable() {
		new_test_ext().execute_with(|| {
			for dest in [
				Location::parent(),
				Location::new(1, [Parachain(SPEC_SIBLING), PalletInstance(3)]),
				Location::new(0, [Parachain(SPEC_SIBLING)]),
			] {
				let (restored, msg, result) = validate(dest.clone(), test_xcm());
				assert_eq!(result.unwrap_err(), SendError::NotApplicable);
				assert_eq!(restored, Some(dest));
				assert_eq!(msg, Some(test_xcm()));
			}
		});
	}

	#[test]
	fn closed_hrmp_and_open_channel_delivers_to_stream() {
		new_test_ext().execute_with(|| {
			open_spec_msg_channel(SPEC_SIBLING);

			let versioned = VersionedXcm::from(test_xcm());
			for expected_position in 0..2usize {
				let (hash, price) = send_xcm::<Router>(sibling(SPEC_SIBLING), test_xcm()).unwrap();
				// Hash is the blake2 of the encoded versioned XCM; the mock
				// price is free.
				assert_eq!(hash, versioned.using_encoded(sp_io::hashing::blake2_256));
				assert_eq!(price, Assets::new());

				// The payload sits on the designated channel stream at the
				// derived position, as a `Data` leaf holding exactly the
				// SCALE-encoded `VersionedXcm` — no extra framing.
				let messages = OutboundMessages::<Test>::get(stream(SPEC_SIBLING));
				assert_eq!(messages.len(), expected_position + 1);
				assert_eq!(
					messages[expected_position].to_vec(),
					SpecMsgKind::Data(versioned.encode()).encode(),
				);
			}
		});
	}

	#[test]
	fn closed_hrmp_without_open_channel_is_not_applicable() {
		new_test_ext().execute_with(|| {
			let (dest, msg, result) = validate(sibling(UNKNOWN_SIBLING), test_xcm());
			assert_eq!(result.unwrap_err(), SendError::NotApplicable);
			assert_eq!(dest, Some(sibling(UNKNOWN_SIBLING)));
			assert_eq!(msg, Some(test_xcm()));
			assert!(OutboundMessages::<Test>::get(stream(UNKNOWN_SIBLING)).is_empty());
		});
	}

	#[test]
	fn per_block_cap_is_a_transport_error() {
		new_test_ext().execute_with(|| {
			open_spec_msg_channel(SPEC_SIBLING);

			// Fill this block's vec for the stream to `MaxMessagesPerBlock`.
			for _ in 0..8 {
				assert_ok!(send_xcm::<Router>(sibling(SPEC_SIBLING), test_xcm()));
			}

			// Exhausted capacity MUST abort the send, never fall through:
			// with HRMP `Closed`, `XcmpQueue` would accept the message and
			// silently swallow it later.
			let (_, _, result) = validate(sibling(SPEC_SIBLING), test_xcm());
			assert_eq!(result.unwrap_err(), SendError::Transport("Spec-msg channel at capacity"));
		});
	}

	#[test]
	fn oversized_message_is_a_transport_error() {
		new_test_ext().execute_with(|| {
			open_spec_msg_channel(SPEC_SIBLING);

			// Encoded `VersionedXcm` past `MaxMsgLen` (64 in the mock) once
			// the `Data` envelope is added.
			let oversized = Xcm(alloc::vec![ClearOrigin; 100]);
			let (_, _, result) = validate(sibling(SPEC_SIBLING), oversized);
			assert_eq!(result.unwrap_err(), SendError::Transport("Spec-msg channel at capacity"));

			// A small message still goes through.
			assert_ok!(send_xcm::<Router>(sibling(SPEC_SIBLING), test_xcm()));
		});
	}

	#[test]
	fn exhausted_credit_window_is_a_transport_error() {
		new_test_ext().execute_with(|| {
			// Two messages of credit; nothing confirmed yet.
			let grant = WindowGrant { max_messages: 2, max_bytes: 4096, max_message_size: 64 };
			open_spec_msg_channel_with_grant(SPEC_SIBLING, grant);

			for _ in 0..2 {
				assert_ok!(send_xcm::<Router>(sibling(SPEC_SIBLING), test_xcm()));
			}

			// The grant-exceeding send MUST abort, never fall through —
			// like the per-block cap, but this is the peer's advisory
			// window enforced by the own STF (backpressure surfacing).
			let (_, _, result) = validate(sibling(SPEC_SIBLING), test_xcm());
			assert_eq!(result.unwrap_err(), SendError::Transport("Spec-msg channel at capacity"));

			// A register read advancing the watermark restores capacity.
			crate::OutChannelsMeta::<Test>::mutate(xcm_channel(SPEC_SIBLING.into()), |meta| {
				meta.confirm(MessagePosition(1))
			});
			assert_ok!(send_xcm::<Router>(sibling(SPEC_SIBLING), test_xcm()));
		});
	}

	#[test]
	fn dry_run_inspection_sees_and_clears_this_blocks_sends() {
		new_test_ext().execute_with(|| {
			open_spec_msg_channel(SPEC_SIBLING);
			assert_ok!(send_xcm::<Router>(sibling(SPEC_SIBLING), test_xcm()));

			// `get_messages` decodes the pending sends back to exactly what
			// was routed, keyed by the destination sibling.
			assert_eq!(
				Router::get_messages(),
				alloc::vec![(
					VersionedLocation::from(sibling(SPEC_SIBLING)),
					alloc::vec![VersionedXcm::from(test_xcm())],
				)],
			);

			// `clear_messages` drops them — the dry-run APIs' isolation.
			Router::clear_messages();
			assert!(Router::get_messages().is_empty());
			assert!(OutboundMessages::<Test>::get(stream(SPEC_SIBLING)).is_empty());
		});
	}

	/// `(1, Here)` stand-in for `ParentAsUmp`, delivering `[1; 32]`.
	pub struct MockParentAsUmp;
	impl SendXcm for MockParentAsUmp {
		type Ticket = ();

		fn validate(dest: &mut Option<Location>, msg: &mut Option<Xcm<()>>) -> SendResult<()> {
			let d = dest.take().ok_or(SendError::MissingArgument)?;
			match d.unpack() {
				(1, []) => {
					msg.take().ok_or(SendError::MissingArgument)?;
					Ok(((), Assets::new()))
				},
				_ => {
					*dest = Some(d);
					Err(SendError::NotApplicable)
				},
			}
		}

		fn deliver(_: ()) -> Result<XcmHash, SendError> {
			Ok([1; 32])
		}
	}

	/// Sibling stand-in for `XcmpQueue`, delivering `[2; 32]`. It diverges
	/// from the real router in ONE deliberate way: it only accepts siblings
	/// whose HRMP channel exists, where the real `XcmpQueue::validate`
	/// accepts ANY sibling and `take_outbound_messages` silently drops the
	/// queued messages of closed channels later — exactly the footgun the
	/// spec-msg router's `Transport`-on-capacity rule exists to avoid. The
	/// divergence makes the tuple's selection observable per destination
	/// (and `Unroutable` reachable) without standing up the full pallet.
	pub struct MockXcmpQueue;
	impl SendXcm for MockXcmpQueue {
		type Ticket = ();

		fn validate(dest: &mut Option<Location>, msg: &mut Option<Xcm<()>>) -> SendResult<()> {
			let d = dest.take().ok_or(SendError::MissingArgument)?;
			match d.unpack() {
				(1, [Parachain(id)])
					if !matches!(
						MockChannelInfo::get_channel_status((*id).into()),
						ChannelStatus::Closed
					) =>
				{
					msg.take().ok_or(SendError::MissingArgument)?;
					Ok(((), Assets::new()))
				},
				_ => {
					*dest = Some(d);
					Err(SendError::NotApplicable)
				},
			}
		}

		fn deliver(_: ()) -> Result<XcmHash, SendError> {
			Ok([2; 32])
		}
	}

	#[test]
	fn router_tuple_selects_the_expected_member() {
		// The wiring every runtime uses: spec-msg BEFORE `XcmpQueue`, since
		// both match the same sibling pattern.
		type Routers = (MockParentAsUmp, Router, MockXcmpQueue);

		new_test_ext().execute_with(|| {
			const HRMP_SIBLING: u32 = 2002;
			HrmpReady::set(alloc::vec![HRMP_SIBLING]);
			open_spec_msg_channel(HRMP_SIBLING);
			open_spec_msg_channel(SPEC_SIBLING);

			// Parent → `ParentAsUmp`.
			assert_eq!(send_xcm::<Routers>(Location::parent(), test_xcm()).unwrap().0, [1; 32]);

			// Open-HRMP sibling → `XcmpQueue`, even though its spec-msg
			// channel is armed too: HRMP wins while it exists.
			assert_eq!(send_xcm::<Routers>(sibling(HRMP_SIBLING), test_xcm()).unwrap().0, [2; 32]);
			assert!(OutboundMessages::<Test>::get(stream(HRMP_SIBLING)).is_empty());

			// HRMP-closed sibling with the spec-msg channel open →
			// `SpecMsgRouter`.
			let hash = send_xcm::<Routers>(sibling(SPEC_SIBLING), test_xcm()).unwrap().0;
			let versioned = VersionedXcm::from(test_xcm());
			assert_eq!(hash, versioned.using_encoded(sp_io::hashing::blake2_256));
			assert_eq!(OutboundMessages::<Test>::get(stream(SPEC_SIBLING)).len(), 1);

			// Unknown sibling: every member falls through, so the tuple
			// yields `NotApplicable` — which `pallet-xcm` surfaces to the
			// caller as `Unroutable` (`From<SendError> for xcm::Error`).
			assert_eq!(
				send_xcm::<Routers>(sibling(UNKNOWN_SIBLING), test_xcm()).unwrap_err(),
				SendError::NotApplicable
			);
		});
	}
}
