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

use std::collections::{HashMap, HashSet};

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
// same runtime is considered an error and shall result in a runtime construction failure. Host
// functions that never allocate guest memory are listed in `RUNTIME_NON_ALLOC_IMPORTS` instead.
static RUNTIME_ALLOC_IMPORTS: std::sync::LazyLock<HashMap<&str, u16>> =
	std::sync::LazyLock::new(|| {
		[
			("storage_get", 2),
			("storage_read", 2),
			("storage_clear_prefix", 4),
			("storage_root", 3),
			("storage_changes_root", 2),
			("storage_next_key", 2),
			("default_child_storage_get", 2),
			("default_child_storage_read", 2),
			("default_child_storage_storage_kill", 5),
			("default_child_storage_clear_prefix", 4),
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
			("statement_store_statements", 2),
			("statement_store_broadcasts", 2),
			("statement_store_posted", 2),
			("statement_store_posted_clear", 2),
			("benchmarking_current_time", 2),
			("benchmarking_read_write_count", 2),
			("benchmarking_get_whitelist", 2),
			("benchmarking_get_read_and_written_keys", 2),
			("benchmarking_proof_size", 2),
			// We include experimental functions here to have them supported by the time they are
			// stabilized.
			("crypto_bls381_generate", 1),
			("crypto_bls381_generate_proof_of_possession", 1),
			("crypto_bls381_public_keys", 1),
			("crypto_bls381_sign", 1),
			("crypto_ecdsa_bls381_generate", 1),
			("crypto_ecdsa_bls381_public_keys", 1),
			// Bandersnatch is still experimental and host-allocating, so we introduce placeholders
			// for its version 2 API.
			("crypto_bandersnatch_generate", 2),
			("crypto_bandersnatch_sign", 2),
		]
		.iter()
		.cloned()
		.collect()
	});

// Host functions that never allocate guest memory. They work regardless of the allocation strategy
// a runtime uses and therefore, unlike the functions in `RUNTIME_ALLOC_IMPORTS`, do not contribute
// to the host-vs-runtime side determination. They are enumerated explicitly so that the checker can
// tell a known-harmless import apart from one it has never seen before (see
// `RuntimeAllocSanityChecker::check`).
static RUNTIME_NON_ALLOC_IMPORTS: std::sync::LazyLock<HashSet<&str>> =
	std::sync::LazyLock::new(|| {
		[
			// storage
			"storage_set",
			"storage_clear",
			"storage_exists",
			"storage_append",
			"storage_start_transaction",
			"storage_rollback_transaction",
			"storage_commit_transaction",
			// default child storage
			"default_child_storage_set",
			"default_child_storage_clear",
			"default_child_storage_exists",
			// trie
			"trie_blake2_256_verify_proof",
			"trie_keccak_256_verify_proof",
			// misc
			"misc_print_num",
			"misc_print_utf8",
			"misc_print_hex",
			// crypto
			"crypto_ed25519_verify",
			"crypto_ed25519_batch_verify",
			"crypto_sr25519_verify",
			"crypto_sr25519_batch_verify",
			"crypto_start_batch_verify",
			"crypto_finish_batch_verify",
			"crypto_ecdsa_verify",
			"crypto_ecdsa_verify_prehashed",
			"crypto_ecdsa_batch_verify",
			// offchain
			"offchain_is_validator",
			"offchain_timestamp",
			"offchain_sleep_until",
			"offchain_local_storage_set",
			"offchain_local_storage_clear",
			"offchain_local_storage_compare_and_set",
			"offchain_set_authorized_nodes",
			// panic handler
			"panic_handler_abort_on_panic",
			// logging
			"logging_log",
			"logging_max_level",
			// wasm tracing
			"wasm_tracing_enabled",
			"wasm_tracing_enter_span",
			"wasm_tracing_event",
			"wasm_tracing_exit",
			// offchain index
			"offchain_index_set",
			"offchain_index_clear",
			// transaction index
			"transaction_index_index",
			"transaction_index_renew",
			// benchmarking
			"benchmarking_wipe_db",
			"benchmarking_commit_db",
			"benchmarking_reset_read_write_count",
			"benchmarking_set_whitelist",
			"benchmarking_add_to_whitelist",
			"benchmarking_remove_from_whitelist",
			// statement store
			"statement_store_submit_statement",
			"statement_store_remove",
			"statement_store_remove_by",
		]
		.into_iter()
		.collect()
	});

/// Checks if the runtime only imports functions that allocate either on the host or the runtime
/// side, but not both.
pub struct RuntimeAllocSanityChecker {
	/// Bit 0 is set once a host-side-allocating import has been seen, bit 1 once a
	/// runtime-side-allocating one has been seen.
	sides: u8,
	/// `ext_*` host-function imports that could be classified as neither allocating nor
	/// non-allocating.
	unclassified: Vec<String>,
}

impl RuntimeAllocSanityChecker {
	/// Creates a new checker.
	pub fn new() -> Self {
		Self { sides: 0, unclassified: Vec::new() }
	}

	/// Checks a single function import.
	pub fn check(&mut self, name: &str) {
		let parts = name.split('_').collect::<Vec<&str>>();
		// All runtime interface host functions are named `ext_<interface>_<function>_version_<n>`.
		if parts.len() < 4 || parts[0] != "ext" || parts[parts.len() - 2] != "version" {
			return;
		}
		let Ok(imported_version) = parts[parts.len() - 1].parse::<u16>() else { return };
		let base = parts[1..parts.len() - 2].join("_");
		if let Some(divide_version) = RUNTIME_ALLOC_IMPORTS.get(base.as_str()) {
			if imported_version < *divide_version {
				self.sides |= 1;
			} else {
				self.sides |= 2;
			}
		} else if !RUNTIME_NON_ALLOC_IMPORTS.contains(base.as_str()) {
			self.unclassified.push(name.to_string());
		}
	}

	/// Returns true if all the functions checked only allocate on the host side or only on the
	/// runtime side, but not both.
	pub fn check_result(&self) -> bool {
		self.sides < 3
	}

	/// Returns the names of the imported `ext_*` host functions that [`Self::check`] could classify
	/// as neither host-side-allocating, runtime-side-allocating nor non-allocating.
	///
	/// This is not necessarily a problem — a chain may legitimately define its own host functions —
	/// but such imports are not covered by the allocation-side consistency check, so the caller may
	/// want to surface them.
	pub fn unclassified_imports(&self) -> &[String] {
		&self.unclassified
	}
}
