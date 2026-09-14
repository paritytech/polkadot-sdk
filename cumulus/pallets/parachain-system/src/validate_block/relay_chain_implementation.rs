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

//! The relay-chain implementation of the validate block functionality.
//!
//! Thin layer over [`super::validate_block_core::execute_blocks`]: it owns everything that is
//! specific to validating on a relay chain and straddles the core call with it — the V3
//! scheduling shape validation before, the scheduling-signature override and the
//! `set_validation_data` re-check inside the two callbacks, the scheduling signal tail after,
//! and the final [`ValidationResult`] assembly.

use super::{
	host_functions::run_with_externalities_and_recorder,
	scheduling,
	trie_cache::CacheProvider,
	trie_recorder::SizeOnlyRecorderProvider,
	validate_block_core::{execute_blocks, SharedValidationInputs},
	MemoryOptimizedValidationParams,
};
use alloc::vec::Vec;
use cumulus_primitives_core::{
	relay_chain::{BlockNumber as RNumber, Hash as RHash, Header as RelayChainHeader},
	ParachainBlockData, PersistedValidationData, SignedSchedulingInfo, VerifySchedulingSignature,
};
use frame_support::traits::{ExecuteBlock, Get, IsSubType};
use polkadot_parachain_primitives::primitives::ValidationResult;
use sp_core::storage::StateVersion;
use sp_io::hashing::blake2_128;
use sp_runtime::traits::{
	Block as BlockT, ExtrinsicCall, HashingFor, Header as HeaderT, LazyBlock,
};
use sp_state_machine::{TrieBackend, TrieBackendBuilder};
use sp_trie::MemoryDB;

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
/// 3. We construct the sparse in-memory database from the storage proof inside the block data
/// and then ensure that the storage root matches the storage root in the `parent_head`.
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
///
/// Steps 1, 3, 4 and 5 live in [`super::validate_block_core::execute_blocks`]; this function
/// provides the relay-specific parts around it.
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

	// V3 scheduling validation (chain-shape only). Signature verification of
	// `signed_scheduling_info` happens in the `on_db_ready` closure below, once the core has
	// built the memory DB.
	let validated_scheduling = scheduling::validate_v3_scheduling(
		PSC::SchedulingSignatureVerifier::V3_SCHEDULING_ENABLED,
		&extension.0,
		block_data.scheduling_proof(),
		PSC::RelayParentOffset::get(),
		crate::Pallet::<PSC>::max_claim_queue_offset(),
	);

	// The override inputs (signed payload + the ISP header), present whenever the proof carried a
	// `signed_scheduling_info`. Wholly relay-side: computed here (before the DB exists) and
	// consumed after the core loop, so it never crosses the seam.
	let scheduling_override_inputs: Option<(SignedSchedulingInfo, RelayChainHeader)> =
		validated_scheduling.and_then(|validated| {
			validated
				.signed_scheduling_info
				.map(|signed_info| (signed_info, validated.internal_scheduling_parent_header))
		});

	let randomness_seed = build_seed_from_head_data::<B>(&block_data, relay_parent_storage_root);
	let mut partial = execute_blocks::<B, E, PSC>(
		SharedValidationInputs::<B> {
			block_data,
			parent_head: parachain_head,
			randomness_seed,
			relay_parent_storage_root: Some(relay_parent_storage_root),
			jam_anchor_state_root: None,
		},
		// Signature verification of the override needs the parachain state behind the relay
		// parent, which only exists inside an externalities scope over the just-built memory DB.
		// The core hands it over exactly here, together with the ORIGINAL parent header; passing
		// `None` (as JAM does) makes the override structurally unreachable.
		Some(&|db: &MemoryDB<HashingFor<B>>,
		       cache_provider: &CacheProvider<HashingFor<B>>,
		       parent_header: &B::Header,
		       state_version: StateVersion| {
			if let Some((signed_info, isp_header)) = scheduling_override_inputs.as_ref() {
				let relay_slot = scheduling::relay_slot_from_header(isp_header).expect(
					"internal_scheduling_parent header must carry a BABE pre-digest; \
					 the relay chain runs BABE; qed",
				);

				let parent_backend: TrieBackend<
					_,
					HashingFor<B>,
					_,
					SizeOnlyRecorderProvider<HashingFor<B>>,
				> = TrieBackendBuilder::new_with_cache(
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
		}),
		// The core fires this inside the externalities scope after each block's seal check, at
		// the exact point `ValidationData` is populated. Re-verify the block's committed
		// validation data against the relay-parent context.
		&|parent_head: &[u8]| {
			validate_validation_data(
				crate::ValidationData::<PSC>::get()
					.expect("`ValidationData` must be set after executing a block; qed"),
				parent_head,
				relay_parent_number,
				relay_parent_storage_root,
			);
		},
	);

	// A `signed_scheduling_info` overrides the block's emitted signals wholesale — they
	// are ignored, not merged.
	match scheduling_override_inputs.as_ref() {
		Some((signed_info, _)) => scheduling::SchedulingSignals::from_scheduling_info(
			signed_info,
			&mut partial.upward_messages,
		),
		None => scheduling::SchedulingSignals::from_block_signals(
			&partial.upward_message_signals,
			&mut partial.upward_messages,
		),
	}

	ValidationResult {
		head_data: partial.head_data.expect("HeadData not set"),
		new_validation_code: partial.new_validation_code.map(Into::into),
		upward_messages: partial.upward_messages,
		processed_downward_messages: partial.processed_downward_messages,
		horizontal_messages: partial.horizontal_messages,
		hrmp_watermark: partial.hrmp_watermark,
	}
}

/// Validates the given [`PersistedValidationData`] against the data from the relay chain.
///
/// There is no relay chain under JAM, so the comparisons carry no weight there:
/// `relay_parent_number` and `relay_parent_storage_root` are mirrored out of the block's own
/// `set_validation_data` before the call, and `parent_head` is established by the anchor state
/// proof rather than by the collator. See [`super::jam_implementation::jam_validate_block`].
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
