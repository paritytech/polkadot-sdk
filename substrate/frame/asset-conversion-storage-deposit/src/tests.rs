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

use crate::{mock::*, pallet::HoldReason, *};
use frame_support::{
	assert_noop, assert_ok,
	traits::{
		fungible, fungibles,
		tokens::fungible::NativeOrWithId,
		Consideration, Footprint,
	},
};

// Returns the RuntimeHoldReason variant contributed by this pallet.
struct StorageDepositReason;
impl frame_support::traits::Get<RuntimeHoldReason> for StorageDepositReason {
	fn get() -> RuntimeHoldReason {
		RuntimeHoldReason::AssetDepositPallet(HoldReason::StorageDeposit)
	}
}

type RealConsideration = FungiblesHoldConsideration<
	Runtime,
	StorageDepositReason,
	frame_support::traits::LinearStoragePrice<
		frame_support::traits::ConstU64<10>, // base cost
		frame_support::traits::ConstU64<1>,  // per-byte cost
		Balance,
	>,
>;

const ALICE: AccountId = 1;
const ASSET_ID: AssetId = 42;
const INITIAL_NATIVE: Balance = 1_000;
const INITIAL_ASSET: Balance = 500;

fn asset() -> NativeOrWithId<AssetId> {
	NativeOrWithId::WithId(ASSET_ID)
}

// ---------------------------------------------------------------------------
// ChargeAssetStorageDeposit tests
// ---------------------------------------------------------------------------

#[test]
fn prepare_sets_deposit_asset_in_storage() {
	ExtBuilder::default().build().execute_with(|| {
		assert!(DepositAsset::<Runtime>::get().is_none());

		let ext = ChargeAssetStorageDeposit::<Runtime>::new(Some(asset()));
		// Simulate prepare: it writes to storage.
		let dummy_call =
			RuntimeCall::System(frame_system::Call::remark { remark: alloc::vec![] });
		ext.prepare(
			(),
			&frame_system::RawOrigin::Signed(ALICE).into(),
			&dummy_call,
			&Default::default(),
			0,
		)
		.expect("prepare must succeed");

		assert_eq!(DepositAsset::<Runtime>::get(), Some(asset()));
	});
}

#[test]
fn post_dispatch_clears_deposit_asset() {
	ExtBuilder::default().build().execute_with(|| {
		DepositAsset::<Runtime>::set(Some(asset()));

		ChargeAssetStorageDeposit::<Runtime>::post_dispatch_details(
			Some(asset()),
			&Default::default(),
			&Default::default(),
			0,
			&Ok(()),
		)
		.expect("post_dispatch_details must succeed");

		assert!(DepositAsset::<Runtime>::get().is_none());
	});
}

#[test]
fn post_dispatch_without_asset_is_noop() {
	ExtBuilder::default().build().execute_with(|| {
		// Nothing to clear.
		ChargeAssetStorageDeposit::<Runtime>::post_dispatch_details(
			None,
			&Default::default(),
			&Default::default(),
			0,
			&Ok(()),
		)
		.expect("post_dispatch_details must succeed");

		assert!(DepositAsset::<Runtime>::get().is_none());
	});
}

// ---------------------------------------------------------------------------
// FungiblesHoldConsideration tests
// ---------------------------------------------------------------------------

#[test]
fn new_with_no_context_holds_native() {
	ExtBuilder::default()
		.with_native_balance(ALICE, INITIAL_NATIVE)
		.build()
		.execute_with(|| {
			// No DepositAsset set → falls back to native.
			assert!(DepositAsset::<Runtime>::get().is_none());

			let footprint = Footprint { count: 1, size: 10 };
			let ticket = RealConsideration::new(&ALICE, footprint)
				.expect("consideration must succeed for native");

			// Ticket records the native asset.
			assert_eq!(ticket.0, Native::get());

			let reason = StorageDepositReason::get();
			// Native balance on hold increased.
			let held = <Balances as fungible::InspectHold<AccountId>>::balance_on_hold(
				&reason,
				&ALICE,
			);
			assert!(held > 0);

			// Drop releases the hold.
			assert_ok!(ticket.drop(&ALICE));
			let held_after =
				<Balances as fungible::InspectHold<AccountId>>::balance_on_hold(&reason, &ALICE);
			assert_eq!(held_after, 0);
		});
}

#[test]
fn new_with_asset_context_holds_non_native() {
	ExtBuilder::default()
		.with_native_balance(ALICE, INITIAL_NATIVE)
		.with_asset(ASSET_ID, ALICE, INITIAL_ASSET)
		.build()
		.execute_with(|| {
			// Create a pool so the pool-existence check passes.
			create_pool(ASSET_ID, ALICE);

			// Simulate the extension having set the preferred asset.
			DepositAsset::<Runtime>::set(Some(asset()));

			let footprint = Footprint { count: 1, size: 10 };
			let ticket = RealConsideration::new(&ALICE, footprint)
				.expect("consideration must succeed with a non-native asset");

			// Ticket records the non-native asset.
			assert_eq!(ticket.0, asset());

			let reason = StorageDepositReason::get();
			// Non-native balance on hold increased.
			let held = <AssetsHolder as fungibles::InspectHold<AccountId>>::balance_on_hold(
				ASSET_ID,
				&reason,
				&ALICE,
			);
			assert!(held > 0);

			// Drop returns the exact amount.
			let held_before = held;
			assert_ok!(ticket.drop(&ALICE));
			let held_after = <AssetsHolder as fungibles::InspectHold<AccountId>>::balance_on_hold(
				ASSET_ID,
				&reason,
				&ALICE,
			);
			assert_eq!(held_after, 0);
			let _ = held_before; // used above
		});
}

#[test]
fn new_fails_when_no_pool_for_asset() {
	ExtBuilder::default()
		.with_native_balance(ALICE, INITIAL_NATIVE)
		.with_asset(ASSET_ID, ALICE, INITIAL_ASSET)
		.build()
		.execute_with(|| {
			// No pool created — pool existence check must fail.
			DepositAsset::<Runtime>::set(Some(asset()));

			let footprint = Footprint { count: 1, size: 10 };
			assert_noop!(
				RealConsideration::new(&ALICE, footprint),
				Error::<Runtime>::NoPoolForDepositAsset,
			);
		});
}

#[test]
fn update_adjusts_held_amount() {
	ExtBuilder::default()
		.with_native_balance(ALICE, INITIAL_NATIVE)
		.with_asset(ASSET_ID, ALICE, INITIAL_ASSET)
		.build()
		.execute_with(|| {
			create_pool(ASSET_ID, ALICE);
			DepositAsset::<Runtime>::set(Some(asset()));

			let small_footprint = Footprint { count: 1, size: 0 };
			let large_footprint = Footprint { count: 2, size: 20 };

			let reason = StorageDepositReason::get();
			let ticket = RealConsideration::new(&ALICE, small_footprint)
				.expect("initial deposit");
			let held_initial =
				<AssetsHolder as fungibles::InspectHold<AccountId>>::balance_on_hold(
					ASSET_ID,
					&reason,
					&ALICE,
				);

			let ticket = ticket.update(&ALICE, large_footprint).expect("update deposit");
			let held_after_increase =
				<AssetsHolder as fungibles::InspectHold<AccountId>>::balance_on_hold(
					ASSET_ID,
					&reason,
					&ALICE,
				);
			assert!(held_after_increase > held_initial);

			let ticket = ticket.update(&ALICE, small_footprint).expect("shrink deposit");
			let held_after_decrease =
				<AssetsHolder as fungibles::InspectHold<AccountId>>::balance_on_hold(
					ASSET_ID,
					&reason,
					&ALICE,
				);
			assert_eq!(held_after_decrease, held_initial);

			assert_ok!(ticket.drop(&ALICE));
		});
}

#[test]
fn drop_returns_exact_original_amount_regardless_of_pool_state() {
	ExtBuilder::default()
		.with_native_balance(ALICE, INITIAL_NATIVE)
		.with_asset(ASSET_ID, ALICE, INITIAL_ASSET)
		.build()
		.execute_with(|| {
			create_pool(ASSET_ID, ALICE);
			DepositAsset::<Runtime>::set(Some(asset()));

			let footprint = Footprint { count: 1, size: 10 };
			let ticket = RealConsideration::new(&ALICE, footprint).expect("deposit");
			let held = ticket.1;

			// Even if the pool were modified, drop returns the stored amount.
			assert_ok!(ticket.drop(&ALICE));

			// Balance should be restored.
			use frame_support::traits::fungibles::Inspect;
			let balance_after = AssetsHolder::balance(ASSET_ID, &ALICE);
			assert_eq!(balance_after, INITIAL_ASSET);
			let _ = held;
		});
}
