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

#![cfg(test)]

use crate::{
	system::AccountInfo,
	tests::{ensure_ti_valid, Balances, ExtBuilder, System, Test, TestId, UseSystem},
	AccountData, ExtraFlags, TotalIssuance,
};
use frame_support::{
	assert_noop, assert_ok, hypothetically,
	traits::{
		fungible::{Mutate, MutateHold},
		tokens::Precision,
	},
};
use sp_runtime::DispatchError;

/// There are some accounts that have one consumer ref too few. These accounts are at risk of losing
/// their held (reserved) balance. They do not just lose it - it is also not accounted for in the
/// Total Issuance. Here we test the case that the account does not reap in such a case, but gets
/// one consumer ref for its reserved balance.
#[test]
fn regression_historic_acc_does_not_evaporate_reserve() {
	ExtBuilder::default().build_and_execute_with(|| {
		UseSystem::set(true);
		let (alice, bob) = (0, 1);
		// Alice is in a bad state with consumer == 0 && reserved > 0:
		Balances::set_balance(&alice, 100);
		TotalIssuance::<Test>::put(100);
		ensure_ti_valid();

		assert_ok!(Balances::hold(&TestId::Foo, &alice, 10));
		// This is the issue of the account:
		System::dec_consumers(&alice);

		assert_eq!(
			System::account(&alice),
			AccountInfo {
				data: AccountData {
					free: 90,
					reserved: 10,
					frozen: 0,
					flags: ExtraFlags(1u128 << 127),
				},
				nonce: 0,
				consumers: 0, // should be 1 on a good acc
				providers: 1,
				sufficients: 0,
			}
		);

		ensure_ti_valid();

		// Reaping the account is prevented by the new logic:
		assert_noop!(
			Balances::transfer_allow_death(Some(alice).into(), bob, 90),
			DispatchError::ConsumerRemaining
		);
		assert_noop!(
			Balances::transfer_all(Some(alice).into(), bob, false),
			DispatchError::ConsumerRemaining
		);

		// normal transfers still work:
		hypothetically!({
			assert_ok!(Balances::transfer_keep_alive(Some(alice).into(), bob, 40));
			// Alice got back her consumer ref:
			assert_eq!(System::consumers(&alice), 1);
			ensure_ti_valid();
		});
		hypothetically!({
			assert_ok!(Balances::transfer_all(Some(alice).into(), bob, true));
			// Alice got back her consumer ref:
			assert_eq!(System::consumers(&alice), 1);
			ensure_ti_valid();
		});

		// un-reserving all does not add a consumer ref:
		hypothetically!({
			assert_ok!(Balances::release(&TestId::Foo, &alice, 10, Precision::Exact));
			assert_eq!(System::consumers(&alice), 0);
			assert_ok!(Balances::transfer_keep_alive(Some(alice).into(), bob, 40));
			assert_eq!(System::consumers(&alice), 0);
			ensure_ti_valid();
		});
		// un-reserving some does add a consumer ref:
		hypothetically!({
			assert_ok!(Balances::release(&TestId::Foo, &alice, 5, Precision::Exact));
			assert_eq!(System::consumers(&alice), 1);
			assert_ok!(Balances::transfer_keep_alive(Some(alice).into(), bob, 40));
			assert_eq!(System::consumers(&alice), 1);
			ensure_ti_valid();
		});
	});
}

#[cfg(feature = "try-runtime")]
#[test]
fn try_state_works() {
	use crate::{Config, Freezes, Holds};
	use frame_support::{
		storage,
		traits::{Get, Hooks, VariantCount},
	};

	ExtBuilder::default().build_and_execute_with(|| {
		storage::unhashed::put(
			&Holds::<Test>::hashed_key_for(1),
			&vec![0u8; <Test as Config>::RuntimeHoldReason::VARIANT_COUNT as usize + 1],
		);

		assert!(format!("{:?}", Balances::try_state(0).unwrap_err())
			.contains("Found `Hold` with too many elements"));
	});

	ExtBuilder::default().build_and_execute_with(|| {
		let max_freezes: u32 = <Test as Config>::MaxFreezes::get();

		storage::unhashed::put(
			&Freezes::<Test>::hashed_key_for(1),
			&vec![0u8; max_freezes as usize + 1],
		);

		assert!(format!("{:?}", Balances::try_state(0).unwrap_err())
			.contains("Found `Freeze` with too many elements"));
	});
}
