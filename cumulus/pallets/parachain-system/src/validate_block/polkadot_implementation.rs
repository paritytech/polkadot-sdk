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

//! The actual implementation of the validate block functionality.

use super::{
	additional_data_reader::AdditionalDataReader, scheduling, trie_cache, trie_recorder,
	MemoryOptimizedValidationParams,
};
use alloc::vec::Vec;
use codec::{Decode, Encode};
use cumulus_primitives_core::{
	relay_chain::{
		BlockNumber as RNumber, Hash as RHash, Header as RelayChainHeader, MAX_HEAD_DATA_SIZE,
		UMP_SEPARATOR,
	},
	CumulusDigestItem, ParachainBlockData, PersistedValidationData, SignedSchedulingInfo,
	VerifySchedulingSignature,
};
use frame_support::{
	traits::{ExecuteBlock, Get, IsSubType},
	BoundedVec,
};
use polkadot_parachain_primitives::primitives::{HeadData, ValidationResult};
use sp_core::storage::{well_known_keys, ChildInfo};
use sp_externalities::{set_and_run_with_externalities, Externalities};
use sp_io::{hashing::blake2_128, StorageIterations};
use sp_runtime::traits::{
	Block as BlockT, ExtrinsicCall, Hash as HashT, HashingFor, Header as HeaderT, LazyBlock,
};
use sp_state_machine::OverlayedChanges;
use sp_trie::{HashDBT, ProofSizeProvider, StorageProof, EMPTY_PREFIX};
use trie_recorder::{SeenNodes, SizeOnlyRecorderProvider};

type Ext<'a, Block, Backend> = sp_state_machine::Ext<'a, HashingFor<Block>, Backend>;

fn with_externalities<F: FnOnce(&mut dyn Externalities) -> R, R>(f: F) -> R {
	sp_externalities::with_externalities(f).expect("Environmental externalities not set.")
}

// Recorder instance to be used during this validate_block call.
environmental::environmental!(recorder: trait ProofSizeProvider);

// The verified relay-state reader is threaded into the replaced
// `read_relay_chain_state`/`finalize` host functions for the duration of block execution. This
// lives in its own module because `environmental!` emits a scope-level `GLOBAL` static that would
// otherwise collide with the `recorder` invocation above.
mod additional_data {
	use sp_additional_data::AdditionalDataProvider;
	environmental::environmental!(env: trait AdditionalDataProvider);
	pub(super) fn using<R, F: FnOnce() -> R>(t: &mut dyn AdditionalDataProvider, f: F) -> R {
		env::using(t, f)
	}
	pub(super) fn with<R, F: for<'a> FnOnce(&'a mut (dyn AdditionalDataProvider + 'a)) -> R>(
		f: F,
	) -> Option<R> {
		env::with(f)
	}
}

use sp_additional_data::AdditionalData;

/// Validate the given parachain block.
///
/// This function is doing roughly the following:
///
/// 1. We decode the [`ParachainBlockData`] from the `block_data` in `params`.
///
/// 2. We are doing some security checks like checking that the `parent_head` in `params`
/// is the parent of the block we are going to check. We also ensure that the `set_validation_data`
/// inherent is present in the block and that the validation data matches the values in `params`.
///
/// 3. We construct the sparse in-memory database from the storage proof inside the block data and
/// then ensure that the storage root matches the storage root in the `parent_head`.
///
/// 4. We replace all the storage related host functions with functions inside the wasm blob.
/// This means instead of calling into the host, we will stay inside the wasm execution. This is
/// very important as the relay chain validator hasn't the state required to verify the block. But
/// we have the in-memory database that contains all the values from the state of the parachain
/// that we require to verify the block.
///
/// 5. The last step is to execute the entire block in the machinery we just have setup. Executing
/// the blocks include running all transactions in the block against our in-memory database and
/// ensuring that the final storage root matches the storage root in the header of the block. In the
/// end we return back the [`ValidationResult`] with all the required information for the validator.
#[doc(hidden)]
pub fn validate_block<B: BlockT, E: ExecuteBlock<B>, PSC: crate::Config>(
	MemoryOptimizedValidationParams {
		block_data,
		parent_head: parachain_head,
		relay_parent_number,
		relay_parent_storage_root,
		extension,
	}: MemoryOptimizedValidationParams,
) -> ValidationResult
where
	B::Extrinsic: ExtrinsicCall,
	<B::Extrinsic as ExtrinsicCall>::Call: IsSubType<crate::Call<PSC>>,
{
	// Decode block data first - we need it for both scheduling validation and block execution
	let block_data = codec::decode_from_bytes::<ParachainBlockData<B::LazyBlock>>(block_data)
		.expect("Invalid parachain block data");

	let _guard = (
		// Replace storage calls with our own implementations
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
		sp_additional_data::additional_data::host_read_relay_chain_state_into
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
	);

	// V3 scheduling validation (chain-shape only). Signature verification of
	// `signed_scheduling_info` happens here at the call site so the verifier wiring
	// stays out of the pure shape check.
	let validated_scheduling = scheduling::validate_v3_scheduling(
		PSC::SchedulingSignatureVerifier::V3_SCHEDULING_ENABLED,
		&extension.0,
		block_data.scheduling_proof(),
		PSC::RelayParentOffset::get(),
		crate::Pallet::<PSC>::max_claim_queue_offset(),
	);

	// The override inputs (signed payload + the ISP header), present whenever the proof carried a
	// `signed_scheduling_info`. The signature is verified later, inside the externalities scope
	// below, since it needs to read `Authorities::<T>` from parachain state.
	let scheduling_override_inputs: Option<(SignedSchedulingInfo, RelayChainHeader)> =
		validated_scheduling.and_then(|validated| {
			validated
				.signed_scheduling_info
				.map(|signed_info| (signed_info, validated.internal_scheduling_parent_header))
		});

	// Initialize hashmaps randomness.
	sp_trie::add_extra_randomness(build_seed_from_head_data::<B>(
		&block_data,
		relay_parent_storage_root,
	));

	let mut parent_header =
		codec::decode_from_bytes::<B::Header>(parachain_head.clone()).expect("Invalid parent head");

	let additional_data_per_block: Vec<Option<AdditionalData>> =
		block_data.additional_data().to_vec();
	let (blocks, proof) = block_data.into_inner();

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
	let mut upward_messages = BoundedVec::default();
	let mut upward_message_signals = Vec::<Vec<_>>::new();
	let mut horizontal_messages = BoundedVec::default();
	let mut hrmp_watermark = Default::default();
	let mut head_data = None;
	let mut new_validation_code = None;
	let num_blocks = blocks.len();
	let state_version = <PSC as frame_system::Config>::Version::get().state_version();

	// Create the db
	let mut db = match proof.to_memory_db(Some(parent_header.state_root())) {
		Ok((db, _)) => db,
		Err(_) => panic!("Compact proof decoding failure."),
	};

	core::mem::drop(proof);

	let cache_provider = trie_cache::CacheProvider::new();
	let seen_nodes = SeenNodes::<HashingFor<B>>::default();

	// Verify the V3 scheduling signature override. Only set up the backend and externalities
	// when there's actually an override to check.
	if let Some((signed_info, isp_header)) = scheduling_override_inputs.as_ref() {
		let relay_slot = scheduling::relay_slot_from_header(isp_header).expect(
			"internal_scheduling_parent header must carry a BABE pre-digest; \
			 the relay chain runs BABE; qed",
		);

		let parent_backend: sp_state_machine::TrieBackend<
			_,
			HashingFor<B>,
			_,
			SizeOnlyRecorderProvider<HashingFor<B>>,
		> = sp_state_machine::TrieBackendBuilder::new_with_cache(
			&db,
			*parent_header.state_root(),
			&cache_provider,
		)
		.build();
		run_with_externalities_and_recorder::<B, _, _>(
			&parent_backend,
			&mut Default::default(),
			&mut Default::default(),
			state_version,
			|| {
				if !PSC::SchedulingSignatureVerifier::verify(signed_info, relay_slot) {
					panic!(
						"V3 scheduling validation failed: invalid \
						 signed_scheduling_info (ISP: {:?})",
						isp_header.hash(),
					);
				}
			},
		);
	}

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

		// Integrity: the carried map must hash to the header's committed digest. Checked up front
		// so a tampered map is rejected by this explicit, named assertion rather than implicitly
		// later by `frame_executive`'s digest-item equality (which would panic with a less
		// specific message).
		if let Some(ref map) = map_opt {
			assert_eq!(
				sp_additional_data::hash(map),
				expected_hash.expect("checked above that both are Some; qed"),
				"additional data hash does not match header digest"
			);
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
		let mut verify_provider: Option<AdditionalDataReader> = map_opt.as_ref().map(|map| {
			let proof_bytes = map
				.get(sp_additional_data::RELAY_PROOF_KEY)
				.expect("additional data map (present) must contain the relay-proof entry");
			let (_, proof) = <(RHash, StorageProof)>::decode(&mut &proof_bytes[..])
				.expect("relay-proof entry must decode as (root, proof)");
			AdditionalDataReader::new(relay_parent_storage_root, proof)
				.expect("additional data proof must verify against relay_parent_storage_root")
		});

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
		// Serve `read_relay_chain_state`/`finalize` from the verified proof for the duration of
		// execution.
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
				// Ensure the validation data is correct.
				validate_validation_data(
					crate::ValidationData::<PSC>::get()
						.expect("`ValidationData` must be set after executing a block; qed"),
					&parachain_head,
					relay_parent_number,
					relay_parent_storage_root,
				);

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

	// A `signed_scheduling_info` overrides the block's emitted signals wholesale — they
	// are ignored, not merged.
	match scheduling_override_inputs.as_ref() {
		Some((signed_info, _)) => {
			scheduling::SchedulingSignals::from_scheduling_info(signed_info, &mut upward_messages)
		},
		None => scheduling::SchedulingSignals::from_block_signals(
			&upward_message_signals,
			&mut upward_messages,
		),
	}

	horizontal_messages.sort_by(|a, b| a.recipient.cmp(&b.recipient));

	ValidationResult {
		head_data: head_data.expect("HeadData not set"),
		new_validation_code: new_validation_code.map(Into::into),
		upward_messages,
		processed_downward_messages,
		horizontal_messages,
		hrmp_watermark,
	}
}

/// Validates the given [`PersistedValidationData`] against the data from the relay chain.
fn validate_validation_data(
	validation_data: PersistedValidationData,
	parent_header: &[u8],
	relay_parent_number: RNumber,
	relay_parent_storage_root: RHash,
) {
	assert_eq!(parent_header, &validation_data.parent_head.0, "Parent head doesn't match");
	assert_eq!(
		relay_parent_number, validation_data.relay_parent_number,
		"Relay parent number doesn't match",
	);
	assert_eq!(
		relay_parent_storage_root, validation_data.relay_parent_storage_root,
		"Relay parent storage root doesn't match",
	);
}

fn verify_blocks_form_chain<B: BlockT>(blocks: &[B::LazyBlock], parent_header: &B::Header) {
	let num_blocks = blocks.len();

	// Check first block's parent matches the given parent_header
	assert_eq!(
		*blocks
			.first()
			.expect("BlockData should have at least one block")
			.header()
			.parent_hash(),
		parent_header.hash(),
		"Parachain head needs to be the parent of the first block"
	);

	let mut first_block_has_bundle_info: Option<bool> = None;

	blocks.iter().enumerate().fold(
		parent_header.hash(),
		|expected_parent, (block_index, block)| {
			// Check chain validity
			assert_eq!(
				expected_parent,
				*block.header().parent_hash(),
				"Not a valid chain of blocks :(; {:?} not a parent of {:?}?",
				array_bytes::bytes2hex("0x", expected_parent.as_ref()),
				array_bytes::bytes2hex("0x", block.header().parent_hash().as_ref()),
			);

			let encoded_header_size = block.header().encoded_size();
			assert!(
				encoded_header_size <= MAX_HEAD_DATA_SIZE as usize,
				"Header size {encoded_header_size} exceeds MAX_HEAD_DATA_SIZE {MAX_HEAD_DATA_SIZE}",
			);

			// Validate BlockBundleInfo consistency
			let bundle_info = CumulusDigestItem::find_block_bundle_info(block.header().digest());
			match (first_block_has_bundle_info, &bundle_info) {
				(None, info) => {
					first_block_has_bundle_info = Some(info.is_some());
				},
				(Some(true), None) => {
					panic!("All blocks in a bundled PoV must include `BlockBundleInfo`");
				},
				(Some(false), _) => {
					panic!("A PoV without `BlockBundleInfo` may only contain a single block");
				},
				_ => {},
			}

			if let Some(ref info) = bundle_info {
				assert_eq!(
					info.index as usize, block_index,
					"BlockBundleInfo index mismatch: expected {block_index}, got {}",
					info.index
				);

				if block_index + 1 < num_blocks {
					assert!(
						!CumulusDigestItem::is_last_block_in_core(block.header().digest()).unwrap_or(false),
						"Intermediate block at index {block_index} is marked as last block in core, \
						but more blocks follow in the PoV",
					);
				} else if !CumulusDigestItem::is_last_block_in_core(block.header().digest())
					.unwrap_or(true)
				{
					panic!(
						"Last block in PoV must include the digest that marks it as the last block in the core"
					);
				}
			}

			block.header().hash()
		},
	);
}

/// Build a seed from the head data of the parachain block.
///
/// Uses both the relay parent storage root and the hash of the blocks
/// in the block data, to make sure the seed changes every block and that
/// the user cannot find about it ahead of time.
fn build_seed_from_head_data<B: BlockT>(
	block_data: &ParachainBlockData<B::LazyBlock>,
	relay_parent_storage_root: crate::relay_chain::Hash,
) -> [u8; 16] {
	let mut bytes_to_hash = Vec::with_capacity(
		block_data.blocks().len() * size_of::<B::Hash>() + size_of::<crate::relay_chain::Hash>(),
	);

	bytes_to_hash.extend_from_slice(relay_parent_storage_root.as_ref());
	block_data.blocks().iter().for_each(|block| {
		bytes_to_hash.extend_from_slice(block.header().hash().as_ref());
	});

	blake2_128(&bytes_to_hash)
}

/// Run the given closure with the externalities and recorder set.
fn run_with_externalities_and_recorder<Block: BlockT, R, F: FnOnce() -> R>(
	backend: &impl sp_state_machine::Backend<HashingFor<Block>>,
	recorder: &mut SizeOnlyRecorderProvider<HashingFor<Block>>,
	overlay: &mut OverlayedChanges<HashingFor<Block>>,
	state_version: sp_core::storage::StateVersion,
	execute: F,
) -> R {
	let mut ext = Ext::<Block, _>::new(overlay, backend).with_state_version(state_version);

	recorder::using(recorder, || set_and_run_with_externalities(&mut ext, || execute()))
}

fn host_storage_read(
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
	// The relay-read proof rides in the PoV outside the block body; count it here too so the
	// runtime's proof-size accounting (weight-reclaim) budgets for the full PoV. Symmetric with the
	// build side, which adds `AdditionalDataExt`'s size to `storage_proof_size`.
	let relay = additional_data::with(|p| p.proof_size()).unwrap_or(0);
	(para + relay) as _
}

fn host_storage_root(out: &mut [u8]) {
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

fn host_storage_clear_prefix(
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

fn host_storage_append(key: &[u8], value: Vec<u8>) {
	with_externalities(|ext| ext.storage_append(key.to_vec(), value))
}

fn host_storage_next_key(key_in: &[u8], key_out: &mut [u8]) -> u32 {
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

fn host_default_child_storage_read(
	storage_key: &[u8],
	key: &[u8],
	value_out: &mut [u8],
	value_offset: u32,
	allow_partial: u32,
) -> Option<u32> {
	let child_info = ChildInfo::new_default(storage_key);
	match with_externalities(|ext| ext.child_storage(&child_info, key)) {
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

fn host_default_child_storage_storage_kill(
	storage_key: &[u8],
	maybe_limit: Option<u32>,
	maybe_cursor_in: Option<&[u8]>,
	maybe_cursor_out: &mut [u8],
	counters: &mut StorageIterations,
) -> u32 {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let removal_results = ext.kill_child_storage(&child_info, maybe_limit, maybe_cursor_in);
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

fn host_default_child_storage_exists(storage_key: &[u8], key: &[u8]) -> bool {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| ext.exists_child_storage(&child_info, key))
}

fn host_default_child_storage_clear_prefix(
	storage_key: &[u8],
	prefix: &[u8],
	maybe_limit: Option<u32>,
	maybe_cursor_in: Option<&[u8]>,
	maybe_cursor_out: &mut [u8],
	counters: &mut StorageIterations,
) -> u32 {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let removal_results =
			ext.clear_child_prefix(&child_info, prefix, maybe_limit, maybe_cursor_in);
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

fn host_default_child_storage_root(storage_key: &[u8], out: &mut [u8]) {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let root = ext.child_storage_root(&child_info);
		let encoded = root.encode();
		let write_len = encoded.len().min(out.len());
		out[..write_len].copy_from_slice(&encoded[..write_len]);
	})
}

fn host_default_child_storage_next_key(
	storage_key: &[u8],
	key_in: &[u8],
	key_out: &mut [u8],
) -> u32 {
	let child_info = ChildInfo::new_default(storage_key);
	with_externalities(|ext| {
		let next_key = ext.next_child_storage_key(&child_info, key_in);
		let next_key_len = next_key.as_ref().map(|k| k.len()).unwrap_or(0);
		if let Some(next_key) = next_key {
			let write_len = next_key.len().min(key_out.len());
			key_out[..write_len].copy_from_slice(&next_key[..write_len]);
		}
		next_key_len as u32
	})
}

#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
fn host_misc_last_cursor(out: &mut [u8]) -> Option<u32> {
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
fn host_offchain_index_set(_key: &[u8], _value: &[u8]) {}

#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
fn host_offchain_index_clear(_key: &[u8]) {}

fn host_read_relay_chain_state_into(key: &[u8], value_out: &mut [u8]) -> i64 {
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

fn host_finalize_into(hash_out: &mut [u8]) -> u32 {
	match additional_data::with(|p| p.finalize()).flatten() {
		Some(h) => {
			hash_out[..32].copy_from_slice(&h);
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
