// Copyright (c) 2018-2019 Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 or MIT license, at your option.
//
// A copy of the Apache License, Version 2.0 is included in the software as
// LICENSE-APACHE and a copy of the MIT license is included in the software
// as LICENSE-MIT. You may also obtain a copy of the Apache License, Version 2.0
// at https://www.apache.org/licenses/LICENSE-2.0 and a copy of the MIT license
// at https://opensource.org/licenses/MIT.

use crate::yamux::{Connection, ConnectionError, Result, Stream, MAX_ACK_BACKLOG};

use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

const LOG_TARGET: &str = "litep2p::yamux::control";

/// A Yamux [`Connection`] controller.
///
/// This presents an alternative API for using a yamux [`Connection`].
///
/// A [`Control`] communicates with a [`ControlledConnection`] via a channel. This allows
/// a [`Control`] to be cloned and shared between tasks and threads.
#[derive(Clone, Debug)]
pub struct Control {
    /// Command channel to [`ControlledConnection`].
    sender: mpsc::Sender<ControlCommand>,
}

impl Control {
    pub fn new<T>(connection: Connection<T>) -> (Self, ControlledConnection<T>) {
        let (sender, receiver) = mpsc::channel(MAX_ACK_BACKLOG);

        let control = Control { sender };
        let connection = ControlledConnection {
            state: State::Idle(connection),
            commands: receiver,
        };

        (control, connection)
    }

    /// Open a new stream to the remote.
    pub async fn open_stream(&mut self) -> Result<Stream> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(ControlCommand::OpenStream(tx)).await?;
        rx.await?
    }

    /// Close the connection.
    pub async fn close(&mut self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        if self.sender.send(ControlCommand::CloseConnection(tx)).await.is_err() {
            // The receiver is closed which means the connection is already closed.
            return Ok(());
        }
        // A dropped `oneshot::Sender` means the `Connection` is gone,
        // so we do not treat receive errors differently here.
        let _ = rx.await;
        Ok(())
    }
}

/// Wraps a [`Connection`] which can be controlled with a [`Control`].
pub struct ControlledConnection<T> {
    state: State<T>,
    commands: mpsc::Receiver<ControlCommand>,
}

impl<T> ControlledConnection<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<Stream>>> {
        loop {
            match std::mem::replace(&mut self.state, State::Poisoned) {
                State::Idle(mut connection) => {
                    match connection.poll_next_inbound(cx) {
                        Poll::Ready(maybe_stream) => {
                            // Transport layers will close the connection on the first
                            // substream error. The `connection.poll_next_inbound` should
                            // not be called again after returning an error. Instead, we
                            // must close the connection gracefully.
                            match maybe_stream.as_ref() {
                                Some(Err(error)) => {
                                    tracing::debug!(target: LOG_TARGET, ?error, "Inbound stream error, closing connection");

                                    self.state = State::Closing {
                                        reply: None,
                                        inner: Closing::DrainingControlCommands { connection },
                                    };
                                }
                                other => {
                                    tracing::debug!(target: LOG_TARGET, ?other, "Inbound stream reset state to idle");
                                    self.state = State::Idle(connection)
                                }
                            }

                            return Poll::Ready(maybe_stream);
                        }
                        Poll::Pending => {}
                    }

                    match self.commands.poll_next_unpin(cx) {
                        Poll::Ready(Some(ControlCommand::OpenStream(reply))) => {
                            self.state = State::OpeningNewStream { reply, connection };
                            continue;
                        }
                        Poll::Ready(Some(ControlCommand::CloseConnection(reply))) => {
                            self.commands.close();

                            self.state = State::Closing {
                                reply: Some(reply),
                                inner: Closing::DrainingControlCommands { connection },
                            };
                            continue;
                        }
                        Poll::Ready(None) => {
                            // Last `Control` sender was dropped, close te connection.
                            self.state = State::Closing {
                                reply: None,
                                inner: Closing::ClosingConnection { connection },
                            };
                            continue;
                        }
                        Poll::Pending => {}
                    }

                    self.state = State::Idle(connection);
                    return Poll::Pending;
                }
                State::OpeningNewStream {
                    reply,
                    mut connection,
                } => match connection.poll_new_outbound(cx) {
                    Poll::Ready(stream) => {
                        let _ = reply.send(stream);

                        self.state = State::Idle(connection);
                        continue;
                    }
                    Poll::Pending => {
                        self.state = State::OpeningNewStream { reply, connection };
                        return Poll::Pending;
                    }
                },
                State::Closing {
                    reply,
                    inner: Closing::DrainingControlCommands { connection },
                } => match self.commands.poll_next_unpin(cx) {
                    Poll::Ready(Some(ControlCommand::OpenStream(new_reply))) => {
                        let _ = new_reply.send(Err(ConnectionError::Closed));

                        self.state = State::Closing {
                            reply,
                            inner: Closing::DrainingControlCommands { connection },
                        };
                        continue;
                    }
                    Poll::Ready(Some(ControlCommand::CloseConnection(new_reply))) => {
                        let _ = new_reply.send(());

                        self.state = State::Closing {
                            reply,
                            inner: Closing::DrainingControlCommands { connection },
                        };
                        continue;
                    }
                    Poll::Ready(None) => {
                        self.state = State::Closing {
                            reply,
                            inner: Closing::ClosingConnection { connection },
                        };
                        continue;
                    }
                    Poll::Pending => {
                        self.state = State::Closing {
                            reply,
                            inner: Closing::DrainingControlCommands { connection },
                        };
                        return Poll::Pending;
                    }
                },
                State::Closing {
                    reply,
                    inner: Closing::ClosingConnection { mut connection },
                } => match connection.poll_close(cx) {
                    Poll::Ready(Ok(())) | Poll::Ready(Err(ConnectionError::Closed)) => {
                        if let Some(reply) = reply {
                            let _ = reply.send(());
                        }
                        return Poll::Ready(None);
                    }
                    Poll::Ready(Err(other)) => {
                        if let Some(reply) = reply {
                            let _ = reply.send(());
                        }
                        return Poll::Ready(Some(Err(other)));
                    }
                    Poll::Pending => {
                        self.state = State::Closing {
                            reply,
                            inner: Closing::ClosingConnection { connection },
                        };
                        return Poll::Pending;
                    }
                },
                State::Poisoned => return Poll::Pending,
            }
        }
    }
}

#[derive(Debug)]
enum ControlCommand {
    /// Open a new stream to the remote end.
    OpenStream(oneshot::Sender<Result<Stream>>),
    /// Close the whole connection.
    CloseConnection(oneshot::Sender<()>),
}

/// The state of a [`ControlledConnection`].
enum State<T> {
    Idle(Connection<T>),
    OpeningNewStream {
        reply: oneshot::Sender<Result<Stream>>,
        connection: Connection<T>,
    },
    Closing {
        /// A channel to the [`Control`] in case the close was requested. `None` if we are closing
        /// because the last [`Control`] was dropped.
        reply: Option<oneshot::Sender<()>>,
        inner: Closing<T>,
    },
    Poisoned,
}

/// A sub-state of our larger state machine for a [`ControlledConnection`].
///
/// Closing connection involves two steps:
///
/// 1. Draining and answered all remaining [`Closing::DrainingControlCommands`].
/// 1. Closing the underlying [`Connection`].
enum Closing<T> {
    DrainingControlCommands { connection: Connection<T> },
    ClosingConnection { connection: Connection<T> },
}

impl<T> futures::Stream for ControlledConnection<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Item = Result<Stream>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().poll_next(cx)
    }
}
