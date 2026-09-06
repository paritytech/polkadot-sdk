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

//! Relay-visible speculative-messaging commitments (Phase 1).
//!
//! The relay chain only sees the **commitments** carried in the `UMPSignal::Provides`/`Requires`
//! signals (inside `CandidateCommitments.upward_messages`):
//! - **`Provides(StreamsRoot)`** — one root per sender block, the root of the sender's keyed stream
//!   commitment tree. Recorded into the relay's provides window.
//! - **`Requires(RequiresSet)`** — a receiver's expected `(source, StreamsRoot)` entries: a
//!   canonical sorted set (strictly increasing `ParaId`, one entry per source), matched against
//!   each source's window.
//!
//! These types live here (not in `cumulus-primitives-spec-messaging`) because the `UMPSignal` enum
//! is defined in `polkadot-primitives::v9` and the relay chain decodes them — putting them in the
//! parachain crate would force a `polkadot -> cumulus` dependency. The parachain off-chain / PoV
//! types and the MMR primitives that *build* these commitments live in that crate.

use alloc::vec::Vec;
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

/// Maximum number of source parachains a receiver can consume from in one block.
/// Bounds the size of the `requires` commitment (one entry per source). The design's
/// `MaxCommitmentEntries`: today's registered-parachain count (~200) rounded up, so a
/// maximally-connected receiver is expressible; the bound is consensus-relevant (decode
/// rejects larger sets), so all implementations must agree on it.
pub const MAX_COMMITMENT_ENTRIES: u32 = 256;

/// Root of a sender's stream commitment tree: a binary compact (Patricia) trie keyed by the
/// canonical SCALE encoding of `StreamId` (8 bytes), leaves = the streams' MMR roots.
///
/// A **newtype** over `Hash` (wire-transparent), deliberately *distinct* from an MMR / stream root
/// so the two kinds of root cannot be confused — they flow through different checks ("confusing
/// roots must not typecheck"; see the design's "Stream Commitment Tree"). The relay chain treats it
/// as opaque; the tree structure and inclusion proofs live in
/// `cumulus-primitives-spec-messaging::streams_root`, which re-exports this type.
#[derive(
	Clone,
	Copy,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	codec::MaxEncodedLen,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
#[cfg_attr(feature = "std", derive(Hash))]
pub struct StreamsRoot(pub Hash);

/// A receiver's `requires` commitment: a canonical, bounded set of `(ParaId, StreamsRoot)` entries
/// — one per **source** parachain (covering all its streams), sorted by strictly-increasing
/// `ParaId`.
///
/// The order is enforced at decode so the encoding is canonical — collators, PVF, and the relay
/// chain all produce the same bytes for the same set. This is the design's `RequiresSet` (Appendix
/// D); the relay matches each `(source, StreamsRoot)` against that source's provides window.
#[derive(
	Clone, codec::Encode, codec::MaxEncodedLen, Debug, Default, Eq, PartialEq, scale_info::TypeInfo,
)]
#[cfg_attr(feature = "std", derive(Hash))]
pub struct RequiresSet(BoundedVec<(ParaId, StreamsRoot), ConstU32<MAX_COMMITMENT_ENTRIES>>);

/// Decode is manually implemented to enforce that `ParaId`s are strictly increasing (canonical
/// form).
impl codec::Decode for RequiresSet {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let inner =
			BoundedVec::<(ParaId, StreamsRoot), ConstU32<MAX_COMMITMENT_ENTRIES>>::decode(input)?;

		for pair in inner.windows(2) {
			if pair[0].0 >= pair[1].0 {
				return Err(codec::Error::from(
					"RequiresSet entries must be sorted by increasing ParaId",
				));
			}
		}

		Ok(Self(inner))
	}
}

// Marker: the manual `Decode` (a sortedness check over `BoundedVec`) is mem-tracking safe. A derive
// does not compose with a manual `Decode`, so it is impl'd by hand.
impl codec::DecodeWithMemTracking for RequiresSet {}

impl RequiresSet {
	/// The committed `StreamsRoot` for `source`, if present (O(log n) lookup).
	pub fn get(&self, source: ParaId) -> Option<&StreamsRoot> {
		self.0
			.binary_search_by_key(&source, |(id, _)| *id)
			.ok()
			.map(|idx| &self.0[idx].1)
	}

	/// The number of entries in the set.
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// Whether the set has no entries.
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	/// Iterate the `(source, StreamsRoot)` entries in ascending `ParaId` order.
	pub fn iter(&self) -> impl Iterator<Item = &(ParaId, StreamsRoot)> {
		self.0.iter()
	}

	/// Build a [`RequiresSet`] from an arbitrary (possibly unordered) iterator, sorting by `ParaId`
	/// to produce the canonical encoding.
	pub fn try_from_iter(
		it: impl IntoIterator<Item = (ParaId, StreamsRoot)>,
	) -> Result<Self, CommitmentError> {
		let mut entries: Vec<(ParaId, StreamsRoot)> = it.into_iter().collect();
		entries.sort_by_key(|(source, _)| *source);

		if entries.windows(2).any(|w| w[0].0 == w[1].0) {
			return Err(CommitmentError::DuplicateParaId);
		}

		let inner = BoundedVec::try_from(entries).map_err(|_| CommitmentError::TooManyEntries)?;

		Ok(Self(inner))
	}
}

impl<'a> IntoIterator for &'a RequiresSet {
	type Item = &'a (ParaId, StreamsRoot);
	type IntoIter = core::slice::Iter<'a, (ParaId, StreamsRoot)>;

	fn into_iter(self) -> Self::IntoIter {
		self.0.iter()
	}
}

/// Errors that can occur when constructing a [`RequiresSet`].
#[derive(Debug, PartialEq, Eq)]
pub enum CommitmentError {
	/// The same source `ParaId` appears more than once.
	DuplicateParaId,
	/// More entries were provided than `MAX_COMMITMENT_ENTRIES` allows.
	TooManyEntries,
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};

	fn sr(byte: u8) -> StreamsRoot {
		StreamsRoot(Hash::repeat_byte(byte))
	}

	/// `n` entries with distinct, strictly-increasing `ParaId`s.
	fn many(n: u32) -> Vec<(ParaId, StreamsRoot)> {
		(0..n).map(|i| (ParaId::from(i), sr(i as u8))).collect()
	}

	#[test]
	fn try_from_iter_sorts_entries() {
		let set = RequiresSet::try_from_iter([
			(ParaId::from(3), sr(3)),
			(ParaId::from(1), sr(1)),
			(ParaId::from(2), sr(2)),
		])
		.unwrap();

		let ids: Vec<ParaId> = set.iter().map(|(id, _)| *id).collect();
		assert_eq!(ids, vec![ParaId::from(1), ParaId::from(2), ParaId::from(3)]);
	}

	#[test]
	fn try_from_iter_rejects_duplicate_para_id() {
		let result =
			RequiresSet::try_from_iter([(ParaId::from(1), sr(1)), (ParaId::from(1), sr(2))]);
		assert_eq!(result, Err(CommitmentError::DuplicateParaId));
	}

	#[test]
	fn try_from_iter_rejects_too_many_entries() {
		let result = RequiresSet::try_from_iter(many(MAX_COMMITMENT_ENTRIES + 1));
		assert_eq!(result, Err(CommitmentError::TooManyEntries));
	}

	#[test]
	fn encode_decode_round_trip_works() {
		let set = RequiresSet::try_from_iter([(ParaId::from(1), sr(1)), (ParaId::from(2), sr(2))])
			.unwrap();

		let encoded = set.encode();
		let decoded = RequiresSet::decode(&mut &encoded[..]).unwrap();

		assert_eq!(set, decoded);
	}

	#[test]
	fn decode_rejects_out_of_order_para_ids() {
		let bad: Vec<(ParaId, StreamsRoot)> =
			vec![(ParaId::from(2), sr(2)), (ParaId::from(1), sr(1))];
		let encoded = bad.encode();

		assert!(RequiresSet::decode(&mut &encoded[..]).is_err());
	}

	#[test]
	fn decode_rejects_duplicate_para_ids() {
		let bad: Vec<(ParaId, StreamsRoot)> =
			vec![(ParaId::from(1), sr(1)), (ParaId::from(1), sr(2))];
		let encoded = bad.encode();

		assert!(RequiresSet::decode(&mut &encoded[..]).is_err());
	}

	#[test]
	fn decode_rejects_too_many_entries() {
		let encoded = many(MAX_COMMITMENT_ENTRIES + 1).encode();
		assert!(RequiresSet::decode(&mut &encoded[..]).is_err());
	}

	#[test]
	fn get_finds_existing_and_missing_entries() {
		let set = RequiresSet::try_from_iter([(ParaId::from(1), sr(1)), (ParaId::from(3), sr(3))])
			.unwrap();

		assert_eq!(set.get(ParaId::from(1)), Some(&sr(1)));
		assert_eq!(set.get(ParaId::from(3)), Some(&sr(3)));
		assert_eq!(set.get(ParaId::from(2)), None);
	}

	#[test]
	fn len_and_is_empty() {
		let empty = RequiresSet::default();
		assert!(empty.is_empty());
		assert_eq!(empty.len(), 0);

		let set = RequiresSet::try_from_iter([(ParaId::from(1), sr(1))]).unwrap();
		assert!(!set.is_empty());
		assert_eq!(set.len(), 1);
	}

	#[test]
	fn into_iter_yields_sorted_entries() {
		let set = RequiresSet::try_from_iter([(ParaId::from(2), sr(2)), (ParaId::from(1), sr(1))])
			.unwrap();

		let collected: Vec<_> = (&set).into_iter().collect();
		assert_eq!(collected, vec![&(ParaId::from(1), sr(1)), &(ParaId::from(2), sr(2))]);
	}
}
