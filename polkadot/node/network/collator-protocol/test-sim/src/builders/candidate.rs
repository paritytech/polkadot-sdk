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

use polkadot_primitives::{
	CandidateHash, CandidateReceiptV2 as CandidateReceipt, Hash, Id as ParaId, MutateDescriptorV2,
};
use polkadot_primitives_test_helpers::dummy_candidate_receipt_v2_bad_sig;

use crate::builders::fixtures::dummy_pvd;

/// Wraps a `CandidateReceiptV2` along with the inputs the scenario used to construct it.
#[derive(Clone, Debug)]
pub struct Candidate {
	/// The receipt itself. Tests typically pass this as part of a `CollationFetchingResponse`.
	pub receipt: CandidateReceipt,
}

impl Candidate {
	/// Build a fresh candidate for the given para id at the given relay parent. The
	/// `persisted_validation_data_hash` is set to the hash of the framework's
	/// [`crate::builders::fixtures::dummy_pvd`].
	pub fn for_para_at(para: ParaId, relay_parent: Hash) -> Self {
		let mut receipt =
			dummy_candidate_receipt_v2_bad_sig(relay_parent, Some(Default::default()));
		receipt.descriptor.set_para_id(para);
		receipt.descriptor.set_persisted_validation_data_hash(dummy_pvd().hash());
		Self { receipt }
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
