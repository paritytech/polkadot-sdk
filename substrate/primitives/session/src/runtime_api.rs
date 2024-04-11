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

use alloc::vec::Vec;
pub use sp_core::crypto::KeyTypeId;

sp_api::decl_runtime_apis! {
	/// Session keys runtime api.
	pub trait SessionKeys {
		/// Generate a set of session keys with optionally using the given seed.
		/// The keys should be stored within the keystore exposed via runtime
		/// externalities.
		///
		/// The seed needs to be a valid `utf8` string.
		///
		/// Returns the concatenated SCALE encoded public keys.
		fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8>;

		/// Decode the given public session keys.
		///
		/// Returns the list of public raw public keys + key type.
		fn decode_session_keys(encoded: Vec<u8>) -> Option<Vec<(Vec<u8>, sp_core::crypto::KeyTypeId)>>;
	}
}
