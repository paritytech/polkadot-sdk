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

//! Helper utilities around pre-compiles.

/// Returns the Solidity selector for `fn_sig`.
///
/// Note that this is a const function, it is evaluated at compile time.
///
/// # Usage
///
/// ```
/// # use pallet_revive_uapi::solidity_selector;
/// let sel = solidity_selector("ownCodeHash()");
/// assert_eq!(sel, [219, 107, 220, 138]);
/// ```
pub const fn solidity_selector(fn_sig: &str) -> [u8; 4] {
	let output: [u8; 32] =
		const_crypto::sha3::Keccak256::new().update(fn_sig.as_bytes()).finalize();
	[output[0], output[1], output[2], output[3]]
}
