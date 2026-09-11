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
	mock::*, AcceptedRequestCount, Channels, CloseRequests, EgressIndex, Error, Event, HoldReason,
	IngressIndex, OpenKind, OpenRequestCount, RequestState, Requests, UnexpectedKind,
};
use frame_support::{assert_noop, assert_ok};
use hrmp_primitives::{
	ChannelId, FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1,
	Outcome, ParaId, ParaNotification, ParaRequest, ParaRequestV1,
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

#[test]
fn accept_open_channel_respects_the_inbound_limit() {
	new_test_ext().execute_with(|| {
		fund_para(CHANNEL.recipient);
		for i in 0..MAX_INBOUND_CHANNELS {
			let channel = ChannelId { sender: 3_000 + i, recipient: CHANNEL.recipient };
			fund_para(channel.sender);
			assert_ok!(init(channel, CAPACITY, MESSAGE_SIZE));
			assert_ok!(accept(channel));
		}

		let one_too_many = ChannelId { sender: 4_000, recipient: CHANNEL.recipient };
		fund_para(one_too_many.sender);
		assert_ok!(init(one_too_many, CAPACITY, MESSAGE_SIZE));
		assert_noop!(accept(one_too_many), Error::<Test>::AcceptHrmpChannelLimitExceeded);
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

/// An open channel, with both logs cleared.
fn opened(channel: ChannelId) {
	agreed(channel);
	let message_id = Requests::<Test>::get(channel).unwrap().message_id;
	assert_ok!(respond(channel, message_id, Ok((CAPACITY, MESSAGE_SIZE))));
	let _ = take_sent();
	let _ = hrmp_events();
}

fn close(channel: ChannelId, initiator: ParaId) -> sp_runtime::DispatchResult {
	forwarded(initiator, ParaRequestV1::CloseChannel { channel })
}

fn respond_close(
	channel: ChannelId,
	message_id: u64,
	outcome: Outcome,
) -> sp_runtime::DispatchResult {
	Hrmp::receive(
		RuntimeOrigin::root(),
		MessageToPara::V1(MessageToParaV1::CloseResponse { channel, message_id, outcome }),
	)
}

/// Ask to close `channel` and hand back the id the relay chain will answer with.
fn close_pending(channel: ChannelId, initiator: ParaId) -> u64 {
	assert_ok!(close(channel, initiator));
	let message_id = CloseRequests::<Test>::get(channel).unwrap().message_id;
	let _ = take_sent();
	let _ = hrmp_events();
	message_id
}

#[test]
fn close_channel_records_the_request_and_asks_the_relay_chain() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);

		assert_ok!(close(CHANNEL, CHANNEL.sender));

		let request = CloseRequests::<Test>::get(CHANNEL).unwrap();
		assert_eq!(request.initiator, CHANNEL.sender);

		// The channel and both deposits stay until the relay chain answers.
		assert!(Channels::<Test>::get(CHANNEL).is_some());
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), DEPOSIT);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), DEPOSIT);

		assert_eq!(
			take_sent(),
			vec![MessageToRelay::V1(MessageToRelayV1::CloseChannel {
				channel: CHANNEL,
				message_id: request.message_id,
				initiator: CHANNEL.sender,
			})]
		);
		assert_eq!(
			hrmp_events(),
			vec![Event::ChannelClosedPending {
				channel: CHANNEL,
				message_id: request.message_id,
				by_parachain: CHANNEL.sender,
			}]
		);
	});
}

#[test]
fn close_channel_is_only_for_a_participant() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);

		assert_noop!(close(CHANNEL, 2002), Error::<Test>::CloseHrmpChannelUnauthorized);
	});
}

#[test]
fn close_channel_needs_an_open_channel() {
	new_test_ext().execute_with(|| {
		assert_noop!(close(CHANNEL, CHANNEL.sender), Error::<Test>::CloseHrmpChannelDoesntExist);

		// A request the relay chain has not confirmed is not a channel either.
		agreed(CHANNEL);
		assert_noop!(close(CHANNEL, CHANNEL.sender), Error::<Test>::CloseHrmpChannelDoesntExist);
	});
}

#[test]
fn close_channel_rejects_a_second_request() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);
		assert_ok!(close(CHANNEL, CHANNEL.sender));

		assert_noop!(
			close(CHANNEL, CHANNEL.recipient),
			Error::<Test>::CloseHrmpChannelAlreadyUnderway
		);
	});
}

#[test]
fn a_confirming_close_response_releases_both_deposits() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);
		let message_id = close_pending(CHANNEL, CHANNEL.sender);

		assert_ok!(respond_close(CHANNEL, message_id, Ok(())));

		assert!(CloseRequests::<Test>::get(CHANNEL).is_none());
		assert!(Channels::<Test>::get(CHANNEL).is_none());
		assert!(EgressIndex::<Test>::get(CHANNEL.sender).is_empty());
		assert!(IngressIndex::<Test>::get(CHANNEL.recipient).is_empty());
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), 0);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), 0);

		// Only the other end is told: the initiator asked for this.
		assert_eq!(
			take_sent(),
			vec![MessageToRelay::V1(MessageToRelayV1::NotifyPara {
				para_id: CHANNEL.recipient,
				notification: ParaNotification::ChannelClosing {
					initiator: CHANNEL.sender,
					sender: CHANNEL.sender,
					recipient: CHANNEL.recipient,
				},
			})]
		);
		assert_eq!(hrmp_events(), vec![Event::ChannelCloseDone { channel: CHANNEL, message_id }]);
	});
}

#[test]
fn either_end_may_close() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);
		let message_id = close_pending(CHANNEL, CHANNEL.recipient);

		assert_ok!(respond_close(CHANNEL, message_id, Ok(())));

		assert_eq!(
			take_sent(),
			vec![MessageToRelay::V1(MessageToRelayV1::NotifyPara {
				para_id: CHANNEL.sender,
				notification: ParaNotification::ChannelClosing {
					initiator: CHANNEL.recipient,
					sender: CHANNEL.sender,
					recipient: CHANNEL.recipient,
				},
			})]
		);
	});
}

#[test]
fn a_refused_close_keeps_the_channel_and_the_deposits() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);
		let message_id = close_pending(CHANNEL, CHANNEL.sender);

		assert_ok!(respond_close(CHANNEL, message_id, Err(FailureReason::NotFound)));

		assert!(CloseRequests::<Test>::get(CHANNEL).is_none());
		assert!(Channels::<Test>::get(CHANNEL).is_some());
		assert_eq!(EgressIndex::<Test>::get(CHANNEL.sender).to_vec(), vec![CHANNEL.recipient]);
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), DEPOSIT);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), DEPOSIT);

		assert!(take_sent().is_empty());
		assert_eq!(
			hrmp_events(),
			vec![Event::ChannelCloseFailed {
				channel: CHANNEL,
				message_id,
				reason: FailureReason::NotFound,
			}]
		);
	});
}

#[test]
fn a_close_response_must_match_a_request_this_chain_is_waiting_for() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);

		// Nothing is being closed.
		assert_noop!(respond_close(CHANNEL, 0, Ok(())), Error::<Test>::UnexpectedResponse);

		let message_id = close_pending(CHANNEL, CHANNEL.sender);
		assert_noop!(
			respond_close(CHANNEL, message_id + 1, Ok(())),
			Error::<Test>::UnexpectedResponse
		);
	});
}

#[test]
fn a_refusing_transport_unwinds_the_close_request() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);
		SendFails::set(true);

		assert_noop!(close(CHANNEL, CHANNEL.sender), Error::<Test>::SendFailed);

		assert!(CloseRequests::<Test>::get(CHANNEL).is_none());
		assert!(Channels::<Test>::get(CHANNEL).is_some());
	});
}

#[test]
fn a_system_channel_closes_without_touching_any_deposit() {
	new_test_ext().execute_with(|| {
		opened(SYSTEM_CHANNEL);
		let message_id = close_pending(SYSTEM_CHANNEL, SYSTEM_CHANNEL.sender);

		assert_ok!(respond_close(SYSTEM_CHANNEL, message_id, Ok(())));

		assert!(Channels::<Test>::get(SYSTEM_CHANNEL).is_none());
		assert_eq!(held(SYSTEM_CHANNEL.sender, HoldReason::SenderDeposit), 0);
		assert_eq!(
			hrmp_events(),
			vec![Event::ChannelCloseDone { channel: SYSTEM_CHANNEL, message_id }]
		);
	});
}

#[test]
fn a_refusing_transport_does_not_undo_a_confirmed_close() {
	new_test_ext().execute_with(|| {
		opened(CHANNEL);
		let message_id = close_pending(CHANNEL, CHANNEL.sender);
		SendFails::set(true);

		assert_ok!(respond_close(CHANNEL, message_id, Ok(())));

		// The relay chain has already closed the channel, so the state stands and the undelivered
		// notification is only recorded.
		assert!(Channels::<Test>::get(CHANNEL).is_none());
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), 0);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), 0);
		assert_eq!(
			hrmp_events(),
			vec![
				Event::Unexpected(UnexpectedKind::NotifyFailed { para_id: CHANNEL.recipient }),
				Event::ChannelCloseDone { channel: CHANNEL, message_id },
			]
		);
	});
}
