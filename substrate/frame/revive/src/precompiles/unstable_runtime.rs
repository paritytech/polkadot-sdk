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
//! Provides low-level access to runtime functionality from within a contract:
//! - [`IUnstableRuntime::dispatch`]: dispatch an arbitrary SCALE-encoded
//!   `RuntimeCall` as the calling contract's account (a `Signed` origin).
//! - [`IUnstableRuntime::storage`]: read the raw bytes of any runtime storage
//!   item by its full storage key.
//!
//! # Warning
//!
//! This interface is **unstable**:
//! - The runtime organization of pallets, indices, and storage keys might change
//!   between runtime upgrades.
//! - The encoding format might change between runtime upgrades.
//!
//! Contracts relying on it can break across upgrades. It is opt-in per runtime
//! and intended for experimentation, not production use.

use crate::precompiles::{alloy::sol, AddressMatcher, Error, Ext, Precompile};
use alloc::vec::Vec;
use core::{marker::PhantomData, num::NonZero};

sol! {
	/// Everything here is unstable; the runtime organization of pallets,
	/// indices, and storage keys might change between runtime upgrades.
	interface IUnstableRuntime {
		/// Dispatch a SCALE-encoded runtime `encoded_call` as the calling
		/// contract's account (a `Signed` origin).
		function dispatch(bytes encoded_call) external;

		/// Read the raw bytes of the runtime storage value at `key`.
		///
		/// Returns empty bytes if the key is absent. `max_len` is the caller's
		/// declared upper bound on the value length, used for metering.
		function storage(bytes key, uint32 max_len) external returns (bytes);
	}
}

/// Precompile that provides access to unstable runtime functionality.
pub struct UnstableRuntime<T>(PhantomData<T>);

impl<T: crate::Config> Precompile for UnstableRuntime<T> {
	type T = T;
	type Interface = IUnstableRuntime::IUnstableRuntimeCalls;
	// TODO: provisional address (see design doc O6); confirm before stabilising.
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(0x0100).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		_env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		use IUnstableRuntime::IUnstableRuntimeCalls;

		match input {
			IUnstableRuntimeCalls::dispatch(IUnstableRuntime::dispatchCall { encoded_call }) => {
				let _ = encoded_call;
				// TODO: implement dispatch (decode RuntimeCall, guards, charge,
				// dispatch as Signed(caller), adjust weight).
				Ok(Default::default())
			},
			IUnstableRuntimeCalls::storage(IUnstableRuntime::storageCall { key, max_len }) => {
				let _ = (key, max_len);
				// TODO: implement storage read (charge by max_len, length-aware
				// read against the main trie, ABI-encode the bytes return).
				Ok(Default::default())
			},
		}
	}
}

#[cfg(test)]
mod tests {}
