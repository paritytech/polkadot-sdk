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

//! Benchmarking setup for pallet-staking-async-rc-client.

use alloc::vec::Vec;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use xcm::latest::Location;
use xcm_builder::EnsureDelivery;

use crate::*;

/// Wrapper pallet for benchmarking.
pub struct Pallet<T: Config>(crate::Pallet<T>);

/// Configuration trait for benchmarking `pallet-staking-async-rc-client`.
///
/// The runtime must implement this trait to provide session keys generation
/// and XCM delivery setup for benchmarking purposes.
pub trait Config: crate::Config {
	/// Helper that ensures successful XCM delivery for benchmarks.
	type DeliveryHelper: EnsureDelivery;

	/// The relay chain location for XCM delivery.
	fn relay_chain_location() -> Location {
		Location::parent()
	}

	/// Convert an AccountId to an XCM Location for fee charging.
	fn account_to_location(account: Self::AccountId) -> Location;

	/// Generate relay chain session keys and ownership proof for benchmarking.
	///
	/// Returns the SCALE-encoded session keys and SCALE-encoded ownership proof.
	fn generate_session_keys_and_proof(owner: Self::AccountId) -> (Vec<u8>, Vec<u8>);

	/// Setup a validator account for benchmarking.
	///
	/// Should return an account that:
	/// - Is registered as a validator in the staking pallet
	/// - Has sufficient balance for XCM delivery fees
	fn setup_validator() -> Self::AccountId;
}

#[benchmarks]
mod benchmarks {
	use super::*;
	use frame_support::BoundedVec;
	use xcm_executor::traits::FeeReason;

	#[benchmark]
	fn set_keys() -> Result<(), BenchmarkError> {
		let stash = T::setup_validator();
		let (keys, proof) = T::generate_session_keys_and_proof(stash.clone());
		let keys: BoundedVec<u8, <T as crate::Config>::MaxSessionKeysLength> =
			keys.try_into().expect("keys should fit in bounded vec");
		let proof: BoundedVec<u8, <T as crate::Config>::MaxSessionKeysProofLength> =
			proof.try_into().expect("proof should fit in bounded vec");

		// Ensure XCM delivery will succeed by setting up required fees/accounts.
		let stash_location = T::account_to_location(stash.clone());
		let dest = T::relay_chain_location();
		T::DeliveryHelper::ensure_successful_delivery(
			&stash_location,
			&dest,
			FeeReason::ChargeFees,
		);

		#[extrinsic_call]
		crate::Pallet::<T>::set_keys(RawOrigin::Signed(stash), keys, proof);

		Ok(())
	}

	#[benchmark]
	fn purge_keys() -> Result<(), BenchmarkError> {
		// purge_keys can be called by any account - it just forwards the request to RC.
		// RC will handle whether keys exist or not. We only need an account with
		// sufficient balance for XCM delivery fees.
		let caller = T::setup_validator();

		// Ensure XCM delivery will succeed.
		let caller_location = T::account_to_location(caller.clone());
		let dest = T::relay_chain_location();
		T::DeliveryHelper::ensure_successful_delivery(
			&caller_location,
			&dest,
			FeeReason::ChargeFees,
		);

		#[extrinsic_call]
		crate::Pallet::<T>::purge_keys(RawOrigin::Signed(caller));

		Ok(())
	}
}
