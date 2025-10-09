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

//! Information around the `System` pre-compile.

#[cfg(feature = "precompiles-sol-interfaces")]
use alloy_core::sol;

/// Address for the System pre-compile.
pub const SYSTEM_PRECOMPILE_ADDR: [u8; 20] =
	hex_literal::hex!("0000000000000000000000000000000000000900");

#[cfg(feature = "precompiles-sol-interfaces")]
sol! {
	interface ISystem {
		/// Computes the BLAKE2 256-bit hash on the given input.
		function hashBlake256(bytes memory input) external pure returns (bytes32 digest);

		/// Computes the BLAKE2 128-bit hash on the given input.
		function hashBlake128(bytes memory input) external pure returns (bytes32 digest);

		/// Retrieve the account id for a specified `H160` address.
		///
		/// Calling this function on a native `H160` chain (`type AccountId = H160`)
		/// does not make sense, as it would just return the `address` that it was
		/// called with.
		///
		/// # Note
		///
		/// If no mapping exists for `addr`, the fallback account id will be returned.
		function toAccountId(address input) external view returns (bytes memory account_id);

		/// Checks whether the caller of the contract calling this function is the origin
		/// of the whole call stack.
		function callerIsOrigin() external view returns (bool);

		/// Checks whether the caller of the contract calling this function is root.
		///
		/// Note that only the origin of the call stack can be root. Hence this
		/// function returning `true` implies that the contract is being called by the origin.
		///
		/// A return value of `true` indicates that this contract is being called by a root origin,
		/// and `false` indicates that the caller is a signed origin.
		function callerIsRoot() external view returns (bool);

		/// Returns the minimum balance that is required for creating an account
		/// (the existential deposit).
		function minimumBalance() external view returns (uint);

		/// Returns the code hash of the caller.
		function ownCodeHash() external view returns (bytes32);

		/// Returns the amount of `Weight` left.
		function weightLeft() external view returns (uint64 refTime, uint64 proofSize);
	}
}
