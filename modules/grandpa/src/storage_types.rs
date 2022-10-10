// Copyright 2022 Parity Technologies (UK) Ltd.
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

//! Wrappers for public types that are implementing `MaxEncodedLen`

use crate::{BridgedHeader, Config, Error};

use bp_header_chain::AuthoritySet;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{traits::Get, BoundedVec, RuntimeDebugNoBound};
use scale_info::{Type, TypeInfo};
use sp_finality_grandpa::{AuthorityId, AuthorityList, AuthorityWeight, SetId};

/// A bounded list of Grandpa authorities with associated weights.
pub type StoredAuthorityList<MaxBridgedAuthorities> =
	BoundedVec<(AuthorityId, AuthorityWeight), MaxBridgedAuthorities>;

/// A bounded GRANDPA Authority List and ID.
#[derive(Clone, Decode, Encode, Eq, TypeInfo, MaxEncodedLen, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(T, I))]
pub struct StoredAuthoritySet<T: Config<I>, I: 'static> {
	/// List of GRANDPA authorities for the current round.
	pub authorities: StoredAuthorityList<<T as Config<I>>::MaxBridgedAuthorities>,
	/// Monotonic identifier of the current GRANDPA authority set.
	pub set_id: SetId,
}

impl<T: Config<I>, I: 'static> StoredAuthoritySet<T, I> {
	/// Try to create a new bounded GRANDPA Authority Set from unbounded list.
	///
	/// Returns error if number of authorities in the provided list is too large.
	pub fn try_new(authorities: AuthorityList, set_id: SetId) -> Result<Self, ()> {
		Ok(Self { authorities: TryFrom::try_from(authorities)?, set_id })
	}
}

impl<T: Config<I>, I: 'static> PartialEq for StoredAuthoritySet<T, I> {
	fn eq(&self, other: &Self) -> bool {
		self.set_id == other.set_id && self.authorities == other.authorities
	}
}

impl<T: Config<I>, I: 'static> Default for StoredAuthoritySet<T, I> {
	fn default() -> Self {
		StoredAuthoritySet { authorities: BoundedVec::default(), set_id: 0 }
	}
}

impl<T: Config<I>, I: 'static> From<StoredAuthoritySet<T, I>> for AuthoritySet {
	fn from(t: StoredAuthoritySet<T, I>) -> Self {
		AuthoritySet { authorities: t.authorities.into(), set_id: t.set_id }
	}
}

/// A bounded chain header.
#[derive(Clone, Decode, Encode, Eq, PartialEq, RuntimeDebugNoBound)]
pub struct StoredBridgedHeader<T: Config<I>, I: 'static>(pub BridgedHeader<T, I>);

impl<T: Config<I>, I: 'static> StoredBridgedHeader<T, I> {
	/// Construct `StoredBridgedHeader` from the `BridgedHeader` with all required checks.
	pub fn try_from_bridged_header(header: BridgedHeader<T, I>) -> Result<Self, Error<T, I>> {
		// this conversion is heavy (since we do encoding here), so we may want to optimize it later
		// (e.g. by introducing custom Encode implementation, and turning `StoredBridgedHeader` into
		// `enum StoredBridgedHeader { Decoded(BridgedHeader), Encoded(Vec<u8>) }`)
		if header.encoded_size() > T::MaxBridgedHeaderSize::get() as usize {
			Err(Error::TooLargeHeader)
		} else {
			Ok(StoredBridgedHeader(header))
		}
	}
}

impl<T: Config<I>, I: 'static> sp_std::ops::Deref for StoredBridgedHeader<T, I> {
	type Target = BridgedHeader<T, I>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<T: Config<I>, I: 'static> TypeInfo for StoredBridgedHeader<T, I> {
	type Identity = Self;

	fn type_info() -> Type {
		BridgedHeader::<T, I>::type_info()
	}
}

impl<T: Config<I>, I: 'static> MaxEncodedLen for StoredBridgedHeader<T, I> {
	fn max_encoded_len() -> usize {
		T::MaxBridgedHeaderSize::get() as usize
	}
}
