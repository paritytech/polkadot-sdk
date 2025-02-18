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
	traits::tokens::fungibles::{Inspect, InspectHold, MutateHold, UnbalancedHold},
};
use pallet_assets::BalanceOnHold;

const WHO: AccountId = 1;
const ASSET_ID: AssetId = 1;

fn test_hold(id: DummyHoldReason, amount: Balance) {
	assert_ok!(AssetsHolder::set_balance_on_hold(ASSET_ID, &id, &WHO, amount));
}

fn test_release(id: DummyHoldReason) {
	assert_ok!(AssetsHolder::set_balance_on_hold(ASSET_ID, &id, &WHO, 0));
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
	use frame_support::traits::tokens::{Fortitude, Precision, Preservation};
	use sp_runtime::TokenError;

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
}
