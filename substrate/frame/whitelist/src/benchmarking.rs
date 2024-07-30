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

//! Whitelist pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::EnsureOrigin;

#[cfg(test)]
use crate::Pallet as Whitelist;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn whitelist_call() -> Result<(), BenchmarkError> {
		let origin =
			T::WhitelistOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let call_hash = Default::default();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, call_hash);

		ensure!(WhitelistedCall::<T>::contains_key(call_hash), "call not whitelisted");
		ensure!(T::Preimages::is_requested(&call_hash), "preimage not requested");
		Ok(())
	}

	#[benchmark]
	fn remove_whitelisted_call() -> Result<(), BenchmarkError> {
		let origin =
			T::WhitelistOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let call_hash = Default::default();
		Pallet::<T>::whitelist_call(origin.clone(), call_hash)
			.expect("whitelisting call must be successful");

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, call_hash);

		ensure!(!WhitelistedCall::<T>::contains_key(call_hash), "whitelist not removed");
		ensure!(!T::Preimages::is_requested(&call_hash), "preimage still requested");
		Ok(())
	}

	// We benchmark with the maximum possible size for a call.
	// If the resulting weight is too big, maybe it worth having a weight which depends
	// on the size of the call, with a new witness in parameter.
	#[benchmark(pov_mode = MaxEncodedLen {
		// Use measured PoV size for the Preimages since we pass in a length witness.
		Preimage::PreimageFor: Measured
	})]
	// NOTE: we remove `10` because we need some bytes to encode the variants and vec length
	fn dispatch_whitelisted_call(
		n: Linear<1, { T::Preimages::MAX_LENGTH as u32 - 10 }>,
	) -> Result<(), BenchmarkError> {
		let origin = T::DispatchWhitelistedOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		let remark = alloc::vec![1u8; n as usize];
		let call: <T as Config>::RuntimeCall = frame_system::Call::remark { remark }.into();
		let call_weight = call.get_dispatch_info().call_weight;
		let encoded_call = call.encode();
		let call_encoded_len = encoded_call.len() as u32;
		let call_hash = T::Hashing::hash_of(&call);

		Pallet::<T>::whitelist_call(origin.clone(), call_hash)
			.expect("whitelisting call must be successful");

		T::Preimages::note(encoded_call.into()).unwrap();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, call_hash, call_encoded_len, call_weight);

		ensure!(!WhitelistedCall::<T>::contains_key(call_hash), "whitelist not removed");
		ensure!(!T::Preimages::is_requested(&call_hash), "preimage still requested");
		Ok(())
	}

	#[benchmark]
	fn dispatch_whitelisted_call_with_preimage(n: Linear<1, 10_000>) -> Result<(), BenchmarkError> {
		let origin = T::DispatchWhitelistedOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		let remark = alloc::vec![1u8; n as usize];

		let call: <T as Config>::RuntimeCall = frame_system::Call::remark { remark }.into();
		let call_hash = T::Hashing::hash_of(&call);

		Pallet::<T>::whitelist_call(origin.clone(), call_hash)
			.expect("whitelisting call must be successful");

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, Box::new(call));

		ensure!(!WhitelistedCall::<T>::contains_key(call_hash), "whitelist not removed");
		ensure!(!T::Preimages::is_requested(&call_hash), "preimage still requested");
		Ok(())
	}

	impl_benchmark_test_suite!(Whitelist, crate::mock::new_test_ext(), crate::mock::Test);
}
