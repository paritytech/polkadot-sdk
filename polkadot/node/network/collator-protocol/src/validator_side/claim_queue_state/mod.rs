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

//! Helper structs used for tracking the state of the claim queue over a set of relay blocks.
//! Refer to [`ClaimQueueState`] and [`PerLeafClaimQueueState`] for more details.

use std::collections::HashSet;

use polkadot_primitives::{CandidateHash, Hash, Id as ParaId};

mod basic;
mod per_leaf;

pub(crate) use basic::ClaimQueueState;
pub(crate) use per_leaf::PerLeafClaimQueueState;

/// Represents the state of a claim.
#[derive(Debug, PartialEq, Clone)]
enum ClaimState {
	/// Unclaimed
	Free,
	/// The candidate is pending fetching or validation. The candidate hash is optional because for
	/// the non-experimental version of the collator protocol, we don't care about specific
	/// candidate hashes, but only about their number.
	Pending(Option<CandidateHash>),
	/// The candidate is seconded.
	Seconded(CandidateHash),
}

impl ClaimState {
	fn candidate_hash(&self) -> Option<&CandidateHash> {
		match self {
			ClaimState::Pending(Some(candidate)) => Some(candidate),
			ClaimState::Seconded(candidate) => Some(candidate),
			_ => None,
		}
	}

	fn clone_or_default(&self, known_candidates: &HashSet<CandidateHash>) -> Self {
		match self {
			ClaimState::Pending(Some(candidate)) | ClaimState::Seconded(candidate)
				if !known_candidates.contains(candidate) =>
				ClaimState::Free,
			_ => self.clone(),
		}
	}
}

/// Represents a single claim from the claim queue, mapped to the relay chain block where it could
/// be backed on-chain.
#[derive(Debug, PartialEq, Clone)]
struct ClaimInfo {
	// Hash of the relay chain block. Can be `None` if it is still not known (a future block).
	hash: Option<Hash>,
	/// Represents the `ParaId` scheduled for the block. Can be `None` if nothing is scheduled.
	claim: Option<ParaId>,
	/// The length of the claim queue at the block. It is used to determine the 'block window'
	/// where a claim can be made.
	claim_queue_len: usize,
	/// The claim state.
	claimed: ClaimState,
}

impl ClaimInfo {
	fn hash_equals(&self, hash: &Hash) -> bool {
		self.hash.as_ref() == Some(hash)
	}
}

trait ClaimInfoRef {
	fn hash_equals(&self, hash: &Hash) -> bool;

	fn claim_queue_len(&self) -> usize;
}

impl<'a> ClaimInfoRef for &'a ClaimInfo {
	fn hash_equals(&self, hash: &Hash) -> bool {
		ClaimInfo::hash_equals(self, hash)
	}

	fn claim_queue_len(&self) -> usize {
		self.claim_queue_len
	}
}

impl<'a> ClaimInfoRef for &'a mut ClaimInfo {
	fn hash_equals(&self, hash: &Hash) -> bool {
		ClaimInfo::hash_equals(self, hash)
	}

	fn claim_queue_len(&self) -> usize {
		self.claim_queue_len
	}
}

#[cfg(test)]
mod test {
	use super::*;

	use sp_core::H256;
	use std::sync::LazyLock;

	pub static ROOT_RELAY_PARENT: LazyLock<H256> = LazyLock::new(|| Hash::from_low_u64_be(0));
	pub static RELAY_PARENT_A: LazyLock<H256> = LazyLock::new(|| Hash::from_low_u64_be(1));
	pub static RELAY_PARENT_B: LazyLock<H256> = LazyLock::new(|| Hash::from_low_u64_be(2));
	pub static RELAY_PARENT_C: LazyLock<H256> = LazyLock::new(|| Hash::from_low_u64_be(3));
	pub static RELAY_PARENT_D: LazyLock<H256> = LazyLock::new(|| Hash::from_low_u64_be(4));

	pub static CANDIDATE_A1: LazyLock<CandidateHash> =
		LazyLock::new(|| CandidateHash(Hash::from_low_u64_be(101)));
	pub static CANDIDATE_A2: LazyLock<CandidateHash> =
		LazyLock::new(|| CandidateHash(Hash::from_low_u64_be(102)));
	pub static CANDIDATE_A3: LazyLock<CandidateHash> =
		LazyLock::new(|| CandidateHash(Hash::from_low_u64_be(103)));
	pub static CANDIDATE_A4: LazyLock<CandidateHash> =
		LazyLock::new(|| CandidateHash(Hash::from_low_u64_be(104)));

	pub static CANDIDATE_B1: LazyLock<CandidateHash> =
		LazyLock::new(|| CandidateHash(Hash::from_low_u64_be(201)));
	pub static CANDIDATE_B2: LazyLock<CandidateHash> =
		LazyLock::new(|| CandidateHash(Hash::from_low_u64_be(202)));

	pub static CANDIDATE_C1: LazyLock<CandidateHash> =
		LazyLock::new(|| CandidateHash(Hash::from_low_u64_be(301)));
	pub static CANDIDATE_C2: LazyLock<CandidateHash> =
		LazyLock::new(|| CandidateHash(Hash::from_low_u64_be(302)));

	pub static CANDIDATE_D1: LazyLock<CandidateHash> =
		LazyLock::new(|| CandidateHash(Hash::from_low_u64_be(401)));

	pub const PARA_1: ParaId = ParaId::new(1);
	pub const PARA_2: ParaId = ParaId::new(2);
	pub const PARA_3: ParaId = ParaId::new(3);

	impl ClaimInfo {
		fn new(claim_queue_len: usize, claimed: ClaimState) -> Self {
			Self { hash: None, claim: None, claim_queue_len, claimed }
		}

		pub fn new_free(claim_queue_len: usize) -> Self {
			Self::new(claim_queue_len, ClaimState::Free)
		}

		pub fn new_pending(claim_queue_len: usize, maybe_candidate: Option<CandidateHash>) -> Self {
			Self::new(claim_queue_len, ClaimState::Pending(maybe_candidate))
		}

		pub fn new_seconded(claim_queue_len: usize, candidate: CandidateHash) -> Self {
			Self::new(claim_queue_len, ClaimState::Seconded(candidate))
		}

		pub fn with_hash(mut self, hash: Hash) -> ClaimInfo {
			self.hash = Some(hash);
			self
		}

		pub fn with_claim(mut self, claim: ParaId) -> ClaimInfo {
			self.claim = Some(claim);
			self
		}

		pub fn with(mut self, hash: Hash, claim: ParaId) -> ClaimInfo {
			self.hash = Some(hash);
			self.claim = Some(claim);
			self
		}
	}
}
