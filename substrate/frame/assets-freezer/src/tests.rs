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
use sp_runtime::BoundedVec;

use frame_support::{
	assert_ok,
	traits::fungibles::{Inspect, InspectFreeze, MutateFreeze},
};
use pallet_assets::FrozenBalance;

const WHO: AccountId = 1;
const ASSET_ID: AssetId = 1;

fn basic_freeze() {
	Freezes::<Test>::set(
		ASSET_ID,
		WHO,
		BoundedVec::truncate_from(vec![IdAmount { id: DummyFreezeReason::Governance, amount: 1 }]),
	);
	FrozenBalances::<Test>::insert(ASSET_ID, WHO, 1);
}

#[test]
fn it_works_returning_balance_frozen() {
	new_test_ext().execute_with(|| {
		basic_freeze();
		assert_eq!(
			AssetsFreezer::balance_frozen(ASSET_ID, &DummyFreezeReason::Governance, &WHO),
			1u64
		);
	});
}

#[test]
fn it_works_returning_frozen_balances() {
	new_test_ext().execute_with(|| {
		basic_freeze();
		assert_eq!(AssetsFreezer::frozen_balance(ASSET_ID, &WHO), Some(1u64));
		FrozenBalances::<Test>::insert(ASSET_ID, WHO, 3);
		assert_eq!(AssetsFreezer::frozen_balance(ASSET_ID, &WHO), Some(3u64));
	});
}

#[test]
fn it_works_returning_can_freeze() {
	new_test_ext().execute_with(|| {
		basic_freeze();
		assert!(AssetsFreezer::can_freeze(ASSET_ID, &DummyFreezeReason::Staking, &WHO));
		Freezes::<Test>::mutate(&ASSET_ID, &WHO, |f| {
			f.try_push(IdAmount { id: DummyFreezeReason::Staking, amount: 1 })
				.expect("current freezes size is less than max freezes; qed");
		});
		assert!(!AssetsFreezer::can_freeze(ASSET_ID, &DummyFreezeReason::Other, &WHO));
	});
}

#[test]
fn set_freeze_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(AssetsFreezer::set_freeze(ASSET_ID, &DummyFreezeReason::Governance, &WHO, 10));
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
			Event::<Test>::AssetFrozen { asset_id: ASSET_ID, who: WHO, amount: 10 }.into(),
		);
		assert_ok!(AssetsFreezer::set_freeze(ASSET_ID, &DummyFreezeReason::Governance, &WHO, 8));
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
			Event::<Test>::AssetThawed { asset_id: ASSET_ID, who: WHO, amount: 2 }.into(),
		);
	});
}

#[test]
fn extend_freeze_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(AssetsFreezer::set_freeze(ASSET_ID, &DummyFreezeReason::Governance, &WHO, 10));
		assert_ok!(AssetsFreezer::extend_freeze(ASSET_ID, &DummyFreezeReason::Governance, &WHO, 8));
		System::assert_last_event(
			Event::<Test>::AssetFrozen { asset_id: ASSET_ID, who: WHO, amount: 10 }.into(),
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
			Event::<Test>::AssetFrozen { asset_id: ASSET_ID, who: WHO, amount: 1 }.into(),
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
	new_test_ext().execute_with(|| {
		assert_ok!(AssetsFreezer::set_freeze(ASSET_ID, &DummyFreezeReason::Governance, &WHO, 10));
		System::assert_has_event(
			Event::<Test>::AssetFrozen { asset_id: ASSET_ID, who: WHO, amount: 10 }.into(),
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
			Event::<Test>::AssetThawed { asset_id: ASSET_ID, who: WHO, amount: 10 }.into(),
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
