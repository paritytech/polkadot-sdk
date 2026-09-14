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

//! Tests for `pallet-hrmp-para`.
//!
//! One test per flow lands with the extrinsic it covers.

use crate::{
	mock::*, AcceptedRequestCount, Channels, EgressIndex, Error, Event, HoldReason, IngressIndex,
	OpenKind, OpenRequestCount, RequestIndex, RequestState, Requests,
};
use frame_support::{assert_noop, assert_ok};
use hrmp_primitives::{
	ChannelId, FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1,
	ParaId, ParaNotification, ParaRequest, ParaRequestV1,
};
use sp_runtime::DispatchError;

const CHANNEL: ChannelId = ChannelId { sender: 2000, recipient: 2001 };
/// The same sender, but towards a system chain: ids at or below 1999 pay no deposit.
const SYSTEM_CHANNEL: ChannelId = ChannelId { sender: 2000, recipient: 1001 };
const CAPACITY: u32 = 4;
const MESSAGE_SIZE: u32 = 512;
/// `LinearStoragePrice` over `channel_footprint`, which is `(count 1, size capacity)`.
const DEPOSIT: Balance = PER_MESSAGE * CAPACITY as Balance;

/// The relay chain forwarding `request`, asked for by `para_id`.
fn forwarded(para_id: ParaId, request: ParaRequestV1) -> sp_runtime::DispatchResult {
	Hrmp::receive_request(RuntimeOrigin::root(), para_id, ParaRequest::V1(request))
}

fn init(channel: ChannelId, capacity: u32, message_size: u32) -> sp_runtime::DispatchResult {
	forwarded(
		channel.sender,
		ParaRequestV1::InitOpenChannel {
			recipient: channel.recipient,
			proposed_max_capacity: capacity,
			proposed_max_message_size: message_size,
		},
	)
}

fn accept(channel: ChannelId) -> sp_runtime::DispatchResult {
	forwarded(channel.recipient, ParaRequestV1::AcceptOpenChannel { sender: channel.sender })
}

/// Give both ends something to put up as a deposit.
fn fund(channel: ChannelId) {
	fund_para(channel.sender);
	fund_para(channel.recipient);
}

/// A request both ends have agreed on, waiting for the relay chain's answer.
fn agreed(channel: ChannelId) {
	fund(channel);
	assert_ok!(init(channel, CAPACITY, MESSAGE_SIZE));
	assert_ok!(accept(channel));
	let _ = take_sent();
	let _ = hrmp_events();
}

fn respond(
	channel: ChannelId,
	message_id: u64,
	outcome: Result<(u32, u32), FailureReason>,
) -> sp_runtime::DispatchResult {
	Hrmp::receive(
		RuntimeOrigin::root(),
		MessageToPara::V1(MessageToParaV1::OpenChannelResponse { channel, message_id, outcome }),
	)
}

/// A channel the relay chain has confirmed is open.
fn opened(channel: ChannelId) {
	agreed(channel);
	let message_id = Requests::<Test>::get(channel).unwrap().message_id;
	assert_ok!(respond(channel, message_id, Ok((CAPACITY, MESSAGE_SIZE))));
	let _ = hrmp_events();
}

fn force_clean(para: ParaId, num_inbound: u32, num_outbound: u32) -> sp_runtime::DispatchResult {
	Hrmp::force_clean_hrmp(RuntimeOrigin::root(), para, num_inbound, num_outbound)
}

#[test]
fn genesis_holds_no_channels_and_no_requests() {
	new_test_ext().execute_with(|| {
		assert_eq!(Requests::<Test>::iter().count(), 0);
		assert_eq!(Channels::<Test>::iter().count(), 0);
		assert!(take_sent().is_empty());
		assert!(hrmp_events().is_empty());
	});
}

#[test]
fn receive_is_only_for_the_relay_chain() {
	new_test_ext().execute_with(|| {
		let report = MessageToPara::V1(MessageToParaV1::OpenChannelResponse {
			channel: CHANNEL,
			message_id: 0,
			outcome: Ok((8, 1_024)),
		});

		assert_noop!(
			Hrmp::receive(RuntimeOrigin::signed(ALICE), report.clone()),
			DispatchError::BadOrigin
		);
		assert_noop!(Hrmp::receive(para_origin(CHANNEL.sender), report), DispatchError::BadOrigin);
	});
}

#[test]
fn receive_request_is_only_for_the_relay_chain() {
	new_test_ext().execute_with(|| {
		let request = ParaRequest::V1(ParaRequestV1::CloseChannel { channel: CHANNEL });

		assert_noop!(
			Hrmp::receive_request(RuntimeOrigin::signed(ALICE), CHANNEL.sender, request.clone()),
			DispatchError::BadOrigin
		);
		assert_noop!(
			Hrmp::receive_request(para_origin(CHANNEL.sender), CHANNEL.sender, request),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn init_open_channel_records_the_request_and_tells_the_recipient() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		assert_ok!(init(CHANNEL, CAPACITY, MESSAGE_SIZE));

		let request = Requests::<Test>::get(CHANNEL).unwrap();
		assert!(matches!(request.state, RequestState::Requested { .. }));
		assert_eq!(request.max_capacity, CAPACITY);
		assert_eq!(request.max_message_size, MESSAGE_SIZE);
		assert_eq!(OpenRequestCount::<Test>::get(CHANNEL.sender), 1);
		assert_eq!(AcceptedRequestCount::<Test>::get(CHANNEL.recipient), 0);

		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), DEPOSIT);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), 0);

		// The recipient has no channel here, so it is told through the relay chain.
		assert_eq!(
			take_sent(),
			vec![MessageToRelay::V1(MessageToRelayV1::NotifyPara {
				para_id: CHANNEL.recipient,
				notification: ParaNotification::NewChannelOpenRequest {
					sender: CHANNEL.sender,
					max_message_size: MESSAGE_SIZE,
					max_capacity: CAPACITY,
				},
			})]
		);
		assert_eq!(
			hrmp_events(),
			vec![Event::OpenChannelRequested {
				channel: CHANNEL,
				message_id: request.message_id,
				proposed_max_capacity: CAPACITY,
				proposed_max_message_size: MESSAGE_SIZE,
			}]
		);
	});
}

#[test]
fn init_open_channel_rejects_bad_parameters() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		let to_self = ChannelId { sender: CHANNEL.sender, recipient: CHANNEL.sender };
		assert_noop!(init(to_self, CAPACITY, MESSAGE_SIZE), Error::<Test>::OpenHrmpChannelToSelf);
		assert_noop!(init(CHANNEL, 0, MESSAGE_SIZE), Error::<Test>::OpenHrmpChannelZeroCapacity);
		assert_noop!(
			init(CHANNEL, MAX_CAPACITY + 1, MESSAGE_SIZE),
			Error::<Test>::OpenHrmpChannelCapacityExceedsLimit
		);
		assert_noop!(init(CHANNEL, CAPACITY, 0), Error::<Test>::OpenHrmpChannelZeroMessageSize);
		assert_noop!(
			init(CHANNEL, CAPACITY, MAX_MESSAGE_SIZE + 1),
			Error::<Test>::OpenHrmpChannelMessageSizeExceedsLimit
		);

		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), 0);
	});
}

#[test]
fn init_open_channel_rejects_a_second_request_or_an_open_channel() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		assert_ok!(init(CHANNEL, CAPACITY, MESSAGE_SIZE));
		assert_noop!(
			init(CHANNEL, CAPACITY, MESSAGE_SIZE),
			Error::<Test>::OpenHrmpChannelAlreadyRequested
		);

		assert_ok!(accept(CHANNEL));
		let message_id = Requests::<Test>::get(CHANNEL).unwrap().message_id;
		assert_ok!(respond(CHANNEL, message_id, Ok((CAPACITY, MESSAGE_SIZE))));

		assert_noop!(
			init(CHANNEL, CAPACITY, MESSAGE_SIZE),
			Error::<Test>::OpenHrmpChannelAlreadyExists
		);
	});
}

#[test]
fn init_open_channel_respects_the_outbound_limit() {
	new_test_ext().execute_with(|| {
		fund_para(CHANNEL.sender);
		for i in 0..MAX_OUTBOUND_CHANNELS {
			let channel = ChannelId { sender: CHANNEL.sender, recipient: 3_000 + i };
			assert_ok!(init(channel, CAPACITY, MESSAGE_SIZE));
		}

		let one_too_many = ChannelId { sender: CHANNEL.sender, recipient: 4_000 };
		assert_noop!(
			init(one_too_many, CAPACITY, MESSAGE_SIZE),
			Error::<Test>::OpenHrmpChannelLimitExceeded
		);
	});
}

#[test]
fn a_system_chain_on_either_end_pays_nothing() {
	new_test_ext().execute_with(|| {
		fund(SYSTEM_CHANNEL);

		assert_ok!(init(SYSTEM_CHANNEL, CAPACITY, MESSAGE_SIZE));
		assert_ok!(accept(SYSTEM_CHANNEL));

		assert_eq!(held(SYSTEM_CHANNEL.sender, HoldReason::SenderDeposit), 0);
		assert_eq!(held(SYSTEM_CHANNEL.recipient, HoldReason::RecipientDeposit), 0);
	});
}

#[test]
fn a_system_chain_needs_no_funded_sovereign_account() {
	new_test_ext().execute_with(|| {
		// Neither end is funded. A deposit-free channel holds nothing rather than holding zero,
		// so it must not need an account to hold it against.
		assert_ok!(init(SYSTEM_CHANNEL, CAPACITY, MESSAGE_SIZE));
		assert_ok!(accept(SYSTEM_CHANNEL));

		let message_id = Requests::<Test>::get(SYSTEM_CHANNEL).unwrap().message_id;
		assert_ok!(respond(SYSTEM_CHANNEL, message_id, Ok((CAPACITY, MESSAGE_SIZE))));

		let channel = Channels::<Test>::get(SYSTEM_CHANNEL).unwrap();
		assert!(channel.sender_deposit.is_none());
		assert!(channel.recipient_deposit.is_none());
	});
}

#[test]
fn a_refused_system_channel_releases_nothing() {
	new_test_ext().execute_with(|| {
		assert_ok!(init(SYSTEM_CHANNEL, CAPACITY, MESSAGE_SIZE));
		assert_ok!(accept(SYSTEM_CHANNEL));
		let message_id = Requests::<Test>::get(SYSTEM_CHANNEL).unwrap().message_id;

		assert_ok!(respond(SYSTEM_CHANNEL, message_id, Err(FailureReason::InvalidPara)));

		assert!(Requests::<Test>::get(SYSTEM_CHANNEL).is_none());
		assert!(Channels::<Test>::get(SYSTEM_CHANNEL).is_none());
	});
}

#[test]
fn accept_open_channel_asks_the_relay_chain() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		assert_ok!(init(CHANNEL, CAPACITY, MESSAGE_SIZE));
		let message_id = Requests::<Test>::get(CHANNEL).unwrap().message_id;
		let _ = take_sent();
		let _ = hrmp_events();

		assert_ok!(accept(CHANNEL));

		let request = Requests::<Test>::get(CHANNEL).unwrap();
		assert!(matches!(request.state, RequestState::Accepted { kind: OpenKind::Agreed, .. }));
		// The id is the request's own, so the answer ties back to it.
		assert_eq!(request.message_id, message_id);
		assert_eq!(AcceptedRequestCount::<Test>::get(CHANNEL.recipient), 1);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), DEPOSIT);

		assert_eq!(
			take_sent(),
			vec![
				MessageToRelay::V1(MessageToRelayV1::OpenChannel {
					channel: CHANNEL,
					message_id,
					max_capacity: CAPACITY,
					max_message_size: MESSAGE_SIZE,
				}),
				MessageToRelay::V1(MessageToRelayV1::NotifyPara {
					para_id: CHANNEL.sender,
					notification: ParaNotification::ChannelAccepted {
						recipient: CHANNEL.recipient
					},
				}),
			]
		);
		assert_eq!(
			hrmp_events(),
			vec![Event::OpenChannelAccepted { channel: CHANNEL, message_id }]
		);
	});
}

#[test]
fn accept_open_channel_rejects_an_unknown_or_confirmed_request() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		assert_noop!(accept(CHANNEL), Error::<Test>::AcceptHrmpChannelDoesntExist);

		assert_ok!(init(CHANNEL, CAPACITY, MESSAGE_SIZE));
		assert_ok!(accept(CHANNEL));
		assert_noop!(accept(CHANNEL), Error::<Test>::AcceptHrmpChannelAlreadyConfirmed);
	});
}

/// The recipient's budget is spent at init, so that `RequestIndex` cannot outgrow its bound. The
/// same check on `accept` is unreachable through this path and stays only as a guard.
#[test]
fn init_open_channel_respects_the_recipients_inbound_limit() {
	new_test_ext().execute_with(|| {
		fund_para(CHANNEL.recipient);
		for i in 0..MAX_INBOUND_CHANNELS {
			let channel = ChannelId { sender: 3_000 + i, recipient: CHANNEL.recipient };
			fund_para(channel.sender);
			assert_ok!(init(channel, CAPACITY, MESSAGE_SIZE));
		}

		let one_too_many = ChannelId { sender: 4_000, recipient: CHANNEL.recipient };
		fund_para(one_too_many.sender);
		assert_noop!(
			init(one_too_many, CAPACITY, MESSAGE_SIZE),
			Error::<Test>::AcceptHrmpChannelLimitExceeded
		);
	});
}

#[test]
fn a_confirming_response_opens_the_channel() {
	new_test_ext().execute_with(|| {
		agreed(CHANNEL);
		let message_id = Requests::<Test>::get(CHANNEL).unwrap().message_id;

		assert_ok!(respond(CHANNEL, message_id, Ok((CAPACITY, MESSAGE_SIZE))));

		assert!(Requests::<Test>::get(CHANNEL).is_none());
		let channel = Channels::<Test>::get(CHANNEL).unwrap();
		assert_eq!(channel.max_capacity, CAPACITY);
		assert_eq!(channel.max_message_size, MESSAGE_SIZE);

		assert_eq!(EgressIndex::<Test>::get(CHANNEL.sender).to_vec(), vec![CHANNEL.recipient]);
		assert_eq!(IngressIndex::<Test>::get(CHANNEL.recipient).to_vec(), vec![CHANNEL.sender]);
		assert_eq!(OpenRequestCount::<Test>::get(CHANNEL.sender), 0);
		assert_eq!(AcceptedRequestCount::<Test>::get(CHANNEL.recipient), 0);

		// The deposits stay put for as long as the channel does.
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), DEPOSIT);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), DEPOSIT);

		assert_eq!(hrmp_events(), vec![Event::ChannelOpened { channel: CHANNEL, message_id }]);
	});
}

#[test]
fn a_refusing_response_releases_both_deposits() {
	new_test_ext().execute_with(|| {
		agreed(CHANNEL);
		let message_id = Requests::<Test>::get(CHANNEL).unwrap().message_id;

		assert_ok!(respond(CHANNEL, message_id, Err(FailureReason::InvalidPara)));

		assert!(Requests::<Test>::get(CHANNEL).is_none());
		assert!(Channels::<Test>::get(CHANNEL).is_none());
		assert_eq!(OpenRequestCount::<Test>::get(CHANNEL.sender), 0);
		assert_eq!(AcceptedRequestCount::<Test>::get(CHANNEL.recipient), 0);

		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), 0);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), 0);

		assert_eq!(
			hrmp_events(),
			vec![Event::OpenChannelFailed {
				channel: CHANNEL,
				message_id,
				reason: FailureReason::InvalidPara,
			}]
		);
	});
}

#[test]
fn a_response_must_match_a_request_this_chain_is_waiting_for() {
	new_test_ext().execute_with(|| {
		// Nothing at all for this channel.
		assert_noop!(
			respond(CHANNEL, 0, Ok((CAPACITY, MESSAGE_SIZE))),
			Error::<Test>::UnexpectedResponse
		);

		// Requested, but the relay chain was never asked.
		fund(CHANNEL);
		assert_ok!(init(CHANNEL, CAPACITY, MESSAGE_SIZE));
		let message_id = Requests::<Test>::get(CHANNEL).unwrap().message_id;
		assert_noop!(
			respond(CHANNEL, message_id, Ok((CAPACITY, MESSAGE_SIZE))),
			Error::<Test>::UnexpectedResponse
		);

		// Agreed, but the answer is for an older message.
		assert_ok!(accept(CHANNEL));
		assert_noop!(
			respond(CHANNEL, message_id.wrapping_add(1), Ok((CAPACITY, MESSAGE_SIZE))),
			Error::<Test>::UnexpectedResponse
		);
	});
}

#[test]
fn a_refusing_transport_unwinds_the_whole_request() {
	new_test_ext().execute_with(|| {
		fund_para(CHANNEL.sender);
		SendFails::set(true);

		assert_noop!(init(CHANNEL, CAPACITY, MESSAGE_SIZE), Error::<Test>::SendFailed);

		assert!(Requests::<Test>::get(CHANNEL).is_none());
		assert_eq!(OpenRequestCount::<Test>::get(CHANNEL.sender), 0);
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), 0);
	});
}

#[test]
fn force_clean_hrmp_is_only_for_the_channel_manager() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Hrmp::force_clean_hrmp(para_origin(CHANNEL.sender), CHANNEL.sender, 0, 0),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn force_clean_hrmp_rejects_a_short_witness() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);

		// One open channel each way, counting from either end.
		assert_noop!(force_clean(CHANNEL.sender, 0, 0), Error::<Test>::WrongWitness);
		assert_noop!(force_clean(CHANNEL.recipient, 0, 0), Error::<Test>::WrongWitness);
		// Over-declaring is fine.
		assert_ok!(force_clean(CHANNEL.sender, 4, 4));
	});
}

#[test]
fn force_clean_hrmp_counts_requests_towards_the_witness() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		assert_ok!(init(CHANNEL, CAPACITY, MESSAGE_SIZE));

		assert_noop!(force_clean(CHANNEL.sender, 0, 0), Error::<Test>::WrongWitness);
		assert_ok!(force_clean(CHANNEL.sender, 0, 1));
	});
}

#[test]
fn force_clean_hrmp_drops_channels_and_releases_both_deposits() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);
		let inbound = ChannelId { sender: CHANNEL.recipient, recipient: CHANNEL.sender };
		opened(inbound);

		assert_ok!(force_clean(CHANNEL.sender, 1, 1));

		assert!(Channels::<Test>::get(CHANNEL).is_none());
		assert!(Channels::<Test>::get(inbound).is_none());
		assert!(IngressIndex::<Test>::get(CHANNEL.sender).is_empty());
		assert!(EgressIndex::<Test>::get(CHANNEL.sender).is_empty());
		// The counterparty's indexes lose the cleaned para too.
		assert!(IngressIndex::<Test>::get(CHANNEL.recipient).is_empty());
		assert!(EgressIndex::<Test>::get(CHANNEL.recipient).is_empty());

		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), 0);
		assert_eq!(held(CHANNEL.sender, HoldReason::RecipientDeposit), 0);
		assert_eq!(held(CHANNEL.recipient, HoldReason::SenderDeposit), 0);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), 0);
	});
}

#[test]
fn force_clean_hrmp_drops_pending_requests_and_refunds_the_counterparty() {
	new_test_ext().execute_with(|| {
		let para = CHANNEL.sender;
		// One request the para sent and the other end accepted, one it was asked for.
		let outbound = CHANNEL;
		let inbound = ChannelId { sender: 2_002, recipient: para };
		fund(outbound);
		fund(inbound);
		assert_ok!(init(outbound, CAPACITY, MESSAGE_SIZE));
		assert_ok!(accept(outbound));
		assert_ok!(init(inbound, CAPACITY, MESSAGE_SIZE));
		let _ = take_sent();
		let _ = hrmp_events();

		assert_ok!(force_clean(para, 1, 1));

		assert!(Requests::<Test>::get(outbound).is_none());
		assert!(Requests::<Test>::get(inbound).is_none());
		assert!(RequestIndex::<Test>::get(para).is_empty());
		assert!(RequestIndex::<Test>::get(outbound.recipient).is_empty());
		assert!(RequestIndex::<Test>::get(inbound.sender).is_empty());
		assert_eq!(OpenRequestCount::<Test>::get(para), 0);
		assert_eq!(OpenRequestCount::<Test>::get(inbound.sender), 0);
		assert_eq!(AcceptedRequestCount::<Test>::get(outbound.recipient), 0);

		// Every deposit is released, the cleaned para's and both counterparties'.
		assert_eq!(held(para, HoldReason::SenderDeposit), 0);
		assert_eq!(held(outbound.recipient, HoldReason::RecipientDeposit), 0);
		assert_eq!(held(inbound.sender, HoldReason::SenderDeposit), 0);
	});
}

#[test]
fn force_clean_hrmp_leaves_other_paras_alone() {
	new_test_ext().execute_with(|| {
		let other = ChannelId { sender: 2_002, recipient: 2_003 };
		opened(CHANNEL);
		opened(other);
		let pending = ChannelId { sender: 2_002, recipient: 2_004 };
		fund(pending);
		assert_ok!(init(pending, CAPACITY, MESSAGE_SIZE));
		let _ = take_sent();
		let _ = hrmp_events();

		assert_ok!(force_clean(CHANNEL.sender, 1, 1));

		assert!(Channels::<Test>::get(other).is_some());
		assert_eq!(EgressIndex::<Test>::get(other.sender).to_vec(), vec![other.recipient]);
		assert_eq!(IngressIndex::<Test>::get(other.recipient).to_vec(), vec![other.sender]);
		assert!(Requests::<Test>::get(pending).is_some());
		assert_eq!(RequestIndex::<Test>::get(pending.sender).to_vec(), vec![pending]);
		assert_eq!(held(other.sender, HoldReason::SenderDeposit), DEPOSIT * 2);
		assert_eq!(held(other.recipient, HoldReason::RecipientDeposit), DEPOSIT);
	});
}

#[test]
fn force_clean_hrmp_asks_the_relay_chain() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);
		let _ = take_sent();

		assert_ok!(force_clean(CHANNEL.sender, 2, 3));

		let message_id = match take_sent().as_slice() {
			[MessageToRelay::V1(MessageToRelayV1::ForceClean {
				para_id,
				message_id,
				num_inbound,
				num_outbound,
			})] => {
				assert_eq!(*para_id, CHANNEL.sender);
				assert_eq!((*num_inbound, *num_outbound), (2, 3));
				*message_id
			},
			sent => panic!("the relay chain was asked for exactly one clean, got {sent:?}"),
		};
		assert_eq!(
			hrmp_events(),
			vec![Event::ForceCleanExecuted { para_id: CHANNEL.sender, message_id }]
		);
	});
}

#[test]
fn a_refusing_transport_unwinds_the_force_clean() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);
		SendFails::set(true);

		assert_noop!(force_clean(CHANNEL.sender, 1, 1), Error::<Test>::SendFailed);

		assert!(Channels::<Test>::get(CHANNEL).is_some());
		assert_eq!(EgressIndex::<Test>::get(CHANNEL.sender).to_vec(), vec![CHANNEL.recipient]);
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), DEPOSIT);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), DEPOSIT);
	});
}
