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

//! Information around the `Storage` pre-compile.

#[cfg(feature = "precompiles-sol-interfaces")]
use alloy_core::sol;

/// Address for the `Storage` pre-compile.
pub const STORAGE_PRECOMPILE_ADDR: [u8; 20] =
	hex_literal::hex!("0000000000000000000000000000000000000901");

#[cfg(feature = "precompiles-sol-interfaces")]
sol! {
	interface IStorage {
		/// Clear the value at the given key in the contract storage.
		///
		/// # Important
		///
		/// This function can only be called via a delegate call! For Solidity, the low level
		/// `delegatecall` function has to be used. For languages that use the FFI
		/// of `pallet-revive`, the [`crate::HostFn::delegate_call`] function can be used.
		///
		/// # Parameters
		///
		/// - `key`: The storage key.
		///
		/// # Return
		///
		/// If no entry existed for this key, `containedKey` is `false` and
		/// `valueLen` is `0`.
		function clearStorage(uint32 flags, bool isFixedKey, bytes memory key)
			external returns (bool containedKey, uint valueLen);

		/// Checks whether there is a value stored under the given key.
		///
		/// The key length must not exceed the maximum defined by the contracts module parameter.
		///
		/// # Important
		///
		/// This function can only be called via a delegate call! For Solidity, the low level
		/// `delegatecall` function has to be used. For languages that use the FFI
		/// of `pallet-revive`, the [`crate::HostFn::delegate_call`] function can be used.
		///
		/// # Parameters
		///
		/// - `key`: The storage key.
		///
		/// # Return
		///
		/// Returns the size of the pre-existing value at the specified key.
		/// If no entry exists for this key `containedKey` is `false` and
		/// `valueLen` is `0`.
		function containsStorage(uint32 flags, bool isFixedKey, bytes memory key)
			external view returns (bool containedKey, uint valueLen);

		/// Retrieve and remove the value under the given key from storage.
		///
		/// # Important
		///
		/// This function can only be called via a delegate call! For Solidity, the low level
		/// `delegatecall` function has to be used. For languages that use the FFI
		/// of `pallet-revive`, the [`crate::HostFn::delegate_call`] function can be used.
		///
		/// # Parameters
		///
		/// - `key`: The storage key.
		///
		/// # Errors
		///
		/// Returns empty bytes if no value was found under `key`.
		function takeStorage(uint32 flags, bool isFixedKey, bytes memory key)
			external returns (bytes memory);
	}
}
