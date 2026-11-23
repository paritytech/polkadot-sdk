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
//! Thin layer over [`crate::validate_block::validate_block_core::execute_blocks`]: it owns
//! everything that is specific to validating on a relay chain and straddles the core call with
//! it — the V3 scheduling shape validation before, the scheduling-signature override and the
//! `set_validation_data` re-check and output reads inside [`RelayValidationHooks`], the
//! scheduling signal tail after, and the final [`ValidationResult`] assembly.

use super::scheduling;
use crate::validate_block::{
	host_functions::run_with_externalities_and_recorder,
	trie_cache::CacheProvider,
	trie_recorder::SizeOnlyRecorderProvider,
	validate_block_core::{execute_blocks, SharedValidationInputs, ValidationHooks},
	MemoryOptimizedValidationParams,
};
use alloc::vec::Vec;
use codec::Encode;
use core::marker::PhantomData;
use cumulus_primitives_core::{
	relay_chain::{
		BlockNumber as RNumber, Hash as RHash, Header as RelayChainHeader, UMP_SEPARATOR,
	},
	ParachainBlockData, PersistedValidationData, SignedSchedulingInfo, VerifySchedulingSignature,
};
use frame_support::traits::{ExecuteBlock, Get, IsSubType};
use polkadot_parachain_primitives::primitives::{
	HeadData, HorizontalMessages, UpwardMessages, ValidationResult,
};
use sp_core::storage::StateVersion;
use sp_io::hashing::blake2_128;
use sp_runtime::traits::{
	Block as BlockT, ExtrinsicCall, HashingFor, Header as HeaderT, LazyBlock,
};
use sp_state_machine::{TrieBackend, TrieBackendBuilder};
use sp_trie::MemoryDB;

/// The outputs collected from the executed blocks, before the scheduling tail is appended.
#[derive(Default)]
struct PartialValidationResult {
	head_data: Option<HeadData>,
	new_validation_code: Option<Vec<u8>>,
	upward_messages: UpwardMessages,
	/// The raw UMP signals the PoV emitted after the in-block `UMP_SEPARATOR`, in order. They
	/// become the scheduling tail unless a `signed_scheduling_info` overrides them.
	upward_message_signals: Vec<Vec<u8>>,
	processed_downward_messages: u32,
	horizontal_messages: HorizontalMessages,
	hrmp_watermark: RNumber,
}

/// The relay-chain [`ValidationHooks`]: verifies the scheduling-signature override once the
/// memory DB exists, and after each block re-checks its validation data against the relay-parent
/// context and collects its outputs into `result`.
struct RelayValidationHooks<PSC> {
	/// The signed payload and the internal scheduling parent header, present whenever the
	/// scheduling proof carried a `signed_scheduling_info`.
	scheduling_override: Option<(SignedSchedulingInfo, RelayChainHeader)>,
	relay_parent_number: RNumber,
	relay_parent_storage_root: RHash,
	result: PartialValidationResult,
	_config: PhantomData<PSC>,
}

impl<B: BlockT, PSC: crate::Config> ValidationHooks<B> for RelayValidationHooks<PSC> {
	/// Signature verification of the override needs the parachain state behind the relay parent,
	/// which only exists inside an externalities scope over the just-built memory DB.
	fn on_db_ready(
		&mut self,
		db: &MemoryDB<HashingFor<B>>,
		cache_provider: &CacheProvider<HashingFor<B>>,
		parent_header: &B::Header,
		state_version: StateVersion,
	) {
		let Some((signed_info, isp_header)) = self.scheduling_override.as_ref() else { return };
		let relay_slot = scheduling::relay_slot_from_header(isp_header).expect(
			"internal_scheduling_parent header must carry a BABE pre-digest; \
			 the relay chain runs BABE; qed",
		);

		let parent_backend: TrieBackend<
			_,
			HashingFor<B>,
			_,
			SizeOnlyRecorderProvider<HashingFor<B>>,
		> = TrieBackendBuilder::new_with_cache(db, *parent_header.state_root(), cache_provider)
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

	fn on_block_executed(
		&mut self,
		_index: usize,
		is_last: bool,
		header: &B::Header,
		parent_head: &[u8],
	) {
		validate_validation_data(
			crate::ValidationData::<PSC>::get()
				.expect("`ValidationData` must be set after executing a block; qed"),
			parent_head,
			self.relay_parent_number,
			self.relay_parent_storage_root,
		);

		let result = &mut self.result;
		result.new_validation_code =
			result.new_validation_code.take().or(crate::NewValidationCode::<PSC>::get());

		let mut found_separator = false;
		crate::UpwardMessages::<PSC>::get()
			.into_iter()
			.filter_map(|m| {
				// Filter out the `UMP_SEPARATOR` and the `UMPSignals`.
				if m == UMP_SEPARATOR {
					found_separator = true;
					None
				} else if found_separator {
					result.upward_message_signals.push(m);
					None
				} else {
					// No signal or separator
					Some(m)
				}
			})
			.for_each(|m| {
				result.upward_messages.try_push(m).expect(
					"Number of upward messages should not be greater than `MAX_UPWARD_MESSAGE_NUM`",
				)
			});

		result.processed_downward_messages += crate::ProcessedDownwardMessages::<PSC>::get();
		result
			.horizontal_messages
			.try_extend(crate::HrmpOutboundMessages::<PSC>::get().into_iter())
			.expect(
				"Number of horizontal messages should not be greater than `MAX_HORIZONTAL_MESSAGE_NUM`",
			);
		result.hrmp_watermark = crate::HrmpWatermark::<PSC>::get();

		if is_last {
			result.head_data = Some(
				crate::CustomValidationHeadData::<PSC>::get()
					.map_or_else(|| HeadData(header.encode()), HeadData),
			);
		}
	}
}

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
/// Steps 1, 3, 4 and 5 live in [`crate::validate_block::validate_block_core::execute_blocks`];
/// this function provides the relay-specific parts around it.
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
	// `signed_scheduling_info` happens in `RelayValidationHooks::on_db_ready`, once the core has
	// built the memory DB.
	let validated_scheduling = scheduling::validate_v3_scheduling(
		PSC::SchedulingSignatureVerifier::V3_SCHEDULING_ENABLED,
		&extension.0,
		block_data.scheduling_proof(),
		PSC::RelayParentOffset::get(),
		crate::Pallet::<PSC>::max_claim_queue_offset(),
	);

	// The override inputs (signed payload + the ISP header), present whenever the proof carried a
	// `signed_scheduling_info`. Wholly relay-side: computed here (before the DB exists), verified
	// by the hooks and consumed after the core loop, so it never crosses the seam.
	let scheduling_override: Option<(SignedSchedulingInfo, RelayChainHeader)> =
		validated_scheduling.and_then(|validated| {
			validated
				.signed_scheduling_info
				.map(|signed_info| (signed_info, validated.internal_scheduling_parent_header))
		});

	let randomness_seed = build_seed_from_head_data::<B>(&block_data, relay_parent_storage_root);
	let mut hooks = RelayValidationHooks::<PSC> {
		scheduling_override,
		relay_parent_number,
		relay_parent_storage_root,
		result: Default::default(),
		_config: PhantomData,
	};
	// Relay-chain candidates carry no additional data, so the default `execute_block` runs each
	// block with nothing armed around it.
	execute_blocks::<B, E>(
		SharedValidationInputs::<B> { block_data, parent_head: parachain_head, randomness_seed },
		&<PSC as frame_system::Config>::Version::get(),
		&mut hooks,
	);
	let RelayValidationHooks { scheduling_override, result: mut partial, .. } = hooks;
	partial.horizontal_messages.sort_by(|a, b| a.recipient.cmp(&b.recipient));

	// A `signed_scheduling_info` overrides the block's emitted signals wholesale — they
	// are ignored, not merged.
	match scheduling_override.as_ref() {
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
