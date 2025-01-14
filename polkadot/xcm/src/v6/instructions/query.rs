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

//! Query related instructions.

use codec::{Decode, Encode};
use scale_info::TypeInfo;

use crate::v6::{Location, QueryResponseInfo, Response, Weight};

/// Respond with information that the local system is expecting.
///
/// - `query_id`: The identifier of the query that resulted in this message being sent.
/// - `response`: The message content.
/// - `max_weight`: The maximum weight that handling this response should take.
/// - `querier`: The location responsible for the initiation of the response, if there is one.
///   In general this will tend to be the same location as the receiver of this message. NOTE:
///   As usual, this is interpreted from the perspective of the receiving consensus system.
///
/// Safety: Since this is information only, there are no immediate concerns. However, it should
/// be remembered that even if the Origin behaves reasonably, it can always be asked to make
/// a response to a third-party chain who may or may not be expecting the response. Therefore
/// the `querier` should be checked to match the expected value.
///
/// Kind: *Information*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct QueryResponse {
	#[codec(compact)]
	pub query_id: u64,
	pub response: Response,
	pub max_weight: Weight,
	pub querier: Option<Location>,
}

/// Query the existence of a particular pallet type.
///
/// - `module_name`: The module name of the pallet to query.
/// - `response_info`: Information for making the response.
///
/// Sends a `QueryResponse` to Origin whose data field `PalletsInfo` containing the information
/// of all pallets on the local chain whose name is equal to `name`. This is empty in the case
/// that the local chain is not based on Substrate Frame.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors: *Fallible*.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct QueryPallet {
	pub module_name: Vec<u8>,
	pub response_info: QueryResponseInfo,
}
