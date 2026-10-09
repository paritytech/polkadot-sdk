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

use alloc::vec::Vec;
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

/// A block's outgoing MMR commitments, keyed by destination [`ParaId`].
///
/// Each entry maps a destination parachain to the root of the MMR built over
/// all messages sent to it in this block. Entries are kept sorted by `ParaId`
/// so that the encoded form is canonical — collators, PVF, and the relay chain
/// all produce the same bytes for the same set of commitments.
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
pub struct CommitmentSet<const N: u32>(BoundedVec<(ParaId, Hash), ConstU32<N>>);

/// Decode is manually implemented to ensure that ParaID is sorted in increasing order.
impl<const N: u32> codec::Decode for CommitmentSet<N> {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let inner = BoundedVec::<(ParaId, Hash), ConstU32<N>>::decode(input)?;

		for pair in inner.windows(2) {
			if pair[0].0 >= pair[1].0 {
				return Err(codec::Error::from(
					"CommitmentSet entries must be sorted by increasing ParaId",
				));
			}
		}

		Ok(Self(inner))
	}
}

impl<const N: u32> CommitmentSet<N> {
	/// Returns the MMR root committed to by `para_id`, or `None` if not present.
	pub fn get(&self, para_id: ParaId) -> Option<&Hash> {
		self.0
			.binary_search_by_key(&para_id, |(id, _)| *id)
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

	/// Iterates over all `(ParaId, Hash)` entries in sorted order.
	pub fn iter(&self) -> impl Iterator<Item = &(ParaId, Hash)> {
		self.0.iter()
	}

	/// Builds a [`CommitmentSet`] from an arbitrary (possibly unordered) iterator,
	/// sorting entries by `ParaId` to produce the encoding.
	pub fn try_from_iter(
		it: impl IntoIterator<Item = (ParaId, Hash)>,
	) -> Result<Self, CommitmentError> {
		let mut entries: Vec<(ParaId, Hash)> = it.into_iter().collect();
		entries.sort_by_key(|(para_id, _)| *para_id);

		if entries.windows(2).any(|w| w[0].0 == w[1].0) {
			return Err(CommitmentError::DuplicateParaId);
		}

		let inner = BoundedVec::try_from(entries).map_err(|_| CommitmentError::TooManyEntries)?;

		Ok(Self(inner))
	}
}

impl<'a, const N: u32> IntoIterator for &'a CommitmentSet<N> {
	type Item = &'a (ParaId, Hash);
	type IntoIter = core::slice::Iter<'a, (ParaId, Hash)>;

	fn into_iter(self) -> Self::IntoIter {
		self.0.iter()
	}
}

/// Errors that can occur when constructing a [`CommitmentSet`]
#[derive(Debug, PartialEq, Eq)]
pub enum CommitmentError {
	/// The same `ParaId` appears more than once.
	DuplicateParaId,
	/// More entries were provided than the bound `N` allows.
	TooManyEntries,
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};

	fn h(byte: u8) -> Hash {
		Hash::repeat_byte(byte)
	}

	#[test]
	fn try_from_iter_sorts_entries() {
		let set = CommitmentSet::<4>::try_from_iter([
			(ParaId::from(3), h(3)),
			(ParaId::from(1), h(1)),
			(ParaId::from(2), h(2)),
		])
		.unwrap();

		let ids: Vec<ParaId> = set.iter().map(|(id, _)| *id).collect();
		assert_eq!(ids, vec![ParaId::from(1), ParaId::from(2), ParaId::from(3)]);
	}

	#[test]
	fn try_from_iter_rejects_duplicate_para_id() {
		let result =
			CommitmentSet::<4>::try_from_iter([(ParaId::from(1), h(1)), (ParaId::from(1), h(2))]);

		assert_eq!(result, Err(CommitmentError::DuplicateParaId));
	}

	#[test]
	fn try_from_iter_rejects_too_many_entries() {
		let result = CommitmentSet::<2>::try_from_iter([
			(ParaId::from(1), h(1)),
			(ParaId::from(2), h(2)),
			(ParaId::from(3), h(3)),
		]);

		assert_eq!(result, Err(CommitmentError::TooManyEntries));
	}

	#[test]
	fn encode_decode_round_trip_works() {
		let set =
			CommitmentSet::<4>::try_from_iter([(ParaId::from(1), h(1)), (ParaId::from(2), h(2))])
				.unwrap();

		let encoded = set.encode();
		let decoded = CommitmentSet::<4>::decode(&mut &encoded[..]).unwrap();

		assert_eq!(set, decoded);
	}

	#[test]
	fn decode_rejects_out_of_order_para_ids() {
		let bad: Vec<(ParaId, Hash)> = vec![(ParaId::from(2), h(2)), (ParaId::from(1), h(1))];
		let encoded = bad.encode();

		assert!(CommitmentSet::<4>::decode(&mut &encoded[..]).is_err());
	}

	#[test]
	fn decode_rejects_duplicate_para_ids() {
		let bad: Vec<(ParaId, Hash)> = vec![(ParaId::from(1), h(1)), (ParaId::from(1), h(2))];
		let encoded = bad.encode();

		assert!(CommitmentSet::<4>::decode(&mut &encoded[..]).is_err());
	}

	#[test]
	fn decode_rejects_too_many_entries() {
		let bad: Vec<(ParaId, Hash)> =
			vec![(ParaId::from(1), h(1)), (ParaId::from(2), h(2)), (ParaId::from(3), h(3))];
		let encoded = bad.encode();

		assert!(CommitmentSet::<2>::decode(&mut &encoded[..]).is_err());
	}

	#[test]
	fn get_finds_existing_and_missing_entries() {
		let set =
			CommitmentSet::<4>::try_from_iter([(ParaId::from(1), h(1)), (ParaId::from(3), h(3))])
				.unwrap();

		assert_eq!(set.get(ParaId::from(1)), Some(&h(1)));
		assert_eq!(set.get(ParaId::from(3)), Some(&h(3)));
		assert_eq!(set.get(ParaId::from(2)), None);
	}

	#[test]
	fn len_and_is_empty() {
		let empty = CommitmentSet::<4>::default();
		assert!(empty.is_empty());
		assert_eq!(empty.len(), 0);

		let set = CommitmentSet::<4>::try_from_iter([(ParaId::from(1), h(1))]).unwrap();
		assert!(!set.is_empty());
		assert_eq!(set.len(), 1);
	}

	#[test]
	fn into_iter_yields_sorted_entries() {
		let set =
			CommitmentSet::<4>::try_from_iter([(ParaId::from(2), h(2)), (ParaId::from(1), h(1))])
				.unwrap();

		let collected: Vec<_> = (&set).into_iter().collect();
		assert_eq!(collected, vec![&(ParaId::from(1), h(1)), &(ParaId::from(2), h(2))]);
	}
}
