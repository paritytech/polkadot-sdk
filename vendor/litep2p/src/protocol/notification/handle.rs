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
    error::Error,
    protocol::notification::types::{
        Direction, InnerNotificationEvent, NotificationCommand, NotificationError,
        NotificationEvent, ValidationResult,
    },
    types::protocol::ProtocolName,
    PeerId,
};

use bytes::BytesMut;
use futures::Stream;
use parking_lot::RwLock;
use tokio::sync::{
    mpsc::{error::TrySendError, Receiver, Sender},
    oneshot,
};

use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::notification::handle";

#[derive(Debug, Clone)]
pub(crate) struct NotificationEventHandle {
    tx: Sender<InnerNotificationEvent>,
}

impl NotificationEventHandle {
    /// Create new [`NotificationEventHandle`].
    pub(crate) fn new(tx: Sender<InnerNotificationEvent>) -> Self {
        Self { tx }
    }

    /// Validate inbound substream.
    pub(crate) async fn report_inbound_substream(
        &self,
        protocol: ProtocolName,
        fallback: Option<ProtocolName>,
        peer: PeerId,
        handshake: Vec<u8>,
        tx: oneshot::Sender<ValidationResult>,
    ) {
        let _ = self
            .tx
            .send(InnerNotificationEvent::ValidateSubstream {
                protocol,
                fallback,
                peer,
                handshake,
                tx,
            })
            .await;
    }

    /// Notification stream opened.
    pub(crate) async fn report_notification_stream_opened(
        &self,
        protocol: ProtocolName,
        fallback: Option<ProtocolName>,
        direction: Direction,
        peer: PeerId,
        handshake: Vec<u8>,
        sink: NotificationSink,
    ) {
        let _ = self
            .tx
            .send(InnerNotificationEvent::NotificationStreamOpened {
                protocol,
                fallback,
                direction,
                peer,
                handshake,
                sink,
            })
            .await;
    }

    /// Notification stream closed.
    pub(crate) async fn report_notification_stream_closed(&self, peer: PeerId) {
        let _ = self.tx.send(InnerNotificationEvent::NotificationStreamClosed { peer }).await;
    }

    /// Failed to open notification stream.
    pub(crate) async fn report_notification_stream_open_failure(
        &self,
        peer: PeerId,
        error: NotificationError,
    ) {
        let _ = self
            .tx
            .send(InnerNotificationEvent::NotificationStreamOpenFailure { peer, error })
            .await;
    }
}

/// Notification sink.
///
/// Allows the user to send notifications both synchronously and asynchronously.
#[derive(Debug, Clone)]
pub struct NotificationSink {
    /// Peer ID.
    peer: PeerId,

    /// TX channel for sending notifications synchronously.
    sync_tx: Sender<Vec<u8>>,

    /// TX channel for sending notifications asynchronously.
    async_tx: Sender<Vec<u8>>,
}

impl NotificationSink {
    /// Create new [`NotificationSink`].
    pub(crate) fn new(peer: PeerId, sync_tx: Sender<Vec<u8>>, async_tx: Sender<Vec<u8>>) -> Self {
        Self {
            peer,
            async_tx,
            sync_tx,
        }
    }

    /// Send notification to `peer` synchronously.
    ///
    /// If the channel is clogged, [`NotificationError::ChannelClogged`] is returned.
    pub fn send_sync_notification(&self, notification: Vec<u8>) -> Result<(), NotificationError> {
        self.sync_tx.try_send(notification).map_err(|error| match error {
            TrySendError::Closed(_) => NotificationError::NoConnection,
            TrySendError::Full(_) => NotificationError::ChannelClogged,
        })
    }

    /// Send notification to `peer` asynchronously, waiting for the channel to have capacity
    /// if it's clogged.
    ///
    /// Returns [`Error::PeerDoesntExist(PeerId)`](crate::error::Error::PeerDoesntExist)
    /// if the connection has been closed.
    pub async fn send_async_notification(&self, notification: Vec<u8>) -> crate::Result<()> {
        self.async_tx
            .send(notification)
            .await
            .map_err(|_| Error::PeerDoesntExist(self.peer))
    }
}

/// Handle allowing the user protocol to interact with the notification protocol.
#[derive(Debug)]
pub struct NotificationHandle {
    /// RX channel for receiving events from the notification protocol.
    event_rx: Receiver<InnerNotificationEvent>,

    /// RX channel for receiving notifications from connection handlers.
    notif_rx: Receiver<(PeerId, BytesMut)>,

    /// TX channel for sending commands to the notification protocol.
    command_tx: Sender<NotificationCommand>,

    /// Peers.
    peers: HashMap<PeerId, NotificationSink>,

    /// Clogged peers.
    clogged: HashSet<PeerId>,

    /// Pending validations.
    pending_validations: HashMap<PeerId, oneshot::Sender<ValidationResult>>,

    /// Handshake.
    handshake: Arc<RwLock<Vec<u8>>>,
}

impl NotificationHandle {
    /// Create new [`NotificationHandle`].
    pub(crate) fn new(
        event_rx: Receiver<InnerNotificationEvent>,
        notif_rx: Receiver<(PeerId, BytesMut)>,
        command_tx: Sender<NotificationCommand>,
        handshake: Arc<RwLock<Vec<u8>>>,
    ) -> Self {
        Self {
            event_rx,
            notif_rx,
            command_tx,
            handshake,
            peers: HashMap::new(),
            clogged: HashSet::new(),
            pending_validations: HashMap::new(),
        }
    }

    /// Open substream to `peer`.
    ///
    /// Returns [`Error::PeerAlreadyExists(PeerId)`](crate::error::Error::PeerAlreadyExists) if
    /// substream is already open to `peer`.
    ///
    /// If connection to peer is closed, `NotificationProtocol` tries to dial the peer and if the
    /// dial succeeds, tries to open a substream. This behavior can be disabled with
    /// [`ConfigBuilder::with_dialing_enabled(false)`](super::config::ConfigBuilder::with_dialing_enabled()).
    pub async fn open_substream(&self, peer: PeerId) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, "open substream");

        if self.peers.contains_key(&peer) {
            return Err(Error::PeerAlreadyExists(peer));
        }

        self.command_tx
            .send(NotificationCommand::OpenSubstream {
                peers: HashSet::from_iter([peer]),
            })
            .await
            .map_or(Ok(()), |_| Ok(()))
    }

    /// Open substreams to multiple peers.
    ///
    /// Similar to [`NotificationHandle::open_substream()`] but multiple substreams are initiated
    /// using a single call to `NotificationProtocol`.
    ///
    /// Peers who are already connected are ignored and returned as `Err(HashSet<PeerId>>)`.
    pub async fn open_substream_batch(
        &self,
        peers: impl Iterator<Item = PeerId>,
    ) -> Result<(), HashSet<PeerId>> {
        let (to_add, to_ignore): (Vec<_>, Vec<_>) = peers
            .map(|peer| match self.peers.contains_key(&peer) {
                true => (None, Some(peer)),
                false => (Some(peer), None),
            })
            .unzip();

        let to_add = to_add.into_iter().flatten().collect::<HashSet<_>>();
        let to_ignore = to_ignore.into_iter().flatten().collect::<HashSet<_>>();

        tracing::trace!(
            target: LOG_TARGET,
            peers_to_add = ?to_add.len(),
            peers_to_ignore = ?to_ignore.len(),
            "open substream",
        );

        let _ = self.command_tx.send(NotificationCommand::OpenSubstream { peers: to_add }).await;

        match to_ignore.is_empty() {
            true => Ok(()),
            false => Err(to_ignore),
        }
    }

    /// Try to open substreams to multiple peers.
    ///
    /// Similar to [`NotificationHandle::open_substream()`] but multiple substreams are initiated
    /// using a single call to `NotificationProtocol`.
    ///
    /// If the channel is clogged, peers for whom a connection is not yet open are returned as
    /// `Err(HashSet<PeerId>)`.
    pub fn try_open_substream_batch(
        &self,
        peers: impl Iterator<Item = PeerId>,
    ) -> Result<(), HashSet<PeerId>> {
        let (to_add, to_ignore): (Vec<_>, Vec<_>) = peers
            .map(|peer| match self.peers.contains_key(&peer) {
                true => (None, Some(peer)),
                false => (Some(peer), None),
            })
            .unzip();

        let to_add = to_add.into_iter().flatten().collect::<HashSet<_>>();
        let to_ignore = to_ignore.into_iter().flatten().collect::<HashSet<_>>();

        tracing::trace!(
            target: LOG_TARGET,
            peers_to_add = ?to_add.len(),
            peers_to_ignore = ?to_ignore.len(),
            "open substream",
        );

        self.command_tx
            .try_send(NotificationCommand::OpenSubstream {
                peers: to_add.clone(),
            })
            .map_err(|_| to_add)
    }

    /// Close substream to `peer`.
    pub async fn close_substream(&self, peer: PeerId) {
        tracing::trace!(target: LOG_TARGET, ?peer, "close substream");

        if !self.peers.contains_key(&peer) {
            return;
        }

        let _ = self
            .command_tx
            .send(NotificationCommand::CloseSubstream {
                peers: HashSet::from_iter([peer]),
            })
            .await;
    }

    /// Close substream to multiple peers.
    ///
    /// Similar to [`NotificationHandle::close_substream()`] but multiple substreams are closed
    /// using a single call to `NotificationProtocol`.
    pub async fn close_substream_batch(&self, peers: impl Iterator<Item = PeerId>) {
        let peers = peers.filter(|peer| self.peers.contains_key(peer)).collect::<HashSet<_>>();

        if peers.is_empty() {
            return;
        }

        tracing::trace!(
            target: LOG_TARGET,
            ?peers,
            "close substreams",
        );

        let _ = self.command_tx.send(NotificationCommand::CloseSubstream { peers }).await;
    }

    /// Try close substream to multiple peers.
    ///
    /// Similar to [`NotificationHandle::close_substream()`] but multiple substreams are closed
    /// using a single call to `NotificationProtocol`.
    ///
    /// If the channel is clogged, `peers` is returned as `Err(HashSet<PeerId>)`.
    ///
    /// If `peers` is empty after filtering all already-connected peers,
    /// `Err(HashMap::new())` is returned.
    pub fn try_close_substream_batch(
        &self,
        peers: impl Iterator<Item = PeerId>,
    ) -> Result<(), HashSet<PeerId>> {
        let peers = peers.filter(|peer| self.peers.contains_key(peer)).collect::<HashSet<_>>();

        if peers.is_empty() {
            return Err(HashSet::new());
        }

        tracing::trace!(
            target: LOG_TARGET,
            ?peers,
            "close substreams",
        );

        self.command_tx
            .try_send(NotificationCommand::CloseSubstream {
                peers: peers.clone(),
            })
            .map_err(|_| peers)
    }

    /// Set new handshake.
    pub fn set_handshake(&mut self, handshake: Vec<u8>) {
        tracing::trace!(target: LOG_TARGET, ?handshake, "set handshake");

        *self.handshake.write() = handshake;
    }

    /// Send validation result to the notification protocol for an inbound substream received from
    /// `peer`.
    pub fn send_validation_result(&mut self, peer: PeerId, result: ValidationResult) {
        tracing::trace!(target: LOG_TARGET, ?peer, ?result, "send validation result");

        self.pending_validations.remove(&peer).map(|tx| tx.send(result));
    }

    /// Send notification to `peer` synchronously.
    ///
    /// If the channel is clogged, [`NotificationError::ChannelClogged`] is returned.
    pub fn send_sync_notification(
        &mut self,
        peer: PeerId,
        notification: Vec<u8>,
    ) -> Result<(), NotificationError> {
        match self.peers.get_mut(&peer) {
            Some(sink) => match sink.send_sync_notification(notification) {
                Ok(()) => Ok(()),
                Err(error) => match error {
                    NotificationError::NoConnection => Err(NotificationError::NoConnection),
                    NotificationError::ChannelClogged => {
                        let _ = self.clogged.insert(peer).then(|| {
                            self.command_tx.try_send(NotificationCommand::ForceClose { peer })
                        });

                        Err(NotificationError::ChannelClogged)
                    }
                    // sink doesn't emit any other `NotificationError`s
                    _ => unreachable!(),
                },
            },
            None => Ok(()),
        }
    }

    /// Send notification to `peer` asynchronously, waiting for the channel to have capacity
    /// if it's clogged.
    ///
    /// Returns [`Error::PeerDoesntExist(PeerId)`](crate::error::Error::PeerDoesntExist) if the
    /// connection has been closed.
    pub async fn send_async_notification(
        &mut self,
        peer: PeerId,
        notification: Vec<u8>,
    ) -> crate::Result<()> {
        match self.peers.get_mut(&peer) {
            Some(sink) => sink.send_async_notification(notification).await,
            None => Err(Error::PeerDoesntExist(peer)),
        }
    }

    /// Get a copy of the underlying notification sink for the peer.
    ///
    /// `None` is returned if `peer` doesn't exist.
    pub fn notification_sink(&self, peer: PeerId) -> Option<NotificationSink> {
        self.peers.get(&peer).cloned()
    }

    #[cfg(feature = "fuzz")]
    /// Expose functionality for fuzzing
    pub async fn fuzz_send_message(&mut self, command: NotificationCommand) -> crate::Result<()> {
        if let NotificationCommand::SendNotification { peer_id, notif } = command {
            self.send_async_notification(peer_id, notif).await?;
        } else {
            let _ = self.command_tx.send(command).await;
        }
        Ok(())
    }
}

impl Stream for NotificationHandle {
    type Item = NotificationEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.event_rx.poll_recv(cx) {
                Poll::Pending => {}
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Ready(Some(event)) => match event {
                    InnerNotificationEvent::NotificationStreamOpened {
                        protocol,
                        fallback,
                        direction,
                        peer,
                        handshake,
                        sink,
                    } => {
                        self.peers.insert(peer, sink);

                        return Poll::Ready(Some(NotificationEvent::NotificationStreamOpened {
                            protocol,
                            fallback,
                            direction,
                            peer,
                            handshake,
                        }));
                    }
                    InnerNotificationEvent::NotificationStreamClosed { peer } => {
                        self.peers.remove(&peer);
                        self.clogged.remove(&peer);

                        return Poll::Ready(Some(NotificationEvent::NotificationStreamClosed {
                            peer,
                        }));
                    }
                    InnerNotificationEvent::ValidateSubstream {
                        protocol,
                        fallback,
                        peer,
                        handshake,
                        tx,
                    } => {
                        self.pending_validations.insert(peer, tx);

                        return Poll::Ready(Some(NotificationEvent::ValidateSubstream {
                            protocol,
                            fallback,
                            peer,
                            handshake,
                        }));
                    }
                    InnerNotificationEvent::NotificationStreamOpenFailure { peer, error } =>
                        return Poll::Ready(Some(
                            NotificationEvent::NotificationStreamOpenFailure { peer, error },
                        )),
                },
            }

            match futures::ready!(self.notif_rx.poll_recv(cx)) {
                None => return Poll::Ready(None),
                Some((peer, notification)) =>
                    if self.peers.contains_key(&peer) {
                        return Poll::Ready(Some(NotificationEvent::NotificationReceived {
                            peer,
                            notification,
                        }));
                    },
            }
        }
    }
}
