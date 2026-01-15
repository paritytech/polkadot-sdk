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

//! Dummy transport.

use crate::{
    transport::{Transport, TransportEvent},
    types::ConnectionId,
};

use futures::Stream;
use multiaddr::Multiaddr;

use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};

/// Dummy transport.
pub(crate) struct DummyTransport {
    /// Events.
    events: VecDeque<TransportEvent>,
}

impl DummyTransport {
    /// Create new [`DummyTransport`].
    pub(crate) fn new() -> Self {
        Self {
            events: VecDeque::new(),
        }
    }

    /// Inject event into `DummyTransport`.
    pub(crate) fn inject_event(&mut self, event: TransportEvent) {
        self.events.push_back(event);
    }
}

impl Stream for DummyTransport {
    type Item = TransportEvent;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.events.is_empty() {
            return Poll::Pending;
        }

        Poll::Ready(self.events.pop_front())
    }
}

impl Transport for DummyTransport {
    fn dial(&mut self, _: ConnectionId, _: Multiaddr) -> crate::Result<()> {
        Ok(())
    }

    fn accept(&mut self, _: ConnectionId) -> crate::Result<()> {
        Ok(())
    }

    fn accept_pending(&mut self, _connection_id: ConnectionId) -> crate::Result<()> {
        Ok(())
    }

    fn reject_pending(&mut self, _connection_id: ConnectionId) -> crate::Result<()> {
        Ok(())
    }

    fn reject(&mut self, _: ConnectionId) -> crate::Result<()> {
        Ok(())
    }

    fn open(&mut self, _: ConnectionId, _: Vec<Multiaddr>) -> crate::Result<()> {
        Ok(())
    }

    fn negotiate(&mut self, _: ConnectionId) -> crate::Result<()> {
        Ok(())
    }

    /// Cancel opening connections.
    fn cancel(&mut self, _: ConnectionId) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::DialError, transport::Endpoint, PeerId};
    use futures::StreamExt;

    #[tokio::test]
    async fn pending_event() {
        let mut transport = DummyTransport::new();

        transport.inject_event(TransportEvent::DialFailure {
            connection_id: ConnectionId::from(1338usize),
            address: Multiaddr::empty(),
            error: DialError::Timeout,
        });

        let peer = PeerId::random();
        let endpoint = Endpoint::listener(Multiaddr::empty(), ConnectionId::from(1337usize));

        transport.inject_event(TransportEvent::ConnectionEstablished {
            peer,
            endpoint: endpoint.clone(),
        });

        match transport.next().await.unwrap() {
            TransportEvent::DialFailure {
                connection_id,
                address,
                ..
            } => {
                assert_eq!(connection_id, ConnectionId::from(1338usize));
                assert_eq!(address, Multiaddr::empty());
            }
            _ => panic!("invalid event"),
        }

        match transport.next().await.unwrap() {
            TransportEvent::ConnectionEstablished {
                peer: event_peer,
                endpoint: event_endpoint,
            } => {
                assert_eq!(peer, event_peer);
                assert_eq!(endpoint, event_endpoint);
            }
            _ => panic!("invalid event"),
        }

        futures::future::poll_fn(|cx| match transport.poll_next_unpin(cx) {
            Poll::Pending => Poll::Ready(()),
            _ => panic!("invalid event"),
        })
        .await;
    }

    #[test]
    fn dummy_handle_connection_states() {
        let mut transport = DummyTransport::new();

        assert!(transport.reject(ConnectionId::new()).is_ok());
        assert!(transport.open(ConnectionId::new(), Vec::new()).is_ok());
        assert!(transport.negotiate(ConnectionId::new()).is_ok());
        transport.cancel(ConnectionId::new());
    }
}
