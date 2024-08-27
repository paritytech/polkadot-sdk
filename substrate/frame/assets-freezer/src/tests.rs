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

//! Tests for pallet-assets-freezer.

use crate::mock::*;

use codec::Compact;
use frame_support::{
	assert_ok, assert_storage_noop,
	traits::{
		fungibles::{Inspect, InspectFreeze, MutateFreeze},
		tokens::{Fortitude, Preservation},
	},
};
use pallet_assets::FrozenBalance;

const WHO: AccountId = 1;
const ASSET_ID: AssetId = 1;

fn test_set_freeze(id: DummyFreezeReason, amount: Balance) {
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
	fn died_works() {
		new_test_ext(|| {
			test_set_freeze(DummyFreezeReason::Governance, 1);
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
				89
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
				91
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
				89
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
				88
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
				89
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
	use frame_support::assert_noop;

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
				Assets::transfer(RuntimeOrigin::signed(WHO), Compact(ASSET_ID), 2, 80),
				pallet_assets::Error::<Test>::BalanceLow,
			);
			assert_ok!(Assets::transfer(RuntimeOrigin::signed(WHO), Compact(ASSET_ID), 2, 79));
		});
	}
}
