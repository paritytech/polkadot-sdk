// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::{Config, Error};

use bp_header_chain::{AuthoritySet, ChainWithGrandpa};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{traits::Get, BoundedVec, CloneNoBound, RuntimeDebugNoBound};
use scale_info::TypeInfo;
use sp_consensus_grandpa::{AuthorityId, AuthorityList, AuthorityWeight, SetId};
use sp_std::marker::PhantomData;

/// A bounded list of Grandpa authorities with associated weights.
pub type StoredAuthorityList<MaxBridgedAuthorities> =
	BoundedVec<(AuthorityId, AuthorityWeight), MaxBridgedAuthorities>;

/// Adapter for using `T::BridgedChain::MAX_BRIDGED_AUTHORITIES` in `BoundedVec`.
pub struct StoredAuthorityListLimit<T, I>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> Get<u32> for StoredAuthorityListLimit<T, I> {
	fn get() -> u32 {
		T::BridgedChain::MAX_AUTHORITIES_COUNT
	}
}

/// A bounded GRANDPA Authority List and ID.
#[derive(CloneNoBound, Decode, Encode, Eq, TypeInfo, MaxEncodedLen, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(T, I))]
pub struct StoredAuthoritySet<T: Config<I>, I: 'static> {
	/// List of GRANDPA authorities for the current round.
	pub authorities: StoredAuthorityList<StoredAuthorityListLimit<T, I>>,
	/// Monotonic identifier of the current GRANDPA authority set.
	pub set_id: SetId,
}

impl<T: Config<I>, I: 'static> StoredAuthoritySet<T, I> {
	/// Try to create a new bounded GRANDPA Authority Set from unbounded list.
	///
	/// Returns error if number of authorities in the provided list is too large.
	pub fn try_new(authorities: AuthorityList, set_id: SetId) -> Result<Self, Error<T, I>> {
		Ok(Self {
			authorities: TryFrom::try_from(authorities)
				.map_err(|_| Error::TooManyAuthoritiesInSet)?,
			set_id,
		})
	}

	/// Returns number of bytes that may be subtracted from the PoV component of
	/// `submit_finality_proof` call, because the actual authorities set is smaller than the maximal
	/// configured.
	///
	/// Maximal authorities set size is configured by the `MaxBridgedAuthorities` constant from
	/// the pallet configuration. The PoV of the call includes the size of maximal authorities
	/// count. If the actual size is smaller, we may subtract extra bytes from this component.
	pub fn unused_proof_size(&self) -> u64 {
		// we can only safely estimate bytes that are occupied by the authority data itself. We have
		// no means here to compute PoV bytes, occupied by extra trie nodes or extra bytes in the
		// whole set encoding
		let single_authority_max_encoded_len =
			<(AuthorityId, AuthorityWeight)>::max_encoded_len() as u64;
		let extra_authorities =
			T::BridgedChain::MAX_AUTHORITIES_COUNT.saturating_sub(self.authorities.len() as _);
		single_authority_max_encoded_len.saturating_mul(extra_authorities as u64)
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

#[cfg(test)]
mod tests {
	use crate::mock::{TestRuntime, MAX_BRIDGED_AUTHORITIES};
	use bp_test_utils::authority_list;

	type StoredAuthoritySet = super::StoredAuthoritySet<TestRuntime, ()>;

	#[test]
	fn unused_proof_size_works() {
		let authority_entry = authority_list().pop().unwrap();

		// when we have exactly `MaxBridgedAuthorities` authorities
		assert_eq!(
			StoredAuthoritySet::try_new(
				vec![authority_entry.clone(); MAX_BRIDGED_AUTHORITIES as usize],
				0,
			)
			.unwrap()
			.unused_proof_size(),
			0,
		);

		// when we have less than `MaxBridgedAuthorities` authorities
		assert_eq!(
			StoredAuthoritySet::try_new(
				vec![authority_entry; MAX_BRIDGED_AUTHORITIES as usize - 1],
				0,
			)
			.unwrap()
			.unused_proof_size(),
			40,
		);

		// and we can't have more than `MaxBridgedAuthorities` authorities in the bounded vec, so
		// no test for this case
	}
}
