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

//! Host functions for a JAM runtime.
//!
//! A JAM runtime gets a small, fixed, unversioned list of host calls declared with the
//! [`sp_jam_runtime_interface::host_calls!`] macro; everything else (hashing, signature checks,
//! trie roots) runs inside the runtime itself. PolkaVM/JAM dispatches host calls by number
//! (`ecalli N`), and the PolkaVM linker rejects a program mixing indexed with unindexed
//! imports — so every import a riscv runtime can emit carries a fixed `index = N`.
//!
//! # PolkaVM host-call indices
//!
//! | Index | Call | Node side |
//! | --- | --- | --- |
//! | 0 | `jam::gas` | no (Parachain Service) |
//! | 1 | `jam::grow_heap` | no |
//! | 2 | `jam::fetch` | no |
//! | 7 | `jam::historical_lookup` | no |
//! | 8 | `jam::export` | no |
//! | 200 | `service::set_parent_head_hash` | no |
//! | 201 | `service::set_head` | no |
//! | 202 | `service::send_upward_message` | no |
//! | 203 | `service::report_error` | no |
//! | 204 | reserved: log (not emitted, logging is a no-op) | - |
//! | 301-311 | `storage::{read, set, clear, exists, clear_prefix, append, root, next_key, start_transaction, rollback_transaction, commit_transaction}` | yes |
//! | 312-316 | reserved (bandersnatch/bls) | - |
//! | 320-327 | `default_child_storage::{read, set, clear, storage_kill, exists, clear_prefix, root, next_key}` | yes |
//! | 328-337 | `crypto::{ed25519_generate, ed25519_public_keys, ed25519_sign, sr25519_generate, sr25519_public_keys, sr25519_sign, ecdsa_generate, ecdsa_public_keys, ecdsa_sign, ecdsa_sign_prehashed}` | yes |
//! | 340 | `input::read` | yes |
//! | 341-344 | reserved, owned outside sp-io (proof size, additional data, jam state read) | - |
//! | 345-346 | `offchain_index::{set, clear}` | yes |
//! | 347-348 | `transaction_index::{index, renew}` | yes |
//! | 350-367 | `offchain::{is_validator, submit_transaction, network_peer_id, timestamp, sleep_until, random_seed, local_storage_set, local_storage_clear, local_storage_compare_and_set, local_storage_read, http_request_start, http_request_add_header, http_request_write_body, http_response_wait, http_response_header_name, http_response_header_value, http_response_read_body, set_authorized_nodes}` | yes |
//!
//! Reserved indices are never emitted as imports; they exist to keep the numbering stable.

pub mod host;
pub mod native;

pub use host::*;
pub use native::*;

#[cfg(all(test, not(substrate_runtime)))]
mod tests;

use codec::{Decode, Encode};

pub use sp_crypto_hashing as hashing;
pub use sp_externalities::MultiRemovalResults;

/// The JAM PolkaVM host-call index table, collected from every module that declares host
/// calls, sorted by index.
pub fn host_call_indices() -> alloc::vec::Vec<(u32, &'static str)> {
	let mut v = alloc::vec::Vec::new();
	v.extend_from_slice(host::jam::HOST_CALL_INDICES);
	v.extend_from_slice(service::HOST_CALL_INDICES);
	v.extend_from_slice(storage::HOST_CALL_INDICES);
	v.extend_from_slice(default_child_storage::HOST_CALL_INDICES);
	v.extend_from_slice(crypto::HOST_CALL_INDICES);
	v.extend_from_slice(input::HOST_CALL_INDICES);
	v.extend_from_slice(transaction_index::HOST_CALL_INDICES);
	v.extend_from_slice(offchain::HOST_CALL_INDICES);
	v.sort_by_key(|(idx, _)| *idx);
	v
}

/// Provides the node-side implementations of the JAM host calls.
#[cfg(not(substrate_runtime))]
pub type HostFunctions = (
	storage::HostFunctions,
	default_child_storage::HostFunctions,
	crypto::HostFunctions,
	input::HostFunctions,
	offchain::HostFunctions,
	transaction_index::HostFunctions,
);

/// The host functions Substrate provides for a JAM runtime environment.
///
/// All these host functions will be callable from inside the PolkaVM environment.
#[cfg(not(substrate_runtime))]
pub type SubstrateHostFunctions = HostFunctions;

/// Extension to signal to [`crypto::ed25519_verify`] to use the dalek crate.
///
/// The switch from `ed25519-dalek` to `ed25519-zebra` was a breaking change. `ed25519-zebra`
/// is more permissive when verifying signatures, so some chains may fail to sync from genesis
/// with it. Register this extension to the runtime execution environment to signal that
/// `ed25519-dalek` should be used instead.
#[cfg(not(substrate_runtime))]
sp_externalities::decl_extension! {
	pub struct UseDalekExt;
}

#[cfg(not(substrate_runtime))]
impl Default for UseDalekExt {
	fn default() -> Self {
		Self
	}
}

/// Error verifying ECDSA signature
#[derive(Encode, Decode)]
pub enum EcdsaVerifyError {
	/// Incorrect value of R or S
	BadRS,
	/// Incorrect value of V
	BadV,
	/// Invalid signature
	BadSignature,
}

/// The outcome of calling `storage_kill`. Returned value is the number of storage items
/// removed from the backend from making the `storage_kill` call.
#[derive(Encode, Decode)]
pub enum KillStorageResult {
	/// All keys to remove were removed, return number of iterations performed during the
	/// operation.
	AllRemoved(u32),
	/// Not all key to remove were removed, return number of iterations performed during the
	/// operation.
	SomeRemaining(u32),
}

impl From<MultiRemovalResults> for KillStorageResult {
	fn from(r: MultiRemovalResults) -> Self {
		// We use `loops` here rather than `backend` because that's the same as the original
		// functionality pre-#11490. This won't matter once we switch to the new host function
		// since we won't be using the `KillStorageResult` type in the runtime any more.
		match r.maybe_cursor {
			None => Self::AllRemoved(r.loops),
			Some(..) => Self::SomeRemaining(r.loops),
		}
	}
}

/// Type alias for Externalities implementation used in tests.
#[cfg(feature = "std")] // NOTE: Deliberately isn't `not(substrate_runtime)`.
pub type TestExternalities = sp_state_machine::TestExternalities<sp_core::Blake2Hasher>;

/// Initialize tracing of sp_tracing.
///
/// The jam tracing functions are no-ops inside the runtime, so this only installs the bridging
/// subscriber when built with `substrate_runtime` and the `with-tracing` feature; otherwise it
/// is a no-op.
pub use wasm_tracing::init_tracing;

/// Crashes the execution of the program.
///
/// Equivalent to the WASM `unreachable` instruction, RISC-V `unimp` instruction,
/// or just the `unreachable!()` macro everywhere else.
pub fn unreachable() -> ! {
	#[cfg(target_family = "wasm")]
	{
		core::arch::wasm32::unreachable();
	}

	#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
	// SAFETY: `unimp` unconditionally traps and never returns.
	unsafe {
		core::arch::asm!("unimp", options(noreturn));
	}

	#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64", target_family = "wasm")))]
	unreachable!();
}
