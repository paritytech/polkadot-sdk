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
	OpenKind, OpenRequestCount, RequestState, Requests, WeightInfo,
};
use frame_support::{assert_noop, assert_ok, dispatch::DispatchResultWithPostInfo};
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

/// The channel manager forcing a channel open, which on this chain is Root.
fn force_open(channel: ChannelId, capacity: u32, message_size: u32) -> DispatchResultWithPostInfo {
	Hrmp::force_open_hrmp_channel(
		RuntimeOrigin::root(),
		channel.sender,
		channel.recipient,
		capacity,
		message_size,
	)
}

/// The weight `force_open_hrmp_channel` reports when it did or did not have to cancel a request.
fn force_open_weight(cancelled: u32) -> frame_support::weights::Weight {
	<TestWeights as WeightInfo>::force_open_hrmp_channel(cancelled)
}

#[test]
fn force_open_records_an_accepted_request_and_asks_the_relay_chain() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);

		assert_ok!(force_open(CHANNEL, CAPACITY, MESSAGE_SIZE));

		let request = Requests::<Test>::get(CHANNEL).unwrap();
		assert!(matches!(request.state, RequestState::Accepted { kind: OpenKind::Forced, .. }));
		assert_eq!(request.max_capacity, CAPACITY);
		assert_eq!(request.max_message_size, MESSAGE_SIZE);
		assert_eq!(OpenRequestCount::<Test>::get(CHANNEL.sender), 1);
		assert_eq!(AcceptedRequestCount::<Test>::get(CHANNEL.recipient), 1);

		// Both ends pay, even though only one of them asked for anything.
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), DEPOSIT);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), DEPOSIT);

		// Nothing is asked of the recipient, so it is told nothing.
		assert_eq!(
			take_sent(),
			vec![MessageToRelay::V1(MessageToRelayV1::ForceOpenChannel {
				channel: CHANNEL,
				message_id: request.message_id,
				max_capacity: CAPACITY,
				max_message_size: MESSAGE_SIZE,
			})]
		);
		assert_eq!(
			hrmp_events(),
			vec![Event::ForceOpenRequested { channel: CHANNEL, message_id: request.message_id }]
		);
	});
}

#[test]
fn force_open_is_only_for_the_channel_manager() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);

		for origin in [RuntimeOrigin::signed(ALICE), para_origin(CHANNEL.sender)] {
			assert_noop!(
				Hrmp::force_open_hrmp_channel(
					origin,
					CHANNEL.sender,
					CHANNEL.recipient,
					CAPACITY,
					MESSAGE_SIZE,
				),
				DispatchError::BadOrigin
			);
		}
		assert!(Requests::<Test>::get(CHANNEL).is_none());
	});
}

#[test]
fn force_open_rejects_bad_parameters() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		let to_self = ChannelId { sender: CHANNEL.sender, recipient: CHANNEL.sender };
		assert_noop!(
			force_open(to_self, CAPACITY, MESSAGE_SIZE),
			Error::<Test>::OpenHrmpChannelToSelf
		);
		assert_noop!(
			force_open(CHANNEL, 0, MESSAGE_SIZE),
			Error::<Test>::OpenHrmpChannelZeroCapacity
		);
		assert_noop!(
			force_open(CHANNEL, MAX_CAPACITY + 1, MESSAGE_SIZE),
			Error::<Test>::OpenHrmpChannelCapacityExceedsLimit
		);
		assert_noop!(
			force_open(CHANNEL, CAPACITY, 0),
			Error::<Test>::OpenHrmpChannelZeroMessageSize
		);
		assert_noop!(
			force_open(CHANNEL, CAPACITY, MAX_MESSAGE_SIZE + 1),
			Error::<Test>::OpenHrmpChannelMessageSizeExceedsLimit
		);

		// An open channel is not opened twice.
		assert_ok!(force_open(CHANNEL, CAPACITY, MESSAGE_SIZE));
		let message_id = Requests::<Test>::get(CHANNEL).unwrap().message_id;
		assert_ok!(respond(CHANNEL, message_id, Ok((CAPACITY, MESSAGE_SIZE))));
		assert_noop!(
			force_open(CHANNEL, CAPACITY, MESSAGE_SIZE),
			Error::<Test>::OpenHrmpChannelAlreadyExists
		);
	});
}

#[test]
fn force_open_respects_the_outbound_limit() {
	new_test_ext().execute_with(|| {
		fund_para(CHANNEL.sender);
		for i in 0..MAX_OUTBOUND_CHANNELS {
			let channel = ChannelId { sender: CHANNEL.sender, recipient: 3_000 + i };
			fund_para(channel.recipient);
			assert_ok!(force_open(channel, CAPACITY, MESSAGE_SIZE));
		}

		let one_too_many = ChannelId { sender: CHANNEL.sender, recipient: 4_000 };
		fund_para(one_too_many.recipient);
		assert_noop!(
			force_open(one_too_many, CAPACITY, MESSAGE_SIZE),
			Error::<Test>::OpenHrmpChannelLimitExceeded
		);
	});
}

#[test]
fn force_open_respects_the_inbound_limit() {
	new_test_ext().execute_with(|| {
		fund_para(CHANNEL.recipient);
		for i in 0..MAX_INBOUND_CHANNELS {
			let channel = ChannelId { sender: 3_000 + i, recipient: CHANNEL.recipient };
			fund_para(channel.sender);
			assert_ok!(force_open(channel, CAPACITY, MESSAGE_SIZE));
		}

		let one_too_many = ChannelId { sender: 4_000, recipient: CHANNEL.recipient };
		fund_para(one_too_many.sender);
		assert_noop!(
			force_open(one_too_many, CAPACITY, MESSAGE_SIZE),
			Error::<Test>::AcceptHrmpChannelLimitExceeded
		);
	});
}

#[test]
fn force_open_at_the_outbound_limit_can_still_replace_a_request() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		// Every outbound slot taken, the channel being forced among them.
		assert_ok!(init(CHANNEL, CAPACITY, MESSAGE_SIZE));
		for i in 1..MAX_OUTBOUND_CHANNELS {
			let channel = ChannelId { sender: CHANNEL.sender, recipient: 3_000 + i };
			assert_ok!(init(channel, CAPACITY, MESSAGE_SIZE));
		}

		// The request it replaces frees the slot it was holding.
		assert_ok!(force_open(CHANNEL, CAPACITY, MESSAGE_SIZE));
		assert_eq!(OpenRequestCount::<Test>::get(CHANNEL.sender), MAX_OUTBOUND_CHANNELS);
	});
}

#[test]
fn force_open_clears_an_unaccepted_request() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		assert_ok!(init(CHANNEL, CAPACITY, MESSAGE_SIZE));
		let asked_for = Requests::<Test>::get(CHANNEL).unwrap().message_id;
		let _ = take_sent();
		let _ = hrmp_events();

		let info = force_open(CHANNEL, CAPACITY, MESSAGE_SIZE).unwrap();
		assert_eq!(info.actual_weight, Some(force_open_weight(1)));

		let request = Requests::<Test>::get(CHANNEL).unwrap();
		assert!(matches!(request.state, RequestState::Accepted { kind: OpenKind::Forced, .. }));
		// A new request, so a new id: the answer to the old one is no longer owed.
		assert_ne!(request.message_id, asked_for);
		// The old deposit was released and a fresh one taken, rather than stacked.
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), DEPOSIT);
		assert_eq!(OpenRequestCount::<Test>::get(CHANNEL.sender), 1);
	});
}

#[test]
fn force_open_refuses_a_request_the_relay_chain_already_has() {
	new_test_ext().execute_with(|| {
		agreed(CHANNEL);

		assert_noop!(
			force_open(CHANNEL, CAPACITY, MESSAGE_SIZE),
			Error::<Test>::OpenHrmpChannelAlreadyConfirmed
		);
		assert!(matches!(
			Requests::<Test>::get(CHANNEL).unwrap().state,
			RequestState::Accepted { kind: OpenKind::Agreed, .. }
		));
	});
}

#[test]
fn force_open_refunds_weight_when_nothing_was_cancelled() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);

		let info = force_open(CHANNEL, CAPACITY, MESSAGE_SIZE).unwrap();

		assert_eq!(info.actual_weight, Some(force_open_weight(0)));
	});
}

#[test]
fn a_refusing_transport_leaves_the_request_it_would_replace_alone() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		assert_ok!(init(CHANNEL, CAPACITY, MESSAGE_SIZE));
		let asked_for = Requests::<Test>::get(CHANNEL).unwrap().message_id;
		SendFails::set(true);

		assert_noop!(force_open(CHANNEL, CAPACITY, MESSAGE_SIZE), Error::<Test>::SendFailed);

		let request = Requests::<Test>::get(CHANNEL).unwrap();
		assert!(matches!(request.state, RequestState::Requested { .. }));
		assert_eq!(request.message_id, asked_for);
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), DEPOSIT);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), 0);
	});
}

#[test]
fn a_forced_response_opens_the_channel_under_its_own_event() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		assert_ok!(force_open(CHANNEL, CAPACITY, MESSAGE_SIZE));
		let message_id = Requests::<Test>::get(CHANNEL).unwrap().message_id;
		let _ = hrmp_events();

		assert_ok!(respond(CHANNEL, message_id, Ok((CAPACITY, MESSAGE_SIZE))));

		assert!(Requests::<Test>::get(CHANNEL).is_none());
		let channel = Channels::<Test>::get(CHANNEL).unwrap();
		assert_eq!(channel.max_capacity, CAPACITY);
		assert_eq!(channel.max_message_size, MESSAGE_SIZE);
		assert_eq!(EgressIndex::<Test>::get(CHANNEL.sender).to_vec(), vec![CHANNEL.recipient]);
		assert_eq!(IngressIndex::<Test>::get(CHANNEL.recipient).to_vec(), vec![CHANNEL.sender]);
		assert_eq!(OpenRequestCount::<Test>::get(CHANNEL.sender), 0);
		assert_eq!(AcceptedRequestCount::<Test>::get(CHANNEL.recipient), 0);

		assert_eq!(
			hrmp_events(),
			vec![Event::HrmpChannelForceOpened {
				channel: CHANNEL,
				proposed_max_capacity: CAPACITY,
				proposed_max_message_size: MESSAGE_SIZE,
			}]
		);
	});
}

#[test]
fn a_refused_force_open_releases_both_deposits() {
	new_test_ext().execute_with(|| {
		fund(CHANNEL);
		assert_ok!(force_open(CHANNEL, CAPACITY, MESSAGE_SIZE));
		let message_id = Requests::<Test>::get(CHANNEL).unwrap().message_id;
		let _ = hrmp_events();

		assert_ok!(respond(CHANNEL, message_id, Err(FailureReason::LimitExceeded)));

		assert!(Requests::<Test>::get(CHANNEL).is_none());
		assert!(Channels::<Test>::get(CHANNEL).is_none());
		assert_eq!(held(CHANNEL.sender, HoldReason::SenderDeposit), 0);
		assert_eq!(held(CHANNEL.recipient, HoldReason::RecipientDeposit), 0);
		assert_eq!(OpenRequestCount::<Test>::get(CHANNEL.sender), 0);
		assert_eq!(AcceptedRequestCount::<Test>::get(CHANNEL.recipient), 0);

		assert_eq!(
			hrmp_events(),
			vec![Event::OpenChannelFailed {
				channel: CHANNEL,
				message_id,
				reason: FailureReason::LimitExceeded,
			}]
		);
	});
}

#[test]
fn a_forced_channel_with_a_system_chain_takes_no_deposit() {
	new_test_ext().execute_with(|| {
		// Neither end is funded: a channel with the system holds nothing rather than zero.
		assert_ok!(force_open(SYSTEM_CHANNEL, CAPACITY, MESSAGE_SIZE));

		assert_eq!(held(SYSTEM_CHANNEL.sender, HoldReason::SenderDeposit), 0);
		assert_eq!(held(SYSTEM_CHANNEL.recipient, HoldReason::RecipientDeposit), 0);

		let message_id = Requests::<Test>::get(SYSTEM_CHANNEL).unwrap().message_id;
		assert_ok!(respond(SYSTEM_CHANNEL, message_id, Ok((CAPACITY, MESSAGE_SIZE))));
		let channel = Channels::<Test>::get(SYSTEM_CHANNEL).unwrap();
		assert!(channel.sender_deposit.is_none());
		assert!(channel.recipient_deposit.is_none());
	});
}
