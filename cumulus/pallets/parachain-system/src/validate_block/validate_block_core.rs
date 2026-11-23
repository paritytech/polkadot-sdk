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

//! Consensus-agnostic core of the parachain block validation engine.
//!
//! This module implements everything the polkadot entry point shares with other entry points
//! (`cumulus-pallet-jam-system` has one): decoding the parent head, replacing the storage related
//! host functions, building the in-memory database from the block's storage proof and executing
//! every block against it.
//!
//! Everything an entry point derives from the executed blocks lives behind [`ValidationHooks`],
//! so the core needs no pallet configuration and no target-specific `cfg`. The relay-chain
//! entry, [`crate::relay_chain::relay_chain_implementation`], verifies the V3 scheduling
//! signature in [`ValidationHooks::on_db_ready`] and re-checks the validation data and collects
//! the candidate outputs in [`ValidationHooks::on_block_executed`]. An entry point that carries
//! additional data arms its reader and finalizer in [`ValidationHooks::execute_block`].

use super::{
	block_checks::verify_blocks_form_chain, bytes::Bytes,
	host_functions::run_with_externalities_and_recorder, trie_cache, trie_recorder,
};
use alloc::vec::Vec;
use cumulus_primitives_core::ParachainBlockData;
use frame_support::traits::ExecuteBlock;
use sp_additional_data::AdditionalData;
use sp_core::storage::{well_known_keys, StateVersion};
use sp_runtime::traits::{
	Block as BlockT, Hash as HashT, HashingFor, Header as HeaderT, LazyBlock,
};
use sp_state_machine::OverlayedChanges;
use sp_trie::{HashDBT, MemoryDB, EMPTY_PREFIX};
use sp_version::RuntimeVersion;
use trie_recorder::{SeenNodes, SizeOnlyRecorderProvider};

/// The input of a single [`execute_blocks`] call.
///
/// `block_data` is already decoded and `randomness_seed` already derived by the caller: the seed
/// hashes the relay-parent storage root alongside the block hashes, which is relay-specific, so
/// only its `[u8; 16]` result crosses the seam.
pub struct SharedValidationInputs<B: BlockT> {
	pub block_data: ParachainBlockData<B::LazyBlock>,
	pub parent_head: Bytes,
	pub randomness_seed: [u8; 16],
}

/// The points at which an entry point takes part in [`execute_blocks`].
pub trait ValidationHooks<B: BlockT> {
	/// Runs once, after the in-memory database was built from the PoV's storage proof and before
	/// the first block executes. `parent` is the header the PoV builds on.
	fn on_db_ready(
		&mut self,
		_db: &MemoryDB<HashingFor<B>>,
		_cache: &trie_cache::CacheProvider<HashingFor<B>>,
		_parent: &B::Header,
		_state_version: StateVersion,
	) {
	}

	/// Executes block `index` by calling `execute` exactly once, with whatever the entry point
	/// arms around it. `additional_data` is the block's entry, absent for V0-V2 PoVs.
	fn execute_block(
		&mut self,
		_index: usize,
		_additional_data: Option<&AdditionalData>,
		execute: &mut dyn FnMut(),
	) {
		execute()
	}

	/// Runs inside the externalities scope over the block's post-state, while the storage
	/// host-function overrides are installed. `header` is the executed block's header and
	/// `parent_head` the encoded header the PoV builds on.
	fn on_block_executed(
		&mut self,
		index: usize,
		is_last: bool,
		header: &B::Header,
		parent_head: &[u8],
	);
}

/// Execute the given blocks against the sparse in-memory database built from their storage proof.
///
/// This is roughly the following:
///
/// 1. We decode the `parent_head` from `inputs` and check that it is the parent of the first block
///    of `block_data` (see [`verify_blocks_form_chain`]).
///
/// 2. We construct the sparse in-memory database from the storage proof inside the block data and
///    then ensure that the storage root matches the storage root in the `parent_head`.
///
/// 3. We replace all the storage related host functions with functions inside the wasm blob. This
///    means instead of calling into the host, we will stay inside the wasm execution. This is very
///    important as the relay chain validator hasn't the state required to verify the block. But we
///    have the in-memory database that contains all the values from the state of the parachain that
///    we require to verify the block.
///
/// 4. The last step is to execute the entire block in the machinery we just have setup. Executing
///    the blocks include running all transactions in the block against our in-memory database and
///    ensuring that the final storage root matches the storage root in the header of the block.
///
/// `version` is the runtime's own version; it selects the state version and the code-upgrade
/// storage key. `hooks` receives the entry-point specific steps, see [`ValidationHooks`].
pub fn execute_blocks<B: BlockT, E: ExecuteBlock<B>>(
	inputs: SharedValidationInputs<B>,
	version: &RuntimeVersion,
	hooks: &mut dyn ValidationHooks<B>,
) {
	let _guard = super::host_functions::install_overrides();

	// Initialize hashmaps randomness.
	sp_trie::add_extra_randomness(inputs.randomness_seed);

	let mut parent_header = codec::decode_from_bytes::<B::Header>(inputs.parent_head.clone())
		.expect("Invalid parent head");

	let additional_data_per_block: Vec<Option<AdditionalData>> =
		inputs.block_data.additional_data().to_vec();
	let (blocks, proof) = inputs.block_data.into_inner();

	// Additional data is either absent entirely (V0/V1/V2) or carries exactly one entry per block
	// (V3). Any other length would smuggle items belonging to no block — committed by no header
	// digest, so otherwise unchecked. Per block, `additional_data_per_block.get(i)` pairs each
	// entry with its block; a header-digest/data mismatch is caught below against that block's
	// digest.
	assert!(
		additional_data_per_block.is_empty() || additional_data_per_block.len() == blocks.len(),
		"additional data vector length does not match the number of blocks"
	);

	verify_blocks_form_chain::<B>(&blocks, &parent_header);

	let num_blocks = blocks.len();
	let state_version = version.state_version();

	// Create the db
	let mut db = match proof.to_memory_db::<HashingFor<B>>(parent_header.state_root()) {
		Ok((db, _)) => db,
		Err(_) => panic!("Compact proof decoding failure."),
	};

	core::mem::drop(proof);

	let cache_provider = trie_cache::CacheProvider::new();
	let seen_nodes = SeenNodes::<HashingFor<B>>::default();

	// Before the loop below reassigns `parent_header`, so it is still the header the PoV builds
	// on.
	hooks.on_db_ready(&db, &cache_provider, &parent_header, state_version);

	for (block_index, mut block) in blocks.into_iter().enumerate() {
		// We use the storage root of the `parent_head` to ensure that it is the correct root.
		// This is already being done above while creating the in-memory db, but let's be paranoid!!
		let backend = sp_state_machine::TrieBackendBuilder::new_with_cache(
			&db,
			*parent_header.state_root(),
			&cache_provider,
		)
		.build();

		// Each node only contributes once to the total size of the storage proof. So, we keep track
		// of them inside `seen_nodes` to always return the correct proof size.
		let mut execute_recorder = SizeOnlyRecorderProvider::with_seen_nodes(seen_nodes.clone());
		// `backend` with the `execute_recorder`. As the `execute_recorder`, this should only be
		// used for `execute_block`.
		let execute_backend = sp_state_machine::TrieBackendBuilder::wrap(&backend)
			.with_recorder(execute_recorder.clone())
			.build();

		let mut overlay = OverlayedChanges::default();

		parent_header = block.header().clone();

		let additional_data_digest_count = parent_header
			.digest()
			.logs()
			.iter()
			.filter(|item| item.as_additional_data().is_some())
			.count();
		assert!(
			additional_data_digest_count <= 1,
			"block header contains multiple AdditionalData digest items"
		);
		let expected_hash: Option<[u8; 32]> = parent_header
			.digest()
			.logs()
			.iter()
			.find_map(|item| item.as_additional_data().copied());
		let map_opt: Option<AdditionalData> =
			additional_data_per_block.get(block_index).and_then(|opt| opt.clone());
		match (map_opt.is_some(), expected_hash.is_some()) {
			(true, false) => {
				panic!("additional data present but header digest missing AdditionalData item")
			},
			(false, true) => {
				panic!("header has AdditionalData digest but no additional data provided")
			},
			_ => {},
		}

		run_with_externalities_and_recorder::<B, _, _>(
			&backend,
			&mut Default::default(),
			&mut Default::default(),
			state_version,
			|| {
				E::verify_and_remove_seal(&mut block);
			},
		);

		// block is moved into the inner FnOnce, so wrap it in RefCell<Option<_>> so that
		// the outer `execute` closure can implement FnMut (required by
		// `ValidationHooks::execute_block`). The `.take()` panics on a second call, and the
		// assertion after the hook rejects a hook that never ran the block.
		let block = core::cell::RefCell::new(Some(block));
		let mut execute = || {
			run_with_externalities_and_recorder::<B, _, _>(
				&execute_backend,
				// Here is the only place where we want to use the recorder.
				// We want to ensure that we not accidentally read something from the proof, that
				// was not yet read and thus, alter the proof size. Otherwise, we end up with
				// mismatches in later blocks.
				&mut execute_recorder,
				&mut overlay,
				state_version,
				|| {
					E::execute_verified_block(
						block
							.borrow_mut()
							.take()
							.expect("the execute closure is invoked exactly once; qed"),
					);
				},
			);
		};
		hooks.execute_block(block_index, map_opt.as_ref(), &mut execute);
		assert!(
			block.borrow().is_none(),
			"`ValidationHooks::execute_block` must execute the block"
		);

		let code_upgrade_detected = if version.system_version >= 3 {
			overlay.storage(well_known_keys::PENDING_CODE).is_some()
		} else {
			overlay.storage(well_known_keys::CODE).is_some()
		};
		if code_upgrade_detected && num_blocks > 1 {
			panic!(
				"When applying a runtime upgrade, only one block per PoV is allowed. Received {num_blocks}."
			)
		}
		run_with_externalities_and_recorder::<B, _, _>(
			&backend,
			&mut Default::default(),
			// We are only reading here, but need to know what the old block has written. Thus, we
			// are passing here the overlay.
			&mut overlay,
			state_version,
			|| {
				hooks.on_block_executed(
					block_index,
					block_index + 1 == num_blocks,
					&parent_header,
					&inputs.parent_head,
				)
			},
		);

		if block_index + 1 != num_blocks {
			let mut changes = overlay
				.drain_storage_changes(&backend, state_version)
				.expect("Failed to get drain storage changes from the overlay.");

			drop(backend);

			// We just forward the changes directly to our db.
			changes.transaction.drain().into_iter().for_each(|(_, (value, count))| {
				// We only care about inserts and not deletes.
				if count > 0 {
					db.insert(EMPTY_PREFIX, &value);

					let hash = HashingFor::<B>::hash(&value);
					seen_nodes.borrow_mut().insert(hash);
				}
			});
		}
	}
}
