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
use codec::{Decode, Encode};
use sp_runtime::traits::Block as BlockT;
use sp_trie::CompactProof;

/// Special prefix used by [`ParachainBlockData`] from version 1 and upwards to distinguish from the
/// unversioned legacy/v0 version.
const VERSIONED_PARACHAIN_BLOCK_DATA_PREFIX: &[u8] = b"VERSIONEDPBD";

// Struct which allows prepending bytes after reading from an input.
pub(crate) struct PrependBytesInput<'a, I> {
	prepend: &'a [u8],
	read: usize,
	inner: &'a mut I,
}

impl<'a, I: codec::Input> codec::Input for PrependBytesInput<'a, I> {
	fn remaining_len(&mut self) -> Result<Option<usize>, codec::Error> {
		let remaining_compact = self.prepend.len().saturating_sub(self.read);
		Ok(self.inner.remaining_len()?.map(|len| len.saturating_add(remaining_compact)))
	}

	fn read(&mut self, into: &mut [u8]) -> Result<(), codec::Error> {
		if into.is_empty() {
			return Ok(());
		}

		let remaining_compact = self.prepend.len().saturating_sub(self.read);
		if remaining_compact > 0 {
			let to_read = into.len().min(remaining_compact);
			into[..to_read].copy_from_slice(&self.prepend[self.read..][..to_read]);
			self.read += to_read;

			if to_read < into.len() {
				// Buffer not full, keep reading the inner.
				self.inner.read(&mut into[to_read..])
			} else {
				// Buffer was filled by the bytes.
				Ok(())
			}
		} else {
			// Prepended bytes has been read, just read from inner.
			self.inner.read(into)
		}
	}
}

/// The parachain block that is created by a collator.
///
/// This is send as PoV (proof of validity block) to the relay-chain validators. There it will be
/// passed to the parachain validation Wasm blob to be validated.
#[derive(Clone)]
pub enum ParachainBlockData<Block: BlockT> {
	V0 { block: [Block; 1], proof: CompactProof },
	V1 { blocks: Vec<Block>, proof: CompactProof },
}

impl<Block: BlockT> Encode for ParachainBlockData<Block> {
	fn encode(&self) -> Vec<u8> {
		match self {
			Self::V0 { block, proof } =>
				(block[0].header(), block[0].extrinsics(), &proof).encode(),
			Self::V1 { blocks, proof } => {
				let mut res = VERSIONED_PARACHAIN_BLOCK_DATA_PREFIX.to_vec();
				1u8.encode_to(&mut res);
				blocks.encode_to(&mut res);
				proof.encode_to(&mut res);
				res
			},
		}
	}
}

impl<Block: BlockT> Decode for ParachainBlockData<Block> {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let mut prefix = [0u8; VERSIONED_PARACHAIN_BLOCK_DATA_PREFIX.len()];
		input.read(&mut prefix)?;

		if prefix == VERSIONED_PARACHAIN_BLOCK_DATA_PREFIX {
			match input.read_byte()? {
				1 => {
					let blocks = Vec::<Block>::decode(input)?;
					let proof = CompactProof::decode(input)?;

					Ok(Self::V1 { blocks, proof })
				},
				_ => Err("Unknown `ParachainBlockData` version".into()),
			}
		} else {
			let mut input = PrependBytesInput { prepend: &prefix, read: 0, inner: input };
			let header = Block::Header::decode(&mut input)?;
			let extrinsics = Vec::<Block::Extrinsic>::decode(&mut input)?;
			let proof = CompactProof::decode(&mut input)?;

			Ok(Self::V0 { block: [Block::new(header, extrinsics)], proof })
		}
	}
}

impl<Block: BlockT> ParachainBlockData<Block> {
	/// Creates a new instance of `Self`.
	pub fn new(blocks: Vec<Block>, proof: CompactProof) -> Self {
		Self::V1 { blocks, proof }
	}

	/// Returns references to the stored blocks.
	pub fn blocks(&self) -> &[Block] {
		match self {
			Self::V0 { block, .. } => &block[..],
			Self::V1 { blocks, .. } => &blocks,
		}
	}

	/// Returns mutable references to the stored blocks.
	pub fn blocks_mut(&mut self) -> &mut [Block] {
		match self {
			Self::V0 { ref mut block, .. } => block,
			Self::V1 { ref mut blocks, .. } => blocks,
		}
	}

	/// Returns the stored blocks.
	pub fn into_blocks(self) -> Vec<Block> {
		match self {
			Self::V0 { block, .. } => block.into_iter().collect(),
			Self::V1 { blocks, .. } => blocks,
		}
	}

	/// Returns a reference to the stored proof.
	pub fn proof(&self) -> &CompactProof {
		match self {
			Self::V0 { proof, .. } => &proof,
			Self::V1 { proof, .. } => proof,
		}
	}

	/// Deconstruct into the inner parts.
	pub fn into_inner(self) -> (Vec<Block>, CompactProof) {
		match self {
			Self::V0 { block, proof } => (block.into_iter().collect(), proof),
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

	/// Converts into [`ParachainBlockData::V0`].
	///
	/// Returns `None` if there is not exactly one block.
	pub fn as_v0(&self) -> Option<Self> {
		match self {
			Self::V0 { .. } => Some(self.clone()),
			Self::V1 { blocks, proof } => {
				if blocks.len() != 1 {
					return None
				}

				blocks
					.first()
					.map(|block| Self::V0 { block: [block.clone()], proof: proof.clone() })
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::testing::*;

	#[derive(codec::Encode, codec::Decode, Clone, PartialEq, Debug)]
	struct ParachainBlockDataV0<B: BlockT> {
		/// The header of the parachain block.
		pub header: B::Header,
		/// The extrinsics of the parachain block.
		pub extrinsics: alloc::vec::Vec<B::Extrinsic>,
		/// The data that is required to emulate the storage accesses executed by all extrinsics.
		pub storage_proof: sp_trie::CompactProof,
	}

	type TestExtrinsic = TestXt<MockCallU64, ()>;
	type TestBlock = Block<TestExtrinsic>;

	#[test]
	fn decoding_encoding_v0_works() {
		let v0 = ParachainBlockDataV0::<TestBlock> {
			header: Header::new_from_number(10),
			extrinsics: vec![
				TestExtrinsic::new_bare(MockCallU64(10)),
				TestExtrinsic::new_bare(MockCallU64(100)),
			],
			storage_proof: CompactProof { encoded_nodes: vec![vec![10u8; 200], vec![20u8; 30]] },
		};

		let encoded = v0.encode();
		let decoded = ParachainBlockData::<TestBlock>::decode(&mut &encoded[..]).unwrap();

		match &decoded {
			ParachainBlockData::V0 { block, proof } => {
				assert_eq!(v0.header, block[0].header);
				assert_eq!(v0.extrinsics, block[0].extrinsics);
				assert_eq!(&v0.storage_proof, proof);
			},
			_ => panic!("Invalid decoding"),
		}

		let encoded = decoded.as_v0().unwrap().encode();

		let decoded = ParachainBlockDataV0::<TestBlock>::decode(&mut &encoded[..]).unwrap();
		assert_eq!(decoded, v0);
	}

	#[test]
	fn decoding_encoding_v1_works() {
		let v1 = ParachainBlockData::<TestBlock>::V1 {
			blocks: vec![TestBlock::new(
				Header::new_from_number(10),
				vec![
					TestExtrinsic::new_bare(MockCallU64(10)),
					TestExtrinsic::new_bare(MockCallU64(100)),
				],
			)],
			proof: CompactProof { encoded_nodes: vec![vec![10u8; 200], vec![20u8; 30]] },
		};

		let encoded = v1.encode();
		let decoded = ParachainBlockData::<TestBlock>::decode(&mut &encoded[..]).unwrap();

		assert_eq!(v1.blocks(), decoded.blocks());
		assert_eq!(v1.proof(), decoded.proof());
	}
}
