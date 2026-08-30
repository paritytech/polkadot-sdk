// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! # HRMP control-plane pallet (relay-chain side)
//!
//! The relay chain's half of HRMP channel management. It takes requests from a trusted system
//! parachain, applies them to the relay chain's own HRMP registry through
//! [`hrmp_primitives::HrmpRegistry`], and reports what happened.
//!
//! ## No deposits here
//!
//! Every request is applied deposit-free. The parachain holds the money now, so reserving here
//! would charge a para twice — and would try to draw on a sovereign account the migration has
//! emptied. The relay chain's protection is no longer economic: it is that only a trusted origin
//! can drive this pallet.
//!
//! ## Refusals are reported, not returned
//!
//! A request this pallet will not act on is *not* an extrinsic failure. Failing would roll the
//! rejection report back along with everything else, and the parachain would sit on a held deposit
//! waiting for news that never comes. So a rejection is applied, reported, and returns `Ok`.
//!
//! The one exception is [`Pallet::establish_system_channel`], which holds no deposit anywhere and
//! so reports its outcome as a local event rather than spending a round trip on it.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::traits::EnsureOrigin;
use hrmp_primitives::{
	ChannelId, FailureReason, HrmpRegistry, MessageToPara, MessageToParaV1, MessageToRelay,
	MessageToRelayV1, Outcome,
};

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Used to send an XCM `Transact` back to the HRMP pallet on the parachain.
pub trait SendToPara {
	/// Send `message` to the parachain.
	///
	/// `Err(())` means the message could not be handed to the transport. Callers here do *not*
	/// fail: this chain's own state is already correct, and unwinding it because a report could
	/// not be sent would be strictly worse than the two chains being out of step.
	#[allow(clippy::result_unit_err)]
	fn send(message: MessageToPara) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl SendToPara for () {
	fn send(_message: MessageToPara) -> Result<(), ()> {
		Ok(())
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// A trusted parachain authorized to drive HRMP channel management.
		type ParaOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Sends reports to the parachain.
		type SendToPara: SendToPara;

		/// The relay chain's HRMP channel registry.
		type Registry: HrmpRegistry;

		/// Weight information for the extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An open-channel request was recorded.
		OpenRequested { channel: ChannelId, message_id: u64 },
		/// An open-channel request was refused.
		OpenRejected { channel: ChannelId, message_id: u64, reason: FailureReason },
		/// An open-channel request was confirmed by its recipient.
		OpenAccepted { channel: ChannelId, message_id: u64 },
		/// An acceptance was refused.
		AcceptRejected { channel: ChannelId, message_id: u64, reason: FailureReason },
		/// A channel was closed.
		Closed { channel: ChannelId, message_id: u64 },
		/// A close was refused.
		CloseRejected { channel: ChannelId, message_id: u64, reason: FailureReason },
		/// An unconfirmed request was dropped.
		Cancelled { channel: ChannelId, message_id: u64 },
		/// A cancellation was refused.
		CancelRejected { channel: ChannelId, message_id: u64, reason: FailureReason },
		/// A deposit-free channel was opened in both directions.
		SystemChannelOpened { channel: ChannelId, message_id: u64 },
		/// A deposit-free channel could not be opened.
		///
		/// Reported here rather than sent back: nothing is staked on the parachain for a system
		/// channel, so a round trip would be paying for news nobody is waiting on.
		SystemChannelRejected { channel: ChannelId, message_id: u64, reason: FailureReason },
		/// A report could not be sent back to the parachain.
		///
		/// This chain's own state is already correct; the parachain is now out of step and will
		/// need governance to reconcile it.
		ReportFailed { channel: ChannelId, message_id: u64 },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Serve one request from the parachain that owns the HRMP user surface.
		///
		/// Only callable by a trusted XCM origin (e.g. the Coretime chain), never by users. One
		/// entry point for every message variant — the wire enum is the protocol, so the call
		/// surface should not re-split what the type already unifies; a new message costs a new
		/// variant and a match arm here, not a new extrinsic. Mirrors the para side's `receive`.
		#[pallet::call_index(0)]
		#[pallet::weight(match message {
			MessageToRelay::V1(MessageToRelayV1::InitOpenChannel { .. }) =>
				T::WeightInfo::init_open_channel(),
			MessageToRelay::V1(MessageToRelayV1::AcceptOpenChannel { .. }) =>
				T::WeightInfo::accept_open_channel(),
			MessageToRelay::V1(MessageToRelayV1::CloseChannel { .. }) =>
				T::WeightInfo::close_channel(),
			MessageToRelay::V1(MessageToRelayV1::CancelOpenRequest { .. }) =>
				T::WeightInfo::cancel_open_request(),
			MessageToRelay::V1(MessageToRelayV1::EstablishSystemChannel { .. }) =>
				T::WeightInfo::establish_system_channel(),
		})]
		pub fn receive(origin: OriginFor<T>, message: MessageToRelay) -> DispatchResult {
			T::ParaOrigin::ensure_origin_or_root(origin)?;

			let MessageToRelay::V1(message) = message;
			match message {
				MessageToRelayV1::InitOpenChannel {
					channel,
					message_id,
					max_capacity,
					max_message_size,
				} => {
					let outcome = Self::guarded(|| {
						T::Registry::init_open_channel(channel, max_capacity, max_message_size)
					});
					Self::settle(
						channel,
						message_id,
						outcome,
						|c, m, o| MessageToParaV1::OpenResponse {
							channel: c,
							message_id: m,
							outcome: o,
						},
						|c, m| Event::OpenRequested { channel: c, message_id: m },
						|c, m, r| Event::OpenRejected { channel: c, message_id: m, reason: r },
					);
				},
				MessageToRelayV1::AcceptOpenChannel { channel, message_id } => {
					// The channel itself comes into existence at this chain's next session
					// boundary; the report goes out now, because what the parachain is waiting
					// on is whether its deposit is owed, not whether the channel has finished
					// opening.
					let outcome = Self::guarded(|| T::Registry::accept_open_channel(channel));
					Self::settle(
						channel,
						message_id,
						outcome,
						|c, m, o| MessageToParaV1::AcceptResponse {
							channel: c,
							message_id: m,
							outcome: o,
						},
						|c, m| Event::OpenAccepted { channel: c, message_id: m },
						|c, m, r| Event::AcceptRejected { channel: c, message_id: m, reason: r },
					);
				},
				MessageToRelayV1::CloseChannel { channel, message_id, initiator } => {
					let outcome = Self::guarded(|| T::Registry::close_channel(channel, initiator));
					Self::settle(
						channel,
						message_id,
						outcome,
						|c, m, o| MessageToParaV1::CloseResponse {
							channel: c,
							message_id: m,
							outcome: o,
						},
						|c, m| Event::Closed { channel: c, message_id: m },
						|c, m, r| Event::CloseRejected { channel: c, message_id: m, reason: r },
					);
				},
				MessageToRelayV1::CancelOpenRequest { channel, message_id } => {
					let outcome = Self::guarded(|| T::Registry::cancel_open_request(channel));
					Self::settle(
						channel,
						message_id,
						outcome,
						|c, m, o| MessageToParaV1::CancelResponse {
							channel: c,
							message_id: m,
							outcome: o,
						},
						|c, m| Event::Cancelled { channel: c, message_id: m },
						|c, m, r| Event::CancelRejected { channel: c, message_id: m, reason: r },
					);
				},
				// Unanswered: the outcome is an event on this chain, because no deposit anywhere
				// depends on it.
				MessageToRelayV1::EstablishSystemChannel { channel, message_id } => {
					match Self::guarded(|| T::Registry::establish_system_channel(channel)) {
						Ok(()) => Self::deposit_event(Event::SystemChannelOpened {
							channel,
							message_id,
						}),
						Err(reason) => Self::deposit_event(Event::SystemChannelRejected {
							channel,
							message_id,
							reason,
						}),
					}
				},
			}
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Run a registry call inside its own storage layer.
		///
		/// The registry may write before it fails, and a partial write must not survive a refusal
		/// that this pallet then reports to the parachain as "nothing happened".
		fn guarded(
			f: impl FnOnce() -> Result<(), FailureReason>,
		) -> Result<(), FailureReason> {
			frame_support::storage::with_storage_layer::<(), FailureReason, _>(f)
		}

		/// Report an outcome to the parachain and raise the matching local event.
		///
		/// Every request in this protocol ends the same way, so the shape is written once: emit
		/// the report first, then the event, and never fail either way.
		fn settle(
			channel: ChannelId,
			message_id: u64,
			outcome: Result<(), FailureReason>,
			report: impl FnOnce(ChannelId, u64, Outcome) -> MessageToParaV1,
			on_ok: impl FnOnce(ChannelId, u64) -> Event<T>,
			on_err: impl FnOnce(ChannelId, u64, FailureReason) -> Event<T>,
		) {
			Self::report(channel, message_id, report(channel, message_id, outcome.clone()));

			match outcome {
				Ok(()) => Self::deposit_event(on_ok(channel, message_id)),
				Err(reason) => Self::deposit_event(on_err(channel, message_id, reason)),
			}
		}

		/// Hand a report to the transport, swallowing a send failure.
		///
		/// See [`SendToPara::send`]: this chain's state is already correct, so unwinding it
		/// because the report could not go out would be strictly worse.
		fn report(channel: ChannelId, message_id: u64, message: MessageToParaV1) {
			if T::SendToPara::send(MessageToPara::V1(message)).is_err() {
				log::error!(
					target: "runtime::hrmp-relay",
					"failed to report the outcome for channel {:?}->{:?} back to the parachain",
					channel.sender,
					channel.recipient,
				);
				Self::deposit_event(Event::ReportFailed { channel, message_id });
			}
		}
	}
}
