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

use alloc::vec::Vec;
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

/// Canonical set ensuring collator / pvf / relay cannot disagree on the shape.
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
	pub fn get(&self, para_id: ParaId) -> Option<&Hash> {
		self.0
			.binary_search_by_key(&para_id, |(id, _)| *id)
			.ok()
			.map(|idx| &self.0[idx].1)
	}

	pub fn len(&self) -> usize {
		self.0.len()
	}

	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

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
