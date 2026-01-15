// Copyright 2023 litep2p developers
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

#![allow(clippy::single_match)]
#![allow(clippy::result_large_err)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::type_complexity)]
#![allow(clippy::result_unit_err)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::assign_op_pattern)]
#![allow(clippy::match_like_matches_macro)]

use crate::{
    addresses::PublicAddresses,
    config::Litep2pConfig,
    error::DialError,
    protocol::{
        libp2p::{bitswap::Bitswap, identify::Identify, kademlia::Kademlia, ping::Ping},
        mdns::Mdns,
        notification::NotificationProtocol,
        request_response::RequestResponseProtocol,
    },
    transport::{
        manager::{SupportedTransport, TransportManager, TransportManagerBuilder},
        tcp::TcpTransport,
        TransportBuilder, TransportEvent,
    },
};

#[cfg(feature = "quic")]
use crate::transport::quic::QuicTransport;
#[cfg(feature = "webrtc")]
use crate::transport::webrtc::WebRtcTransport;
#[cfg(feature = "websocket")]
use crate::transport::websocket::WebSocketTransport;

use hickory_resolver::{name_server::TokioConnectionProvider, TokioResolver};
use multiaddr::{Multiaddr, Protocol};
use transport::Endpoint;
use types::ConnectionId;

pub use bandwidth::BandwidthSink;
pub use error::Error;
pub use peer_id::PeerId;
use std::{collections::HashSet, sync::Arc};
pub use types::protocol::ProtocolName;

pub(crate) mod peer_id;

pub mod addresses;
pub mod codec;
pub mod config;
pub mod crypto;
pub mod error;
pub mod executor;
pub mod protocol;
pub mod substream;
pub mod transport;
pub mod types;
pub mod yamux;

mod bandwidth;
mod multistream_select;
pub mod utils;

#[cfg(test)]
mod mock;

/// Public result type used by the crate.
pub type Result<T> = std::result::Result<T, error::Error>;

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p";

/// Default channel size.
const DEFAULT_CHANNEL_SIZE: usize = 4096usize;

/// Litep2p events.
#[derive(Debug)]
pub enum Litep2pEvent {
    /// Connection established to peer.
    ConnectionEstablished {
        /// Remote peer ID.
        peer: PeerId,

        /// Endpoint.
        endpoint: Endpoint,
    },

    /// Connection closed to remote peer.
    ConnectionClosed {
        /// Peer ID.
        peer: PeerId,

        /// Connection ID.
        connection_id: ConnectionId,
    },

    /// Failed to dial peer.
    ///
    /// This error can originate from dialing a single peer address.
    DialFailure {
        /// Address of the peer.
        address: Multiaddr,

        /// Dial error.
        error: DialError,
    },

    /// A list of multiple dial failures.
    ListDialFailures {
        /// List of errors.
        ///
        /// Depending on the transport, the address might be different for each error.
        errors: Vec<(Multiaddr, DialError)>,
    },
}

/// [`Litep2p`] object.
pub struct Litep2p {
    /// Local peer ID.
    local_peer_id: PeerId,

    /// Listen addresses.
    listen_addresses: Vec<Multiaddr>,

    /// Transport manager.
    transport_manager: TransportManager,

    /// Bandwidth sink.
    bandwidth_sink: BandwidthSink,
}

impl Litep2p {
    /// Create new [`Litep2p`].
    pub fn new(mut litep2p_config: Litep2pConfig) -> crate::Result<Litep2p> {
        let local_peer_id = PeerId::from_public_key(&litep2p_config.keypair.public().into());
        let bandwidth_sink = BandwidthSink::new();
        let mut listen_addresses = vec![];

        let (resolver_config, resolver_opts) = if litep2p_config.use_system_dns_config {
            hickory_resolver::system_conf::read_system_conf()
                .map_err(Error::CannotReadSystemDnsConfig)?
        } else {
            (Default::default(), Default::default())
        };
        let resolver = Arc::new(
            TokioResolver::builder_with_config(resolver_config, TokioConnectionProvider::default())
                .with_options(resolver_opts)
                .build(),
        );

        let supported_transports = Self::supported_transports(&litep2p_config);
        let mut transport_manager = TransportManagerBuilder::new()
            .with_keypair(litep2p_config.keypair.clone())
            .with_supported_transports(supported_transports)
            .with_bandwidth_sink(bandwidth_sink.clone())
            .with_max_parallel_dials(litep2p_config.max_parallel_dials)
            .with_connection_limits_config(litep2p_config.connection_limits)
            .build();

        let transport_handle = transport_manager.transport_manager_handle();
        // add known addresses to `TransportManager`, if any exist
        if !litep2p_config.known_addresses.is_empty() {
            for (peer, addresses) in litep2p_config.known_addresses {
                transport_manager.add_known_address(peer, addresses.iter().cloned());
            }
        }

        // start notification protocol event loops
        for (protocol, config) in litep2p_config.notification_protocols.into_iter() {
            tracing::debug!(
                target: LOG_TARGET,
                ?protocol,
                "enable notification protocol",
            );

            let service = transport_manager.register_protocol(
                protocol,
                config.fallback_names.clone(),
                config.codec,
                litep2p_config.keep_alive_timeout,
            );
            let executor = Arc::clone(&litep2p_config.executor);
            litep2p_config.executor.run(Box::pin(async move {
                NotificationProtocol::new(service, config, executor).run().await
            }));
        }

        // start request-response protocol event loops
        for (protocol, config) in litep2p_config.request_response_protocols.into_iter() {
            tracing::debug!(
                target: LOG_TARGET,
                ?protocol,
                "enable request-response protocol",
            );

            let service = transport_manager.register_protocol(
                protocol,
                config.fallback_names.clone(),
                config.codec,
                litep2p_config.keep_alive_timeout,
            );
            litep2p_config.executor.run(Box::pin(async move {
                RequestResponseProtocol::new(service, config).run().await
            }));
        }

        // start user protocol event loops
        for (protocol_name, protocol) in litep2p_config.user_protocols.into_iter() {
            tracing::debug!(target: LOG_TARGET, protocol = ?protocol_name, "enable user protocol");

            let service = transport_manager.register_protocol(
                protocol_name,
                Vec::new(),
                protocol.codec(),
                litep2p_config.keep_alive_timeout,
            );
            litep2p_config.executor.run(Box::pin(async move {
                let _ = protocol.run(service).await;
            }));
        }

        // start ping protocol event loop if enabled
        if let Some(ping_config) = litep2p_config.ping.take() {
            tracing::debug!(
                target: LOG_TARGET,
                protocol = ?ping_config.protocol,
                "enable ipfs ping protocol",
            );

            let service = transport_manager.register_protocol(
                ping_config.protocol.clone(),
                Vec::new(),
                ping_config.codec,
                litep2p_config.keep_alive_timeout,
            );
            litep2p_config.executor.run(Box::pin(async move {
                Ping::new(service, ping_config).run().await
            }));
        }

        // start kademlia protocol event loops
        for kademlia_config in litep2p_config.kademlia.into_iter() {
            tracing::debug!(
                target: LOG_TARGET,
                protocol_names = ?kademlia_config.protocol_names,
                "enable ipfs kademlia protocol",
            );

            let main_protocol =
                kademlia_config.protocol_names.first().expect("protocol name to exist");
            let fallback_names = kademlia_config.protocol_names.iter().skip(1).cloned().collect();

            let service = transport_manager.register_protocol(
                main_protocol.clone(),
                fallback_names,
                kademlia_config.codec,
                litep2p_config.keep_alive_timeout,
            );
            litep2p_config.executor.run(Box::pin(async move {
                let _ = Kademlia::new(service, kademlia_config).run().await;
            }));
        }

        // start identify protocol event loop if enabled
        let mut identify_info = match litep2p_config.identify.take() {
            None => None,
            Some(mut identify_config) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    protocol = ?identify_config.protocol,
                    "enable ipfs identify protocol",
                );

                let service = transport_manager.register_protocol(
                    identify_config.protocol.clone(),
                    Vec::new(),
                    identify_config.codec,
                    litep2p_config.keep_alive_timeout,
                );
                identify_config.public = Some(litep2p_config.keypair.public().into());

                Some((service, identify_config))
            }
        };

        // start bitswap protocol event loop if enabled
        if let Some(bitswap_config) = litep2p_config.bitswap.take() {
            tracing::debug!(
                target: LOG_TARGET,
                protocol = ?bitswap_config.protocol,
                "enable ipfs bitswap protocol",
            );

            let service = transport_manager.register_protocol(
                bitswap_config.protocol.clone(),
                Vec::new(),
                bitswap_config.codec,
                litep2p_config.keep_alive_timeout,
            );
            litep2p_config.executor.run(Box::pin(async move {
                Bitswap::new(service, bitswap_config).run().await
            }));
        }

        // enable tcp transport if the config exists
        if let Some(config) = litep2p_config.tcp.take() {
            let handle = transport_manager.transport_handle(Arc::clone(&litep2p_config.executor));
            let (transport, transport_listen_addresses) =
                <TcpTransport as TransportBuilder>::new(handle, config, resolver.clone())?;

            for address in transport_listen_addresses {
                transport_manager.register_listen_address(address.clone());
                listen_addresses.push(address.with(Protocol::P2p(*local_peer_id.as_ref())));
            }

            transport_manager.register_transport(SupportedTransport::Tcp, Box::new(transport));
        }

        // enable quic transport if the config exists
        #[cfg(feature = "quic")]
        if let Some(config) = litep2p_config.quic.take() {
            let handle = transport_manager.transport_handle(Arc::clone(&litep2p_config.executor));
            let (transport, transport_listen_addresses) =
                <QuicTransport as TransportBuilder>::new(handle, config, resolver.clone())?;

            for address in transport_listen_addresses {
                transport_manager.register_listen_address(address.clone());
                listen_addresses.push(address.with(Protocol::P2p(*local_peer_id.as_ref())));
            }

            transport_manager.register_transport(SupportedTransport::Quic, Box::new(transport));
        }

        // enable webrtc transport if the config exists
        #[cfg(feature = "webrtc")]
        if let Some(config) = litep2p_config.webrtc.take() {
            let handle = transport_manager.transport_handle(Arc::clone(&litep2p_config.executor));
            let (transport, transport_listen_addresses) =
                <WebRtcTransport as TransportBuilder>::new(handle, config, resolver.clone())?;

            for address in transport_listen_addresses {
                transport_manager.register_listen_address(address.clone());
                listen_addresses.push(address.with(Protocol::P2p(*local_peer_id.as_ref())));
            }

            transport_manager.register_transport(SupportedTransport::WebRtc, Box::new(transport));
        }

        // enable websocket transport if the config exists
        #[cfg(feature = "websocket")]
        if let Some(config) = litep2p_config.websocket.take() {
            let handle = transport_manager.transport_handle(Arc::clone(&litep2p_config.executor));
            let (transport, transport_listen_addresses) =
                <WebSocketTransport as TransportBuilder>::new(handle, config, resolver)?;

            for address in transport_listen_addresses {
                transport_manager.register_listen_address(address.clone());
                listen_addresses.push(address.with(Protocol::P2p(*local_peer_id.as_ref())));
            }

            transport_manager
                .register_transport(SupportedTransport::WebSocket, Box::new(transport));
        }

        // enable mdns if the config exists
        if let Some(config) = litep2p_config.mdns.take() {
            let mdns = Mdns::new(transport_handle, config, listen_addresses.clone());

            litep2p_config.executor.run(Box::pin(async move {
                let _ = mdns.start().await;
            }));
        }

        // if identify was enabled, give it the enabled protocols and listen addresses and start it
        if let Some((service, mut identify_config)) = identify_info.take() {
            identify_config.protocols = transport_manager.protocols().cloned().collect();
            let identify = Identify::new(service, identify_config);

            litep2p_config.executor.run(Box::pin(async move {
                let _ = identify.run().await;
            }));
        }

        if transport_manager.installed_transports().count() == 0 {
            return Err(Error::Other("No transport specified".to_string()));
        }

        // verify that at least one transport is specified
        if listen_addresses.is_empty() {
            tracing::warn!(
                target: LOG_TARGET,
                "litep2p started with no listen addresses, cannot accept inbound connections",
            );
        }

        Ok(Self {
            local_peer_id,
            bandwidth_sink,
            listen_addresses,
            transport_manager,
        })
    }

    /// Collect supported transports before initializing the transports themselves.
    ///
    /// Information of the supported transports is needed to initialize protocols but
    /// information about protocols must be known to initialize transports so the initialization
    /// has to be split.
    fn supported_transports(config: &Litep2pConfig) -> HashSet<SupportedTransport> {
        let mut supported_transports = HashSet::new();

        config
            .tcp
            .is_some()
            .then(|| supported_transports.insert(SupportedTransport::Tcp));
        #[cfg(feature = "quic")]
        config
            .quic
            .is_some()
            .then(|| supported_transports.insert(SupportedTransport::Quic));
        #[cfg(feature = "websocket")]
        config
            .websocket
            .is_some()
            .then(|| supported_transports.insert(SupportedTransport::WebSocket));
        #[cfg(feature = "webrtc")]
        config
            .webrtc
            .is_some()
            .then(|| supported_transports.insert(SupportedTransport::WebRtc));

        supported_transports
    }

    /// Get local peer ID.
    pub fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }

    /// Get the list of public addresses of the node.
    pub fn public_addresses(&self) -> PublicAddresses {
        self.transport_manager.public_addresses()
    }

    /// Get the list of listen addresses of the node.
    pub fn listen_addresses(&self) -> impl Iterator<Item = &Multiaddr> {
        self.listen_addresses.iter()
    }

    /// Get handle to bandwidth sink.
    pub fn bandwidth_sink(&self) -> BandwidthSink {
        self.bandwidth_sink.clone()
    }

    /// Dial peer.
    pub async fn dial(&mut self, peer: &PeerId) -> crate::Result<()> {
        self.transport_manager.dial(*peer).await
    }

    /// Dial address.
    pub async fn dial_address(&mut self, address: Multiaddr) -> crate::Result<()> {
        self.transport_manager.dial_address(address).await
    }

    /// Add one ore more known addresses for peer.
    ///
    /// Return value denotes how many addresses were added for the peer.
    /// Addresses belonging to disabled/unsupported transports will be ignored.
    pub fn add_known_address(
        &mut self,
        peer: PeerId,
        address: impl Iterator<Item = Multiaddr>,
    ) -> usize {
        self.transport_manager.add_known_address(peer, address)
    }

    /// Poll next event.
    ///
    /// This function must be called in order for litep2p to make progress.
    pub async fn next_event(&mut self) -> Option<Litep2pEvent> {
        loop {
            match self.transport_manager.next().await? {
                TransportEvent::ConnectionEstablished { peer, endpoint, .. } =>
                    return Some(Litep2pEvent::ConnectionEstablished { peer, endpoint }),
                TransportEvent::ConnectionClosed {
                    peer,
                    connection_id,
                } =>
                    return Some(Litep2pEvent::ConnectionClosed {
                        peer,
                        connection_id,
                    }),
                TransportEvent::DialFailure { address, error, .. } =>
                    return Some(Litep2pEvent::DialFailure { address, error }),

                TransportEvent::OpenFailure { errors, .. } => {
                    return Some(Litep2pEvent::ListDialFailures { errors });
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::ConfigBuilder,
        protocol::{libp2p::ping, notification::Config as NotificationConfig},
        types::protocol::ProtocolName,
        Litep2p, Litep2pEvent, PeerId,
    };
    use multiaddr::{Multiaddr, Protocol};
    use multihash::Multihash;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn initialize_litep2p() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (config1, _service1) = NotificationConfig::new(
            ProtocolName::from("/notificaton/1"),
            1337usize,
            vec![1, 2, 3, 4],
            Vec::new(),
            false,
            64,
            64,
            true,
        );
        let (config2, _service2) = NotificationConfig::new(
            ProtocolName::from("/notificaton/2"),
            1337usize,
            vec![1, 2, 3, 4],
            Vec::new(),
            false,
            64,
            64,
            true,
        );
        let (ping_config, _ping_event_stream) = ping::Config::default();

        let config = ConfigBuilder::new()
            .with_tcp(Default::default())
            .with_notification_protocol(config1)
            .with_notification_protocol(config2)
            .with_libp2p_ping(ping_config)
            .build();

        let _litep2p = Litep2p::new(config).unwrap();
    }

    #[tokio::test]
    async fn no_transport_given() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (config1, _service1) = NotificationConfig::new(
            ProtocolName::from("/notificaton/1"),
            1337usize,
            vec![1, 2, 3, 4],
            Vec::new(),
            false,
            64,
            64,
            true,
        );
        let (config2, _service2) = NotificationConfig::new(
            ProtocolName::from("/notificaton/2"),
            1337usize,
            vec![1, 2, 3, 4],
            Vec::new(),
            false,
            64,
            64,
            true,
        );
        let (ping_config, _ping_event_stream) = ping::Config::default();

        let config = ConfigBuilder::new()
            .with_notification_protocol(config1)
            .with_notification_protocol(config2)
            .with_libp2p_ping(ping_config)
            .build();

        assert!(Litep2p::new(config).is_err());
    }

    #[tokio::test]
    async fn dial_same_address_twice() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (config1, _service1) = NotificationConfig::new(
            ProtocolName::from("/notificaton/1"),
            1337usize,
            vec![1, 2, 3, 4],
            Vec::new(),
            false,
            64,
            64,
            true,
        );
        let (config2, _service2) = NotificationConfig::new(
            ProtocolName::from("/notificaton/2"),
            1337usize,
            vec![1, 2, 3, 4],
            Vec::new(),
            false,
            64,
            64,
            true,
        );
        let (ping_config, _ping_event_stream) = ping::Config::default();

        let config = ConfigBuilder::new()
            .with_tcp(Default::default())
            .with_notification_protocol(config1)
            .with_notification_protocol(config2)
            .with_libp2p_ping(ping_config)
            .build();

        let peer = PeerId::random();
        let address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(255, 254, 253, 252)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        let mut litep2p = Litep2p::new(config).unwrap();
        litep2p.dial_address(address.clone()).await.unwrap();
        litep2p.dial_address(address.clone()).await.unwrap();

        match litep2p.next_event().await {
            Some(Litep2pEvent::DialFailure { .. }) => {}
            _ => panic!("invalid event received"),
        }

        // verify that the second same dial was ignored and the dial failure is reported only once
        match tokio::time::timeout(std::time::Duration::from_secs(20), litep2p.next_event()).await {
            Err(_) => {}
            _ => panic!("invalid event received"),
        }
    }
}
