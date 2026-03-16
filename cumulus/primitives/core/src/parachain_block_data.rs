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
use polkadot_parachain_primitives::primitives::MAX_REQUIRES_COMMITMENT_NUM;
use polkadot_primitives_speculative_messaging::LateBlockProof;
use sp_runtime::traits::Block as BlockT;
use sp_trie::CompactProof;

/// Maximum number of late block proofs allowed in a V2 `ParachainBlockData`.
///
/// Each late block proof corresponds to at most one requires commitment, so
/// the bound mirrors [`MAX_REQUIRES_COMMITMENT_NUM`]. Decoding will reject
/// payloads that exceed this limit to prevent memory exhaustion from
/// untrusted PoV data.
const MAX_LATE_BLOCK_PROOFS: u32 = MAX_REQUIRES_COMMITMENT_NUM;

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
pub enum ParachainBlockData<Block> {
	V0 { block: [Block; 1], proof: CompactProof },
	V1 { blocks: Vec<Block>, proof: CompactProof },
	V2 { blocks: Vec<Block>, proof: CompactProof, late_block_proofs: Vec<LateBlockProof> },
}

impl<Block: Encode> Encode for ParachainBlockData<Block> {
	fn encode(&self) -> Vec<u8> {
		match self {
			Self::V0 { block, proof } => (&block[0], &proof).encode(),
			Self::V1 { blocks, proof } => {
				let mut res = VERSIONED_PARACHAIN_BLOCK_DATA_PREFIX.to_vec();
				1u8.encode_to(&mut res);
				blocks.encode_to(&mut res);
				proof.encode_to(&mut res);
				res
			},
			Self::V2 { blocks, proof, late_block_proofs } => {
				let mut res = VERSIONED_PARACHAIN_BLOCK_DATA_PREFIX.to_vec();
				2u8.encode_to(&mut res);
				blocks.encode_to(&mut res);
				proof.encode_to(&mut res);
				late_block_proofs.encode_to(&mut res);
				res
			},
		}
	}
}

impl<Block: Decode> Decode for ParachainBlockData<Block> {
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
				2 => {
					let blocks = Vec::<Block>::decode(input)?;
					let proof = CompactProof::decode(input)?;
					let late_block_proofs = Vec::<LateBlockProof>::decode(input)?;

					if late_block_proofs.len() > MAX_LATE_BLOCK_PROOFS as usize {
						return Err(
							"Too many late block proofs in ParachainBlockData V2".into(),
						);
					}

					Ok(Self::V2 { blocks, proof, late_block_proofs })
				},
				_ => Err("Unknown `ParachainBlockData` version".into()),
			}
		} else {
			let mut input = PrependBytesInput { prepend: &prefix, read: 0, inner: input };
			let block = Block::decode(&mut input)?;
			let proof = CompactProof::decode(&mut input)?;

			Ok(Self::V0 { block: [block], proof })
		}
	}
}

impl<Block> ParachainBlockData<Block> {
	/// Creates a new instance of `Self`.
	pub fn new(blocks: Vec<Block>, proof: CompactProof) -> Self {
		Self::V1 { blocks, proof }
	}

	/// Creates a new V2 instance with late block proofs for speculative messaging.
	pub fn new_with_late_block_proofs(
		blocks: Vec<Block>,
		proof: CompactProof,
		late_block_proofs: Vec<LateBlockProof>,
	) -> Self {
		Self::V2 { blocks, proof, late_block_proofs }
	}

	/// Returns references to the stored blocks.
	pub fn blocks(&self) -> &[Block] {
		match self {
			Self::V0 { block, .. } => &block[..],
			Self::V1 { blocks, .. } | Self::V2 { blocks, .. } => &blocks,
		}
	}

	/// Returns mutable references to the stored blocks.
	pub fn blocks_mut(&mut self) -> &mut [Block] {
		match self {
			Self::V0 { ref mut block, .. } => block,
			Self::V1 { ref mut blocks, .. } | Self::V2 { ref mut blocks, .. } => blocks,
		}
	}

	/// Returns the stored blocks.
	pub fn into_blocks(self) -> Vec<Block> {
		match self {
			Self::V0 { block, .. } => block.into_iter().collect(),
			Self::V1 { blocks, .. } | Self::V2 { blocks, .. } => blocks,
		}
	}

	/// Returns a reference to the stored proof.
	pub fn proof(&self) -> &CompactProof {
		match self {
			Self::V0 { proof, .. } | Self::V1 { proof, .. } | Self::V2 { proof, .. } => proof,
		}
	}

	/// Returns the late block proofs, if any.
	///
	/// V0 and V1 always return an empty slice. V2 returns the proofs
	/// included by the collator for speculative messaging timing mismatches.
	pub fn late_block_proofs(&self) -> &[LateBlockProof] {
		match self {
			Self::V0 { .. } | Self::V1 { .. } => &[],
			Self::V2 { late_block_proofs, .. } => late_block_proofs,
		}
	}

	/// Deconstruct into the inner parts.
	///
	/// Returns `(blocks, proof, late_block_proofs)`. For V0/V1, late block proofs
	/// will be an empty vec.
	pub fn into_inner(self) -> (Vec<Block>, CompactProof, Vec<LateBlockProof>) {
		match self {
			Self::V0 { block, proof } => (block.into_iter().collect(), proof, Vec::new()),
			Self::V1 { blocks, proof } => (blocks, proof, Vec::new()),
			Self::V2 { blocks, proof, late_block_proofs } => (blocks, proof, late_block_proofs),
		}
	}
}

impl<Block: BlockT> ParachainBlockData<Block> {
	/// Log the size of the individual components (header, extrinsics, storage proof) as info.
	pub fn log_size_info(&self) {
		tracing::info!(
			target: "cumulus",
			header_kb = %self.blocks().iter().map(|b| b.header().encoded_size()).sum::<usize>() as f64 / 1024f64,
			extrinsics_kb = %self.blocks().iter().map(|b| b.extrinsics().encoded_size()).sum::<usize>() as f64 / 1024f64,
			storage_proof_kb = %self.proof().encoded_size() as f64 / 1024f64,
			late_block_proofs_kb = %self.late_block_proofs().encoded_size() as f64 / 1024f64,
			"PoV size",
		);
	}

	/// Converts into [`ParachainBlockData::V0`].
	///
	/// Returns `None` if there is not exactly one block.
	pub fn as_v0(&self) -> Option<Self> {
		match self {
			Self::V0 { .. } => Some(self.clone()),
			Self::V1 { blocks, proof } | Self::V2 { blocks, proof, .. } => {
				if blocks.len() != 1 {
					return None;
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
		assert!(decoded.late_block_proofs().is_empty());
	}

	#[test]
	fn decoding_encoding_v2_works() {
		use polkadot_primitives_speculative_messaging::{
			LateBlockProof, MerkleProof,
		};
		use sp_core::H256;

		let late_proof = LateBlockProof {
			source: polkadot_parachain_primitives::primitives::Id::from(1000u32),
			old_subtree_root: H256::repeat_byte(0x01),
			old_subtree_proof: MerkleProof {
				leaf_index: 0,
				leaf_count: 1,
				siblings: vec![],
			},
			new_provides_root: H256::repeat_byte(0x02),
			new_subtree_root: H256::repeat_byte(0x03),
			new_subtree_proof: MerkleProof {
				leaf_index: 0,
				leaf_count: 1,
				siblings: vec![],
			},
			subtree_extension: None,
		};

		let v2 = ParachainBlockData::<TestBlock>::V2 {
			blocks: vec![TestBlock::new(
				Header::new_from_number(10),
				vec![
					TestExtrinsic::new_bare(MockCallU64(10)),
					TestExtrinsic::new_bare(MockCallU64(100)),
				],
			)],
			proof: CompactProof { encoded_nodes: vec![vec![10u8; 200], vec![20u8; 30]] },
			late_block_proofs: vec![late_proof.clone()],
		};

		let encoded = v2.encode();
		let decoded = ParachainBlockData::<TestBlock>::decode(&mut &encoded[..]).unwrap();

		assert_eq!(v2.blocks(), decoded.blocks());
		assert_eq!(v2.proof(), decoded.proof());
		assert_eq!(decoded.late_block_proofs().len(), 1);
		assert_eq!(decoded.late_block_proofs()[0], late_proof);
	}

	#[test]
	fn v2_into_inner_returns_late_block_proofs() {
		let v2 = ParachainBlockData::<TestBlock>::V2 {
			blocks: vec![],
			proof: CompactProof { encoded_nodes: vec![] },
			late_block_proofs: vec![],
		};

		let (blocks, _proof, late_proofs) = v2.into_inner();
		assert!(blocks.is_empty());
		assert!(late_proofs.is_empty());
	}

	#[test]
	fn v0_v1_into_inner_returns_empty_late_block_proofs() {
		let v1 = ParachainBlockData::<TestBlock>::V1 {
			blocks: vec![],
			proof: CompactProof { encoded_nodes: vec![] },
		};

		let (_blocks, _proof, late_proofs) = v1.into_inner();
		assert!(late_proofs.is_empty());
	}
}
