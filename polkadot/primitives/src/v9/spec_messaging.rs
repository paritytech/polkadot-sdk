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

//! Speculative Messaging commitment types embedded in `UMPSignal`s.
//!
//! Only the types the relay chain itself stores or compares live here; all
//! stream-level machinery (stream ids, MMR frontiers, commitment tree,
//! proofs) is parachain-side and lives in `cumulus-primitives-spec-messaging`.

use alloc::vec::Vec;
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

/// Root of a sender's stream commitment tree: a binary compact (Patricia)
/// trie keyed by the canonical encoding of a stream id, leaves = the
/// streams' MMR roots.
///
/// This is the only thing the relay chain ever stores or compares —
/// everything below it is proven parachain-side. Distinct newtype from the
/// parachain-side `MmrRoot`: the two kinds of root flow through different
/// checks and must not be confusable.
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
	Ord,
	PartialOrd,
	scale_info::TypeInfo,
)]
pub struct StreamsRoot(pub Hash);

/// Maximum number of entries in a [`RequiresSet`].
///
/// Entries are per *source* parachain, so the natural ceiling is the number
/// of registered parachains; the cap exists to bound candidate receipt
/// growth and relay matching work per candidate, not to constrain topology.
pub const MAX_COMMITMENT_ENTRIES: u32 = 256;

/// Canonical, bounded set of `(ParaId, StreamsRoot)` entries — the receiver
/// side commitment ("requires") of a candidate.
///
/// One entry per SOURCE, not per stream: the [`StreamsRoot`] covers all of
/// that source's streams at once, so the set is naturally bounded by the
/// number of parachains a receiver consumes from.
///
/// Entries are kept sorted by strictly increasing `ParaId` so the encoded
/// form is canonical — collators, PVF and the relay chain all produce the
/// same bytes for the same set. Construction is sealed: [`Self::try_from_iter`]
/// (sorts, rejects duplicates) and `Decode` are the only ways in; no mutable
/// access.
#[derive(
	Clone,
	codec::Encode,
	codec::MaxEncodedLen,
	codec::DecodeWithMemTracking,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct RequiresSet(BoundedVec<(ParaId, StreamsRoot), ConstU32<MAX_COMMITMENT_ENTRIES>>);

/// Manual `Decode` REJECTS input whose `ParaId`s aren't strictly increasing
/// (no silent normalization): the bytes come from untrusted parachain wasm,
/// so malformed sets (duplicate sources with conflicting roots) must fail
/// loudly at the boundary, and canonical bytes make `decode ∘ encode` the
/// identity.
impl codec::Decode for RequiresSet {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let inner =
			BoundedVec::<(ParaId, StreamsRoot), ConstU32<MAX_COMMITMENT_ENTRIES>>::decode(input)?;

		for pair in inner.windows(2) {
			if pair[0].0 >= pair[1].0 {
				return Err(codec::Error::from(
					"RequiresSet entries must be sorted by strictly increasing ParaId",
				));
			}
		}

		Ok(Self(inner))
	}
}

impl RequiresSet {
	/// Returns the [`StreamsRoot`] required of `source`, or `None` if not present.
	pub fn get(&self, source: ParaId) -> Option<&StreamsRoot> {
		self.0
			.binary_search_by_key(&source, |(id, _)| *id)
			.ok()
			.map(|idx| &self.0[idx].1)
	}

	/// Returns the number of entries in the set.
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// Returns `true` if the set contains no entries.
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	/// Iterates over all `(ParaId, StreamsRoot)` entries in sorted order.
	pub fn iter(&self) -> impl Iterator<Item = &(ParaId, StreamsRoot)> {
		self.0.iter()
	}

	/// Builds a [`RequiresSet`] from an arbitrary (possibly unordered)
	/// iterator, sorting entries by `ParaId` to produce the canonical
	/// encoding.
	pub fn try_from_iter(
		it: impl IntoIterator<Item = (ParaId, StreamsRoot)>,
	) -> Result<Self, RequiresSetError> {
		let mut entries: Vec<(ParaId, StreamsRoot)> = it.into_iter().collect();
		entries.sort_by_key(|(para_id, _)| *para_id);

		if entries.windows(2).any(|w| w[0].0 == w[1].0) {
			return Err(RequiresSetError::DuplicateParaId);
		}

		let inner = BoundedVec::try_from(entries).map_err(|_| RequiresSetError::TooManyEntries)?;

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
pub enum RequiresSetError {
	/// The same `ParaId` appears more than once.
	DuplicateParaId,
	/// More entries were provided than [`MAX_COMMITMENT_ENTRIES`] allows.
	TooManyEntries,
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};

	fn r(byte: u8) -> StreamsRoot {
		StreamsRoot(Hash::repeat_byte(byte))
	}

	#[test]
	fn try_from_iter_sorts_entries() {
		let set = RequiresSet::try_from_iter([
			(ParaId::from(3), r(3)),
			(ParaId::from(1), r(1)),
			(ParaId::from(2), r(2)),
		])
		.unwrap();

		let ids: Vec<ParaId> = set.iter().map(|(id, _)| *id).collect();
		assert_eq!(ids, vec![ParaId::from(1), ParaId::from(2), ParaId::from(3)]);
	}

	#[test]
	fn try_from_iter_rejects_duplicate_para_id() {
		let result = RequiresSet::try_from_iter([(ParaId::from(1), r(1)), (ParaId::from(1), r(2))]);

		assert_eq!(result, Err(RequiresSetError::DuplicateParaId));
	}

	#[test]
	fn try_from_iter_rejects_too_many_entries() {
		let result = RequiresSet::try_from_iter(
			(0..=MAX_COMMITMENT_ENTRIES).map(|i| (ParaId::from(i), r(1))),
		);

		assert_eq!(result, Err(RequiresSetError::TooManyEntries));
	}

	#[test]
	fn encode_decode_round_trip_works() {
		let set =
			RequiresSet::try_from_iter([(ParaId::from(1), r(1)), (ParaId::from(2), r(2))]).unwrap();

		let encoded = set.encode();
		let decoded = RequiresSet::decode(&mut &encoded[..]).unwrap();

		assert_eq!(set, decoded);
	}

	#[test]
	fn decode_rejects_out_of_order_para_ids() {
		let bad: Vec<(ParaId, StreamsRoot)> =
			vec![(ParaId::from(2), r(2)), (ParaId::from(1), r(1))];
		let encoded = bad.encode();

		assert!(RequiresSet::decode(&mut &encoded[..]).is_err());
	}

	#[test]
	fn decode_rejects_duplicate_para_ids() {
		let bad: Vec<(ParaId, StreamsRoot)> =
			vec![(ParaId::from(1), r(1)), (ParaId::from(1), r(2))];
		let encoded = bad.encode();

		assert!(RequiresSet::decode(&mut &encoded[..]).is_err());
	}

	#[test]
	fn get_finds_existing_and_missing_entries() {
		let set =
			RequiresSet::try_from_iter([(ParaId::from(1), r(1)), (ParaId::from(3), r(3))]).unwrap();

		assert_eq!(set.get(ParaId::from(1)), Some(&r(1)));
		assert_eq!(set.get(ParaId::from(3)), Some(&r(3)));
		assert_eq!(set.get(ParaId::from(2)), None);
	}

	#[test]
	fn streams_root_is_distinct_type() {
		// Compile-time property really; keep a runtime witness that encoding
		// is the bare 32 bytes.
		assert_eq!(r(7).encode(), Hash::repeat_byte(7).encode());
	}
}
