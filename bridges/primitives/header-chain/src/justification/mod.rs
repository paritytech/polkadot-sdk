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

//! Logic for checking GRANDPA Finality Proofs.
//!
//! Adapted copy of substrate/client/finality-grandpa/src/justification.rs. If origin
//! will ever be moved to the sp_consensus_grandpa, we should reuse that implementation.

mod verification;

use crate::ChainWithGrandpa;
pub use verification::{
	equivocation::{EquivocationsCollector, GrandpaEquivocationsFinder},
	optimizer::verify_and_optimize_justification,
	strict::verify_justification,
	AncestryChain, Error as JustificationVerificationError, JustificationVerificationContext,
	PrecommitError,
};

use bp_runtime::{BlockNumberOf, Chain, HashOf, HeaderId};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::RuntimeDebugNoBound;
use scale_info::TypeInfo;
use sp_consensus_grandpa::{AuthorityId, AuthoritySignature};
use sp_runtime::{traits::Header as HeaderT, RuntimeDebug, SaturatedConversion};
use sp_std::prelude::*;

/// A GRANDPA Justification is a proof that a given header was finalized
/// at a certain height and with a certain set of authorities.
///
/// This particular proof is used to prove that headers on a bridged chain
/// (so not our chain) have been finalized correctly.
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeDebugNoBound)]
pub struct GrandpaJustification<Header: HeaderT> {
	/// The round (voting period) this justification is valid for.
	pub round: u64,
	/// The set of votes for the chain which is to be finalized.
	pub commit:
		finality_grandpa::Commit<Header::Hash, Header::Number, AuthoritySignature, AuthorityId>,
	/// A proof that the chain of blocks in the commit are related to each other.
	pub votes_ancestries: Vec<Header>,
}

impl<H: HeaderT> GrandpaJustification<H> {
	/// Returns reasonable size of justification using constants from the provided chain.
	///
	/// An imprecise analogue of `MaxEncodedLen` implementation. We don't use it for
	/// any precise calculations - that's just an estimation.
	pub fn max_reasonable_size<C>(required_precommits: u32) -> u32
	where
		C: Chain + ChainWithGrandpa,
	{
		// we don't need precise results here - just estimations, so some details
		// are removed from computations (e.g. bytes required to encode vector length)

		// structures in `finality_grandpa` crate are not implementing `MaxEncodedLength`, so
		// here's our estimation for the `finality_grandpa::Commit` struct size
		//
		// precommit is: hash + number
		// signed precommit is: precommit + signature (64b) + authority id
		// commit is: hash + number + vec of signed precommits
		let signed_precommit_size: u32 = BlockNumberOf::<C>::max_encoded_len()
			.saturating_add(HashOf::<C>::max_encoded_len().saturated_into())
			.saturating_add(64)
			.saturating_add(AuthorityId::max_encoded_len().saturated_into())
			.saturated_into();
		let max_expected_signed_commit_size = signed_precommit_size
			.saturating_mul(required_precommits)
			.saturating_add(BlockNumberOf::<C>::max_encoded_len().saturated_into())
			.saturating_add(HashOf::<C>::max_encoded_len().saturated_into());

		let max_expected_votes_ancestries_size =
			C::REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY.saturating_mul(C::AVERAGE_HEADER_SIZE);

		// justification is round number (u64=8b), a signed GRANDPA commit and the
		// `votes_ancestries` vector
		8u32.saturating_add(max_expected_signed_commit_size)
			.saturating_add(max_expected_votes_ancestries_size)
	}

	/// Return identifier of header that this justification claims to finalize.
	pub fn commit_target_id(&self) -> HeaderId<H::Hash, H::Number> {
		HeaderId(self.commit.target_number, self.commit.target_hash)
	}
}

impl<H: HeaderT> crate::FinalityProof<H::Hash, H::Number> for GrandpaJustification<H> {
	fn target_header_hash(&self) -> H::Hash {
		self.commit.target_hash
	}

	fn target_header_number(&self) -> H::Number {
		self.commit.target_number
	}
}

/// Justification verification error.
#[derive(Eq, RuntimeDebug, PartialEq)]
pub enum Error {
	/// Failed to decode justification.
	JustificationDecode,
}

/// Given GRANDPA authorities set size, return number of valid authorities votes that the
/// justification must have to be valid.
///
/// This function assumes that all authorities have the same vote weight.
pub fn required_justification_precommits(authorities_set_length: u32) -> u32 {
	authorities_set_length - authorities_set_length.saturating_sub(1) / 3
}

/// Decode justification target.
pub fn decode_justification_target<Header: HeaderT>(
	raw_justification: &[u8],
) -> Result<(Header::Hash, Header::Number), Error> {
	GrandpaJustification::<Header>::decode(&mut &*raw_justification)
		.map(|justification| (justification.commit.target_hash, justification.commit.target_number))
		.map_err(|_| Error::JustificationDecode)
}
