// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Utilities to build bridged chain and BEEFY+MMR structures.

use crate::{
	mock::{
		sign_commitment, validator_pairs, BeefyPair, TestBridgedBlockNumber, TestBridgedCommitment,
		TestBridgedHeader, TestBridgedMmrHash, TestBridgedMmrHashing, TestBridgedMmrNode,
		TestBridgedMmrProof, TestBridgedRawMmrLeaf, TestBridgedValidatorSet,
		TestBridgedValidatorSignature, TestRuntime,
	},
	utils::get_authorities_mmr_root,
};

use bp_beefy::{BeefyPayload, Commitment, ValidatorSetId, MMR_ROOT_PAYLOAD_ID};
use codec::Encode;
use pallet_mmr::NodeIndex;
use rand::Rng;
use sp_consensus_beefy::mmr::{BeefyNextAuthoritySet, MmrLeafVersion};
use sp_core::Pair;
use sp_runtime::traits::{Hash, Header as HeaderT};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HeaderAndCommitment {
	pub header: TestBridgedHeader,
	pub commitment: Option<TestBridgedCommitment>,
	pub validator_set: TestBridgedValidatorSet,
	pub leaf: TestBridgedRawMmrLeaf,
	pub leaf_proof: TestBridgedMmrProof,
	pub mmr_root: TestBridgedMmrHash,
}

impl HeaderAndCommitment {
	pub fn customize_signatures(
		&mut self,
		f: impl FnOnce(&mut Vec<Option<TestBridgedValidatorSignature>>),
	) {
		if let Some(commitment) = &mut self.commitment {
			f(&mut commitment.signatures);
		}
	}

	pub fn customize_commitment(
		&mut self,
		f: impl FnOnce(&mut Commitment<TestBridgedBlockNumber>),
		validator_pairs: &[BeefyPair],
		signature_count: usize,
	) {
		if let Some(mut commitment) = self.commitment.take() {
			f(&mut commitment.commitment);
			self.commitment =
				Some(sign_commitment(commitment.commitment, validator_pairs, signature_count));
		}
	}
}

pub struct ChainBuilder {
	headers: Vec<HeaderAndCommitment>,
	validator_set_id: ValidatorSetId,
	validator_keys: Vec<BeefyPair>,
	mmr: mmr_lib::MMR<TestBridgedMmrNode, BridgedMmrHashMerge, BridgedMmrStorage>,
}

struct BridgedMmrStorage {
	nodes: HashMap<NodeIndex, TestBridgedMmrNode>,
}

impl mmr_lib::MMRStore<TestBridgedMmrNode> for BridgedMmrStorage {
	fn get_elem(&self, pos: NodeIndex) -> mmr_lib::Result<Option<TestBridgedMmrNode>> {
		Ok(self.nodes.get(&pos).cloned())
	}

	fn append(&mut self, pos: NodeIndex, elems: Vec<TestBridgedMmrNode>) -> mmr_lib::Result<()> {
		for (i, elem) in elems.into_iter().enumerate() {
			self.nodes.insert(pos + i as NodeIndex, elem);
		}
		Ok(())
	}
}

impl ChainBuilder {
	/// Creates new chain builder with given validator set size.
	pub fn new(initial_validators_count: u32) -> Self {
		ChainBuilder {
			headers: Vec::new(),
			validator_set_id: 0,
			validator_keys: validator_pairs(0, initial_validators_count),
			mmr: mmr_lib::MMR::new(0, BridgedMmrStorage { nodes: HashMap::new() }),
		}
	}

	/// Get header with given number.
	pub fn header(&self, number: TestBridgedBlockNumber) -> HeaderAndCommitment {
		self.headers[number as usize - 1].clone()
	}

	/// Returns single built header.
	pub fn to_header(&self) -> HeaderAndCommitment {
		assert_eq!(self.headers.len(), 1);
		self.headers[0].clone()
	}

	/// Returns built chain.
	pub fn to_chain(&self) -> Vec<HeaderAndCommitment> {
		self.headers.clone()
	}

	/// Appends header, that has been finalized by BEEFY (so it has a linked signed commitment).
	pub fn append_finalized_header(self) -> Self {
		let next_validator_set_id = self.validator_set_id;
		let next_validator_keys = self.validator_keys.clone();
		HeaderBuilder::with_chain(self, next_validator_set_id, next_validator_keys).finalize()
	}

	/// Append multiple finalized headers at once.
	pub fn append_finalized_headers(mut self, count: usize) -> Self {
		for _ in 0..count {
			self = self.append_finalized_header();
		}
		self
	}

	/// Appends header, that enacts new validator set.
	///
	/// Such headers are explicitly finalized by BEEFY.
	pub fn append_handoff_header(self, next_validators_len: u32) -> Self {
		let new_validator_set_id = self.validator_set_id + 1;
		let new_validator_pairs =
			validator_pairs(rand::thread_rng().gen::<u32>() % (u32::MAX / 2), next_validators_len);

		HeaderBuilder::with_chain(self, new_validator_set_id, new_validator_pairs).finalize()
	}

	/// Append several default header without commitment.
	pub fn append_default_headers(mut self, count: usize) -> Self {
		for _ in 0..count {
			let next_validator_set_id = self.validator_set_id;
			let next_validator_keys = self.validator_keys.clone();
			self =
				HeaderBuilder::with_chain(self, next_validator_set_id, next_validator_keys).build()
		}
		self
	}
}

/// Custom header builder.
pub struct HeaderBuilder {
	chain: ChainBuilder,
	header: TestBridgedHeader,
	leaf: TestBridgedRawMmrLeaf,
	leaf_proof: Option<TestBridgedMmrProof>,
	next_validator_set_id: ValidatorSetId,
	next_validator_keys: Vec<BeefyPair>,
}

impl HeaderBuilder {
	fn with_chain(
		chain: ChainBuilder,
		next_validator_set_id: ValidatorSetId,
		next_validator_keys: Vec<BeefyPair>,
	) -> Self {
		// we're starting with header#1, since header#0 is always finalized
		let header_number = chain.headers.len() as TestBridgedBlockNumber + 1;
		let header = TestBridgedHeader::new(
			header_number,
			Default::default(),
			Default::default(),
			chain.headers.last().map(|h| h.header.hash()).unwrap_or_default(),
			Default::default(),
		);

		let next_validators =
			next_validator_keys.iter().map(|pair| pair.public()).collect::<Vec<_>>();
		let next_validators_mmr_root =
			get_authorities_mmr_root::<TestRuntime, (), _>(next_validators.iter());
		let leaf = sp_consensus_beefy::mmr::MmrLeaf {
			version: MmrLeafVersion::new(1, 0),
			parent_number_and_hash: (header.number().saturating_sub(1), *header.parent_hash()),
			beefy_next_authority_set: BeefyNextAuthoritySet {
				id: next_validator_set_id,
				len: next_validators.len() as u32,
				keyset_commitment: next_validators_mmr_root,
			},
			leaf_extra: (),
		};

		HeaderBuilder {
			chain,
			header,
			leaf,
			leaf_proof: None,
			next_validator_keys,
			next_validator_set_id,
		}
	}

	/// Customize generated proof of header MMR leaf.
	///
	/// Can only be called once.
	pub fn customize_proof(
		mut self,
		f: impl FnOnce(TestBridgedMmrProof) -> TestBridgedMmrProof,
	) -> Self {
		assert!(self.leaf_proof.is_none());

		let leaf_hash = TestBridgedMmrHashing::hash(&self.leaf.encode());
		let node = TestBridgedMmrNode::Hash(leaf_hash);
		let leaf_position = self.chain.mmr.push(node).unwrap();

		let proof = self.chain.mmr.gen_proof(vec![leaf_position]).unwrap();
		// genesis has no leaf => leaf index is header number minus 1
		let leaf_index = *self.header.number() - 1;
		let leaf_count = *self.header.number();
		self.leaf_proof = Some(f(TestBridgedMmrProof {
			leaf_indices: vec![leaf_index],
			leaf_count,
			items: proof.proof_items().iter().map(|i| i.hash()).collect(),
		}));

		self
	}

	/// Build header without commitment.
	pub fn build(mut self) -> ChainBuilder {
		if self.leaf_proof.is_none() {
			self = self.customize_proof(|proof| proof);
		}

		let validators =
			self.chain.validator_keys.iter().map(|pair| pair.public()).collect::<Vec<_>>();
		self.chain.headers.push(HeaderAndCommitment {
			header: self.header,
			commitment: None,
			validator_set: TestBridgedValidatorSet::new(validators, self.chain.validator_set_id)
				.unwrap(),
			leaf: self.leaf,
			leaf_proof: self.leaf_proof.expect("guaranteed by the customize_proof call above; qed"),
			mmr_root: self.chain.mmr.get_root().unwrap().hash(),
		});

		self.chain.validator_set_id = self.next_validator_set_id;
		self.chain.validator_keys = self.next_validator_keys;

		self.chain
	}

	/// Build header with commitment.
	pub fn finalize(self) -> ChainBuilder {
		let validator_count = self.chain.validator_keys.len();
		let current_validator_set_id = self.chain.validator_set_id;
		let current_validator_set_keys = self.chain.validator_keys.clone();
		let mut chain = self.build();

		let last_header = chain.headers.last_mut().expect("added by append_header; qed");
		last_header.commitment = Some(sign_commitment(
			Commitment {
				payload: BeefyPayload::from_single_entry(
					MMR_ROOT_PAYLOAD_ID,
					chain.mmr.get_root().unwrap().hash().encode(),
				),
				block_number: *last_header.header.number(),
				validator_set_id: current_validator_set_id,
			},
			&current_validator_set_keys,
			validator_count * 2 / 3 + 1,
		));

		chain
	}
}

/// Default Merging & Hashing behavior for MMR.
pub struct BridgedMmrHashMerge;

impl mmr_lib::Merge for BridgedMmrHashMerge {
	type Item = TestBridgedMmrNode;

	fn merge(left: &Self::Item, right: &Self::Item) -> mmr_lib::Result<Self::Item> {
		let mut concat = left.hash().as_ref().to_vec();
		concat.extend_from_slice(right.hash().as_ref());

		Ok(TestBridgedMmrNode::Hash(TestBridgedMmrHashing::hash(&concat)))
	}
}
