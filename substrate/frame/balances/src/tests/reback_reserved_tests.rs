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

//! Tests for [`Pallet::reserved_shortfall`], the `try_state` check built on it, and the
//! [`RebackReserved`] migration that repairs accounts where it is not zero.

use super::*;
use crate::{
	migration::{reback_reserved, RebackOutcome, RebackReserved},
	Event, Holds,
};
use frame_support::{
	hypothetically,
	traits::{
		fungible::{InspectHold, MutateFreeze, MutateHold},
		Currency, NamedReservableCurrency, OnRuntimeUpgrade, ReservableCurrency,
	},
};

parameter_types! {
	pub static Listed: Vec<AccountId> = vec![];
	pub static UntilSpecVersion: Option<u32> = None;
}

/// Give `who` a free balance of `free` and a hold of `held`.
fn held_account(who: AccountId, free: Balance, held: Balance) {
	let _ = Balances::deposit_creating(&who, free + held);
	assert_ok!(Balances::hold(&TestId::Foo, &who, held));
}

/// Move `amount` of `who`'s reserved balance to its free balance, leaving its holds and named
/// reserves recorded in full. This is what an `unreserve` of more than the unnamed reserve used to
/// do.
fn under_back(who: AccountId, amount: Balance) {
	assert_ok!(Balances::mutate_account(&who, false, |a| {
		a.reserved -= amount;
		a.free += amount;
	}));
}

fn free_reserved_frozen(who: AccountId) -> (Balance, Balance, Balance) {
	let data = get_test_account_data(who);
	(data.free, data.reserved, data.frozen)
}

fn run_migration() {
	#[cfg(feature = "try-runtime")]
	assert_ok!(RebackReserved::<Test, Listed, UntilSpecVersion>::try_on_runtime_upgrade(true));
	#[cfg(not(feature = "try-runtime"))]
	RebackReserved::<Test, Listed, UntilSpecVersion>::on_runtime_upgrade();
}

fn set_on_chain_spec_version(spec_version: u32) {
	frame_system::LastRuntimeUpgrade::<Test>::put(frame_system::LastRuntimeUpgradeInfo {
		spec_version: spec_version.into(),
		spec_name: "test".into(),
	});
}

#[test]
fn reserved_shortfall_works() {
	ExtBuilder::default().build_and_execute_with(|| {
		// given an account with a hold of 10, a named reserve of 20 and an unnamed reserve of 5.
		held_account(1, 90, 10);
		assert_ok!(Balances::reserve_named(&TestId::Bar, &1, 20));
		assert_ok!(Balances::reserve(&1, 5));
		assert_eq!(free_reserved_frozen(1), (65, 35, 0));
		// and an account with a named reserve of 20 only, so without a `Holds` entry.
		let _ = Balances::deposit_creating(&2, 100);
		assert_ok!(Balances::reserve_named(&TestId::Bar, &2, 20));
		assert!(!Holds::<Test>::contains_key(2));

		// then neither is short.
		assert_eq!(Balances::reserved_shortfall(&1), 0);
		assert_eq!(Balances::reserved_shortfall(&2), 0);
		assert_ok!(Balances::do_try_state(1));

		hypothetically!({
			// when the first account loses its unnamed reserve.
			under_back(1, 5);

			// then it is still not short.
			assert_eq!(Balances::reserved_shortfall(&1), 0);
			assert_ok!(Balances::do_try_state(1));

			// when both lose some of what their hold and named reserve claim.
			under_back(1, 12);
			under_back(2, 3);

			// then both are short, and `try_state` fails.
			assert_eq!(Balances::reserved_shortfall(&1), 12);
			assert_eq!(Balances::reserved_shortfall(&2), 3);
			assert_eq!(Balances::under_backed_accounts(), vec![(1, 12), (2, 3)]);
			assert_eq!(
				Balances::do_try_state(1),
				Err(DispatchError::Other(
					"Found accounts whose reserved balance does not back all holds and named \
					 reserves"
				))
			);
		});
	});
}

#[test]
fn reback_reserved_moves_shortfall_from_free_to_reserved() {
	ExtBuilder::default().build_and_execute_with(|| {
		// given an account with a hold of 10, of which 4 is not backed, and a freeze larger than
		// its free balance will be.
		held_account(1, 90, 10);
		under_back(1, 4);
		assert_ok!(Balances::set_freeze(&TestId::Foo, &1, 95));
		assert_eq!(free_reserved_frozen(1), (94, 6, 95));
		// and an account with a hold of 10 that is not backed at all.
		held_account(2, 90, 10);
		under_back(2, 10);
		assert_eq!(free_reserved_frozen(2), (100, 0, 0));

		let consumers = (System::consumers(&1), System::consumers(&2));
		let issuance = Balances::total_issuance();
		let _ = events();

		// when
		assert_eq!(reback_reserved::<Test, ()>(&1), RebackOutcome::Repaired(4));
		assert_eq!(reback_reserved::<Test, ()>(&2), RebackOutcome::Repaired(10));

		// then the shortfall moved from free to reserved, and the freeze did not stop it.
		assert_eq!(free_reserved_frozen(1), (90, 10, 95));
		assert_eq!(free_reserved_frozen(2), (90, 10, 0));
		assert_eq!(Balances::reserved_shortfall(&1), 0);
		assert_eq!(Balances::reserved_shortfall(&2), 0);
		assert_eq!(Balances::balance_on_hold(&TestId::Foo, &1), 10);
		assert_eq!(Balances::balance_on_hold(&TestId::Foo, &2), 10);
		// the second account gets back the consumer reference that a reserved balance needs.
		assert_eq!((System::consumers(&1), System::consumers(&2)), (consumers.0, consumers.1 + 1));
		assert_eq!(Balances::total_issuance(), issuance);
		assert_eq!(
			events(),
			vec![
				RuntimeEvent::Balances(Event::Reserved { who: 1, amount: 4 }),
				RuntimeEvent::Balances(Event::Reserved { who: 2, amount: 10 }),
			]
		);

		// when it runs again, then nothing changes.
		assert_storage_noop!({
			assert_eq!(reback_reserved::<Test, ()>(&1), RebackOutcome::AlreadyValid);
			assert_eq!(reback_reserved::<Test, ()>(&2), RebackOutcome::AlreadyValid);
		});
	});
}

#[test]
fn reback_reserved_skips_account_that_cannot_cover_shortfall() {
	ExtBuilder::default().existential_deposit(5).build_and_execute_with(|| {
		// given an account with a hold of 10 that is not backed at all, and that spent the
		// released balance down to one less than the shortfall plus the existential deposit.
		held_account(1, 90, 10);
		under_back(1, 10);
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(1), 2, 86));
		assert_eq!(free_reserved_frozen(1), (14, 0, 0));

		// when, then it is left as it is.
		assert_storage_noop!(assert_eq!(
			reback_reserved::<Test, ()>(&1),
			RebackOutcome::Skipped(10)
		));

		// when it receives one more unit.
		let _ = Balances::deposit_creating(&1, 1);

		// then it is repaired, and keeps exactly the existential deposit free.
		assert_eq!(reback_reserved::<Test, ()>(&1), RebackOutcome::Repaired(10));
		assert_eq!(free_reserved_frozen(1), (5, 10, 0));
	});
}

#[test]
fn reback_reserved_migration_works() {
	ExtBuilder::default().auto_try_state(false).build_and_execute_with(|| {
		// given listed accounts with a hold of 10 each: one partly backed, one backed, one not
		// backed at all, and one not backed at all that spent too much to cover it.
		held_account(1, 90, 10);
		under_back(1, 4);
		held_account(2, 90, 10);
		held_account(3, 90, 10);
		under_back(3, 10);
		held_account(4, 90, 10);
		under_back(4, 10);
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(4), 5, 91));
		Listed::set(vec![1, 2, 3, 4]);

		// when
		run_migration();

		// then the first and third are repaired, the others are left as they are.
		assert_eq!(free_reserved_frozen(1), (90, 10, 0));
		assert_eq!(free_reserved_frozen(2), (90, 10, 0));
		assert_eq!(free_reserved_frozen(3), (90, 10, 0));
		assert_eq!(free_reserved_frozen(4), (9, 0, 0));

		// and `try_state` still reports the one it skipped.
		assert_eq!(Balances::under_backed_accounts(), vec![(4, 10)]);
		assert!(Balances::do_try_state(1).is_err());

		// when it runs again, then nothing changes.
		assert_storage_noop!(run_migration());
	});
}
