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

//! Requests and responses as sent over the wire for the individual protocols.

use parity_scale_codec::{Decode, Encode};

use polkadot_node_primitives::PoV;
use polkadot_primitives::{CandidateHash, CandidateReceipt, Hash, HeadData, Id as ParaId};

use super::{IsRequest, Protocol};

/// Request the advertised collation at that relay-parent.
// Same as v2.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CollationFetchingRequest {
	/// Relay parent collation is built on top of.
	pub relay_parent: Hash,
	/// The `ParaId` of the collation.
	pub para_id: ParaId,
	/// Candidate hash.
	pub candidate_hash: CandidateHash,
}

/// Responses as sent by collators.
#[derive(Debug, Clone, Encode, Decode)]
pub enum CollationFetchingResponse {
	/// Deliver requested collation along with parent head data.
	#[codec(index = 1)]
	Collation {
		/// The receipt of the candidate.
		receipt: CandidateReceipt,
		/// Candidate's proof of validity.
		pov: PoV,
		/// The head data of the candidate's parent.
		/// This is needed for elastic scaling to work.
		parent_head_data: HeadData,
	},
}

impl IsRequest for CollationFetchingRequest {
	type Response = CollationFetchingResponse;
	const PROTOCOL: Protocol = Protocol::CollationFetchingV3;
}
