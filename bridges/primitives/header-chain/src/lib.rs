// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use bp_runtime::{
	BasicOperatingMode, Chain, HashOf, HasherOf, HeaderOf, RawStorageProof, StorageProofChecker,
	StorageProofError,
};
use codec::{Codec, Decode, Encode, EncodeLike, MaxEncodedLen};
use core::{clone::Clone, cmp::Eq, default::Default, fmt::Debug};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_finality_grandpa::{AuthorityList, ConsensusLog, SetId, GRANDPA_ENGINE_ID};
use sp_runtime::{traits::Header as HeaderT, Digest, RuntimeDebug};
use sp_std::boxed::Box;

pub mod justification;
pub mod storage_keys;

/// Header chain error.
#[derive(Clone, Eq, PartialEq, RuntimeDebug)]
pub enum HeaderChainError {
	/// Header with given hash is missing from the chain.
	UnknownHeader,
	/// Storage proof related error.
	StorageProof(StorageProofError),
}

impl From<HeaderChainError> for &'static str {
	fn from(err: HeaderChainError) -> &'static str {
		match err {
			HeaderChainError::UnknownHeader => "UnknownHeader",
			HeaderChainError::StorageProof(e) => e.into(),
		}
	}
}

/// Header data that we're storing on-chain.
///
/// Even though we may store full header, our applications (XCM) only use couple of header
/// fields. Extracting those values makes on-chain storage and PoV smaller, which is good.
#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct StoredHeaderData<Number, Hash> {
	/// Header number.
	pub number: Number,
	/// Header state root.
	pub state_root: Hash,
}

/// Stored header data builder.
pub trait StoredHeaderDataBuilder<Number, Hash> {
	/// Build header data from self.
	fn build(&self) -> StoredHeaderData<Number, Hash>;
}

impl<H: HeaderT> StoredHeaderDataBuilder<H::Number, H::Hash> for H {
	fn build(&self) -> StoredHeaderData<H::Number, H::Hash> {
		StoredHeaderData { number: *self.number(), state_root: *self.state_root() }
	}
}

/// Substrate header chain, abstracted from the way it is stored.
pub trait HeaderChain<C: Chain> {
	/// Returns state (storage) root of given finalized header.
	fn finalized_header_state_root(header_hash: HashOf<C>) -> Option<HashOf<C>>;
	/// Parse storage proof using finalized header.
	fn parse_finalized_storage_proof<R>(
		header_hash: HashOf<C>,
		storage_proof: RawStorageProof,
		parse: impl FnOnce(StorageProofChecker<HasherOf<C>>) -> R,
	) -> Result<R, HeaderChainError> {
		let state_root = Self::finalized_header_state_root(header_hash)
			.ok_or(HeaderChainError::UnknownHeader)?;
		let storage_proof_checker = bp_runtime::StorageProofChecker::new(state_root, storage_proof)
			.map_err(HeaderChainError::StorageProof)?;

		Ok(parse(storage_proof_checker))
	}
}

/// A type that can be used as a parameter in a dispatchable function.
///
/// When using `decl_module` all arguments for call functions must implement this trait.
pub trait Parameter: Codec + EncodeLike + Clone + Eq + Debug + TypeInfo {}
impl<T> Parameter for T where T: Codec + EncodeLike + Clone + Eq + Debug + TypeInfo {}

/// A GRANDPA Authority List and ID.
#[derive(Default, Encode, Eq, Decode, RuntimeDebug, PartialEq, Clone, TypeInfo)]
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

/// Data required for initializing the bridge pallet.
///
/// The bridge needs to know where to start its sync from, and this provides that initial context.
#[derive(Default, Encode, Decode, RuntimeDebug, PartialEq, Eq, Clone, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct InitializationData<H: HeaderT> {
	/// The header from which we should start syncing.
	pub header: Box<H>,
	/// The initial authorities of the pallet.
	pub authority_list: AuthorityList,
	/// The ID of the initial authority set.
	pub set_id: SetId,
	/// Pallet operating mode.
	pub operating_mode: BasicOperatingMode,
}

/// Abstract finality proof that is justifying block finality.
pub trait FinalityProof<Number>: Clone + Send + Sync + Debug {
	/// Return number of header that this proof is generated for.
	fn target_header_number(&self) -> Number;
}

/// A trait that provides helper methods for querying the consensus log.
pub trait ConsensusLogReader {
	/// Returns true if digest contains item that schedules authorities set change.
	fn schedules_authorities_change(digest: &Digest) -> bool;
}

/// A struct that provides helper methods for querying the GRANDPA consensus log.
pub struct GrandpaConsensusLogReader<Number>(sp_std::marker::PhantomData<Number>);

impl<Number: Codec> GrandpaConsensusLogReader<Number> {
	pub fn find_authorities_change(
		digest: &Digest,
	) -> Option<sp_finality_grandpa::ScheduledChange<Number>> {
		// find the first consensus digest with the right ID which converts to
		// the right kind of consensus log.
		digest
			.convert_first(|log| log.consensus_try_to(&GRANDPA_ENGINE_ID))
			.and_then(|log| match log {
				ConsensusLog::ScheduledChange(change) => Some(change),
				_ => None,
			})
	}
}

impl<Number: Codec> ConsensusLogReader for GrandpaConsensusLogReader<Number> {
	fn schedules_authorities_change(digest: &Digest) -> bool {
		GrandpaConsensusLogReader::<Number>::find_authorities_change(digest).is_some()
	}
}

/// A minimized version of `pallet-bridge-grandpa::Call` that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeGrandpaCall<Header: HeaderT> {
	/// `pallet-bridge-grandpa::Call::submit_finality_proof`
	#[codec(index = 0)]
	submit_finality_proof {
		finality_target: Box<Header>,
		justification: justification::GrandpaJustification<Header>,
	},
	/// `pallet-bridge-grandpa::Call::initialize`
	#[codec(index = 1)]
	initialize { init_data: InitializationData<Header> },
}

/// The `BridgeGrandpaCall` used by a chain.
pub type BridgeGrandpaCallOf<C> = BridgeGrandpaCall<HeaderOf<C>>;
