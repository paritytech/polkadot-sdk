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
    error::{ImmediateDialError, SubstreamError},
    multistream_select::ProtocolError,
    types::{protocol::ProtocolName, RequestId},
    Error, PeerId,
};

use futures::channel;
use tokio::sync::{
    mpsc::{Receiver, Sender},
    oneshot,
};

use std::{
    collections::HashMap,
    io::ErrorKind,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::request-response::handle";

/// Request-response error.
#[derive(Debug, PartialEq)]
pub enum RequestResponseError {
    /// Request was rejected.
    Rejected(RejectReason),

    /// Request was canceled by the local node.
    Canceled,

    /// Request timed out.
    Timeout,

    /// The peer is not connected and the dialing option was [`DialOptions::Reject`].
    NotConnected,

    /// Too large payload.
    TooLargePayload,

    /// Protocol not supported.
    UnsupportedProtocol,
}

/// The reason why a request was rejected.
#[derive(Debug, PartialEq)]
pub enum RejectReason {
    /// Substream error.
    SubstreamOpenError(SubstreamError),

    /// The peer disconnected before the request was processed.
    ConnectionClosed,

    /// The substream was closed before the request was processed.
    SubstreamClosed,

    /// The dial failed.
    ///
    /// If the dial failure is immediate, the error is included.
    ///
    /// If the dialing process is happening in parallel on multiple
    /// addresses (potentially with multiple protocols), the dialing
    /// process is not considered immediate and the given errors are not
    /// propagated for simplicity.
    DialFailed(Option<ImmediateDialError>),
}

impl From<SubstreamError> for RejectReason {
    fn from(error: SubstreamError) -> Self {
        // Convert `ErrorKind::NotConnected` to `RejectReason::ConnectionClosed`.
        match error {
            SubstreamError::IoError(ErrorKind::NotConnected) => RejectReason::ConnectionClosed,
            SubstreamError::YamuxError(crate::yamux::ConnectionError::Io(error), _)
                if error.kind() == ErrorKind::NotConnected =>
                RejectReason::ConnectionClosed,
            SubstreamError::NegotiationError(crate::error::NegotiationError::IoError(
                ErrorKind::NotConnected,
            )) => RejectReason::ConnectionClosed,
            SubstreamError::NegotiationError(
                crate::error::NegotiationError::MultistreamSelectError(
                    crate::multistream_select::NegotiationError::ProtocolError(
                        ProtocolError::IoError(error),
                    ),
                ),
            ) if error.kind() == ErrorKind::NotConnected => RejectReason::ConnectionClosed,
            error => RejectReason::SubstreamOpenError(error),
        }
    }
}

/// Request-response events.
#[derive(Debug)]
pub(super) enum InnerRequestResponseEvent {
    /// Request received from remote
    RequestReceived {
        /// Peer Id.
        peer: PeerId,

        /// Fallback protocol, if the substream was negotiated using a fallback.
        fallback: Option<ProtocolName>,

        /// Request ID.
        request_id: RequestId,

        /// Received request.
        request: Vec<u8>,

        /// `oneshot::Sender` for response.
        response_tx: oneshot::Sender<(Vec<u8>, Option<channel::oneshot::Sender<()>>)>,
    },

    /// Response received.
    ResponseReceived {
        /// Peer Id.
        peer: PeerId,

        /// Fallback protocol, if the substream was negotiated using a fallback.
        fallback: Option<ProtocolName>,

        /// Request ID.
        request_id: RequestId,

        /// Received request.
        response: Vec<u8>,
    },

    /// Request failed.
    RequestFailed {
        /// Peer Id.
        peer: PeerId,

        /// Request ID.
        request_id: RequestId,

        /// Request-response error.
        error: RequestResponseError,
    },
}

impl From<InnerRequestResponseEvent> for RequestResponseEvent {
    fn from(event: InnerRequestResponseEvent) -> Self {
        match event {
            InnerRequestResponseEvent::ResponseReceived {
                peer,
                request_id,
                response,
                fallback,
            } => RequestResponseEvent::ResponseReceived {
                peer,
                request_id,
                response,
                fallback,
            },
            InnerRequestResponseEvent::RequestFailed {
                peer,
                request_id,
                error,
            } => RequestResponseEvent::RequestFailed {
                peer,
                request_id,
                error,
            },
            _ => panic!("unhandled event"),
        }
    }
}

/// Request-response events.
#[derive(Debug, PartialEq)]
pub enum RequestResponseEvent {
    /// Request received from remote
    RequestReceived {
        /// Peer Id.
        peer: PeerId,

        /// Fallback protocol, if the substream was negotiated using a fallback.
        fallback: Option<ProtocolName>,

        /// Request ID.
        ///
        /// While `request_id` is guaranteed to be unique for this protocols, the request IDs are
        /// not unique across different request-response protocols, meaning two different
        /// request-response protocols can both assign `RequestId(123)` for any given request.
        request_id: RequestId,

        /// Received request.
        request: Vec<u8>,
    },

    /// Response received.
    ResponseReceived {
        /// Peer Id.
        peer: PeerId,

        /// Request ID.
        request_id: RequestId,

        /// Fallback protocol, if the substream was negotiated using a fallback.
        fallback: Option<ProtocolName>,

        /// Received request.
        response: Vec<u8>,
    },

    /// Request failed.
    RequestFailed {
        /// Peer Id.
        peer: PeerId,

        /// Request ID.
        request_id: RequestId,

        /// Request-response error.
        error: RequestResponseError,
    },
}

/// Dial behavior when sending requests.
#[derive(Debug)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub enum DialOptions {
    /// If the peer is not currently connected, attempt to dial them before sending a request.
    ///
    /// If the dial succeeds, the request is sent to the peer once the peer has been registered
    /// to the protocol.
    ///
    /// If the dial fails, [`RequestResponseError::Rejected`] is returned.
    Dial,

    /// If the peer is not connected, immediately reject the request and return
    /// [`RequestResponseError::NotConnected`].
    Reject,
}

/// Request-response commands.
#[derive(Debug)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub enum RequestResponseCommand {
    /// Send request to remote peer.
    SendRequest {
        /// Peer ID.
        peer: PeerId,

        /// Request ID.
        ///
        /// When a response is received or the request fails, the event contains this ID that
        /// the user protocol can associate with the correct request.
        ///
        /// If the user protocol only has one active request per peer, this ID can be safely
        /// discarded.
        request_id: RequestId,

        /// Request.
        request: Vec<u8>,

        /// Dial options, see [`DialOptions`] for more details.
        dial_options: DialOptions,
    },

    SendRequestWithFallback {
        /// Peer ID.
        peer: PeerId,

        /// Request ID.
        request_id: RequestId,

        /// Request that is sent over the main protocol, if negotiated.
        request: Vec<u8>,

        /// Request that is sent over the fallback protocol, if negotiated.
        fallback: (ProtocolName, Vec<u8>),

        /// Dial options, see [`DialOptions`] for more details.
        dial_options: DialOptions,
    },

    /// Cancel outbound request.
    CancelRequest {
        /// Request ID.
        request_id: RequestId,
    },
}

/// Handle given to the user protocol which allows it to interact with the request-response
/// protocol.
pub struct RequestResponseHandle {
    /// TX channel for sending commands to the request-response protocol.
    event_rx: Receiver<InnerRequestResponseEvent>,

    /// RX channel for receiving events from the request-response protocol.
    command_tx: Sender<RequestResponseCommand>,

    /// Pending responses.
    pending_responses:
        HashMap<RequestId, oneshot::Sender<(Vec<u8>, Option<channel::oneshot::Sender<()>>)>>,

    /// Next ephemeral request ID.
    next_request_id: Arc<AtomicUsize>,
}

impl RequestResponseHandle {
    /// Create new [`RequestResponseHandle`].
    pub(super) fn new(
        event_rx: Receiver<InnerRequestResponseEvent>,
        command_tx: Sender<RequestResponseCommand>,
        next_request_id: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            event_rx,
            command_tx,
            next_request_id,
            pending_responses: HashMap::new(),
        }
    }

    #[cfg(feature = "fuzz")]
    /// Expose functionality for fuzzing
    pub async fn fuzz_send_message(
        &mut self,
        command: RequestResponseCommand,
    ) -> crate::Result<RequestId> {
        let request_id = self.next_request_id();
        self.command_tx.send(command).await.map(|_| request_id).map_err(From::from)
    }

    /// Reject an inbound request.
    ///
    /// Reject request received from a remote peer. The substream is dropped which signals
    /// to the remote peer that request was rejected.
    pub fn reject_request(&mut self, request_id: RequestId) {
        match self.pending_responses.remove(&request_id) {
            None => {
                tracing::debug!(target: LOG_TARGET, ?request_id, "rejected request doesn't exist")
            }
            Some(sender) => {
                tracing::debug!(target: LOG_TARGET, ?request_id, "reject request");
                drop(sender);
            }
        }
    }

    /// Cancel an outbound request.
    ///
    /// Allows canceling an in-flight request if the local node is not interested in the answer
    /// anymore. If the request was canceled, no event is reported to the user as the cancelation
    /// always succeeds and it's assumed that the user does the necessary state clean up in their
    /// end after calling [`RequestResponseHandle::cancel_request()`].
    pub async fn cancel_request(&mut self, request_id: RequestId) {
        tracing::trace!(target: LOG_TARGET, ?request_id, "cancel request");

        let _ = self.command_tx.send(RequestResponseCommand::CancelRequest { request_id }).await;
    }

    /// Get next request ID.
    fn next_request_id(&self) -> RequestId {
        let request_id = self.next_request_id.fetch_add(1usize, Ordering::Relaxed);
        RequestId::from(request_id)
    }

    /// Send request to remote peer.
    ///
    /// While the returned `RequestId` is guaranteed to be unique for this request-response
    /// protocol, it's not unique across all installed request-response protocols. That is,
    /// multiple request-response protocols can return the same `RequestId` and this must be
    /// handled by the calling code correctly if the `RequestId`s are stored somewhere.
    pub async fn send_request(
        &mut self,
        peer: PeerId,
        request: Vec<u8>,
        dial_options: DialOptions,
    ) -> crate::Result<RequestId> {
        tracing::trace!(target: LOG_TARGET, ?peer, "send request to peer");

        let request_id = self.next_request_id();
        self.command_tx
            .send(RequestResponseCommand::SendRequest {
                peer,
                request_id,
                request,
                dial_options,
            })
            .await
            .map(|_| request_id)
            .map_err(From::from)
    }

    /// Attempt to send request to peer and if the channel is clogged, return
    /// `Error::ChannelClogged`.
    ///
    /// While the returned `RequestId` is guaranteed to be unique for this request-response
    /// protocol, it's not unique across all installed request-response protocols. That is,
    /// multiple request-response protocols can return the same `RequestId` and this must be
    /// handled by the calling code correctly if the `RequestId`s are stored somewhere.
    pub fn try_send_request(
        &mut self,
        peer: PeerId,
        request: Vec<u8>,
        dial_options: DialOptions,
    ) -> crate::Result<RequestId> {
        tracing::trace!(target: LOG_TARGET, ?peer, "send request to peer");

        let request_id = self.next_request_id();
        self.command_tx
            .try_send(RequestResponseCommand::SendRequest {
                peer,
                request_id,
                request,
                dial_options,
            })
            .map(|_| request_id)
            .map_err(|_| Error::ChannelClogged)
    }

    /// Send request to remote peer with fallback.
    pub async fn send_request_with_fallback(
        &mut self,
        peer: PeerId,
        request: Vec<u8>,
        fallback: (ProtocolName, Vec<u8>),
        dial_options: DialOptions,
    ) -> crate::Result<RequestId> {
        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            fallback = %fallback.0,
            ?dial_options,
            "send request with fallback to peer",
        );

        let request_id = self.next_request_id();
        self.command_tx
            .send(RequestResponseCommand::SendRequestWithFallback {
                peer,
                request_id,
                fallback,
                request,
                dial_options,
            })
            .await
            .map(|_| request_id)
            .map_err(From::from)
    }

    /// Attempt to send request to peer with fallback and if the channel is clogged,
    /// return `Error::ChannelClogged`.
    pub fn try_send_request_with_fallback(
        &mut self,
        peer: PeerId,
        request: Vec<u8>,
        fallback: (ProtocolName, Vec<u8>),
        dial_options: DialOptions,
    ) -> crate::Result<RequestId> {
        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            fallback = %fallback.0,
            ?dial_options,
            "send request with fallback to peer",
        );

        let request_id = self.next_request_id();
        self.command_tx
            .try_send(RequestResponseCommand::SendRequestWithFallback {
                peer,
                request_id,
                fallback,
                request,
                dial_options,
            })
            .map(|_| request_id)
            .map_err(|_| Error::ChannelClogged)
    }

    /// Send response to remote peer.
    pub fn send_response(&mut self, request_id: RequestId, response: Vec<u8>) {
        match self.pending_responses.remove(&request_id) {
            None => {
                tracing::debug!(target: LOG_TARGET, ?request_id, "pending response doens't exist");
            }
            Some(response_tx) => {
                tracing::trace!(target: LOG_TARGET, ?request_id, "send response to peer");

                if let Err(_) = response_tx.send((response, None)) {
                    tracing::debug!(target: LOG_TARGET, ?request_id, "substream closed");
                }
            }
        }
    }

    /// Send response to remote peer with feedback.
    ///
    /// The feedback system is inherited from Polkadot SDK's `sc-network` and it's used to notify
    /// the sender of the response whether it was sent successfully or not. Once the response has
    /// been sent over the substream successfully, `()` will be sent over the feedback channel
    /// to the sender to notify them about it. If the substream has been closed or the substream
    /// failed while sending the response, the feedback channel will be dropped, notifying the
    /// sender that sending the response failed.
    pub fn send_response_with_feedback(
        &mut self,
        request_id: RequestId,
        response: Vec<u8>,
        feedback: channel::oneshot::Sender<()>,
    ) {
        match self.pending_responses.remove(&request_id) {
            None => {
                tracing::debug!(target: LOG_TARGET, ?request_id, "pending response doens't exist");
            }
            Some(response_tx) => {
                tracing::trace!(target: LOG_TARGET, ?request_id, "send response to peer");

                if let Err(_) = response_tx.send((response, Some(feedback))) {
                    tracing::debug!(target: LOG_TARGET, ?request_id, "substream closed");
                }
            }
        }
    }
}

impl futures::Stream for RequestResponseHandle {
    type Item = RequestResponseEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match futures::ready!(self.event_rx.poll_recv(cx)) {
            None => Poll::Ready(None),
            Some(event) => match event {
                InnerRequestResponseEvent::RequestReceived {
                    peer,
                    fallback,
                    request_id,
                    request,
                    response_tx,
                } => {
                    self.pending_responses.insert(request_id, response_tx);
                    Poll::Ready(Some(RequestResponseEvent::RequestReceived {
                        peer,
                        fallback,
                        request_id,
                        request,
                    }))
                }
                event => Poll::Ready(Some(event.into())),
            },
        }
    }
}
