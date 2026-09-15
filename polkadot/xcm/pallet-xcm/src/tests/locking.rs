// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Tests for locking fungibles locally, which is backed by a `fungible` freeze.

#![cfg(test)]

use crate::{
	migration::MigrateLocksToFreezes, mock::*, pallet::LockedFungibles, FreezeReason, XCM_LOCK_ID,
};
use bounded_collections::BoundedVec;
use frame_support::{
	assert_ok,
	migrations::SteppedMigration,
	traits::{
		fungible::InspectFreeze, Get, InspectLockableCurrency, LockableCurrency, WithdrawReasons,
	},
	weights::{RuntimeDbWeight, WeightMeter},
};
use xcm::prelude::*;
use xcm_executor::traits::{AssetLock, Enact, LockError};

const ALICE: AccountId = AccountId::new([0u8; 32]);
const BOB: AccountId = AccountId::new([1u8; 32]);
const INITIAL_BALANCE: Balance = 1000;

/// A local account as the mock's `SovereignAccountOf` understands it.
fn account_location(who: &AccountId) -> Location {
	Junction::AccountId32 { network: None, id: who.clone().into() }.into()
}

/// An asset the mock's `CurrencyMatcher` matches.
fn asset(amount: Balance) -> Asset {
	(Location::here(), amount).into()
}

fn frozen(who: &AccountId) -> Balance {
	<Balances as InspectFreeze<AccountId>>::balance_frozen(&FreezeReason::AssetLock.into(), who)
}

fn legacy_locked(who: &AccountId) -> Balance {
	<Balances as InspectLockableCurrency<AccountId>>::balance_locked(XCM_LOCK_ID, who)
}

/// Pre-migration state: a `LockedFungibles` entry backed by a legacy lock.
fn set_legacy_lock(who: &AccountId, amount: Balance, unlocker: Location) {
	LockedFungibles::<Test>::insert(
		who,
		BoundedVec::truncate_from(vec![(amount, VersionedLocation::from(unlocker))]),
	);
	<Balances as LockableCurrency<AccountId>>::set_lock(
		XCM_LOCK_ID,
		who,
		amount,
		WithdrawReasons::all(),
	);
}

fn run_migration() {
	// One item per step, to exercise the cursor.
	let db_weight: RuntimeDbWeight = <Test as frame_system::Config>::DbWeight::get();
	let limit = db_weight.reads_writes(3, 2);
	let mut cursor = None;
	loop {
		let mut meter = WeightMeter::with_limit(limit);
		match MigrateLocksToFreezes::<Test>::step(cursor, &mut meter) {
			Ok(None) => break,
			Ok(Some(next)) => cursor = Some(next),
			Err(error) => panic!("migration failed: {error:?}"),
		}
	}
}

#[test]
fn locking_freezes_the_locked_amount() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		let unlocker = Location::parent();

		let ticket = <XcmPallet as AssetLock>::prepare_lock(
			unlocker.clone(),
			asset(100),
			account_location(&ALICE),
		)
		.unwrap();
		assert_eq!(frozen(&ALICE), 0, "only applied on `enact`");

		assert_ok!(ticket.enact());

		assert_eq!(frozen(&ALICE), 100);
		assert_eq!(legacy_locked(&ALICE), 0, "no legacy lock must be created");
		assert_eq!(
			LockedFungibles::<Test>::get(ALICE).unwrap().into_inner(),
			vec![(100, unlocker.into())],
		);
		assert_ok!(XcmPallet::do_try_state());
	});
}

#[test]
fn freeze_covers_the_largest_of_several_lockers() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		for (unlocker, amount) in
			[(Location::parent(), 100), (Location::new(1, [Parachain(1)]), 250)]
		{
			let ticket = <XcmPallet as AssetLock>::prepare_lock(
				unlocker,
				asset(amount),
				account_location(&ALICE),
			)
			.unwrap();
			assert_ok!(ticket.enact());
		}

		// One freeze of the largest amount covers both lockers.
		assert_eq!(frozen(&ALICE), 250);
		assert_eq!(LockedFungibles::<Test>::get(ALICE).unwrap().len(), 2);
		assert_ok!(XcmPallet::do_try_state());
	});
}

#[test]
fn unlocking_lowers_and_finally_removes_the_freeze() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		let unlocker = Location::parent();
		let ticket = <XcmPallet as AssetLock>::prepare_lock(
			unlocker.clone(),
			asset(100),
			account_location(&ALICE),
		)
		.unwrap();
		assert_ok!(ticket.enact());

		let ticket = <XcmPallet as AssetLock>::prepare_unlock(
			unlocker.clone(),
			asset(40),
			account_location(&ALICE),
		)
		.unwrap();
		assert_ok!(ticket.enact());
		assert_eq!(frozen(&ALICE), 60);

		let ticket =
			<XcmPallet as AssetLock>::prepare_unlock(unlocker, asset(60), account_location(&ALICE))
				.unwrap();
		assert_ok!(ticket.enact());

		// Gone, not left behind with a zero amount.
		assert_eq!(frozen(&ALICE), 0);
		assert!(LockedFungibles::<Test>::get(ALICE).unwrap().is_empty());
		assert_ok!(XcmPallet::do_try_state());
	});
}

#[test]
fn cannot_lock_more_than_owned() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		assert!(matches!(
			<XcmPallet as AssetLock>::prepare_lock(
				Location::parent(),
				asset(INITIAL_BALANCE + 1),
				account_location(&ALICE),
			)
			.map(|_| ()),
			Err(LockError::AssetNotOwned),
		));
		assert_eq!(frozen(&ALICE), 0);
	});
}

#[test]
fn locking_again_migrates_a_legacy_lock() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		let unlocker = Location::parent();
		set_legacy_lock(&ALICE, 100, unlocker.clone());
		// Unmigrated is a valid state for the invariant.
		assert_ok!(XcmPallet::do_try_state());

		let ticket =
			<XcmPallet as AssetLock>::prepare_lock(unlocker, asset(250), account_location(&ALICE))
				.unwrap();
		assert_ok!(ticket.enact());

		assert_eq!(frozen(&ALICE), 250);
		assert_eq!(legacy_locked(&ALICE), 0, "the legacy lock must be released");
		assert_ok!(XcmPallet::do_try_state());
	});
}

#[test]
fn unlocking_migrates_a_legacy_lock() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		let unlocker = Location::parent();
		set_legacy_lock(&ALICE, 100, unlocker.clone());

		let ticket =
			<XcmPallet as AssetLock>::prepare_unlock(unlocker, asset(40), account_location(&ALICE))
				.unwrap();
		assert_ok!(ticket.enact());

		assert_eq!(frozen(&ALICE), 60);
		assert_eq!(legacy_locked(&ALICE), 0);
		assert_ok!(XcmPallet::do_try_state());
	});
}

#[test]
fn multi_block_migration_converts_legacy_locks() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE), (BOB, INITIAL_BALANCE)])
		.execute_with(|| {
			set_legacy_lock(&ALICE, 100, Location::parent());
			set_legacy_lock(&BOB, 250, Location::new(1, [Parachain(1)]));

			#[cfg(feature = "try-runtime")]
			let state = MigrateLocksToFreezes::<Test>::pre_upgrade().unwrap();

			run_migration();

			#[cfg(feature = "try-runtime")]
			MigrateLocksToFreezes::<Test>::post_upgrade(state).unwrap();

			assert_eq!(frozen(&ALICE), 100);
			assert_eq!(frozen(&BOB), 250);
			assert_eq!(legacy_locked(&ALICE), 0);
			assert_eq!(legacy_locked(&BOB), 0);
			// The pallet's own storage is untouched.
			assert_eq!(LockedFungibles::<Test>::iter().count(), 2);
			assert_ok!(XcmPallet::do_try_state());
		});
}

#[test]
fn multi_block_migration_is_a_noop_without_locks() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		let _guard = frame_support::StorageNoopGuard::new();
		run_migration();
	});
}

#[test]
fn try_state_catches_an_unbacked_lock() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		// Neither a freeze nor a legacy lock behind it.
		LockedFungibles::<Test>::insert(
			ALICE,
			BoundedVec::truncate_from(vec![(100u128, VersionedLocation::from(Location::parent()))]),
		);
		assert!(XcmPallet::do_try_state().is_err());
	});
}
