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

//! QUIC connection.

use std::time::Duration;

use crate::{
    config::Role,
    error::{Error, NegotiationError, SubstreamError},
    multistream_select::{dialer_select_proto, listener_select_proto, Negotiated, Version},
    protocol::{Direction, Permit, ProtocolCommand, ProtocolSet},
    substream,
    transport::{
        quic::substream::{NegotiatingSubstream, Substream},
        Endpoint,
    },
    types::{protocol::ProtocolName, SubstreamId},
    BandwidthSink, PeerId,
};

use futures::{future::BoxFuture, stream::FuturesUnordered, AsyncRead, AsyncWrite, StreamExt};
use quinn::{Connection as QuinnConnection, RecvStream, SendStream};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::quic::connection";

/// QUIC connection error.
#[derive(Debug)]
enum ConnectionError {
    /// Timeout
    Timeout {
        /// Protocol.
        protocol: Option<ProtocolName>,

        /// Substream ID.
        substream_id: Option<SubstreamId>,
    },

    /// Failed to negotiate connection/substream.
    FailedToNegotiate {
        /// Protocol.
        protocol: Option<ProtocolName>,

        /// Substream ID.
        substream_id: Option<SubstreamId>,

        /// Error.
        error: SubstreamError,
    },
}

struct NegotiatedSubstream {
    /// Substream direction.
    direction: Direction,

    /// Substream ID.
    substream_id: SubstreamId,

    /// Protocol name.
    protocol: ProtocolName,

    /// Substream used to send data.
    sender: SendStream,

    /// Substream used to receive data.
    receiver: RecvStream,

    /// Permit.
    permit: Permit,
}

/// QUIC connection.
pub struct QuicConnection {
    /// Remote peer ID.
    peer: PeerId,

    /// Endpoint.
    endpoint: Endpoint,

    /// Substream open timeout.
    substream_open_timeout: Duration,

    /// QUIC connection.
    connection: QuinnConnection,

    /// Protocol set.
    protocol_set: ProtocolSet,

    /// Bandwidth sink.
    bandwidth_sink: BandwidthSink,

    /// Pending substreams.
    pending_substreams:
        FuturesUnordered<BoxFuture<'static, Result<NegotiatedSubstream, ConnectionError>>>,
}

impl QuicConnection {
    /// Creates a new [`QuicConnection`].
    pub fn new(
        peer: PeerId,
        endpoint: Endpoint,
        connection: QuinnConnection,
        protocol_set: ProtocolSet,
        bandwidth_sink: BandwidthSink,
        substream_open_timeout: Duration,
    ) -> Self {
        Self {
            peer,
            endpoint,
            connection,
            protocol_set,
            bandwidth_sink,
            substream_open_timeout,
            pending_substreams: FuturesUnordered::new(),
        }
    }

    /// Negotiate protocol.
    async fn negotiate_protocol<S: AsyncRead + AsyncWrite + Unpin>(
        stream: S,
        role: &Role,
        protocols: Vec<&str>,
    ) -> Result<(Negotiated<S>, ProtocolName), NegotiationError> {
        tracing::trace!(target: LOG_TARGET, ?protocols, "negotiating protocols");

        let (protocol, socket) = match role {
            Role::Dialer => dialer_select_proto(stream, protocols, Version::V1).await,
            Role::Listener => listener_select_proto(stream, protocols).await,
        }
        .map_err(NegotiationError::MultistreamSelectError)?;

        tracing::trace!(target: LOG_TARGET, ?protocol, "protocol negotiated");

        Ok((socket, ProtocolName::from(protocol.to_string())))
    }

    /// Open substream for `protocol`.
    async fn open_substream(
        handle: QuinnConnection,
        permit: Permit,
        substream_id: SubstreamId,
        protocol: ProtocolName,
        fallback_names: Vec<ProtocolName>,
    ) -> Result<NegotiatedSubstream, SubstreamError> {
        tracing::debug!(target: LOG_TARGET, ?protocol, ?substream_id, "open substream");

        let stream = match handle.open_bi().await {
            Ok((send_stream, recv_stream)) => NegotiatingSubstream::new(send_stream, recv_stream),
            Err(error) => return Err(NegotiationError::Quic(error.into()).into()),
        };

        // TODO: https://github.com/paritytech/litep2p/issues/346 protocols don't change after
        // they've been initialized so this should be done only once
        let protocols = std::iter::once(&*protocol)
            .chain(fallback_names.iter().map(|protocol| &**protocol))
            .collect();

        let (io, protocol) = Self::negotiate_protocol(stream, &Role::Dialer, protocols).await?;

        tracing::trace!(
            target: LOG_TARGET,
            ?protocol,
            ?substream_id,
            "substream accepted and negotiated"
        );

        let stream = io.inner();
        let (sender, receiver) = stream.into_parts();

        Ok(NegotiatedSubstream {
            sender,
            receiver,
            substream_id,
            direction: Direction::Outbound(substream_id),
            permit,
            protocol,
        })
    }

    /// Accept bidirectional substream from rmeote peer.
    async fn accept_substream(
        stream: NegotiatingSubstream,
        protocols: Vec<ProtocolName>,
        substream_id: SubstreamId,
        permit: Permit,
    ) -> Result<NegotiatedSubstream, NegotiationError> {
        tracing::trace!(
            target: LOG_TARGET,
            ?substream_id,
            "accept inbound substream"
        );

        let protocols = protocols.iter().map(|protocol| &**protocol).collect::<Vec<&str>>();
        let (io, protocol) = Self::negotiate_protocol(stream, &Role::Listener, protocols).await?;

        tracing::trace!(
            target: LOG_TARGET,
            ?substream_id,
            ?protocol,
            "substream accepted and negotiated"
        );

        let stream = io.inner();
        let (sender, receiver) = stream.into_parts();

        Ok(NegotiatedSubstream {
            permit,
            sender,
            receiver,
            protocol,
            substream_id,
            direction: Direction::Inbound,
        })
    }

    /// Start event loop for [`QuicConnection`].
    pub async fn start(mut self) -> crate::Result<()> {
        self.protocol_set
            .report_connection_established(self.peer, self.endpoint.clone())
            .await?;

        loop {
            tokio::select! {
                event = self.connection.accept_bi() => match event {
                    Ok((send_stream, receive_stream)) => {

                        let substream = self.protocol_set.next_substream_id();
                        let protocols = self.protocol_set.protocols();
                        let permit = self.protocol_set.try_get_permit().ok_or(Error::ConnectionClosed)?;
                        let stream = NegotiatingSubstream::new(send_stream, receive_stream);
                        let substream_open_timeout = self.substream_open_timeout;

                        self.pending_substreams.push(Box::pin(async move {
                            match tokio::time::timeout(
                                substream_open_timeout,
                                Self::accept_substream(stream, protocols, substream, permit),
                            )
                            .await
                            {
                                Ok(Ok(substream)) => Ok(substream),
                                Ok(Err(error)) => Err(ConnectionError::FailedToNegotiate {
                                    protocol: None,
                                    substream_id: None,
                                    error: SubstreamError::NegotiationError(error),
                                }),
                                Err(_) => Err(ConnectionError::Timeout {
                                    protocol: None,
                                    substream_id: None
                                }),
                            }
                        }));
                    }
                    Err(error) => {
                        tracing::debug!(target: LOG_TARGET, peer = ?self.peer, ?error, "failed to accept substream");
                        return self.protocol_set.report_connection_closed(self.peer, self.endpoint.connection_id()).await;
                    }
                },
                substream = self.pending_substreams.select_next_some(), if !self.pending_substreams.is_empty() => {
                    match substream {
                        Err(error) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?error,
                                "failed to accept/open substream",
                            );

                            let (protocol, substream_id, error) = match error {
                                ConnectionError::Timeout { protocol, substream_id } => {
                                    (protocol, substream_id, SubstreamError::NegotiationError(NegotiationError::Timeout))
                                }
                                ConnectionError::FailedToNegotiate { protocol, substream_id, error } => {
                                    (protocol, substream_id, error)
                                }
                            };

                            if let (Some(protocol), Some(substream_id)) = (protocol, substream_id) {
                                self.protocol_set
                                    .report_substream_open_failure(protocol, substream_id, error)
                                    .await?;
                            }
                        }
                        Ok(substream) => {
                            let protocol = substream.protocol.clone();
                            let substream_id = substream.substream_id;
                            let direction = substream.direction;
                            let bandwidth_sink = self.bandwidth_sink.clone();
                            let substream = substream::Substream::new_quic(
                                self.peer,
                                substream_id,
                                Substream::new(
                                    substream.permit,
                                    substream.sender,
                                    substream.receiver,
                                    bandwidth_sink
                                ),
                                self.protocol_set.protocol_codec(&protocol)
                            );

                            self.protocol_set
                                .report_substream_open(self.peer, protocol, direction, substream)
                                .await?;
                        }
                    }
                }
                command = self.protocol_set.next() => match command {
                    None => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer = ?self.peer,
                            connection_id = ?self.endpoint.connection_id(),
                            "protocols have dropped connection"
                        );
                        return self.protocol_set.report_connection_closed(self.peer, self.endpoint.connection_id()).await;
                    }
                    Some(ProtocolCommand::OpenSubstream { protocol, fallback_names, substream_id, permit, .. }) => {
                        let connection = self.connection.clone();
                        let substream_open_timeout = self.substream_open_timeout;

                        tracing::trace!(
                            target: LOG_TARGET,
                            ?protocol,
                            ?fallback_names,
                            ?substream_id,
                            "open substream"
                        );

                        self.pending_substreams.push(Box::pin(async move {
                            match tokio::time::timeout(
                                substream_open_timeout,
                                Self::open_substream(
                                    connection,
                                    permit,
                                    substream_id,
                                    protocol.clone(),
                                    fallback_names,
                                ),
                            )
                            .await
                            {
                                Ok(Ok(substream)) => Ok(substream),
                                Ok(Err(error)) => Err(ConnectionError::FailedToNegotiate {
                                    protocol: Some(protocol),
                                    substream_id: Some(substream_id),
                                    error,
                                }),
                                Err(_) => Err(ConnectionError::Timeout {
                                    protocol: None,
                                    substream_id: None
                                }),
                            }
                        }));
                    }
                    Some(ProtocolCommand::ForceClose) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer = ?self.peer,
                            connection_id = ?self.endpoint.connection_id(),
                            "force closing connection",
                        );

                        return self.protocol_set.report_connection_closed(self.peer, self.endpoint.connection_id()).await;
                    }
                }
            }
        }
    }
}
