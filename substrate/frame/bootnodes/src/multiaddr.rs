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

//! Validation of binary multiaddrs for bootnode entries.
//!
//! The runtime cannot use the `multiaddr` crate. It accepts exactly the shapes a node can dial a
//! bootnode on, the same ones authority discovery publishes: an IP or DNS host, a TCP port and
//! optionally a WebSocket layer:
//!
//! `/(ip4|ip6|dns|dns4|dns6)/<host>/tcp/<port>[/ws|/wss]`
//!
//! DNS names must be UTF-8, which is what the `multiaddr` crate requires when decoding. A `/p2p/`
//! component is rejected with its own error, since a bootnode's peer id is derived from its node
//! key.
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
	/// A protocol other than ip4, ip6, dns, dns4, dns6, tcp, ws or wss.
	UnsupportedProtocol(u32),
	/// The address contains a `/p2p/` component.
	ContainsPeerId,
	/// A DNS name is not UTF-8.
	InvalidDnsName,
	/// The components are not `host/tcp` optionally followed by `ws` or `wss`.
	InvalidLayout,
}

/// Multicodec code of the `/p2p/` protocol (also known as `/ipfs/`).
pub const P2P: u32 = 421;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Component {
	/// ip4, ip6, dns, dns4 or dns6.
	Host,
	/// tcp.
	Tcp,
	/// ws or wss.
	WebSocket,
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

/// Read one component, returning its kind and the remaining input.
fn read_component(input: &[u8]) -> Result<(Component, &[u8]), Error> {
	let (code, rest) = read_varint(input)?;
	let (component, payload_len) = match code {
		// ip4
		4 => (Component::Host, 4),
		// ip6
		41 => (Component::Host, 16),
		// dns, dns4, dns6
		53..=55 => {
			let (len, rest) = read_varint(rest)?;
			let name = rest.get(..len as usize).ok_or(Error::UnexpectedEnd)?;
			core::str::from_utf8(name).map_err(|_| Error::InvalidDnsName)?;
			return Ok((Component::Host, &rest[name.len()..]));
		},
		// tcp
		6 => (Component::Tcp, 2),
		// ws, wss
		477 | 478 => (Component::WebSocket, 0),
		P2P => return Err(Error::ContainsPeerId),
		other => return Err(Error::UnsupportedProtocol(other)),
	};
	let rest = rest.get(payload_len..).ok_or(Error::UnexpectedEnd)?;
	Ok((component, rest))
}

/// What may come next while walking an address.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
	Host,
	Tcp,
	WebSocketOrEnd,
	End,
}

/// Check that `addr` is `/(ip4|ip6|dns|dns4|dns6)/<host>/tcp/<port>` optionally followed by
/// `/ws` or `/wss`, with UTF-8 DNS names and no `/p2p/` component.
pub fn validate(addr: &[u8]) -> Result<(), Error> {
	if addr.is_empty() {
		return Err(Error::Empty);
	}
	let mut rest = addr;
	let mut expect = Expect::Host;
	while !rest.is_empty() {
		let (component, after) = read_component(rest)?;
		expect = match (expect, component) {
			(Expect::Host, Component::Host) => Expect::Tcp,
			(Expect::Tcp, Component::Tcp) => Expect::WebSocketOrEnd,
			(Expect::WebSocketOrEnd, Component::WebSocket) => Expect::End,
			_ => return Err(Error::InvalidLayout),
		};
		rest = after;
	}
	match expect {
		Expect::WebSocketOrEnd | Expect::End => Ok(()),
		Expect::Host | Expect::Tcp => Err(Error::InvalidLayout),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use multiaddr::Multiaddr;

	const PEER: &str = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";

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
	fn accepts_host_tcp_and_optional_websocket() {
		for addr in [
			"/ip4/127.0.0.1/tcp/30333",
			"/ip4/10.0.0.1/tcp/30333/ws",
			"/ip4/10.0.0.1/tcp/30333/wss",
			"/ip6/::1/tcp/30333",
			"/ip6/2001:db8::1/tcp/443/wss",
			"/dns/boot.example.org/tcp/30333",
			"/dns4/boot.example.org/tcp/30333/ws",
			"/dns6/boot.example.org/tcp/30333/wss",
		] {
			assert_eq!(validate(&bytes(addr)), Ok(()), "{addr}");
		}
	}

	#[test]
	fn rejects_peer_id_components_before_any_layout_error() {
		for addr in [
			&format!("/ip4/1.2.3.4/tcp/30333/p2p/{PEER}"),
			&format!("/dns/boot.example.org/tcp/30333/ws/p2p/{PEER}"),
			&format!("/ip4/1.2.3.4/p2p/{PEER}"),
			&format!("/p2p/{PEER}"),
		] {
			assert_eq!(validate(&bytes(addr)), Err(Error::ContainsPeerId), "{addr}");
		}
	}

	#[test]
	fn rejects_transports_a_bootnode_cannot_be_dialed_on() {
		for (addr, code) in [
			("/ip4/1.2.3.4/udp/30333/quic-v1", 273),
			("/dnsaddr/boot.example.org", 56),
			("/ip4/1.2.3.4/tcp/30333/tls/ws", 448),
			("/ip4/1.2.3.4/tcp/30333/http", 480),
			("/unix/node.sock", 400),
			("/memory/1", 777),
		] {
			assert_eq!(validate(&bytes(addr)), Err(Error::UnsupportedProtocol(code)), "{addr}");
		}
	}

	#[test]
	fn rejects_layouts_other_than_host_tcp_websocket() {
		for addr in [
			"/ip4/1.2.3.4",
			"/dns/boot.example.org",
			"/tcp/30333",
			"/tcp/30333/ip4/1.2.3.4",
			"/ip4/1.2.3.4/ip4/1.2.3.5/tcp/30333",
			"/ip4/1.2.3.4/tcp/30333/tcp/30334",
			"/ip4/1.2.3.4/tcp/30333/ws/wss",
			"/ip4/1.2.3.4/ws",
		] {
			assert_eq!(validate(&bytes(addr)), Err(Error::InvalidLayout), "{addr}");
		}
	}

	#[test]
	fn rejects_dns_names_the_multiaddr_crate_cannot_decode() {
		let bad = [53, 2, 0xff, 0xfe, 6, 0x76, 0x7d];
		assert!(Multiaddr::try_from(bad.to_vec()).is_err());
		assert_eq!(validate(&bad), Err(Error::InvalidDnsName));
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
		assert_eq!(validate(&unknown), Err(Error::UnsupportedProtocol(999_999)));
		// Trailing garbage after a valid address.
		let mut trailing = bytes("/ip4/1.2.3.4/tcp/30333");
		trailing.push(0);
		assert_eq!(validate(&trailing), Err(Error::UnsupportedProtocol(0)));
	}

	/// Every accepted code, given a payload shaped as this module expects, parses with the
	/// `multiaddr` crate. This pins the payload layouts to the reference implementation.
	#[test]
	fn payload_layouts_agree_with_multiaddr_crate() {
		for (code, payload) in [
			(4u32, vec![0u8; 4]),
			(41, vec![0; 16]),
			(53, vec![0]),
			(54, vec![0]),
			(55, vec![0]),
			(6, vec![0; 2]),
			(477, vec![]),
			(478, vec![]),
		] {
			let mut addr = varint(code);
			addr.extend(payload);
			assert!(
				Multiaddr::try_from(addr.clone()).is_ok(),
				"code {code}: {addr:?} rejected by the multiaddr crate"
			);
			assert_eq!(read_component(&addr).map(|(_, rest)| rest.len()), Ok(0), "code {code}");
		}
	}
}
