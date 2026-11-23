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
//!
//! The functions whose host-function signature is the same for every host interface `sp_io` can
//! be built with live here. [`classic`] implements the remaining storage functions for the classic
//! `#[runtime_interface]`, and [`install_overrides`] installs them. A runtime built with
//! `--cfg jam` has differently shaped ones, which `cumulus-pallet-jam-system` installs before it
//! calls the validation core.

use alloc::vec::Vec;
use sp_core::storage::ChildInfo;
use sp_externalities::{set_and_run_with_externalities, Externalities};
use sp_runtime::traits::{Block as BlockT, HashingFor};
use sp_state_machine::OverlayedChanges;
use sp_trie::ProofSizeProvider;

use super::trie_recorder::SizeOnlyRecorderProvider;

#[cfg(not(jam))]
pub mod classic;

/// Swap the `sp_io` host functions for the parachain-validation implementations in this module.
///
/// The returned guard restores the originals when dropped, so the caller must keep it alive for
/// the whole validation run.
#[must_use]
pub fn install_overrides() -> impl Sized {
	(
		#[cfg(not(jam))]
		classic::install_overrides(),
		sp_io::storage::host_set.replace_implementation(host_storage_set),
		sp_io::storage::host_exists.replace_implementation(host_storage_exists),
		sp_io::storage::host_clear.replace_implementation(host_storage_clear),
		sp_io::storage::host_append.replace_implementation(host_storage_append),
		sp_io::storage::host_start_transaction
			.replace_implementation(host_storage_start_transaction),
		sp_io::storage::host_rollback_transaction
			.replace_implementation(host_storage_rollback_transaction),
		sp_io::storage::host_commit_transaction
			.replace_implementation(host_storage_commit_transaction),
		sp_io::default_child_storage::host_set
			.replace_implementation(host_default_child_storage_set),
		sp_io::default_child_storage::host_clear
			.replace_implementation(host_default_child_storage_clear),
		sp_io::default_child_storage::host_exists
			.replace_implementation(host_default_child_storage_exists),
		// `offchain_index` and `transaction_index` are host functions on wasm only; on
		// PolkaVM the runtime uses the native in-blob implementations, so there is nothing
		// to replace. Gate matches `sp_io::host_functions::wasm_only_host_functions!`.
		#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
		sp_io::offchain_index::host_set.replace_implementation(host_offchain_index_set),
		#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
		sp_io::offchain_index::host_clear.replace_implementation(host_offchain_index_clear),
		cumulus_primitives_proof_size_hostfunction::storage_proof_size::host_storage_proof_size
			.replace_implementation(host_storage_proof_size),
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

type Ext<'a, Block, Backend> = sp_state_machine::Ext<'a, HashingFor<Block>, Backend>;

fn with_externalities<F: FnOnce(&mut dyn Externalities) -> R, R>(f: F) -> R {
	sp_externalities::with_externalities(f).expect("Environmental externalities not set.")
}

// Recorder instance to be used during this validate_block call.
environmental::environmental!(recorder: trait ProofSizeProvider);

// The additional-data finalizer is threaded into the replaced `finalize` host function for the
// duration of block execution. This lives in its own module because `environmental!` emits a
// scope-level `GLOBAL` static that would otherwise collide with the `recorder` invocation above.
pub mod additional_data {
	use sp_additional_data::AdditionalDataFinalizer;

	/// The finalize provider served to the `finalize` host function during PVF block execution.
	/// Digest side comes from [`AdditionalDataFinalizer`]; blanket-implemented for any finalizer
	/// an entry point arms.
	pub trait Provider: AdditionalDataFinalizer {}
	impl<T: AdditionalDataFinalizer> Provider for T {}

	environmental::environmental!(env: trait Provider);
	pub fn using<R, F: FnOnce() -> R>(t: &mut dyn Provider, f: F) -> R {
		env::using(t, f)
	}
	pub fn with<R, F: for<'a> FnOnce(&'a mut (dyn Provider + 'a)) -> R>(f: F) -> Option<R> {
		env::with(f)
	}
}

/// Run the given closure with the externalities and recorder set.
pub fn run_with_externalities_and_recorder<Block: BlockT, R, F: FnOnce() -> R>(
	backend: &impl sp_state_machine::Backend<HashingFor<Block>>,
	recorder: &mut SizeOnlyRecorderProvider<HashingFor<Block>>,
	overlay: &mut OverlayedChanges<HashingFor<Block>>,
	state_version: sp_core::storage::StateVersion,
	execute: F,
) -> R {
	let mut ext = Ext::<Block, _>::new(overlay, backend, state_version);

	recorder::using(recorder, || set_and_run_with_externalities(&mut ext, || execute()))
}

fn host_storage_set(key: &[u8], value: &[u8]) {
	with_externalities(|ext| ext.place_storage(key.to_vec(), Some(value.to_vec())))
}

fn host_storage_exists(key: &[u8]) -> bool {
	with_externalities(|ext| ext.exists_storage(key))
}

fn host_storage_clear(key: &[u8]) {
	with_externalities(|ext| ext.place_storage(key.to_vec(), None))
}

fn host_storage_proof_size() -> u64 {
	let para =
		recorder::with(|rec| rec.estimate_encoded_size()).expect("Recorder is always set; qed");
	para as _
}

fn host_storage_append(key: &[u8], value: Vec<u8>) {
	with_externalities(|ext| ext.storage_append(key.to_vec(), value))
}

fn host_storage_start_transaction() {
	with_externalities(|ext| ext.storage_start_transaction())
}

fn host_storage_rollback_transaction() {
	with_externalities(|ext| ext.storage_rollback_transaction().ok())
		.expect("No open transaction that can be rolled back.");
}

fn host_storage_commit_transaction() {
	with_externalities(|ext| ext.storage_commit_transaction().ok())
		.expect("No open transaction that can be committed.");
}

fn host_default_child_storage_set(storage_key: &[u8], key: &[u8], value: &[u8]) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		ext.place_child_storage(&child_info, key.to_vec(), Some(value.to_vec()))
	})
}

fn host_default_child_storage_clear(storage_key: &[u8], key: &[u8]) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.place_child_storage(&child_info, key.to_vec(), None))
}

fn host_default_child_storage_exists(storage_key: &[u8], key: &[u8]) -> bool {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.exists_child_storage(&child_info, key))
}

#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
fn host_offchain_index_set(_key: &[u8], _value: &[u8]) {}

#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
fn host_offchain_index_clear(_key: &[u8]) {}

fn host_finalize_into(hash_out: &mut [u8]) -> u32 {
	// The digest folds each producer's sub-hash; on the PVF the sole producer is the finalizer
	// the entry point arms (relay V2 candidates carry no additional data, so the relay entry arms
	// none), so fold exactly its one commitment to match what the collator committed (and what
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
fn host_transaction_index_index(_extrinsic: u32, _size: u32, _context_hash: [u8; 32]) {
	// No-op host function used during parachain validation.
}

/// Parachain validation does not require maintaining a transaction index,
/// and indexing transactions does **not** contribute to the parachain state.
/// However, the host environment still expects this function to exist,
/// so we provide a no-op implementation.
#[cfg(feature = "transaction-index")]
fn host_transaction_index_renew(_extrinsic: u32, _context_hash: [u8; 32]) {
	// No-op host function used during parachain validation.
}
