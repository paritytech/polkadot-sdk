// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

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
}

/// The parachain block that is created by a collator.
///
/// This is send as PoV (proof of validity block) to the relay-chain validators. There it will be
/// passed to the parachain validation Wasm blob to be validated.
#[derive(codec::Encode, codec::Decode, Clone)]
pub struct ParachainBlockData<Block: BlockT> {
	blocks: Vec<(Block, CompactProof)>,
}

impl<Block: BlockT> ParachainBlockData<Block> {
	/// Creates a new instance of `Self`.
	pub fn new(blocks: Vec<(Block, CompactProof)>) -> Self {
		Self { blocks }
	}

	/// Returns an iterator yielding references to the stored blocks.
	pub fn blocks(&self) -> impl Iterator<Item = &Block> {
		self.blocks.iter().map(|e| &e.0)
	}

	/// Returns an iterator yielding mutable references to the stored blocks.
	pub fn blocks_mut(&mut self) -> impl Iterator<Item = &mut Block> {
		self.blocks.iter_mut().map(|e| &mut e.0)
	}

	/// Returns an iterator yielding the stored blocks.
	pub fn into_blocks(self) -> impl Iterator<Item = Block> {
		self.blocks.into_iter().map(|d| d.0)
	}

	/// Returns an iterator yielding references to the stored proofs.
	pub fn proofs(&self) -> impl Iterator<Item = &CompactProof> {
		self.blocks.iter().map(|d| &d.1)
	}

	/// Deconstruct into the inner parts.
	pub fn into_inner(self) -> Vec<(Block, CompactProof)> {
		self.blocks
	}

	/// Log the size of the individual components (header, extrinsics, storage proof) as info.
	pub fn log_size_info(&self) {
		tracing::info!(
			target: "cumulus",
			"PoV size {{ header: {}kb, extrinsics: {}kb, storage_proof: {}kb }}",
			self.blocks().map(|b| b.header().encoded_size()).sum::<usize>() as f64 / 1024f64,
			self.blocks().map(|b| b.extrinsics().encoded_size()).sum::<usize>() as f64 / 1024f64,
			self.proofs().map(|p| p.encoded_size()).sum::<usize>() as f64 / 1024f64,
		);
	}

	/// Converts into [`v0::ParachainBlockData`].
	///
	/// Returns `None` if there is not exactly one block.
	pub fn as_v0(&self) -> Option<v0::ParachainBlockData<Block>> {
		if self.blocks.len() != 1 {
			return None
		}

		self.blocks.first().map(|(block, storage_proof)| {
			let (header, extrinsics) = block.clone().deconstruct();
			v0::ParachainBlockData { header, extrinsics, storage_proof: storage_proof.clone() }
		})
	}
}
