// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Network packet message types. These get serialized and put into the lower level protocol
//! payload.

use codec::{Decode, Encode};
use sc_client_api::StorageProof;
use sc_network_common::message::RequestId;

/// Remote call response.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
#[allow(dead_code)]
pub struct RemoteCallResponse {
	/// Id of a request this response was made for.
	pub id: RequestId,
	/// Execution proof.
	pub proof: StorageProof,
}

#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
#[allow(dead_code)]
/// Remote read response.
pub struct RemoteReadResponse {
	/// Id of a request this response was made for.
	pub id: RequestId,
	/// Read proof.
	pub proof: StorageProof,
}

/// Generic types.
pub mod generic {
	use codec::{Decode, Encode, Input};
	use sc_client_api::StorageProof;
	use sc_network_common::{message::RequestId, role::Roles};
	use sp_runtime::ConsensusEngineId;

	/// Consensus is mostly opaque to us
	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	#[allow(dead_code)]
	pub struct ConsensusMessage {
		/// Identifies consensus engine.
		pub protocol: ConsensusEngineId,
		/// Message payload.
		pub data: Vec<u8>,
	}

	/// Status sent on connection.
	// TODO https://github.com/paritytech/substrate/issues/4674: replace the `Status`
	// struct with this one, after waiting a few releases beyond `NetworkSpecialization`'s
	// removal (https://github.com/paritytech/substrate/pull/4665)
	//
	// and set MIN_VERSION to 6.
	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	#[allow(dead_code)]
	pub struct CompactStatus<Hash, Number> {
		/// Protocol version.
		pub version: u32,
		/// Minimum supported version.
		pub min_supported_version: u32,
		/// Supported roles.
		pub roles: Roles,
		/// Best block number.
		pub best_number: Number,
		/// Best block hash.
		pub best_hash: Hash,
		/// Genesis block hash.
		pub genesis_hash: Hash,
	}

	/// Status sent on connection.
	#[derive(Debug, PartialEq, Eq, Clone, Encode)]
	#[allow(dead_code)]
	pub struct Status<Hash, Number> {
		/// Protocol version.
		pub version: u32,
		/// Minimum supported version.
		pub min_supported_version: u32,
		/// Supported roles.
		pub roles: Roles,
		/// Best block number.
		pub best_number: Number,
		/// Best block hash.
		pub best_hash: Hash,
		/// Genesis block hash.
		pub genesis_hash: Hash,
		/// DEPRECATED. Chain-specific status.
		pub chain_status: Vec<u8>,
	}

	impl<Hash: Decode, Number: Decode> Decode for Status<Hash, Number> {
		fn decode<I: Input>(value: &mut I) -> Result<Self, codec::Error> {
			const LAST_CHAIN_STATUS_VERSION: u32 = 5;
			let compact = CompactStatus::decode(value)?;
			let chain_status = match <Vec<u8>>::decode(value) {
				Ok(v) => v,
				Err(e) =>
					if compact.version <= LAST_CHAIN_STATUS_VERSION {
						return Err(e)
					} else {
						Vec::new()
					},
			};

			let CompactStatus {
				version,
				min_supported_version,
				roles,
				best_number,
				best_hash,
				genesis_hash,
			} = compact;

			Ok(Self {
				version,
				min_supported_version,
				roles,
				best_number,
				best_hash,
				genesis_hash,
				chain_status,
			})
		}
	}

	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	#[allow(dead_code)]
	/// Remote call request.
	pub struct RemoteCallRequest<H> {
		/// Unique request id.
		pub id: RequestId,
		/// Block at which to perform call.
		pub block: H,
		/// Method name.
		pub method: String,
		/// Call data.
		pub data: Vec<u8>,
	}

	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	#[allow(dead_code)]
	/// Remote storage read request.
	pub struct RemoteReadRequest<H> {
		/// Unique request id.
		pub id: RequestId,
		/// Block at which to perform call.
		pub block: H,
		/// Storage key.
		pub keys: Vec<Vec<u8>>,
	}

	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	#[allow(dead_code)]
	/// Remote storage read child request.
	pub struct RemoteReadChildRequest<H> {
		/// Unique request id.
		pub id: RequestId,
		/// Block at which to perform call.
		pub block: H,
		/// Child Storage key.
		pub storage_key: Vec<u8>,
		/// Storage key.
		pub keys: Vec<Vec<u8>>,
	}

	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	#[allow(dead_code)]
	/// Remote header request.
	pub struct RemoteHeaderRequest<N> {
		/// Unique request id.
		pub id: RequestId,
		/// Block number to request header for.
		pub block: N,
	}

	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	#[allow(dead_code)]
	/// Remote header response.
	pub struct RemoteHeaderResponse<Header> {
		/// Id of a request this response was made for.
		pub id: RequestId,
		/// Header. None if proof generation has failed (e.g. header is unknown).
		pub header: Option<Header>,
		/// Header proof.
		pub proof: StorageProof,
	}

	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	#[allow(dead_code)]
	/// Remote changes request.
	pub struct RemoteChangesRequest<H> {
		/// Unique request id.
		pub id: RequestId,
		/// Hash of the first block of the range (including first) where changes are requested.
		pub first: H,
		/// Hash of the last block of the range (including last) where changes are requested.
		pub last: H,
		/// Hash of the first block for which the requester has the changes trie root. All other
		/// affected roots must be proved.
		pub min: H,
		/// Hash of the last block that we can use when querying changes.
		pub max: H,
		/// Storage child node key which changes are requested.
		pub storage_key: Option<Vec<u8>>,
		/// Storage key which changes are requested.
		pub key: Vec<u8>,
	}

	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
	#[allow(dead_code)]
	/// Remote changes response.
	pub struct RemoteChangesResponse<N, H> {
		/// Id of a request this response was made for.
		pub id: RequestId,
		/// Proof has been generated using block with this number as a max block. Should be
		/// less than or equal to the RemoteChangesRequest::max block number.
		pub max: N,
		/// Changes proof.
		pub proof: Vec<Vec<u8>>,
		/// Changes tries roots missing on the requester' node.
		pub roots: Vec<(N, H)>,
		/// Missing changes tries roots proof.
		pub roots_proof: StorageProof,
	}
}
