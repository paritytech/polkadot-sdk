// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT-0

// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
// of the Software, and to permit persons to whom the Software is furnished to do
// so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Tests for pallet-assets-freezer.

use crate::mock::{self, *};

use codec::Compact;
use frame::testing_prelude::*;
use pallet_assets::FrozenBalance;

const WHO: AccountId = 1;
const ASSET_ID: mock::AssetId = 1;

fn test_set_freeze(id: DummyFreezeReason, amount: mock::Balance) {
	let mut freezes = Freezes::<Test>::get(ASSET_ID, WHO);

	if let Some(i) = freezes.iter_mut().find(|l| l.id == id) {
		i.amount = amount;
	} else {
		freezes
			.try_push(IdAmount { id, amount })
			.expect("freeze is added without exceeding bounds; qed");
	}

	assert_ok!(AssetsFreezer::update_freezes(ASSET_ID, &WHO, freezes.as_bounded_slice()));
}

fn test_thaw(id: DummyFreezeReason) {
	let mut freezes = Freezes::<Test>::get(ASSET_ID, WHO);
	freezes.retain(|l| l.id != id);

	assert_ok!(AssetsFreezer::update_freezes(ASSET_ID, &WHO, freezes.as_bounded_slice()));
}

mod impl_frozen_balance {
	use super::*;

	#[test]
	fn frozen_balance_works() {
		new_test_ext(|| {
			assert_eq!(AssetsFreezer::frozen_balance(ASSET_ID, &WHO), None);
			test_set_freeze(DummyFreezeReason::Governance, 1);
			assert_eq!(AssetsFreezer::frozen_balance(ASSET_ID, &WHO), Some(1u64));
			test_set_freeze(DummyFreezeReason::Staking, 3);
			assert_eq!(AssetsFreezer::frozen_balance(ASSET_ID, &WHO), Some(3u64));
			test_set_freeze(DummyFreezeReason::Governance, 2);
			assert_eq!(AssetsFreezer::frozen_balance(ASSET_ID, &WHO), Some(3u64));
			// also test thawing works to reduce a balance, and finally thawing everything resets to
			// None
			test_thaw(DummyFreezeReason::Governance);
			assert_eq!(AssetsFreezer::frozen_balance(ASSET_ID, &WHO), Some(3u64));
			test_thaw(DummyFreezeReason::Staking);
			assert_eq!(AssetsFreezer::frozen_balance(ASSET_ID, &WHO), None);
		});
	}

	#[test]
	#[should_panic = "The list of Freezes should be empty before allowing an account to die"]
	fn died_fails_if_freezes_exist() {
		new_test_ext(|| {
			test_set_freeze(DummyFreezeReason::Governance, 1);
			AssetsFreezer::died(ASSET_ID, &WHO);
		});
	}

	#[test]
	fn died_works() {
		new_test_ext(|| {
			test_set_freeze(DummyFreezeReason::Governance, 1);
			test_thaw(DummyFreezeReason::Governance);
			AssetsFreezer::died(ASSET_ID, &WHO);
			assert!(FrozenBalances::<Test>::get(ASSET_ID, WHO).is_none());
			assert!(Freezes::<Test>::get(ASSET_ID, WHO).is_empty());
		});
	}
}

mod impl_inspect_freeze {
	use super::*;

	#[test]
	fn balance_frozen_works() {
		new_test_ext(|| {
			assert_eq!(
				AssetsFreezer::balance_frozen(ASSET_ID, &DummyFreezeReason::Governance, &WHO),
				0u64
			);
			test_set_freeze(DummyFreezeReason::Governance, 1);
			assert_eq!(
				AssetsFreezer::balance_frozen(ASSET_ID, &DummyFreezeReason::Governance, &WHO),
				1u64
			);
			test_set_freeze(DummyFreezeReason::Staking, 3);
			assert_eq!(
				AssetsFreezer::balance_frozen(ASSET_ID, &DummyFreezeReason::Staking, &WHO),
				3u64
			);
			test_set_freeze(DummyFreezeReason::Staking, 2);
			assert_eq!(
				AssetsFreezer::balance_frozen(ASSET_ID, &DummyFreezeReason::Staking, &WHO),
				2u64
			);
			// also test thawing works to reduce a balance, and finally thawing everything resets to
			// 0
			test_thaw(DummyFreezeReason::Governance);
			assert_eq!(
				AssetsFreezer::balance_frozen(ASSET_ID, &DummyFreezeReason::Governance, &WHO),
				0u64
			);
			test_thaw(DummyFreezeReason::Staking);
			assert_eq!(
				AssetsFreezer::balance_frozen(ASSET_ID, &DummyFreezeReason::Staking, &WHO),
				0u64
			);
		});
	}

	/// This tests it's not possible to freeze once the freezes [`BoundedVec`] is full. This is,
	/// the lenght of the vec is equal to [`Config::MaxFreezes`].
	/// This test assumes a mock configuration where this parameter is set to `2`.
	#[test]
	fn can_freeze_works() {
		new_test_ext(|| {
			test_set_freeze(DummyFreezeReason::Governance, 1);
			assert!(AssetsFreezer::can_freeze(ASSET_ID, &DummyFreezeReason::Staking, &WHO));
			test_set_freeze(DummyFreezeReason::Staking, 1);
			assert!(!AssetsFreezer::can_freeze(ASSET_ID, &DummyFreezeReason::Other, &WHO));
		});
	}
}

mod impl_mutate_freeze {
	use super::*;

	#[test]
	fn set_freeze_works() {
		new_test_ext(|| {
			assert_eq!(
				Assets::reducible_balance(
					ASSET_ID,
					&WHO,
					Preservation::Preserve,
					Fortitude::Polite,
				),
				99
			);
			assert_ok!(AssetsFreezer::set_freeze(
				ASSET_ID,
				&DummyFreezeReason::Governance,
				&WHO,
				10
			));
			assert_eq!(
				Assets::reducible_balance(
					ASSET_ID,
					&WHO,
					Preservation::Preserve,
					Fortitude::Polite,
				),
				90
			);
			System::assert_last_event(
				Event::<Test>::Frozen { asset_id: ASSET_ID, who: WHO, amount: 10 }.into(),
			);
			assert_ok!(AssetsFreezer::set_freeze(
				ASSET_ID,
				&DummyFreezeReason::Governance,
				&WHO,
				8
			));
			assert_eq!(
				Assets::reducible_balance(
					ASSET_ID,
					&WHO,
					Preservation::Preserve,
					Fortitude::Polite,
				),
				92
			);
			System::assert_last_event(
				Event::<Test>::Thawed { asset_id: ASSET_ID, who: WHO, amount: 2 }.into(),
			);
		});
	}

	#[test]
	fn extend_freeze_works() {
		new_test_ext(|| {
			assert_ok!(AssetsFreezer::set_freeze(
				ASSET_ID,
				&DummyFreezeReason::Governance,
				&WHO,
				10
			));
			assert_storage_noop!(assert_ok!(AssetsFreezer::extend_freeze(
				ASSET_ID,
				&DummyFreezeReason::Governance,
				&WHO,
				8
			)));
			System::assert_last_event(
				Event::<Test>::Frozen { asset_id: ASSET_ID, who: WHO, amount: 10 }.into(),
			);
			assert_eq!(
				Assets::reducible_balance(
					ASSET_ID,
					&WHO,
					Preservation::Preserve,
					Fortitude::Polite,
				),
				90
			);
			assert_ok!(AssetsFreezer::extend_freeze(
				ASSET_ID,
				&DummyFreezeReason::Governance,
				&WHO,
				11
			));
			System::assert_last_event(
				Event::<Test>::Frozen { asset_id: ASSET_ID, who: WHO, amount: 1 }.into(),
			);
			assert_eq!(
				Assets::reducible_balance(
					ASSET_ID,
					&WHO,
					Preservation::Preserve,
					Fortitude::Polite,
				),
				89
			);
		});
	}

	#[test]
	fn thaw_works() {
		new_test_ext(|| {
			assert_ok!(AssetsFreezer::set_freeze(
				ASSET_ID,
				&DummyFreezeReason::Governance,
				&WHO,
				10
			));
			System::assert_has_event(
				Event::<Test>::Frozen { asset_id: ASSET_ID, who: WHO, amount: 10 }.into(),
			);
			assert_eq!(
				Assets::reducible_balance(
					ASSET_ID,
					&WHO,
					Preservation::Preserve,
					Fortitude::Polite,
				),
				90
			);
			assert_ok!(AssetsFreezer::thaw(ASSET_ID, &DummyFreezeReason::Governance, &WHO));
			System::assert_has_event(
				Event::<Test>::Thawed { asset_id: ASSET_ID, who: WHO, amount: 10 }.into(),
			);
			assert_eq!(
				Assets::reducible_balance(
					ASSET_ID,
					&WHO,
					Preservation::Preserve,
					Fortitude::Polite,
				),
				99
			);
		});
	}
}

mod with_pallet_assets {
	use super::*;

	#[test]
	fn frozen_balance_affects_balance_transferring() {
		new_test_ext(|| {
			assert_ok!(AssetsFreezer::set_freeze(
				ASSET_ID,
				&DummyFreezeReason::Governance,
				&WHO,
				20
			));
			assert_noop!(
				Assets::transfer(RuntimeOrigin::signed(WHO), Compact(ASSET_ID), 2, 81),
				pallet_assets::Error::<Test>::BalanceLow,
			);
			assert_ok!(Assets::transfer(RuntimeOrigin::signed(WHO), Compact(ASSET_ID), 2, 80));
		});
	}
}
