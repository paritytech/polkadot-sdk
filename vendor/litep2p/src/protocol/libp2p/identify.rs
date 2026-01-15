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

//! [`/ipfs/identify/1.0.0`](https://github.com/libp2p/specs/blob/master/identify/README.md) implementation.

use crate::{
    codec::ProtocolCodec,
    crypto::PublicKey,
    error::{Error, SubstreamError},
    protocol::{Direction, TransportEvent, TransportService},
    substream::Substream,
    transport::Endpoint,
    types::{protocol::ProtocolName, SubstreamId},
    utils::futures_stream::FuturesStream,
    PeerId, DEFAULT_CHANNEL_SIZE,
};

use futures::{future::BoxFuture, Stream, StreamExt};
use multiaddr::Multiaddr;
use prost::Message;
use tokio::sync::mpsc::{channel, Sender};
use tokio_stream::wrappers::ReceiverStream;

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

/// Log target for the file.
const LOG_TARGET: &str = "litep2p::ipfs::identify";

/// IPFS Identify protocol name
const PROTOCOL_NAME: &str = "/ipfs/id/1.0.0";

/// IPFS Identify push protocol name.
const _PUSH_PROTOCOL_NAME: &str = "/ipfs/id/push/1.0.0";

/// Default agent version.
const DEFAULT_AGENT: &str = "litep2p/1.0.0";

/// Size for `/ipfs/ping/1.0.0` payloads.
// TODO: https://github.com/paritytech/litep2p/issues/334 what is the max size?
const IDENTIFY_PAYLOAD_SIZE: usize = 4096;

mod identify_schema {
    include!(concat!(env!("OUT_DIR"), "/identify.rs"));
}

/// Identify configuration.
pub struct Config {
    /// Protocol name.
    pub(crate) protocol: ProtocolName,

    /// Codec used by the protocol.
    pub(crate) codec: ProtocolCodec,

    /// TX channel for sending events to the user protocol.
    tx_event: Sender<IdentifyEvent>,

    // Public key of the local node, filled by `Litep2p`.
    pub(crate) public: Option<PublicKey>,

    /// Protocols supported by the local node, filled by `Litep2p`.
    pub(crate) protocols: Vec<ProtocolName>,

    /// Protocol version.
    pub(crate) protocol_version: String,

    /// User agent.
    pub(crate) user_agent: Option<String>,
}

impl Config {
    /// Create new [`Config`].
    ///
    /// Returns a config that is given to `Litep2pConfig` and an event stream for
    /// [`IdentifyEvent`]s.
    pub fn new(
        protocol_version: String,
        user_agent: Option<String>,
    ) -> (Self, Box<dyn Stream<Item = IdentifyEvent> + Send + Unpin>) {
        let (tx_event, rx_event) = channel(DEFAULT_CHANNEL_SIZE);

        (
            Self {
                tx_event,
                public: None,
                protocol_version,
                user_agent,
                codec: ProtocolCodec::UnsignedVarint(Some(IDENTIFY_PAYLOAD_SIZE)),
                protocols: Vec::new(),
                protocol: ProtocolName::from(PROTOCOL_NAME),
            },
            Box::new(ReceiverStream::new(rx_event)),
        )
    }
}

/// Events emitted by Identify protocol.
#[derive(Debug)]
pub enum IdentifyEvent {
    /// Peer identified.
    PeerIdentified {
        /// Peer ID.
        peer: PeerId,

        /// Protocol version.
        protocol_version: Option<String>,

        /// User agent.
        user_agent: Option<String>,

        /// Supported protocols.
        supported_protocols: HashSet<ProtocolName>,

        /// Observed address.
        observed_address: Multiaddr,

        /// Listen addresses.
        listen_addresses: Vec<Multiaddr>,
    },
}

/// Identify response received from remote.
struct IdentifyResponse {
    /// Remote peer ID.
    peer: PeerId,

    /// Protocol version.
    protocol_version: Option<String>,

    /// User agent.
    user_agent: Option<String>,

    /// Protocols supported by remote.
    supported_protocols: HashSet<String>,

    /// Remote's listen addresses.
    listen_addresses: Vec<Multiaddr>,

    /// Observed address.
    observed_address: Option<Multiaddr>,
}

pub(crate) struct Identify {
    // Connection service.
    service: TransportService,

    /// TX channel for sending events to the user protocol.
    tx: Sender<IdentifyEvent>,

    /// Connected peers and their observed addresses.
    peers: HashMap<PeerId, Endpoint>,

    // Public key of the local node, filled by `Litep2p`.
    public: PublicKey,

    /// Local peer ID.
    local_peer_id: PeerId,

    /// Protocol version.
    protocol_version: String,

    /// User agent.
    user_agent: String,

    /// Protocols supported by the local node, filled by `Litep2p`.
    protocols: Vec<String>,

    /// Pending outbound substreams.
    pending_outbound: FuturesStream<BoxFuture<'static, crate::Result<IdentifyResponse>>>,

    /// Pending inbound substreams.
    pending_inbound: FuturesStream<BoxFuture<'static, ()>>,
}

impl Identify {
    /// Create new [`Identify`] protocol.
    pub(crate) fn new(service: TransportService, config: Config) -> Self {
        // The public key is always supplied by litep2p and is the one
        // used to identify the local peer. This is a similar story to the
        // supported protocols.
        let public = config.public.expect("public key to always be supplied by litep2p; qed");
        let local_peer_id = public.to_peer_id();

        Self {
            service,
            tx: config.tx_event,
            peers: HashMap::new(),
            public,
            local_peer_id,
            protocol_version: config.protocol_version,
            user_agent: config.user_agent.unwrap_or(DEFAULT_AGENT.to_string()),
            pending_inbound: FuturesStream::new(),
            pending_outbound: FuturesStream::new(),
            protocols: config.protocols.iter().map(|protocol| protocol.to_string()).collect(),
        }
    }

    /// Connection established to remote peer.
    fn on_connection_established(&mut self, peer: PeerId, endpoint: Endpoint) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, ?endpoint, "connection established");

        self.service.open_substream(peer)?;
        self.peers.insert(peer, endpoint);

        Ok(())
    }

    /// Connection closed to remote peer.
    fn on_connection_closed(&mut self, peer: PeerId) {
        tracing::trace!(target: LOG_TARGET, ?peer, "connection closed");

        self.peers.remove(&peer);
    }

    /// Inbound substream opened.
    fn on_inbound_substream(
        &mut self,
        peer: PeerId,
        protocol: ProtocolName,
        mut substream: Substream,
    ) {
        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            ?protocol,
            "inbound substream opened"
        );

        let observed_addr = match self.peers.get(&peer) {
            Some(endpoint) => Some(endpoint.address().to_vec()),
            None => {
                tracing::warn!(
                    target: LOG_TARGET,
                    ?peer,
                    %protocol,
                    "inbound identify substream opened for peer who doesn't exist",
                );
                None
            }
        };

        let mut listen_addr: HashSet<_> =
            self.service.listen_addresses().into_iter().map(|addr| addr.to_vec()).collect();
        listen_addr
            .extend(self.service.public_addresses().inner.read().iter().map(|addr| addr.to_vec()));

        let identify = identify_schema::Identify {
            protocol_version: Some(self.protocol_version.clone()),
            agent_version: Some(self.user_agent.clone()),
            public_key: Some(self.public.to_protobuf_encoding()),
            listen_addrs: listen_addr.into_iter().collect(),
            observed_addr,
            protocols: self.protocols.clone(),
        };

        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            ?identify,
            "sending identify response",
        );

        let mut msg = Vec::with_capacity(identify.encoded_len());
        identify.encode(&mut msg).expect("`msg` to have enough capacity");

        self.pending_inbound.push(Box::pin(async move {
            match tokio::time::timeout(Duration::from_secs(10), substream.send_framed(msg.into()))
                .await
            {
                Err(error) => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        ?error,
                        "timed out while sending ipfs identify response",
                    );
                }
                Ok(Err(error)) => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        ?error,
                        "failed to send ipfs identify response",
                    );
                }
                Ok(_) => {
                    substream.close().await;
                }
            }
        }))
    }

    /// Outbound substream opened.
    fn on_outbound_substream(
        &mut self,
        peer: PeerId,
        protocol: ProtocolName,
        substream_id: SubstreamId,
        mut substream: Substream,
    ) {
        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            ?protocol,
            ?substream_id,
            "outbound substream opened"
        );

        let local_peer_id = self.local_peer_id;

        self.pending_outbound.push(Box::pin(async move {
            let payload =
                match tokio::time::timeout(Duration::from_secs(10), substream.next()).await {
                    Err(_) => return Err(Error::Timeout),
                    Ok(None) =>
                        return Err(Error::SubstreamError(SubstreamError::ReadFailure(Some(
                            substream_id,
                        )))),
                    Ok(Some(Err(error))) => return Err(error.into()),
                    Ok(Some(Ok(payload))) => payload,
                };

            let info = identify_schema::Identify::decode(payload.to_vec().as_slice()).map_err(
                |err| {
                    tracing::debug!(target: LOG_TARGET, ?peer, ?err, "peer identified provided undecodable identify response");
                    err
                })?;

            tracing::trace!(target: LOG_TARGET, ?peer, ?info, "peer identified");

            let listen_addresses = info
                .listen_addrs
                .iter()
                .filter_map(|address| {
                    let address = Multiaddr::try_from(address.clone()).ok()?;

                    // Ensure the address ends with the provided peer ID and is not empty.
                    if address.is_empty() {
                        tracing::debug!(target: LOG_TARGET, ?peer, ?address, "peer identified provided empty listen address");
                        return None;
                    }
                    if let Some(multiaddr::Protocol::P2p(peer_id)) = address.iter().last() {
                        if peer_id != peer.into() {
                            tracing::debug!(target: LOG_TARGET, ?peer, ?address, "peer identified provided listen address with incorrect peer ID; discarding the address");
                            return None;
                        }
                    }

                    Some(address)
                })
                .collect();

            let observed_address =
                info.observed_addr.and_then(|address| {
                    let address = Multiaddr::try_from(address).ok()?;

                    if address.is_empty() {
                        tracing::debug!(target: LOG_TARGET, ?peer, ?address, "peer identified provided empty observed address");
                        return None;
                    }

                    if let Some(multiaddr::Protocol::P2p(peer_id)) = address.iter().last() {
                        if peer_id != local_peer_id.into() {
                            tracing::debug!(target: LOG_TARGET, ?peer, ?address, "peer identified provided observed address with peer ID not matching our peer ID; discarding address");
                            return None;
                        }
                    }

                    Some(address)
                });

            let protocol_version = info.protocol_version;
            let user_agent = info.agent_version;

            Ok(IdentifyResponse {
                peer,
                protocol_version,
                user_agent,
                supported_protocols: HashSet::from_iter(info.protocols),
                observed_address,
                listen_addresses,
            })
        }));
    }

    /// Start [`Identify`] event loop.
    pub async fn run(mut self) {
        tracing::debug!(target: LOG_TARGET, "starting identify event loop");

        loop {
            tokio::select! {
                event = self.service.next() => match event {
                    None => {
                        tracing::warn!(target: LOG_TARGET, "transport service stream ended, terminating identify event loop");
                        return
                    },
                    Some(TransportEvent::ConnectionEstablished { peer, endpoint }) => {
                        let _ = self.on_connection_established(peer, endpoint);
                    }
                    Some(TransportEvent::ConnectionClosed { peer }) => {
                        self.on_connection_closed(peer);
                    }
                    Some(TransportEvent::SubstreamOpened {
                        peer,
                        protocol,
                        direction,
                        substream,
                        ..
                    }) => match direction {
                        Direction::Inbound => self.on_inbound_substream(peer, protocol, substream),
                        Direction::Outbound(substream_id) => self.on_outbound_substream(peer, protocol, substream_id, substream),
                    },
                    _ => {}
                },
                _ = self.pending_inbound.next(), if !self.pending_inbound.is_empty() => {}
                event = self.pending_outbound.next(), if !self.pending_outbound.is_empty() => match event {
                    Some(Ok(response)) => {
                        let _ = self.tx
                            .send(IdentifyEvent::PeerIdentified {
                                peer: response.peer,
                                protocol_version: response.protocol_version,
                                user_agent: response.user_agent,
                                supported_protocols: response.supported_protocols.into_iter().map(From::from).collect(),
                                observed_address: response.observed_address.map_or(Multiaddr::empty(), |address| address),
                                listen_addresses: response.listen_addresses,
                            })
                            .await;
                    }
                    Some(Err(error)) => tracing::debug!(target: LOG_TARGET, ?error, "failed to read ipfs identify response"),
                    None => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::ConfigBuilder, transport::tcp::config::Config as TcpConfig, Litep2p};
    use multiaddr::{Multiaddr, Protocol};

    fn create_litep2p() -> (
        Litep2p,
        Box<dyn Stream<Item = IdentifyEvent> + Send + Unpin>,
        PeerId,
    ) {
        let (identify_config, identify) =
            Config::new("1.0.0".to_string(), Some("litep2p/1.0.0".to_string()));

        let keypair = crate::crypto::ed25519::Keypair::generate();
        let peer = PeerId::from_public_key(&crate::crypto::PublicKey::Ed25519(keypair.public()));
        let config = ConfigBuilder::new()
            .with_keypair(keypair)
            .with_tcp(TcpConfig {
                listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
                ..Default::default()
            })
            .with_libp2p_identify(identify_config)
            .build();

        (Litep2p::new(config).unwrap(), identify, peer)
    }

    #[tokio::test]
    async fn update_identify_addresses() {
        // Create two instances of litep2p
        let (mut litep2p1, mut event_stream1, peer1) = create_litep2p();
        let (mut litep2p2, mut event_stream2, _peer2) = create_litep2p();
        let litep2p1_address = litep2p1.listen_addresses().next().unwrap();

        let multiaddr: Multiaddr = "/ip6/::9/tcp/111".parse().unwrap();
        // Litep2p1 is now reporting the new address.
        assert!(litep2p1.public_addresses().add_address(multiaddr.clone()).unwrap());

        // Dial `litep2p1`
        litep2p2.dial_address(litep2p1_address.clone()).await.unwrap();

        let expected_multiaddr = multiaddr.with(Protocol::P2p(peer1.into()));

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = litep2p1.next_event() => {}
                    _event = event_stream1.next() => {}
                }
            }
        });

        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {}
                event = event_stream2.next() => match event {
                    Some(IdentifyEvent::PeerIdentified {
                        listen_addresses,
                        ..
                    }) => {
                        assert!(listen_addresses.iter().any(|address| address == &expected_multiaddr));
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}
