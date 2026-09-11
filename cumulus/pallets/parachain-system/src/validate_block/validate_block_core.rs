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
//! This module implements everything the polkadot and the (future) JAM entry points share:
//! decoding the parent head, replacing the storage related host functions, building the
//! in-memory database from the block's storage proof, executing every block against it and
//! assembling the resulting head/message outputs into a [`PartialValidationResult`].
//!
//! Relay-chain-only concerns stay out of the seam and live in
//! [`super::polkadot_implementation`]: V3 scheduling shape validation, the scheduling-signature
//! override, `validate_validation_data` and the final `ValidationResult` assembly. The two
//! callbacks are the boundary:
//!
//! - `on_db_ready` receives the built memory DB, its cache provider, the ORIGINAL parent header and
//!   the state version at the exact moment the relay layer's V3 signature override needs them (the
//!   core's loop has not reassigned the parent header yet). JAM passes `None`, which makes
//!   scheduling-signature verification structurally unreachable.
//! - `on_block_validated` fires inside the `run_with_externalities_and_recorder` scope after each
//!   block's seal check, where the relay layer re-verifies the block's `set_validation_data`
//!   against the relay-parent context (JAM passes `|_| {}`).

#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
use super::host_functions::jam_data;
use super::{
	additional_data_reader::AdditionalDataReader,
	block_checks::verify_blocks_form_chain,
	bytes::Bytes,
	host_functions::{additional_data, run_with_externalities_and_recorder},
	trie_cache, trie_recorder,
};
use alloc::vec::Vec;
use codec::{Decode, Encode};
use cumulus_primitives_additional_data::RELAY_PROOF_KEY;
#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
use cumulus_primitives_additional_data::{JamProofReader, RelayStateReader, JAM_PROOF_KEY};
use cumulus_primitives_core::{
	relay_chain::{BlockNumber as RNumber, Hash as RHash, UMP_SEPARATOR},
	ParachainBlockData,
};
use frame_support::traits::{ExecuteBlock, Get};
#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
use jam_state_helpers::StateProof;
use polkadot_parachain_primitives::primitives::{HeadData, HorizontalMessages, UpwardMessages};
use sp_additional_data::AdditionalData;
#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
use sp_additional_data::{hash_value, AdditionalDataFinalizer};
use sp_core::storage::{well_known_keys, StateVersion};
use sp_runtime::traits::{
	Block as BlockT, Hash as HashT, HashingFor, Header as HeaderT, LazyBlock,
};
use sp_state_machine::OverlayedChanges;
use sp_trie::{HashDBT, MemoryDB, StorageProof, EMPTY_PREFIX};
use trie_recorder::{SeenNodes, SizeOnlyRecorderProvider};

/// The parachain service's id on the JAM chain: the single service that hosts this and every other
/// para's `ParaInfo` records. Phase-1 network constant — the service id is assigned when the
/// service is registered (it cannot live in a chain spec), so this must match the value the
/// network's genesis registered the parachain service under (and the collator's
/// `--jam-service-id`). The state-key derivation interleaves it, so a mismatch reads the wrong key
/// and every read panics.
#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
const PARACHAIN_SERVICE_ID: u32 = 5;

/// The `AdditionalDataFinalizer` committing the carried JAM state proof under `JAM_PROOF_KEY`.
///
/// The commitment is `sp_additional_data::hash_value` of the exact bytes the `JAM_PROOF_KEY`
/// entry carries in the additional-data map, so the digest recomputed on refine matches the one
/// committed at authoring (the omni-node's `JamProofFinalizer`). Implements [`RelayStateReader`]
/// trivially — JAM blocks carry no relay reads — so it can ride the relay-style
/// `additional_data::using` provider seam that `host_finalize_into` folds.
#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
struct JamProofFinalizer {
	commitment: [u8; 32],
}

#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
impl AdditionalDataFinalizer for JamProofFinalizer {
	fn finalize(&self) -> Option<[u8; 32]> {
		Some(self.commitment)
	}
}

#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
impl RelayStateReader for JamProofFinalizer {
	fn read(&self, _key: &[u8]) -> Option<Vec<u8>> {
		None
	}

	fn proof_size(&self) -> usize {
		0
	}
}

/// The input of a single [`execute_blocks`] call.
///
/// `block_data` is already decoded and `randomness_seed` already derived by the caller: the seed
/// hashes the relay-parent storage root alongside the block hashes, which is relay-specific, so
/// only its `[u8; 16]` result crosses the seam.
pub(super) struct SharedValidationInputs<B: BlockT> {
	pub block_data: ParachainBlockData<B::LazyBlock>,
	pub parent_head: Bytes,
	pub randomness_seed: [u8; 16],
	pub relay_parent_storage_root: Option<RHash>,
	/// Trusted JAM anchor state root the carried `JAM_PROOF_KEY` proof must verify against. `None`
	/// on the relay chain (polkadot), where there is no JAM anchor and the field is never read.
	#[cfg_attr(
		not(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64"))),
		allow(dead_code)
	)]
	pub jam_anchor_state_root: Option<[u8; 32]>,
}

/// The raw outcome of the validation engine, before relay-layer post-processing.
///
/// The relay layer finalizes it into a [`ValidationResult`]
/// (polkadot_parachain_primitives::primitives::ValidationResult): it appends the scheduling
/// signal tail to `upward_messages` (from `upward_message_signals` or the signed override,
/// whichever applies) and unwraps `head_data`/`new_validation_code`.
pub(super) struct PartialValidationResult {
	pub head_data: Option<HeadData>,
	pub new_validation_code: Option<Vec<u8>>,
	pub upward_messages: UpwardMessages,
	/// The raw UMP signals a PoV emitted after the in-block `UMP_SEPARATOR`, in order. The relay
	/// layer routes them into the scheduling tail; JAM has no scheduling and ignores them.
	pub upward_message_signals: Vec<Vec<u8>>,
	pub processed_downward_messages: u32,
	pub horizontal_messages: HorizontalMessages,
	pub hrmp_watermark: RNumber,
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
pub(super) fn execute_blocks<B: BlockT, E: ExecuteBlock<B>, PSC: crate::Config>(
	inputs: SharedValidationInputs<B>,
	on_db_ready: Option<
		&dyn Fn(
			&MemoryDB<HashingFor<B>>,
			&trie_cache::CacheProvider<HashingFor<B>>,
			&B::Header,
			StateVersion,
		),
	>,
	on_block_validated: &dyn Fn(&[u8]),
) -> PartialValidationResult {
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

	let mut processed_downward_messages = 0;
	let mut upward_messages = UpwardMessages::default();
	let mut upward_message_signals = Vec::<Vec<u8>>::new();
	let mut horizontal_messages = HorizontalMessages::default();
	let mut hrmp_watermark = Default::default();
	let mut head_data = None;
	let mut new_validation_code = None;
	let num_blocks = blocks.len();
	let state_version = <PSC as frame_system::Config>::Version::get().state_version();

	// Create the db
	let mut db = match proof.to_memory_db::<HashingFor<B>>(parent_header.state_root()) {
		Ok((db, _)) => db,
		Err(_) => panic!("Compact proof decoding failure."),
	};

	core::mem::drop(proof);

	let cache_provider = trie_cache::CacheProvider::new();
	let seen_nodes = SeenNodes::<HashingFor<B>>::default();

	// Hand the built DB to the relay layer at the exact moment its V3 scheduling override needs
	// it — before the loop below reassigns `parent_header`, so `&parent_header` here is still the
	// ORIGINAL parent header from `inputs.parent_head`.
	on_db_ready.map(|f| f(&db, &cache_provider, &parent_header, state_version));

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

		// Build the verifying provider from the relay-state proof carried in the additional-data
		// blob. The blob is the SCALE-encoding of `(root, proof)`; the carried root is *ignored* —
		// reads are verified against the trusted `relay_parent_storage_root` from the validation
		// params, so a candidate that recorded reads against a different root fails here. If the
		// blob is `None` (the block read no relay state), no provider is set and
		// `read_relay_chain_state`/`finalize` fall back to their empty/`None` results.
		//
		// A malformed blob, or a proof that does not verify against the trusted
		// `relay_parent_storage_root`, means the candidate recorded its relay reads against a
		// different root (a lying collator, or wrong validation params) — reject it loudly.
		let mut verify_provider: Option<AdditionalDataReader> = None;
		// Without a trusted relay-parent storage root no proof can be verified against anything,
		// so the reader is skipped entirely (JAM passes `None`; its blocks carry no relay reads).
		if let Some(root) = inputs.relay_parent_storage_root {
			verify_provider = map_opt.as_ref().map(|map| {
				let proof_bytes = map
					.get(RELAY_PROOF_KEY)
					.expect("additional data map (present) must contain the relay-proof entry");
				let (_, proof) = <(RHash, StorageProof)>::decode(&mut &proof_bytes[..])
					.expect("relay-proof entry must decode as (root, proof)");
				AdditionalDataReader::new(root, proof)
					.expect("additional data proof must verify against relay_parent_storage_root")
			});
		}

		let execute = || {
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
					E::execute_verified_block(block);
				},
			);
		};
		// Build the proof-backed JAM reader from the state proof carried in the additional-data
		// blob under `JAM_PROOF_KEY` (the SCALE-encoding of `(state_root, proof)`), verified
		// against the trusted `jam_anchor_state_root` from the JAM refine context. As with the
		// relay blob, the carried root is *ignored* — reads verify against the trusted root, so a
		// candidate that recorded its JAM reads against a different root fails at the first read.
		// A malformed blob, or a proof that cannot authenticate a key, panics inside `read`
		// (task 7's semantic) rather than silently serving `None`. Without a trusted anchor root no
		// proof can be verified against anything, so the reader is skipped entirely (polkadot
		// passes `None`; its blocks carry no JAM reads).
		//
		// The same entry also arms the digest finalizer: it commits `hash_value` of the exact
		// carried bytes, so `host_finalize_into` folds it into the same
		// `DigestItem::AdditionalData` the collator committed at authoring (task 9). Threading
		// the reader without the finalizer recomputes an empty digest — `frame_executive`'s
		// digest-count check then panics (3 vs 2).
		#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
		let mut jam_provider: Option<(JamProofReader, JamProofFinalizer)> = None;
		#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
		if let Some(root) = inputs.jam_anchor_state_root {
			jam_provider = map_opt.as_ref().map(|map| {
				let proof_bytes = map
					.get(JAM_PROOF_KEY)
					.expect("additional data map (present) must contain the jam-proof entry");
				let (_, proof) = <([u8; 32], StateProof)>::decode(&mut &proof_bytes[..])
					.expect("jam-proof entry must decode as (state_root, proof)");
				(
					JamProofReader::new(PARACHAIN_SERVICE_ID, root, proof),
					JamProofFinalizer { commitment: hash_value(proof_bytes) },
				)
			});
		}
		// Serve `read_relay_chain_state`/`finalize` from the verified proof for the duration of
		// execution, and on JAM the `jam_state_read` read from the proof-backed JAM reader and the
		// `finalize` fold from the JAM-proof finalizer (both armed together from the same entry).
		#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
		match (verify_provider.as_mut(), jam_provider.as_mut()) {
			(Some(vp), Some((jr, _))) => {
				additional_data::using(vp, || jam_data::using(jr, execute))
			},
			(Some(vp), None) => additional_data::using(vp, execute),
			(None, Some((jr, jf))) => additional_data::using(jf, || jam_data::using(jr, execute)),
			(None, None) => execute(),
		}
		#[cfg(not(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64"))))]
		match verify_provider.as_mut() {
			Some(vp) => additional_data::using(vp, execute),
			None => execute(),
		}

		let code_upgrade_detected =
			if <PSC as frame_system::Config>::Version::get().system_version >= 3 {
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
				// The relay layer re-verifies the block's validation data against its relay
				// context here, inside the externalities scope where `ValidationData` is set.
				on_block_validated(&inputs.parent_head);

				new_validation_code =
					new_validation_code.take().or(crate::NewValidationCode::<PSC>::get());

				let mut found_separator = false;
				crate::UpwardMessages::<PSC>::get()
					.into_iter()
					.filter_map(|m| {
						// Filter out the `UMP_SEPARATOR` and the `UMPSignals`.
						if m == UMP_SEPARATOR {
							found_separator = true;
							None
						} else if found_separator {
							upward_message_signals.push(m);
							None
						} else {
							// No signal or separator
							Some(m)
						}
					})
					.for_each(|m| {
						upward_messages.try_push(m).expect(
							"Number of upward messages should not be greater than `MAX_UPWARD_MESSAGE_NUM`",
						)
					});

				processed_downward_messages += crate::ProcessedDownwardMessages::<PSC>::get();
				horizontal_messages
					.try_extend(crate::HrmpOutboundMessages::<PSC>::get().into_iter())
					.expect(
						"Number of horizontal messages should not be greater than `MAX_HORIZONTAL_MESSAGE_NUM`",
					);
				hrmp_watermark = crate::HrmpWatermark::<PSC>::get();

				if block_index + 1 == num_blocks {
					head_data = Some(
						crate::CustomValidationHeadData::<PSC>::get()
							.map_or_else(|| HeadData(parent_header.encode()), HeadData),
					);
				}
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

	horizontal_messages.sort_by(|a, b| a.recipient.cmp(&b.recipient));

	PartialValidationResult {
		head_data,
		new_validation_code,
		upward_messages,
		upward_message_signals,
		processed_downward_messages,
		horizontal_messages,
		hrmp_watermark,
	}
}
