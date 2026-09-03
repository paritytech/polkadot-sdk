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

//! Minimal structural validation of binary multiaddrs.
//!
//! The runtime cannot use the `multiaddr` crate and has no need to interpret addresses. It only
//! checks that an address is a well-formed sequence of known protocol components and that none
//! of them is `/p2p/`, since a bootnode's peer id is derived from its node key. Payload contents
//! (DNS names, multihashes, ...) are not inspected.
//!
//! Protocol codes and payload layouts follow the multiformats protocol table as implemented by
//! the `multiaddr` crate, version 0.18.

/// Why an address was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
	/// The address is empty.
	Empty,
	/// A protocol code or length prefix is not a valid, minimal, unsigned varint.
	InvalidVarint,
	/// A component's payload runs past the end of the address.
	UnexpectedEnd,
	/// A protocol code that is not in the table.
	UnknownProtocol(u32),
	/// The address contains a `/p2p/` component.
	ContainsPeerId,
}

/// Multicodec code of the `/p2p/` protocol (also known as `/ipfs/`).
pub const P2P: u32 = 421;

/// How a protocol's payload is laid out after its code.
enum Payload {
	/// No payload.
	Empty,
	/// A fixed number of bytes.
	Fixed(usize),
	/// An unsigned varint length followed by that many bytes.
	LengthPrefixed,
}

/// Payload layout of a protocol, or `None` for an unknown code.
fn payload(code: u32) -> Option<Payload> {
	use Payload::*;
	Some(match code {
		// ip4
		4 => Fixed(4),
		// tcp, dccp, sctp, udp
		6 | 33 | 132 | 273 => Fixed(2),
		// ip6
		41 => Fixed(16),
		// ip6zone
		42 => LengthPrefixed,
		// ipcidr
		43 => Fixed(1),
		// dns, dns4, dns6, dnsaddr
		53..=56 => LengthPrefixed,
		// p2p-webrtc-star, p2p-webrtc-direct, p2p-stardust, webrtc-direct, webrtc, p2p-circuit
		275..=277 | 280 | 281 | 290 => Empty,
		// utp, udt
		301 | 302 => Empty,
		// unix
		400 => LengthPrefixed,
		// p2p
		P2P => LengthPrefixed,
		// https, tls, noise, quic, quic-v1, webtransport, ws, wss, p2p-websocket-star, http
		443 | 448 | 454 | 460 | 461 | 465 | 477..=480 => Empty,
		// onion: 10 byte address plus 2 byte port
		444 => Fixed(12),
		// onion3: 35 byte address plus 2 byte port
		445 => Fixed(37),
		// garlic64, garlic32, sni, certhash
		446 | 447 | 449 | 466 => LengthPrefixed,
		// memory
		777 => Fixed(8),
		// ws and wss with a path; non-standard, but emitted by the `multiaddr` crate
		4770 | 4780 => LengthPrefixed,
		_ => return None,
	})
}

/// Read an unsigned LEB128 varint fitting in a `u32`, returning it and the remaining input.
///
/// Non-minimal encodings (a zero continuation byte, e.g. `0x80 0x00`) are rejected.
fn read_varint(input: &[u8]) -> Result<(u32, &[u8]), Error> {
	let mut value: u32 = 0;
	for (i, &byte) in input.iter().enumerate().take(5) {
		let bits = u32::from(byte & 0x7f);
		// Four full bytes carry 28 bits; the fifth may only carry the remaining four.
		if i == 4 && bits > 0x0f {
			return Err(Error::InvalidVarint);
		}
		value |= bits << (7 * i);
		if byte & 0x80 == 0 {
			if i > 0 && bits == 0 {
				return Err(Error::InvalidVarint);
			}
			return Ok((value, &input[i + 1..]));
		}
	}
	Err(Error::InvalidVarint)
}

/// Check that `addr` is a well-formed multiaddr made only of known protocols and that it does
/// not contain a `/p2p/` component.
pub fn validate(addr: &[u8]) -> Result<(), Error> {
	if addr.is_empty() {
		return Err(Error::Empty);
	}
	let mut rest = addr;
	while !rest.is_empty() {
		let (code, mut after) = read_varint(rest)?;
		if code == P2P {
			return Err(Error::ContainsPeerId);
		}
		let len = match payload(code).ok_or(Error::UnknownProtocol(code))? {
			Payload::Empty => 0,
			Payload::Fixed(n) => n,
			Payload::LengthPrefixed => {
				let (n, after_len) = read_varint(after)?;
				after = after_len;
				n as usize
			},
		};
		rest = after.get(len..).ok_or(Error::UnexpectedEnd)?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use multiaddr::Multiaddr;

	fn bytes(addr: &str) -> Vec<u8> {
		addr.parse::<Multiaddr>().expect("valid test address").to_vec()
	}

	fn varint(mut value: u32) -> Vec<u8> {
		let mut out = Vec::new();
		loop {
			let byte = (value & 0x7f) as u8;
			value >>= 7;
			if value == 0 {
				out.push(byte);
				return out;
			}
			out.push(byte | 0x80);
		}
	}

	#[test]
	fn accepts_common_addresses() {
		for addr in [
			"/ip4/127.0.0.1/tcp/30333",
			"/ip4/10.0.0.1/tcp/30333/ws",
			"/ip6/::1/tcp/30333",
			"/ip6/2001:db8::1/tcp/443/wss",
			"/dns/boot.example.org/tcp/30333",
			"/dns4/boot.example.org/tcp/30333/ws",
			"/dns6/boot.example.org/udp/30333/quic-v1",
			"/dnsaddr/boot.example.org",
			"/ip4/1.2.3.4/udp/30333/quic-v1/webtransport",
			"/ip4/1.2.3.4/tcp/30333/tls/ws",
		] {
			assert_eq!(validate(&bytes(addr)), Ok(()), "{addr}");
		}
	}

	#[test]
	fn rejects_peer_id_components() {
		let peer = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
		for addr in [
			&format!("/ip4/1.2.3.4/tcp/30333/p2p/{peer}"),
			&format!("/dns/boot.example.org/tcp/30333/ws/p2p/{peer}"),
			&format!("/ip4/1.2.3.4/tcp/30333/p2p/{peer}/p2p-circuit"),
			&format!("/p2p/{peer}"),
		] {
			assert_eq!(validate(&bytes(addr)), Err(Error::ContainsPeerId), "{addr}");
		}
	}

	#[test]
	fn rejects_malformed_addresses() {
		assert_eq!(validate(&[]), Err(Error::Empty));
		// ip4 needs four bytes.
		assert_eq!(validate(&[4, 127, 0, 0]), Err(Error::UnexpectedEnd));
		// tcp with a single port byte after a valid ip4.
		assert_eq!(validate(&[4, 127, 0, 0, 1, 6, 0x76]), Err(Error::UnexpectedEnd));
		// dns claiming more bytes than present.
		assert_eq!(validate(&[53, 5, b'a', b'b']), Err(Error::UnexpectedEnd));
		// Dangling continuation bit.
		assert_eq!(validate(&[0x80]), Err(Error::InvalidVarint));
		// Non-minimal encoding of code 4.
		assert_eq!(validate(&[0x84, 0x00, 1, 2, 3, 4]), Err(Error::InvalidVarint));
		// Varint too wide for a u32.
		assert_eq!(validate(&[0xff, 0xff, 0xff, 0xff, 0x7f]), Err(Error::InvalidVarint));
		assert_eq!(validate(&[0xff, 0xff, 0xff, 0xff, 0xff, 0x01]), Err(Error::InvalidVarint));
		// Unknown protocol code.
		let mut unknown = varint(999_999);
		unknown.push(0);
		assert_eq!(validate(&unknown), Err(Error::UnknownProtocol(999_999)));
		// Trailing garbage after a valid address.
		let mut trailing = bytes("/ip4/1.2.3.4/tcp/30333");
		trailing.push(0);
		assert_eq!(validate(&trailing), Err(Error::UnknownProtocol(0)));
	}

	/// Every code in the table, given a payload shaped as the table says, parses with the
	/// `multiaddr` crate. This pins the table to the reference implementation.
	#[test]
	fn table_agrees_with_multiaddr_crate() {
		let codes = [
			4u32, 6, 33, 41, 42, 43, 53, 54, 55, 56, 132, 273, 275, 276, 277, 280, 281, 290, 301,
			302, 400, 443, 444, 445, 446, 447, 448, 449, 454, 460, 461, 465, 466, 477, 478, 479,
			480, 777, 4770, 4780,
		];
		for code in codes {
			let mut addr = varint(code);
			match payload(code).expect("code is in the table") {
				Payload::Empty => {},
				Payload::Fixed(n) => addr.extend(core::iter::repeat_n(0u8, n)),
				// certhash needs a decodable multihash; the empty identity hash is one.
				Payload::LengthPrefixed if code == 466 => addr.extend([2, 0, 0]),
				Payload::LengthPrefixed => addr.push(0),
			}
			assert!(
				Multiaddr::try_from(addr.clone()).is_ok(),
				"code {code}: {addr:?} rejected by the multiaddr crate"
			);
			assert_eq!(validate(&addr), Ok(()), "code {code}");
		}
		// And p2p, which the crate accepts and this module rejects.
		let peer = bytes("/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp");
		assert!(Multiaddr::try_from(peer.clone()).is_ok());
		assert_eq!(validate(&peer), Err(Error::ContainsPeerId));
	}
}
