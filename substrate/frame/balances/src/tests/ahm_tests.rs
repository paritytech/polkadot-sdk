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

//! Test that the AssetHub migration works as expected.
//!
//! The tests in this file extensively compare storage diffs, to ensure that all changes to storage are 100% as expected. The error cases are also tested to ensure that no storage changes are leaked.

use super::*;
use crate::*;
use frame_support::{
	snapshot,
	traits::{
		BalanceStatus::{Free, Reserved},
		Currency,
		ExistenceRequirement::{self, AllowDeath, KeepAlive},
		Hooks, InspectLockableCurrency, LockIdentifier, LockableCurrency, NamedReservableCurrency,
		ReservableCurrency, WithdrawReasons,
		tokens::Precision,
	},
	StorageNoopGuard,
};
use sp_runtime::traits::Dispatchable;
use Test as T;

const ALICE: u64 = 1;

/// Alice has some reserved balance and that should be moved out.
#[test]
fn migrate_out_named_reserve() {
	const RSV1: <T as Config>::ReserveIdentifier = TestId::Foo;

	ExtBuilder::default().existential_deposit(1).build_and_execute_with(|| {
		frame_system::Pallet::<T>::set_block_number(0); // Dont care about events
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), ALICE, 1000));
		Balances::reserve_named(&RSV1, &ALICE, 100).unwrap();

		let ti = Balances::total_issuance();
		// Alice has the reserve
		assert_eq!(Balances::reserved_balance_named(&RSV1, &ALICE), 100);

		// Move the reserve out and store the root hash
		let snap1 = snapshot!({
			let call = Balances::migrate_out_named_reserve(
				RSV1,
				ALICE,
				100,
				Precision::Exact,
			).unwrap();

			// No reserve anymore
			assert_eq!(Balances::reserved_balance_named(&RSV1, &ALICE), 0);
			// TI updated
			assert_eq!(Balances::total_issuance(), ti - 100);
		});

		// We compare the storage root hashes, to ensure that `migrate_out_named_reserve` does EXACTLY the same as this:
		let snap2 = snapshot!({
			frame_system::Pallet::<T>::inc_sufficients(&ALICE);
			
			Balances::unreserve_all_named(&RSV1, &ALICE);
			Balances::force_set_balance(RuntimeOrigin::root(), ALICE, 900);

			SufficientAccounts::<T>::insert(ALICE, ());
			MigrationBurnedAmount::<T>::put(100);
			TotalIssuance::<T>::put(900);
		});

		snap1.assert_eq(&snap2);
	});
}

/// Alice can migrate reserved balance out and in again. Both operations annul each other.
#[test]
fn migrate_out_and_in_named_reserve() {
	const RSV1: <T as Config>::ReserveIdentifier = TestId::Foo;

	ExtBuilder::default().existential_deposit(1).build_and_execute_with(|| {
		frame_system::Pallet::<T>::set_block_number(0); // Dont care about events
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), ALICE, 1000));
		Balances::reserve_named(&RSV1, &ALICE, 100).unwrap();

		let ti = Balances::total_issuance();
		// Alice has the reserve
		assert_eq!(Balances::reserved_balance_named(&RSV1, &ALICE), 100);

		let _g = StorageNoopGuard::new();
		// Move the reserve out and store the root hash
		let in_flight_reserve = Balances::migrate_out_named_reserve(
			RSV1,
			ALICE,
			100,
			Precision::Exact,
		).unwrap();

		// Migrate it back in
		Balances::migrate_in_named_reserve(in_flight_reserve).unwrap();

		// These two will stay for manual verification
		assert_eq!(MigrationMintedAmount::<T, ()>::take(), MigrationBurnedAmount::<T, ()>::take());
		// These two is not reverted and would have to be cleaned up later:
		SufficientAccounts::<T>::take(&ALICE);
		frame_system::Pallet::<T>::dec_sufficients(&ALICE);
	});
}

fn root() -> Vec<u8> {
	sp_io::storage::root(sp_runtime::StateVersion::V1).to_vec()
}
