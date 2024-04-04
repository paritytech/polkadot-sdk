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

use frame_support::{
	assert_ok,
	traits::fungibles::{Inspect, InspectFreeze, MutateFreeze},
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

mod impls_frozen_balance {
	use super::*;

	#[test]
	fn it_works_returning_frozen_balance() {
		new_test_ext(|| {
			test_set_freeze(DummyFreezeReason::Governance, 1);
			assert_eq!(AssetsFreezer::frozen_balance(ASSET_ID, &WHO), Some(1u64));
			test_set_freeze(DummyFreezeReason::Staking, 3);
			FrozenBalances::<Test>::insert(ASSET_ID, WHO, 3);
			assert_eq!(AssetsFreezer::frozen_balance(ASSET_ID, &WHO), Some(3u64));
		});
	}

	#[test]
	fn calling_died_works() {
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
	fn it_works_returning_balance_frozen() {
		new_test_ext(|| {
			test_set_freeze(DummyFreezeReason::Governance, 1);
			assert_eq!(
				AssetsFreezer::balance_frozen(ASSET_ID, &DummyFreezeReason::Governance, &WHO),
				1u64
			);
		});
	}

	#[test]
	fn it_works_returning_can_freeze() {
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
			assert_ok!(AssetsFreezer::set_freeze(
				ASSET_ID,
				&DummyFreezeReason::Governance,
				&WHO,
				10
			));
			assert_eq!(
				AssetsFreezer::reducible_balance(
					ASSET_ID,
					&WHO,
					frame_support::traits::tokens::Preservation::Preserve,
					frame_support::traits::tokens::Fortitude::Polite,
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
				AssetsFreezer::reducible_balance(
					ASSET_ID,
					&WHO,
					frame_support::traits::tokens::Preservation::Preserve,
					frame_support::traits::tokens::Fortitude::Polite,
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
			assert_ok!(AssetsFreezer::extend_freeze(
				ASSET_ID,
				&DummyFreezeReason::Governance,
				&WHO,
				8
			));
			System::assert_last_event(
				Event::<Test>::Frozen { asset_id: ASSET_ID, who: WHO, amount: 10 }.into(),
			);
			assert_eq!(
				AssetsFreezer::reducible_balance(
					ASSET_ID,
					&WHO,
					frame_support::traits::tokens::Preservation::Preserve,
					frame_support::traits::tokens::Fortitude::Polite,
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
				AssetsFreezer::reducible_balance(
					ASSET_ID,
					&WHO,
					frame_support::traits::tokens::Preservation::Preserve,
					frame_support::traits::tokens::Fortitude::Polite,
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
				AssetsFreezer::reducible_balance(
					ASSET_ID,
					&WHO,
					frame_support::traits::tokens::Preservation::Preserve,
					frame_support::traits::tokens::Fortitude::Polite,
				),
				89
			);
			assert_ok!(AssetsFreezer::thaw(ASSET_ID, &DummyFreezeReason::Governance, &WHO));
			System::assert_has_event(
				Event::<Test>::Thawed { asset_id: ASSET_ID, who: WHO, amount: 10 }.into(),
			);
			assert_eq!(
				AssetsFreezer::reducible_balance(
					ASSET_ID,
					&WHO,
					frame_support::traits::tokens::Preservation::Preserve,
					frame_support::traits::tokens::Fortitude::Polite,
				),
				99
			);
		});
	}
}
