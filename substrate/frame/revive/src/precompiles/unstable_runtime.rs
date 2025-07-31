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

//! [`UnstableRuntime`] precompile implementation.
//!
//! Provides access to runtime functionality including storage reads and call dispatch.
//!
//! # Warning
//!
//! This interface is marked as unstable because:
//! - The runtime organization of pallets, indices, and storage keys might change
//! - The encoding format might change between runtime upgrades

use crate::precompiles::{alloy::sol, AddressMatcher, Error, Ext, Precompile};
use alloc::vec::Vec;
use codec::Decode;
use core::{marker::PhantomData, num::NonZero};
use frame_support::dispatch::GetDispatchInfo;
use sp_runtime::traits::{Dispatchable, Get};

sol! {
	/// Everything here is unstable; the runtime organization of pallets,
	/// indices, and storage keys might change.
	interface IUnstableRuntime {
		/// Dispatch a runtime `encoded_call`.
		function dispatch(bytes encoded_call) external;

		/// Read the value associated with the given storage key and return the bytes
		/// of the raw value associated with it.
		function storage(bytes key) external returns (bytes);
	}
}

/// Precompile that provides access to unstable runtime functionality.
pub struct UnstableRuntime<T>(PhantomData<T>);

impl<T: crate::Config> Precompile for UnstableRuntime<T> {
	type T = T;
	type Interface = IUnstableRuntime::IUnstableRuntimeCalls;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(0x0100).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		use IUnstableRuntime::IUnstableRuntimeCalls;

		match input {
			IUnstableRuntimeCalls::dispatch(IUnstableRuntime::dispatchCall { encoded_call }) => {
				let runtime_call = <T as crate::Config>::RuntimeCall::decode(
					&mut &encoded_call[..],
				)
				.map_err(|_| {
					Error::Error(
						sp_runtime::DispatchError::Other("Invalid runtime_call encoding").into(),
					)
				})?;

				// TODO: add the weight for decoding.
				// Calculate the pre-dispatch weight of it (ignoring the decode)
				let pre_dispatch_info =
					<<T as crate::Config>::RuntimeCall as GetDispatchInfo>::get_dispatch_info(
						&runtime_call,
					);
				let pre_dispatch_weight = pre_dispatch_info.call_weight;
				let charged = env.charge(pre_dispatch_weight)?;

				let origin = env.caller();

				let post_info = runtime_call
					.dispatch(origin.into_runtime_origin())
					.map_err(|error_with_post_info| error_with_post_info.error)?;
				match post_info.actual_weight {
					Some(actual_weight) if actual_weight.all_lt(pre_dispatch_weight) => {
						env.adjust_gas(charged, actual_weight);
					},
					Some(_post_dispatch_weight_higher) => {
						// bad benchmarking. Should not happen.
					},
					_ => {
						// nada
					},
				};
				Ok(Default::default())
			},
			IUnstableRuntimeCalls::storage(IUnstableRuntime::storageCall { key }) => {
				// Charge for storage read
				let storage_read_weight = <T as frame_system::Config>::DbWeight::get().reads(1);
				// TODO: we need to also take into account the number of bytes read.
				// TODO: DbWeight doesn't count proof_size.
				// Solution: we need a benchmark that measures the actual time+proof to read a
				// storage item with a given "length". We must ask the caller to provide the upper
				// bound of the "length". This benchmark is supposedly ran on a database deep enough
				// to represent the worst case complexity of reading this storage item.
				env.charge(storage_read_weight)?;

				let value = sp_io::storage::get(key).unwrap_or_default();
				Ok(value.to_vec())
			},
		}
	}
}

#[cfg(test)]
mod tests {}
