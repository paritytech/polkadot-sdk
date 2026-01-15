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
    protocol::notification::handle::NotificationSink, types::protocol::ProtocolName, PeerId,
};

use bytes::BytesMut;
use tokio::sync::oneshot;

use std::collections::HashSet;

/// Default channel size for synchronous notifications.
pub(super) const SYNC_CHANNEL_SIZE: usize = 2048;

/// Default channel size for asynchronous notifications.
pub(super) const ASYNC_CHANNEL_SIZE: usize = 8;

/// Direction of the connection.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Direction {
    /// Connection is considered inbound, i.e., it was initiated by the remote node.
    Inbound,

    /// Connection is considered outbound, i.e., it was initiated by the local node.
    Outbound,
}

/// Validation result.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// Accept the inbound substream.
    Accept,

    /// Reject the inbound substream.
    Reject,
}

/// Notification error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationError {
    /// Remote rejected the substream.
    Rejected,

    /// Connection to peer doesn't exist.
    NoConnection,

    /// Synchronous notification channel is clogged.
    ChannelClogged,

    /// Validation for a previous substream still pending.
    ValidationPending,

    /// Failed to dial peer.
    DialFailure,

    /// Notification protocol has been closed.
    EssentialTaskClosed,
}

/// Notification events.
pub(crate) enum InnerNotificationEvent {
    /// Validate substream.
    ValidateSubstream {
        /// Protocol name.
        protocol: ProtocolName,

        /// Fallback, if the substream was negotiated using a fallback protocol.
        fallback: Option<ProtocolName>,

        /// Peer ID.
        peer: PeerId,

        /// Handshake.
        handshake: Vec<u8>,

        /// `oneshot::Sender` for sending the validation result back to the protocol.
        tx: oneshot::Sender<ValidationResult>,
    },

    /// Notification stream opened.
    NotificationStreamOpened {
        /// Protocol name.
        protocol: ProtocolName,

        /// Fallback, if the substream was negotiated using a fallback protocol.
        fallback: Option<ProtocolName>,

        /// Direction of the substream.
        direction: Direction,

        /// Peer ID.
        peer: PeerId,

        /// Handshake.
        handshake: Vec<u8>,

        /// Notification sink.
        sink: NotificationSink,
    },

    /// Notification stream closed.
    NotificationStreamClosed {
        /// Peer ID.
        peer: PeerId,
    },

    /// Failed to open notification stream.
    NotificationStreamOpenFailure {
        /// Peer ID.
        peer: PeerId,

        /// Error.
        error: NotificationError,
    },
}

/// Notification events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationEvent {
    /// Validate substream.
    ValidateSubstream {
        /// Protocol name.
        protocol: ProtocolName,

        /// Fallback, if the substream was negotiated using a fallback protocol.
        fallback: Option<ProtocolName>,

        /// Peer ID.
        peer: PeerId,

        /// Handshake.
        handshake: Vec<u8>,
    },

    /// Notification stream opened.
    NotificationStreamOpened {
        /// Protocol name.
        protocol: ProtocolName,

        /// Fallback, if the substream was negotiated using a fallback protocol.
        fallback: Option<ProtocolName>,

        /// Direction of the substream.
        ///
        /// [`Direction::Inbound`](crate::protocol::Direction::Outbound) indicates that the
        /// substream was opened by the remote peer and
        /// [`Direction::Outbound`](crate::protocol::Direction::Outbound) that it was
        /// opened by the local node.
        direction: Direction,

        /// Peer ID.
        peer: PeerId,

        /// Handshake.
        handshake: Vec<u8>,
    },

    /// Notification stream closed.
    NotificationStreamClosed {
        /// Peer ID.
        peer: PeerId,
    },

    /// Failed to open notification stream.
    NotificationStreamOpenFailure {
        /// Peer ID.
        peer: PeerId,

        /// Error.
        error: NotificationError,
    },

    /// Notification received.
    NotificationReceived {
        /// Peer ID.
        peer: PeerId,

        /// Notification.
        notification: BytesMut,
    },
}

/// Notification commands sent to the protocol.
#[derive(Debug)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub enum NotificationCommand {
    /// Open substreams to one or more peers.
    OpenSubstream {
        /// Peer IDs.
        peers: HashSet<PeerId>,
    },

    /// Close substreams to one or more peers.
    CloseSubstream {
        /// Peer IDs.
        peers: HashSet<PeerId>,
    },

    /// Force close the connection because notification channel is clogged.
    ForceClose {
        /// Peer to disconnect.
        peer: PeerId,
    },

    #[cfg(feature = "fuzz")]
    SendNotification { notif: Vec<u8>, peer_id: PeerId },
}
