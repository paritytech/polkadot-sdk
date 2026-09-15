// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Markets of the price oracle: what the oracle nodes fetch to price a pair.

use crate::PairId;
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;

/// Identifier of a venue, an exchange the oracle fetches prices from.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
)]
pub struct VenueId(pub u32);

/// Identifier of a market, one pair traded on one venue.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
)]
pub struct MarketId(pub u32);

/// Distinguishes the queries of a market, such as its order book from its recent trades.
///
/// The meaning of a tag is defined by the runtime.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
)]
pub struct QueryTag(pub u8);

/// HTTP method of a request.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum Method {
	Get,
	Post,
}

/// An HTTP header.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct Header {
	pub name: Vec<u8>,
	pub value: Vec<u8>,
}

/// An HTTPS request the node performs to fetch data from a venue.
///
/// The URL is `https://{host}{path}?{query}`, with the query values percent-encoded by the node.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct Request {
	pub method: Method,
	/// Host name, e.g. `api.binance.com`.
	pub host: Vec<u8>,
	/// Path, starting with `/`.
	pub path: Vec<u8>,
	/// Query parameters as `(name, value)` pairs, not encoded.
	pub query: Vec<(Vec<u8>, Vec<u8>)>,
	pub headers: Vec<Header>,
	/// Request body, empty for [`Method::Get`].
	pub body: Vec<u8>,
	/// Time after which the request is abandoned.
	pub timeout_ms: u32,
	/// Responses larger than this are discarded.
	pub max_response_bytes: u32,
}

/// One query of a market: what to request, and a tag telling the runtime what the response is.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct Query {
	pub tag: QueryTag,
	pub request: Request,
}

/// A pair traded on a venue, with the queries needed to price it.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct Market {
	pub id: MarketId,
	pub venue: VenueId,
	pub pair: PairId,
	pub queries: Vec<Query>,
}
