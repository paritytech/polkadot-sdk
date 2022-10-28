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

use bp_runtime::BasicOperatingMode;
use codec::{Codec, Decode, Encode, EncodeLike};
use core::{clone::Clone, cmp::Eq, default::Default, fmt::Debug};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_finality_grandpa::{AuthorityList, ConsensusLog, SetId, GRANDPA_ENGINE_ID};
use sp_runtime::{traits::Header as HeaderT, Digest, RuntimeDebug};
use sp_std::boxed::Box;

pub mod justification;
pub mod storage_keys;

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
