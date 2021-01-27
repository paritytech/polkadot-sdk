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

//! Defines traits which represent a common interface for Substrate pallets which want to
//! incorporate bridge functionality.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Codec, Decode, Encode, EncodeLike};
use core::clone::Clone;
use core::cmp::Eq;
use core::default::Default;
use core::fmt::Debug;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_finality_grandpa::{AuthorityList, SetId};
use sp_runtime::RuntimeDebug;

pub mod justification;

/// A type that can be used as a parameter in a dispatchable function.
///
/// When using `decl_module` all arguments for call functions must implement this trait.
pub trait Parameter: Codec + EncodeLike + Clone + Eq + Debug {}
impl<T> Parameter for T where T: Codec + EncodeLike + Clone + Eq + Debug {}

/// A GRANDPA Authority List and ID.
#[derive(Default, Encode, Decode, RuntimeDebug, PartialEq, Clone)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct AuthoritySet {
	/// List of GRANDPA authorities for the current round.
	pub authorities: AuthorityList,
	/// Monotonic identifier of the current GRANDPA authority set.
	pub set_id: SetId,
}

impl AuthoritySet {
	/// Create a new GRANDPA Authority Set.
	pub fn new(authorities: AuthorityList, set_id: SetId) -> Self {
		Self { authorities, set_id }
	}
}

/// base trait for verifying transaction inclusion proofs.
pub trait InclusionProofVerifier {
	/// Transaction type.
	type Transaction: Parameter;
	/// Transaction inclusion proof type.
	type TransactionInclusionProof: Parameter;

	/// Verify that transaction is a part of given block.
	///
	/// Returns Some(transaction) if proof is valid and None otherwise.
	fn verify_transaction_inclusion_proof(proof: &Self::TransactionInclusionProof) -> Option<Self::Transaction>;
}

/// A base trait for pallets which want to keep track of a full set of headers from a bridged chain.
pub trait HeaderChain<H, E> {
	/// Get the best finalized header known to the header chain.
	fn best_finalized() -> H;

	/// Get the best authority set known to the header chain.
	fn authority_set() -> AuthoritySet;

	/// Write a finalized chain of headers to the underlying pallet storage.
	///
	/// It is assumed that each header in this chain been finalized, and that the given headers are
	/// in order (e.g vec![header_1, header_2, ..., header_n]).
	///
	/// This function should fail if the first header is not a child of the current best finalized
	/// header known to the underlying pallet storage.
	fn append_finalized_chain(headers: impl IntoIterator<Item = H>) -> Result<(), E>;
}

impl<H: Default, E> HeaderChain<H, E> for () {
	fn best_finalized() -> H {
		H::default()
	}

	fn authority_set() -> AuthoritySet {
		AuthoritySet::default()
	}

	fn append_finalized_chain(_headers: impl IntoIterator<Item = H>) -> Result<(), E> {
		Ok(())
	}
}

/// A trait for checking if a given child header is a direct descendant of an ancestor.
pub trait AncestryChecker<H, P> {
	/// Is the child header a descendant of the ancestor header?
	fn are_ancestors(ancestor: &H, child: &H, proof: &P) -> bool;
}

impl<H, P> AncestryChecker<H, P> for () {
	fn are_ancestors(_ancestor: &H, _child: &H, _proof: &P) -> bool {
		true
	}
}
