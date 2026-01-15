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

//! [`Litep2p`](`crate::Litep2p`) configuration.

use crate::{
    crypto::ed25519::Keypair,
    executor::{DefaultExecutor, Executor},
    protocol::{
        libp2p::{bitswap, identify, kademlia, ping},
        mdns::Config as MdnsConfig,
        notification, request_response, UserProtocol,
    },
    transport::{
        manager::limits::ConnectionLimitsConfig, tcp::config::Config as TcpConfig,
        KEEP_ALIVE_TIMEOUT, MAX_PARALLEL_DIALS,
    },
    types::protocol::ProtocolName,
    PeerId,
};

#[cfg(feature = "quic")]
use crate::transport::quic::config::Config as QuicConfig;
#[cfg(feature = "webrtc")]
use crate::transport::webrtc::config::Config as WebRtcConfig;
#[cfg(feature = "websocket")]
use crate::transport::websocket::config::Config as WebSocketConfig;

use multiaddr::Multiaddr;

use std::{collections::HashMap, sync::Arc, time::Duration};

/// Connection role.
#[derive(Debug, Copy, Clone)]
pub enum Role {
    /// Dialer.
    Dialer,

    /// Listener.
    Listener,
}

impl From<Role> for crate::yamux::Mode {
    fn from(value: Role) -> Self {
        match value {
            Role::Dialer => crate::yamux::Mode::Client,
            Role::Listener => crate::yamux::Mode::Server,
        }
    }
}

/// Configuration builder for [`Litep2p`](`crate::Litep2p`).
pub struct ConfigBuilder {
    /// TCP transport configuration.
    tcp: Option<TcpConfig>,

    /// QUIC transport config.
    #[cfg(feature = "quic")]
    quic: Option<QuicConfig>,

    /// WebRTC transport config.
    #[cfg(feature = "webrtc")]
    webrtc: Option<WebRtcConfig>,

    /// WebSocket transport config.
    #[cfg(feature = "websocket")]
    websocket: Option<WebSocketConfig>,

    /// Keypair.
    keypair: Option<Keypair>,

    /// Ping protocol config.
    ping: Option<ping::Config>,

    /// Identify protocol config.
    identify: Option<identify::Config>,

    /// Kademlia protocol config.
    kademlia: Vec<kademlia::Config>,

    /// Bitswap protocol config.
    bitswap: Option<bitswap::Config>,

    /// Notification protocols.
    notification_protocols: HashMap<ProtocolName, notification::Config>,

    /// Request-response protocols.
    request_response_protocols: HashMap<ProtocolName, request_response::Config>,

    /// User protocols.
    user_protocols: HashMap<ProtocolName, Box<dyn UserProtocol>>,

    /// mDNS configuration.
    mdns: Option<MdnsConfig>,

    /// Known addresess.
    known_addresses: Vec<(PeerId, Vec<Multiaddr>)>,

    /// Executor for running futures.
    executor: Option<Arc<dyn Executor>>,

    /// Maximum number of parallel dial attempts.
    max_parallel_dials: usize,

    /// Connection limits config.
    connection_limits: ConnectionLimitsConfig,

    /// Close the connection if no substreams are open within this time frame.
    keep_alive_timeout: Duration,

    /// Use system's DNS config.
    use_system_dns_config: bool,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    /// Create empty [`ConfigBuilder`].
    pub fn new() -> Self {
        Self {
            tcp: None,
            #[cfg(feature = "quic")]
            quic: None,
            #[cfg(feature = "webrtc")]
            webrtc: None,
            #[cfg(feature = "websocket")]
            websocket: None,
            keypair: None,
            ping: None,
            identify: None,
            kademlia: Vec::new(),
            bitswap: None,
            mdns: None,
            executor: None,
            max_parallel_dials: MAX_PARALLEL_DIALS,
            user_protocols: HashMap::new(),
            notification_protocols: HashMap::new(),
            request_response_protocols: HashMap::new(),
            known_addresses: Vec::new(),
            connection_limits: ConnectionLimitsConfig::default(),
            keep_alive_timeout: KEEP_ALIVE_TIMEOUT,
            use_system_dns_config: false,
        }
    }

    /// Add TCP transport configuration, enabling the transport.
    pub fn with_tcp(mut self, config: TcpConfig) -> Self {
        self.tcp = Some(config);
        self
    }

    /// Add QUIC transport configuration, enabling the transport.
    #[cfg(feature = "quic")]
    pub fn with_quic(mut self, config: QuicConfig) -> Self {
        self.quic = Some(config);
        self
    }

    /// Add WebRTC transport configuration, enabling the transport.
    #[cfg(feature = "webrtc")]
    pub fn with_webrtc(mut self, config: WebRtcConfig) -> Self {
        self.webrtc = Some(config);
        self
    }

    /// Add WebSocket transport configuration, enabling the transport.
    #[cfg(feature = "websocket")]
    pub fn with_websocket(mut self, config: WebSocketConfig) -> Self {
        self.websocket = Some(config);
        self
    }

    /// Add keypair.
    ///
    /// If no keypair is specified, litep2p creates a new keypair.
    pub fn with_keypair(mut self, keypair: Keypair) -> Self {
        self.keypair = Some(keypair);
        self
    }

    /// Enable notification protocol.
    pub fn with_notification_protocol(mut self, config: notification::Config) -> Self {
        self.notification_protocols.insert(config.protocol_name().clone(), config);
        self
    }

    /// Enable IPFS Ping protocol.
    pub fn with_libp2p_ping(mut self, config: ping::Config) -> Self {
        self.ping = Some(config);
        self
    }

    /// Enable IPFS Identify protocol.
    pub fn with_libp2p_identify(mut self, config: identify::Config) -> Self {
        self.identify = Some(config);
        self
    }

    /// Enable IPFS Kademlia protocol.
    pub fn with_libp2p_kademlia(mut self, config: kademlia::Config) -> Self {
        self.kademlia.push(config);
        self
    }

    /// Enable IPFS Bitswap protocol.
    pub fn with_libp2p_bitswap(mut self, config: bitswap::Config) -> Self {
        self.bitswap = Some(config);
        self
    }

    /// Enable request-response protocol.
    pub fn with_request_response_protocol(mut self, config: request_response::Config) -> Self {
        self.request_response_protocols.insert(config.protocol_name().clone(), config);
        self
    }

    /// Enable user protocol.
    pub fn with_user_protocol(mut self, protocol: Box<dyn UserProtocol>) -> Self {
        self.user_protocols.insert(protocol.protocol(), protocol);
        self
    }

    /// Enable mDNS for peer discoveries in the local network.
    pub fn with_mdns(mut self, config: MdnsConfig) -> Self {
        self.mdns = Some(config);
        self
    }

    /// Add known address(es) for one or more peers.
    pub fn with_known_addresses(
        mut self,
        addresses: impl Iterator<Item = (PeerId, Vec<Multiaddr>)>,
    ) -> Self {
        self.known_addresses = addresses.collect();
        self
    }

    /// Add executor for running futures spawned by `litep2p`.
    ///
    /// If no executor is specified, `litep2p` defaults to calling `tokio::spawn()`.
    pub fn with_executor(mut self, executor: Arc<dyn Executor>) -> Self {
        self.executor = Some(executor);
        self
    }

    /// How many addresses should litep2p attempt to dial in parallel.
    pub fn with_max_parallel_dials(mut self, max_parallel_dials: usize) -> Self {
        self.max_parallel_dials = max_parallel_dials;
        self
    }

    /// Set connection limits configuration.
    pub fn with_connection_limits(mut self, config: ConnectionLimitsConfig) -> Self {
        self.connection_limits = config;
        self
    }

    /// Set keep alive timeout for connections.
    pub fn with_keep_alive_timeout(mut self, timeout: Duration) -> Self {
        self.keep_alive_timeout = timeout;
        self
    }

    /// Set DNS resolver according to system configuration instead of default (Google).
    pub fn with_system_resolver(mut self) -> Self {
        self.use_system_dns_config = true;
        self
    }

    /// Build [`Litep2pConfig`].
    pub fn build(mut self) -> Litep2pConfig {
        let keypair = match self.keypair {
            Some(keypair) => keypair,
            None => Keypair::generate(),
        };

        Litep2pConfig {
            keypair,
            tcp: self.tcp.take(),
            mdns: self.mdns.take(),
            #[cfg(feature = "quic")]
            quic: self.quic.take(),
            #[cfg(feature = "webrtc")]
            webrtc: self.webrtc.take(),
            #[cfg(feature = "websocket")]
            websocket: self.websocket.take(),
            ping: self.ping.take(),
            identify: self.identify.take(),
            kademlia: self.kademlia,
            bitswap: self.bitswap.take(),
            max_parallel_dials: self.max_parallel_dials,
            executor: self.executor.map_or(Arc::new(DefaultExecutor {}), |executor| executor),
            user_protocols: self.user_protocols,
            notification_protocols: self.notification_protocols,
            request_response_protocols: self.request_response_protocols,
            known_addresses: self.known_addresses,
            connection_limits: self.connection_limits,
            keep_alive_timeout: self.keep_alive_timeout,
            use_system_dns_config: self.use_system_dns_config,
        }
    }
}

/// Configuration for [`Litep2p`](`crate::Litep2p`).
pub struct Litep2pConfig {
    // TCP transport configuration.
    pub(crate) tcp: Option<TcpConfig>,

    /// QUIC transport config.
    #[cfg(feature = "quic")]
    pub(crate) quic: Option<QuicConfig>,

    /// WebRTC transport config.
    #[cfg(feature = "webrtc")]
    pub(crate) webrtc: Option<WebRtcConfig>,

    /// WebSocket transport config.
    #[cfg(feature = "websocket")]
    pub(crate) websocket: Option<WebSocketConfig>,

    /// Keypair.
    pub(crate) keypair: Keypair,

    /// Ping protocol configuration, if enabled.
    pub(crate) ping: Option<ping::Config>,

    /// Identify protocol configuration, if enabled.
    pub(crate) identify: Option<identify::Config>,

    /// Kademlia protocol configuration, if enabled.
    pub(crate) kademlia: Vec<kademlia::Config>,

    /// Bitswap protocol configuration, if enabled.
    pub(crate) bitswap: Option<bitswap::Config>,

    /// Notification protocols.
    pub(crate) notification_protocols: HashMap<ProtocolName, notification::Config>,

    /// Request-response protocols.
    pub(crate) request_response_protocols: HashMap<ProtocolName, request_response::Config>,

    /// User protocols.
    pub(crate) user_protocols: HashMap<ProtocolName, Box<dyn UserProtocol>>,

    /// mDNS configuration.
    pub(crate) mdns: Option<MdnsConfig>,

    /// Executor.
    pub(crate) executor: Arc<dyn Executor>,

    /// Maximum number of parallel dial attempts.
    pub(crate) max_parallel_dials: usize,

    /// Known addresses.
    pub(crate) known_addresses: Vec<(PeerId, Vec<Multiaddr>)>,

    /// Connection limits config.
    pub(crate) connection_limits: ConnectionLimitsConfig,

    /// Close the connection if no substreams are open within this time frame.
    pub(crate) keep_alive_timeout: Duration,

    /// Use system's DNS config.
    pub(crate) use_system_dns_config: bool,
}
