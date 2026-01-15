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

//! [`/ipfs/ping/1.0.0`](https://github.com/libp2p/specs/blob/master/ping/ping.md) implementation.

use crate::{
    error::{Error, SubstreamError},
    protocol::{Direction, TransportEvent, TransportService},
    substream::Substream,
    types::SubstreamId,
    PeerId,
};

use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use tokio::sync::mpsc::Sender;

use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

pub use config::{Config, ConfigBuilder};

mod config;

// TODO: https://github.com/paritytech/litep2p/issues/132 let the user handle max failures

/// Log target for the file.
const LOG_TARGET: &str = "litep2p::ipfs::ping";

/// Events emitted by the ping protocol.
#[derive(Debug)]
pub enum PingEvent {
    /// Ping time with remote peer.
    Ping {
        /// Peer ID.
        peer: PeerId,

        /// Measured ping time with the peer.
        ping: Duration,
    },
}

/// Ping protocol.
pub(crate) struct Ping {
    /// Maximum failures before the peer is considered unreachable.
    _max_failures: usize,

    // Connection service.
    service: TransportService,

    /// TX channel for sending events to the user protocol.
    tx: Sender<PingEvent>,

    /// Connected peers.
    peers: HashSet<PeerId>,

    /// Pending outbound substreams.
    pending_outbound: FuturesUnordered<BoxFuture<'static, crate::Result<(PeerId, Duration)>>>,

    /// Pending inbound substreams.
    pending_inbound: FuturesUnordered<BoxFuture<'static, crate::Result<()>>>,
}

impl Ping {
    /// Create new [`Ping`] protocol.
    pub fn new(service: TransportService, config: Config) -> Self {
        Self {
            service,
            tx: config.tx_event,
            peers: HashSet::new(),
            pending_outbound: FuturesUnordered::new(),
            pending_inbound: FuturesUnordered::new(),
            _max_failures: config.max_failures,
        }
    }

    /// Connection established to remote peer.
    fn on_connection_established(&mut self, peer: PeerId) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, "connection established");

        self.service.open_substream(peer)?;
        self.peers.insert(peer);

        Ok(())
    }

    /// Connection closed to remote peer.
    fn on_connection_closed(&mut self, peer: PeerId) {
        tracing::trace!(target: LOG_TARGET, ?peer, "connection closed");

        self.peers.remove(&peer);
    }

    /// Handle outbound substream.
    fn on_outbound_substream(
        &mut self,
        peer: PeerId,
        substream_id: SubstreamId,
        mut substream: Substream,
    ) {
        tracing::trace!(target: LOG_TARGET, ?peer, "handle outbound substream");

        self.pending_outbound.push(Box::pin(async move {
            let future = async move {
                // TODO: https://github.com/paritytech/litep2p/issues/134 generate random payload and verify it
                substream.send_framed(vec![0u8; 32].into()).await?;
                let now = Instant::now();
                let _ = substream.next().await.ok_or(Error::SubstreamError(
                    SubstreamError::ReadFailure(Some(substream_id)),
                ))?;
                let _ = substream.close().await;

                Ok(now.elapsed())
            };

            match tokio::time::timeout(Duration::from_secs(10), future).await {
                Err(_) => Err(Error::Timeout),
                Ok(Err(error)) => Err(error),
                Ok(Ok(elapsed)) => Ok((peer, elapsed)),
            }
        }));
    }

    /// Substream opened to remote peer.
    fn on_inbound_substream(&mut self, peer: PeerId, mut substream: Substream) {
        tracing::trace!(target: LOG_TARGET, ?peer, "handle inbound substream");

        self.pending_inbound.push(Box::pin(async move {
            let future = async move {
                let payload = substream
                    .next()
                    .await
                    .ok_or(Error::SubstreamError(SubstreamError::ReadFailure(None)))??;
                substream.send_framed(payload.freeze()).await?;
                let _ = substream.next().await.map(|_| ());

                Ok(())
            };

            match tokio::time::timeout(Duration::from_secs(10), future).await {
                Err(_) => Err(Error::Timeout),
                Ok(Err(error)) => Err(error),
                Ok(Ok(())) => Ok(()),
            }
        }));
    }

    /// Start [`Ping`] event loop.
    pub async fn run(mut self) {
        tracing::debug!(target: LOG_TARGET, "starting ping event loop");

        loop {
            tokio::select! {
                event = self.service.next() => match event {
                    Some(TransportEvent::ConnectionEstablished { peer, .. }) => {
                        let _ = self.on_connection_established(peer);
                    }
                    Some(TransportEvent::ConnectionClosed { peer }) => {
                        self.on_connection_closed(peer);
                    }
                    Some(TransportEvent::SubstreamOpened {
                        peer,
                        substream,
                        direction,
                        ..
                    }) => match direction {
                        Direction::Inbound => {
                            self.on_inbound_substream(peer, substream);
                        }
                        Direction::Outbound(substream_id) => {
                            self.on_outbound_substream(peer, substream_id, substream);
                        }
                    },
                    Some(_) => {}
                    None => return,
                },
                _event = self.pending_inbound.next(), if !self.pending_inbound.is_empty() => {}
                event = self.pending_outbound.next(), if !self.pending_outbound.is_empty() => {
                    match event {
                        Some(Ok((peer, elapsed))) => {
                            let _ = self
                                .tx
                                .send(PingEvent::Ping {
                                    peer,
                                    ping: elapsed,
                                })
                                .await;
                        }
                        event => tracing::debug!(target: LOG_TARGET, "failed to handle ping for an outbound peer: {event:?}"),
                    }
                }
            }
        }
    }
}
