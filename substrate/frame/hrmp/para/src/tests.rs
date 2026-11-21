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

use crate::{mock::*, Call, Deposit, Deposits, Error, Event, HoldReason, RECEIVE_CALL_INDEX};
use frame_support::{
	assert_noop, assert_ok, assert_storage_noop,
	traits::{
		fungible::{self, InspectHold, MutateHold},
		GetCallIndex, UnfilteredDispatchable,
	},
};
use hrmp_primitives::{
	Balance, ChannelId, DepositError, DepositKey, DepositSide, MessageToPara, MessageToParaV1,
	MessageToRelay, MessageToRelayV1, ParaId, ReceiveMigratedDeposits,
};
use sp_runtime::{DispatchError, DispatchResult, TokenError};

use DepositSide::{Recipient, Sender};

fn ch(sender: ParaId, recipient: ParaId) -> ChannelId {
	ChannelId { sender, recipient }
}

/// What `para` holds for channel deposits.
fn on_hold(para: ParaId) -> Balance {
	Balances::balance_on_hold(&HoldReason::ChannelDeposit.into(), &para.into())
}

/// The reason a migration holds the deposits it hands over for.
fn migration_reason() -> RuntimeHoldReason {
	migration::HoldReason::HrmpDeposit.into()
}

/// What `para` holds for the migration.
fn migration_hold(para: ParaId) -> Balance {
	Balances::balance_on_hold(&migration_reason(), &para.into())
}

/// Hands the deposit of `amount` for `side` of `channel` over, from the migration's hold.
fn receive_deposit(channel: ChannelId, side: DepositSide, amount: Balance) -> DispatchResult {
	HrmpPara::receive_deposit(DepositKey { channel, side }, amount, &migration_reason())
}

fn deposit(channel: ChannelId, side: DepositSide) -> Option<Balance> {
	Deposits::<Test>::get((channel, side)).map(|d| d.amount)
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
				Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 0 },
				Event::DepositSet { channel: ch(1, 2), side: Recipient, amount: 0 },
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

/// Re-anchoring the sovereign accounts does not move a deposit: it is still released to the
/// account it was held from.
#[test]
fn release_after_reanchor_goes_to_depositor() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		assert_eq!(Deposits::<Test>::get((ch(1, 2), Sender)).map(|d| d.depositor), Some(1));

		ANCHOR.set(1000);
		release(ch(1, 2), true, false);
		assert_eq!(on_hold(1), 0);
		assert_eq!(Balances::balance_on_hold(&HoldReason::ChannelDeposit.into(), &1001), 0);
		assert_eq!(deposit(ch(1, 2), Sender), None);
	});
}

/// After re-anchoring, a side that holds something keeps holding from the recorded account;
/// only a side that holds nothing uses the new sovereign account.
#[test]
fn set_after_reanchor_keeps_depositor() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));

		ANCHOR.set(1000);
		<Balances as fungible::Mutate<_>>::set_balance(&1002, ENDOWMENT);
		assert_ok!(set(1, ch(1, 2), Some(15), Some(20)));
		assert_eq!(on_hold(1), 15);
		assert_eq!(Deposits::<Test>::get((ch(1, 2), Sender)).map(|d| d.depositor), Some(1));
		// The recipient held nothing: its deposit comes from the re-anchored account.
		assert_eq!(on_hold(2), 0);
		assert_eq!(Balances::balance_on_hold(&HoldReason::ChannelDeposit.into(), &1002), 20);
		assert_eq!(Deposits::<Test>::get((ch(1, 2), Recipient)).map(|d| d.depositor), Some(1002));
	});
}

/// A migrated deposit moves from the migration's hold to the side, and the free balance is left
/// as it was.
#[test]
fn receive_deposit_moves_the_migration_hold() {
	new_test_ext(|| {
		assert_ok!(Balances::hold(&migration_reason(), &1, 50));
		let free = Balances::free_balance(1);

		assert_ok!(receive_deposit(ch(1, 2), Sender, 30));

		assert_eq!((on_hold(1), migration_hold(1)), (30, 20));
		assert_eq!(Balances::free_balance(1), free);
		assert_eq!(deposit(ch(1, 2), Sender), Some(30));
		assert_eq!(Deposits::<Test>::get((ch(1, 2), Sender)).map(|d| d.depositor), Some(1));
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

/// The whole balance but the existential deposit may be under the migration's hold.
#[test]
fn receive_deposit_with_only_the_existential_deposit_free() {
	new_test_ext(|| {
		assert_ok!(Balances::hold(&migration_reason(), &1, ENDOWMENT - 1));

		assert_ok!(receive_deposit(ch(1, 2), Sender, ENDOWMENT - 1));

		assert_eq!((on_hold(1), migration_hold(1)), (ENDOWMENT - 1, 0));
		assert_eq!(Balances::free_balance(1), 1);
	});
}

/// A side is one deposit: a migrated deposit delivered twice is held once, a higher one takes
/// what is missing from the migration's hold, and a lower one gives the excess back to it. The
/// free balance is left as it was throughout.
#[test]
fn receive_deposit_overrides_what_the_side_holds() {
	new_test_ext(|| {
		assert_ok!(Balances::hold(&migration_reason(), &1, 50));
		let free = Balances::free_balance(1);

		assert_ok!(receive_deposit(ch(1, 2), Sender, 20));
		assert_ok!(receive_deposit(ch(1, 2), Sender, 20));
		assert_eq!((on_hold(1), migration_hold(1)), (20, 30));

		assert_ok!(receive_deposit(ch(1, 2), Sender, 25));
		assert_eq!((on_hold(1), migration_hold(1)), (25, 25));

		assert_ok!(receive_deposit(ch(1, 2), Sender, 15));
		assert_eq!((on_hold(1), migration_hold(1)), (15, 35));
		assert_eq!(deposit(ch(1, 2), Sender), Some(15));

		assert_ok!(receive_deposit(ch(1, 2), Sender, 0));
		assert_eq!((on_hold(1), migration_hold(1)), (0, 50));
		assert_eq!(deposit(ch(1, 2), Sender), None);

		assert_eq!(Balances::free_balance(1), free);
		assert_eq!(
			events(),
			vec![
				Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 20 },
				Event::DepositMigrated { channel: ch(1, 2), side: Sender, amount: 20, held: 20 },
				Event::DepositMigrated { channel: ch(1, 2), side: Sender, amount: 20, held: 20 },
				Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 25 },
				Event::DepositMigrated { channel: ch(1, 2), side: Sender, amount: 25, held: 25 },
				Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 15 },
				Event::DepositMigrated { channel: ch(1, 2), side: Sender, amount: 15, held: 15 },
				Event::DepositSet { channel: ch(1, 2), side: Sender, amount: 0 },
				Event::DepositMigrated { channel: ch(1, 2), side: Sender, amount: 0, held: 0 },
			]
		);
	});
}

/// A migration's hold that cannot cover a migrated deposit whole is not expected.
#[test]
#[should_panic(expected = "hrmp-para: moved holds not as wanted")]
fn receive_deposit_with_a_short_migration_hold_is_defensive() {
	new_test_ext(|| {
		assert_ok!(Balances::hold(&migration_reason(), &1, 20));
		let _ = receive_deposit(ch(1, 2), Sender, 30);
	});
}

/// Nor is a para that the migration holds nothing for.
#[test]
#[should_panic(expected = "hrmp-para: moved holds not as wanted")]
fn receive_deposit_without_a_migration_hold_is_defensive() {
	new_test_ext(|| {
		let _ = receive_deposit(ch(2, 3), Recipient, 10);
	});
}

/// Nor is a para with no account at all.
#[test]
#[should_panic(expected = "hrmp-para: moved holds not as wanted")]
fn receive_deposit_without_an_account_is_defensive() {
	new_test_ext(|| {
		let _ = receive_deposit(ch(1, 4), Recipient, 10);
	});
}

/// After re-anchoring, a migrated deposit for a side that holds something moves on the recorded
/// depositor.
#[test]
fn receive_deposit_after_reanchor_keeps_depositor() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		assert_ok!(Balances::hold(&migration_reason(), &1, 50));
		let free = Balances::free_balance(1);

		ANCHOR.set(1000);
		assert_ok!(receive_deposit(ch(1, 2), Sender, 30));
		assert_eq!((on_hold(1), migration_hold(1)), (30, 30));
		assert_eq!(Balances::free_balance(1), free);
		assert_eq!(Balances::balance_on_hold(&HoldReason::ChannelDeposit.into(), &1001), 0);
		assert_eq!(Deposits::<Test>::get((ch(1, 2), Sender)).map(|d| d.depositor), Some(1));
	});
}

/// The deposits of a para's several channels come out of its one migration hold; lowering one
/// gives back to that hold and leaves the others as they are.
#[test]
fn receive_deposit_shares_the_migration_hold_across_channels() {
	new_test_ext(|| {
		assert_ok!(Balances::hold(&migration_reason(), &1, 60));

		assert_ok!(receive_deposit(ch(1, 2), Sender, 20));
		assert_ok!(receive_deposit(ch(1, 3), Sender, 20));
		assert_ok!(receive_deposit(ch(2, 1), Recipient, 20));
		assert_eq!((on_hold(1), migration_hold(1)), (60, 0));

		assert_ok!(receive_deposit(ch(1, 2), Sender, 5));
		assert_eq!((on_hold(1), migration_hold(1)), (45, 15));
		assert_eq!(deposit(ch(1, 2), Sender), Some(5));
		assert_eq!(deposit(ch(1, 3), Sender), Some(20));
		assert_eq!(deposit(ch(2, 1), Recipient), Some(20));
	});
}

/// The excess of a side the relay chain set goes under the migration's hold too, not to the
/// free balance.
#[test]
fn receive_deposit_lowers_a_side_the_relay_set() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(30), None));
		let free = Balances::free_balance(1);

		assert_ok!(receive_deposit(ch(1, 2), Sender, 10));
		assert_eq!((on_hold(1), migration_hold(1)), (10, 20));
		assert_eq!(Balances::free_balance(1), free);
		assert_eq!(deposit(ch(1, 2), Sender), Some(10));
	});
}

/// A migrated deposit that is released but cannot be held again writes nothing.
#[test]
fn receive_deposit_writes_nothing_if_it_fails() {
	new_test_ext(|| {
		assert_ok!(Balances::hold(&migration_reason(), &1, 30));
		// Not a state the currency gets into by itself: an account that the release provides
		// for anew, so that the hold has to leave it its existential deposit.
		frame_system::Account::<Test>::mutate(1, |account| {
			account.data.free = 0;
			account.providers = 0;
		});

		assert_noop!(receive_deposit(ch(1, 2), Sender, 30), TokenError::FundsUnavailable);
		assert_eq!((on_hold(1), migration_hold(1)), (0, 30));
	});
}

/// A migration is charged the benchmarked weight for a deposit it hands over.
#[test]
fn receive_deposit_weight_is_the_benchmarked_one() {
	assert_eq!(
		<HrmpPara as ReceiveMigratedDeposits<RuntimeHoldReason>>::weight_of_receive_deposit(),
		<() as crate::WeightInfo>::receive_deposit()
	);
}

/// Nothing is held for a migrated deposit of nothing.
#[test]
fn receive_deposit_zero_holds_nothing() {
	new_test_ext(|| {
		assert_ok!(Balances::hold(&migration_reason(), &2, 50));
		assert_ok!(receive_deposit(ch(1, 2), Recipient, 0));
		assert_eq!((on_hold(2), migration_hold(2)), (0, 50));
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
		Deposits::<Test>::insert((ch(1, 2), Sender), Deposit { amount: 0, depositor: 1 });
		assert!(HrmpPara::do_try_state().is_err());
		Deposits::<Test>::remove((ch(1, 2), Sender));
	});
}

/// `try_state` finds a para whose hold differs from its recorded deposits.
#[test]
fn try_state_detects_hold_mismatch() {
	new_test_ext(|| {
		assert_ok!(set(0, ch(1, 2), Some(10), None));
		Deposits::<Test>::insert((ch(3, 1), Recipient), Deposit { amount: 5, depositor: 1 });
		assert!(HrmpPara::do_try_state().is_err());
		Deposits::<Test>::remove((ch(3, 1), Recipient));
	});
}
