// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Builder for `CandidateReceiptV2` shaped to whatever para / relay-parent the scenario uses.

use polkadot_node_primitives::PoV;
use polkadot_primitives::{
	CandidateCommitments, CandidateHash, CandidateReceiptV2 as CandidateReceipt, HeadData,
	Hash, Id as ParaId, MutateDescriptorV2, PersistedValidationData,
};
use polkadot_primitives_test_helpers::dummy_candidate_receipt_v2_bad_sig;

use crate::builders::fixtures::dummy_pvd;

/// Wraps a `CandidateReceiptV2` along with the inputs the scenario used to construct it.
///
/// Tests that need fragment-chain semantics (parent_head_data ↔ output head_data threaded
/// across N candidates) should build via [`Candidate::builder`] and capture the
/// [`Candidate::pvd`] for use when responding to `GetProspectiveValidationData` queries.
#[derive(Clone, Debug)]
pub struct Candidate {
	/// The receipt itself. Tests typically pass this as part of a `CollationFetchingResponse`.
	pub receipt: CandidateReceipt,
	/// PVD the receipt's `persisted_validation_data_hash` was computed against. Real
	/// candidate-backing looks up the PVD via `ProspectiveParachainsMessage::
	/// GetProspectiveValidationData`; tests that drive the full backing flow should hand this
	/// PVD back to that responder.
	pub pvd: PersistedValidationData,
	/// Commitments that hash to the receipt's `commitments_hash`. Includes `head_data`,
	/// which is the parachain block's *output* head data — the next candidate in a fragment
	/// chain takes this as its `parent_head` in its own PVD.
	pub commitments: CandidateCommitments,
}

impl Candidate {
	/// Build a fresh candidate for the given para id at the given relay parent with empty
	/// parent and output head data. Most one-shot scenarios want this.
	pub fn for_para_at(para: ParaId, relay_parent: Hash) -> Self {
		Self::builder()
			.para(para)
			.relay_parent(relay_parent)
			.build()
	}

	/// Start a fluent builder. All fields default to the framework's zero-shape (empty head
	/// data, default para = `ParaId::from(0)`, default relay parent = `Hash::zero()`,
	/// `relay_parent_number = 0`).
	pub fn builder() -> CandidateBuilder {
		CandidateBuilder {
			para: ParaId::from(0),
			relay_parent: Hash::zero(),
			relay_parent_number: 0,
			parent_head: HeadData(Vec::new()),
			head_data: HeadData(Vec::new()),
		}
	}

	/// Hash of the candidate.
	pub fn hash(&self) -> CandidateHash {
		self.receipt.hash()
	}

	/// Para id this candidate belongs to.
	pub fn para(&self) -> ParaId {
		self.receipt.descriptor.para_id()
	}

	/// Relay parent this candidate is anchored at.
	pub fn relay_parent(&self) -> Hash {
		self.receipt.descriptor.relay_parent()
	}

	/// Parent head data hash — what the V2 advertisement's `parent_head_data_hash` field
	/// should carry to match this candidate.
	pub fn parent_head_hash(&self) -> Hash {
		self.pvd.parent_head.hash()
	}

	/// Parent head data — what a chained child candidate's `pvd.parent_head` should be.
	/// Equivalent to `commitments.head_data`.
	pub fn output_head(&self) -> HeadData {
		self.commitments.head_data.clone()
	}

	/// PoV with empty block data — most scenarios don't care about PoV bytes, only that
	/// the response carries one. Provided here so scenarios don't need to construct it
	/// manually.
	pub fn empty_pov() -> PoV {
		PoV { block_data: polkadot_node_primitives::BlockData(vec![]) }
	}

	/// Wrap a pre-built `CandidateReceiptV2` (e.g. one produced by
	/// `dummy_committed_candidate_receipt_v2(...).to_plain()`) into a [`Candidate`]. The
	/// resulting `pvd` and `commitments` are zero-shape placeholders — only useful for
	/// scenarios that don't drive the real backing pipeline (sanity-check rejection
	/// scenarios, in particular).
	pub fn from_receipt(receipt: CandidateReceipt) -> Self {
		Self {
			receipt,
			pvd: PersistedValidationData {
				parent_head: HeadData(Vec::new()),
				relay_parent_number: 0,
				relay_parent_storage_root: Hash::zero(),
				max_pov_size: 0,
			},
			commitments: CandidateCommitments {
				head_data: HeadData(Vec::new()),
				horizontal_messages: Default::default(),
				upward_messages: Default::default(),
				new_validation_code: None,
				processed_downward_messages: 0,
				hrmp_watermark: 0,
			},
		}
	}
}

/// Fluent builder for [`Candidate`]. Defaults to empty parent/output head data — override
/// via [`Self::parent_head`] / [`Self::head_data`] when chaining candidates in a fragment.
pub struct CandidateBuilder {
	para: ParaId,
	relay_parent: Hash,
	relay_parent_number: u32,
	parent_head: HeadData,
	head_data: HeadData,
}

impl CandidateBuilder {
	/// Para this candidate belongs to.
	pub fn para(mut self, para: ParaId) -> Self {
		self.para = para;
		self
	}

	/// Relay parent the candidate is anchored at.
	pub fn relay_parent(mut self, relay_parent: Hash) -> Self {
		self.relay_parent = relay_parent;
		self
	}

	/// Block number of the relay parent. Real backing's PVD lookup checks this; if the
	/// candidate's PVD's `relay_parent_number` doesn't match the chain's actual number
	/// the receipt's `persisted_validation_data_hash` won't validate. Defaults to 0;
	/// scenarios should pass `world.leaf_number()` or similar.
	pub fn relay_parent_number(mut self, n: u32) -> Self {
		self.relay_parent_number = n;
		self
	}

	/// Parent head data — i.e. the parachain head this candidate is *building on*. In a
	/// fragment chain, take the previous candidate's [`Candidate::output_head`].
	pub fn parent_head(mut self, head: HeadData) -> Self {
		self.parent_head = head;
		self
	}

	/// Output head data — the parachain block this candidate would produce.
	pub fn head_data(mut self, head: HeadData) -> Self {
		self.head_data = head;
		self
	}

	/// Finalise the builder.
	pub fn build(self) -> Candidate {
		let pvd = if self.parent_head.0.is_empty() && self.relay_parent_number == 0 {
			// Use the framework's well-known dummy PVD shape so existing scenarios that
			// don't care about head-data threading get the same result they did before.
			dummy_pvd()
		} else {
			PersistedValidationData {
				parent_head: self.parent_head,
				relay_parent_number: self.relay_parent_number,
				relay_parent_storage_root: Hash::zero(),
				max_pov_size: 5 * 1024 * 1024,
			}
		};
		let commitments = CandidateCommitments {
			head_data: self.head_data,
			horizontal_messages: Default::default(),
			upward_messages: Default::default(),
			new_validation_code: None,
			processed_downward_messages: 0,
			hrmp_watermark: 0,
		};

		let mut receipt =
			dummy_candidate_receipt_v2_bad_sig(self.relay_parent, Some(Default::default()));
		receipt.descriptor.set_para_id(self.para);
		receipt.descriptor.set_persisted_validation_data_hash(pvd.hash());
		receipt.commitments_hash = commitments.hash();

		Candidate { receipt, pvd, commitments }
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn candidate_for_para_at_carries_para_and_relay_parent() {
		let para = ParaId::from(2000);
		let rp = Hash::from_low_u64_be(0xab);
		let cand = Candidate::for_para_at(para, rp);
		assert_eq!(cand.para(), para);
		assert_eq!(cand.relay_parent(), rp);
		// Hash is deterministic given the inputs, but the actual value isn't load-bearing
		// here; just confirm it isn't the default zero hash.
		assert_ne!(cand.hash(), CandidateHash::default());
	}
}
