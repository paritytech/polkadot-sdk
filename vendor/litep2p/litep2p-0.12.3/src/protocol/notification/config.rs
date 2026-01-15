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
    protocol::notification::{
        handle::NotificationHandle,
        types::{
            InnerNotificationEvent, NotificationCommand, ASYNC_CHANNEL_SIZE, SYNC_CHANNEL_SIZE,
        },
    },
    types::protocol::ProtocolName,
    PeerId, DEFAULT_CHANNEL_SIZE,
};

use bytes::BytesMut;
use parking_lot::RwLock;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::sync::Arc;

/// Notification configuration.
#[derive(Debug)]
pub struct Config {
    /// Protocol name.
    pub(crate) protocol_name: ProtocolName,

    /// Protocol codec.
    pub(crate) codec: ProtocolCodec,

    /// Maximum notification size.
    _max_notification_size: usize,

    /// Handshake bytes.
    pub(crate) handshake: Arc<RwLock<Vec<u8>>>,

    /// Auto accept inbound substream.
    pub(super) auto_accept: bool,

    /// Protocol aliases.
    pub(crate) fallback_names: Vec<ProtocolName>,

    /// TX channel passed to the protocol used for sending events.
    pub(crate) event_tx: Sender<InnerNotificationEvent>,

    /// TX channel for sending notifications from the connection handlers.
    pub(crate) notif_tx: Sender<(PeerId, BytesMut)>,

    /// RX channel passed to the protocol used for receiving commands.
    pub(crate) command_rx: Receiver<NotificationCommand>,

    /// Synchronous channel size.
    pub(crate) sync_channel_size: usize,

    /// Asynchronous channel size.
    pub(crate) async_channel_size: usize,

    /// Should `NotificationProtocol` dial the peer if there is no connection to them
    /// when an outbound substream is requested.
    pub(crate) should_dial: bool,
}

impl Config {
    /// Create new [`Config`].
    pub fn new(
        protocol_name: ProtocolName,
        max_notification_size: usize,
        handshake: Vec<u8>,
        fallback_names: Vec<ProtocolName>,
        auto_accept: bool,
        sync_channel_size: usize,
        async_channel_size: usize,
        should_dial: bool,
    ) -> (Self, NotificationHandle) {
        let (event_tx, event_rx) = channel(DEFAULT_CHANNEL_SIZE);
        let (notif_tx, notif_rx) = channel(DEFAULT_CHANNEL_SIZE);
        let (command_tx, command_rx) = channel(DEFAULT_CHANNEL_SIZE);
        let handshake = Arc::new(RwLock::new(handshake));
        let handle =
            NotificationHandle::new(event_rx, notif_rx, command_tx, Arc::clone(&handshake));

        (
            Self {
                protocol_name,
                codec: ProtocolCodec::UnsignedVarint(Some(max_notification_size)),
                _max_notification_size: max_notification_size,
                auto_accept,
                handshake,
                fallback_names,
                event_tx,
                notif_tx,
                command_rx,
                should_dial,
                sync_channel_size,
                async_channel_size,
            },
            handle,
        )
    }

    /// Get protocol name.
    pub(crate) fn protocol_name(&self) -> &ProtocolName {
        &self.protocol_name
    }

    /// Set handshake for the protocol.
    ///
    /// This function is used to work around an issue in Polkadot SDK and users
    /// should not depend on its continued existence.
    pub fn set_handshake(&mut self, handshake: Vec<u8>) {
        let mut inner = self.handshake.write();
        *inner = handshake;
    }
}

/// Notification configuration builder.
pub struct ConfigBuilder {
    /// Protocol name.
    protocol_name: ProtocolName,

    /// Maximum notification size.
    max_notification_size: Option<usize>,

    /// Handshake bytes.
    handshake: Option<Vec<u8>>,

    /// Should `NotificationProtocol` dial the peer if an outbound substream is requested but there
    /// is no connection to the peer.
    should_dial: bool,

    /// Fallback names.
    fallback_names: Vec<ProtocolName>,

    /// Auto accept inbound substream.
    auto_accept_inbound_for_initiated: bool,

    /// Synchronous channel size.
    sync_channel_size: usize,

    /// Asynchronous channel size.
    async_channel_size: usize,
}

impl ConfigBuilder {
    /// Create new [`ConfigBuilder`].
    pub fn new(protocol_name: ProtocolName) -> Self {
        Self {
            protocol_name,
            max_notification_size: None,
            handshake: None,
            fallback_names: Vec::new(),
            auto_accept_inbound_for_initiated: false,
            sync_channel_size: SYNC_CHANNEL_SIZE,
            async_channel_size: ASYNC_CHANNEL_SIZE,
            should_dial: true,
        }
    }

    /// Set maximum notification size.
    pub fn with_max_size(mut self, max_notification_size: usize) -> Self {
        self.max_notification_size = Some(max_notification_size);
        self
    }

    /// Set handshake.
    pub fn with_handshake(mut self, handshake: Vec<u8>) -> Self {
        self.handshake = Some(handshake);
        self
    }

    /// Set fallback names.
    pub fn with_fallback_names(mut self, fallback_names: Vec<ProtocolName>) -> Self {
        self.fallback_names = fallback_names;
        self
    }

    /// Auto-accept inbound substreams for those connections which were initiated by the local
    /// node.
    ///
    /// Connection in this context means a bidirectional substream pair between two peers over a
    /// given protocol.
    ///
    /// By default, when a node starts a connection with a remote node and opens an outbound
    /// substream to them, that substream is validated and if it's accepted, remote node sends
    /// their handshake over that substream and opens another substream to local node. The
    /// substream that was opened by the local node is used for sending data and the one opened
    /// by the remote node is used for receiving data.
    ///
    /// By default, even if the local node was the one that opened the first substream, this inbound
    /// substream coming from remote node must be validated as the handshake of the remote node
    /// may reveal that it's not someone that the local node is willing to accept.
    ///
    /// To disable this behavior, auto accepting for the inbound substream can be enabled. If local
    /// node is the one that opened the connection and it was accepted by the remote node, local
    /// node is only notified via
    /// [`NotificationStreamOpened`](super::types::NotificationEvent::NotificationStreamOpened).
    pub fn with_auto_accept_inbound(mut self, auto_accept: bool) -> Self {
        self.auto_accept_inbound_for_initiated = auto_accept;
        self
    }

    /// Configure size of the channel for sending synchronous notifications.
    ///
    /// Default value is `16`.
    pub fn with_sync_channel_size(mut self, size: usize) -> Self {
        self.sync_channel_size = size;
        self
    }

    /// Configure size of the channel for sending asynchronous notifications.
    ///
    /// Default value is `8`.
    pub fn with_async_channel_size(mut self, size: usize) -> Self {
        self.async_channel_size = size;
        self
    }

    /// Should `NotificationProtocol` attempt to dial the peer if an outbound substream is opened
    /// but no connection to the peer exist.
    ///
    /// Dialing is enabled by default.
    pub fn with_dialing_enabled(mut self, should_dial: bool) -> Self {
        self.should_dial = should_dial;
        self
    }

    /// Build notification configuration.
    pub fn build(mut self) -> (Config, NotificationHandle) {
        Config::new(
            self.protocol_name,
            self.max_notification_size.take().expect("notification size to be specified"),
            self.handshake.take().expect("handshake to be specified"),
            self.fallback_names,
            self.auto_accept_inbound_for_initiated,
            self.sync_channel_size,
            self.async_channel_size,
            self.should_dial,
        )
    }
}
