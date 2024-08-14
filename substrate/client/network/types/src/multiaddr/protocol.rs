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

use crate::multihash::Multihash;
use litep2p::types::multiaddr::Protocol as LiteP2pProtocol;
use std::{
	borrow::Cow,
	net::{Ipv4Addr, Ipv6Addr},
};

/// [`Protocol`] describes all possible multiaddress protocols.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Protocol<'a> {
	Dccp(u16),
	Dns(Cow<'a, str>),
	Dns4(Cow<'a, str>),
	Dns6(Cow<'a, str>),
	Dnsaddr(Cow<'a, str>),
	Http,
	Https,
	Ip4(Ipv4Addr),
	Ip6(Ipv6Addr),
	P2pWebRtcDirect,
	P2pWebRtcStar,
	WebRTC,
	Certhash(Multihash),
	P2pWebSocketStar,
	/// Contains the "port" to contact. Similar to TCP or UDP, 0 means "assign me a port".
	Memory(u64),
	Onion(Cow<'a, [u8; 10]>, u16),
	Onion3(Cow<'a, [u8; 35]>, u16),
	P2p(Multihash),
	P2pCircuit,
	Quic,
	QuicV1,
	Sctp(u16),
	Tcp(u16),
	Tls,
	Noise,
	Udp(u16),
	Udt,
	Unix(Cow<'a, str>),
	Utp,
	Ws(Cow<'a, str>),
	Wss(Cow<'a, str>),
}

impl<'a> From<LiteP2pProtocol<'a>> for Protocol<'a> {
	fn from(protocol: LiteP2pProtocol<'a>) -> Self {
		match protocol {
			LiteP2pProtocol::Dccp(port) => Protocol::Dccp(port),
			LiteP2pProtocol::Dns(str) => Protocol::Dns(str),
			LiteP2pProtocol::Dns4(str) => Protocol::Dns4(str),
			LiteP2pProtocol::Dns6(str) => Protocol::Dns6(str),
			LiteP2pProtocol::Dnsaddr(str) => Protocol::Dnsaddr(str),
			LiteP2pProtocol::Http => Protocol::Http,
			LiteP2pProtocol::Https => Protocol::Https,
			LiteP2pProtocol::Ip4(ipv4_addr) => Protocol::Ip4(ipv4_addr),
			LiteP2pProtocol::Ip6(ipv6_addr) => Protocol::Ip6(ipv6_addr),
			LiteP2pProtocol::P2pWebRtcDirect => Protocol::P2pWebRtcDirect,
			LiteP2pProtocol::P2pWebRtcStar => Protocol::P2pWebRtcStar,
			LiteP2pProtocol::WebRTC => Protocol::WebRTC,
			LiteP2pProtocol::Certhash(multihash) => Protocol::Certhash(multihash.into()),
			LiteP2pProtocol::P2pWebSocketStar => Protocol::P2pWebSocketStar,
			LiteP2pProtocol::Memory(port) => Protocol::Memory(port),
			LiteP2pProtocol::Onion(str, port) => Protocol::Onion(str, port),
			LiteP2pProtocol::Onion3(addr) =>
				Protocol::Onion3(Cow::Owned(*addr.hash()), addr.port()),
			LiteP2pProtocol::P2p(multihash) => Protocol::P2p(multihash.into()),
			LiteP2pProtocol::P2pCircuit => Protocol::P2pCircuit,
			LiteP2pProtocol::Quic => Protocol::Quic,
			LiteP2pProtocol::QuicV1 => Protocol::QuicV1,
			LiteP2pProtocol::Sctp(port) => Protocol::Sctp(port),
			LiteP2pProtocol::Tcp(port) => Protocol::Tcp(port),
			LiteP2pProtocol::Tls => Protocol::Tls,
			LiteP2pProtocol::Noise => Protocol::Noise,
			LiteP2pProtocol::Udp(port) => Protocol::Udp(port),
			LiteP2pProtocol::Udt => Protocol::Udt,
			LiteP2pProtocol::Unix(str) => Protocol::Unix(str),
			LiteP2pProtocol::Utp => Protocol::Utp,
			LiteP2pProtocol::Ws(str) => Protocol::Ws(str),
			LiteP2pProtocol::Wss(str) => Protocol::Wss(str),
		}
	}
}

impl<'a> From<Protocol<'a>> for LiteP2pProtocol<'a> {
	fn from(protocol: Protocol<'a>) -> Self {
		match protocol {
			Protocol::Dccp(port) => LiteP2pProtocol::Dccp(port),
			Protocol::Dns(str) => LiteP2pProtocol::Dns(str),
			Protocol::Dns4(str) => LiteP2pProtocol::Dns4(str),
			Protocol::Dns6(str) => LiteP2pProtocol::Dns6(str),
			Protocol::Dnsaddr(str) => LiteP2pProtocol::Dnsaddr(str),
			Protocol::Http => LiteP2pProtocol::Http,
			Protocol::Https => LiteP2pProtocol::Https,
			Protocol::Ip4(ipv4_addr) => LiteP2pProtocol::Ip4(ipv4_addr),
			Protocol::Ip6(ipv6_addr) => LiteP2pProtocol::Ip6(ipv6_addr),
			Protocol::P2pWebRtcDirect => LiteP2pProtocol::P2pWebRtcDirect,
			Protocol::P2pWebRtcStar => LiteP2pProtocol::P2pWebRtcStar,
			Protocol::WebRTC => LiteP2pProtocol::WebRTC,
			Protocol::Certhash(multihash) => LiteP2pProtocol::Certhash(multihash.into()),
			Protocol::P2pWebSocketStar => LiteP2pProtocol::P2pWebSocketStar,
			Protocol::Memory(port) => LiteP2pProtocol::Memory(port),
			Protocol::Onion(str, port) => LiteP2pProtocol::Onion(str, port),
			Protocol::Onion3(str, port) => LiteP2pProtocol::Onion3((str.into_owned(), port).into()),
			Protocol::P2p(multihash) => LiteP2pProtocol::P2p(multihash.into()),
			Protocol::P2pCircuit => LiteP2pProtocol::P2pCircuit,
			Protocol::Quic => LiteP2pProtocol::Quic,
			Protocol::QuicV1 => LiteP2pProtocol::QuicV1,
			Protocol::Sctp(port) => LiteP2pProtocol::Sctp(port),
			Protocol::Tcp(port) => LiteP2pProtocol::Tcp(port),
			Protocol::Tls => LiteP2pProtocol::Tls,
			Protocol::Noise => LiteP2pProtocol::Noise,
			Protocol::Udp(port) => LiteP2pProtocol::Udp(port),
			Protocol::Udt => LiteP2pProtocol::Udt,
			Protocol::Unix(str) => LiteP2pProtocol::Unix(str),
			Protocol::Utp => LiteP2pProtocol::Utp,
			Protocol::Ws(str) => LiteP2pProtocol::Ws(str),
			Protocol::Wss(str) => LiteP2pProtocol::Wss(str),
		}
	}
}
