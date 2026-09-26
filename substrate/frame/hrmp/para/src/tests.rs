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

use crate::{mock::*, Deposits, Error, Event};
use frame_support::{assert_noop, assert_ok};
use hrmp_primitives::{
	DepositKey, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1,
	ReceiveMigratedDeposits,
};
use sp_runtime::DispatchError;

fn hold(key: DepositKey, amount: u128) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::Hold { key, amount })
}

fn release(key: DepositKey) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::Release { key, amount: None })
}

fn release_part(key: DepositKey, amount: u128) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::Release { key, amount: Some(amount) })
}

fn answer(key: DepositKey, held: bool) -> MessageToRelay {
	MessageToRelay::V1(MessageToRelayV1::HoldResult { key, held })
}

fn relay() -> RuntimeOrigin {
	RuntimeOrigin::signed(RELAY)
}

#[test]
fn a_hold_is_taken_from_the_payers_sovereign_and_answered() {
	new_test_ext().execute_with(|| {
		// WHEN the relay chain asks for both ends of a channel
		assert_ok!(HrmpPara::receive(relay(), hold(sender_key(), 100)));
		assert_ok!(HrmpPara::receive(relay(), hold(recipient_key(), 40)));

		// THEN each end pays its own, and each hold is answered.
		assert_eq!((on_hold(PARA_A), on_hold(PARA_B)), (100, 40));
		assert_eq!(Deposits::<Test>::get(sender_key()), Some(100));
		assert_eq!(Deposits::<Test>::get(recipient_key()), Some(40));
		assert_eq!(Sent::get(), vec![answer(sender_key(), true), answer(recipient_key(), true)]);
		assert_eq!(
			events(),
			vec![
				Event::DepositHeld { key: sender_key(), amount: 100 },
				Event::DepositHeld { key: recipient_key(), amount: 40 },
			]
		);
		assert_ok!(HrmpPara::do_try_state());
	});
}

#[test]
fn a_hold_the_payer_cannot_cover_is_refused_and_answered() {
	new_test_ext().execute_with(|| {
		// WHEN the relay chain asks for more than the sender has
		assert_ok!(HrmpPara::receive(relay(), hold(sender_key(), 5_000)));

		// THEN nothing is held, and the relay chain is told.
		assert_eq!(on_hold(PARA_A), 0);
		assert_eq!(Deposits::<Test>::get(sender_key()), None);
		assert_eq!(Sent::get(), vec![answer(sender_key(), false)]);
		assert_eq!(events(), vec![Event::DepositRefused { key: sender_key(), amount: 5_000 }]);
	});
}

#[test]
fn a_second_hold_for_the_same_key_adds_to_it() {
	new_test_ext().execute_with(|| {
		assert_ok!(HrmpPara::receive(relay(), hold(sender_key(), 100)));
		assert_ok!(HrmpPara::receive(relay(), hold(sender_key(), 20)));

		assert_eq!(Deposits::<Test>::get(sender_key()), Some(120));
		assert_eq!(on_hold(PARA_A), 120);
		assert_ok!(HrmpPara::do_try_state());
	});
}

#[test]
fn a_release_returns_everything_held_for_the_key() {
	new_test_ext().execute_with(|| {
		// GIVEN both ends are held.
		assert_ok!(HrmpPara::receive(relay(), hold(sender_key(), 100)));
		assert_ok!(HrmpPara::receive(relay(), hold(recipient_key(), 40)));
		let _ = events();

		// WHEN the sender's is released
		assert_ok!(HrmpPara::receive(relay(), release(sender_key())));

		// THEN only the sender's comes back, and nothing is answered.
		assert_eq!((on_hold(PARA_A), on_hold(PARA_B)), (0, 40));
		assert_eq!(Deposits::<Test>::get(sender_key()), None);
		assert_eq!(Sent::get().len(), 2);
		assert_eq!(events(), vec![Event::DepositReleased { key: sender_key(), amount: 100 }]);

		// AND releasing it again does nothing.
		assert_ok!(HrmpPara::receive(relay(), release(sender_key())));
		assert!(events().is_empty());
		assert_ok!(HrmpPara::do_try_state());
	});
}

#[test]
fn only_the_relay_chain_or_root_may_send() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			HrmpPara::receive(RuntimeOrigin::signed(ALICE), hold(sender_key(), 100)),
			DispatchError::BadOrigin
		);
		assert_ok!(HrmpPara::receive(RuntimeOrigin::root(), hold(sender_key(), 100)));
		assert_eq!(on_hold(PARA_A), 100);
	});
}

#[test]
fn a_hold_whose_answer_cannot_be_sent_is_undone() {
	new_test_ext().execute_with(|| {
		SendFails::set(true);

		assert_noop!(
			HrmpPara::receive(relay(), hold(sender_key(), 100)),
			Error::<Test>::SendFailed
		);
		assert_eq!(on_hold(PARA_A), 0);
	});
}

#[test]
fn root_can_force_a_release() {
	new_test_ext().execute_with(|| {
		assert_ok!(HrmpPara::receive(relay(), hold(sender_key(), 100)));
		let _ = events();

		assert_noop!(
			HrmpPara::force_release(RuntimeOrigin::signed(RELAY), sender_key()),
			DispatchError::BadOrigin
		);
		assert_ok!(HrmpPara::force_release(RuntimeOrigin::root(), sender_key()));

		assert_eq!(on_hold(PARA_A), 0);
		assert_eq!(events(), vec![Event::DepositForceReleased { key: sender_key(), amount: 100 }]);
		assert_noop!(
			HrmpPara::force_release(RuntimeOrigin::root(), sender_key()),
			Error::<Test>::NoSuchDeposit
		);
	});
}

#[test]
fn a_migrated_deposit_holds_what_the_payer_can_cover() {
	let poor = DepositKey {
		channel: hrmp_primitives::ChannelId { sender: PARA_POOR, recipient: PARA_A },
		side: hrmp_primitives::DepositSide::Sender,
	};

	new_test_ext().execute_with(|| {
		// WHEN a covered deposit migrates
		assert_ok!(HrmpPara::receive_deposit(sender_key(), 100));
		// THEN all of it is held.
		assert_eq!(Deposits::<Test>::get(sender_key()), Some(100));
		assert_eq!(on_hold(PARA_A), 100);

		// WHEN a para that holds 30 owes 100
		assert_ok!(HrmpPara::receive_deposit(poor, 100));
		// THEN what it can spend while staying alive is held, and the rest is reported.
		// Existential deposit is 1, so 29 of its 30 can be held.
		assert_eq!(Deposits::<Test>::get(poor), Some(29));
		assert_eq!(on_hold(PARA_POOR), 29);
		assert_eq!(
			events(),
			vec![
				Event::DepositMigrated { key: sender_key(), held: 100, missing: 0 },
				Event::DepositMigrated { key: poor, held: 29, missing: 71 },
			]
		);

		// AND releasing it returns only what was held.
		assert_ok!(HrmpPara::receive(relay(), release(poor)));
		assert_eq!(on_hold(PARA_POOR), 0);
		assert_ok!(HrmpPara::do_try_state());
	});
}

#[test]
fn a_migrated_deposit_the_payer_cannot_cover_at_all_records_nothing() {
	new_test_ext().execute_with(|| {
		let broke = DepositKey {
			channel: hrmp_primitives::ChannelId { sender: 2_999, recipient: PARA_A },
			side: hrmp_primitives::DepositSide::Sender,
		};

		assert_ok!(HrmpPara::receive_deposit(broke, 100));

		assert_eq!(Deposits::<Test>::get(broke), None);
		assert_eq!(events(), vec![Event::DepositMigrated { key: broke, held: 0, missing: 100 }]);
		assert_ok!(HrmpPara::do_try_state());
	});
}

#[test]
fn try_state_catches_a_record_that_disagrees_with_the_hold() {
	new_test_ext().execute_with(|| {
		// GIVEN a valid deposit.
		assert_ok!(HrmpPara::receive(relay(), hold(sender_key(), 100)));

		// WHEN its record is corrupted
		Deposits::<Test>::insert(sender_key(), 90);
		// THEN try-state notices.
		assert!(HrmpPara::do_try_state().is_err());

		// Restore.
		Deposits::<Test>::insert(sender_key(), 100);
		assert_ok!(HrmpPara::do_try_state());
	});
}

#[test]
fn a_partial_release_keeps_the_rest_held() {
	new_test_ext().execute_with(|| {
		// GIVEN a deposit of 100.
		assert_ok!(HrmpPara::receive(relay(), hold(sender_key(), 100)));
		let _ = events();

		// WHEN 30 of it is released
		assert_ok!(HrmpPara::receive(relay(), release_part(sender_key(), 30)));

		// THEN 70 stays held.
		assert_eq!(Deposits::<Test>::get(sender_key()), Some(70));
		assert_eq!(on_hold(PARA_A), 70);
		assert_eq!(events(), vec![Event::DepositReleased { key: sender_key(), amount: 30 }]);

		// AND releasing more than is held releases what is left.
		assert_ok!(HrmpPara::receive(relay(), release_part(sender_key(), 500)));
		assert_eq!(Deposits::<Test>::get(sender_key()), None);
		assert_eq!(on_hold(PARA_A), 0);
		assert_eq!(events(), vec![Event::DepositReleased { key: sender_key(), amount: 70 }]);
		assert_ok!(HrmpPara::do_try_state());
	});
}

#[test]
fn users_ask_the_relay_chain_for_its_signed_hrmp_calls() {
	new_test_ext().execute_with(|| {
		let system = hrmp_primitives::ChannelId { sender: 1_000, recipient: 1_001 };

		// Signed only.
		assert_noop!(
			HrmpPara::poke_channel_deposits(RuntimeOrigin::root(), PARA_A, PARA_B),
			DispatchError::BadOrigin
		);

		assert_ok!(HrmpPara::poke_channel_deposits(RuntimeOrigin::signed(ALICE), PARA_A, PARA_B));
		assert_ok!(HrmpPara::establish_system_channel(RuntimeOrigin::signed(ALICE), 1_000, 1_001));

		assert_eq!(
			Sent::get(),
			vec![
				MessageToRelay::V1(MessageToRelayV1::PokeChannelDeposits { channel: CHANNEL }),
				MessageToRelay::V1(MessageToRelayV1::EstablishSystemChannel { channel: system }),
			]
		);
		assert_eq!(
			events(),
			vec![
				Event::PokeRequested { channel: CHANNEL },
				Event::SystemChannelRequested { channel: system },
			]
		);

		// A request that cannot be sent fails.
		SendFails::set(true);
		assert_noop!(
			HrmpPara::poke_channel_deposits(RuntimeOrigin::signed(ALICE), PARA_A, PARA_B),
			Error::<Test>::SendFailed
		);
	});
}

#[test]
fn root_can_answer_a_hold_whose_answer_never_arrived() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			HrmpPara::force_answer(RuntimeOrigin::signed(ALICE), sender_key(), true),
			DispatchError::BadOrigin
		);

		assert_ok!(HrmpPara::force_answer(RuntimeOrigin::root(), sender_key(), false));

		assert_eq!(Sent::get(), vec![answer(sender_key(), false)]);
		assert_eq!(events(), vec![Event::AnswerForced { key: sender_key(), held: false }]);
	});
}
