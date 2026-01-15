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

//! Request-response protocol implementation.

use crate::{
    error::{Error, NegotiationError, SubstreamError},
    multistream_select::NegotiationError::Failed as MultistreamFailed,
    protocol::{
        request_response::handle::InnerRequestResponseEvent, Direction, TransportEvent,
        TransportService,
    },
    substream::Substream,
    types::{protocol::ProtocolName, RequestId, SubstreamId},
    utils::futures_stream::FuturesStream,
    PeerId,
};

use bytes::BytesMut;
use futures::{channel, future::BoxFuture, stream::FuturesUnordered, StreamExt};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
    time::sleep,
};

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    io::ErrorKind,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

pub use config::{Config, ConfigBuilder};
pub use handle::{
    DialOptions, RejectReason, RequestResponseCommand, RequestResponseError, RequestResponseEvent,
    RequestResponseHandle,
};

mod config;
mod handle;
#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::request-response::protocol";

/// Default request timeout.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Pending request.
type PendingRequest = (
    PeerId,
    RequestId,
    Option<ProtocolName>,
    Result<Vec<u8>, RequestResponseError>,
);

/// Request context.
struct RequestContext {
    /// Peer ID.
    peer: PeerId,

    /// Request ID.
    request_id: RequestId,

    /// Request.
    request: Vec<u8>,

    /// Fallback request.
    fallback: Option<(ProtocolName, Vec<u8>)>,
}

impl RequestContext {
    /// Create new [`RequestContext`].
    fn new(
        peer: PeerId,
        request_id: RequestId,
        request: Vec<u8>,
        fallback: Option<(ProtocolName, Vec<u8>)>,
    ) -> Self {
        Self {
            peer,
            request_id,
            request,
            fallback,
        }
    }
}

/// Peer context.
struct PeerContext {
    /// Active requests.
    active: HashSet<RequestId>,

    /// Active inbound requests and their fallback names.
    active_inbound: HashMap<RequestId, Option<ProtocolName>>,
}

impl PeerContext {
    /// Create new [`PeerContext`].
    fn new() -> Self {
        Self {
            active: HashSet::new(),
            active_inbound: HashMap::new(),
        }
    }
}

/// Request-response protocol.
pub(crate) struct RequestResponseProtocol {
    /// Transport service.
    service: TransportService,

    /// Protocol.
    protocol: ProtocolName,

    /// Connected peers.
    peers: HashMap<PeerId, PeerContext>,

    /// Pending outbound substreams, mapped from `SubstreamId` to `RequestId`.
    pending_outbound: HashMap<SubstreamId, RequestContext>,

    /// Pending outbound responses.
    ///
    /// The future listens to a `oneshot::Sender` which is given to `RequestResponseHandle`.
    /// If the request is accepted by the local node, the response is sent over the channel to the
    /// the future which sends it to remote peer and closes the substream.
    ///
    /// If the substream is rejected by the local node, the `oneshot::Sender` is dropped which
    /// notifies the future that the request should be rejected by closing the substream.
    pending_outbound_responses: FuturesUnordered<BoxFuture<'static, ()>>,

    /// Pending outbound cancellation handles.
    pending_outbound_cancels: HashMap<RequestId, oneshot::Sender<()>>,

    /// Pending inbound responses.
    pending_inbound: FuturesUnordered<BoxFuture<'static, PendingRequest>>,

    /// Pending inbound requests.
    pending_inbound_requests: FuturesStream<
        BoxFuture<
            'static,
            (
                PeerId,
                RequestId,
                Result<BytesMut, SubstreamError>,
                Substream,
            ),
        >,
    >,

    /// Pending dials for outbound requests.
    pending_dials: HashMap<PeerId, RequestContext>,

    /// TX channel for sending events to the user protocol.
    event_tx: Sender<InnerRequestResponseEvent>,

    /// RX channel for receive commands from the `RequestResponseHandle`.
    command_rx: Receiver<RequestResponseCommand>,

    /// Next request ID.
    next_request_id: Arc<AtomicUsize>,

    /// Timeout for outbound requests.
    timeout: Duration,

    /// Maximum concurrent inbound requests, if specified.
    max_concurrent_inbound_requests: Option<usize>,
}

impl RequestResponseProtocol {
    /// Create new [`RequestResponseProtocol`].
    pub(crate) fn new(service: TransportService, config: Config) -> Self {
        Self {
            service,
            peers: HashMap::new(),
            timeout: config.timeout,
            next_request_id: config.next_request_id,
            event_tx: config.event_tx,
            command_rx: config.command_rx,
            protocol: config.protocol_name,
            pending_dials: HashMap::new(),
            pending_outbound: HashMap::new(),
            pending_inbound: FuturesUnordered::new(),
            pending_outbound_cancels: HashMap::new(),
            pending_inbound_requests: FuturesStream::new(),
            pending_outbound_responses: FuturesUnordered::new(),
            max_concurrent_inbound_requests: config.max_concurrent_inbound_request,
        }
    }

    /// Get next ephemeral request ID.
    fn next_request_id(&mut self) -> RequestId {
        RequestId::from(self.next_request_id.fetch_add(1usize, Ordering::Relaxed))
    }

    /// Connection established to remote peer.
    async fn on_connection_established(&mut self, peer: PeerId) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?peer, protocol = %self.protocol, "connection established");

        let Entry::Vacant(entry) = self.peers.entry(peer) else {
            tracing::error!(
                target: LOG_TARGET,
                ?peer,
                "state mismatch: peer already exists",
            );
            debug_assert!(false);
            return Err(Error::PeerAlreadyExists(peer));
        };

        match self.pending_dials.remove(&peer) {
            None => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    "peer connected without pending dial",
                );
                entry.insert(PeerContext::new());
            }
            Some(context) => match self.service.open_substream(peer) {
                Ok(substream_id) => {
                    tracing::trace!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        request_id = ?context.request_id,
                        ?substream_id,
                        "dial succeeded, open substream",
                    );

                    entry.insert(PeerContext {
                        active: HashSet::from_iter([context.request_id]),
                        active_inbound: HashMap::new(),
                    });
                    self.pending_outbound.insert(
                        substream_id,
                        RequestContext::new(
                            peer,
                            context.request_id,
                            context.request,
                            context.fallback,
                        ),
                    );
                }
                // only reason the substream would fail to open would be that the connection
                // would've been reported to the protocol with enough delay that the keep-alive
                // timeout had expired and no other protocol had opened a substream to it, causing
                // the connection to be closed
                Err(error) => {
                    tracing::warn!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        request_id = ?context.request_id,
                        ?error,
                        "failed to open substream",
                    );

                    return self
                        .report_request_failure(
                            peer,
                            context.request_id,
                            RequestResponseError::Rejected(error.into()),
                        )
                        .await;
                }
            },
        }

        Ok(())
    }

    /// Connection closed to remote peer.
    async fn on_connection_closed(&mut self, peer: PeerId) {
        tracing::debug!(target: LOG_TARGET, ?peer, protocol = %self.protocol, "connection closed");

        // Remove any pending outbound substreams for this peer.
        self.pending_outbound.retain(|_, context| context.peer != peer);

        let Some(context) = self.peers.remove(&peer) else {
            tracing::error!(
                target: LOG_TARGET,
                ?peer,
                "Peer does not exist or substream open failed during connection establishment",
            );
            return;
        };

        // sent failure events for all pending outbound requests
        for request_id in context.active {
            let _ = self
                .event_tx
                .send(InnerRequestResponseEvent::RequestFailed {
                    peer,
                    request_id,
                    error: RequestResponseError::Rejected(RejectReason::ConnectionClosed),
                })
                .await;
        }
    }

    /// Local node opened a substream to remote node.
    async fn on_outbound_substream(
        &mut self,
        peer: PeerId,
        substream_id: SubstreamId,
        mut substream: Substream,
        fallback_protocol: Option<ProtocolName>,
    ) -> crate::Result<()> {
        let Some(RequestContext {
            request_id,
            request,
            fallback,
            ..
        }) = self.pending_outbound.remove(&substream_id)
        else {
            tracing::error!(
                target: LOG_TARGET,
                ?peer,
                protocol = %self.protocol,
                ?substream_id,
                "pending outbound request does not exist",
            );
            debug_assert!(false);

            return Err(Error::InvalidState);
        };

        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            protocol = %self.protocol,
            ?substream_id,
            ?request_id,
            "substream opened, send request",
        );

        let request = match (&fallback_protocol, fallback) {
            (Some(protocol), Some((fallback_protocol, fallback_request)))
                if protocol == &fallback_protocol =>
                fallback_request,
            _ => request,
        };

        let request_timeout = self.timeout;
        let protocol = self.protocol.clone();
        let (tx, rx) = oneshot::channel();
        self.pending_outbound_cancels.insert(request_id, tx);

        self.pending_inbound.push(Box::pin(async move {
            match tokio::time::timeout(request_timeout, substream.send_framed(request.into())).await
            {
                Err(_) => (
                    peer,
                    request_id,
                    fallback_protocol,
                    Err(RequestResponseError::Timeout),
                ),
                Ok(Err(SubstreamError::IoError(ErrorKind::PermissionDenied))) => {
                    tracing::warn!(
                        target: LOG_TARGET,
                        ?peer,
                        %protocol,
                        "tried to send too large request",
                    );

                    (
                        peer,
                        request_id,
                        fallback_protocol,
                        Err(RequestResponseError::TooLargePayload),
                    )
                }
                Ok(Err(error)) => (
                    peer,
                    request_id,
                    fallback_protocol,
                    Err(RequestResponseError::Rejected(error.into())),
                ),
                Ok(Ok(_)) => {
                    tokio::select! {
                        _ = rx => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?peer,
                                %protocol,
                                ?request_id,
                                "request canceled",
                            );

                            let _ = substream.close().await;
                            (
                                peer,
                                request_id,
                                fallback_protocol,
                                Err(RequestResponseError::Canceled))
                        }
                        _ = sleep(request_timeout) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?peer,
                                %protocol,
                                ?request_id,
                                "request timed out",
                            );

                            let _ = substream.close().await;
                            (peer, request_id, fallback_protocol, Err(RequestResponseError::Timeout))
                        }
                        event = substream.next() => match event {
                            Some(Ok(response)) => {
                                (peer, request_id, fallback_protocol, Ok(response.freeze().into()))
                            },
                            Some(Err(error)) => {
                                (peer, request_id, fallback_protocol, Err(RequestResponseError::Rejected(error.into())))
                            },
                            None => {
                                tracing::debug!(
                                    target: LOG_TARGET,
                                    ?peer,
                                    %protocol,
                                    ?request_id,
                                    "substream closed",
                                );
                                (peer, request_id, fallback_protocol, Err(RequestResponseError::Rejected(RejectReason::SubstreamClosed)))
                            }
                        }
                    }
                }
            }
        }));

        Ok(())
    }

    /// Handle pending inbound response.
    async fn on_inbound_request(
        &mut self,
        peer: PeerId,
        request_id: RequestId,
        request: Result<BytesMut, SubstreamError>,
        mut substream: Substream,
    ) -> crate::Result<()> {
        // The peer will no longer exist if the connection was closed before processing the request.
        let peer_context = self.peers.get_mut(&peer).ok_or(Error::PeerDoesntExist(peer))?;
        let fallback = peer_context.active_inbound.remove(&request_id).ok_or_else(|| {
            tracing::debug!(
                target: LOG_TARGET,
                ?peer,
                protocol = %self.protocol,
                ?request_id,
                "no active inbound request",
            );

            Error::InvalidState
        })?;

        let protocol = self.protocol.clone();

        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            %protocol,
            ?request_id,
            "inbound request",
        );

        let Ok(request) = request else {
            tracing::debug!(
                target: LOG_TARGET,
                ?peer,
                %protocol,
                ?request_id,
                ?request,
                "failed to read request from substream",
            );
            return Err(Error::InvalidData);
        };

        // once the request has been read from the substream, start a future which waits
        // for an input from the user.
        //
        // the input is either a response (succes) or rejection (failure) which is communicated
        // by sending the response over the `oneshot::Sender` or closing it, respectively.
        let timeout = self.timeout;
        let (response_tx, rx): (
            oneshot::Sender<(Vec<u8>, Option<channel::oneshot::Sender<()>>)>,
            _,
        ) = oneshot::channel();

        self.pending_outbound_responses.push(Box::pin(async move {
            match rx.await {
                Err(_) => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        %protocol,
                        ?request_id,
                        "request rejected",
                    );
                    let _ = substream.close().await;
                }
                Ok((response, mut feedback)) => {
                    tracing::trace!(
                        target: LOG_TARGET,
                        ?peer,
                        %protocol,
                        ?request_id,
                        "send response",
                    );

                    match tokio::time::timeout(timeout, substream.send_framed(response.into()))
                        .await
                    {
                        Err(_) => tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            %protocol,
                            ?request_id,
                            "timed out while sending response",
                        ),
                        Ok(Ok(_)) => feedback.take().map_or((), |feedback| {
                            let _ = feedback.send(());
                        }),
                        Ok(Err(error)) => tracing::trace!(
                        target: LOG_TARGET,
                            ?peer,
                            %protocol,
                            ?request_id,
                            ?error,
                            "failed to send request to peer",
                        ),
                    }
                }
            }
        }));

        self.event_tx
            .send(InnerRequestResponseEvent::RequestReceived {
                peer,
                fallback,
                request_id,
                request: request.freeze().into(),
                response_tx,
            })
            .await
            .map_err(From::from)
    }

    /// Remote opened a substream to local node.
    async fn on_inbound_substream(
        &mut self,
        peer: PeerId,
        fallback: Option<ProtocolName>,
        mut substream: Substream,
    ) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, protocol = %self.protocol, "handle inbound substream");

        if let Some(max_requests) = self.max_concurrent_inbound_requests {
            let num_inbound_requests =
                self.pending_inbound_requests.len() + self.pending_outbound_responses.len();

            if max_requests <= num_inbound_requests {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    ?fallback,
                    ?max_requests,
                    "rejecting request as already at maximum",
                );

                let _ = substream.close().await;
                return Ok(());
            }
        }

        // allocate ephemeral id for the inbound request and return it to the user protocol
        //
        // when user responds to the request, this is used to associate the response with the
        // correct substream.
        let request_id = self.next_request_id();
        self.peers
            .get_mut(&peer)
            .ok_or(Error::PeerDoesntExist(peer))?
            .active_inbound
            .insert(request_id, fallback);

        self.pending_inbound_requests.push(Box::pin(async move {
            let request = match substream.next().await {
                Some(Ok(request)) => Ok(request),
                Some(Err(error)) => Err(error),
                None => Err(SubstreamError::ConnectionClosed),
            };

            (peer, request_id, request, substream)
        }));

        Ok(())
    }

    async fn on_dial_failure(&mut self, peer: PeerId) {
        if let Some(context) = self.pending_dials.remove(&peer) {
            tracing::debug!(target: LOG_TARGET, ?peer, protocol = %self.protocol, "failed to dial peer");

            let _ = self
                .peers
                .get_mut(&peer)
                .map(|peer_context| peer_context.active.remove(&context.request_id));
            let _ = self
                .report_request_failure(
                    peer,
                    context.request_id,
                    RequestResponseError::Rejected(RejectReason::DialFailed(None)),
                )
                .await;
        }
    }

    /// Failed to open substream to remote peer.
    async fn on_substream_open_failure(
        &mut self,
        substream: SubstreamId,
        error: SubstreamError,
    ) -> crate::Result<()> {
        let Some(RequestContext {
            request_id, peer, ..
        }) = self.pending_outbound.remove(&substream)
        else {
            tracing::error!(
                target: LOG_TARGET,
                protocol = %self.protocol,
                ?substream,
                "pending outbound request does not exist",
            );
            debug_assert!(false);

            return Err(Error::InvalidState);
        };

        tracing::debug!(
            target: LOG_TARGET,
            ?peer,
            protocol = %self.protocol,
            ?request_id,
            ?substream,
            ?error,
            "failed to open substream",
        );

        let _ = self
            .peers
            .get_mut(&peer)
            .map(|peer_context| peer_context.active.remove(&request_id));

        self.event_tx
            .send(InnerRequestResponseEvent::RequestFailed {
                peer,
                request_id,
                error: match error {
                    SubstreamError::NegotiationError(NegotiationError::MultistreamSelectError(
                        MultistreamFailed,
                    )) => RequestResponseError::UnsupportedProtocol,
                    _ => RequestResponseError::Rejected(error.into()),
                },
            })
            .await
            .map_err(From::from)
    }

    /// Report request send failure to user.
    async fn report_request_failure(
        &mut self,
        peer: PeerId,
        request_id: RequestId,
        error: RequestResponseError,
    ) -> crate::Result<()> {
        self.event_tx
            .send(InnerRequestResponseEvent::RequestFailed {
                peer,
                request_id,
                error,
            })
            .await
            .map_err(From::from)
    }

    /// Send request to remote peer.
    fn on_send_request(
        &mut self,
        peer: PeerId,
        request_id: RequestId,
        request: Vec<u8>,
        dial_options: DialOptions,
        fallback: Option<(ProtocolName, Vec<u8>)>,
    ) -> Result<(), RequestResponseError> {
        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            protocol = %self.protocol,
            ?request_id,
            ?dial_options,
            "send request to remote peer",
        );

        let Some(context) = self.peers.get_mut(&peer) else {
            match dial_options {
                DialOptions::Reject => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        ?request_id,
                        ?dial_options,
                        "peer not connected and should not dial",
                    );

                    return Err(RequestResponseError::NotConnected);
                }
                DialOptions::Dial => match self.service.dial(&peer) {
                    Ok(_) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            ?peer,
                            protocol = %self.protocol,
                            ?request_id,
                            "started dialing peer",
                        );

                        self.pending_dials.insert(
                            peer,
                            RequestContext::new(peer, request_id, request, fallback),
                        );
                        return Ok(());
                    }
                    Err(error) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            protocol = %self.protocol,
                            ?error,
                            "failed to dial peer"
                        );

                        return Err(RequestResponseError::Rejected(RejectReason::DialFailed(
                            Some(error),
                        )));
                    }
                },
            }
        };

        // open substream and push it pending outbound substreams
        // once the substream is opened, send the request.
        match self.service.open_substream(peer) {
            Ok(substream_id) => {
                let unique_request_id = context.active.insert(request_id);
                debug_assert!(unique_request_id);

                self.pending_outbound.insert(
                    substream_id,
                    RequestContext::new(peer, request_id, request, fallback),
                );

                Ok(())
            }
            Err(error) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    protocol = %self.protocol,
                    ?request_id,
                    ?error,
                    "failed to open substream",
                );

                Err(RequestResponseError::Rejected(error.into()))
            }
        }
    }

    /// Handle substream event.
    async fn on_substream_event(
        &mut self,
        peer: PeerId,
        request_id: RequestId,
        fallback: Option<ProtocolName>,
        message: Result<Vec<u8>, RequestResponseError>,
    ) -> crate::Result<()> {
        if !self
            .peers
            .get_mut(&peer)
            .ok_or(Error::PeerDoesntExist(peer))?
            .active
            .remove(&request_id)
        {
            tracing::warn!(
                target: LOG_TARGET,
                ?peer,
                protocol = %self.protocol,
                ?request_id,
                "invalid state: received substream event but no active substream",
            );
            return Err(Error::InvalidState);
        }

        let event = match message {
            Ok(response) => InnerRequestResponseEvent::ResponseReceived {
                peer,
                request_id,
                response,
                fallback,
            },
            Err(error) => match error {
                RequestResponseError::Canceled => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        ?request_id,
                        "request canceled by local node",
                    );
                    return Ok(());
                }
                error => InnerRequestResponseEvent::RequestFailed {
                    peer,
                    request_id,
                    error,
                },
            },
        };

        self.event_tx.send(event).await.map_err(From::from)
    }

    /// Cancel outbound request.
    fn on_cancel_request(&mut self, request_id: RequestId) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, protocol = %self.protocol, ?request_id, "cancel outbound request");

        match self.pending_outbound_cancels.remove(&request_id) {
            Some(tx) => tx.send(()).map_err(|_| Error::SubstreamDoesntExist),
            None => {
                tracing::debug!(
                    target: LOG_TARGET,
                    protocol = %self.protocol,
                    ?request_id,
                    "tried to cancel request which doesn't exist",
                );

                Ok(())
            }
        }
    }

    /// Handles the service event.
    async fn handle_service_event(&mut self, event: TransportEvent) {
        match event {
            TransportEvent::ConnectionEstablished { peer, .. } => {
                if let Err(error) = self.on_connection_established(peer).await {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        ?error,
                        "failed to handle connection established",
                    );
                }
            }

            TransportEvent::ConnectionClosed { peer } => {
                self.on_connection_closed(peer).await;
            }

            TransportEvent::SubstreamOpened {
                peer,
                substream,
                direction,
                fallback,
                ..
            } => match direction {
                Direction::Inbound => {
                    if let Err(error) = self.on_inbound_substream(peer, fallback, substream).await {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            protocol = %self.protocol,
                            ?error,
                            "failed to handle inbound substream",
                        );
                    }
                }
                Direction::Outbound(substream_id) => {
                    let _ =
                        self.on_outbound_substream(peer, substream_id, substream, fallback).await;
                }
            },

            TransportEvent::SubstreamOpenFailure { substream, error } => {
                if let Err(error) = self.on_substream_open_failure(substream, error).await {
                    tracing::warn!(
                        target: LOG_TARGET,
                        protocol = %self.protocol,
                        ?error,
                        "failed to handle substream open failure",
                    );
                }
            }

            TransportEvent::DialFailure { peer, .. } => self.on_dial_failure(peer).await,
        }
    }

    /// Handles the user command.
    async fn handle_user_command(&mut self, command: RequestResponseCommand) {
        match command {
            RequestResponseCommand::SendRequest {
                peer,
                request_id,
                request,
                dial_options,
            } => {
                if let Err(error) =
                    self.on_send_request(peer, request_id, request, dial_options, None)
                {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        ?request_id,
                        ?error,
                        "failed to send request",
                    );

                    if let Err(error) = self.report_request_failure(peer, request_id, error).await {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            protocol = %self.protocol,
                            ?request_id,
                            ?error,
                            "failed to report request failure",
                        );
                    }
                }
            }
            RequestResponseCommand::SendRequestWithFallback {
                peer,
                request_id,
                request,
                fallback,
                dial_options,
            } => {
                if let Err(error) =
                    self.on_send_request(peer, request_id, request, dial_options, Some(fallback))
                {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        protocol = %self.protocol,
                        ?request_id,
                        ?error,
                        "failed to send request",
                    );

                    if let Err(error) = self.report_request_failure(peer, request_id, error).await {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            protocol = %self.protocol,
                            ?request_id,
                            ?error,
                            "failed to report request failure",
                        );
                    }
                }
            }
            RequestResponseCommand::CancelRequest { request_id } => {
                if let Err(error) = self.on_cancel_request(request_id) {
                    tracing::debug!(
                        target: LOG_TARGET,
                        protocol = %self.protocol,
                        ?request_id,
                        ?error,
                        "failed to cancel reqeuest",
                    );
                }
            }
        }
    }

    /// Start [`RequestResponseProtocol`] event loop.
    pub async fn run(mut self) {
        tracing::debug!(target: LOG_TARGET, "starting request-response event loop");

        loop {
            tokio::select! {
                // events coming from the network have higher priority than user commands as all user commands are
                // responses to network behaviour so ensure that the commands operate on the most up to date information.
                biased;

                // Connection and substream events from the transport service.
                event = self.service.next() => match event {
                    Some(event) => self.handle_service_event(event).await,
                    None => {
                        tracing::debug!(target: LOG_TARGET, protocol = %self.protocol, "service has exited, exiting");
                        return
                    }
                },

                // These are outbound requests waiting for the substream to produce a response.
                event = self.pending_inbound.select_next_some(), if !self.pending_inbound.is_empty() => {
                    let (peer, request_id, fallback, event) = event;

                    if let Err(error) = self.on_substream_event(peer, request_id, fallback, event).await {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            protocol = %self.protocol,
                            ?request_id,
                            ?error,
                            "failed to handle substream event",
                        );
                    }

                    self.pending_outbound_cancels.remove(&request_id);
                }

                // These are inbound requests waiting for the user to respond, then for the substream to send the response.
                _ = self.pending_outbound_responses.next(), if !self.pending_outbound_responses.is_empty() => {}

                // Inbound requests that are moved to `pending_outbound_responses`.
                event = self.pending_inbound_requests.next(), if !self.pending_inbound_requests.is_empty() => match event {
                    Some((peer, request_id, request, substream)) => {
                        if let Err(error) = self.on_inbound_request(peer, request_id, request, substream).await {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?peer,
                                protocol = %self.protocol,
                                ?request_id,
                                ?error,
                                "failed to handle inbound request",
                            );
                        }
                    }
                    None => return,
                },

                // User commands.
                command = self.command_rx.recv() => match command {
                    Some(command) => self.handle_user_command(command).await,
                    None => {
                        tracing::debug!(target: LOG_TARGET, protocol = %self.protocol, "user protocol has exited, exiting");
                        return
                    }
                },
            }
        }
    }
}
