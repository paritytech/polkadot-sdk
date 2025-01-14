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

//! Versions related instructions.

/// Ask the destination system to respond with the most recent version of XCM that they
/// support in a `QueryResponse` instruction. Any changes to this should also elicit similar
/// responses when they happen.
///
/// - `query_id`: An identifier that will be replicated into the returned XCM message.
/// - `max_response_weight`: The maximum amount of weight that the `QueryResponse` item which
///   is sent as a reply may take to execute. NOTE: If this is unexpectedly large then the
///   response may not execute at all.
///
/// Kind: *Command*
///
/// Errors: *Fallible*
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct SubscribeVersion {
	#[codec(compact)]
	pub query_id: QueryId,
	pub max_response_weight: Weight,
}

/// Cancel the effect of a previous `SubscribeVersion` instruction.
///
/// Kind: *Command*
///
/// Errors: *Fallible*
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct UnsubscribeVersion;
