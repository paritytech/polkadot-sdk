// This file is part of Cumulus.
// SPDX-License-Identifier: Unlicense

// This is free and unencumbered software released into the public domain.

// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.

// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.

// For more information, please refer to <http://unlicense.org/>

//! Tests of the Speculative Messaging runtime wiring: router placement (HRMP
//! wins while a channel exists; spec-msg diverts only over an open channel),
//! `Provides` emission through the `UmpSignalSource` hook, and the messaging
//! inherent's PoV `proof_size` reservation against the block budget.

use crate::{
	xcm_config::XcmRouter, PolkadotXcm, Runtime, RuntimeBlockWeights, RuntimeCall, RuntimeOrigin,
	SpecMessaging, SpecMsgMaxMsgLen,
};
use codec::DecodeAll;
use cumulus_pallet_parachain_system::{
	relay_state_snapshot::{MessagingStateSnapshot, RelayDispatchQueueRemainingCapacity},
	RelevantMessagingState,
};
use cumulus_pallet_spec_messaging::{
	xcm_router::xcm_channel, OutChannels, OutboundMessages, ADVANCE_PROOF_RESERVATION_BYTES,
	LIFT_RESERVATION_BYTES, PROTOCOL_VERSION,
};
use cumulus_primitives_core::{relay_chain::UMPSignal, AbridgedHrmpChannel, ParaId};
use cumulus_primitives_spec_messaging::{
	MessagePosition, MmrInclusionProof, OutChannelState, ProvideUmpSignals, Register,
	SpecMsgInherentData, SpecMsgKind, StreamId, SPMS_ENGINE_ID,
};
use frame_support::{
	assert_ok,
	dispatch::{DispatchClass, GetDispatchInfo},
	traits::Get,
};
use sp_runtime::{generic::DigestItem, BuildStorage};
use xcm::{latest::prelude::*, VersionedXcm};

/// The sibling parachain the tests route to.
const SIBLING: u32 = 2001;

fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities = frame_system::GenesisConfig::<Runtime>::default()
		.build_storage()
		.expect("frame system genesis builds; qed")
		.into();
	ext.execute_with(|| {
		// Both transports `wrap_version` against the destination: set the
		// `SafeXcmVersion` fallback, exactly the operational requirement of
		// opening a spec-msg pair (normally genesis-set).
		PolkadotXcm::force_default_xcm_version(RuntimeOrigin::root(), Some(XCM_VERSION))
			.expect("root can set the default XCM version; qed");
	});
	ext
}

fn sibling() -> Location {
	Location::new(1, [Parachain(SIBLING)])
}

fn test_xcm() -> Xcm<()> {
	Xcm(vec![ClearOrigin])
}

/// The data stream the designated XCM channel to [`SIBLING`] appends to.
fn sibling_stream() -> StreamId {
	StreamId::Channel { recipient: SIBLING.into(), domain: 0, num: 0 }
}

/// Puts the outbound spec-msg channel to [`SIBLING`] into phase `Open`
/// under the runtime's configured grant — the state a completed
/// open/accept/register round-trip leaves behind.
fn open_spec_msg_channel() {
	OutChannels::<Runtime>::insert(
		xcm_channel(SIBLING.into()),
		OutChannelState {
			closed_by_us: false,
			announced_version: PROTOCOL_VERSION,
			register: Some(Register {
				version: PROTOCOL_VERSION,
				up_to: MessagePosition(0),
				grant: crate::SpecMsgWindowGrant::get(),
				closed: false,
			}),
		},
	);
}

/// Makes `ParachainSystem` report an HRMP egress channel to [`SIBLING`] as
/// `Ready`, as the validation-data inherent would from the relay state.
fn open_hrmp_channel() {
	RelevantMessagingState::<Runtime>::put(MessagingStateSnapshot {
		dmq_mqc_head: Default::default(),
		relay_dispatch_queue_remaining_capacity: RelayDispatchQueueRemainingCapacity {
			remaining_count: u32::MAX,
			remaining_size: u32::MAX,
		},
		ingress_channels: Vec::new(),
		egress_channels: vec![(
			SIBLING.into(),
			AbridgedHrmpChannel {
				max_capacity: 8,
				max_total_size: 102400,
				max_message_size: 102400,
				msg_count: 0,
				total_size: 0,
				mqc_head: None,
			},
		)],
	});
}

#[test]
fn hrmp_wins_while_a_channel_exists() {
	new_test_ext().execute_with(|| {
		// Even with the spec-msg channel armed: while the HRMP channel
		// exists, sibling traffic keeps flowing through `XcmpQueue`,
		// byte-identical to a runtime without the pallet.
		open_hrmp_channel();
		open_spec_msg_channel();

		assert_ok!(send_xcm::<XcmRouter>(sibling(), test_xcm()));
		assert!(OutboundMessages::<Runtime>::iter().next().is_none());
	});
}

#[test]
fn hrmp_closed_sibling_routes_via_spec_msg_only_over_an_open_channel() {
	new_test_ext().execute_with(|| {
		// No HRMP channel (fresh state: everything reports `Closed`) and no
		// spec-msg channel: the router falls through — nothing lands on a
		// stream and the send surfaces `XcmpQueue`'s delivery failure
		// instead of being silently diverted.
		assert!(send_xcm::<XcmRouter>(sibling(), test_xcm()).is_err());
		assert!(OutboundMessages::<Runtime>::iter().next().is_none());

		// Arming the spec-msg channel diverts the send onto the designated
		// channel stream, as a `Data` leaf holding exactly the SCALE-encoded
		// `VersionedXcm`.
		open_spec_msg_channel();
		assert_ok!(send_xcm::<XcmRouter>(sibling(), test_xcm()));

		let sent = OutboundMessages::<Runtime>::get(sibling_stream());
		assert_eq!(sent.len(), 1);
		let leaf = SpecMsgKind::decode_all(&mut &sent[0][..])
			.expect("the router only appends well-formed `SpecMsgKind` leaves; qed");
		let SpecMsgKind::Data(data) = leaf else { panic!("XCM travels as a `Data` leaf") };
		// The payload is the routed XCM (`WithUniqueTopic` appends a
		// `SetTopic`, so decode rather than compare bytes).
		VersionedXcm::<()>::decode_all(&mut &data[..])
			.expect("the `Data` payload is exactly the SCALE-encoded `VersionedXcm`; qed");

		// The end-of-block fold commits the touched stream and feeds the
		// `Provides` UMP signal (`parachain_system::Config::UmpSignalSource`)
		// and the header digest.
		let root =
			SpecMessaging::commit_streams_root().expect("a stream was touched this block; qed");
		assert_eq!(
			<SpecMessaging as ProvideUmpSignals>::provides_root(),
			Some(UMPSignal::Provides(root)),
		);
		assert!(frame_system::Pallet::<Runtime>::digest()
			.logs
			.iter()
			.any(|log| matches!(log, DigestItem::Consensus(id, _) if *id == SPMS_ENGINE_ID)));
	});
}

#[test]
fn hrmp_closing_flag_diverts_new_traffic_while_the_channel_drains() {
	new_test_ext().execute_with(|| {
		open_hrmp_channel();
		open_spec_msg_channel();

		// Mid-cutover: the flag is set (root = the channel-management
		// origin) in the same governance batch as the relay-side
		// `hrmp.close_channel` — the still-open HRMP channel keeps
		// draining its queued messages while every new send diverts to
		// spec-msg, before the closure is observable in the relay state.
		assert_ok!(SpecMessaging::set_hrmp_closing(RuntimeOrigin::root(), SIBLING.into()));
		assert_ok!(send_xcm::<XcmRouter>(sibling(), test_xcm()));
		assert_eq!(OutboundMessages::<Runtime>::get(sibling_stream()).len(), 1);

		// Rollback: clearing the flag restores HRMP-wins while the channel
		// exists — nothing new lands on the stream.
		assert_ok!(SpecMessaging::clear_hrmp_closing(RuntimeOrigin::root(), SIBLING.into()));
		assert_ok!(send_xcm::<XcmRouter>(sibling(), test_xcm()));
		assert_eq!(OutboundMessages::<Runtime>::get(sibling_stream()).len(), 1);
	});
}

#[test]
fn consumed_payloads_fit_the_message_queue() {
	// `EnqueueToXcmQueue` requires the queue's `MaxMessageLen` (derived from
	// its `HeapSize`) to cover everything the receiver part can consume.
	assert!(pallet_message_queue::MaxMessageLenOf::<Runtime>::get() >= SpecMsgMaxMsgLen::get());
}

#[test]
fn worst_case_messaging_inherent_fits_the_pov_budget() {
	let touched: u32 = <Runtime as cumulus_pallet_spec_messaging::Config>::MaxTouchedStreams::get();
	let gaps: u32 = <Runtime as cumulus_pallet_spec_messaging::Config>::MaxContextGaps::get();

	// A worst-case consumption block: the touched-stream cap exhausted, the
	// register-read slots all opening context gaps.
	let data = SpecMsgInherentData {
		messages: (0..touched - gaps)
			.map(|i| {
				(
					ParaId::from(2000 + i),
					StreamId::Channel { recipient: 2000.into(), domain: 0, num: 0 },
					vec![vec![0u8; 32]],
				)
			})
			.collect(),
		register_reads: (0..gaps)
			.map(|i| {
				(
					ParaId::from(3000 + i),
					StreamId::Ack { recipient: 2000.into(), domain: 0, num: 0 },
					Vec::new(),
					MmrInclusionProof { mmr_size: 1, items: Vec::new() },
				)
			})
			.collect(),
	};
	let call =
		RuntimeCall::SpecMessaging(cumulus_pallet_spec_messaging::Call::enact_messages { data });
	let info = call.get_dispatch_info();

	// The `proof_size` reservation charged up front for the POV lifts the
	// submitter attaches outside block execution...
	assert_eq!(info.class, DispatchClass::Mandatory);
	assert_eq!(
		info.call_weight.proof_size(),
		u64::from(touched) * LIFT_RESERVATION_BYTES +
			u64::from(gaps) * ADVANCE_PROOF_RESERVATION_BYTES,
	);
	// ...must leave the bulk of the block's POV budget to the consumed
	// payloads and everything else.
	assert!(info.call_weight.proof_size() <= RuntimeBlockWeights::get().max_block.proof_size() / 4);
}
