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

//! Report related instructions.

/// Immediately report the contents of the Error Register to the given destination via XCM.
///
/// A `QueryResponse` message of type `ExecutionOutcome` is sent to the described destination.
///
/// - `response_info`: Information for making the response.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ReportError(pub QueryResponseInfo);

/// Report to a given destination the contents of the Holding Register.
///
/// A `QueryResponse` message of type `Assets` is sent to the described destination.
///
/// - `response_info`: Information for making the response.
/// - `assets`: A filter for the assets that should be reported back. The assets reported back
///   will be, asset-wise, *the lesser of this value and the holding register*. No wildcards
///   will be used when reporting assets back.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ReportHolding {
	pub response_info: QueryResponseInfo,
	pub assets: AssetFilter,
}

/// Send a `QueryResponse` message containing the value of the Transact Status Register to some
/// destination.
///
/// - `query_response_info`: The information needed for constructing and sending the
///   `QueryResponse` message.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors: *Fallible*.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ReportTransactStatus(pub QueryResponseInfo);
