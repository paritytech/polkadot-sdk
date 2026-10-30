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

use crate::{mock::*, Call, Deposits, Error, Event, HoldReason, RECEIVE_CALL_INDEX};
use frame_support::{
	assert_noop, assert_ok, assert_storage_noop,
	traits::{
		fungible::{InspectHold, MutateHold},
		tokens::{Fortitude, Precision},
		GetCallIndex, UnfilteredDispatchable,
	},
};
use hrmp_primitives::{
	Balance, ChannelId, DepositError, DepositKey, DepositSide, MessageToPara, MessageToParaV1,
	MessageToRelay, MessageToRelayV1, ParaId, ReceiveMigratedDeposits,
};
use sp_runtime::{DispatchError, DispatchResult};

use DepositSide::{Recipient, Sender};

fn ch(sender: ParaId, recipient: ParaId) -> ChannelId {
	ChannelId { sender, recipient }
}

/// What `para` holds for channel deposits.
fn on_hold(para: ParaId) -> Balance {
	Balances::balance_on_hold(&HoldReason::ChannelDeposit.into(), &para.into())
}

fn deposit(channel: ChannelId, side: DepositSide) -> Option<Balance> {
	Deposits::<Test>::get((channel, side))
}

fn set_message(
	id: u64,
	channel: ChannelId,
	sender: Option<Balance>,
	recipient: Option<Balance>,
) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::SetDeposits { id, channel, sender, recipient })
}

fn set(
	id: u64,
	channel: ChannelId,
	sender: Option<Balance>,
	recipient: Option<Balance>,
) -> DispatchResult {
	HrmpPara::receive(RuntimeOrigin::root(), set_message(id, channel, sender, recipient))
}

fn release(channel: ChannelId, sender: bool, recipient: bool) {
	let message = MessageToPara::V1(MessageToParaV1::Release { channel, sender, recipient });
	assert_ok!(HrmpPara::receive(RuntimeOrigin::root(), message));
}

fn answer(id: u64, result: Result<(), DepositError>) -> MessageToRelay {
	MessageToRelay::V1(MessageToRelayV1::DepositsSet { id, result })
}

/// Takes the messages sent so far.
fn sent() -> Vec<MessageToRelay> {
	SENT.take()
}

/// Takes this pallet's events so far.
fn events() -> Vec<Event<Test>> {
	let events = System::events()
		.into_iter()
		.filter_map(|r| match r.event {
			RuntimeEvent::HrmpPara(e) => Some(e),
			_ => None,
		})
		.collect();
	System::reset_events();
	events
}

/// The relay chain encodes `receive` with [`RECEIVE_CALL_INDEX`].
#[test]
fn receive_call_index_ok() {
	let channel = ChannelId { sender: 0, recipient: 0 };
	let message =
		MessageToPara::V1(MessageToParaV1::Release { channel, sender: false, recipient: false });
	let call = Call::<Test>::receive { message };
	assert_eq!(call.get_call_index(), RECEIVE_CALL_INDEX);
}

/// Each side's deposit is held from its own para's sovereign account, and the request answered.
#[test]
fn set_deposits_holds_both_sides() {
	new_test_ext(|| {
		assert_ok!(set(7, ch(1, 2), Some(10), Some(20)));
		assert_eq!((on_hold(1), on_hold(2)), (10, 20));
		assert_eq!((deposit(ch(1, 2), Sender), deposit(ch(1, 2), Recipient)), (Some(10), Some(20)));
		assert_eq!(
			events(),
			vec![
				Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 10 },
				Event::DepositSet { channel: ch(1, 2), side: Recipient, amount: 20 },
			]
		);
		assert_eq!(sent(), vec![answer(7, Ok(()))]);
	});
}

/// A side named `None` is left as it is.
#[test]
fn set_deposits_one_side_leaves_other() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), Some(20)));
		events();

		assert_ok!(set(1, ch(1, 2), Some(15), None));
		assert_eq!((on_hold(1), on_hold(2)), (15, 20));
		assert_eq!(
			events(),
			vec![Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 15 }]
		);
	});
}

/// A request that names no side changes nothing, and is answered.
#[test]
fn set_deposits_none_none_answers_ok() {
	new_test_ext(|| {
		assert_ok!(set(3, ch(1, 2), None, None));
		assert!(events().is_empty());
		assert_eq!(Deposits::<Test>::iter().count(), 0);
		assert_eq!(sent(), vec![answer(3, Ok(()))]);
	});
}

/// A side holds or releases the difference to its target.
#[test]
fn set_deposits_raises_and_lowers() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		assert_ok!(set(1, ch(1, 2), Some(15), None));
		assert_eq!(on_hold(1), 15);
		assert_ok!(set(2, ch(1, 2), Some(4), None));
		assert_eq!(on_hold(1), 4);
		assert_eq!(Balances::free_balance(1), ENDOWMENT - 4);
		assert_eq!(deposit(ch(1, 2), Sender), Some(4));
	});
}

/// A side moved to zero releases everything and holds no entry.
#[test]
fn set_deposits_to_zero_removes_entry() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		events();

		assert_ok!(set(1, ch(1, 2), Some(0), None));
		assert_eq!(on_hold(1), 0);
		assert_eq!(deposit(ch(1, 2), Sender), None);
		assert_eq!(
			events(),
			vec![Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 0 }]
		);
	});
}

/// A side already at its target is answered `Ok` without moving funds.
#[test]
fn set_deposits_same_target() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		sent();

		assert_ok!(set(1, ch(1, 2), Some(10), None));
		assert_eq!(on_hold(1), 10);
		assert_eq!(Balances::free_balance(1), ENDOWMENT - 10);
		assert_eq!(sent(), vec![answer(1, Ok(()))]);
	});
}

/// If one side cannot hold, the other side's hold is undone: both sides move, or neither.
#[test]
fn set_deposits_is_atomic() {
	new_test_ext(|| {
		assert_ok!(set(4, ch(1, 2), Some(10), Some(ENDOWMENT + 1)));
		assert_eq!((on_hold(1), on_hold(2)), (0, 0));
		assert_eq!(Deposits::<Test>::iter().count(), 0);
		// The sender's `DepositSet` is undone with it.
		assert_eq!(events(), vec![Event::DepositRefused { id: 4, channel: ch(1, 2) }]);
		assert_eq!(sent(), vec![answer(4, Err(DepositError::InsufficientFunds))]);
	});
}

/// A release on one side is undone too when the other side cannot hold.
#[test]
fn set_deposits_atomic_with_release_and_hold() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), Some(10)));
		sent();

		assert_ok!(set(1, ch(1, 2), Some(5), Some(ENDOWMENT + 1)));
		assert_eq!((on_hold(1), on_hold(2)), (10, 10));
		assert_eq!((deposit(ch(1, 2), Sender), deposit(ch(1, 2), Recipient)), (Some(10), Some(10)));
		assert_eq!(sent(), vec![answer(1, Err(DepositError::InsufficientFunds))]);
	});
}

/// A para's deposits for different channels and sides add up in one hold.
#[test]
fn set_deposits_accumulate_across_channels() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		assert_ok!(set(1, ch(3, 1), None, Some(20)));
		assert_eq!(on_hold(1), 30);

		release(ch(1, 2), true, false);
		assert_eq!(on_hold(1), 20);
	});
}

/// A channel from a para to itself charges the same account for both sides.
#[test]
fn set_deposits_sender_equals_recipient() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 1), Some(10), Some(20)));
		assert_eq!(on_hold(1), 30);
		assert_eq!((deposit(ch(1, 1), Sender), deposit(ch(1, 1), Recipient)), (Some(10), Some(20)));
	});
}

/// An answer that cannot be sent fails the message, which then changes nothing: an unanswered
/// request holds nothing, so the relay chain may force a refusal.
#[test]
fn set_deposits_send_failure_reverts() {
	new_test_ext(|| {
		FAIL_SEND.set(true);
		let dispatch = |sender, recipient| {
			Call::<Test>::receive { message: set_message(0, ch(1, 2), sender, recipient) }
				.dispatch_bypass_filter(RuntimeOrigin::root())
		};
		// Applied.
		assert_noop!(dispatch(Some(10), Some(20)), Error::<Test>::SendFailed);
		// Refused.
		assert_noop!(dispatch(Some(10), Some(ENDOWMENT + 1)), Error::<Test>::SendFailed);
		assert!(sent().is_empty());
	});
}

/// A release gives back everything held for both sides, and is not answered.
#[test]
fn release_both_sides() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), Some(20)));
		sent();
		events();

		release(ch(1, 2), true, true);
		assert_eq!((on_hold(1), on_hold(2)), (0, 0));
		assert_eq!(Deposits::<Test>::iter().count(), 0);
		assert_eq!(
			events(),
			vec![
				Event::DepositReleased { channel: ch(1, 2), side: Sender, amount: 10 },
				Event::DepositReleased { channel: ch(1, 2), side: Recipient, amount: 20 },
			]
		);
		assert!(sent().is_empty());
	});
}

/// A release gives back only the sides it names.
#[test]
fn release_only_named_side() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), Some(20)));
		release(ch(1, 2), true, false);
		assert_eq!((on_hold(1), on_hold(2)), (0, 20));
		assert_eq!(deposit(ch(1, 2), Recipient), Some(20));
	});
}

/// Releasing a side that holds nothing is not an error.
#[test]
fn release_nothing_held_is_ok() {
	new_test_ext(|| {
		release(ch(1, 2), true, true);
		assert!(events().is_empty());
	});
}

/// A release leaves the para's deposits for other channels held.
#[test]
fn release_one_channel_keeps_others() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		assert_ok!(set(1, ch(1, 3), Some(20), None));
		release(ch(1, 2), true, true);
		assert_eq!(on_hold(1), 20);
		assert_eq!(deposit(ch(1, 3), Sender), Some(20));
	});
}

/// After part of a hold is slashed, a release gives back what is left and forgets the side.
#[test]
fn release_after_hold_partially_slashed() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		assert_ok!(Balances::burn_held(
			&HoldReason::ChannelDeposit.into(),
			&1,
			4,
			Precision::Exact,
			Fortitude::Force,
		));
		events();

		release(ch(1, 2), true, false);
		assert_eq!(on_hold(1), 0);
		assert_eq!(deposit(ch(1, 2), Sender), None);
		assert_eq!(
			events(),
			vec![Event::DepositReleased { channel: ch(1, 2), side: Sender, amount: 6 }]
		);
	});
}

/// Only the relay chain sends messages, and only governance forces a deposit.
#[test]
fn calls_require_root() {
	new_test_ext(|| {
		assert_noop!(
			HrmpPara::receive(RuntimeOrigin::signed(1), set_message(0, ch(1, 2), Some(10), None)),
			DispatchError::BadOrigin
		);
		assert_noop!(
			HrmpPara::force_set_deposit(RuntimeOrigin::signed(1), ch(1, 2), Sender, 10),
			DispatchError::BadOrigin
		);
	});
}

/// Forcing a side to what it holds succeeds and changes nothing.
#[test]
fn force_set_unchanged() {
	new_test_ext(|| {
		let force = |amount| {
			let origin = RuntimeOrigin::root();
			assert_storage_noop!(assert_eq!(
				HrmpPara::force_set_deposit(origin, ch(1, 2), Sender, amount),
				Ok(())
			));
		};
		force(0);
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		force(10);
	});
}

/// Governance holds or releases the difference, without telling the relay chain.
#[test]
fn force_set_raises_lowers_and_clears() {
	new_test_ext(|| {
		let force = |amount| {
			HrmpPara::force_set_deposit(RuntimeOrigin::root(), ch(1, 2), Recipient, amount)
		};
		assert_ok!(force(10));
		assert_eq!((on_hold(2), deposit(ch(1, 2), Recipient)), (10, Some(10)));
		assert_ok!(force(4));
		assert_eq!((on_hold(2), deposit(ch(1, 2), Recipient)), (4, Some(4)));
		assert_ok!(force(0));
		assert_eq!((on_hold(2), deposit(ch(1, 2), Recipient)), (0, None));
		assert!(sent().is_empty());
	});
}

/// A forced deposit the para cannot cover changes nothing.
#[test]
fn force_set_cannot_hold() {
	new_test_ext(|| {
		assert_noop!(
			HrmpPara::force_set_deposit(RuntimeOrigin::root(), ch(1, 2), Sender, ENDOWMENT + 1),
			Error::<Test>::InsufficientFunds
		);
	});
}

/// A migrated deposit sets what the side holds, from the para's free balance.
#[test]
fn receive_deposit_sets_the_amount() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		events();
		sent();

		let key = DepositKey { channel: ch(1, 2), side: Sender };
		assert_ok!(HrmpPara::receive_deposit(key, 30));

		assert_eq!(on_hold(1), 30);
		assert_eq!(Balances::free_balance(1), ENDOWMENT - 30);
		assert_eq!(deposit(ch(1, 2), Sender), Some(30));
		assert_eq!(
			events(),
			vec![
				Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 30 },
				Event::DepositMigrated { channel: ch(1, 2), side: Sender, amount: 30, held: 30 },
			]
		);
		assert!(sent().is_empty());
	});
}

/// A side is one deposit: a migrated deposit delivered twice is held once, and a lower one
/// releases the excess.
#[test]
fn receive_deposit_overrides_what_the_side_holds() {
	new_test_ext(|| {
		let key = DepositKey { channel: ch(1, 2), side: Sender };
		assert_ok!(HrmpPara::receive_deposit(key, 20));
		assert_ok!(HrmpPara::receive_deposit(key, 20));
		assert_eq!(on_hold(1), 20);
		assert_eq!(deposit(ch(1, 2), Sender), Some(20));

		assert_ok!(HrmpPara::receive_deposit(key, 15));
		assert_eq!(on_hold(1), 15);
		assert_eq!(deposit(ch(1, 2), Sender), Some(15));
		assert_eq!(
			events(),
			vec![
				Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 20 },
				Event::DepositMigrated { channel: ch(1, 2), side: Sender, amount: 20, held: 20 },
				Event::DepositMigrated { channel: ch(1, 2), side: Sender, amount: 20, held: 20 },
				Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 15 },
				Event::DepositMigrated { channel: ch(1, 2), side: Sender, amount: 15, held: 15 },
			]
		);
	});
}

/// A migrated deposit the para cannot cover whole is held as far as its existential deposit;
/// a para with nothing holds nothing, and neither is an error.
#[test]
fn receive_deposit_holds_what_the_para_can_cover() {
	new_test_ext(|| {
		let key = DepositKey { channel: ch(1, 2), side: Sender };
		assert_ok!(HrmpPara::receive_deposit(key, ENDOWMENT + 1));
		assert_eq!(on_hold(1), ENDOWMENT - 1);
		assert_eq!(Balances::free_balance(1), 1);
		assert_eq!(deposit(ch(1, 2), Sender), Some(ENDOWMENT - 1));
		events();

		let key = DepositKey { channel: ch(1, 4), side: Recipient };
		assert_ok!(HrmpPara::receive_deposit(key, 100));
		assert_eq!(on_hold(4), 0);
		assert_eq!(deposit(ch(1, 4), Recipient), None);
		assert_eq!(
			events(),
			vec![Event::DepositMigrated {
				channel: ch(1, 4),
				side: Recipient,
				amount: 100,
				held: 0
			}]
		);
	});
}

/// A migration is charged the benchmarked weight for a deposit it hands over.
#[test]
fn receive_deposit_weight_is_the_benchmarked_one() {
	assert_eq!(
		<HrmpPara as ReceiveMigratedDeposits>::weight_of_receive_deposit(),
		<() as crate::WeightInfo>::receive_deposit()
	);
}

/// Nothing is held for a migrated deposit of nothing.
#[test]
fn receive_deposit_zero_holds_nothing() {
	new_test_ext(|| {
		let key = DepositKey { channel: ch(1, 2), side: Recipient };
		assert_ok!(HrmpPara::receive_deposit(key, 0));
		assert_eq!(on_hold(2), 0);
		assert_eq!(deposit(ch(1, 2), Recipient), None);
		assert_eq!(
			events(),
			vec![Event::DepositMigrated { channel: ch(1, 2), side: Recipient, amount: 0, held: 0 }]
		);
	});
}

/// `try_state` finds a side recorded as holding nothing.
#[test]
fn try_state_detects_zero_entry() {
	new_test_ext(|| {
		Deposits::<Test>::insert((ch(1, 2), Sender), 0);
		assert!(HrmpPara::do_try_state().is_err());
		Deposits::<Test>::remove((ch(1, 2), Sender));
	});
}

/// `try_state` finds a para whose hold differs from its recorded deposits.
#[test]
fn try_state_detects_hold_mismatch() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		Deposits::<Test>::insert((ch(3, 1), Recipient), 5);
		assert!(HrmpPara::do_try_state().is_err());
		Deposits::<Test>::remove((ch(3, 1), Recipient));
	});
}
