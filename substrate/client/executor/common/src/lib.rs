// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! A set of common definitions that are needed for defining execution engines.

#![warn(missing_docs)]
#![deny(unused_crate_dependencies)]

use std::collections::HashMap;

pub mod error;
pub mod runtime_blob;
pub mod util;
pub mod wasm_runtime;

pub(crate) fn is_polkavm_enabled() -> bool {
	std::env::var_os("SUBSTRATE_ENABLE_POLKAVM").map_or(false, |value| value == "1")
}

// Defines the divide between host-allocating host functions and runtime-allocating host functions.
// Each tuple consists of the function name and the version where the runtime-side allocation
// was first introduced. For obsolete host-allocating function the version specified must be the
// last version defined plus one. Importing functions from different sides of the divide into the
// same runtime is considered an error and shall result in a runtime construction failure.
static RUNTIME_ALLOC_IMPORTS: std::sync::LazyLock<HashMap<&str, u16>> =
	std::sync::LazyLock::new(|| {
		[
			("storage_get", 2),
			("storage_read", 2),
			("storage_clear_prefix", 3),
			("storage_root", 3),
			("storage_changes_root", 2),
			("storage_next_key", 2),
			("default_child_storage_get", 2),
			("default_child_storage_read", 2),
			("default_child_storage_storage_kill", 4),
			("default_child_storage_clear_prefix", 3),
			("default_child_storage_root", 3),
			("default_child_storage_next_key", 2),
			("trie_blake2_256_root", 3),
			("trie_blake2_256_ordered_root", 3),
			("trie_keccak_256_root", 3),
			("trie_keccak_256_ordered_root", 3),
			("misc_runtime_version", 2),
			("misc_last_cursor", 1),
			("crypto_ed25519_public_keys", 2),
			("crypto_ed25519_num_public_keys", 1),
			("crypto_ed25519_public_key", 1),
			("crypto_ed25519_generate", 2),
			("crypto_ed25519_sign", 2),
			("crypto_sr25519_public_keys", 2),
			("crypto_sr25519_num_public_keys", 1),
			("crypto_sr25519_public_key", 1),
			("crypto_sr25519_generate", 2),
			("crypto_sr25519_sign", 2),
			("crypto_ecdsa_public_keys", 2),
			("crypto_ecdsa_num_public_keys", 1),
			("crypto_ecdsa_public_key", 1),
			("crypto_ecdsa_generate", 2),
			("crypto_ecdsa_sign", 2),
			("crypto_ecdsa_sign_prehashed", 2),
			("crypto_secp256k1_ecdsa_recover", 3),
			("crypto_secp256k1_ecdsa_recover_compressed", 3),
			("hashing_keccak_256", 2),
			("hashing_keccak_512", 2),
			("hashing_sha2_256", 2),
			("hashing_blake2_128", 2),
			("hashing_blake2_256", 2),
			("hashing_twox_256", 2),
			("hashing_twox_128", 2),
			("hashing_twox_64", 2),
			("offchain_submit_transaction", 2),
			("offchain_network_state", 2),
			("offchain_network_peer_id", 1),
			("offchain_random_seed", 2),
			("offchain_local_storage_get", 2),
			("offchain_local_storage_read", 1),
			("offchain_http_request_start", 2),
			("offchain_http_request_add_header", 2),
			("offchain_http_request_write_body", 2),
			("offchain_http_response_wait", 2),
			("offchain_http_response_headers", 2),
			("offchain_http_response_header_name", 1),
			("offchain_http_response_header_value", 1),
			("offchain_http_response_read_body", 2),
			("allocator_malloc", 2),
			("allocator_free", 2),
			("input_read", 1),
		]
		.iter()
		.cloned()
		.collect()
	});

/// Checks if the runtime only imports functions that allocate either on the host or the runtime
/// side, but not both.
pub struct RuntimeAllocSanityChecker(u8);

impl RuntimeAllocSanityChecker {
	/// Creates a new checker.
	pub fn new() -> Self {
		Self(0)
	}

	/// Checks a function import.
	pub fn check(&mut self, name: &str) {
		let parts = name.split('_').collect::<Vec<&str>>();
		if parts.len() < 3 {
			return;
		}
		if parts[0] != "ext" {
			return;
		}
		let name = parts[1..parts.len() - 2].join("_");
		if let Some(divide_version) = RUNTIME_ALLOC_IMPORTS.get(name.as_str()) {
			if let Ok(imported_version) = parts[parts.len() - 1].parse::<u16>() {
				if imported_version < *divide_version {
					self.0 |= 1;
				} else {
					self.0 |= 2;
				}
			}
		}
	}

	/// Returns true if all the functions checked only allocate on the host side or only on the
	/// runtime side, but not both.
	pub fn check_result(&self) -> bool {
		self.0 < 3
	}
}
