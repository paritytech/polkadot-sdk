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

//! Notification protocol implementation.

use crate::{
    error::{Error, SubstreamError},
    executor::Executor,
    protocol::{
        self,
        notification::{
            connection::Connection,
            handle::NotificationEventHandle,
            negotiation::{HandshakeEvent, HandshakeService},
        },
        TransportEvent, TransportService,
    },
    substream::Substream,
    types::{protocol::ProtocolName, SubstreamId},
    PeerId, DEFAULT_CHANNEL_SIZE,
};

use bytes::BytesMut;
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use multiaddr::Multiaddr;
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    oneshot,
};

use std::{collections::HashMap, sync::Arc, time::Duration};

pub use config::{Config, ConfigBuilder};
pub use handle::{NotificationHandle, NotificationSink};
pub use types::{
    Direction, NotificationCommand, NotificationError, NotificationEvent, ValidationResult,
};

mod config;
mod connection;
mod handle;
mod negotiation;
mod types;

#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::notification";

/// Connection state.
///
/// Used to track transport level connectivity state when there is a pending validation.
/// See [`PeerState::ValidationPending`] for more details.
#[derive(Debug, PartialEq, Eq)]
enum ConnectionState {
    /// There is a active, transport-level connection open to the peer.
    Open,

    /// There is no transport-level connection open to the peer.
    Closed,
}

/// Inbound substream state.
#[derive(Debug)]
enum InboundState {
    /// Substream is closed.
    Closed,

    /// Handshake is being read from the remote node.
    ReadingHandshake,

    /// Substream and its handshake are being validated by the user protocol.
    Validating {
        /// Inbound substream.
        inbound: Substream,
    },

    /// Handshake is being sent to the remote node.
    SendingHandshake,

    /// Substream is open.
    Open {
        /// Inbound substream.
        inbound: Substream,
    },
}

/// Outbound substream state.
#[derive(Debug)]
enum OutboundState {
    /// Substream is closed.
    Closed,

    /// Outbound substream initiated.
    OutboundInitiated {
        /// Substream ID.
        substream: SubstreamId,
    },

    /// Substream is in the state of being negotiated.
    ///
    /// This process entails sending local node's handshake and reading back the remote node's
    /// handshake if they've accepted the substream or detecting that the substream was closed
    /// in case the substream was rejected.
    Negotiating,

    /// Substream is open.
    Open {
        /// Received handshake.
        handshake: Vec<u8>,

        /// Outbound substream.
        outbound: Substream,
    },
}

impl OutboundState {
    /// Get pending outboud substream ID, if it exists.
    fn pending_open(&self) -> Option<SubstreamId> {
        match &self {
            OutboundState::OutboundInitiated { substream } => Some(*substream),
            _ => None,
        }
    }
}

#[derive(Debug)]
enum PeerState {
    /// Peer state is poisoned due to invalid state transition.
    Poisoned,

    /// Validation for an inbound substream is still pending.
    ///
    /// In order to enforce valid state transitions, `NotificationProtocol` keeps track of pending
    /// validations across connectivity events (open/closed) and enforces that no activity happens
    /// for any peer that is still awaiting validation for their inbound substream.
    ///
    /// If connection closes while the substream is being validated, instead of removing peer from
    /// `peers`, the peer state is set as `ValidationPending` which indicates to the state machine
    /// that a response for a inbound substream is pending validation. The substream itself will be
    /// dead by the time validation is received if the peer state is `ValidationPending` since the
    /// substream was part of a previous, now-closed substream but this state allows
    /// `NotificationProtocol` to enforce correct state transitions by, e.g., rejecting new inbound
    /// substream while a previous substream is still being validated or rejecting outbound
    /// substreams on new connections if that same condition holds.
    ValidationPending {
        /// What is current connectivity state of the peer.
        ///
        /// If `state` is `ConnectionState::Closed` when the validation is finally received, peer
        /// is removed from `peer` and if the `state` is `ConnectionState::Open`, peer is moved to
        /// state `PeerState::Closed` and user is allowed to retry opening an outbound substream.
        state: ConnectionState,
    },

    /// Connection to peer is closed.
    Closed {
        /// Connection might have been closed while there was an outbound substream still pending.
        ///
        /// To handle this state transition correctly in case the substream opens after the
        /// connection is considered closed, store the `SubstreamId` to that it can be verified in
        /// case the substream ever opens.
        pending_open: Option<SubstreamId>,
    },

    /// Peer is being dialed in order to open an outbound substream to them.
    Dialing,

    /// Outbound substream initiated.
    OutboundInitiated {
        /// Substream ID.
        substream: SubstreamId,
    },

    /// Substream is being validated.
    Validating {
        /// Protocol.
        protocol: ProtocolName,

        /// Fallback protocol, if the substream was negotiated using a fallback name.
        fallback: Option<ProtocolName>,

        /// Outbound protocol state.
        outbound: OutboundState,

        /// Inbound protocol state.
        inbound: InboundState,

        /// Direction.
        direction: Direction,
    },

    /// Notification stream has been opened.
    Open {
        /// `Oneshot::Sender` for shutting down the connection.
        shutdown: oneshot::Sender<()>,
    },
}

/// Peer context.
#[derive(Debug)]
struct PeerContext {
    /// Peer state.
    state: PeerState,
}

impl PeerContext {
    /// Create new [`PeerContext`].
    fn new() -> Self {
        Self {
            state: PeerState::Closed { pending_open: None },
        }
    }
}

pub(crate) struct NotificationProtocol {
    /// Transport service.
    service: TransportService,

    /// Protocol.
    protocol: ProtocolName,

    /// Auto accept inbound substream if the outbound substream was initiated by the local node.
    auto_accept: bool,

    /// TX channel passed to the protocol used for sending events.
    event_handle: NotificationEventHandle,

    /// TX channel for sending shut down notifications from connection handlers to
    /// [`NotificationProtocol`].
    shutdown_tx: Sender<PeerId>,

    /// RX channel for receiving shutdown notifications from the connection handlers.
    shutdown_rx: Receiver<PeerId>,

    /// RX channel passed to the protocol used for receiving commands.
    command_rx: Receiver<NotificationCommand>,

    /// TX channel given to connection handlers for sending notifications.
    notif_tx: Sender<(PeerId, BytesMut)>,

    /// Connected peers.
    peers: HashMap<PeerId, PeerContext>,

    /// Pending outbound substreams.
    pending_outbound: HashMap<SubstreamId, PeerId>,

    /// Handshaking service which reads and writes the handshakes to inbound
    /// and outbound substreams asynchronously.
    negotiation: HandshakeService,

    /// Synchronous channel size.
    sync_channel_size: usize,

    /// Asynchronous channel size.
    async_channel_size: usize,

    /// Executor for connection handlers.
    executor: Arc<dyn Executor>,

    /// Pending substream validations.
    pending_validations: FuturesUnordered<BoxFuture<'static, (PeerId, ValidationResult)>>,

    /// Timers for pending outbound substreams.
    timers: FuturesUnordered<BoxFuture<'static, PeerId>>,

    /// Should `NotificationProtocol` attempt to dial the peer.
    should_dial: bool,
}

impl NotificationProtocol {
    pub(crate) fn new(
        service: TransportService,
        config: Config,
        executor: Arc<dyn Executor>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = channel(DEFAULT_CHANNEL_SIZE);

        Self {
            service,
            shutdown_tx,
            shutdown_rx,
            executor,
            peers: HashMap::new(),
            protocol: config.protocol_name,
            auto_accept: config.auto_accept,
            pending_validations: FuturesUnordered::new(),
            timers: FuturesUnordered::new(),
            event_handle: NotificationEventHandle::new(config.event_tx),
            notif_tx: config.notif_tx,
            command_rx: config.command_rx,
            pending_outbound: HashMap::new(),
            negotiation: HandshakeService::new(config.handshake),
            sync_channel_size: config.sync_channel_size,
            async_channel_size: config.async_channel_size,
            should_dial: config.should_dial,
        }
    }

    /// Connection established to remote node.
    ///
    /// If the peer already exists, the only valid state for it is `Dialing` as it indicates that
    /// the user tried to open a substream to a peer who was not connected to local node.
    ///
    /// Any other state indicates that there's an error in the state transition logic.
    async fn on_connection_established(&mut self, peer: PeerId) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, protocol = %self.protocol, "connection established");

        let Some(context) = self.peers.get_mut(&peer) else {
            self.peers.insert(peer, PeerContext::new());
            return Ok(());
        };

        match std::mem::replace(&mut context.state, PeerState::Poisoned) {
            PeerState::Dialing => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    "dial succeeded, open substream to peer",
                );

                context.state = PeerState::Closed { pending_open: None };
                self.on_open_substream(peer).await
            }
            // connection established but validation is still pending
            //
            // update the connection state so that `NotificationProtocol` can proceed
            // to correct state after the validation result has beern received
            PeerState::ValidationPending { state } => {
                debug_assert_eq!(state, ConnectionState::Closed);

                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    "new connection established while validation still pending",
                );

                context.state = PeerState::ValidationPending {
                    state: ConnectionState::Open,
                };

                Ok(())
            }
            state => {
                tracing::error!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    ?state,
                    "state mismatch: peer already exists",
                );
                debug_assert!(false);
                Err(Error::PeerAlreadyExists(peer))
            }
        }
    }

    /// Connection closed to remote node.
    ///
    /// If the connection was considered open (both substreams were open), user is notified that
    /// the notification stream was closed.
    ///
    /// If the connection was still in progress (either substream was not fully open), the user is
    /// reported about it only if they had opened an outbound substream (outbound is either fully
    /// open, it had been initiated or the substream was under negotiation).
    async fn on_connection_closed(&mut self, peer: PeerId) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, protocol = %self.protocol, "connection closed");

        self.pending_outbound.retain(|_, p| p != &peer);

        let Some(context) = self.peers.remove(&peer) else {
            tracing::error!(
                target: LOG_TARGET,
                ?peer,
                protocol = %self.protocol,
                "state mismatch: peer doesn't exist",
            );
            debug_assert!(false);
            return Err(Error::PeerDoesntExist(peer));
        };

        // clean up all pending state for the peer
        self.negotiation.remove_outbound(&peer);
        self.negotiation.remove_inbound(&peer);

        match context.state {
            // outbound initiated, report open failure to peer
            PeerState::OutboundInitiated { .. } => {
                self.event_handle
                    .report_notification_stream_open_failure(peer, NotificationError::Rejected)
                    .await;
            }
            // substream fully open, report that the notification stream is closed
            PeerState::Open { shutdown } => {
                let _ = shutdown.send(());
            }
            // if the substream was being validated, user must be notified that the substream is
            // now considered rejected if they had been made aware of the existence of the pending
            // connection
            PeerState::Validating {
                outbound, inbound, ..
            } => {
                match (outbound, inbound) {
                    // substream was being validated by the protocol when the connection was closed
                    (OutboundState::Closed, InboundState::Validating { .. }) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            protocol = %self.protocol,
                            "connection closed while validation pending",
                        );

                        self.peers.insert(
                            peer,
                            PeerContext {
                                state: PeerState::ValidationPending {
                                    state: ConnectionState::Closed,
                                },
                            },
                        );
                    }
                    // user either initiated an outbound substream or an outbound substream was
                    // opened/being opened as a result of an accepted inbound substream but was not
                    // yet fully open
                    //
                    // to have consistent state tracking in the user protocol, substream rejection
                    // must be reported to the user
                    (
                        OutboundState::OutboundInitiated { .. }
                        | OutboundState::Negotiating
                        | OutboundState::Open { .. },
                        _,
                    ) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            protocol = %self.protocol,
                            "connection closed outbound substream under negotiation",
                        );

                        self.event_handle
                            .report_notification_stream_open_failure(
                                peer,
                                NotificationError::Rejected,
                            )
                            .await;
                    }
                    (_, _) => {}
                }
            }
            // pending validations must be tracked across connection open/close events
            PeerState::ValidationPending { .. } => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    "validation pending while connection closed",
                );

                self.peers.insert(
                    peer,
                    PeerContext {
                        state: PeerState::ValidationPending {
                            state: ConnectionState::Closed,
                        },
                    },
                );
            }
            _ => {}
        }

        Ok(())
    }

    /// Local node opened a substream to remote node.
    ///
    /// The connection can be in three different states:
    ///   - this is the first substream that was opened and thus the connection was initiated by the
    ///     local node
    ///   - this is a response to a previously received inbound substream which the local node
    ///     accepted and as a result, opened its own substream
    ///   - local and remote nodes opened substreams at the same time
    ///
    /// In the first case, the local node's handshake is sent to remote node and the substream is
    /// polled in the background until they either send their handshake or close the substream.
    ///
    /// For the second case, the connection was initiated by the remote node and the substream was
    /// accepted by the local node which initiated an outbound substream to the remote node.
    /// The only valid states for this case are [`InboundState::Open`],
    /// and [`InboundState::SendingHandshake`] as they imply
    /// that the inbound substream have been accepted by the local node and this opened outbound
    /// substream is a result of a valid state transition.
    ///
    /// For the third case, if the nodes have opened substreams at the same time, the outbound state
    /// must be [`OutboundState::OutboundInitiated`] to ascertain that the an outbound substream was
    /// actually opened. Any other state would be a state mismatch and would mean that the
    /// connection is opening substreams without the permission of the protocol handler.
    async fn on_outbound_substream(
        &mut self,
        protocol: ProtocolName,
        fallback: Option<ProtocolName>,
        peer: PeerId,
        substream_id: SubstreamId,
        outbound: Substream,
    ) -> crate::Result<()> {
        tracing::debug!(
            target: LOG_TARGET,
            ?peer,
            ?protocol,
            ?substream_id,
            "handle outbound substream",
        );

        // peer must exist since an outbound substream was received from them
        let Some(context) = self.peers.get_mut(&peer) else {
            tracing::error!(target: LOG_TARGET, ?peer, "peer doesn't exist for outbound substream");
            debug_assert!(false);
            return Err(Error::PeerDoesntExist(peer));
        };

        let pending_peer = self.pending_outbound.remove(&substream_id);

        match std::mem::replace(&mut context.state, PeerState::Poisoned) {
            // the connection was initiated by the local node, send handshake to remote and wait to
            // receive their handshake back
            PeerState::OutboundInitiated { substream } => {
                debug_assert!(substream == substream_id);
                debug_assert!(pending_peer == Some(peer));

                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    ?fallback,
                    ?substream_id,
                    "negotiate outbound protocol",
                );

                self.negotiation.negotiate_outbound(peer, outbound);
                context.state = PeerState::Validating {
                    protocol,
                    fallback,
                    inbound: InboundState::Closed,
                    outbound: OutboundState::Negotiating,
                    direction: Direction::Outbound,
                };
            }
            PeerState::Validating {
                protocol,
                fallback,
                inbound,
                direction,
                outbound: outbound_state,
            } => {
                // the inbound substream has been accepted by the local node since the handshake has
                // been read and the local handshake has either already been sent or
                // it's in the process of being sent.
                match inbound {
                    InboundState::SendingHandshake | InboundState::Open { .. } => {
                        context.state = PeerState::Validating {
                            protocol,
                            fallback,
                            inbound,
                            direction,
                            outbound: OutboundState::Negotiating,
                        };
                        self.negotiation.negotiate_outbound(peer, outbound);
                    }
                    // nodes have opened substreams at the same time
                    inbound_state => match outbound_state {
                        OutboundState::OutboundInitiated { substream } => {
                            debug_assert!(substream == substream_id);

                            context.state = PeerState::Validating {
                                protocol,
                                fallback,
                                direction,
                                inbound: inbound_state,
                                outbound: OutboundState::Negotiating,
                            };
                            self.negotiation.negotiate_outbound(peer, outbound);
                        }
                        // invalid state: more than one outbound substream has been opened
                        inner_state => {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?peer,
                                %protocol,
                                ?substream_id,
                                ?inbound_state,
                                ?inner_state,
                                "invalid state, expected `OutboundInitiated`",
                            );

                            let _ = outbound.close().await;
                            debug_assert!(false);
                        }
                    },
                }
            }
            // the connection may have been closed while an outbound substream was pending
            // if the outbound substream was initiated successfully, close it and reset
            // `pending_open`
            PeerState::Closed { pending_open } if pending_open == Some(substream_id) => {
                let _ = outbound.close().await;

                context.state = PeerState::Closed { pending_open: None };
            }
            state => {
                tracing::error!(
                    target: LOG_TARGET,
                    ?peer,
                    %protocol,
                    ?substream_id,
                    ?state,
                    "invalid state: more than one outbound substream opened",
                );

                let _ = outbound.close().await;
                debug_assert!(false);
            }
        }

        Ok(())
    }

    /// Remote opened a substream to local node.
    ///
    /// The peer can be in four different states for the inbound substream to be considered valid:
    ///   - the connection is closed
    ///   - conneection is open but substream validation from a previous connection is still pending
    ///   - outbound substream has been opened but not yet acknowledged by the remote peer
    ///   - outbound substream has been opened and acknowledged by the remote peer and it's being
    ///     negotiated
    ///
    /// If remote opened more than one substream, the new substream is simply discarded.
    async fn on_inbound_substream(
        &mut self,
        protocol: ProtocolName,
        fallback: Option<ProtocolName>,
        peer: PeerId,
        substream: Substream,
    ) -> crate::Result<()> {
        // peer must exist since an inbound substream was received from them
        let Some(context) = self.peers.get_mut(&peer) else {
            tracing::error!(target: LOG_TARGET, ?peer, "peer doesn't exist for inbound substream");
            debug_assert!(false);
            return Err(Error::PeerDoesntExist(peer));
        };

        tracing::debug!(
            target: LOG_TARGET,
            ?peer,
            %protocol,
            ?fallback,
            state = ?context.state,
            "handle inbound substream",
        );

        match std::mem::replace(&mut context.state, PeerState::Poisoned) {
            // inbound substream of a previous connection is still pending validation,
            // reject any new inbound substreams until an answer is heard from the user
            state @ PeerState::ValidationPending { .. } => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    %protocol,
                    ?fallback,
                    ?state,
                    "validation for previous substream still pending",
                );

                let _ = substream.close().await;
                context.state = state;
            }
            // outbound substream for previous connection still pending, reject inbound substream
            // and wait for the outbound substream state to conclude as either succeeded or failed
            // before accepting any inbound substreams.
            PeerState::Closed {
                pending_open: Some(substream_id),
            } => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    "received inbound substream while outbound substream opening, rejecting",
                );
                let _ = substream.close().await;

                context.state = PeerState::Closed {
                    pending_open: Some(substream_id),
                };
            }
            // the peer state is closed so this is a fresh inbound substream.
            PeerState::Closed { pending_open: None } => {
                self.negotiation.read_handshake(peer, substream);

                context.state = PeerState::Validating {
                    protocol,
                    fallback,
                    direction: Direction::Inbound,
                    inbound: InboundState::ReadingHandshake,
                    outbound: OutboundState::Closed,
                };
            }
            // if the connection is under validation (so an outbound substream has been opened and
            // it's still pending or under negotiation), the only valid state for the
            // inbound state is closed as it indicates that there isn't an inbound substream yet for
            // the remote node duplicate substreams are prohibited.
            PeerState::Validating {
                protocol,
                fallback,
                outbound,
                direction,
                inbound: InboundState::Closed,
            } => {
                self.negotiation.read_handshake(peer, substream);

                context.state = PeerState::Validating {
                    protocol,
                    fallback,
                    outbound,
                    direction,
                    inbound: InboundState::ReadingHandshake,
                };
            }
            // outbound substream may have been initiated by the local node while a remote node also
            // opened a substream roughly at the same time
            PeerState::OutboundInitiated {
                substream: outbound,
            } => {
                self.negotiation.read_handshake(peer, substream);

                context.state = PeerState::Validating {
                    protocol,
                    fallback,
                    direction: Direction::Outbound,
                    outbound: OutboundState::OutboundInitiated {
                        substream: outbound,
                    },
                    inbound: InboundState::ReadingHandshake,
                };
            }
            // new inbound substream opend while validation for the previous substream was still
            // pending
            //
            // the old substream can be considered dead because remote wouldn't open a new substream
            // to us unless they had discarded the previous substream.
            //
            // set peer state to `ValidationPending` to indicate that the peer is "blocked" until a
            // validation for the substream is heard, blocking any further activity for
            // the connection and once the validation is received and in case the
            // substream is accepted, it will be reported as open failure to to the peer
            // because the states have gone out of sync.
            PeerState::Validating {
                outbound: OutboundState::Closed,
                inbound:
                    InboundState::Validating {
                        inbound: pending_substream,
                    },
                ..
            } => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    "remote opened substream while previous was still pending, connection failed",
                );
                let _ = substream.close().await;
                let _ = pending_substream.close().await;

                context.state = PeerState::ValidationPending {
                    state: ConnectionState::Open,
                };
            }
            // remote opened another inbound substream, close it and otherwise ignore the event
            // as this is a non-serious protocol violation.
            state => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    %protocol,
                    ?fallback,
                    ?state,
                    "remote opened more than one inbound substreams, discarding",
                );

                let _ = substream.close().await;
                context.state = state;
            }
        }

        Ok(())
    }

    /// Failed to open substream to remote node.
    ///
    /// If the substream was initiated by the local node, it must be reported that the substream
    /// failed to open. Otherwise the peer state can silently be converted to `Closed`.
    async fn on_substream_open_failure(
        &mut self,
        substream_id: SubstreamId,
        error: SubstreamError,
    ) {
        tracing::debug!(
            target: LOG_TARGET,
            protocol = %self.protocol,
            ?substream_id,
            ?error,
            "failed to open substream"
        );

        let Some(peer) = self.pending_outbound.remove(&substream_id) else {
            tracing::warn!(
                target: LOG_TARGET,
                protocol = %self.protocol,
                ?substream_id,
                "pending outbound substream doesn't exist",
            );
            debug_assert!(false);
            return;
        };

        // peer must exist since an outbound substream failure was received from them
        let Some(context) = self.peers.get_mut(&peer) else {
            tracing::warn!(target: LOG_TARGET, ?peer, "peer doesn't exist");
            debug_assert!(false);
            return;
        };

        match &mut context.state {
            PeerState::OutboundInitiated { .. } => {
                context.state = PeerState::Closed { pending_open: None };

                self.event_handle
                    .report_notification_stream_open_failure(peer, NotificationError::Rejected)
                    .await;
            }
            // if the substream was accepted by the local node and as a result, an outbound
            // substream was accepted as a result this should not be reported to local node
            PeerState::Validating { outbound, .. } => {
                self.negotiation.remove_inbound(&peer);
                self.negotiation.remove_outbound(&peer);

                let pending_open = match outbound {
                    OutboundState::Closed => None,
                    OutboundState::OutboundInitiated { substream } => {
                        self.event_handle
                            .report_notification_stream_open_failure(
                                peer,
                                NotificationError::Rejected,
                            )
                            .await;

                        Some(*substream)
                    }
                    OutboundState::Negotiating | OutboundState::Open { .. } => {
                        self.event_handle
                            .report_notification_stream_open_failure(
                                peer,
                                NotificationError::Rejected,
                            )
                            .await;

                        None
                    }
                };

                context.state = PeerState::Closed { pending_open };
            }
            PeerState::Closed { pending_open } => {
                tracing::debug!(
                    target: LOG_TARGET,
                    protocol = %self.protocol,
                    ?substream_id,
                    "substream open failure for a closed connection",
                );
                debug_assert_eq!(pending_open, &Some(substream_id));
                context.state = PeerState::Closed { pending_open: None };
            }
            state => {
                tracing::warn!(
                    target: LOG_TARGET,
                    protocol = %self.protocol,
                    ?substream_id,
                    ?state,
                    "invalid state for outbound substream open failure",
                );
                context.state = PeerState::Closed { pending_open: None };
                debug_assert!(false);
            }
        }
    }

    /// Open substream to remote `peer`.
    ///
    /// Outbound substream can opened only if the `PeerState` is `Closed`.
    /// By forcing the substream to be opened only if the state is currently closed,
    /// `NotificationProtocol` can enfore more predictable state transitions.
    ///
    /// Other states either imply an invalid state transition ([`PeerState::Open`]) or that an
    /// inbound substream has already been received and its currently being validated by the user.
    async fn on_open_substream(&mut self, peer: PeerId) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, protocol = %self.protocol, "open substream");

        let Some(context) = self.peers.get_mut(&peer) else {
            if !self.should_dial {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    "connection to peer not open and dialing disabled",
                );

                self.event_handle
                    .report_notification_stream_open_failure(peer, NotificationError::DialFailure)
                    .await;
                return Ok(());
            }

            match self.service.dial(&peer) {
                Err(error) => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        ?error,
                        "failed to dial peer",
                    );

                    self.event_handle
                        .report_notification_stream_open_failure(
                            peer,
                            NotificationError::DialFailure,
                        )
                        .await;

                    return Err(error.into());
                }
                Ok(()) => {
                    tracing::trace!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        "started to dial peer",
                    );

                    self.peers.insert(
                        peer,
                        PeerContext {
                            state: PeerState::Dialing,
                        },
                    );
                    return Ok(());
                }
            }
        };

        match context.state {
            // protocol can only request a new outbound substream to be opened if the state is
            // `Closed` other states imply that it's already open
            PeerState::Closed {
                pending_open: Some(substream_id),
            } => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    ?substream_id,
                    "outbound substream opening, reusing pending open substream",
                );

                self.pending_outbound.insert(substream_id, peer);
                context.state = PeerState::OutboundInitiated {
                    substream: substream_id,
                };
            }
            PeerState::Closed { .. } => match self.service.open_substream(peer) {
                Ok(substream_id) => {
                    tracing::trace!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        ?substream_id,
                        "outbound substream opening",
                    );

                    self.pending_outbound.insert(substream_id, peer);
                    context.state = PeerState::OutboundInitiated {
                        substream: substream_id,
                    };
                }
                Err(error) => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        ?error,
                        "failed to open substream",
                    );

                    self.event_handle
                        .report_notification_stream_open_failure(
                            peer,
                            NotificationError::NoConnection,
                        )
                        .await;
                    context.state = PeerState::Closed { pending_open: None };
                }
            },
            // while a validation is pending for an inbound substream, user is not allowed to open
            // any outbound substreams until the old inbond substream is either accepted or rejected
            PeerState::ValidationPending { .. } => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    "validation still pending, rejecting outbound substream request",
                );

                self.event_handle
                    .report_notification_stream_open_failure(
                        peer,
                        NotificationError::ValidationPending,
                    )
                    .await;
            }
            _ => {}
        }

        Ok(())
    }

    /// Close substream to remote `peer`.
    ///
    /// This function can only be called if the substream was actually open, any other state is
    /// unreachable as the user is unable to emit this command to [`NotificationProtocol`] unless
    /// the connection has been fully opened.
    async fn on_close_substream(&mut self, peer: PeerId) {
        tracing::debug!(target: LOG_TARGET, ?peer, protocol = %self.protocol, "close substream");

        let Some(context) = self.peers.get_mut(&peer) else {
            tracing::debug!(target: LOG_TARGET, ?peer, "peer doesn't exist");
            return;
        };

        match std::mem::replace(&mut context.state, PeerState::Poisoned) {
            PeerState::Open { shutdown } => {
                let _ = shutdown.send(());

                context.state = PeerState::Closed { pending_open: None };
            }
            state => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    ?state,
                    "substream already closed",
                );
                context.state = state;
            }
        }
    }

    /// Handle validation result.
    ///
    /// The validation result binary (accept/reject). If the node is rejected, the substreams are
    /// discarded and state is set to `PeerState::Closed`. If there was an outbound substream in
    /// progress while the connection was rejected by the user, the oubound state is discarded,
    /// except for the substream ID of the substream which is kept for later use, in case the
    /// substream happens to open.
    ///
    /// If the node is accepted and there is no outbound substream to them open yet, a new substream
    /// is opened and once it opens, the local handshake will be sent to the remote peer and if
    /// they also accept the substream the connection is considered fully open.
    async fn on_validation_result(
        &mut self,
        peer: PeerId,
        result: ValidationResult,
    ) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            protocol = %self.protocol,
            ?result,
            "handle validation result",
        );

        let Some(context) = self.peers.get_mut(&peer) else {
            tracing::debug!(target: LOG_TARGET, ?peer, "peer doesn't exist");
            return Err(Error::PeerDoesntExist(peer));
        };

        match std::mem::replace(&mut context.state, PeerState::Poisoned) {
            PeerState::Validating {
                protocol,
                fallback,
                outbound,
                direction,
                inbound: InboundState::Validating { inbound },
            } => match result {
                // substream was rejected by the local node, if an outbound substream was under
                // negotation, discard that data and if an outbound substream was
                // initiated, save the `SubstreamId` of that substream and later if the substream
                // is opened, the state can be corrected to `pending_open: None`.
                ValidationResult::Reject => {
                    let _ = inbound.close().await;
                    self.negotiation.remove_outbound(&peer);
                    self.negotiation.remove_inbound(&peer);
                    context.state = PeerState::Closed {
                        pending_open: outbound.pending_open(),
                    };

                    Ok(())
                }
                ValidationResult::Accept => match outbound {
                    // no outbound substream exists so initiate a new substream open and send the
                    // local handshake to remote node, indicating that the
                    // connection was accepted by the local node
                    OutboundState::Closed => match self.service.open_substream(peer) {
                        Ok(substream) => {
                            self.negotiation.send_handshake(peer, inbound);
                            self.pending_outbound.insert(substream, peer);

                            context.state = PeerState::Validating {
                                protocol,
                                fallback,
                                direction,
                                inbound: InboundState::SendingHandshake,
                                outbound: OutboundState::OutboundInitiated { substream },
                            };
                            Ok(())
                        }
                        // failed to open outbound substream after accepting an inbound substream
                        //
                        // since the user was notified of this substream and they accepted it,
                        // they expecting some kind of answer (open success/failure).
                        //
                        // report to user that the substream failed to open so they can track the
                        // state transitions of the peer correctly
                        Err(error) => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                ?peer,
                                protocol = %self.protocol,
                                ?result,
                                ?error,
                                "failed to open outbound substream for accepted substream",
                            );

                            let _ = inbound.close().await;
                            context.state = PeerState::Closed { pending_open: None };

                            self.event_handle
                                .report_notification_stream_open_failure(
                                    peer,
                                    NotificationError::Rejected,
                                )
                                .await;

                            Err(error.into())
                        }
                    },
                    // here the state is one of `OutboundState::{OutboundInitiated, Negotiating,
                    // Open}` so that state can be safely ignored and all that
                    // has to be done is to send the local handshake to remote
                    // node to indicate that the connection was accepted.
                    _ => {
                        self.negotiation.send_handshake(peer, inbound);

                        context.state = PeerState::Validating {
                            protocol,
                            fallback,
                            direction,
                            inbound: InboundState::SendingHandshake,
                            outbound,
                        };
                        Ok(())
                    }
                },
            },
            // validation result received for an inbound substream which is now considered dead
            // because while the substream was being validated, the connection had closed.
            //
            // if the substream was rejected and there is no active connection to the peer,
            // just remove the peer from `peers` without informing user
            //
            // if the substream was accepted, the user must be informed that the substream failed to
            // open. Depending on whether there is currently a connection open to the peer, either
            // report `Rejected`/`NoConnection` and let the user try again.
            PeerState::ValidationPending { state } => {
                if let Some(error) = match state {
                    ConnectionState::Open => {
                        context.state = PeerState::Closed { pending_open: None };

                        std::matches!(result, ValidationResult::Accept)
                            .then_some(NotificationError::Rejected)
                    }
                    ConnectionState::Closed => {
                        self.peers.remove(&peer);

                        std::matches!(result, ValidationResult::Accept)
                            .then_some(NotificationError::NoConnection)
                    }
                } {
                    self.event_handle.report_notification_stream_open_failure(peer, error).await;
                }

                Ok(())
            }
            // if the user incorrectly send a validation result for a peer that doesn't require
            // validation, set state back to what it was and ignore the event
            //
            // the user protocol may send a stale validation result not because of a programming
            // error but because it has a backlock of unhandled events, with one event potentially
            // nullifying the need for substream validation, and is just temporarily out of sync
            // with `NotificationProtocol`
            state => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    ?state,
                    "validation result received for peer that doesn't require validation",
                );

                context.state = state;
                Ok(())
            }
        }
    }

    /// Handle handshake event.
    ///
    /// There are three different handshake event types:
    ///   - outbound substream negotiated
    ///   - inbound substream negotiated
    ///   - substream negotiation error
    ///
    /// Neither outbound nor inbound substream negotiated automatically means that the connection is
    /// considered open as both substreams must be fully negotiated for that to be the case. That is
    /// why the peer state for inbound and outbound are set separately and at the end of the
    /// function is the collective state of the substreams checked and if both substreams are
    /// negotiated, the user informed that the connection is open.
    ///
    /// If the negotiation fails, the user may have to be informed of that. Outbound substream
    /// failure always results in user getting notified since the existence of an outbound substream
    /// means that the user has either initiated an outbound substreams or has accepted an inbound
    /// substreams, resulting in an outbound substreams.
    ///
    /// Negotiation failure for inbound substreams which are in the state
    /// [`InboundState::ReadingHandshake`] don't result in any notification because while the
    /// handshake is being read from the substream, the user is oblivious to the fact that an
    /// inbound substream has even been received.
    async fn on_handshake_event(&mut self, peer: PeerId, event: HandshakeEvent) {
        let Some(context) = self.peers.get_mut(&peer) else {
            tracing::error!(target: LOG_TARGET, "invalid state: negotiation event received but peer doesn't exist");
            debug_assert!(false);
            return;
        };

        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            protocol = %self.protocol,
            ?event,
            "handle handshake event",
        );

        match event {
            // either an inbound or outbound substream has been negotiated successfully
            HandshakeEvent::Negotiated {
                peer,
                handshake,
                substream,
                direction,
            } => match direction {
                // outbound substream was negotiated, the only valid state for peer is `Validating`
                // and only valid state for `OutboundState` is `Negotiating`
                negotiation::Direction::Outbound => {
                    self.negotiation.remove_outbound(&peer);

                    match std::mem::replace(&mut context.state, PeerState::Poisoned) {
                        PeerState::Validating {
                            protocol,
                            fallback,
                            direction,
                            outbound: OutboundState::Negotiating,
                            inbound,
                        } => {
                            context.state = PeerState::Validating {
                                protocol,
                                fallback,
                                direction,
                                outbound: OutboundState::Open {
                                    handshake,
                                    outbound: substream,
                                },
                                inbound,
                            };
                        }
                        state => {
                            tracing::warn!(
                                target: LOG_TARGET,
                                ?peer,
                                ?state,
                                "outbound substream negotiated but peer has invalid state",
                            );
                            debug_assert!(false);
                        }
                    }
                }
                // inbound negotiation event completed
                //
                // the negotiation event can be on of two different types:
                //   - remote handshake was read from the substream
                //   - local handshake has been sent to remote node
                //
                // For the first case, the substream has to be validated by the local node.
                // This means reporting the protocol name, potential negotiated fallback and the
                // handshake. Local node will then either accept or reject the substream which is
                // handled by [`NotificationProtocol::on_validation_result()`]. Compared to
                // Substrate, litep2p requires both peers to validate the inbound handshake to allow
                // more complex connection validation. If this is not necessary and the protocol
                // wishes to auto-accept the inbound substreams that are a result of
                // an outbound substream already accepted by the remote node, the
                // substream validation is skipped and the local handshake is sent
                // right away.
                //
                // For the second case, the local handshake was sent to remote node successfully and
                // the inbound substream is considered open and if the outbound
                // substream is open as well, the connection is fully open.
                //
                // Only valid states for [`InboundState`] are [`InboundState::ReadingHandshake`] and
                // [`InboundState::SendingHandshake`] because otherwise the inbound
                // substream cannot be in [`HandshakeService`](super::negotiation::HandshakeService)
                // unless there is a logic bug in the state machine.
                negotiation::Direction::Inbound => {
                    self.negotiation.remove_inbound(&peer);

                    match std::mem::replace(&mut context.state, PeerState::Poisoned) {
                        PeerState::Validating {
                            protocol,
                            fallback,
                            direction,
                            outbound,
                            inbound: InboundState::ReadingHandshake,
                        } => {
                            if !std::matches!(outbound, OutboundState::Closed) && self.auto_accept {
                                tracing::trace!(
                                    target: LOG_TARGET,
                                    ?peer,
                                    %protocol,
                                    ?fallback,
                                    ?direction,
                                    ?outbound,
                                    "auto-accept inbound substream",
                                );

                                self.negotiation.send_handshake(peer, substream);
                                context.state = PeerState::Validating {
                                    protocol,
                                    fallback,
                                    direction,
                                    inbound: InboundState::SendingHandshake,
                                    outbound,
                                };

                                return;
                            }

                            tracing::trace!(
                                target: LOG_TARGET,
                                ?peer,
                                %protocol,
                                ?fallback,
                                ?outbound,
                                "send inbound protocol for validation",
                            );

                            context.state = PeerState::Validating {
                                protocol: protocol.clone(),
                                fallback: fallback.clone(),
                                inbound: InboundState::Validating { inbound: substream },
                                outbound,
                                direction,
                            };

                            let (tx, rx) = oneshot::channel();
                            self.pending_validations.push(Box::pin(async move {
                                match rx.await {
                                    Ok(ValidationResult::Accept) =>
                                        (peer, ValidationResult::Accept),
                                    _ => (peer, ValidationResult::Reject),
                                }
                            }));

                            self.event_handle
                                .report_inbound_substream(protocol, fallback, peer, handshake, tx)
                                .await;
                        }
                        PeerState::Validating {
                            protocol,
                            fallback,
                            direction,
                            inbound: InboundState::SendingHandshake,
                            outbound,
                        } => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                ?peer,
                                %protocol,
                                ?fallback,
                                "inbound substream negotiated, waiting for outbound substream to complete",
                            );

                            context.state = PeerState::Validating {
                                protocol: protocol.clone(),
                                fallback: fallback.clone(),
                                inbound: InboundState::Open { inbound: substream },
                                outbound,
                                direction,
                            };
                        }
                        _state => debug_assert!(false),
                    }
                }
            },
            // error occurred during negotiation, eitehr for inbound or outbound substream
            // user is notified of the error only if they've either initiated an outbound substream
            // or if they accepted an inbound substream and as a result initiated an outbound
            // substream.
            HandshakeEvent::NegotiationError { peer, direction } => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    ?direction,
                    state = ?context.state,
                    "failed to negotiate substream",
                );
                let _ = self.negotiation.remove_outbound(&peer);
                let _ = self.negotiation.remove_inbound(&peer);

                // if an outbound substream had been initiated (whatever its state is), it means
                // that the user knows about the connection and must be notified that it failed to
                // negotiate.
                match std::mem::replace(&mut context.state, PeerState::Poisoned) {
                    PeerState::Validating { outbound, .. } => {
                        context.state = PeerState::Closed {
                            pending_open: outbound.pending_open(),
                        };

                        // notify user if the outbound substream is not considered closed
                        if !std::matches!(outbound, OutboundState::Closed) {
                            return self
                                .event_handle
                                .report_notification_stream_open_failure(
                                    peer,
                                    NotificationError::Rejected,
                                )
                                .await;
                        }
                    }
                    _state => debug_assert!(false),
                }
            }
        }

        // if both inbound and outbound substreams are considered open, notify the user that
        // a notification stream has been opened and set up for sending and receiving
        // notifications to and from remote node
        match std::mem::replace(&mut context.state, PeerState::Poisoned) {
            PeerState::Validating {
                protocol,
                fallback,
                direction,
                outbound:
                    OutboundState::Open {
                        handshake,
                        outbound,
                    },
                inbound: InboundState::Open { inbound },
            } => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    %protocol,
                    ?fallback,
                    "notification stream opened",
                );

                let (async_tx, async_rx) = channel(self.async_channel_size);
                let (sync_tx, sync_rx) = channel(self.sync_channel_size);
                let sink = NotificationSink::new(peer, sync_tx, async_tx);

                // start connection handler for the peer which only deals with sending/receiving
                // notifications
                //
                // the connection handler must be started only after the newly opened notification
                // substream is reported to user because the connection handler
                // might exit immediately after being started if remote closed the connection.
                //
                // if the order of events (open & close) is not ensured to be correct, the code
                // handling the connectivity logic on the `NotificationHandle` side
                // might get confused about the current state of the connection.
                let shutdown_tx = self.shutdown_tx.clone();
                let (connection, shutdown) = Connection::new(
                    peer,
                    inbound,
                    outbound,
                    self.event_handle.clone(),
                    shutdown_tx.clone(),
                    self.notif_tx.clone(),
                    async_rx,
                    sync_rx,
                );

                context.state = PeerState::Open { shutdown };
                self.event_handle
                    .report_notification_stream_opened(
                        protocol, fallback, direction, peer, handshake, sink,
                    )
                    .await;

                self.executor.run(Box::pin(async move {
                    connection.start().await;
                }));
            }
            state => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    ?state,
                    "validation for substream still pending",
                );
                self.timers.push(Box::pin(async move {
                    futures_timer::Delay::new(Duration::from_secs(5)).await;
                    peer
                }));

                context.state = state;
            }
        }
    }

    /// Handle dial failure.
    async fn on_dial_failure(&mut self, peer: PeerId, addresses: Vec<Multiaddr>) {
        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            protocol = %self.protocol,
            ?addresses,
            "handle dial failure",
        );

        let Some(context) = self.peers.remove(&peer) else {
            tracing::trace!(
                target: LOG_TARGET,
                ?peer,
                protocol = %self.protocol,
                ?addresses,
                "dial failure for an unknown peer",
            );
            return;
        };

        match context.state {
            PeerState::Dialing => {
                tracing::debug!(target: LOG_TARGET, ?peer, protocol = %self.protocol, ?addresses, "failed to dial peer");
                self.event_handle
                    .report_notification_stream_open_failure(peer, NotificationError::DialFailure)
                    .await;
            }
            state => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    ?state,
                    "dial failure for peer that's not being dialed",
                );
                self.peers.insert(peer, PeerContext { state });
            }
        }
    }

    /// Handle next notification event.
    ///
    /// Returns `true` when the user command stream was dropped.
    async fn next_event(&mut self) -> bool {
        // biased select is used because the substream events must be prioritized above other events
        // that is because a closed substream is detected by either `substreams` or `negotiation`
        // and if that event is not handled with priority but, e.g., inbound substream is
        // handled before, it can create a situation where the state machine gets confused
        // about the peer's state.
        tokio::select! {
            biased;

            event = self.negotiation.next(), if !self.negotiation.is_empty() => {
                if let Some((peer, event)) = event {
                    self.on_handshake_event(peer, event).await;
                } else {
                    tracing::error!(target: LOG_TARGET, "`HandshakeService` expected to return `Some(..)`");
                    debug_assert!(false);
                };
            }
            event = self.shutdown_rx.recv() => match event {
                None => (),
                Some(peer) => {
                    if let Some(context) = self.peers.get_mut(&peer) {
                        tracing::trace!(
                            target: LOG_TARGET,
                            ?peer,
                            protocol = %self.protocol,
                            "notification stream to peer closed",
                        );
                        context.state = PeerState::Closed { pending_open: None };
                    }
                }
            },
            // TODO: https://github.com/paritytech/litep2p/issues/338 this could be combined with `Negotiation`
            peer = self.timers.next(), if !self.timers.is_empty() => match peer {
                Some(peer) => {
                    match self.peers.get_mut(&peer) {
                        Some(context) => match std::mem::replace(&mut context.state, PeerState::Poisoned) {
                            PeerState::Validating {
                                outbound: OutboundState::Open { outbound, .. },
                                inbound: InboundState::Closed,
                                ..
                            } => {
                                tracing::debug!(
                                    target: LOG_TARGET,
                                    ?peer,
                                    protocol = %self.protocol,
                                    "peer didn't answer in 10 seconds, canceling substream and closing connection",
                                );
                                context.state = PeerState::Closed { pending_open: None };

                                let _ = outbound.close().await;
                                self.event_handle
                                    .report_notification_stream_open_failure(peer, NotificationError::Rejected)
                                    .await;

                                // NOTE: this is used to work around an issue in Substrate where the protocol
                                // is not notified if an inbound substream is closed. That indicates that remote
                                // wishes the close the connection but `Notifications` still keeps the substream state
                                // as `Open` until the outbound substream is closed (even though the outbound substream
                                // is also closed at that point). This causes a further issue: inbound substreams
                                // are automatically opened when state is `Open`, even if the inbound substream belongs
                                // to a new "connection" (pair of substreams).
                                //
                                // basically what happens (from Substrate's PoV) is there are pair of substreams (`inbound1`, `outbound1`),
                                // litep2p closes both substreams so both `inbound1` and outbound1 become non-readable/writable.
                                // Substrate doesn't detect this an instead only marks `inbound1` is closed while still keeping
                                // the (now-closed) `outbound1` active and it will be detected closed only when Substrate tries to
                                // write something into that substream. If now litep2p tries to open a new connection to Substrate,
                                // the outbound substream from litep2p's PoV will be automatically accepted (https://github.com/paritytech/polkadot-sdk/blob/59b2661444de2a25f2125a831bd786035a9fac4b/substrate/client/network/src/protocol/notifications/handler.rs#L544-L556)
                                // but since Substrate thinks `outbound1` is still active, it won't open a new outbound substream
                                // and it ends up having (`inbound2`, `outbound1`) as its pair of substreams which doens't make sense.
                                //
                                // since litep2p is expecting to receive an inbound substream from Substrate and never receives it,
                                // it basically can't make progress with the substream open request because litep2p can't force Substrate
                                // to detect that `outbound1` is closed. Easiest (and very hacky at the same time) way to reset the substream
                                // state is to close the connection. This is not an appropriate way to fix the issue and causes issues with,
                                // e.g., smoldot which at the time of writing this doesn't support the transaction protocol. The only way to fix
                                // this cleanly is to make Substrate detect closed substreams correctly.
                                if let Err(error) = self.service.force_close(peer) {
                                    tracing::debug!(
                                        target: LOG_TARGET,
                                        ?peer,
                                        protocol = %self.protocol,
                                        ?error,
                                        "failed to force close connection",
                                    );
                                }
                            }
                            state => {
                                tracing::trace!(
                                    target: LOG_TARGET,
                                    ?peer,
                                    protocol = %self.protocol,
                                    ?state,
                                    "ignore expired timer for peer",
                                );
                                context.state = state;
                            }
                        }
                        None => tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            protocol = %self.protocol,
                            "peer doesn't exist anymore",
                        ),
                    }
                }
                None => (),
            },
            event = self.service.next() => match event {
                Some(TransportEvent::ConnectionEstablished { peer, .. }) => {
                    if let Err(error) = self.on_connection_established(peer).await {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            ?error,
                            "failed to register peer",
                        );
                    }
                }
                Some(TransportEvent::ConnectionClosed { peer }) => {
                    if let Err(error) = self.on_connection_closed(peer).await {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            ?error,
                            "failed to disconnect peer",
                        );
                    }
                }
                Some(TransportEvent::SubstreamOpened {
                    peer,
                    substream,
                    direction,
                    protocol,
                    fallback,
                }) => match direction {
                    protocol::Direction::Inbound => {
                        if let Err(error) = self.on_inbound_substream(protocol, fallback, peer, substream).await {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?peer,
                                ?error,
                                "failed to handle inbound substream",
                            );
                        }
                    }
                    protocol::Direction::Outbound(substream_id) => {
                        if let Err(error) = self
                            .on_outbound_substream(protocol, fallback, peer, substream_id, substream)
                            .await
                        {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?peer,
                                ?error,
                                "failed to handle outbound substream",
                            );
                        }
                    }
                },
                Some(TransportEvent::SubstreamOpenFailure { substream, error }) => {
                    self.on_substream_open_failure(substream, error).await;
                }
                Some(TransportEvent::DialFailure { peer, addresses }) => self.on_dial_failure(peer, addresses).await,
                None => (),
            },
            result = self.pending_validations.select_next_some(), if !self.pending_validations.is_empty() => {
                if let Err(error) = self.on_validation_result(result.0, result.1).await {
                    tracing::debug!(
                        target: LOG_TARGET,
                        peer = ?result.0,
                        result = ?result.1,
                        ?error,
                        "failed to handle validation result",
                    );
                }
            }

            // User commands.
            command = self.command_rx.recv() => match command {
                None => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        protocol = %self.protocol,
                        "user protocol has exited, exiting"
                    );

                    self.service.unregister_protocol();

                    return true;
                }
                Some(command) => match command {
                    NotificationCommand::OpenSubstream { peers } => {
                        for peer in peers {
                            if let Err(error) = self.on_open_substream(peer).await {
                                tracing::debug!(
                                    target: LOG_TARGET,
                                    ?peer,
                                    ?error,
                                    "failed to open substream",
                                );
                            }
                        }
                    }
                    NotificationCommand::CloseSubstream { peers } => {
                        for peer in peers {
                            self.on_close_substream(peer).await;
                        }
                    }
                    NotificationCommand::ForceClose { peer } => {
                        let _ = self.service.force_close(peer);
                    }
                    #[cfg(feature = "fuzz")]
                    NotificationCommand::SendNotification{ .. } => unreachable!()
                }
            },
        }

        false
    }

    /// Start [`NotificationProtocol`] event loop.
    pub(crate) async fn run(mut self) {
        tracing::debug!(target: LOG_TARGET, "starting notification event loop");

        while !self.next_event().await {}
    }
}
