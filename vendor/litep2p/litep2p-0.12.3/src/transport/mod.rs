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

//! Transport protocol implementations provided by [`Litep2p`](`crate::Litep2p`).

use crate::{error::DialError, transport::manager::TransportHandle, types::ConnectionId, PeerId};

use futures::Stream;
use hickory_resolver::TokioResolver;
use multiaddr::Multiaddr;

use std::{fmt::Debug, sync::Arc, time::Duration};

pub(crate) mod common;
#[cfg(feature = "quic")]
pub mod quic;
pub mod tcp;
#[cfg(feature = "webrtc")]
pub mod webrtc;
#[cfg(feature = "websocket")]
pub mod websocket;

#[cfg(test)]
pub(crate) mod dummy;

pub(crate) mod manager;

pub use manager::limits::{ConnectionLimitsConfig, ConnectionLimitsError};

/// Timeout for opening a connection.
pub(crate) const CONNECTION_OPEN_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for opening a substream.
pub(crate) const SUBSTREAM_OPEN_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for connection waiting new substreams.
pub(crate) const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of parallel dial attempts.
pub(crate) const MAX_PARALLEL_DIALS: usize = 8;

/// Connection endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// Successfully established outbound connection.
    Dialer {
        /// Address that was dialed.
        address: Multiaddr,

        /// Connection ID.
        connection_id: ConnectionId,
    },

    /// Successfully established inbound connection.
    Listener {
        /// Local connection address.
        address: Multiaddr,

        /// Connection ID.
        connection_id: ConnectionId,
    },
}

impl Endpoint {
    /// Get `Multiaddr` of the [`Endpoint`].
    pub fn address(&self) -> &Multiaddr {
        match self {
            Self::Dialer { address, .. } => address,
            Self::Listener { address, .. } => address,
        }
    }

    /// Crate dialer.
    pub(crate) fn dialer(address: Multiaddr, connection_id: ConnectionId) -> Self {
        Endpoint::Dialer {
            address,
            connection_id,
        }
    }

    /// Create listener.
    pub(crate) fn listener(address: Multiaddr, connection_id: ConnectionId) -> Self {
        Endpoint::Listener {
            address,
            connection_id,
        }
    }

    /// Get `ConnectionId` of the `Endpoint`.
    pub fn connection_id(&self) -> ConnectionId {
        match self {
            Self::Dialer { connection_id, .. } => *connection_id,
            Self::Listener { connection_id, .. } => *connection_id,
        }
    }

    /// Is this a listener endpoint?
    pub fn is_listener(&self) -> bool {
        std::matches!(self, Self::Listener { .. })
    }
}

/// Transport event.
#[derive(Debug)]
pub(crate) enum TransportEvent {
    /// Fully negotiated connection established to remote peer.
    ConnectionEstablished {
        /// Peer ID.
        peer: PeerId,

        /// Endpoint.
        endpoint: Endpoint,
    },

    PendingInboundConnection {
        /// Connection ID.
        connection_id: ConnectionId,
    },

    /// Connection opened to remote but not yet negotiated.
    ConnectionOpened {
        /// Connection ID.
        connection_id: ConnectionId,

        /// Address that was dialed.
        address: Multiaddr,
    },

    /// Connection closed to remote peer.
    #[allow(unused)]
    ConnectionClosed {
        /// Peer ID.
        peer: PeerId,

        /// Connection ID.
        connection_id: ConnectionId,
    },

    /// Failed to dial remote peer.
    DialFailure {
        /// Connection ID.
        connection_id: ConnectionId,

        /// Dialed address.
        address: Multiaddr,

        /// Error.
        error: DialError,
    },

    /// Open failure for an unnegotiated set of connections.
    OpenFailure {
        /// Connection ID.
        connection_id: ConnectionId,

        /// Errors.
        errors: Vec<(Multiaddr, DialError)>,
    },
}

pub(crate) trait TransportBuilder {
    type Config: Debug;
    type Transport: Transport;

    /// Create new [`Transport`] object.
    fn new(
        context: TransportHandle,
        config: Self::Config,
        resolver: Arc<TokioResolver>,
    ) -> crate::Result<(Self, Vec<Multiaddr>)>
    where
        Self: Sized;
}

pub(crate) trait Transport: Stream + Unpin + Send {
    /// Dial `address` and negotiate connection.
    fn dial(&mut self, connection_id: ConnectionId, address: Multiaddr) -> crate::Result<()>;

    /// Accept negotiated connection.
    fn accept(&mut self, connection_id: ConnectionId) -> crate::Result<()>;

    /// Accept pending connection.
    fn accept_pending(&mut self, connection_id: ConnectionId) -> crate::Result<()>;

    /// Reject pending connection.
    fn reject_pending(&mut self, connection_id: ConnectionId) -> crate::Result<()>;

    /// Reject negotiated connection.
    fn reject(&mut self, connection_id: ConnectionId) -> crate::Result<()>;

    /// Attempt to open connection to remote peer over one or more addresses.
    fn open(&mut self, connection_id: ConnectionId, addresses: Vec<Multiaddr>)
        -> crate::Result<()>;

    /// Negotiate opened connection.
    fn negotiate(&mut self, connection_id: ConnectionId) -> crate::Result<()>;

    /// Cancel opening connections.
    ///
    /// This is a no-op for connections that have already succeeded/canceled.
    fn cancel(&mut self, connection_id: ConnectionId);
}
