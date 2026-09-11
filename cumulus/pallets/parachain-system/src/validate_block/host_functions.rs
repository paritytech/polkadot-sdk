// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Host-function replacement implementations for parachain block validation.

use alloc::vec::Vec;
use codec::Encode;
use sp_externalities::{set_and_run_with_externalities, Externalities};

pub(super) use super::child_storage_host_functions::{
	host_default_child_storage_clear, host_default_child_storage_clear_prefix,
	host_default_child_storage_exists, host_default_child_storage_next_key,
	host_default_child_storage_read, host_default_child_storage_root,
	host_default_child_storage_set, host_default_child_storage_storage_kill,
};
use sp_io::StorageIterations;
use sp_runtime::traits::{Block as BlockT, HashingFor};
use sp_state_machine::OverlayedChanges;
use sp_trie::ProofSizeProvider;

/// Swap the `sp_io` host functions for the parachain-validation implementations in this module.
///
/// The returned guard restores the originals when dropped, so the caller must keep it alive for
/// the whole validation run.
#[must_use]
pub(super) fn install_overrides() -> impl Sized {
	(
		sp_io::storage::host_read.replace_implementation(host_storage_read),
		sp_io::storage::host_set.replace_implementation(host_storage_set),
		sp_io::storage::host_exists.replace_implementation(host_storage_exists),
		sp_io::storage::host_clear.replace_implementation(host_storage_clear),
		sp_io::storage::host_root.replace_implementation(host_storage_root),
		sp_io::storage::host_clear_prefix.replace_implementation(host_storage_clear_prefix),
		sp_io::storage::host_append.replace_implementation(host_storage_append),
		sp_io::storage::host_next_key.replace_implementation(host_storage_next_key),
		sp_io::storage::host_start_transaction
			.replace_implementation(host_storage_start_transaction),
		sp_io::storage::host_rollback_transaction
			.replace_implementation(host_storage_rollback_transaction),
		sp_io::storage::host_commit_transaction
			.replace_implementation(host_storage_commit_transaction),
		sp_io::default_child_storage::host_read
			.replace_implementation(host_default_child_storage_read),
		sp_io::default_child_storage::host_set
			.replace_implementation(host_default_child_storage_set),
		sp_io::default_child_storage::host_clear
			.replace_implementation(host_default_child_storage_clear),
		sp_io::default_child_storage::host_storage_kill
			.replace_implementation(host_default_child_storage_storage_kill),
		sp_io::default_child_storage::host_exists
			.replace_implementation(host_default_child_storage_exists),
		sp_io::default_child_storage::host_clear_prefix
			.replace_implementation(host_default_child_storage_clear_prefix),
		sp_io::default_child_storage::host_root
			.replace_implementation(host_default_child_storage_root),
		sp_io::default_child_storage::host_next_key
			.replace_implementation(host_default_child_storage_next_key),
		// `misc`, `offchain_index` and `transaction_index` are host functions on wasm only; on
		// PolkaVM/JAM the runtime uses the native in-blob implementations, so there is nothing
		// to replace. Gate matches `sp_io::host_functions::wasm_only_host_functions!`.
		#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
		sp_io::misc::host_last_cursor.replace_implementation(host_misc_last_cursor),
		#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
		sp_io::offchain_index::host_set.replace_implementation(host_offchain_index_set),
		#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
		sp_io::offchain_index::host_clear.replace_implementation(host_offchain_index_clear),
		cumulus_primitives_proof_size_hostfunction::storage_proof_size::host_storage_proof_size
			.replace_implementation(host_storage_proof_size),
		cumulus_primitives_additional_data::relay_chain_state::host_read_relay_chain_state_into
			.replace_implementation(host_read_relay_chain_state_into),
		sp_additional_data::additional_data::host_finalize_into
			.replace_implementation(host_finalize_into),
		#[cfg(all(
			feature = "transaction-index",
			any(not(substrate_runtime), target_family = "wasm")
		))]
		sp_io::transaction_index::host_index.replace_implementation(host_transaction_index_index),
		#[cfg(all(
			feature = "transaction-index",
			any(not(substrate_runtime), target_family = "wasm")
		))]
		sp_io::transaction_index::host_renew.replace_implementation(host_transaction_index_renew),
	)
}

use super::trie_recorder::SizeOnlyRecorderProvider;

type Ext<'a, Block, Backend> = sp_state_machine::Ext<'a, HashingFor<Block>, Backend>;

pub(super) fn with_externalities<F: FnOnce(&mut dyn Externalities) -> R, R>(f: F) -> R {
	sp_externalities::with_externalities(f).expect("Environmental externalities not set.")
}

// Recorder instance to be used during this validate_block call.
environmental::environmental!(recorder: trait ProofSizeProvider);

// The verified relay-state reader is threaded into the replaced
// `read_relay_chain_state`/`finalize` host functions for the duration of block execution. This
// lives in its own module because `environmental!` emits a scope-level `GLOBAL` static that would
// otherwise collide with the `recorder` invocation above.
pub(super) mod additional_data {
	use cumulus_primitives_additional_data::RelayStateReader;
	use sp_additional_data::AdditionalDataFinalizer;

	/// The combined read + finalize provider served to the relay-read and finalize host functions
	/// during PVF block execution. Read side comes from [`RelayStateReader`], digest side from
	/// [`AdditionalDataFinalizer`]; blanket-implemented for any reader that is both (e.g. the
	/// `AdditionalDataReader`).
	pub trait Provider: RelayStateReader + AdditionalDataFinalizer {}
	impl<T: RelayStateReader + AdditionalDataFinalizer> Provider for T {}

	environmental::environmental!(env: trait Provider);
	pub fn using<R, F: FnOnce() -> R>(t: &mut dyn Provider, f: F) -> R {
		env::using(t, f)
	}
	pub fn with<R, F: for<'a> FnOnce(&'a mut (dyn Provider + 'a)) -> R>(f: F) -> Option<R> {
		env::with(f)
	}
}

/// Run the given closure with the externalities and recorder set.
pub(super) fn run_with_externalities_and_recorder<Block: BlockT, R, F: FnOnce() -> R>(
	backend: &impl sp_state_machine::Backend<HashingFor<Block>>,
	recorder: &mut SizeOnlyRecorderProvider<HashingFor<Block>>,
	overlay: &mut OverlayedChanges<HashingFor<Block>>,
	state_version: sp_core::storage::StateVersion,
	execute: F,
) -> R {
	let mut ext = Ext::<Block, _>::new(overlay, backend).with_state_version(state_version);

	recorder::using(recorder, || set_and_run_with_externalities(&mut ext, || execute()))
}

pub(super) fn host_storage_read(
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
	allow_partial: u32,
) -> Option<u32> {
	match with_externalities(|ext| ext.storage(key)) {
		Some(value) => {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let out_len = core::cmp::min(data.len(), value_out.len());
			if value_out.len() >= data.len() || allow_partial != 0 {
				value_out[..out_len].copy_from_slice(&data[..out_len]);
			}
			Some(data.len() as u32)
		},
		None => None,
	}
}

pub(super) fn host_storage_set(key: &[u8], value: &[u8]) {
	with_externalities(|ext| ext.place_storage(key.to_vec(), Some(value.to_vec())))
}

pub(super) fn host_storage_exists(key: &[u8]) -> bool {
	with_externalities(|ext| ext.exists_storage(key))
}

pub(super) fn host_storage_clear(key: &[u8]) {
	with_externalities(|ext| ext.place_storage(key.to_vec(), None))
}

pub(super) fn host_storage_proof_size() -> u64 {
	let para =
		recorder::with(|rec| rec.estimate_encoded_size()).expect("Recorder is always set; qed");
	// The relay-read proof rides in the PoV outside the block body; count it here too so the
	// runtime's proof-size accounting (weight-reclaim) budgets for the full PoV. Symmetric with the
	// build side, which adds `AdditionalDataExt`'s size to `storage_proof_size`.
	let relay = additional_data::with(|p| p.proof_size()).unwrap_or(0);
	(para + relay) as _
}

pub(super) fn host_storage_root(out: &mut [u8]) {
	with_externalities(|ext| {
		let root = ext.storage_root();
		let encoded = root.encode();
		let out_len = out.len();
		let encoded_len = encoded.len();
		assert!(
			out_len >= encoded_len,
			"Output buffer ({out_len} bytes) provided to store the storage root hash is not large enough ({encoded_len} bytes needed)"
		);
		out[..encoded_len].copy_from_slice(&encoded[..]);
	})
}

pub(super) fn host_storage_clear_prefix(
	prefix: &[u8],
	maybe_limit: Option<u32>,
	maybe_cursor_in: Option<&[u8]>,
	maybe_cursor_out: &mut [u8],
	counters: &mut StorageIterations,
) -> u32 {
	with_externalities(|ext| {
		let removal_results =
			ext.clear_prefix(prefix, maybe_limit, maybe_cursor_in.as_ref().map(|c| &c[..]));
		let cursor_out_len = removal_results.maybe_cursor.as_ref().map(|c| c.len()).unwrap_or(0);
		if let Some(cursor_out) = removal_results.maybe_cursor {
			ext.store_last_cursor(&cursor_out[..]);
			let write_len = cursor_out_len.min(maybe_cursor_out.len());
			maybe_cursor_out[..write_len].copy_from_slice(&cursor_out[..write_len]);
		}
		counters.backend = removal_results.backend;
		counters.unique = removal_results.unique;
		counters.loops = removal_results.loops;
		cursor_out_len as u32
	})
}

pub(super) fn host_storage_append(key: &[u8], value: Vec<u8>) {
	with_externalities(|ext| ext.storage_append(key.to_vec(), value))
}

pub(super) fn host_storage_next_key(key_in: &[u8], key_out: &mut [u8]) -> u32 {
	with_externalities(|ext| {
		let next_key = ext.next_storage_key(key_in);
		let next_key_len = next_key.as_ref().map(|k| k.len()).unwrap_or(0);
		if let Some(next_key) = next_key {
			let write_len = next_key.len().min(key_out.len());
			key_out[..write_len].copy_from_slice(&next_key[..write_len]);
		}
		next_key_len as u32
	})
}

pub(super) fn host_storage_start_transaction() {
	with_externalities(|ext| ext.storage_start_transaction())
}

pub(super) fn host_storage_rollback_transaction() {
	with_externalities(|ext| ext.storage_rollback_transaction().ok())
		.expect("No open transaction that can be rolled back.");
}

pub(super) fn host_storage_commit_transaction() {
	with_externalities(|ext| ext.storage_commit_transaction().ok())
		.expect("No open transaction that can be committed.");
}

#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
pub(super) fn host_misc_last_cursor(out: &mut [u8]) -> Option<u32> {
	with_externalities(|ext| {
		let cursor = ext.take_last_cursor()?;
		if out.len() >= cursor.len() {
			out[..cursor.len()].copy_from_slice(&cursor[..]);
		} else {
			ext.store_last_cursor(&cursor[..]);
		}
		Some(cursor.len() as u32)
	})
}

#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
pub(super) fn host_offchain_index_set(_key: &[u8], _value: &[u8]) {}

#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
pub(super) fn host_offchain_index_clear(_key: &[u8]) {}

pub(super) fn host_read_relay_chain_state_into(key: &[u8], value_out: &mut [u8]) -> i64 {
	// Served by the verifying provider set up around block execution; if none is set (a block with
	// no relay reads), reports the key as absent.
	match additional_data::with(|p| p.read(key)).flatten() {
		Some(v) => {
			let n = core::cmp::min(v.len(), value_out.len());
			value_out[..n].copy_from_slice(&v[..n]);
			v.len() as i64
		},
		None => -1,
	}
}

pub(super) fn host_finalize_into(hash_out: &mut [u8]) -> u32 {
	// The digest folds each producer's sub-hash; on the PVF the sole producer is the relay reader,
	// so fold exactly its one commitment to match what the collator committed (and what
	// `AdditionalDataExt::finalize` recomputes on build/import).
	match additional_data::with(|p| p.finalize()).flatten() {
		Some(sub) => {
			let folded = sp_additional_data::hash_commitments(core::iter::once(sub))
				.expect("non-empty input yields Some; qed");
			hash_out[..32].copy_from_slice(&folded);
			1
		},
		None => 0,
	}
}

/// Parachain validation does not require maintaining a transaction index,
/// and indexing transactions does **not** contribute to the parachain state.
/// However, the host environment still expects this function to exist,
/// so we provide a no-op implementation.
#[cfg(feature = "transaction-index")]
pub(super) fn host_transaction_index_index(_extrinsic: u32, _size: u32, _context_hash: [u8; 32]) {
	// No-op host function used during parachain validation.
}

/// Parachain validation does not require maintaining a transaction index,
/// and indexing transactions does **not** contribute to the parachain state.
/// However, the host environment still expects this function to exist,
/// so we provide a no-op implementation.
#[cfg(feature = "transaction-index")]
pub(super) fn host_transaction_index_renew(_extrinsic: u32, _context_hash: [u8; 32]) {
	// No-op host function used during parachain validation.
}
