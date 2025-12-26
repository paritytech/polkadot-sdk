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
use frame_support::assert_ok;
use frame_system::RawOrigin;

use crate::*;

/// Wrapper pallet for benchmarking.
pub struct Pallet<T: Config>(crate::Pallet<T>);

/// Configuration trait for benchmarking `pallet-staking-async-rc-client`.
///
/// The runtime must implement this trait to provide session keys generation
/// for benchmarking purposes.
pub trait Config: crate::Config {
	/// Generate relay chain session keys and ownership proof for benchmarking.
	///
	/// Returns the SCALE-encoded session keys and SCALE-encoded ownership proof.
	fn generate_session_keys_and_proof(owner: Self::AccountId) -> (Vec<u8>, Vec<u8>);

	/// Setup a validator account for benchmarking.
	fn setup_validator() -> Self::AccountId;
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_keys() -> Result<(), BenchmarkError> {
		let stash = T::setup_validator();
		let (keys, proof) = T::generate_session_keys_and_proof(stash.clone());

		#[extrinsic_call]
		crate::Pallet::<T>::set_keys(RawOrigin::Signed(stash), keys, proof);

		Ok(())
	}

	#[benchmark]
	fn purge_keys() -> Result<(), BenchmarkError> {
		let stash = T::setup_validator();
		let (keys, proof) = T::generate_session_keys_and_proof(stash.clone());

		// First set keys so we have something to purge
		assert_ok!(crate::Pallet::<T>::set_keys(
			RawOrigin::Signed(stash.clone()).into(),
			keys,
			proof
		));

		#[extrinsic_call]
		crate::Pallet::<T>::purge_keys(RawOrigin::Signed(stash));

		Ok(())
	}
}
