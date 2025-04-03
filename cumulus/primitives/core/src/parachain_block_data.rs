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

//! Provides [`ParachainBlockData`] and its historical versions.

use alloc::vec::Vec;
use codec::Encode;
use sp_runtime::traits::Block as BlockT;
use sp_trie::CompactProof;

pub mod v0 {
	use super::*;

	#[derive(codec::Encode, codec::Decode, Clone)]
	pub struct ParachainBlockData<B: BlockT> {
		/// The header of the parachain block.
		pub header: B::Header,
		/// The extrinsics of the parachain block.
		pub extrinsics: alloc::vec::Vec<B::Extrinsic>,
		/// The data that is required to emulate the storage accesses executed by all extrinsics.
		pub storage_proof: sp_trie::CompactProof,
	}

	impl<Block: BlockT> From<ParachainBlockData<Block>> for super::ParachainBlockData<Block> {
		fn from(block_data: ParachainBlockData<Block>) -> Self {
			Self::new(
				alloc::vec![Block::new(block_data.header, block_data.extrinsics)],
				block_data.storage_proof,
			)
		}
	}
}

/// The parachain block that is created by a collator.
///
/// This is send as PoV (proof of validity block) to the relay-chain validators. There it will be
/// passed to the parachain validation Wasm blob to be validated.
#[derive(codec::Encode, codec::Decode, Clone)]
pub enum ParachainBlockData<Block: BlockT> {
	#[codec(index = 1)]
	V1 { blocks: Vec<Block>, proof: CompactProof },
}

impl<Block: BlockT> ParachainBlockData<Block> {
	/// Creates a new instance of `Self`.
	pub fn new(blocks: Vec<Block>, proof: CompactProof) -> Self {
		Self::V1 { blocks, proof }
	}

	/// Returns references to the stored blocks.
	pub fn blocks(&self) -> &[Block] {
		match self {
			Self::V1 { blocks, .. } => &blocks,
		}
	}

	/// Returns mutable references to the stored blocks.
	pub fn blocks_mut(&mut self) -> &mut [Block] {
		match self {
			Self::V1 { ref mut blocks, .. } => blocks,
		}
	}

	/// Returns the stored blocks.
	pub fn into_blocks(self) -> Vec<Block> {
		match self {
			Self::V1 { blocks, .. } => blocks,
		}
	}

	/// Returns a reference to the stored proof.
	pub fn proof(&self) -> &CompactProof {
		match self {
			Self::V1 { proof, .. } => proof,
		}
	}

	/// Deconstruct into the inner parts.
	pub fn into_inner(self) -> (Vec<Block>, CompactProof) {
		match self {
			Self::V1 { blocks, proof } => (blocks, proof),
		}
	}

	/// Log the size of the individual components (header, extrinsics, storage proof) as info.
	pub fn log_size_info(&self) {
		tracing::info!(
			target: "cumulus",
			header_kb = %self.blocks().iter().map(|b| b.header().encoded_size()).sum::<usize>() as f64 / 1024f64,
			extrinsics_kb = %self.blocks().iter().map(|b| b.extrinsics().encoded_size()).sum::<usize>() as f64 / 1024f64,
			storage_proof_kb = %self.proof().encoded_size() as f64 / 1024f64,
			"PoV size",
		);
	}

	/// Converts into [`v0::ParachainBlockData`].
	///
	/// Returns `None` if there is not exactly one block.
	pub fn as_v0(&self) -> Option<v0::ParachainBlockData<Block>> {
		match self {
			Self::V1 { blocks, proof } => {
				if blocks.len() != 1 {
					return None
				}

				blocks.first().map(|block| {
					let (header, extrinsics) = block.clone().deconstruct();
					v0::ParachainBlockData { header, extrinsics, storage_proof: proof.clone() }
				})
			},
		}
	}
}
