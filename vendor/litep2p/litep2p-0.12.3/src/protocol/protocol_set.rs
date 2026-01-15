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

use crate::{
    codec::ProtocolCodec,
    error::{Error, NegotiationError, SubstreamError},
    multistream_select::{
        NegotiationError as MultiStreamNegotiationError, ProtocolError as MultiStreamProtocolError,
    },
    protocol::{
        connection::{ConnectionHandle, Permit},
        Direction, TransportEvent,
    },
    substream::Substream,
    transport::{
        manager::{ProtocolContext, TransportManagerEvent},
        Endpoint,
    },
    types::{protocol::ProtocolName, ConnectionId, SubstreamId},
    PeerId,
};

use futures::{stream::FuturesUnordered, Stream, StreamExt};
use multiaddr::Multiaddr;
use tokio::sync::mpsc::{channel, Receiver, Sender};

#[cfg(any(feature = "quic", feature = "webrtc", feature = "websocket"))]
use std::sync::atomic::Ordering;
use std::{
    collections::HashMap,
    fmt::Debug,
    pin::Pin,
    sync::{atomic::AtomicUsize, Arc},
    task::{Context, Poll},
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::protocol-set";

/// Events emitted by the underlying transport protocols.
#[derive(Debug)]
pub enum InnerTransportEvent {
    /// Connection established to `peer`.
    ConnectionEstablished {
        /// Peer ID.
        peer: PeerId,

        /// Connection ID.
        connection: ConnectionId,

        /// Endpoint.
        endpoint: Endpoint,

        /// Handle for communicating with the connection.
        sender: ConnectionHandle,
    },

    /// Connection closed.
    ConnectionClosed {
        /// Peer ID.
        peer: PeerId,

        /// Connection ID.
        connection: ConnectionId,
    },

    /// Failed to dial peer.
    ///
    /// This is reported to that protocol which initiated the connection.
    DialFailure {
        /// Peer ID.
        peer: PeerId,

        /// Dialed addresses.
        addresses: Vec<Multiaddr>,
    },

    /// Substream opened for `peer`.
    SubstreamOpened {
        /// Peer ID.
        peer: PeerId,

        /// Protocol name.
        ///
        /// One protocol handler may handle multiple sub-protocols (such as `/ipfs/identify/1.0.0`
        /// and `/ipfs/identify/push/1.0.0`) or it may have aliases which should be handled by
        /// the same protocol handler. When the substream is sent from transport to the protocol
        /// handler, the protocol name that was used to negotiate the substream is also sent so
        /// the protocol can handle the substream appropriately.
        protocol: ProtocolName,

        /// Fallback name.
        ///
        /// If the substream was negotiated using a fallback name of the main protocol,
        /// `fallback` is `Some`.
        fallback: Option<ProtocolName>,

        /// Substream direction.
        ///
        /// Informs the protocol whether the substream is inbound (opened by the remote node)
        /// or outbound (opened by the local node). This allows the protocol to distinguish
        /// between the two types of substreams and execute correct code for the substream.
        ///
        /// Outbound substreams also contain the substream ID which allows the protocol to
        /// distinguish between different outbound substreams.
        direction: Direction,

        /// Connection ID.
        connection_id: ConnectionId,

        /// Substream.
        substream: Substream,
    },

    /// Failed to open substream.
    ///
    /// Substream open failures are reported only for outbound substreams.
    SubstreamOpenFailure {
        /// Substream ID.
        substream: SubstreamId,

        /// Error that occurred when the substream was being opened.
        error: SubstreamError,
    },
}

impl From<InnerTransportEvent> for TransportEvent {
    fn from(event: InnerTransportEvent) -> Self {
        match event {
            InnerTransportEvent::DialFailure { peer, addresses } =>
                TransportEvent::DialFailure { peer, addresses },
            InnerTransportEvent::SubstreamOpened {
                peer,
                protocol,
                fallback,
                direction,
                substream,
                ..
            } => TransportEvent::SubstreamOpened {
                peer,
                protocol,
                fallback,
                direction,
                substream,
            },
            InnerTransportEvent::SubstreamOpenFailure { substream, error } =>
                TransportEvent::SubstreamOpenFailure { substream, error },
            event => panic!("cannot convert {event:?}"),
        }
    }
}

/// Events emitted by the installed protocols to transport.
#[derive(Debug, Clone)]
pub enum ProtocolCommand {
    /// Open substream.
    OpenSubstream {
        /// Protocol name.
        protocol: ProtocolName,

        /// Fallback names.
        ///
        /// If the protocol has changed its name but wishes to support the old name(s), it must
        /// provide the old protocol names in `fallback_names`. These are fed into
        /// `multistream-select` which them attempts to negotiate a protocol for the substream
        /// using one of the provided names and if the substream is negotiated successfully, will
        /// report back the actual protocol name that was negotiated, in case the protocol
        /// needs to deal with the old version of the protocol in different way compared to
        /// the new version.
        fallback_names: Vec<ProtocolName>,

        /// Substream ID.
        ///
        /// Protocol allocates an ephemeral ID for outbound substreams which allows it to track
        /// the state of its pending substream. The ID is given back to protocol in
        /// [`TransportEvent::SubstreamOpened`]/[`TransportEvent::SubstreamOpenFailure`].
        ///
        /// This allows the protocol to distinguish inbound substreams from outbound substreams
        /// and associate incoming substreams with whatever logic it has.
        substream_id: SubstreamId,

        /// Connection ID.
        connection_id: ConnectionId,

        /// Connection permit.
        ///
        /// `Permit` allows the connection to be kept open while the permit is held and it is given
        /// to the substream to hold once it has been opened. When the substream is dropped, the
        /// permit is dropped and the connection may be closed if no other permit is being
        /// held.
        permit: Permit,
    },

    /// Forcibly close the connection, even if other protocols have substreams open over it.
    ForceClose,
}

/// Supported protocol information.
///
/// Each connection gets a copy of [`ProtocolSet`] which allows it to interact
/// directly with installed protocols.
pub struct ProtocolSet {
    /// Installed protocols.
    pub(crate) protocols: HashMap<ProtocolName, ProtocolContext>,
    mgr_tx: Sender<TransportManagerEvent>,
    connection: ConnectionHandle,
    rx: Receiver<ProtocolCommand>,
    #[allow(unused)]
    next_substream_id: Arc<AtomicUsize>,
    fallback_names: HashMap<ProtocolName, ProtocolName>,
}

impl ProtocolSet {
    pub fn new(
        connection_id: ConnectionId,
        mgr_tx: Sender<TransportManagerEvent>,
        next_substream_id: Arc<AtomicUsize>,
        protocols: HashMap<ProtocolName, ProtocolContext>,
    ) -> Self {
        let (tx, rx) = channel(256);

        let fallback_names = protocols
            .iter()
            .flat_map(|(protocol, context)| {
                context
                    .fallback_names
                    .iter()
                    .map(|fallback| (fallback.clone(), protocol.clone()))
                    .collect::<HashMap<_, _>>()
            })
            .collect();

        ProtocolSet {
            rx,
            mgr_tx,
            protocols,
            next_substream_id,
            fallback_names,
            connection: ConnectionHandle::new(connection_id, tx),
        }
    }

    /// Try to acquire permit to keep the connection open.
    pub fn try_get_permit(&mut self) -> Option<Permit> {
        self.connection.try_get_permit()
    }

    /// Get next substream ID.
    #[cfg(any(feature = "quic", feature = "webrtc", feature = "websocket"))]
    pub fn next_substream_id(&self) -> SubstreamId {
        SubstreamId::from(self.next_substream_id.fetch_add(1usize, Ordering::Relaxed))
    }

    /// Get the list of all supported protocols.
    pub fn protocols(&self) -> Vec<ProtocolName> {
        self.protocols
            .keys()
            .cloned()
            .chain(self.fallback_names.keys().cloned())
            .collect()
    }

    /// Report to `protocol` that substream was opened for `peer`.
    pub async fn report_substream_open(
        &mut self,
        peer: PeerId,
        protocol: ProtocolName,
        direction: Direction,
        substream: Substream,
    ) -> Result<(), SubstreamError> {
        tracing::debug!(target: LOG_TARGET, %protocol, ?peer, ?direction, "substream opened");

        let (protocol, fallback) = match self.fallback_names.get(&protocol) {
            Some(main_protocol) => (main_protocol.clone(), Some(protocol)),
            None => (protocol, None),
        };

        let Some(protocol_context) = self.protocols.get(&protocol) else {
            return Err(NegotiationError::MultistreamSelectError(
                MultiStreamNegotiationError::ProtocolError(
                    MultiStreamProtocolError::ProtocolNotSupported,
                ),
            )
            .into());
        };

        let event = InnerTransportEvent::SubstreamOpened {
            peer,
            protocol: protocol.clone(),
            fallback,
            direction,
            substream,
            connection_id: *self.connection.connection_id(),
        };

        protocol_context
            .tx
            .send(event)
            .await
            .map_err(|_| SubstreamError::ConnectionClosed)
    }

    /// Get codec used by the protocol.
    pub fn protocol_codec(&self, protocol: &ProtocolName) -> ProtocolCodec {
        // NOTE: `protocol` must exist in `self.protocol` as it was negotiated
        // using the protocols from this set
        self.protocols
            .get(self.fallback_names.get(protocol).map_or(protocol, |protocol| protocol))
            .expect("protocol to exist")
            .codec
    }

    /// Report to `protocol` that connection failed to open substream for `peer`.
    pub async fn report_substream_open_failure(
        &mut self,
        protocol: ProtocolName,
        substream: SubstreamId,
        error: SubstreamError,
    ) -> crate::Result<()> {
        tracing::debug!(
            target: LOG_TARGET,
            %protocol,
            ?substream,
            ?error,
            "failed to open substream",
        );

        self.protocols
            .get_mut(&protocol)
            .ok_or(Error::ProtocolNotSupported(protocol.to_string()))?
            .tx
            .send(InnerTransportEvent::SubstreamOpenFailure { substream, error })
            .await
            .map_err(From::from)
    }

    /// Report to protocols that a connection was established.
    pub(crate) async fn report_connection_established(
        &mut self,
        peer: PeerId,
        endpoint: Endpoint,
    ) -> crate::Result<()> {
        let connection_handle = self.connection.downgrade();
        let mut futures = self
            .protocols
            .values()
            .map(|sender| {
                let endpoint = endpoint.clone();
                let connection_handle = connection_handle.clone();

                async move {
                    sender
                        .tx
                        .send(InnerTransportEvent::ConnectionEstablished {
                            peer,
                            connection: endpoint.connection_id(),
                            endpoint,
                            sender: connection_handle,
                        })
                        .await
                }
            })
            .collect::<FuturesUnordered<_>>();

        while !futures.is_empty() {
            if let Some(Err(error)) = futures.next().await {
                return Err(error.into());
            }
        }

        Ok(())
    }

    /// Report to protocols that a connection was closed.
    pub(crate) async fn report_connection_closed(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
    ) -> crate::Result<()> {
        let mut futures = self
            .protocols
            .values()
            .map(|sender| async move {
                sender
                    .tx
                    .send(InnerTransportEvent::ConnectionClosed {
                        peer,
                        connection: connection_id,
                    })
                    .await
            })
            .collect::<FuturesUnordered<_>>();

        while !futures.is_empty() {
            if let Some(Err(error)) = futures.next().await {
                return Err(error.into());
            }
        }

        self.mgr_tx
            .send(TransportManagerEvent::ConnectionClosed {
                peer,
                connection: connection_id,
            })
            .await
            .map_err(From::from)
    }
}

impl Stream for ProtocolSet {
    type Item = ProtocolCommand;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::substream::MockSubstream;
    use std::collections::HashSet;

    #[tokio::test]
    async fn fallback_is_provided() {
        let (tx, _rx) = channel(64);
        let (tx1, _rx1) = channel(64);

        let mut protocol_set = ProtocolSet::new(
            ConnectionId::from(0usize),
            tx,
            Default::default(),
            HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx1,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: vec![
                        ProtocolName::from("/notif/1/fallback/1"),
                        ProtocolName::from("/notif/1/fallback/2"),
                    ],
                },
            )]),
        );

        let expected_protocols = HashSet::from([
            ProtocolName::from("/notif/1"),
            ProtocolName::from("/notif/1/fallback/1"),
            ProtocolName::from("/notif/1/fallback/2"),
        ]);

        for protocol in protocol_set.protocols().iter() {
            assert!(expected_protocols.contains(protocol));
        }

        protocol_set
            .report_substream_open(
                PeerId::random(),
                ProtocolName::from("/notif/1/fallback/2"),
                Direction::Inbound,
                Substream::new_mock(
                    PeerId::random(),
                    SubstreamId::from(0usize),
                    Box::new(MockSubstream::new()),
                ),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn main_protocol_reported_if_main_protocol_negotiated() {
        let (tx, _rx) = channel(64);
        let (tx1, mut rx1) = channel(64);

        let mut protocol_set = ProtocolSet::new(
            ConnectionId::from(0usize),
            tx,
            Default::default(),
            HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx1,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: vec![
                        ProtocolName::from("/notif/1/fallback/1"),
                        ProtocolName::from("/notif/1/fallback/2"),
                    ],
                },
            )]),
        );

        protocol_set
            .report_substream_open(
                PeerId::random(),
                ProtocolName::from("/notif/1"),
                Direction::Inbound,
                Substream::new_mock(
                    PeerId::random(),
                    SubstreamId::from(0usize),
                    Box::new(MockSubstream::new()),
                ),
            )
            .await
            .unwrap();

        match rx1.recv().await.unwrap() {
            InnerTransportEvent::SubstreamOpened {
                protocol, fallback, ..
            } => {
                assert!(fallback.is_none());
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
            }
            _ => panic!("invalid event received"),
        }
    }

    #[tokio::test]
    async fn fallback_is_reported_to_protocol() {
        let (tx, _rx) = channel(64);
        let (tx1, mut rx1) = channel(64);

        let mut protocol_set = ProtocolSet::new(
            ConnectionId::from(0usize),
            tx,
            Default::default(),
            HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx1,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: vec![
                        ProtocolName::from("/notif/1/fallback/1"),
                        ProtocolName::from("/notif/1/fallback/2"),
                    ],
                },
            )]),
        );

        protocol_set
            .report_substream_open(
                PeerId::random(),
                ProtocolName::from("/notif/1/fallback/2"),
                Direction::Inbound,
                Substream::new_mock(
                    PeerId::random(),
                    SubstreamId::from(0usize),
                    Box::new(MockSubstream::new()),
                ),
            )
            .await
            .unwrap();

        match rx1.recv().await.unwrap() {
            InnerTransportEvent::SubstreamOpened {
                protocol, fallback, ..
            } => {
                assert_eq!(fallback, Some(ProtocolName::from("/notif/1/fallback/2")));
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
            }
            _ => panic!("invalid event received"),
        }
    }
}
