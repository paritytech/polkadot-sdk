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

//! Tests for consumer limit behavior in balance locks.

use super::*;
use frame_support::traits::{
	fungible::{InspectFreeze, MutateFreeze},
	Currency, Get, LockIdentifier, LockableCurrency, WithdrawReasons,
};

const ID: LockIdentifier = *b"1       ";

#[test]
fn lock_behavior_when_consumer_limit_fully_exhausted() {
	ExtBuilder::default()
		.existential_deposit(1)
		.monied(true)
		.build()
		.execute_with(|| {
			// Account 1 starts with 100 balance
			Balances::make_free_balance_be(&1, 100);
			assert_eq!(System::providers(&1), 1);
			assert_eq!(System::consumers(&1), 0);

			// Fill up all 16 consumer refs.
			// Note: asset pallets prevents all the consumers to be filled and leaves one untouched.
			// But other operations in the runtime, notably `uniques::set_accept_ownership` might.
			let max_consumers: u32 = <Test as frame_system::Config>::MaxConsumers::get();
			for _ in 0..max_consumers {
				assert_ok!(System::inc_consumers(&1));
			}
			assert_eq!(System::consumers(&1), max_consumers);

			// We cannot manually increment consumers beyond the limit
			assert_noop!(System::inc_consumers(&1), DispatchError::TooManyConsumers);

			// Although without limits it would work
			frame_support::hypothetically!({
				assert_ok!(System::inc_consumers_without_limit(&1));
			});

			// Now try to set a lock - this will still work because we use
			// `inc_consumers_without_limit` in `update_lock`.
			Balances::set_lock(ID, &1, 20, WithdrawReasons::all());
			assert_eq!(Balances::locks(&1).len(), 1);
			assert_eq!(Balances::locks(&1)[0].amount, 20);

			// now this account has 1 more consumer reference for the lock
			assert_eq!(System::consumers(&1), max_consumers + 1);

			// frozen amount is also updated
			assert_eq!(get_test_account_data(1).frozen, 20);

			// And this account cannot transfer any funds out.
			assert_noop!(
				Balances::transfer_allow_death(frame_system::RawOrigin::Signed(1).into(), 2, 90,),
				DispatchError::Token(TokenError::Frozen)
			);
		});
}

#[test]
fn freeze_behavior_when_consumer_limit_fully_exhausted() {
	ExtBuilder::default()
		.existential_deposit(1)
		.monied(true)
		.build()
		.execute_with(|| {
			// Account 1 starts with 100 balance
			Balances::make_free_balance_be(&1, 100);
			assert_eq!(System::providers(&1), 1);
			assert_eq!(System::consumers(&1), 0);

			// Fill up all 16 consumer refs.
			// Note: asset pallets prevents all the consumers to be filled and leaves one untouched.
			// But other operations in the runtime, notably `uniques::set_accept_ownership` might.
			let max_consumers: u32 = <Test as frame_system::Config>::MaxConsumers::get();
			for _ in 0..max_consumers {
				assert_ok!(System::inc_consumers(&1));
			}
			assert_eq!(System::consumers(&1), max_consumers);

			// We cannot manually increment consumers beyond the limit
			assert_noop!(System::inc_consumers(&1), DispatchError::TooManyConsumers);

			// Although without limits it would work
			frame_support::hypothetically!({
				assert_ok!(System::inc_consumers_without_limit(&1));
			});

			// Now try to set a freeze - this will create the freeze, and update the frozen amount
			// just like locks, but without requiring additional consumer references.
			assert_ok!(Balances::set_freeze(&TestId::Foo, &1, 20));
			assert_eq!(Balances::balance_frozen(&TestId::Foo, &1), 20);

			// now this account has 1 more consumer reference for the lock
			assert_eq!(System::consumers(&1), max_consumers + 1);

			// frozen amount is also updated
			assert_eq!(get_test_account_data(1).frozen, 20);

			// And this account cannot transfer any funds out, same as with locks.
			assert_noop!(
				Balances::transfer_allow_death(frame_system::RawOrigin::Signed(1).into(), 2, 90,),
				DispatchError::Token(TokenError::Frozen)
			);
		});
}
