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

//! Tests for pallet-assets-holder.

use crate::mock::*;

use frame_support::{
	assert_noop, assert_ok,
	traits::tokens::{
		fungibles::{Inspect, InspectHold, Mutate, MutateHold, UnbalancedHold},
		IdAmount,
	},
};
use pallet_assets::BalanceOnHold;
use sp_runtime::TokenError;

const WHO: AccountId = 1;
/// An account without any asset account at genesis.
const OTHER: AccountId = 2;
const ASSET_ID: AssetId = 1;
/// An asset that doesn't exist.
const UNKNOWN_ASSET_ID: AssetId = 2;

fn test_hold(id: DummyHoldReason, amount: Balance) {
	assert_ok!(AssetsHolder::set_balance_on_hold(ASSET_ID, &id, &WHO, amount));
}

fn test_release(id: DummyHoldReason) {
	assert_ok!(AssetsHolder::set_balance_on_hold(ASSET_ID, &id, &WHO, 0));
}

/// Assert that no hold related state exists for `who`.
fn assert_holds_nothing(who: &AccountId) {
	assert!(!Holds::<Test>::contains_key(ASSET_ID, who));
	assert!(!BalancesOnHold::<Test>::contains_key(ASSET_ID, who));
	assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, who), 0);
}

mod impl_balance_on_hold {
	use super::*;

	#[test]
	fn balance_on_hold_works() {
		new_test_ext(|| {
			assert_eq!(
				<AssetsHolder as BalanceOnHold<_, _, _>>::balance_on_hold(ASSET_ID, &WHO),
				None
			);
			test_hold(DummyHoldReason::Governance, 1);
			assert_eq!(
				<AssetsHolder as BalanceOnHold<_, _, _>>::balance_on_hold(ASSET_ID, &WHO),
				Some(1u64)
			);
			test_hold(DummyHoldReason::Staking, 3);
			assert_eq!(
				<AssetsHolder as BalanceOnHold<_, _, _>>::balance_on_hold(ASSET_ID, &WHO),
				Some(4u64)
			);
			test_hold(DummyHoldReason::Governance, 2);
			assert_eq!(
				<AssetsHolder as BalanceOnHold<_, _, _>>::balance_on_hold(ASSET_ID, &WHO),
				Some(5u64)
			);
			// also test releasing works to reduce a balance, and finally releasing everything
			// resets to None
			test_release(DummyHoldReason::Governance);
			assert_eq!(
				<AssetsHolder as BalanceOnHold<_, _, _>>::balance_on_hold(ASSET_ID, &WHO),
				Some(3u64)
			);
			test_release(DummyHoldReason::Staking);
			assert_eq!(
				<AssetsHolder as BalanceOnHold<_, _, _>>::balance_on_hold(ASSET_ID, &WHO),
				None
			);
		});
	}

	#[test]
	#[should_panic = "The list of Holds should be empty before allowing an account to die"]
	fn died_fails_if_holds_exist() {
		new_test_ext(|| {
			test_hold(DummyHoldReason::Governance, 1);
			AssetsHolder::died(ASSET_ID, &WHO);
		});
	}

	#[test]
	fn died_works() {
		new_test_ext(|| {
			test_hold(DummyHoldReason::Governance, 1);
			test_release(DummyHoldReason::Governance);
			AssetsHolder::died(ASSET_ID, &WHO);
			assert!(BalancesOnHold::<Test>::get(ASSET_ID, WHO).is_none());
			assert!(Holds::<Test>::get(ASSET_ID, WHO).is_empty());
		});
	}
}

mod impl_hold_inspect {
	use super::*;

	#[test]
	fn total_balance_on_hold_works() {
		new_test_ext(|| {
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 0u64);
			test_hold(DummyHoldReason::Governance, 1);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 1u64);
			test_hold(DummyHoldReason::Staking, 3);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 4u64);
			test_hold(DummyHoldReason::Governance, 2);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 5u64);
			// also test release to reduce a balance, and finally releasing everything resets to
			// 0
			test_release(DummyHoldReason::Governance);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 3u64);
			test_release(DummyHoldReason::Staking);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 0u64);
		});
	}

	#[test]
	fn balance_on_hold_works() {
		new_test_ext(|| {
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO
				),
				0u64
			);
			test_hold(DummyHoldReason::Governance, 1);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO
				),
				1u64
			);
			test_hold(DummyHoldReason::Staking, 3);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Staking,
					&WHO
				),
				3u64
			);
			test_hold(DummyHoldReason::Staking, 2);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Staking,
					&WHO
				),
				2u64
			);
			// also test release to reduce a balance, and finally releasing everything resets to
			// 0
			test_release(DummyHoldReason::Governance);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO
				),
				0u64
			);
			test_release(DummyHoldReason::Staking);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Staking,
					&WHO
				),
				0u64
			);
		});
	}

	#[test]
	fn hold_available_works() {
		new_test_ext(|| {
			// A hold is available on an existing asset account, for any reason.
			for reason in
				[DummyHoldReason::Governance, DummyHoldReason::Staking, DummyHoldReason::Other]
			{
				assert!(AssetsHolder::hold_available(ASSET_ID, &reason, &WHO));
			}
			// Having some balance on hold already doesn't change anything.
			test_hold(DummyHoldReason::Governance, 1);
			assert!(AssetsHolder::hold_available(ASSET_ID, &DummyHoldReason::Governance, &WHO));
			assert!(AssetsHolder::hold_available(ASSET_ID, &DummyHoldReason::Staking, &WHO));
			test_release(DummyHoldReason::Governance);

			// A hold is not available if the asset account doesn't exist.
			assert!(!AssetsHolder::hold_available(ASSET_ID, &DummyHoldReason::Governance, &OTHER));
			// Nor if the asset doesn't exist.
			assert!(!AssetsHolder::hold_available(
				UNKNOWN_ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO
			));

			// It becomes available once the asset account is created.
			assert_ok!(Assets::mint_into(ASSET_ID, &OTHER, 10));
			assert!(AssetsHolder::hold_available(ASSET_ID, &DummyHoldReason::Governance, &OTHER));

			// And it is not available anymore once the asset account is dead.
			assert_ok!(Assets::transfer(RuntimeOrigin::signed(OTHER), ASSET_ID, WHO, 10));
			assert!(!pallet_assets::Account::<Test>::contains_key(ASSET_ID, OTHER));
			assert!(!AssetsHolder::hold_available(ASSET_ID, &DummyHoldReason::Governance, &OTHER));
		});
	}

	#[test]
	fn can_hold_works() {
		new_test_ext(|| {
			// Everything but the minimum balance can be put on hold.
			assert!(AssetsHolder::can_hold(ASSET_ID, &DummyHoldReason::Governance, &WHO, 99));
			assert_eq!(
				AssetsHolder::ensure_can_hold(ASSET_ID, &DummyHoldReason::Governance, &WHO, 100),
				Err(TokenError::FundsUnavailable.into())
			);
			assert!(!AssetsHolder::can_hold(ASSET_ID, &DummyHoldReason::Governance, &WHO, 100));

			// Nothing can be put on hold for an account that doesn't exist, not even zero.
			for amount in [0, 1, 10] {
				assert_eq!(
					AssetsHolder::ensure_can_hold(
						ASSET_ID,
						&DummyHoldReason::Governance,
						&OTHER,
						amount
					),
					Err(TokenError::CannotCreateHold.into())
				);
				assert!(!AssetsHolder::can_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&OTHER,
					amount
				));
			}

			// Nor for an asset that doesn't exist.
			assert_eq!(
				AssetsHolder::ensure_can_hold(
					UNKNOWN_ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO,
					1
				),
				Err(TokenError::CannotCreateHold.into())
			);
		});
	}
}

mod impl_hold_unbalanced {
	use super::*;

	// Note: Tests for `handle_dust`, `write_balance`, `set_total_issuance`, `decrease_balance`
	// and `increase_balance` are intentionally left out without testing, since:
	// 1. It is expected these methods are tested within `pallet-assets`, and
	// 2. There are no valid cases that can be directly asserted using those methods in
	// the scope of this pallet.

	#[test]
	fn set_balance_on_hold_works() {
		new_test_ext(|| {
			assert_eq!(Holds::<Test>::get(ASSET_ID, WHO).to_vec(), vec![]);
			assert_eq!(BalancesOnHold::<Test>::get(ASSET_ID, WHO), None);
			// Adding balance on hold works
			assert_ok!(AssetsHolder::set_balance_on_hold(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				1
			));
			assert_eq!(
				Holds::<Test>::get(ASSET_ID, WHO).to_vec(),
				vec![IdAmount { id: DummyHoldReason::Governance, amount: 1 }]
			);
			assert_eq!(BalancesOnHold::<Test>::get(ASSET_ID, WHO), Some(1));
			// Increasing hold works
			assert_ok!(AssetsHolder::set_balance_on_hold(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				3
			));
			assert_eq!(
				Holds::<Test>::get(ASSET_ID, WHO).to_vec(),
				vec![IdAmount { id: DummyHoldReason::Governance, amount: 3 }]
			);
			assert_eq!(BalancesOnHold::<Test>::get(ASSET_ID, WHO), Some(3));
			// Adding new balance on hold works
			assert_ok!(AssetsHolder::set_balance_on_hold(
				ASSET_ID,
				&DummyHoldReason::Staking,
				&WHO,
				2
			));
			assert_eq!(
				Holds::<Test>::get(ASSET_ID, WHO).to_vec(),
				vec![
					IdAmount { id: DummyHoldReason::Governance, amount: 3 },
					IdAmount { id: DummyHoldReason::Staking, amount: 2 }
				]
			);
			assert_eq!(BalancesOnHold::<Test>::get(ASSET_ID, WHO), Some(5));

			// Note: Assertion skipped to meet @gavofyork's suggestion of matching the number of
			// variant count with the number of enum's variants.
			// // Adding more than max holds fails
			// assert_noop!(
			// 	AssetsHolder::set_balance_on_hold(ASSET_ID, &DummyHoldReason::Other, &WHO, 1),
			// 	Error::<Test>::TooManyHolds
			// );

			// Decreasing balance on hold works
			assert_ok!(AssetsHolder::set_balance_on_hold(
				ASSET_ID,
				&DummyHoldReason::Staking,
				&WHO,
				1
			));
			assert_eq!(
				Holds::<Test>::get(ASSET_ID, WHO).to_vec(),
				vec![
					IdAmount { id: DummyHoldReason::Governance, amount: 3 },
					IdAmount { id: DummyHoldReason::Staking, amount: 1 }
				]
			);
			assert_eq!(BalancesOnHold::<Test>::get(ASSET_ID, WHO), Some(4));
			// Decreasing until removal of balance on hold works
			assert_ok!(AssetsHolder::set_balance_on_hold(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				0
			));
			assert_eq!(
				Holds::<Test>::get(ASSET_ID, WHO).to_vec(),
				vec![IdAmount { id: DummyHoldReason::Staking, amount: 1 }]
			);
			assert_eq!(BalancesOnHold::<Test>::get(ASSET_ID, WHO), Some(1));
			// Clearing ol all holds works
			assert_ok!(AssetsHolder::set_balance_on_hold(
				ASSET_ID,
				&DummyHoldReason::Staking,
				&WHO,
				0
			));
			assert_eq!(Holds::<Test>::get(ASSET_ID, WHO).to_vec(), vec![]);
			assert_eq!(BalancesOnHold::<Test>::get(ASSET_ID, WHO), None);
		});
	}
}

mod impl_hold_mutate {
	use super::*;
	use frame_support::traits::tokens::{Fortitude, Precision, Preservation, Restriction};

	#[test]
	fn hold_works() {
		super::new_test_ext(|| {
			// Holding some `amount` would decrease the asset account balance and change the
			// reducible balance, while total issuance is preserved.
			assert_ok!(AssetsHolder::hold(ASSET_ID, &DummyHoldReason::Governance, &WHO, 10));
			assert_eq!(Assets::balance(ASSET_ID, &WHO), 90);
			// Reducible balance is tested once to ensure token balance model is compliant.
			assert_eq!(
				Assets::reducible_balance(
					ASSET_ID,
					&WHO,
					Preservation::Expendable,
					Fortitude::Force
				),
				89
			);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO
				),
				10
			);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 10);
			// Holding preserves `total_balance`
			assert_eq!(Assets::total_balance(ASSET_ID, &WHO), 100);
			// Holding preserves `total_issuance`
			assert_eq!(Assets::total_issuance(ASSET_ID), 100);

			// Increasing the amount on hold for the same reason has the same effect as described
			// above in `set_balance_on_hold_works`, while total issuance is preserved.
			// Consideration: holding for an amount `x` will increase the already amount on hold by
			// `x`.
			assert_ok!(AssetsHolder::hold(ASSET_ID, &DummyHoldReason::Governance, &WHO, 20));
			assert_eq!(Assets::balance(ASSET_ID, &WHO), 70);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO
				),
				30
			);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 30);
			assert_eq!(Assets::total_issuance(ASSET_ID), 100);

			// Holding some amount for a different reason has the same effect as described above in
			// `set_balance_on_hold_works`, while total issuance is preserved.
			assert_ok!(AssetsHolder::hold(ASSET_ID, &DummyHoldReason::Staking, &WHO, 20));
			assert_eq!(Assets::balance(ASSET_ID, &WHO), 50);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Staking,
					&WHO
				),
				20
			);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 50);
			assert_eq!(Assets::total_issuance(ASSET_ID), 100);
		});
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		super::new_test_ext(|| {
			assert_ok!(AssetsHolder::hold(ASSET_ID, &DummyHoldReason::Governance, &WHO, 30));
			assert_ok!(AssetsHolder::hold(ASSET_ID, &DummyHoldReason::Staking, &WHO, 20));
		})
	}

	#[test]
	fn release_works() {
		// Releasing up to some amount will increase the balance by the released
		// amount, while preserving total issuance.
		new_test_ext().execute_with(|| {
			assert_ok!(AssetsHolder::release(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				20,
				Precision::Exact,
			));
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO
				),
				10
			);
			assert_eq!(Assets::balance(ASSET_ID, WHO), 70);
		});

		// Releasing over the max amount on hold with `BestEffort` will increase the
		// balance by the previously amount on hold, while preserving total issuance.
		new_test_ext().execute_with(|| {
			assert_ok!(AssetsHolder::release(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				31,
				Precision::BestEffort,
			));
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO
				),
				0
			);
			assert_eq!(Assets::balance(ASSET_ID, WHO), 80);
		});

		// Releasing over the max amount on hold with `Exact` will fail.
		new_test_ext().execute_with(|| {
			assert_noop!(
				AssetsHolder::release(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO,
					31,
					Precision::Exact,
				),
				TokenError::FundsUnavailable
			);
		});
	}

	#[test]
	fn burn_held_works() {
		// Burning works, reducing total issuance and `total_balance`.
		new_test_ext().execute_with(|| {
			assert_ok!(AssetsHolder::burn_held(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				1,
				Precision::BestEffort,
				Fortitude::Polite
			));
			assert_eq!(Assets::total_balance(ASSET_ID, &WHO), 99);
			assert_eq!(Assets::total_issuance(ASSET_ID), 99);
		});

		// Burning by an amount up to the balance on hold with `Exact` works, reducing balance on
		// hold up to the given amount.
		new_test_ext().execute_with(|| {
			assert_ok!(AssetsHolder::burn_held(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				10,
				Precision::Exact,
				Fortitude::Polite
			));
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 40);
			assert_eq!(Assets::balance(ASSET_ID, WHO), 50);
		});

		// Burning by an amount over the balance on hold with `BestEffort` works, reducing balance
		// on hold up to the given amount.
		new_test_ext().execute_with(|| {
			assert_ok!(AssetsHolder::burn_held(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				31,
				Precision::BestEffort,
				Fortitude::Polite
			));
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 20);
			assert_eq!(Assets::balance(ASSET_ID, WHO), 50);
		});

		// Burning by an amount over the balance on hold with `Exact` fails.
		new_test_ext().execute_with(|| {
			assert_noop!(
				AssetsHolder::burn_held(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO,
					31,
					Precision::Exact,
					Fortitude::Polite
				),
				TokenError::FundsUnavailable
			);
		});
	}

	#[test]
	fn burn_all_held_works() {
		new_test_ext().execute_with(|| {
			// Burning all balance on hold works as burning passing it as amount with `BestEffort`
			assert_ok!(AssetsHolder::burn_all_held(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				Precision::BestEffort,
				Fortitude::Polite,
			));
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 20);
			assert_eq!(Assets::balance(ASSET_ID, WHO), 50);
		});
	}

	#[test]
	fn done_held_works() {
		new_test_ext().execute_with(|| {
			System::assert_has_event(
				Event::<Test>::Held {
					who: WHO,
					asset_id: ASSET_ID,
					reason: DummyHoldReason::Governance,
					amount: 30,
				}
				.into(),
			);
		});
	}

	#[test]
	fn done_release_works() {
		new_test_ext().execute_with(|| {
			assert_ok!(AssetsHolder::release(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				31,
				Precision::BestEffort
			));
			System::assert_has_event(
				Event::<Test>::Released {
					who: WHO,
					asset_id: ASSET_ID,
					reason: DummyHoldReason::Governance,
					amount: 30,
				}
				.into(),
			);
		});
	}

	#[test]
	fn done_burn_held_works() {
		new_test_ext().execute_with(|| {
			assert_ok!(AssetsHolder::burn_all_held(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				Precision::BestEffort,
				Fortitude::Polite,
			));
			System::assert_has_event(
				Event::<Test>::Burned {
					who: WHO,
					asset_id: ASSET_ID,
					reason: DummyHoldReason::Governance,
					amount: 30,
				}
				.into(),
			);
		});
	}

	#[test]
	fn hold_fails_if_account_does_not_exist() {
		super::new_test_ext(|| {
			for amount in [0, 1, 10] {
				assert_noop!(
					AssetsHolder::hold(ASSET_ID, &DummyHoldReason::Governance, &OTHER, amount),
					TokenError::CannotCreateHold
				);
			}
			assert_noop!(
				AssetsHolder::hold(UNKNOWN_ASSET_ID, &DummyHoldReason::Governance, &WHO, 1),
				TokenError::CannotCreateHold
			);
			assert_holds_nothing(&OTHER);
		});
	}

	#[test]
	fn hold_works_once_account_is_created() {
		super::new_test_ext(|| {
			assert_noop!(
				AssetsHolder::hold(ASSET_ID, &DummyHoldReason::Governance, &OTHER, 5),
				TokenError::CannotCreateHold
			);

			assert_ok!(Assets::mint_into(ASSET_ID, &OTHER, 10));
			assert_ok!(AssetsHolder::hold(ASSET_ID, &DummyHoldReason::Governance, &OTHER, 5));
			assert_eq!(Assets::balance(ASSET_ID, &OTHER), 5);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &OTHER), 5);
			assert_eq!(Assets::total_balance(ASSET_ID, &OTHER), 10);
		});
	}

	#[test]
	fn transfer_and_hold_works() {
		super::new_test_ext(|| {
			// The destination must be an existing asset account.
			assert_ok!(Assets::mint_into(ASSET_ID, &OTHER, 10));
			assert_eq!(Assets::total_issuance(ASSET_ID), 110);

			assert_eq!(
				AssetsHolder::transfer_and_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO,
					&OTHER,
					20,
					Precision::Exact,
					Preservation::Preserve,
					Fortitude::Polite,
				),
				Ok(20)
			);

			// The amount is taken from the free balance of the source.
			assert_eq!(Assets::balance(ASSET_ID, &WHO), 80);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 0);
			// And put on hold on the destination, leaving its free balance untouched.
			assert_eq!(Assets::balance(ASSET_ID, &OTHER), 10);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&OTHER
				),
				20
			);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &OTHER), 20);
			assert_eq!(Assets::total_balance(ASSET_ID, &OTHER), 30);
			// Total issuance is preserved.
			assert_eq!(Assets::total_issuance(ASSET_ID), 110);

			// Transferring again for the same reason increases the hold.
			assert_ok!(AssetsHolder::transfer_and_hold(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				&OTHER,
				5,
				Precision::Exact,
				Preservation::Preserve,
				Fortitude::Polite,
			));
			assert_eq!(
				Holds::<Test>::get(ASSET_ID, OTHER).to_vec(),
				vec![IdAmount { id: DummyHoldReason::Governance, amount: 25 }]
			);
			assert_eq!(BalancesOnHold::<Test>::get(ASSET_ID, OTHER), Some(25));
		});
	}

	#[test]
	fn transfer_and_hold_to_inexisting_addresses_fails() {
		// Without any reason of existence, an account can't receive some balance on hold.
		// Otherwise anybody able to trigger a `transfer_and_hold` to an arbitrary destination
		// could bloat the state with held balances, possibly below the minimum balance, which
		// are not backed by any provider reference or deposit.
		const NUM_ACCOUNTS: u64 = 100;

		super::new_test_ext(|| {
			let initial_balance = Assets::balance(ASSET_ID, &WHO);

			for i in 0..NUM_ACCOUNTS {
				let target_account: AccountId = 1_000_000 + i;

				// The target account doesn't exist.
				assert!(!pallet_assets::Account::<Test>::contains_key(ASSET_ID, target_account));
				assert_eq!(Assets::total_balance(ASSET_ID, &target_account), 0);

				// Whether the amount is the minimum balance or more, and whatever the precision,
				// nothing is transferred.
				for (amount, precision) in [
					(1, Precision::Exact),
					(1, Precision::BestEffort),
					(50, Precision::Exact),
					(50, Precision::BestEffort),
				] {
					assert_noop!(
						AssetsHolder::transfer_and_hold(
							ASSET_ID,
							&DummyHoldReason::Governance,
							&WHO,
							&target_account,
							amount,
							precision,
							Preservation::Preserve,
							Fortitude::Polite,
						),
						TokenError::CannotCreateHold
					);
				}

				// The target account still doesn't exist and holds nothing.
				assert!(!pallet_assets::Account::<Test>::contains_key(ASSET_ID, target_account));
				assert_eq!(Assets::total_balance(ASSET_ID, &target_account), 0);
				assert_holds_nothing(&target_account);
			}

			// No state was created and the source kept its balance.
			assert_eq!(Holds::<Test>::iter().count(), 0);
			assert_eq!(BalancesOnHold::<Test>::iter().count(), 0);
			assert_eq!(Assets::balance(ASSET_ID, &WHO), initial_balance);
		});
	}

	#[test]
	fn transfer_on_hold_works() {
		// Transferring on hold to an existing account works, the balance stays on hold.
		new_test_ext().execute_with(|| {
			assert_ok!(Assets::mint_into(ASSET_ID, &OTHER, 10));

			assert_eq!(
				AssetsHolder::transfer_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO,
					&OTHER,
					10,
					Precision::Exact,
					Restriction::OnHold,
					Fortitude::Polite,
				),
				Ok(10)
			);

			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO
				),
				20
			);
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 40);
			assert_eq!(Assets::balance(ASSET_ID, &WHO), 50);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&OTHER
				),
				10
			);
			assert_eq!(Assets::balance(ASSET_ID, &OTHER), 10);
			assert_eq!(Assets::total_balance(ASSET_ID, &OTHER), 20);
			assert_eq!(Assets::total_issuance(ASSET_ID), 110);
		});

		// Transferring to the free balance of an existing account works.
		new_test_ext().execute_with(|| {
			assert_ok!(Assets::mint_into(ASSET_ID, &OTHER, 10));

			assert_eq!(
				AssetsHolder::transfer_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO,
					&OTHER,
					10,
					Precision::Exact,
					Restriction::Free,
					Fortitude::Polite,
				),
				Ok(10)
			);

			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &WHO), 40);
			assert_eq!(Assets::balance(ASSET_ID, &OTHER), 20);
			assert_holds_nothing(&OTHER);
		});
	}

	#[test]
	fn transfer_on_hold_to_inexisting_address_fails_if_on_hold() {
		new_test_ext().execute_with(|| {
			for precision in [Precision::Exact, Precision::BestEffort] {
				assert_noop!(
					AssetsHolder::transfer_on_hold(
						ASSET_ID,
						&DummyHoldReason::Governance,
						&WHO,
						&OTHER,
						10,
						precision,
						Restriction::OnHold,
						Fortitude::Polite,
					),
					TokenError::CannotCreateHold
				);
			}

			assert!(!pallet_assets::Account::<Test>::contains_key(ASSET_ID, OTHER));
			assert_holds_nothing(&OTHER);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO
				),
				30
			);
		});
	}

	#[test]
	fn transfer_on_hold_to_inexisting_address_works_if_free() {
		// No hold is created on the destination, so the hold availability doesn't matter: the
		// destination account is created with a free balance.
		new_test_ext().execute_with(|| {
			assert_eq!(
				AssetsHolder::transfer_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO,
					&OTHER,
					10,
					Precision::Exact,
					Restriction::Free,
					Fortitude::Polite,
				),
				Ok(10)
			);

			assert!(pallet_assets::Account::<Test>::contains_key(ASSET_ID, OTHER));
			assert_eq!(Assets::balance(ASSET_ID, &OTHER), 10);
			assert_holds_nothing(&OTHER);
			assert_eq!(
				<AssetsHolder as InspectHold<_>>::balance_on_hold(
					ASSET_ID,
					&DummyHoldReason::Governance,
					&WHO
				),
				20
			);

			// Now that the account exists, it can receive some balance on hold.
			assert_ok!(AssetsHolder::transfer_on_hold(
				ASSET_ID,
				&DummyHoldReason::Governance,
				&WHO,
				&OTHER,
				10,
				Precision::Exact,
				Restriction::OnHold,
				Fortitude::Polite,
			));
			assert_eq!(AssetsHolder::total_balance_on_hold(ASSET_ID, &OTHER), 10);
		});
	}
}
