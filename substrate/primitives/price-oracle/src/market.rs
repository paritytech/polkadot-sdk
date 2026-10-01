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
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
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
	MaxEncodedLen,
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
	MaxEncodedLen,
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
	MaxEncodedLen,
	TypeInfo,
)]
pub struct QueryTag(pub u8);

/// HTTP method of a request.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
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

/// Whether `host` is a host name of letters, digits, `.` and `-`, optionally followed by `:` and
/// a port, and not an IP address.
///
/// The runtime cannot tell where a name points, as that takes a DNS lookup. The nodes do the
/// lookup and discard any non-public address it returns. IP addresses skip the lookup, so they
/// are refused. A host counts as an IP address when its last label is a number, as URL parsers
/// also read short forms such as `127.1` or `0x7f.1` as IPv4 addresses.
pub fn is_public_host(host: &[u8]) -> bool {
	let (name, port) = match host.iter().position(|&b| b == b':') {
		Some(i) => (&host[..i], Some(&host[i + 1..])),
		None => (host, None),
	};
	let name_ok = name.iter().all(|&b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
	let port_ok = port.map_or(true, |p| {
		p.iter().all(u8::is_ascii_digit) &&
			core::str::from_utf8(p).map_or(false, |p| p.parse::<u16>().is_ok())
	});
	let last = name.strip_suffix(b".").unwrap_or(name);
	let last = last.rsplit(|&b| b == b'.').next().unwrap_or_default();
	let numeric = match last {
		[b'0', b'x' | b'X', hex @ ..] => hex.iter().all(u8::is_ascii_hexdigit),
		_ => last.iter().all(u8::is_ascii_digit),
	};
	name_ok && port_ok && !last.is_empty() && !numeric
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn host_names_are_accepted() {
		for host in ["api.binance.com", "x.io:8443", "x.io.", "a-1.b2.io"] {
			assert!(is_public_host(host.as_bytes()), "{host}");
		}
		// Whether a name reaches a public address is only checked by the nodes.
		assert!(is_public_host(b"localhost"));
	}

	#[test]
	fn ip_and_malformed_hosts_are_rejected() {
		for host in [
			"127.0.0.1",
			"127.0.0.1:8080",
			"1.1.1.1",
			"2130706433",
			"127.1",
			"0x7f.1",
			"x.0X1f",
			"x.0x",
			"[::1]",
			"[::1]:443",
			"",
			".",
			":443",
			"x.io:",
			"x.io:abc",
			"x.io:65536",
			"x io",
			"x.io/evil",
			"user@x.io",
		] {
			assert!(!is_public_host(host.as_bytes()), "{host}");
		}
	}
}
