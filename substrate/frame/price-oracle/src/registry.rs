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
//! Storage types of venues and markets, and their conversion to the wire types served to the
//! oracle nodes.
//!
//! Bounds are constants of this module: they are enforced on admin input only, and raising one
//! is a runtime upgrade with no effect on the nodes, which receive unbounded wire types.

use crate::schema::ResponseSchema;
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{traits::ConstU32, BoundedVec};
use scale_info::TypeInfo;
use sp_price_oracle::{
	market::{Header, Market, MarketId, Method, Query, QueryTag, Request, VenueId},
	PairId,
};

pub type MaxVenueName = ConstU32<32>;
pub type MaxHost = ConstU32<128>;
pub type MaxPath = ConstU32<256>;
pub type MaxQueryParams = ConstU32<16>;
pub type MaxParamName = ConstU32<32>;
pub type MaxParamValue = ConstU32<64>;
pub type MaxHeaders = ConstU32<8>;
pub type MaxHeaderName = ConstU32<64>;
pub type MaxHeaderValue = ConstU32<256>;
pub type MaxBody = ConstU32<1024>;
pub type MaxQueries = ConstU32<4>;

/// An exchange.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct Venue {
	/// Human readable name, e.g. `Kraken`.
	pub name: BoundedVec<u8, MaxVenueName>,
}

/// A stored HTTP header.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct StoredHeader {
	pub name: BoundedVec<u8, MaxHeaderName>,
	pub value: BoundedVec<u8, MaxHeaderValue>,
}

/// A stored HTTPS request. See [`Request`] for the meaning of the fields.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct StoredRequest {
	pub method: Method,
	pub host: BoundedVec<u8, MaxHost>,
	pub path: BoundedVec<u8, MaxPath>,
	pub query:
		BoundedVec<(BoundedVec<u8, MaxParamName>, BoundedVec<u8, MaxParamValue>), MaxQueryParams>,
	pub headers: BoundedVec<StoredHeader, MaxHeaders>,
	pub body: BoundedVec<u8, MaxBody>,
	pub timeout_ms: u32,
	pub max_response_bytes: u32,
}

/// A stored query of a market.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct StoredQuery {
	pub tag: QueryTag,
	pub request: StoredRequest,
	/// How to read the response.
	pub schema: ResponseSchema,
}

/// A stored market.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct StoredMarket {
	pub venue: VenueId,
	pub pair: PairId,
	pub queries: BoundedVec<StoredQuery, MaxQueries>,
	/// Inactive markets are kept in storage but not served to the nodes.
	pub active: bool,
}

impl From<StoredHeader> for Header {
	fn from(h: StoredHeader) -> Self {
		Header { name: h.name.into_inner(), value: h.value.into_inner() }
	}
}

impl From<StoredRequest> for Request {
	fn from(r: StoredRequest) -> Self {
		Request {
			method: r.method,
			host: r.host.into_inner(),
			path: r.path.into_inner(),
			query: r
				.query
				.into_inner()
				.into_iter()
				.map(|(n, v)| (n.into_inner(), v.into_inner()))
				.collect(),
			headers: r.headers.into_inner().into_iter().map(Into::into).collect(),
			body: r.body.into_inner(),
			timeout_ms: r.timeout_ms,
			max_response_bytes: r.max_response_bytes,
		}
	}
}

impl From<StoredQuery> for Query {
	fn from(q: StoredQuery) -> Self {
		Query { tag: q.tag, request: q.request.into() }
	}
}

impl StoredMarket {
	/// The wire form of this market, as served to the oracle nodes.
	pub fn to_wire(self, id: MarketId) -> Market {
		Market {
			id,
			venue: self.venue,
			pair: self.pair,
			queries: self.queries.into_inner().into_iter().map(Into::into).collect::<Vec<_>>(),
		}
	}
}
