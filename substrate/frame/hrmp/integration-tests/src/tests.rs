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

//! End-to-end tests for HRMP channel management.
//!
//! One test per flow lands with the flow it covers. What is here is what can be asserted while
//! the pallets' bodies are `todo!()`.

use crate::{
	para, relay, senders, HrmpPara, MockNet, Relay, PARA_ID, RECIPIENT, SENDER, SYSTEM_PARA,
};
use frame_support::{
	assert_ok,
	traits::{fungible::InspectHold, EnsureOrigin},
};
use hrmp_primitives::{
	ChannelId, MessageToRelay, MessageToRelayV1, ParaId, ParaNotification, ParaRequest,
	ParaRequestV1,
};
use pallet_hrmp_para::RequestState;
use polkadot_runtime_parachains::{
	dmp as parachains_dmp, hrmp as parachains_hrmp, Origin as ParachainsOrigin,
};
use sp_runtime::traits::Convert;
use xcm_simulator::TestExt;
use codec::DecodeAll;

const CHANNEL: ChannelId = ChannelId { sender: SENDER, recipient: RECIPIENT };
const SYSTEM_CHANNEL: ChannelId = ChannelId { sender: SENDER, recipient: SYSTEM_PARA };
const CAPACITY: u32 = 4;
const MESSAGE_SIZE: u32 = 512;

/// What `para_id` has held on the channel-managing parachain under `reason`.
fn held(para_id: ParaId, reason: pallet_hrmp_para::HoldReason) -> para::Balance {
	para::Balances::balance_on_hold(
		&para::RuntimeHoldReason::Hrmp(reason),
		&para::SovereignAccountOf::convert(para_id),
	)
}

/// The origin `para_id` reaches the relay chain's HRMP pallet with.
fn para_origin(para_id: ParaId) -> relay::RuntimeOrigin {
	ParachainsOrigin::Parachain(para_id.into()).into()
}

/// A para asking the relay chain to forward `request` to the channel-managing parachain.
fn ask(para_id: ParaId, request: ParaRequestV1) {
	assert_ok!(relay::Hrmp::relay_request(para_origin(para_id), ParaRequest::V1(request)));
}

/// Everything the relay chain has queued for `para_id`, oldest first.
///
/// Nothing drains these queues in the harness, so they accumulate over a test.
fn downward_queue(para_id: ParaId) -> Vec<Vec<u8>> {
	parachains_dmp::Pallet::<relay::Runtime>::dmq_contents_do_not_call_in_consensus(para_id.into())
		.into_iter()
		.map(|message| message.msg)
		.collect()
}

/// The queued messages that are XCM, which is the three notifications with an instruction.
///
/// The rest go down as a bare [`ParaNotification`], so the two wire formats are told apart by
/// which one decodes; see [`crate::senders::RelayNotifyParachain`].
fn downward_messages(para_id: ParaId) -> Vec<xcm::opaque::VersionedXcm> {

	downward_queue(para_id)
		.into_iter()
		.filter_map(|message| xcm::opaque::VersionedXcm::decode_all(&mut &message[..]).ok())
		.collect()
}

/// The queued messages that conclude an open request, which have no XCM instruction.
fn downward_conclusions(para_id: ParaId) -> Vec<ParaNotification> {

	downward_queue(para_id)
		.into_iter()
		.filter_map(|message| ParaNotification::decode_all(&mut &message[..]).ok())
		.collect()
}

#[test]
fn only_the_channel_managing_parachain_may_drive_hrmp() {
	MockNet::reset();

	Relay::execute_with(|| {
		let message = MessageToRelay::V1(MessageToRelayV1::OpenChannel {
			channel: CHANNEL,
			message_id: 0,
			max_capacity: crate::MAX_CAPACITY,
			max_message_size: crate::MAX_MESSAGE_SIZE,
		});

		// A different parachain's origin is not accepted...
		let other_para: relay::RuntimeOrigin =
			ParachainsOrigin::Parachain((PARA_ID + 1).into()).into();
		assert!(senders::EnsureHrmpPara::try_origin(other_para.clone()).is_err());
		assert!(relay::Hrmp::receive(other_para, message.clone()).is_err());

		// ...nor is a plain signed account.
		assert!(
			senders::EnsureHrmpPara::try_origin(relay::RuntimeOrigin::signed(crate::BOB)).is_err()
		);
		assert!(relay::Hrmp::receive(relay::RuntimeOrigin::signed(crate::BOB), message).is_err());

		// The configured parachain is. Dispatching it is left to the flow tests, since the
		// handlers are still `todo!()`.
		let ours: relay::RuntimeOrigin = ParachainsOrigin::Parachain(PARA_ID.into()).into();
		assert!(senders::EnsureHrmpPara::try_origin(ours).is_ok());
	});
}

#[test]
fn a_para_opens_a_channel_through_the_channel_managing_parachain() {
	MockNet::reset();

	// The sender asks the relay chain, which forwards to the parachain that holds the deposits.
	Relay::execute_with(|| {
		ask(
			SENDER,
			ParaRequestV1::InitOpenChannel {
				recipient: RECIPIENT,
				proposed_max_capacity: CAPACITY,
				proposed_max_message_size: MESSAGE_SIZE,
			},
		);
	});

	// The recipient has no channel to the channel-managing parachain, so it hears about the
	// request from the relay chain.
	Relay::execute_with(|| {
		assert_eq!(
			downward_messages(RECIPIENT),
			vec![xcm::opaque::VersionedXcm::from(xcm::opaque::latest::Xcm(vec![
				xcm::opaque::latest::prelude::HrmpNewChannelOpenRequest {
					sender: SENDER,
					max_capacity: CAPACITY,
					max_message_size: MESSAGE_SIZE,
				}
			]))]
		);
	});

	HrmpPara::execute_with(|| {
		let request = pallet_hrmp_para::Requests::<para::Runtime>::get(CHANNEL)
			.expect("the forwarded request was recorded");
		assert!(matches!(request.state, RequestState::Requested { .. }));
		assert_eq!(request.max_capacity, CAPACITY);
		assert_eq!(request.max_message_size, MESSAGE_SIZE);
		// Nothing is open until the relay chain says so.
		assert!(pallet_hrmp_para::Channels::<para::Runtime>::get(CHANNEL).is_none());
	});

	// The recipient accepts, which is what sends the relay chain the open.
	Relay::execute_with(|| {
		ask(RECIPIENT, ParaRequestV1::AcceptOpenChannel { sender: SENDER });
	});

	Relay::execute_with(|| {
		assert_eq!(
			downward_messages(SENDER),
			vec![xcm::opaque::VersionedXcm::from(xcm::opaque::latest::Xcm(vec![
				xcm::opaque::latest::prelude::HrmpChannelAccepted { recipient: RECIPIENT }
			]))]
		);

		let channel = parachains_hrmp::HrmpChannels::<relay::Runtime>::get(
			&polkadot_primitives::HrmpChannelId {
				sender: CHANNEL.sender.into(),
				recipient: CHANNEL.recipient.into(),
			},
		)
		.expect("the channel is in the routing table");
		assert_eq!(channel.max_capacity, CAPACITY);
		assert_eq!(channel.max_message_size, MESSAGE_SIZE);
		// The parachain holds the money, so the relay chain took nothing.
		assert_eq!(channel.sender_deposit, 0);
		assert_eq!(channel.recipient_deposit, 0);
	});

	// Both ends learn the channel exists, which only the relay chain can tell them.
	Relay::execute_with(|| {
		let opened = ParaNotification::ChannelOpened { channel: CHANNEL };
		assert_eq!(downward_conclusions(SENDER), vec![opened.clone()]);
		assert_eq!(downward_conclusions(RECIPIENT), vec![opened]);
	});

	HrmpPara::execute_with(|| {
		assert!(pallet_hrmp_para::Requests::<para::Runtime>::get(CHANNEL).is_none());
		let channel = pallet_hrmp_para::Channels::<para::Runtime>::get(CHANNEL)
			.expect("the relay chain confirmed the channel");
		assert_eq!(channel.max_capacity, CAPACITY);
		assert_eq!(channel.max_message_size, MESSAGE_SIZE);
		assert_eq!(
			pallet_hrmp_para::EgressIndex::<para::Runtime>::get(CHANNEL.sender).to_vec(),
			vec![CHANNEL.recipient]
		);
		assert_eq!(
			pallet_hrmp_para::IngressIndex::<para::Runtime>::get(CHANNEL.recipient).to_vec(),
			vec![CHANNEL.sender]
		);
	});
}

#[test]
fn a_channel_with_a_system_chain_takes_no_deposit() {
	MockNet::reset();

	// The system chain has no sovereign account on the channel-managing parachain, and needs
	// none: neither end pays for a channel with the system.
	Relay::execute_with(|| {
		ask(
			SENDER,
			ParaRequestV1::InitOpenChannel {
				recipient: SYSTEM_PARA,
				proposed_max_capacity: CAPACITY,
				proposed_max_message_size: MESSAGE_SIZE,
			},
		);
		ask(SYSTEM_PARA, ParaRequestV1::AcceptOpenChannel { sender: SENDER });
	});

	Relay::execute_with(|| {
		let channel = parachains_hrmp::HrmpChannels::<relay::Runtime>::get(
			&polkadot_primitives::HrmpChannelId {
				sender: SYSTEM_CHANNEL.sender.into(),
				recipient: SYSTEM_CHANNEL.recipient.into(),
			},
		)
		.expect("the channel is in the routing table");
		assert_eq!(channel.max_capacity, CAPACITY);
	});

	HrmpPara::execute_with(|| {
		let channel = pallet_hrmp_para::Channels::<para::Runtime>::get(SYSTEM_CHANNEL)
			.expect("the relay chain confirmed the channel");
		assert!(channel.sender_deposit.is_none());
		assert!(channel.recipient_deposit.is_none());

		// The paying channel next to it still holds both deposits, so this is not a blanket free
		// pass.
		assert!(pallet_hrmp_para::Channels::<para::Runtime>::get(CHANNEL).is_none());
	});
}

#[test]
fn a_para_cancels_its_open_request_through_the_channel_managing_parachain() {
	MockNet::reset();

	Relay::execute_with(|| {
		ask(
			SENDER,
			ParaRequestV1::InitOpenChannel {
				recipient: RECIPIENT,
				proposed_max_capacity: CAPACITY,
				proposed_max_message_size: MESSAGE_SIZE,
			},
		);
	});

	HrmpPara::execute_with(|| {
		assert!(pallet_hrmp_para::Requests::<para::Runtime>::get(CHANNEL).is_some());
		assert!(held(SENDER, pallet_hrmp_para::HoldReason::SenderDeposit) > 0);
	});

	// The relay chain was never asked to open this, so the cancel stops at the parachain.
	Relay::execute_with(|| {
		ask(SENDER, ParaRequestV1::CancelOpenRequest { channel: CHANNEL, open_requests: 1 });
	});

	HrmpPara::execute_with(|| {
		assert!(pallet_hrmp_para::Requests::<para::Runtime>::get(CHANNEL).is_none());
		assert_eq!(pallet_hrmp_para::OpenRequestCount::<para::Runtime>::get(SENDER), 0);
		assert_eq!(held(SENDER, pallet_hrmp_para::HoldReason::SenderDeposit), 0);
	});

	Relay::execute_with(|| {
		// The routing table never heard of it, before or after.
		assert!(parachains_hrmp::HrmpChannels::<relay::Runtime>::get(
			&polkadot_primitives::HrmpChannelId {
				sender: CHANNEL.sender.into(),
				recipient: CHANNEL.recipient.into(),
			},
		)
		.is_none());

		// The recipient was told about the request, so it is told the request is gone.
		assert_eq!(
			downward_conclusions(RECIPIENT),
			vec![ParaNotification::OpenRequestCanceled { channel: CHANNEL, by_parachain: SENDER }]
		);
	});
}

#[test]
fn the_recipient_can_cancel_too_and_the_sender_is_told() {
	MockNet::reset();

	Relay::execute_with(|| {
		ask(
			SENDER,
			ParaRequestV1::InitOpenChannel {
				recipient: RECIPIENT,
				proposed_max_capacity: CAPACITY,
				proposed_max_message_size: MESSAGE_SIZE,
			},
		);
		ask(RECIPIENT, ParaRequestV1::CancelOpenRequest { channel: CHANNEL, open_requests: 1 });
	});

	HrmpPara::execute_with(|| {
		assert!(pallet_hrmp_para::Requests::<para::Runtime>::get(CHANNEL).is_none());
		// The released deposit is the sender's either way.
		assert_eq!(held(SENDER, pallet_hrmp_para::HoldReason::SenderDeposit), 0);
	});

	Relay::execute_with(|| {
		assert_eq!(
			downward_conclusions(SENDER),
			vec![ParaNotification::OpenRequestCanceled {
				channel: CHANNEL,
				by_parachain: RECIPIENT,
			}]
		);
	});
}
