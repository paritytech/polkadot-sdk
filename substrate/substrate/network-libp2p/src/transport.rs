// Copyright 2018 Parity Technologies (UK) Ltd.
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
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.?

use libp2p::{self, Transport, secio};
use libp2p::core::{MuxedTransport, either, upgrade};
use libp2p::transport_timeout::TransportTimeout;
use std::time::Duration;
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, AsyncWrite};

/// Builds the transport that serves as a common ground for all connections.
pub fn build_transport(
	core: Handle,
	unencrypted_allowed: UnencryptedAllowed,
	local_private_key: secio::SecioKeyPair
) -> impl MuxedTransport<Output = impl AsyncRead + AsyncWrite> + Clone {
	let base = libp2p::CommonTransport::new(core)
		.with_upgrade({
			let secio = secio::SecioConfig {
				key: local_private_key,
			};

			let mut plaintext = upgrade::toggleable(upgrade::PlainTextConfig);
			match unencrypted_allowed {
				UnencryptedAllowed::Allowed => plaintext.disable(),
				UnencryptedAllowed::Denied => (),
			};

			// TODO: this `EitherOutput` thing shows that libp2p's API could be improved
			upgrade::or(
				upgrade::map(plaintext, |out|
					(either::EitherOutput::First(out), None)
				),
				upgrade::map(secio, |out: secio::SecioOutput<_>|
					(either::EitherOutput::Second(out.stream),
						Some(out.remote_key))
				),
			)
		})
		// TODO: check that the public key matches what is reported by identify
		.map(|(socket, _key), _| socket)
		.with_upgrade(libp2p::mplex::MultiplexConfig::new())
		.into_connection_reuse();

	TransportTimeout::new(base, Duration::from_secs(20))
		.map_err(|err| {
			debug!(target: "sub-libp2p", "Error in base transport \
				layer: {:?}", err);
			err
		})
}

/// Specifies whether unencrypted communications are allowed or denied.
#[derive(Debug, Copy, Clone)]
pub enum UnencryptedAllowed {
	#[allow(dead_code)]
	Allowed,
	Denied,
}
