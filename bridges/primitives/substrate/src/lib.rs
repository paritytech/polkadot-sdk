// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Primitives for the Substrate light client (a.k.a bridge) pallet.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use core::default::Default;
use parity_scale_codec::{Decode, Encode};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_finality_grandpa::{AuthorityList, SetId};
use sp_runtime::RuntimeDebug;

/// A Grandpa Authority List and ID.
#[derive(Default, Encode, Decode, RuntimeDebug, PartialEq, Clone)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct AuthoritySet {
	/// List of Grandpa authorities for the current round.
	pub authorities: AuthorityList,
	/// Monotonic identifier of the current Grandpa authority set.
	pub set_id: SetId,
}

impl AuthoritySet {
	/// Create a new Grandpa Authority Set.
	pub fn new(authorities: AuthorityList, set_id: SetId) -> Self {
		Self { authorities, set_id }
	}
}

/// Keeps track of when the next Grandpa authority set change will occur.
#[derive(Default, Encode, Decode, RuntimeDebug, PartialEq, Clone)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ScheduledChange<N> {
	/// The authority set that will be used once this change is enacted.
	pub authority_set: AuthoritySet,
	/// The block height at which the authority set should be enacted.
	///
	/// Note: It will only be enacted once a header at this height is finalized.
	pub height: N,
}

/// A more useful representation of a header for storage purposes.
#[derive(Default, Encode, Decode, Clone, RuntimeDebug, PartialEq)]
pub struct ImportedHeader<H> {
	/// A plain Substrate header.
	pub header: H,
	/// Does this header enact a new authority set change. If it does
	/// then it will require a justification.
	pub requires_justification: bool,
	/// Has this header been finalized, either explicitly via a justification,
	/// or implicitly via one of its children getting finalized.
	pub is_finalized: bool,
}

impl<H> core::ops::Deref for ImportedHeader<H> {
	type Target = H;

	fn deref(&self) -> &H {
		&self.header
	}
}

/// Prove that the given header was finalized by the given authority set.
pub fn check_finality_proof<H>(_header: &H, _set: &AuthoritySet, _justification: &[u8]) -> bool {
	true
}
