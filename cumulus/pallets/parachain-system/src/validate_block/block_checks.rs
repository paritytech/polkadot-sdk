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

//! Structural checks over the blocks carried by a PoV.
//!
//! These are pure predicates over the candidate: they need no execution state, so they live
//! apart from the validation engine in `validate_block_core`.

use codec::Encode;
use cumulus_primitives_core::{relay_chain::MAX_HEAD_DATA_SIZE, CumulusDigestItem};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, LazyBlock};

/// Validates that the given `blocks` form a valid chain, starting from `parent_header`.
pub(super) fn verify_blocks_form_chain<B: BlockT>(
	blocks: &[B::LazyBlock],
	parent_header: &B::Header,
) {
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
