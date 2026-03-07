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
use ethereum_standards::IERC20;
use frame_benchmarking::v2::*;
use frame_support::{migrations::SteppedMigration, traits::Currency, weights::WeightMeter};
use pallet_revive::{
	precompiles::{alloy, H160},
	AddressMapper,
};
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
		T: crate::permit::Config + pallet_revive::Config,
		<T as pallet_assets::Config<T::AssetsInstance>>::Balance: From<u32>,
		<T as pallet_assets::Config<T::AssetsInstance>>::AssetIdParameter: From<<T as pallet_assets::Config<T::AssetsInstance>>::AssetId>,
		pallet_assets::Call<T, T::AssetsInstance>: Into<<T as pallet_revive::Config>::RuntimeCall>,
		alloy::primitives::U256: TryInto<<T as pallet_assets::Config<T::AssetsInstance>>::Balance>,
		alloy::primitives::U256: TryFrom<<T as pallet_assets::Config<T::AssetsInstance>>::Balance>,
)]
mod benchmarks {
	use super::*;
	use frame_support::traits::Get;

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

	/// End-to-end benchmark for the full `permit()` precompile call (EIP-2612).
	///
	/// Measures all operations performed by the ERC20 permit precompile in a single
	/// call: asset name DB read, ECDSA recovery + nonce write, approval write, and
	/// event deposit. This is the weight that must be charged up-front before
	/// executing a permit.
	///
	/// Pre-computed signature parameters use chain_id=31337 (mock::Test config).
	/// See signature comment block for regeneration instructions.
	#[benchmark]
	fn permit() {
		// ── Setup: asset ────────────────────────────────────────────────────────
		let asset_id: <T as pallet_assets::Config<T::AssetsInstance>>::AssetId = 42u32.into();
		let asset_id_param: <T as pallet_assets::Config<T::AssetsInstance>>::AssetIdParameter =
			asset_id.clone().into();
		let admin: T::AccountId = whitelisted_caller();
		let admin_lookup = <T as frame_system::Config>::Lookup::unlookup(admin.clone());

		pallet_assets::Pallet::<T, T::AssetsInstance>::force_create(
			frame_system::RawOrigin::Root.into(),
			asset_id_param.clone(),
			admin_lookup,
			true,
			1u32.into(),
		)
		.expect("asset creation should succeed");

		// Set the asset name so that the name() DB read in permit() is warm/cold as expected.
		pallet_assets::Pallet::<T, T::AssetsInstance>::force_set_metadata(
			frame_system::RawOrigin::Root.into(),
			asset_id_param,
			TEST_TOKEN_NAME.to_vec(),
			b"ASSET".to_vec(),
			0,
			false,
		)
		.expect("metadata set should succeed");

		// ── Setup: owner native balance for the approval deposit ─────────────
		let owner = test_owner();
		let owner_account = <T as pallet_revive::Config>::AddressMapper::to_account_id(&owner);
		let deposit = <T as pallet_assets::Config<T::AssetsInstance>>::ApprovalDeposit::get();
		<T as pallet_assets::Config<T::AssetsInstance>>::Currency::make_free_balance_be(
			&owner_account,
			deposit + deposit,
		);

		let spender = H160::from_low_u64_be(0x9876_5432);

		// ── Permit signature ─────────────────────────────────────────────────
		let verifying_contract = test_verifying_contract();
		let v = 27u8;
		let r: [u8; 32] = [
			175, 252, 243, 1, 254, 212, 189, 22, 49, 158, 63, 188, 243, 21, 56, 240, 124, 215, 220,
			121, 137, 153, 208, 70, 123, 109, 221, 94, 191, 131, 210, 111,
		];
		let s: [u8; 32] = [
			21, 240, 201, 4, 59, 104, 154, 99, 230, 111, 29, 9, 150, 225, 57, 209, 15, 222, 27, 5,
			147, 40, 44, 246, 24, 108, 82, 129, 121, 73, 44, 234,
		];

		// Build the permitCall with alloy types.
		let call = IERC20::permitCall {
			owner: alloy::primitives::Address::from(owner.0),
			spender: alloy::primitives::Address::from(spender.0),
			value: alloy::primitives::U256::from(1000u64),
			deadline: alloy::primitives::U256::from(u64::MAX),
			v,
			r: alloy::primitives::FixedBytes(r),
			s: alloy::primitives::FixedBytes(s),
		};

		// Create env like pallet-revive benchmarks do.
		let mut call_setup = pallet_revive::call_builder::CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		#[block]
		{
			crate::ERC20::<T, crate::InlineIdConfig<0x0120>, T::AssetsInstance>::permit(
				asset_id,
				verifying_contract,
				&call,
				&mut ext,
			)
			.expect("permit should succeed");
		}

		// Verify nonce was incremented, confirming the full flow ran.
		assert_eq!(crate::permit::Pallet::<T>::nonce(&verifying_contract, &owner), U256::one());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
