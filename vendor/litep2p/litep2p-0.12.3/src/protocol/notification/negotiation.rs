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

//! Implementation of the notification handshaking.

use crate::{substream::Substream, PeerId};

use futures::{FutureExt, Sink, Stream};
use futures_timer::Delay;
use parking_lot::RwLock;

use std::{
    collections::{HashMap, VecDeque},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::notification::negotiation";

/// Maximum timeout wait before for handshake before operation is considered failed.
const NEGOTIATION_TIMEOUT: Duration = Duration::from_secs(10);

/// Substream direction.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Outbound substream, opened by local node.
    Outbound,

    /// Inbound substream, opened by remote node.
    Inbound,
}

/// Events emitted by [`HandshakeService`].
#[derive(Debug)]
pub enum HandshakeEvent {
    /// Substream has been negotiated.
    Negotiated {
        /// Peer ID.
        peer: PeerId,

        /// Handshake.
        handshake: Vec<u8>,

        /// Substream.
        substream: Substream,

        /// Direction.
        direction: Direction,
    },

    /// Outbound substream has been negotiated.
    NegotiationError {
        /// Peer ID.
        peer: PeerId,

        /// Direction.
        direction: Direction,
    },
}

/// Outbound substream's handshake state
enum HandshakeState {
    /// Send handshake to remote peer.
    SendHandshake,

    /// Sink is ready for the handshake to be sent.
    SinkReady,

    /// Handshake has been sent.
    HandshakeSent,

    /// Read handshake from remote peer.
    ReadHandshake,
}

/// Handshake service.
pub(crate) struct HandshakeService {
    /// Handshake.
    handshake: Arc<RwLock<Vec<u8>>>,

    /// Pending outbound substreams.
    /// Substreams:
    substreams: HashMap<(PeerId, Direction), (Substream, Delay, HandshakeState)>,

    /// Ready substreams.
    ready: VecDeque<(PeerId, Direction, Vec<u8>)>,
}

impl HandshakeService {
    /// Create new [`HandshakeService`].
    pub fn new(handshake: Arc<RwLock<Vec<u8>>>) -> Self {
        Self {
            handshake,
            ready: VecDeque::new(),
            substreams: HashMap::new(),
        }
    }

    /// Remove outbound substream from [`HandshakeService`].
    pub fn remove_outbound(&mut self, peer: &PeerId) -> Option<Substream> {
        self.substreams
            .remove(&(*peer, Direction::Outbound))
            .map(|(substream, _, _)| substream)
    }

    /// Remove inbound substream from [`HandshakeService`].
    pub fn remove_inbound(&mut self, peer: &PeerId) -> Option<Substream> {
        self.substreams
            .remove(&(*peer, Direction::Inbound))
            .map(|(substream, _, _)| substream)
    }

    /// Negotiate outbound handshake.
    pub fn negotiate_outbound(&mut self, peer: PeerId, substream: Substream) {
        tracing::trace!(target: LOG_TARGET, ?peer, "negotiate outbound");

        self.substreams.insert(
            (peer, Direction::Outbound),
            (
                substream,
                Delay::new(NEGOTIATION_TIMEOUT),
                HandshakeState::SendHandshake,
            ),
        );
    }

    /// Read handshake from remote peer.
    pub fn read_handshake(&mut self, peer: PeerId, substream: Substream) {
        tracing::trace!(target: LOG_TARGET, ?peer, "read handshake");

        self.substreams.insert(
            (peer, Direction::Inbound),
            (
                substream,
                Delay::new(NEGOTIATION_TIMEOUT),
                HandshakeState::ReadHandshake,
            ),
        );
    }

    /// Write handshake to remote peer.
    pub fn send_handshake(&mut self, peer: PeerId, substream: Substream) {
        tracing::trace!(target: LOG_TARGET, ?peer, "send handshake");

        self.substreams.insert(
            (peer, Direction::Inbound),
            (
                substream,
                Delay::new(NEGOTIATION_TIMEOUT),
                HandshakeState::SendHandshake,
            ),
        );
    }

    /// Returns `true` if [`HandshakeService`] contains no elements.
    pub fn is_empty(&self) -> bool {
        self.substreams.is_empty()
    }

    /// Pop event from the event queue.
    ///
    /// The substream may not exist in the queue anymore as it may have been removed
    /// by `NotificationProtocol` if either one of the substreams failed to negotiate.
    fn pop_event(&mut self) -> Option<(PeerId, HandshakeEvent)> {
        while let Some((peer, direction, handshake)) = self.ready.pop_front() {
            if let Some((substream, _, _)) = self.substreams.remove(&(peer, direction)) {
                return Some((
                    peer,
                    HandshakeEvent::Negotiated {
                        peer,
                        handshake,
                        substream,
                        direction,
                    },
                ));
            }
        }

        None
    }
}

impl Stream for HandshakeService {
    type Item = (PeerId, HandshakeEvent);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = Pin::into_inner(self);

        if let Some(event) = inner.pop_event() {
            return Poll::Ready(Some(event));
        }

        if inner.substreams.is_empty() {
            return Poll::Pending;
        }

        'outer: for ((peer, direction), (ref mut substream, ref mut timer, state)) in
            inner.substreams.iter_mut()
        {
            if let Poll::Ready(()) = timer.poll_unpin(cx) {
                return Poll::Ready(Some((
                    *peer,
                    HandshakeEvent::NegotiationError {
                        peer: *peer,
                        direction: *direction,
                    },
                )));
            }

            loop {
                let pinned = Pin::new(&mut *substream);

                match state {
                    HandshakeState::SendHandshake => match pinned.poll_ready(cx) {
                        Poll::Ready(Ok(())) => {
                            *state = HandshakeState::SinkReady;
                            continue;
                        }
                        Poll::Ready(Err(_)) =>
                            return Poll::Ready(Some((
                                *peer,
                                HandshakeEvent::NegotiationError {
                                    peer: *peer,
                                    direction: *direction,
                                },
                            ))),
                        Poll::Pending => continue 'outer,
                    },
                    HandshakeState::SinkReady => {
                        match pinned.start_send((*inner.handshake.read()).clone().into()) {
                            Ok(()) => {
                                *state = HandshakeState::HandshakeSent;
                                continue;
                            }
                            Err(_) =>
                                return Poll::Ready(Some((
                                    *peer,
                                    HandshakeEvent::NegotiationError {
                                        peer: *peer,
                                        direction: *direction,
                                    },
                                ))),
                        }
                    }
                    HandshakeState::HandshakeSent => match pinned.poll_flush(cx) {
                        Poll::Ready(Ok(())) => match direction {
                            Direction::Outbound => {
                                *state = HandshakeState::ReadHandshake;
                                continue;
                            }
                            Direction::Inbound => {
                                inner.ready.push_back((*peer, *direction, vec![]));
                                continue 'outer;
                            }
                        },
                        Poll::Ready(Err(_)) =>
                            return Poll::Ready(Some((
                                *peer,
                                HandshakeEvent::NegotiationError {
                                    peer: *peer,
                                    direction: *direction,
                                },
                            ))),
                        Poll::Pending => continue 'outer,
                    },
                    HandshakeState::ReadHandshake => match pinned.poll_next(cx) {
                        Poll::Ready(Some(Ok(handshake))) => {
                            inner.ready.push_back((*peer, *direction, handshake.freeze().into()));
                            continue 'outer;
                        }
                        Poll::Ready(Some(Err(_))) | Poll::Ready(None) => {
                            return Poll::Ready(Some((
                                *peer,
                                HandshakeEvent::NegotiationError {
                                    peer: *peer,
                                    direction: *direction,
                                },
                            )));
                        }
                        Poll::Pending => continue 'outer,
                    },
                }
            }
        }

        if let Some((peer, direction, handshake)) = inner.ready.pop_front() {
            let (substream, _, _) =
                inner.substreams.remove(&(peer, direction)).expect("peer to exist");

            return Poll::Ready(Some((
                peer,
                HandshakeEvent::Negotiated {
                    peer,
                    handshake,
                    substream,
                    direction,
                },
            )));
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock::substream::{DummySubstream, MockSubstream},
        types::SubstreamId,
    };
    use futures::StreamExt;

    #[tokio::test]
    async fn substream_error_when_sending_handshake() {
        let mut service = HandshakeService::new(Arc::new(RwLock::new(vec![1, 2, 3, 4])));

        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            Poll::Pending => Poll::Ready(()),
            _ => panic!("invalid event received"),
        })
        .await;

        let mut substream = MockSubstream::new();
        substream.expect_poll_ready().times(1).return_once(|_| Poll::Ready(Ok(())));
        substream
            .expect_start_send()
            .times(1)
            .return_once(|_| Err(crate::error::SubstreamError::ConnectionClosed));

        let peer = PeerId::random();
        let substream = Substream::new_mock(peer, SubstreamId::from(0usize), Box::new(substream));

        service.send_handshake(peer, substream);
        match service.next().await {
            Some((
                failed_peer,
                HandshakeEvent::NegotiationError {
                    peer: event_peer,
                    direction,
                },
            )) => {
                assert_eq!(failed_peer, peer);
                assert_eq!(event_peer, peer);
                assert_eq!(direction, Direction::Inbound);
            }
            _ => panic!("invalid event received"),
        }
    }

    #[tokio::test]
    async fn substream_error_when_flushing_substream() {
        let mut service = HandshakeService::new(Arc::new(RwLock::new(vec![1, 2, 3, 4])));

        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            Poll::Pending => Poll::Ready(()),
            _ => panic!("invalid event received"),
        })
        .await;

        let mut substream = MockSubstream::new();
        substream.expect_poll_ready().times(1).return_once(|_| Poll::Ready(Ok(())));
        substream.expect_start_send().times(1).return_once(|_| Ok(()));
        substream
            .expect_poll_flush()
            .times(1)
            .return_once(|_| Poll::Ready(Err(crate::error::SubstreamError::ConnectionClosed)));

        let peer = PeerId::random();
        let substream = Substream::new_mock(peer, SubstreamId::from(0usize), Box::new(substream));

        service.send_handshake(peer, substream);
        match service.next().await {
            Some((
                failed_peer,
                HandshakeEvent::NegotiationError {
                    peer: event_peer,
                    direction,
                },
            )) => {
                assert_eq!(failed_peer, peer);
                assert_eq!(event_peer, peer);
                assert_eq!(direction, Direction::Inbound);
            }
            _ => panic!("invalid event received"),
        }
    }

    // inbound substream is negotiated and it pushed into `inner` but outbound substream fails to
    // negotiate
    #[tokio::test]
    async fn pop_event_but_substream_doesnt_exist() {
        let mut service = HandshakeService::new(Arc::new(RwLock::new(vec![1, 2, 3, 4])));
        let peer = PeerId::random();

        // inbound substream has finished
        service.ready.push_front((peer, Direction::Inbound, vec![]));
        service.substreams.insert(
            (peer, Direction::Inbound),
            (
                Substream::new_mock(
                    peer,
                    SubstreamId::from(1337usize),
                    Box::new(DummySubstream::new()),
                ),
                Delay::new(NEGOTIATION_TIMEOUT),
                HandshakeState::HandshakeSent,
            ),
        );
        service.substreams.insert(
            (peer, Direction::Outbound),
            (
                Substream::new_mock(
                    peer,
                    SubstreamId::from(1337usize),
                    Box::new(DummySubstream::new()),
                ),
                Delay::new(NEGOTIATION_TIMEOUT),
                HandshakeState::SendHandshake,
            ),
        );

        // outbound substream failed and `NotificationProtocol` removes
        // both substreams from `HandshakeService`
        assert!(service.remove_outbound(&peer).is_some());
        assert!(service.remove_inbound(&peer).is_some());

        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            Poll::Pending => Poll::Ready(()),
            _ => panic!("invalid event received"),
        })
        .await
    }
}
