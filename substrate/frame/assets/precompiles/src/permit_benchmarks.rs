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

#![cfg(feature = "runtime-benchmarks")]

use crate::permit::{pallet::Config, Pallet};
use frame_benchmarking::v2::*;
use frame_support::traits::Currency;
use pallet_revive::{precompiles::H160, AddressMapper};
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
		T: pallet_assets::Config + pallet_revive::Config,
		<T as pallet_assets::Config>::AssetId: From<u32>,
		<T as pallet_assets::Config>::Balance: From<u32>,
		<T as pallet_assets::Config>::AssetIdParameter: From<<T as pallet_assets::Config>::AssetId>,
)]
mod benchmarks {
	use super::*;
	use frame_support::traits::{fungibles::metadata::Inspect as FungiblesMetadata, Get};

	#[benchmark]
	fn nonces() {
		let verifying_contract = test_verifying_contract();
		let owner = test_owner();
		crate::permit::Nonces::<T>::insert(&verifying_contract, &owner, U256::from(42));

		let result;
		#[block]
		{
			result = Pallet::<T>::nonce(&verifying_contract, &owner);
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
			result = Pallet::<T>::compute_domain_separator(&verifying_contract, name);
		}
		assert_ne!(result, sp_core::H256::zero());
	}

	#[benchmark]
	fn use_permit() {
		// Pre-computed valid permit signature for chain_id=31337
		// Generated using Hardhat account #0 private key
		//
		// Parameters:
		// - Chain ID: 31337
		// - Token Name: "Asset Permit"
		// - Owner: 0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266
		// - Verifying Contract: 0x0000000000000000000000000000000012345678
		// - Spender: 0x0000000000000000000000000000000098765432
		// - Value: 1000
		// - Nonce: 0
		// - Deadline: 18446744073709551615 (u64::MAX)
		//
		// NOTE: If you change TEST_TOKEN_NAME or other parameters, you must regenerate
		// this signature using the Hardhat account #0 private key:
		// 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
		let verifying_contract = test_verifying_contract();
		let name = TEST_TOKEN_NAME;
		let owner = test_owner();
		let spender = H160::from_low_u64_be(0x9876_5432);
		let value: [u8; 32] = U256::from(1000).to_big_endian();
		let deadline: [u8; 32] = U256::from(u64::MAX).to_big_endian();

		let v = 27u8;
		let r: [u8; 32] = [
			175, 252, 243, 1, 254, 212, 189, 22, 49, 158, 63, 188, 243, 21, 56, 240, 124, 215, 220,
			121, 137, 153, 208, 70, 123, 109, 221, 94, 191, 131, 210, 111,
		];
		let s: [u8; 32] = [
			21, 240, 201, 4, 59, 104, 154, 99, 230, 111, 29, 9, 150, 225, 57, 209, 15, 222, 27, 5,
			147, 40, 44, 246, 24, 108, 82, 129, 121, 73, 44, 234,
		];

		#[block]
		{
			Pallet::<T>::use_permit(
				&verifying_contract,
				name,
				&owner,
				&spender,
				&value,
				&deadline,
				v,
				&r,
				&s,
			)
			.expect("permit should be valid");
		}

		// Verify nonce was incremented
		assert_eq!(Pallet::<T>::nonce(&verifying_contract, &owner), U256::one());
	}

	/// End-to-end benchmark for the full `permit()` precompile call (EIP-2612).
	///
	/// Measures all operations performed by the ERC20 permit precompile in a single
	/// call: asset name DB read, ECDSA recovery + nonce write, and approval write.
	/// This is the weight that must be charged up-front before executing a permit.
	///
	/// Pre-computed signature parameters match those of `use_permit` (chain_id=31337,
	/// mock::Test config). See `use_permit` docs for regeneration instructions.
	#[benchmark]
	fn permit() {
		// ── Setup: asset ────────────────────────────────────────────────────────
		let asset_id: <T as pallet_assets::Config>::AssetId = 42u32.into();
		let asset_id_param: <T as pallet_assets::Config>::AssetIdParameter =
			asset_id.clone().into();
		let admin: T::AccountId = whitelisted_caller();
		let admin_lookup = <T as frame_system::Config>::Lookup::unlookup(admin.clone());

		pallet_assets::Pallet::<T>::force_create(
			frame_system::RawOrigin::Root.into(),
			asset_id_param.clone(),
			admin_lookup,
			true,
			1u32.into(),
		)
		.expect("asset creation should succeed");

		// Set the asset name so that the name() DB read in permit() is warm/cold as expected.
		pallet_assets::Pallet::<T>::force_set_metadata(
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
		let owner_account =
			<T as pallet_revive::Config>::AddressMapper::to_account_id(&owner);
		let deposit = <T as pallet_assets::Config>::ApprovalDeposit::get();
		<T as pallet_assets::Config>::Currency::make_free_balance_be(
			&owner_account,
			deposit + deposit,
		);

		let spender = H160::from_low_u64_be(0x9876_5432);
		let spender_account =
			<T as pallet_revive::Config>::AddressMapper::to_account_id(&spender);

		// ── Permit signature (same as use_permit benchmark) ──────────────────
		let verifying_contract = test_verifying_contract();
		let value: [u8; 32] = U256::from(1000).to_big_endian();
		let deadline: [u8; 32] = U256::from(u64::MAX).to_big_endian();
		let v = 27u8;
		let r: [u8; 32] = [
			175, 252, 243, 1, 254, 212, 189, 22, 49, 158, 63, 188, 243, 21, 56, 240, 124, 215,
			220, 121, 137, 153, 208, 70, 123, 109, 221, 94, 191, 131, 210, 111,
		];
		let s: [u8; 32] = [
			21, 240, 201, 4, 59, 104, 154, 99, 230, 111, 29, 9, 150, 225, 57, 209, 15, 222, 27,
			5, 147, 40, 44, 246, 24, 108, 82, 129, 121, 73, 44, 234,
		];

		#[block]
		{
			// 1. Asset name DB read (same as lib.rs::permit()).
			let token_name =
				<pallet_assets::Pallet<T> as FungiblesMetadata<T::AccountId>>::name(asset_id.clone());

			// 2. Permit digest computation, ECDSA recovery, and nonce write.
			Pallet::<T>::use_permit(
				&verifying_contract,
				&token_name,
				&owner,
				&spender,
				&value,
				&deadline,
				v,
				&r,
				&s,
			)
			.expect("permit should be valid");

			// 3. Approval record write (do_approve_transfer reads Asset + Approvals,
			//    writes Approvals + Asset, and reserves the deposit from owner).
			pallet_assets::Pallet::<T>::do_approve_transfer(
				asset_id,
				&owner_account,
				&spender_account,
				1u32.into(),
			)
			.expect("approval should succeed");
		}

		// Verify nonce was incremented, confirming the full flow ran.
		assert_eq!(Pallet::<T>::nonce(&verifying_contract, &owner), U256::one());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
