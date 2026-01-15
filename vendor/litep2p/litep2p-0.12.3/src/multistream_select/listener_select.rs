// Copyright 2017 Parity Technologies (UK) Ltd.
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

//! Protocol negotiation strategies for the peer acting as the listener
//! in a multistream-select protocol negotiation.

use crate::{
    codec::unsigned_varint::UnsignedVarint,
    error::{self, Error},
    multistream_select::{
        drain_trailing_protocols,
        protocol::{
            webrtc_encode_multistream_message, HeaderLine, Message, MessageIO, Protocol,
            ProtocolError, PROTO_MULTISTREAM_1_0,
        },
        Negotiated, NegotiationError,
    },
    types::protocol::ProtocolName,
};

use bytes::{Bytes, BytesMut};
use futures::prelude::*;
use smallvec::SmallVec;
use std::{
    convert::TryFrom as _,
    iter::FromIterator,
    mem,
    pin::Pin,
    task::{Context, Poll},
};

const LOG_TARGET: &str = "litep2p::multistream-select";

/// Returns a `Future` that negotiates a protocol on the given I/O stream
/// for a peer acting as the _listener_ (or _responder_).
///
/// This function is given an I/O stream and a list of protocols and returns a
/// computation that performs the protocol negotiation with the remote. The
/// returned `Future` resolves with the name of the negotiated protocol and
/// a [`Negotiated`] I/O stream.
pub fn listener_select_proto<R, I>(inner: R, protocols: I) -> ListenerSelectFuture<R, I::Item>
where
    R: AsyncRead + AsyncWrite,
    I: IntoIterator,
    I::Item: AsRef<[u8]>,
{
    let protocols = protocols.into_iter().filter_map(|n| match Protocol::try_from(n.as_ref()) {
        Ok(p) => Some((n, p)),
        Err(e) => {
            tracing::warn!(
                target: LOG_TARGET,
                "Listener: Ignoring invalid protocol: {} due to {}",
                String::from_utf8_lossy(n.as_ref()),
                e
            );
            None
        }
    });
    ListenerSelectFuture {
        protocols: SmallVec::from_iter(protocols),
        state: State::RecvHeader {
            io: MessageIO::new(inner),
        },
        last_sent_na: false,
    }
}

/// The `Future` returned by [`listener_select_proto`] that performs a
/// multistream-select protocol negotiation on an underlying I/O stream.
#[pin_project::pin_project]
pub struct ListenerSelectFuture<R, N> {
    protocols: SmallVec<[(N, Protocol); 8]>,
    state: State<R, N>,
    /// Whether the last message sent was a protocol rejection (i.e. `na\n`).
    ///
    /// If the listener reads garbage or EOF after such a rejection,
    /// the dialer is likely using `V1Lazy` and negotiation must be
    /// considered failed, but not with a protocol violation or I/O
    /// error.
    last_sent_na: bool,
}

enum State<R, N> {
    RecvHeader {
        io: MessageIO<R>,
    },
    SendHeader {
        io: MessageIO<R>,
    },
    RecvMessage {
        io: MessageIO<R>,
    },
    SendMessage {
        io: MessageIO<R>,
        message: Message,
        protocol: Option<N>,
    },
    Flush {
        io: MessageIO<R>,
        protocol: Option<N>,
    },
    Done,
}

impl<R, N> Future for ListenerSelectFuture<R, N>
where
    // The Unpin bound here is required because we
    // produce a `Negotiated<R>` as the output.
    // It also makes the implementation considerably
    // easier to write.
    R: AsyncRead + AsyncWrite + Unpin,
    N: AsRef<[u8]> + Clone,
{
    type Output = Result<(N, Negotiated<R>), NegotiationError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        loop {
            match mem::replace(this.state, State::Done) {
                State::RecvHeader { mut io } => {
                    match io.poll_next_unpin(cx) {
                        Poll::Ready(Some(Ok(Message::Header(h)))) => match h {
                            HeaderLine::V1 => *this.state = State::SendHeader { io },
                        },
                        Poll::Ready(Some(Ok(_))) =>
                            return Poll::Ready(Err(ProtocolError::InvalidMessage.into())),
                        Poll::Ready(Some(Err(err))) => return Poll::Ready(Err(From::from(err))),
                        // Treat EOF error as [`NegotiationError::Failed`], not as
                        // [`NegotiationError::ProtocolError`], allowing dropping or closing an I/O
                        // stream as a permissible way to "gracefully" fail a negotiation.
                        Poll::Ready(None) => return Poll::Ready(Err(NegotiationError::Failed)),
                        Poll::Pending => {
                            *this.state = State::RecvHeader { io };
                            return Poll::Pending;
                        }
                    }
                }

                State::SendHeader { mut io } => {
                    match Pin::new(&mut io).poll_ready(cx) {
                        Poll::Pending => {
                            *this.state = State::SendHeader { io };
                            return Poll::Pending;
                        }
                        Poll::Ready(Ok(())) => {}
                        Poll::Ready(Err(err)) => return Poll::Ready(Err(From::from(err))),
                    }

                    let msg = Message::Header(HeaderLine::V1);
                    if let Err(err) = Pin::new(&mut io).start_send(msg) {
                        return Poll::Ready(Err(From::from(err)));
                    }

                    *this.state = State::Flush { io, protocol: None };
                }

                State::RecvMessage { mut io } => {
                    let msg = match Pin::new(&mut io).poll_next(cx) {
                        Poll::Ready(Some(Ok(msg))) => msg,
                        // Treat EOF error as [`NegotiationError::Failed`], not as
                        // [`NegotiationError::ProtocolError`], allowing dropping or closing an I/O
                        // stream as a permissible way to "gracefully" fail a negotiation.
                        //
                        // This is e.g. important when a listener rejects a protocol with
                        // [`Message::NotAvailable`] and the dialer does not have alternative
                        // protocols to propose. Then the dialer will stop the negotiation and drop
                        // the corresponding stream. As a listener this EOF should be interpreted as
                        // a failed negotiation.
                        Poll::Ready(None) => return Poll::Ready(Err(NegotiationError::Failed)),
                        Poll::Pending => {
                            *this.state = State::RecvMessage { io };
                            return Poll::Pending;
                        }
                        Poll::Ready(Some(Err(err))) => {
                            if *this.last_sent_na {
                                // When we read garbage or EOF after having already rejected a
                                // protocol, the dialer is most likely using `V1Lazy` and has
                                // optimistically settled on this protocol, so this is really a
                                // failed negotiation, not a protocol violation. In this case
                                // the dialer also raises `NegotiationError::Failed` when finally
                                // reading the `N/A` response.
                                if let ProtocolError::InvalidMessage = &err {
                                    tracing::trace!(
                                        target: LOG_TARGET,
                                        "Listener: Negotiation failed with invalid \
                                        message after protocol rejection."
                                    );
                                    return Poll::Ready(Err(NegotiationError::Failed));
                                }
                                if let ProtocolError::IoError(e) = &err {
                                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                                        tracing::trace!(
                                            target: LOG_TARGET,
                                            "Listener: Negotiation failed with EOF \
                                            after protocol rejection."
                                        );
                                        return Poll::Ready(Err(NegotiationError::Failed));
                                    }
                                }
                            }

                            return Poll::Ready(Err(From::from(err)));
                        }
                    };

                    match msg {
                        Message::ListProtocols => {
                            let supported =
                                this.protocols.iter().map(|(_, p)| p).cloned().collect();
                            let message = Message::Protocols(supported);
                            *this.state = State::SendMessage {
                                io,
                                message,
                                protocol: None,
                            }
                        }
                        Message::Protocol(p) => {
                            let protocol = this.protocols.iter().find_map(|(name, proto)| {
                                if &p == proto {
                                    Some(name.clone())
                                } else {
                                    None
                                }
                            });

                            let message = if protocol.is_some() {
                                tracing::debug!("Listener: confirming protocol: {}", p);
                                Message::Protocol(p.clone())
                            } else {
                                tracing::debug!(
                                    "Listener: rejecting protocol: {}",
                                    String::from_utf8_lossy(p.as_ref())
                                );
                                Message::NotAvailable
                            };

                            *this.state = State::SendMessage {
                                io,
                                message,
                                protocol,
                            };
                        }
                        _ => return Poll::Ready(Err(ProtocolError::InvalidMessage.into())),
                    }
                }

                State::SendMessage {
                    mut io,
                    message,
                    protocol,
                } => {
                    match Pin::new(&mut io).poll_ready(cx) {
                        Poll::Pending => {
                            *this.state = State::SendMessage {
                                io,
                                message,
                                protocol,
                            };
                            return Poll::Pending;
                        }
                        Poll::Ready(Ok(())) => {}
                        Poll::Ready(Err(err)) => return Poll::Ready(Err(From::from(err))),
                    }

                    if let Message::NotAvailable = &message {
                        *this.last_sent_na = true;
                    } else {
                        *this.last_sent_na = false;
                    }

                    if let Err(err) = Pin::new(&mut io).start_send(message) {
                        return Poll::Ready(Err(From::from(err)));
                    }

                    *this.state = State::Flush { io, protocol };
                }

                State::Flush { mut io, protocol } => {
                    match Pin::new(&mut io).poll_flush(cx) {
                        Poll::Pending => {
                            *this.state = State::Flush { io, protocol };
                            return Poll::Pending;
                        }
                        Poll::Ready(Ok(())) => {
                            // If a protocol has been selected, finish negotiation.
                            // Otherwise expect to receive another message.
                            match protocol {
                                Some(protocol) => {
                                    tracing::debug!(
                                        "Listener: sent confirmed protocol: {}",
                                        String::from_utf8_lossy(protocol.as_ref())
                                    );
                                    let io = Negotiated::completed(io.into_inner());
                                    return Poll::Ready(Ok((protocol, io)));
                                }
                                None => *this.state = State::RecvMessage { io },
                            }
                        }
                        Poll::Ready(Err(err)) => return Poll::Ready(Err(From::from(err))),
                    }
                }

                State::Done => panic!("State::poll called after completion"),
            }
        }
    }
}

/// Result of [`webrtc_listener_negotiate()`].
#[derive(Debug)]
pub enum ListenerSelectResult {
    /// Requested protocol is available and substream can be accepted.
    Accepted {
        /// Protocol that is confirmed.
        protocol: ProtocolName,

        /// `multistream-select` message.
        message: BytesMut,
    },

    /// Requested protocol is not available.
    Rejected {
        /// `multistream-select` message.
        message: BytesMut,
    },
}

/// Negotiate protocols for listener.
///
/// Parse protocols offered by the remote peer and check if any of the offered protocols match
/// locally available protocols. If a match is found, return an encoded multistream-select
/// response and the negotiated protocol. If parsing fails or no match is found, return an error.
pub fn webrtc_listener_negotiate<'a>(
    supported_protocols: &'a mut impl Iterator<Item = &'a ProtocolName>,
    mut payload: Bytes,
) -> crate::Result<ListenerSelectResult> {
    let protocols = drain_trailing_protocols(payload)?;
    let mut protocol_iter = protocols.into_iter();

    // skip the multistream-select header because it's not part of user protocols but verify it's
    // present
    if protocol_iter.next() != Some(PROTO_MULTISTREAM_1_0) {
        return Err(Error::NegotiationError(
            error::NegotiationError::MultistreamSelectError(NegotiationError::Failed),
        ));
    }

    for protocol in protocol_iter {
        tracing::trace!(
            target: LOG_TARGET,
            protocol = ?std::str::from_utf8(protocol.as_ref()),
            "listener: checking protocol",
        );

        for supported in &mut *supported_protocols {
            if protocol.as_ref() == supported.as_bytes() {
                return Ok(ListenerSelectResult::Accepted {
                    protocol: supported.clone(),
                    message: webrtc_encode_multistream_message(std::iter::once(
                        Message::Protocol(protocol),
                    ))?,
                });
            }
        }
    }

    tracing::trace!(
        target: LOG_TARGET,
        "listener: handshake rejected, no supported protocol found",
    );

    Ok(ListenerSelectResult::Rejected {
        message: webrtc_encode_multistream_message(std::iter::once(Message::NotAvailable))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error;
    use bytes::BufMut;

    #[test]
    fn webrtc_listener_negotiate_works() {
        let mut local_protocols = [
            ProtocolName::from("/13371338/proto/1"),
            ProtocolName::from("/sup/proto/1"),
            ProtocolName::from("/13371338/proto/2"),
            ProtocolName::from("/13371338/proto/3"),
            ProtocolName::from("/13371338/proto/4"),
        ];
        let message = webrtc_encode_multistream_message(vec![
            Message::Protocol(Protocol::try_from(&b"/13371338/proto/1"[..]).unwrap()),
            Message::Protocol(Protocol::try_from(&b"/sup/proto/1"[..]).unwrap()),
        ])
        .unwrap()
        .freeze();

        match webrtc_listener_negotiate(&mut local_protocols.iter(), message) {
            Err(error) => panic!("error received: {error:?}"),
            Ok(ListenerSelectResult::Rejected { .. }) => panic!("message rejected"),
            Ok(ListenerSelectResult::Accepted { protocol, message }) => {
                assert_eq!(protocol, ProtocolName::from("/13371338/proto/1"));
            }
        }
    }

    #[test]
    fn invalid_message() {
        let mut local_protocols = [
            ProtocolName::from("/13371338/proto/1"),
            ProtocolName::from("/sup/proto/1"),
            ProtocolName::from("/13371338/proto/2"),
            ProtocolName::from("/13371338/proto/3"),
            ProtocolName::from("/13371338/proto/4"),
        ];
        // The invalid message is really two multistream-select messages inside one `WebRtcMessage`:
        // 1. the multistream-select header
        // 2. an "ls response" message (that does not contain another header)
        //
        // This is invalid for two reasons:
        // 1. It is malformed. Either the header is followed by one or more `Message::Protocol`
        //    instances or the header is part of the "ls response".
        // 2. This sequence of messages is not spec compliant. A listener receives one of the
        //    following on an inbound substream:
        //      - a multistream-select header followed by a `Message::Protocol` instance
        //      - a multistream-select header followed by an "ls" message (<length prefix><ls><\n>)
        //
        // `webrtc_listener_negotiate()` should reject this invalid message. The error can either be
        // `InvalidData` because the message is malformed or `StateMismatch` because the message is
        // not expected at this point in the protocol.
        let message = webrtc_encode_multistream_message(std::iter::once(Message::Protocols(vec![
            Protocol::try_from(&b"/13371338/proto/1"[..]).unwrap(),
            Protocol::try_from(&b"/sup/proto/1"[..]).unwrap(),
        ])))
        .unwrap()
        .freeze();

        match webrtc_listener_negotiate(&mut local_protocols.iter(), message) {
            Err(error) => assert!(std::matches!(
                error,
                // something has gone off the rails here...
                Error::NegotiationError(error::NegotiationError::ParseError(
                    error::ParseError::InvalidData
                )),
            )),
            _ => panic!("invalid event"),
        }
    }

    #[test]
    fn only_header_line_received() {
        let mut local_protocols = [
            ProtocolName::from("/13371338/proto/1"),
            ProtocolName::from("/sup/proto/1"),
            ProtocolName::from("/13371338/proto/2"),
            ProtocolName::from("/13371338/proto/3"),
            ProtocolName::from("/13371338/proto/4"),
        ];

        // send only header line
        let mut bytes = BytesMut::with_capacity(32);
        let message = Message::Header(HeaderLine::V1);
        message.encode(&mut bytes).map_err(|_| Error::InvalidData).unwrap();

        match webrtc_listener_negotiate(&mut local_protocols.iter(), bytes.freeze()) {
            Err(error) => assert!(std::matches!(
                error,
                Error::NegotiationError(error::NegotiationError::ParseError(
                    error::ParseError::InvalidData
                )),
            )),
            event => panic!("invalid event: {event:?}"),
        }
    }

    #[test]
    fn header_line_missing() {
        let mut local_protocols = [
            ProtocolName::from("/13371338/proto/1"),
            ProtocolName::from("/sup/proto/1"),
            ProtocolName::from("/13371338/proto/2"),
            ProtocolName::from("/13371338/proto/3"),
            ProtocolName::from("/13371338/proto/4"),
        ];

        // header line missing
        let mut bytes = BytesMut::with_capacity(256);
        vec![&b"/13371338/proto/1"[..], &b"/sup/proto/1"[..]]
            .into_iter()
            .for_each(|proto| {
                bytes.put_u8((proto.len() + 1) as u8);

                Message::Protocol(Protocol::try_from(proto).unwrap())
                    .encode(&mut bytes)
                    .unwrap();
            });

        match webrtc_listener_negotiate(&mut local_protocols.iter(), bytes.freeze()) {
            Err(error) => assert!(std::matches!(
                error,
                Error::NegotiationError(error::NegotiationError::MultistreamSelectError(
                    NegotiationError::Failed
                ))
            )),
            event => panic!("invalid event: {event:?}"),
        }
    }

    #[test]
    fn protocol_not_supported() {
        let mut local_protocols = [
            ProtocolName::from("/13371338/proto/1"),
            ProtocolName::from("/sup/proto/1"),
            ProtocolName::from("/13371338/proto/2"),
            ProtocolName::from("/13371338/proto/3"),
            ProtocolName::from("/13371338/proto/4"),
        ];
        let message = webrtc_encode_multistream_message(vec![Message::Protocol(
            Protocol::try_from(&b"/13371339/proto/1"[..]).unwrap(),
        )])
        .unwrap()
        .freeze();

        match webrtc_listener_negotiate(&mut local_protocols.iter(), message) {
            Err(error) => panic!("error received: {error:?}"),
            Ok(ListenerSelectResult::Rejected { message }) => {
                assert_eq!(
                    message,
                    webrtc_encode_multistream_message(std::iter::once(Message::NotAvailable))
                        .unwrap()
                );
            }
            Ok(ListenerSelectResult::Accepted { protocol, message }) => panic!("message accepted"),
        }
    }
}
