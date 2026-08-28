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

use crate::{mock::*, ChannelState, Channels, Error, Event};
use frame_support::{assert_noop, assert_ok};
use hrmp_primitives::{
	ChannelId, FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1,
	OnParaRegistered, Outcome, ParaId,
};
use sp_runtime::DispatchError;

fn chan(sender: ParaId, recipient: ParaId) -> ChannelId {
	ChannelId { sender, recipient }
}

fn state(channel: ChannelId) -> Option<ChannelState<BlockNumber>> {
	Channels::<Test>::get(channel).map(|info| info.state)
}

/// Open a request from `sender` and have the relay chain confirm it, leaving it `Pending`.
fn pending_channel(sender: ParaId, recipient: ParaId) -> ChannelId {
	let channel = chan(sender, recipient);
	assert_ok!(Hrmp::open_channel(
		para_origin(sender),
		sender,
		recipient,
		MAX_CAPACITY,
		MAX_MESSAGE_SIZE
	));
	assert_ok!(Hrmp::receive(RuntimeOrigin::root(), open_response(channel, Ok(()))));
	let _ = take_sent();
	let _ = hrmp_events();
	channel
}

/// A fully open channel, both deposits held.
fn open_channel(sender: ParaId, recipient: ParaId) -> ChannelId {
	let channel = pending_channel(sender, recipient);
	assert_ok!(Hrmp::accept_open_channel(para_origin(recipient), sender, recipient));
	assert_ok!(Hrmp::receive(RuntimeOrigin::root(), accept_response(channel, Ok(()))));
	let _ = take_sent();
	let _ = hrmp_events();
	channel
}

fn open_response(channel: ChannelId, outcome: Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::OpenResponse { channel, message_id: 0, outcome })
}

fn accept_response(channel: ChannelId, outcome: Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::AcceptResponse { channel, message_id: 1, outcome })
}

fn close_response(channel: ChannelId, outcome: Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::CloseResponse { channel, message_id: 2, outcome })
}

fn cancel_response(channel: ChannelId, outcome: Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::CancelResponse { channel, message_id: 2, outcome })
}

mod origins {
	use super::*;

	#[test]
	fn the_para_itself_and_its_manager_and_root_may_all_open() {
		new_test_ext().execute_with(|| {
			// GIVEN three paras, one opened by each kind of origin the pallet accepts.
			// WHEN each opens a channel.
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_C,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));
			assert_ok!(Hrmp::open_channel(
				RuntimeOrigin::signed(BOB), // manager of PARA_B
				PARA_B,
				PARA_C,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));
			assert_ok!(Hrmp::open_channel(
				RuntimeOrigin::root(),
				PARA_C,
				PARA_A,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));

			// THEN all three requests went out. The para-origin path is what preserves today's
			// trust model; the manager path is the fallback.
			assert_eq!(take_sent().len(), 3);
		});
	}

	#[test]
	fn a_stranger_and_the_wrong_manager_are_both_refused() {
		new_test_ext().execute_with(|| {
			// Charlie manages nothing.
			assert_noop!(
				Hrmp::open_channel(
					RuntimeOrigin::signed(CHARLIE),
					PARA_A,
					PARA_B,
					MAX_CAPACITY,
					MAX_MESSAGE_SIZE
				),
				Error::<Test>::NotOwner
			);
			// Bob manages PARA_B, not PARA_A.
			assert_noop!(
				Hrmp::open_channel(
					RuntimeOrigin::signed(BOB),
					PARA_A,
					PARA_B,
					MAX_CAPACITY,
					MAX_MESSAGE_SIZE
				),
				Error::<Test>::NotOwner
			);
			// A para may only speak for itself.
			assert_noop!(
				Hrmp::open_channel(
					para_origin(PARA_B),
					PARA_A,
					PARA_B,
					MAX_CAPACITY,
					MAX_MESSAGE_SIZE
				),
				Error::<Test>::NotOwner
			);
			assert_eq!(take_sent(), vec![]);
		});
	}

	#[test]
	fn either_end_may_close_and_the_relay_chain_is_told_which() {
		new_test_ext().execute_with(|| {
			// GIVEN an open channel A -> B.
			let channel = open_channel(PARA_A, PARA_B);

			// WHEN the *recipient's* manager closes it.
			assert_ok!(Hrmp::close_channel(RuntimeOrigin::signed(BOB), PARA_A, PARA_B));

			// THEN the relay chain is told B asked, not A: it has to know which end initiated.
			assert_eq!(
				take_sent(),
				vec![MessageToRelay::V1(MessageToRelayV1::CloseChannel {
					channel,
					message_id: 2,
					initiator: PARA_B,
				})]
			);
		});
	}
}

mod open_channel {
	use super::*;

	#[test]
	fn holds_the_sender_deposit_on_its_sovereign_account() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);

			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_B,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));

			// The deposit comes off the para's sovereign account here, not off whoever called.
			// That is what the migration produces, so migrated and fresh channels look alike.
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert_eq!(held(PARA_B), 0);
			assert!(matches!(state(channel), Some(ChannelState::Opening { .. })));
			assert_eq!(
				take_sent(),
				vec![MessageToRelay::V1(MessageToRelayV1::InitOpenChannel {
					channel,
					message_id: 0,
					max_capacity: MAX_CAPACITY,
					max_message_size: MAX_MESSAGE_SIZE,
				})]
			);
			assert_eq!(hrmp_events(), vec![Event::OpenRequested { channel, message_id: 0 }]);
		});
	}

	#[test]
	fn a_channel_touching_a_system_chain_is_deposit_free() {
		new_test_ext().execute_with(|| {
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				SYSTEM_PARA,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));

			// Mirrors the relay chain's own rule, so migrated system channels need no deposit
			// found for them.
			assert_eq!(held(PARA_A), 0);
			assert!(matches!(
				state(chan(PARA_A, SYSTEM_PARA)),
				Some(ChannelState::Opening { .. })
			));
		});
	}

	#[test]
	fn refuses_parameters_the_relay_chain_would_not_take() {
		new_test_ext().execute_with(|| {
			for (capacity, size) in [
				(0, MAX_MESSAGE_SIZE),
				(MAX_CAPACITY + 1, MAX_MESSAGE_SIZE),
				(MAX_CAPACITY, 0),
				(MAX_CAPACITY, MAX_MESSAGE_SIZE + 1),
			] {
				assert_noop!(
					Hrmp::open_channel(para_origin(PARA_A), PARA_A, PARA_B, capacity, size),
					Error::<Test>::InvalidParameters
				);
			}
			// Failing early costs the relay chain nothing and the caller no deposit.
			assert_eq!(held(PARA_A), 0);
			assert_eq!(take_sent(), vec![]);
		});
	}

	#[test]
	fn refuses_a_channel_to_itself_and_a_duplicate() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Hrmp::open_channel(
					para_origin(PARA_A),
					PARA_A,
					PARA_A,
					MAX_CAPACITY,
					MAX_MESSAGE_SIZE
				),
				Error::<Test>::ToSelf
			);

			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_B,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));
			assert_noop!(
				Hrmp::open_channel(
					para_origin(PARA_A),
					PARA_A,
					PARA_B,
					MAX_CAPACITY,
					MAX_MESSAGE_SIZE
				),
				Error::<Test>::AlreadyExists
			);
			// Only one deposit was ever taken.
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		new_test_ext().execute_with(|| {
			SendFails::set(true);

			assert_noop!(
				Hrmp::open_channel(
					para_origin(PARA_A),
					PARA_A,
					PARA_B,
					MAX_CAPACITY,
					MAX_MESSAGE_SIZE
				),
				Error::<Test>::SendFailed
			);

			// Nothing half-done: no record, no deposit, and the message id was not spent.
			assert!(state(chan(PARA_A, PARA_B)).is_none());
			assert_eq!(held(PARA_A), 0);
			assert_eq!(crate::NextMessageId::<Test>::get(), 0);
		});
	}
}

mod responses {
	use super::*;

	#[test]
	fn a_refused_open_returns_the_deposit_and_forgets_the_channel() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_B,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));
			let _ = hrmp_events();

			assert_ok!(Hrmp::receive(
				RuntimeOrigin::root(),
				open_response(channel, Err(FailureReason::LimitExceeded))
			));

			assert_eq!(held(PARA_A), 0);
			assert!(state(channel).is_none());
			assert_eq!(
				hrmp_events(),
				vec![Event::OpenFailed {
					channel,
					message_id: 0,
					reason: FailureReason::LimitExceeded
				}]
			);
		});
	}

	#[test]
	fn a_refused_acceptance_returns_only_the_recipients_half() {
		new_test_ext().execute_with(|| {
			// GIVEN a request the relay chain is holding, and a recipient who has just accepted.
			let channel = pending_channel(PARA_A, PARA_B);
			assert_ok!(Hrmp::accept_open_channel(para_origin(PARA_B), PARA_A, PARA_B));
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert_eq!(held(PARA_B), CHANNEL_DEPOSIT);
			let _ = hrmp_events();

			// WHEN the relay chain refuses the acceptance.
			assert_ok!(Hrmp::receive(
				RuntimeOrigin::root(),
				accept_response(channel, Err(FailureReason::LimitExceeded))
			));

			// THEN only the recipient is made whole. The request itself still stands on the relay
			// chain, so the sender's deposit is still owed.
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert_eq!(held(PARA_B), 0);
			assert_eq!(state(channel), Some(ChannelState::Pending));
		});
	}

	#[test]
	fn only_a_confirmed_close_releases_the_deposits() {
		new_test_ext().execute_with(|| {
			// GIVEN an open channel with both deposits held.
			let channel = open_channel(PARA_A, PARA_B);
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert_eq!(held(PARA_B), CHANNEL_DEPOSIT);

			// WHEN a close is merely requested.
			assert_ok!(Hrmp::close_channel(para_origin(PARA_A), PARA_A, PARA_B));

			// THEN nothing is released: the channel may still be carrying messages.
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert_eq!(held(PARA_B), CHANNEL_DEPOSIT);
			assert!(matches!(state(channel), Some(ChannelState::Closing { .. })));

			// WHEN the relay chain refuses.
			assert_ok!(Hrmp::receive(
				RuntimeOrigin::root(),
				close_response(channel, Err(FailureReason::NotFound))
			));
			// THEN the channel is open again, deposits untouched.
			assert_eq!(state(channel), Some(ChannelState::Open));
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);

			// WHEN it confirms instead.
			assert_ok!(Hrmp::close_channel(para_origin(PARA_A), PARA_A, PARA_B));
			assert_ok!(Hrmp::receive(RuntimeOrigin::root(), close_response(channel, Ok(()))));

			// THEN, and only then, both ends are made whole and the record is gone.
			assert_eq!(held(PARA_A), 0);
			assert_eq!(held(PARA_B), 0);
			assert!(state(channel).is_none());
		});
	}

	#[test]
	fn a_confirmed_cancellation_returns_the_senders_deposit() {
		new_test_ext().execute_with(|| {
			let channel = pending_channel(PARA_A, PARA_B);

			assert_ok!(Hrmp::cancel_open_request(para_origin(PARA_A), PARA_A, PARA_B));
			// Still held while the relay chain has not answered.
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert!(matches!(state(channel), Some(ChannelState::Cancelling { .. })));

			assert_ok!(Hrmp::receive(RuntimeOrigin::root(), cancel_response(channel, Ok(()))));

			assert_eq!(held(PARA_A), 0);
			assert!(state(channel).is_none());
		});
	}

	#[test]
	fn a_refused_cancellation_puts_the_request_back() {
		new_test_ext().execute_with(|| {
			let channel = pending_channel(PARA_A, PARA_B);
			assert_ok!(Hrmp::cancel_open_request(para_origin(PARA_A), PARA_A, PARA_B));

			assert_ok!(Hrmp::receive(
				RuntimeOrigin::root(),
				cancel_response(channel, Err(FailureReason::AlreadyExists))
			));

			assert_eq!(state(channel), Some(ChannelState::Pending));
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
		});
	}

	#[test]
	fn only_the_relay_chain_may_report() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			assert_noop!(
				Hrmp::receive(RuntimeOrigin::signed(ALICE), open_response(channel, Ok(()))),
				DispatchError::BadOrigin
			);
		});
	}
}

mod wrong_state {
	use super::*;

	#[test]
	fn every_call_refuses_a_channel_that_is_not_ready_for_it() {
		new_test_ext().execute_with(|| {
			// GIVEN a request that is still in flight to the relay chain.
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_B,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));
			let _ = take_sent();

			// THEN nothing that needs a settled channel may run, and no message is built.
			assert_noop!(
				Hrmp::accept_open_channel(para_origin(PARA_B), PARA_A, PARA_B),
				Error::<Test>::WrongState
			);
			assert_noop!(
				Hrmp::close_channel(para_origin(PARA_A), PARA_A, PARA_B),
				Error::<Test>::WrongState
			);
			assert_noop!(
				Hrmp::cancel_open_request(para_origin(PARA_A), PARA_A, PARA_B),
				Error::<Test>::WrongState
			);
			assert_eq!(take_sent(), vec![]);
		});
	}

	#[test]
	fn acting_on_a_channel_this_chain_never_heard_of_is_refused() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Hrmp::accept_open_channel(para_origin(PARA_B), PARA_A, PARA_B),
				Error::<Test>::NoSuchChannel
			);
			assert_noop!(
				Hrmp::close_channel(para_origin(PARA_A), PARA_A, PARA_B),
				Error::<Test>::NoSuchChannel
			);
		});
	}
}

mod system_channels {
	use super::*;

	#[test]
	fn root_opens_both_directions_deposit_free() {
		new_test_ext().execute_with(|| {
			assert_ok!(Hrmp::establish_system_channel(RuntimeOrigin::root(), SELF_PARA, PARA_A));

			// Both directions, because a one-way system channel is never what was meant.
			assert_eq!(state(chan(SELF_PARA, PARA_A)), Some(ChannelState::Open));
			assert_eq!(state(chan(PARA_A, SELF_PARA)), Some(ChannelState::Open));
			assert_eq!(held(PARA_A), 0);
			assert_eq!(held(SELF_PARA), 0);
			assert_eq!(
				take_sent(),
				vec![MessageToRelay::V1(MessageToRelayV1::EstablishSystemChannel {
					channel: chan(SELF_PARA, PARA_A),
					message_id: 0,
				})]
			);
		});
	}

	#[test]
	fn a_para_may_pair_itself_with_a_system_chain_without_governance() {
		new_test_ext().execute_with(|| {
			// GIVEN a para whose manager wants a channel to a system chain. Requiring root here
			// would mean a referendum before a new para could talk to Asset Hub.
			assert_ok!(Hrmp::establish_system_channel(
				RuntimeOrigin::signed(ALICE), // manager of PARA_A
				PARA_A,
				SYSTEM_PARA,
			));
			assert_eq!(state(chan(PARA_A, SYSTEM_PARA)), Some(ChannelState::Open));
			assert_eq!(state(chan(SYSTEM_PARA, PARA_A)), Some(ChannelState::Open));

			// The para itself may do the same, whichever way round the pair is given.
			assert_ok!(Hrmp::establish_system_channel(
				para_origin(PARA_B),
				SYSTEM_PARA,
				PARA_B,
			));
			assert_eq!(state(chan(SYSTEM_PARA, PARA_B)), Some(ChannelState::Open));

			// Deposit-free at both ends by definition, which is why this can stay open.
			assert_eq!(held(PARA_A), 0);
			assert_eq!(held(PARA_B), 0);
		});
	}

	#[test]
	fn a_para_cannot_use_it_to_pair_anyone_else() {
		new_test_ext().execute_with(|| {
			// Not the caller's own para.
			assert_noop!(
				Hrmp::establish_system_channel(RuntimeOrigin::signed(ALICE), PARA_B, SYSTEM_PARA),
				Error::<Test>::NotOwner
			);
			// Neither end is a system chain: free channels between two public paras would be a
			// way around the deposit entirely.
			assert_noop!(
				Hrmp::establish_system_channel(RuntimeOrigin::signed(ALICE), PARA_A, PARA_B),
				Error::<Test>::NotOwner
			);
			// Both ends are system chains: only root pairs those.
			assert_noop!(
				Hrmp::establish_system_channel(
					RuntimeOrigin::signed(ALICE),
					SELF_PARA,
					SYSTEM_PARA
				),
				Error::<Test>::NotOwner
			);
			// A manager of nothing.
			assert_noop!(
				Hrmp::establish_system_channel(
					RuntimeOrigin::signed(CHARLIE),
					PARA_A,
					SYSTEM_PARA
				),
				Error::<Test>::NotOwner
			);
			assert_eq!(take_sent(), vec![]);
		});
	}

	#[test]
	fn registering_a_para_opens_a_channel_with_this_chain() {
		new_test_ext().execute_with(|| {
			// WHEN the registrar tells this pallet a para is registered.
			Hrmp::on_registered(PARA_A);

			// THEN this chain has a route to it, in both directions, at no cost. Without that a
			// para could never `Transact` into here, and the para-origin path above would be
			// unreachable for it.
			assert_eq!(state(chan(SELF_PARA, PARA_A)), Some(ChannelState::Open));
			assert_eq!(state(chan(PARA_A, SELF_PARA)), Some(ChannelState::Open));
			assert_eq!(held(PARA_A), 0);
			assert_eq!(
				hrmp_events(),
				vec![Event::SystemChannelOpened {
					channel: chan(SELF_PARA, PARA_A),
					message_id: 0
				}]
			);
		});
	}

	#[test]
	fn a_failed_channel_at_registration_is_reported_not_raised() {
		new_test_ext().execute_with(|| {
			// GIVEN a transport that refuses everything.
			SendFails::set(true);

			// WHEN a para registers. Registration has already succeeded by this point, so this
			// must not be able to unwind anything.
			Hrmp::on_registered(PARA_A);

			// THEN the failure is an event, the records are rolled back, and the retry is
			// `establish_system_channel`.
			assert!(state(chan(SELF_PARA, PARA_A)).is_none());
			assert!(state(chan(PARA_A, SELF_PARA)).is_none());
			assert_eq!(
				hrmp_events(),
				vec![Event::SystemChannelFailed { channel: chan(SELF_PARA, PARA_A) }]
			);

			// And the retry works once the transport is back.
			SendFails::set(false);
			assert_ok!(Hrmp::establish_system_channel(RuntimeOrigin::root(), SELF_PARA, PARA_A));
			assert_eq!(state(chan(SELF_PARA, PARA_A)), Some(ChannelState::Open));
		});
	}
}

mod force_remove_channel {
	use super::*;

	#[test]
	fn root_can_only_tear_down_a_verdict_that_is_actually_overdue() {
		new_test_ext().execute_with(|| {
			// GIVEN a request in flight, whose verdict is not due yet.
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_B,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));
			let channel = chan(PARA_A, PARA_B);

			assert_noop!(
				Hrmp::force_remove_channel(RuntimeOrigin::root(), PARA_A, PARA_B),
				Error::<Test>::NotOverdue
			);
			assert_noop!(
				Hrmp::force_remove_channel(RuntimeOrigin::signed(ALICE), PARA_A, PARA_B),
				DispatchError::BadOrigin
			);

			// WHEN the deadline has passed.
			run_to_block(1 + PENDING_DEADLINE);

			// THEN governance can forget the channel and the deposit comes back.
			assert_ok!(Hrmp::force_remove_channel(RuntimeOrigin::root(), PARA_A, PARA_B));
			assert!(state(channel).is_none());
			assert_eq!(held(PARA_A), 0);
			assert_eq!(
				hrmp_events(),
				vec![
					Event::OpenRequested { channel, message_id: 0 },
					Event::ChannelForceRemoved { channel },
				]
			);
		});
	}

	#[test]
	fn a_settled_channel_needs_no_deadline_and_releases_both_ends() {
		new_test_ext().execute_with(|| {
			// Nothing is in flight for an open channel, so there is no verdict to be overdue.
			let channel = open_channel(PARA_A, PARA_B);

			assert_ok!(Hrmp::force_remove_channel(RuntimeOrigin::root(), PARA_A, PARA_B));

			assert!(state(channel).is_none());
			assert_eq!(held(PARA_A), 0);
			assert_eq!(held(PARA_B), 0);
		});
	}
}
