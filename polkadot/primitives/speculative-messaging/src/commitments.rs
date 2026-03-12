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

//! Commitment types for speculative cross-chain messaging.
//!
//! This module defines the on-chain commitment structures that the relay chain
//! verifies. A [`ProvidesCommitment`] is produced by a sending parachain and
//! contains a top-level Merkle root over all per-destination MMR roots. A
//! [`RequiresCommitment`] is produced by a receiving parachain and references
//! the expected root of the sender it consumed messages from.

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use scale_info::TypeInfo;
use sp_core::H256;

/// A commitment published by a sending parachain.
///
/// Contains a single top-level Merkle root computed over all per-destination
/// MMR roots. The relay chain stores this root and later uses it to verify
/// that receiving parachains consumed messages consistently.
#[derive(
	Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode, DecodeWithMemTracking, TypeInfo,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProvidesCommitment {
	/// Top-level Merkle root over all per-destination MMR roots.
	pub root: H256,
}

impl ProvidesCommitment {
	/// Returns `true` if the root is the zero hash, indicating no messages
	/// were committed.
	pub fn is_empty(&self) -> bool {
		self.root == H256::zero()
	}
}

/// A commitment published by a receiving parachain.
///
/// References the source parachain and the expected provides root that the
/// receiver built its state transition against. The relay chain checks that
/// the referenced provides root matches what the source actually published.
#[derive(
	Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode, DecodeWithMemTracking, TypeInfo,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RequiresCommitment {
	/// The source parachain we are receiving messages from.
	pub source: ParaId,
	/// The provides root we built our state transition against.
	pub expected_root: H256,
}

impl RequiresCommitment {
	/// Returns `true` if the `expected_root` matches the root in the given
	/// [`ProvidesCommitment`].
	pub fn matches_provides(&self, provides: &ProvidesCommitment) -> bool {
		self.expected_root == provides.root
	}
}

/// Convenience struct pairing a [`ProvidesCommitment`] with its associated
/// [`RequiresCommitment`]s.
///
/// This is useful when validating a candidate that both provides messages to
/// downstream consumers and requires messages from upstream senders.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct CommitmentPair {
	/// The provides commitment published by this parachain.
	pub provides: ProvidesCommitment,
	/// The requires commitments referencing upstream providers.
	pub requires: Vec<RequiresCommitment>,
}

impl CommitmentPair {
	/// Returns the subset of [`RequiresCommitment`]s that do not match any
	/// of the given available provides commitments.
	pub fn unmatched_requires<'a>(
		&'a self,
		available_provides: &[ProvidesCommitment],
	) -> Vec<&'a RequiresCommitment> {
		self.requires
			.iter()
			.filter(|req| !available_provides.iter().any(|p| req.matches_provides(p)))
			.collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_hash(byte: u8) -> H256 {
		H256::from([byte; 32])
	}

	#[test]
	fn provides_commitment_is_empty() {
		let empty = ProvidesCommitment { root: H256::zero() };
		assert!(empty.is_empty());

		let non_empty = ProvidesCommitment { root: make_hash(1) };
		assert!(!non_empty.is_empty());
	}

	#[test]
	fn requires_matches_provides() {
		let root = make_hash(42);
		let provides = ProvidesCommitment { root };
		let requires = RequiresCommitment { source: ParaId::from(1000), expected_root: root };
		assert!(requires.matches_provides(&provides));

		let different_provides = ProvidesCommitment { root: make_hash(99) };
		assert!(!requires.matches_provides(&different_provides));
	}

	#[test]
	fn unmatched_requires() {
		let root_a = make_hash(1);
		let root_b = make_hash(2);
		let root_c = make_hash(3);

		let pair = CommitmentPair {
			provides: ProvidesCommitment { root: make_hash(0) },
			requires: alloc::vec![
				RequiresCommitment { source: ParaId::from(100), expected_root: root_a },
				RequiresCommitment { source: ParaId::from(200), expected_root: root_b },
				RequiresCommitment { source: ParaId::from(300), expected_root: root_c },
			],
		};

		let available =
			alloc::vec![ProvidesCommitment { root: root_a }, ProvidesCommitment { root: root_c },];

		let unmatched = pair.unmatched_requires(&available);
		assert_eq!(unmatched.len(), 1);
		assert_eq!(unmatched[0].source, ParaId::from(200));
		assert_eq!(unmatched[0].expected_root, root_b);
	}

	#[test]
	fn encode_decode_roundtrip() {
		// ProvidesCommitment roundtrip
		let provides = ProvidesCommitment { root: make_hash(55) };
		let encoded = provides.encode();
		let decoded = ProvidesCommitment::decode(&mut &encoded[..])
			.expect("ProvidesCommitment should decode");
		assert_eq!(provides, decoded);

		// RequiresCommitment roundtrip
		let requires =
			RequiresCommitment { source: ParaId::from(2000), expected_root: make_hash(77) };
		let encoded = requires.encode();
		let decoded = RequiresCommitment::decode(&mut &encoded[..])
			.expect("RequiresCommitment should decode");
		assert_eq!(requires, decoded);

		// CommitmentPair roundtrip
		let pair = CommitmentPair {
			provides: ProvidesCommitment { root: make_hash(10) },
			requires: alloc::vec![RequiresCommitment {
				source: ParaId::from(500),
				expected_root: make_hash(20),
			},],
		};
		let encoded = pair.encode();
		let decoded =
			CommitmentPair::decode(&mut &encoded[..]).expect("CommitmentPair should decode");
		assert_eq!(pair, decoded);
	}

	#[test]
	fn commitment_pair_empty_requires() {
		let pair = CommitmentPair {
			provides: ProvidesCommitment { root: make_hash(5) },
			requires: alloc::vec![],
		};

		// With no requires, unmatched should also be empty regardless of
		// available provides.
		let available = alloc::vec![ProvidesCommitment { root: make_hash(5) }];
		let unmatched = pair.unmatched_requires(&available);
		assert!(unmatched.is_empty());

		// Even with no available provides, still empty.
		let unmatched = pair.unmatched_requires(&[]);
		assert!(unmatched.is_empty());
	}
}
