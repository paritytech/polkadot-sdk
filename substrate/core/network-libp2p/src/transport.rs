// Copyright 2018 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use futures::prelude::*;
use libp2p::{InboundUpgradeExt, OutboundUpgradeExt, PeerId, Transport, mplex, secio, yamux, tcp, dns};
use libp2p::core::{self, transport::boxed::Boxed, muxing::StreamMuxerBox};
use std::{io, time::Duration, usize};

/// Builds the transport that serves as a common ground for all connections.
pub fn build_transport(
	local_private_key: secio::SecioKeyPair
) -> Boxed<(PeerId, StreamMuxerBox), io::Error> {
	let mut mplex_config = mplex::MplexConfig::new();
	mplex_config.max_buffer_len_behaviour(mplex::MaxBufferBehaviour::Block);
	mplex_config.max_buffer_len(usize::MAX);

	let transport = tcp::TcpConfig::new();
	let transport = dns::DnsConfig::new(transport);

	// TODO: rework the transport creation (https://github.com/libp2p/rust-libp2p/issues/783)
	transport
		.with_upgrade(secio::SecioConfig::new(local_private_key))
		.and_then(move |out, endpoint| {
			let peer_id = out.remote_key.into_peer_id();
			let peer_id2 = peer_id.clone();
			let upgrade = core::upgrade::SelectUpgrade::new(yamux::Config::default(), mplex_config)
				.map_inbound(move |muxer| (peer_id, muxer))
				.map_outbound(move |muxer| (peer_id2, muxer));

			core::upgrade::apply(out.stream, upgrade, endpoint)
				.map(|(id, muxer)| (id, core::muxing::StreamMuxerBox::new(muxer)))
		})
		.with_timeout(Duration::from_secs(20))
		.map_err(|err| io::Error::new(io::ErrorKind::Other, err))
		.boxed()
}
