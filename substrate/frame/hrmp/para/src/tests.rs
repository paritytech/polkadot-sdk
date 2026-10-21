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

use crate::{
	mock::{new_test_ext, Balances, HrmpPara, RuntimeOrigin, System, Test},
	Call, Deposits, Event, HoldReason, Pallet, MIGRATE_DEPOSITS_CALL_INDEX, RECEIVE_CALL_INDEX,
};
use frame_support::{
	assert_noop, assert_ok,
	traits::{
		fungible::{Inspect, InspectHold, Mutate},
		GetCallIndex,
	},
	BoundedVec,
};
use hrmp_primitives::{ChannelId, DepositSide, MessageToPara, MessageToParaV1, MigratedDeposit};
use sp_runtime::DispatchError;

const CHANNEL: ChannelId = ChannelId { sender: 2000, recipient: 2001 };

fn migrated(side: DepositSide, amount: u128) -> MigratedDeposit {
	MigratedDeposit { channel: CHANNEL, side, amount }
}

fn migrate(deposits: Vec<MigratedDeposit>) {
	assert_ok!(HrmpPara::migrate_deposits(
		RuntimeOrigin::root(),
		BoundedVec::truncate_from(deposits)
	));
}

fn on_hold(who: u64) -> u128 {
	Balances::balance_on_hold(&HoldReason::ChannelDeposit.into(), &who)
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

/// The relay chain encodes `migrate_deposits` with [`MIGRATE_DEPOSITS_CALL_INDEX`].
#[test]
fn migrate_deposits_call_index_ok() {
	let call = Call::<Test>::migrate_deposits { deposits: Default::default() };
	assert_eq!(call.get_call_index(), MIGRATE_DEPOSITS_CALL_INDEX);
}

/// A funded account holds the whole migrated deposit, minted here.
#[test]
fn migrate_holds_all_on_funded_account() {
	new_test_ext().execute_with(|| {
		assert_ok!(Balances::mint_into(&2000, 10));
		migrate(vec![migrated(DepositSide::Sender, 100)]);

		assert_eq!(on_hold(2000), 100);
		assert_eq!(Balances::balance(&2000), 10);
		assert_eq!(Balances::total_issuance(), 110);
		assert_eq!(Deposits::<Test>::get((CHANNEL, DepositSide::Sender)), Some(100));
		System::assert_last_event(
			Event::DepositMigrated {
				channel: CHANNEL,
				side: DepositSide::Sender,
				amount: 100,
				held: 100,
			}
			.into(),
		);
		assert_ok!(Pallet::<Test>::do_try_state());
	});
}

/// A new account keeps its existential deposit free: it holds less, nothing more is minted.
#[test]
fn migrate_holds_less_on_new_account() {
	new_test_ext().execute_with(|| {
		migrate(vec![migrated(DepositSide::Recipient, 100)]);

		assert_eq!(on_hold(2001), 90);
		assert_eq!(Balances::balance(&2001), 10);
		assert_eq!(Balances::total_issuance(), 100);
		assert_eq!(Deposits::<Test>::get((CHANNEL, DepositSide::Recipient)), Some(90));
		System::assert_last_event(
			Event::DepositMigrated {
				channel: CHANNEL,
				side: DepositSide::Recipient,
				amount: 100,
				held: 90,
			}
			.into(),
		);
		assert_ok!(Pallet::<Test>::do_try_state());
	});
}

/// A deposit too small to create its account is not minted, and the others still are.
#[test]
fn migrate_skips_what_cannot_be_minted() {
	new_test_ext().execute_with(|| {
		migrate(vec![migrated(DepositSide::Sender, 5), migrated(DepositSide::Recipient, 100)]);

		assert_eq!(Balances::balance(&2000), 0);
		assert_eq!(Deposits::<Test>::get((CHANNEL, DepositSide::Sender)), None);
		System::assert_has_event(
			Event::DepositNotMigrated { channel: CHANNEL, side: DepositSide::Sender, amount: 5 }
				.into(),
		);
		assert_eq!(on_hold(2001), 90);
		assert_ok!(Pallet::<Test>::do_try_state());
	});
}

/// Only Root migrates deposits.
#[test]
fn migrate_needs_root() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			HrmpPara::migrate_deposits(
				RuntimeOrigin::signed(1),
				BoundedVec::truncate_from(vec![migrated(DepositSide::Sender, 100)])
			),
			DispatchError::BadOrigin
		);
	});
}
