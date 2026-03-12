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

//! Benchmarks for `pallet_assets_precompiles`.
//!
//! All benchmarks are registered under `foreign_assets::Pallet` so that a single
//! `frame-omni-bencher --pallet=pallet_assets_precompiles` run generates one
//! `weights.rs` containing every weight function.

#![cfg(feature = "runtime-benchmarks")]

use crate::{
	foreign_assets::pallet::{Config, Pallet},
	migration::MigrateForeignAssetPrecompileMappings,
};
use frame_benchmarking::v2::*;
use frame_support::{migrations::SteppedMigration, weights::WeightMeter};
use pallet_revive::precompiles::H160;
use sp_core::U256;
use sp_runtime::traits::StaticLookup;

/// Test owner address (Hardhat account #0: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266)
const TEST_OWNER: [u8; 20] = [
	0xf3, 0x9f, 0xd6, 0xe5, 0x1a, 0xad, 0x88, 0xf6, 0xf4, 0xce, 0x6a, 0xb8, 0x82, 0x72, 0x79, 0xcf,
	0xff, 0xb9, 0x22, 0x66,
];

fn test_verifying_contract() -> H160 {
	H160::from_low_u64_be(0x1234_5678)
}

fn test_owner() -> H160 {
	H160::from_slice(&TEST_OWNER)
}

/// Test token name for EIP-712 domain separator.
const TEST_TOKEN_NAME: &[u8] = b"Asset Permit";

#[benchmarks(
	where
		// Migration bounds
		T: pallet_assets::Config<T::AssetsInstance, AssetId = <T as Config>::ForeignAssetId>,
		T::ForeignAssetId: From<u32>,
		// Permit bounds
		T: crate::permit::Config,
		<T as pallet_assets::Config<T::AssetsInstance>>::Balance: From<u32>,
		<T as pallet_assets::Config<T::AssetsInstance>>::AssetIdParameter: From<<T as pallet_assets::Config<T::AssetsInstance>>::AssetId>,
)]
mod benchmarks {
	use super::*;

	// ==================== Migration benchmarks ====================

	/// Benchmark one complete `step()` invocation of the
	/// [`MigrateForeignAssetPrecompileMappings`] stepped migration.
	#[benchmark]
	fn migrate_foreign_asset_step() {
		// Clear any pre-existing assets from genesis so that only our
		// benchmark asset is present during the migration step.
		let _ = pallet_assets::Asset::<T, T::AssetsInstance>::clear(u32::MAX, None);

		// Create one asset in pallet_assets storage.
		let caller: T::AccountId = whitelisted_caller();
		let caller_lookup = <T as frame_system::Config>::Lookup::unlookup(caller);
		let asset_id: <T as pallet_assets::Config<T::AssetsInstance>>::AssetId = 42u32.into();
		let asset_id_param: <T as pallet_assets::Config<T::AssetsInstance>>::AssetIdParameter =
			asset_id.into();

		pallet_assets::Pallet::<T, T::AssetsInstance>::force_create(
			frame_system::RawOrigin::Root.into(),
			asset_id_param,
			caller_lookup,
			true,
			1u32.into(),
		)
		.unwrap();

		// Verify no precompile mapping exists yet.
		let foreign_asset_id: T::ForeignAssetId = 42u32.into();
		assert!(Pallet::<T>::asset_index_of(&foreign_asset_id).is_none());

		let mut meter = WeightMeter::new();

		#[block]
		{
			MigrateForeignAssetPrecompileMappings::<T, T::AssetsInstance, ()>::step(
				None, &mut meter,
			)
			.unwrap();
		}

		// Verify the asset was migrated.
		assert!(Pallet::<T>::asset_index_of(&foreign_asset_id).is_some());
		// The step consumes the weight twice: once for migrating the asset and once for
		// discovering that there are no more assets to migrate.
		assert_eq!(
			meter.consumed(),
			<() as crate::weights::WeightInfo>::migrate_foreign_asset_step() * 2
		);
	}

	// ==================== Permit benchmarks ====================

	#[benchmark]
	fn nonces() {
		let verifying_contract = test_verifying_contract();
		let owner = test_owner();
		crate::permit::Nonces::<T>::insert(&verifying_contract, &owner, U256::from(42));

		let result;
		#[block]
		{
			result = crate::permit::Pallet::<T>::nonce(&verifying_contract, &owner);
		}
		assert_eq!(result, U256::from(42));
	}

	#[benchmark]
	fn domain_separator() {
		let verifying_contract = test_verifying_contract();
		let name = TEST_TOKEN_NAME;

		let result;
		#[block]
		{
			result =
				crate::permit::Pallet::<T>::compute_domain_separator(&verifying_contract, name);
		}
		assert_ne!(result, sp_core::H256::zero());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
