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
use sp_runtime::{traits::Convert, DispatchError, DispatchResult};

fn chan(sender: ParaId, recipient: ParaId) -> ChannelId {
	ChannelId { sender, recipient }
}

fn state_of(channel: ChannelId) -> Option<ChannelState<BlockNumber>> {
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
			assert!(matches!(state_of(channel), Some(ChannelState::Opening { .. })));
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
		build_and_execute(|| {
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
				state_of(chan(PARA_A, SYSTEM_PARA)),
				Some(ChannelState::Opening { .. })
			));
		});
	}

	#[test]
	fn refuses_parameters_the_relay_chain_would_not_take() {
		build_and_execute(|| {
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
			assert!(state_of(chan(PARA_A, PARA_B)).is_none());
			assert_eq!(held(PARA_A), 0);
			assert_eq!(crate::NextMessageId::<Test>::get(), 0);
		});
	}
}

mod responses {
	use super::*;

	#[test]
	fn a_refused_open_returns_the_deposit_and_forgets_the_channel() {
		build_and_execute(|| {
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
			assert!(state_of(channel).is_none());
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
		build_and_execute(|| {
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
			assert_eq!(state_of(channel), Some(ChannelState::Pending));
		});
	}

	#[test]
	fn only_a_confirmed_close_releases_the_deposits() {
		build_and_execute(|| {
			// GIVEN an open channel with both deposits held.
			let channel = open_channel(PARA_A, PARA_B);
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert_eq!(held(PARA_B), CHANNEL_DEPOSIT);

			// WHEN a close is merely requested.
			assert_ok!(Hrmp::close_channel(para_origin(PARA_A), PARA_A, PARA_B));

			// THEN nothing is released: the channel may still be carrying messages.
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert_eq!(held(PARA_B), CHANNEL_DEPOSIT);
			assert!(matches!(state_of(channel), Some(ChannelState::Closing { .. })));

			// WHEN the relay chain refuses.
			assert_ok!(Hrmp::receive(
				RuntimeOrigin::root(),
				close_response(channel, Err(FailureReason::NotFound))
			));
			// THEN the channel is open again, deposits untouched.
			assert_eq!(state_of(channel), Some(ChannelState::Open));
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);

			// WHEN it confirms instead.
			assert_ok!(Hrmp::close_channel(para_origin(PARA_A), PARA_A, PARA_B));
			assert_ok!(Hrmp::receive(RuntimeOrigin::root(), close_response(channel, Ok(()))));

			// THEN, and only then, both ends are made whole and the record is gone.
			assert_eq!(held(PARA_A), 0);
			assert_eq!(held(PARA_B), 0);
			assert!(state_of(channel).is_none());
		});
	}

	#[test]
	fn a_confirmed_cancellation_returns_the_senders_deposit() {
		build_and_execute(|| {
			let channel = pending_channel(PARA_A, PARA_B);

			assert_ok!(Hrmp::cancel_open_request(para_origin(PARA_A), PARA_A, PARA_B));
			// Still held while the relay chain has not answered.
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert!(matches!(state_of(channel), Some(ChannelState::Cancelling { .. })));

			assert_ok!(Hrmp::receive(RuntimeOrigin::root(), cancel_response(channel, Ok(()))));

			assert_eq!(held(PARA_A), 0);
			assert!(state_of(channel).is_none());
		});
	}

	#[test]
	fn a_refused_cancellation_puts_the_request_back() {
		build_and_execute(|| {
			let channel = pending_channel(PARA_A, PARA_B);
			assert_ok!(Hrmp::cancel_open_request(para_origin(PARA_A), PARA_A, PARA_B));

			assert_ok!(Hrmp::receive(
				RuntimeOrigin::root(),
				cancel_response(channel, Err(FailureReason::AlreadyExists))
			));

			assert_eq!(state_of(channel), Some(ChannelState::Pending));
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
		});
	}

	#[test]
	fn only_the_relay_chain_may_report() {
		build_and_execute(|| {
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
		build_and_execute(|| {
			assert_ok!(Hrmp::establish_system_channel(RuntimeOrigin::root(), SELF_PARA, PARA_A));

			// Both directions, because a one-way system channel is never what was meant.
			assert_eq!(state_of(chan(SELF_PARA, PARA_A)), Some(ChannelState::Open));
			assert_eq!(state_of(chan(PARA_A, SELF_PARA)), Some(ChannelState::Open));
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
		build_and_execute(|| {
			// GIVEN a para whose manager wants a channel to a system chain. Requiring root here
			// would mean a referendum before a new para could talk to Asset Hub.
			assert_ok!(Hrmp::establish_system_channel(
				RuntimeOrigin::signed(ALICE), // manager of PARA_A
				PARA_A,
				SYSTEM_PARA,
			));
			assert_eq!(state_of(chan(PARA_A, SYSTEM_PARA)), Some(ChannelState::Open));
			assert_eq!(state_of(chan(SYSTEM_PARA, PARA_A)), Some(ChannelState::Open));

			// The para itself may do the same, whichever way round the pair is given.
			assert_ok!(Hrmp::establish_system_channel(
				para_origin(PARA_B),
				SYSTEM_PARA,
				PARA_B,
			));
			assert_eq!(state_of(chan(SYSTEM_PARA, PARA_B)), Some(ChannelState::Open));

			// Deposit-free at both ends by definition, which is why this can stay open.
			assert_eq!(held(PARA_A), 0);
			assert_eq!(held(PARA_B), 0);
		});
	}

	#[test]
	fn a_para_cannot_use_it_to_pair_anyone_else() {
		build_and_execute(|| {
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
		build_and_execute(|| {
			// WHEN the registrar tells this pallet a para is registered.
			Hrmp::on_registered(PARA_A);

			// THEN this chain has a route to it, in both directions, at no cost. Without that a
			// para could never `Transact` into here, and the para-origin path above would be
			// unreachable for it.
			assert_eq!(state_of(chan(SELF_PARA, PARA_A)), Some(ChannelState::Open));
			assert_eq!(state_of(chan(PARA_A, SELF_PARA)), Some(ChannelState::Open));
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
		build_and_execute(|| {
			// GIVEN a transport that refuses everything.
			SendFails::set(true);

			// WHEN a para registers. Registration has already succeeded by this point, so this
			// must not be able to unwind anything.
			Hrmp::on_registered(PARA_A);

			// THEN the failure is an event, the records are rolled back, and the retry is
			// `establish_system_channel`.
			assert!(state_of(chan(SELF_PARA, PARA_A)).is_none());
			assert!(state_of(chan(PARA_A, SELF_PARA)).is_none());
			assert_eq!(
				hrmp_events(),
				vec![Event::SystemChannelFailed { channel: chan(SELF_PARA, PARA_A) }]
			);

			// And the retry works once the transport is back.
			SendFails::set(false);
			assert_ok!(Hrmp::establish_system_channel(RuntimeOrigin::root(), SELF_PARA, PARA_A));
			assert_eq!(state_of(chan(SELF_PARA, PARA_A)), Some(ChannelState::Open));
		});
	}
}

mod force_remove_channel {
	use super::*;

	#[test]
	fn root_can_only_tear_down_a_verdict_that_is_actually_overdue() {
		build_and_execute(|| {
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
			assert!(state_of(channel).is_none());
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
		build_and_execute(|| {
			// Nothing is in flight for an open channel, so there is no verdict to be overdue.
			let channel = open_channel(PARA_A, PARA_B);

			assert_ok!(Hrmp::force_remove_channel(RuntimeOrigin::root(), PARA_A, PARA_B));

			assert!(state_of(channel).is_none());
			assert_eq!(held(PARA_A), 0);
			assert_eq!(held(PARA_B), 0);
		});
	}
}

mod state_machine {
	use super::*;

	/// The six states a channel can be in, each with a way to reach it from nothing.
	///
	/// Written out rather than derived, so a new state cannot be added without deciding what every
	/// call does from it.
	pub(super) fn reach(state: &str, sender: ParaId, recipient: ParaId) -> ChannelId {
		let channel = chan(sender, recipient);
		let open = || {
			assert_ok!(Hrmp::open_channel(
				para_origin(sender),
				sender,
				recipient,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));
		};
		let confirm_open = || {
			assert_ok!(Hrmp::receive(RuntimeOrigin::root(), open_response(channel, Ok(()))));
		};
		match state {
			"Opening" => open(),
			"Pending" => {
				open();
				confirm_open();
			},
			"Accepting" => {
				open();
				confirm_open();
				assert_ok!(Hrmp::accept_open_channel(para_origin(recipient), sender, recipient));
			},
			"Open" => {
				open();
				confirm_open();
				assert_ok!(Hrmp::accept_open_channel(para_origin(recipient), sender, recipient));
				assert_ok!(Hrmp::receive(
					RuntimeOrigin::root(),
					accept_response(channel, Ok(()))
				));
			},
			"Closing" => {
				open();
				confirm_open();
				assert_ok!(Hrmp::accept_open_channel(para_origin(recipient), sender, recipient));
				assert_ok!(Hrmp::receive(
					RuntimeOrigin::root(),
					accept_response(channel, Ok(()))
				));
				assert_ok!(Hrmp::close_channel(para_origin(sender), sender, recipient));
			},
			"Cancelling" => {
				open();
				confirm_open();
				assert_ok!(Hrmp::cancel_open_request(para_origin(sender), sender, recipient));
			},
			other => panic!("unknown state {other}"),
		}
		let _ = take_sent();
		let _ = hrmp_events();
		channel
	}

	/// Every call, from every state, with the answer written down.
	///
	/// `None` means the call should succeed. A pallet whose state machine is only described by
	/// scattered `ensure!`s is one where a missing arm is invisible; this makes the whole table
	/// visible at once.
	const MATRIX: &[(&str, Option<&str>, Option<&str>, Option<&str>, Option<&str>)] = &[
		//  state          open              accept            close             cancel
		("Opening", Some("AlreadyExists"), Some("WrongState"), Some("WrongState"), Some("WrongState")),
		("Pending", Some("AlreadyExists"), None, Some("WrongState"), None),
		("Accepting", Some("AlreadyExists"), Some("WrongState"), Some("WrongState"), Some("WrongState")),
		("Open", Some("AlreadyExists"), Some("WrongState"), None, Some("WrongState")),
		("Closing", Some("AlreadyExists"), Some("WrongState"), Some("WrongState"), Some("WrongState")),
		("Cancelling", Some("AlreadyExists"), Some("WrongState"), Some("WrongState"), Some("WrongState")),
	];

	fn err_name(e: DispatchError) -> String {
		match e {
			DispatchError::Module(m) => m.message.unwrap_or("?").to_string(),
			other => format!("{other:?}"),
		}
	}

	fn check(expected: Option<&str>, got: DispatchResult, what: &str, state: &str) {
		match (expected, got) {
			(None, Ok(())) => {},
			(Some(want), Err(e)) => {
				assert_eq!(err_name(e), want, "{what} from {state}");
			},
			(None, Err(e)) => panic!("{what} from {state}: expected Ok, got {}", err_name(e)),
			(Some(want), Ok(())) => panic!("{what} from {state}: expected {want}, got Ok"),
		}
	}

	#[test]
	fn every_call_from_every_state_does_what_the_table_says() {
		for (state, open, accept, close, cancel) in MATRIX {
			// One fresh chain per state per call, so nothing a successful call does leaks into
			// the next row.
			build_and_execute(|| {
				reach(state, PARA_A, PARA_B);
				check(
					*open,
					Hrmp::open_channel(
						para_origin(PARA_A),
						PARA_A,
						PARA_B,
						MAX_CAPACITY,
						MAX_MESSAGE_SIZE,
					),
					"open_channel",
					state,
				);
			});
			build_and_execute(|| {
				reach(state, PARA_A, PARA_B);
				check(
					*accept,
					Hrmp::accept_open_channel(para_origin(PARA_B), PARA_A, PARA_B),
					"accept_open_channel",
					state,
				);
			});
			build_and_execute(|| {
				reach(state, PARA_A, PARA_B);
				check(
					*close,
					Hrmp::close_channel(para_origin(PARA_A), PARA_A, PARA_B),
					"close_channel",
					state,
				);
			});
			build_and_execute(|| {
				reach(state, PARA_A, PARA_B);
				check(
					*cancel,
					Hrmp::cancel_open_request(para_origin(PARA_A), PARA_A, PARA_B),
					"cancel_open_request",
					state,
				);
			});
		}
	}

	#[test]
	fn nothing_can_be_done_to_a_channel_this_chain_has_never_heard_of() {
		build_and_execute(|| {
			for (what, result) in [
				("accept", Hrmp::accept_open_channel(para_origin(PARA_B), PARA_A, PARA_B)),
				("close", Hrmp::close_channel(para_origin(PARA_A), PARA_A, PARA_B)),
				("cancel", Hrmp::cancel_open_request(para_origin(PARA_A), PARA_A, PARA_B)),
				("force_remove", Hrmp::force_remove_channel(RuntimeOrigin::root(), PARA_A, PARA_B)),
			] {
				assert_eq!(
					result,
					Err(Error::<Test>::NoSuchChannel.into()),
					"{what} on an unknown channel"
				);
			}
		});
	}

	#[test]
	fn the_two_directions_are_separate_channels() {
		build_and_execute(|| {
			// GIVEN A -> B open.
			let forward = open_channel(PARA_A, PARA_B);

			// THEN B -> A does not exist, and opening it is a fresh request with its own deposit.
			assert!(state_of(chan(PARA_B, PARA_A)).is_none());
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_B),
				PARA_B,
				PARA_A,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));

			// Each para now holds two deposits: one as sender, one as recipient.
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert_eq!(held(PARA_B), 2 * CHANNEL_DEPOSIT);
			assert_eq!(state_of(forward), Some(ChannelState::Open));
			assert!(matches!(
				state_of(chan(PARA_B, PARA_A)),
				Some(ChannelState::Opening { .. })
			));
		});
	}

	#[test]
	fn one_channel_settling_does_not_disturb_another() {
		build_and_execute(|| {
			// GIVEN two unrelated channels from the same sender.
			let first = open_channel(PARA_A, PARA_B);
			let second = pending_channel(PARA_A, PARA_C);
			assert_eq!(held(PARA_A), 2 * CHANNEL_DEPOSIT);

			// WHEN the second is cancelled and confirmed.
			assert_ok!(Hrmp::cancel_open_request(para_origin(PARA_A), PARA_A, PARA_C));
			assert_ok!(Hrmp::receive(RuntimeOrigin::root(), cancel_response(second, Ok(()))));

			// THEN only its deposit came back, and the first is untouched.
			assert!(state_of(second).is_none());
			assert_eq!(state_of(first), Some(ChannelState::Open));
			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert_eq!(held(PARA_B), CHANNEL_DEPOSIT);
		});
	}
}

mod message_ids {
	use super::*;

	#[test]
	fn every_request_carries_the_next_id_and_the_counter_never_repeats() {
		build_and_execute(|| {
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_B,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_C,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));

			let ids: Vec<u64> = take_sent()
				.into_iter()
				.map(|m| match m {
					MessageToRelay::V1(MessageToRelayV1::InitOpenChannel { message_id, .. }) =>
						message_id,
					other => panic!("unexpected message {other:?}"),
				})
				.collect();

			// Ids are what tie a request, its answer, and the two chains' events together, so
			// they must not repeat across channels.
			assert_eq!(ids, vec![0, 1]);
			assert_eq!(crate::NextMessageId::<Test>::get(), 2);
		});
	}

	#[test]
	fn a_failed_send_does_not_spend_an_id() {
		build_and_execute(|| {
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

			// The id was taken inside the call and unwound with it, so the next request still
			// gets 0 and no id is silently skipped.
			assert_eq!(crate::NextMessageId::<Test>::get(), 0);
			SendFails::set(false);
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_B,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));
			assert_eq!(crate::NextMessageId::<Test>::get(), 1);
		});
	}
}

mod deposits {
	use super::*;

	#[test]
	fn a_deposit_is_taken_from_and_returned_to_the_sovereign_account_not_the_caller() {
		build_and_execute(|| {
			// GIVEN a manager with a balance of their own. Alice manages PARA_A.
			let alice_before = Balances::free_balance(ALICE);
			let sovereign = SovereignOf::convert(PARA_A);
			let sovereign_before = Balances::free_balance(sovereign);

			// WHEN the manager opens a channel and then cancels it.
			let channel = pending_channel(PARA_A, PARA_B);
			assert_eq!(Balances::free_balance(ALICE), alice_before, "manager paid");
			assert_eq!(
				Balances::free_balance(sovereign),
				sovereign_before - CHANNEL_DEPOSIT,
				"deposit did not come off the sovereign account"
			);

			assert_ok!(Hrmp::cancel_open_request(RuntimeOrigin::signed(ALICE), PARA_A, PARA_B));
			assert_ok!(Hrmp::receive(RuntimeOrigin::root(), cancel_response(channel, Ok(()))));

			// THEN the money is back where it came from, and the manager's balance never moved.
			// This is what makes a migrated channel and a fresh one indistinguishable.
			assert_eq!(Balances::free_balance(ALICE), alice_before);
			assert_eq!(Balances::free_balance(sovereign), sovereign_before);
		});
	}

	#[test]
	fn each_end_pays_its_own_half_from_its_own_account() {
		build_and_execute(|| {
			let channel = open_channel(PARA_A, PARA_B);

			assert_eq!(held(PARA_A), CHANNEL_DEPOSIT);
			assert_eq!(held(PARA_B), CHANNEL_DEPOSIT);

			assert_ok!(Hrmp::close_channel(para_origin(PARA_A), PARA_A, PARA_B));
			assert_ok!(Hrmp::receive(RuntimeOrigin::root(), close_response(channel, Ok(()))));

			// A close returns both halves, each to the end that paid it.
			assert_eq!(held(PARA_A), 0);
			assert_eq!(held(PARA_B), 0);
		});
	}

	#[test]
	fn a_para_that_cannot_pay_cannot_open_a_channel() {
		build_and_execute(|| {
			// GIVEN a sovereign account with nothing in it. PARA_C is funded at genesis, so drain
			// it rather than picking an unfunded id, to prove it is the balance that matters.
			let sovereign = SovereignOf::convert(PARA_C);
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), sovereign, 1));

			assert_noop!(
				Hrmp::open_channel(
					para_origin(PARA_C),
					PARA_C,
					PARA_A,
					MAX_CAPACITY,
					MAX_MESSAGE_SIZE
				),
				sp_runtime::DispatchError::Token(sp_runtime::TokenError::FundsUnavailable)
			);
			// Nothing half-done.
			assert!(state_of(chan(PARA_C, PARA_A)).is_none());
			assert_eq!(take_sent(), vec![]);
		});
	}
}

mod invariants {
	use super::*;

	/// Move `channel`'s record to `key`, mutated by `f`, and put everything back afterwards.
	///
	/// Corruption is done by relocating a real record rather than by building a ticket by hand,
	/// because `Consideration` tickets cannot be conjured — which is the point of them.
	fn with_corrupted(
		channel: ChannelId,
		key: ChannelId,
		f: impl FnOnce(&mut crate::ChannelInfoOf<Test>),
	) {
		let original = Channels::<Test>::take(channel).expect("channel exists");
		let mut corrupted = original.clone();
		f(&mut corrupted);
		Channels::<Test>::insert(key, corrupted);

		assert_try_state_invalid();

		Channels::<Test>::remove(key);
		Channels::<Test>::insert(channel, original);
		assert_try_state_ok();
	}

	#[test]
	fn a_channel_past_acceptance_must_hold_both_deposits() {
		build_and_execute(|| {
			// GIVEN a channel holding only the sender's half, which is correct for `Opening`.
			let channel = chan(PARA_A, PARA_B);
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_B,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));

			// WHEN it claims to be Open without the recipient ever having paid.
			with_corrupted(channel, channel, |info| info.state = ChannelState::Open);
		});
	}

	#[test]
	fn a_channel_before_acceptance_must_not_hold_the_recipients_deposit() {
		build_and_execute(|| {
			// GIVEN a fully open channel, holding both halves.
			let channel = open_channel(PARA_A, PARA_B);

			// WHEN it claims to be back at Pending while still holding the recipient's half. A
			// refused acceptance is supposed to return that money; this is what it looks like if
			// it ever stopped doing so.
			with_corrupted(channel, channel, |info| info.state = ChannelState::Pending);
		});
	}

	#[test]
	fn a_system_channel_must_not_hold_a_deposit_at_all() {
		build_and_execute(|| {
			// GIVEN a paid-for channel between two public paras.
			let channel = chan(PARA_A, PARA_B);
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_B,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));

			// WHEN the same record turns up under a system-chain key. System channels are free at
			// both ends, so money held against one is money held against nothing.
			with_corrupted(channel, chan(PARA_A, SYSTEM_PARA), |_| {});
		});
	}

	#[test]
	fn a_channel_cannot_have_the_same_para_at_both_ends() {
		build_and_execute(|| {
			let channel = chan(PARA_A, PARA_B);
			assert_ok!(Hrmp::open_channel(
				para_origin(PARA_A),
				PARA_A,
				PARA_B,
				MAX_CAPACITY,
				MAX_MESSAGE_SIZE
			));

			with_corrupted(channel, chan(PARA_A, PARA_A), |_| {});
		});
	}
}

mod force_remove_from_every_state {
	use super::*;

	/// Governance must be able to unstick a channel from any state it can be in, releasing
	/// whatever is held. Anything this cannot reach is money nobody can ever get back.
	#[test]
	fn every_state_can_be_torn_down_and_gives_the_deposits_back() {
		for (state, sender_held, recipient_held) in [
			("Opening", CHANNEL_DEPOSIT, 0),
			("Pending", CHANNEL_DEPOSIT, 0),
			("Accepting", CHANNEL_DEPOSIT, CHANNEL_DEPOSIT),
			("Open", CHANNEL_DEPOSIT, CHANNEL_DEPOSIT),
			("Closing", CHANNEL_DEPOSIT, CHANNEL_DEPOSIT),
			("Cancelling", CHANNEL_DEPOSIT, 0),
		] {
			build_and_execute(|| {
				let channel = state_machine::reach(state, PARA_A, PARA_B);
				assert_eq!(held(PARA_A), sender_held, "sender hold in {state}");
				assert_eq!(held(PARA_B), recipient_held, "recipient hold in {state}");

				// In-flight states refuse until the verdict is genuinely overdue, so a slow
				// answer cannot be torn down from under the relay chain.
				run_to_block(System::block_number() + PENDING_DEADLINE);

				assert_ok!(Hrmp::force_remove_channel(RuntimeOrigin::root(), PARA_A, PARA_B));

				assert!(state_of(channel).is_none(), "record survived in {state}");
				assert_eq!(held(PARA_A), 0, "sender still held after {state}");
				assert_eq!(held(PARA_B), 0, "recipient still held after {state}");
			});
		}
	}

	#[test]
	fn a_system_channel_can_be_torn_down_too() {
		build_and_execute(|| {
			assert_ok!(Hrmp::establish_system_channel(RuntimeOrigin::root(), SELF_PARA, PARA_A));

			// Nothing is in flight for an Open channel, so no deadline applies.
			assert_ok!(Hrmp::force_remove_channel(RuntimeOrigin::root(), SELF_PARA, PARA_A));

			assert!(state_of(chan(SELF_PARA, PARA_A)).is_none());
			// The other direction is a separate record and is left alone.
			assert_eq!(state_of(chan(PARA_A, SELF_PARA)), Some(ChannelState::Open));
		});
	}
}
