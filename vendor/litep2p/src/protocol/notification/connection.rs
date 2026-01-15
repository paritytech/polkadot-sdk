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
    protocol::notification::handle::NotificationEventHandle, substream::Substream, PeerId,
};

use bytes::BytesMut;
use futures::{FutureExt, SinkExt, Stream, StreamExt};
use tokio::sync::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use tokio_util::sync::PollSender;

use std::{
    pin::Pin,
    task::{Context, Poll},
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::notification::connection";

/// Bidirectional substream pair representing a connection to a remote peer.
pub(crate) struct Connection {
    /// Remote peer ID.
    peer: PeerId,

    /// Inbound substreams for receiving notifications.
    inbound: Substream,

    /// Outbound substream for sending notifications.
    outbound: Substream,

    /// Handle for sending notification events to user.
    event_handle: NotificationEventHandle,

    /// TX channel used to notify [`NotificationProtocol`](super::NotificationProtocol)
    /// that the connection has been closed.
    conn_closed_tx: Sender<PeerId>,

    /// TX channel for sending notifications.
    notif_tx: PollSender<(PeerId, BytesMut)>,

    /// Receiver for asynchronously sent notifications.
    async_rx: Receiver<Vec<u8>>,

    /// Receiver for synchronously sent notifications.
    sync_rx: Receiver<Vec<u8>>,

    /// Oneshot receiver used by [`NotificationProtocol`](super::NotificationProtocol)
    /// to signal that local node wishes the close the connection.
    rx: oneshot::Receiver<()>,

    /// Next notification to send, if any.
    next_notification: Option<Vec<u8>>,
}

/// Notify [`NotificationProtocol`](super::NotificationProtocol) that the connection was closed.
#[derive(Debug)]
pub enum NotifyProtocol {
    /// Notify the protocol handler.
    Yes,

    /// Do not notify protocol handler.
    No,
}

impl Connection {
    /// Create new [`Connection`].
    pub(crate) fn new(
        peer: PeerId,
        inbound: Substream,
        outbound: Substream,
        event_handle: NotificationEventHandle,
        conn_closed_tx: Sender<PeerId>,
        notif_tx: Sender<(PeerId, BytesMut)>,
        async_rx: Receiver<Vec<u8>>,
        sync_rx: Receiver<Vec<u8>>,
    ) -> (Self, oneshot::Sender<()>) {
        let (tx, rx) = oneshot::channel();

        (
            Self {
                rx,
                peer,
                sync_rx,
                async_rx,
                inbound,
                outbound,
                event_handle,
                conn_closed_tx,
                next_notification: None,
                notif_tx: PollSender::new(notif_tx),
            },
            tx,
        )
    }

    /// Connection closed, clean up state.
    ///
    /// If [`NotificationProtocol`](super::NotificationProtocol) was the one that initiated
    /// shut down, it's not notified of connection getting closed.
    async fn close_connection(self, notify_protocol: NotifyProtocol) {
        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            ?notify_protocol,
            "close notification protocol",
        );

        let _ = self.inbound.close().await;
        let _ = self.outbound.close().await;

        if std::matches!(notify_protocol, NotifyProtocol::Yes) {
            let _ = self.conn_closed_tx.send(self.peer).await;
        }

        self.event_handle.report_notification_stream_closed(self.peer).await;
    }

    pub async fn start(mut self) {
        tracing::debug!(
            target: LOG_TARGET,
            peer = ?self.peer,
            "start connection event loop",
        );

        loop {
            match self.next().await {
                None
                | Some(ConnectionEvent::CloseConnection {
                    notify: NotifyProtocol::Yes,
                }) => return self.close_connection(NotifyProtocol::Yes).await,
                Some(ConnectionEvent::CloseConnection {
                    notify: NotifyProtocol::No,
                }) => return self.close_connection(NotifyProtocol::No).await,
                Some(ConnectionEvent::NotificationReceived { notification }) => {
                    if let Err(_) = self.notif_tx.send_item((self.peer, notification)) {
                        return self.close_connection(NotifyProtocol::Yes).await;
                    }
                }
            }
        }
    }
}

/// Connection events.
pub enum ConnectionEvent {
    /// Close connection.
    ///
    /// If `NotificationProtocol` requested [`Connection`] to be closed, it doesn't need to be
    /// notified. If, on the other hand, connection closes because it encountered an error or one
    /// of the substreams was closed, `NotificationProtocol` must be informed so it can inform the
    /// user.
    CloseConnection {
        /// Whether to notify `NotificationProtocol` or not.
        notify: NotifyProtocol,
    },

    /// Notification read from the inbound substream.
    ///
    /// NOTE: [`Connection`] uses `PollSender::send_item()` to send the notification to user.
    /// `PollSender::poll_reserve()` must be called before calling `PollSender::send_item()` or it
    /// will panic. `PollSender::poll_reserve()` is called in the `Stream` implementation below
    /// before polling the inbound substream to ensure the channel has capacity to receive a
    /// notification.
    NotificationReceived {
        /// Notification.
        notification: BytesMut,
    },
}

impl Stream for Connection {
    type Item = ConnectionEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = Pin::into_inner(self);

        if let Poll::Ready(_) = this.rx.poll_unpin(cx) {
            return Poll::Ready(Some(ConnectionEvent::CloseConnection {
                notify: NotifyProtocol::No,
            }));
        }

        loop {
            let notification = match this.next_notification.take() {
                Some(notification) => Some(notification),
                None => {
                    let future = async {
                        tokio::select! {
                            notification = this.async_rx.recv() => notification,
                            notification = this.sync_rx.recv() => notification,
                        }
                    };
                    futures::pin_mut!(future);

                    match future.poll_unpin(cx) {
                        Poll::Pending => None,
                        Poll::Ready(None) =>
                            return Poll::Ready(Some(ConnectionEvent::CloseConnection {
                                notify: NotifyProtocol::Yes,
                            })),
                        Poll::Ready(Some(notification)) => Some(notification),
                    }
                }
            };

            let Some(notification) = notification else {
                break;
            };

            match this.outbound.poll_ready_unpin(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Pending => {
                    this.next_notification = Some(notification);
                    break;
                }
                Poll::Ready(Err(_)) =>
                    return Poll::Ready(Some(ConnectionEvent::CloseConnection {
                        notify: NotifyProtocol::Yes,
                    })),
            }

            if let Err(_) = this.outbound.start_send_unpin(notification.into()) {
                return Poll::Ready(Some(ConnectionEvent::CloseConnection {
                    notify: NotifyProtocol::Yes,
                }));
            }
        }

        match this.outbound.poll_flush_unpin(cx) {
            Poll::Ready(Err(_)) =>
                return Poll::Ready(Some(ConnectionEvent::CloseConnection {
                    notify: NotifyProtocol::Yes,
                })),
            Poll::Ready(Ok(())) | Poll::Pending => {}
        }

        if let Err(_) = futures::ready!(this.notif_tx.poll_reserve(cx)) {
            return Poll::Ready(Some(ConnectionEvent::CloseConnection {
                notify: NotifyProtocol::Yes,
            }));
        }

        match futures::ready!(this.inbound.poll_next_unpin(cx)) {
            None | Some(Err(_)) => Poll::Ready(Some(ConnectionEvent::CloseConnection {
                notify: NotifyProtocol::Yes,
            })),
            Some(Ok(notification)) =>
                Poll::Ready(Some(ConnectionEvent::NotificationReceived { notification })),
        }
    }
}
