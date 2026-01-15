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

//! Protocol negotiation strategies for the peer acting as the dialer.

use crate::{
    codec::unsigned_varint::UnsignedVarint,
    error::{self, Error, ParseError, SubstreamError},
    multistream_select::{
        drain_trailing_protocols,
        protocol::{
            webrtc_encode_multistream_message, HeaderLine, Message, MessageIO, Protocol,
            ProtocolError, PROTO_MULTISTREAM_1_0,
        },
        Negotiated, NegotiationError, Version,
    },
    types::protocol::ProtocolName,
};

use bytes::{Bytes, BytesMut};
use futures::prelude::*;
use std::{
    convert::TryFrom as _,
    iter, mem,
    pin::Pin,
    task::{Context, Poll},
};

const LOG_TARGET: &str = "litep2p::multistream-select";

/// Returns a `Future` that negotiates a protocol on the given I/O stream
/// for a peer acting as the _dialer_ (or _initiator_).
///
/// This function is given an I/O stream and a list of protocols and returns a
/// computation that performs the protocol negotiation with the remote. The
/// returned `Future` resolves with the name of the negotiated protocol and
/// a [`Negotiated`] I/O stream.
///
/// Within the scope of this library, a dialer always commits to a specific
/// multistream-select [`Version`], whereas a listener always supports
/// all versions supported by this library. Frictionless multistream-select
/// protocol upgrades may thus proceed by deployments with updated listeners,
/// eventually followed by deployments of dialers choosing the newer protocol.
pub fn dialer_select_proto<R, I>(
    inner: R,
    protocols: I,
    version: Version,
) -> DialerSelectFuture<R, I::IntoIter>
where
    R: AsyncRead + AsyncWrite,
    I: IntoIterator,
    I::Item: AsRef<[u8]>,
{
    let protocols = protocols.into_iter().peekable();
    DialerSelectFuture {
        version,
        protocols,
        state: State::SendHeader {
            io: MessageIO::new(inner),
        },
    }
}

/// A `Future` returned by [`dialer_select_proto`] which negotiates
/// a protocol iteratively by considering one protocol after the other.
#[pin_project::pin_project]
pub struct DialerSelectFuture<R, I: Iterator> {
    protocols: iter::Peekable<I>,
    state: State<R, I::Item>,
    version: Version,
}

enum State<R, N> {
    SendHeader {
        io: MessageIO<R>,
    },
    SendProtocol {
        io: MessageIO<R>,
        protocol: N,
        header_received: bool,
    },
    FlushProtocol {
        io: MessageIO<R>,
        protocol: N,
        header_received: bool,
    },
    AwaitProtocol {
        io: MessageIO<R>,
        protocol: N,
        header_received: bool,
    },
    Done,
}

impl<R, I> Future for DialerSelectFuture<R, I>
where
    // The Unpin bound here is required because we produce
    // a `Negotiated<R>` as the output. It also makes
    // the implementation considerably easier to write.
    R: AsyncRead + AsyncWrite + Unpin,
    I: Iterator,
    I::Item: AsRef<[u8]>,
{
    type Output = Result<(I::Item, Negotiated<R>), NegotiationError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        loop {
            match mem::replace(this.state, State::Done) {
                State::SendHeader { mut io } => {
                    match Pin::new(&mut io).poll_ready(cx)? {
                        Poll::Ready(()) => {}
                        Poll::Pending => {
                            *this.state = State::SendHeader { io };
                            return Poll::Pending;
                        }
                    }

                    let h = HeaderLine::from(*this.version);
                    if let Err(err) = Pin::new(&mut io).start_send(Message::Header(h)) {
                        return Poll::Ready(Err(From::from(err)));
                    }

                    let protocol = this.protocols.next().ok_or(NegotiationError::Failed)?;

                    // The dialer always sends the header and the first protocol
                    // proposal in one go for efficiency.
                    *this.state = State::SendProtocol {
                        io,
                        protocol,
                        header_received: false,
                    };
                }

                State::SendProtocol {
                    mut io,
                    protocol,
                    header_received,
                } => {
                    match Pin::new(&mut io).poll_ready(cx)? {
                        Poll::Ready(()) => {}
                        Poll::Pending => {
                            *this.state = State::SendProtocol {
                                io,
                                protocol,
                                header_received,
                            };
                            return Poll::Pending;
                        }
                    }

                    let p = Protocol::try_from(protocol.as_ref())?;
                    if let Err(err) = Pin::new(&mut io).start_send(Message::Protocol(p.clone())) {
                        return Poll::Ready(Err(From::from(err)));
                    }
                    tracing::debug!(target: LOG_TARGET, "Dialer: Proposed protocol: {}", p);

                    if this.protocols.peek().is_some() {
                        *this.state = State::FlushProtocol {
                            io,
                            protocol,
                            header_received,
                        }
                    } else {
                        match this.version {
                            Version::V1 =>
                                *this.state = State::FlushProtocol {
                                    io,
                                    protocol,
                                    header_received,
                                },
                            // This is the only effect that `V1Lazy` has compared to `V1`:
                            // Optimistically settling on the only protocol that
                            // the dialer supports for this negotiation. Notably,
                            // the dialer expects a regular `V1` response.
                            Version::V1Lazy => {
                                tracing::debug!(
                                    target: LOG_TARGET,
                                    "Dialer: Expecting proposed protocol: {}",
                                    p
                                );
                                let hl = HeaderLine::from(Version::V1Lazy);
                                let io = Negotiated::expecting(io.into_reader(), p, Some(hl));
                                return Poll::Ready(Ok((protocol, io)));
                            }
                        }
                    }
                }

                State::FlushProtocol {
                    mut io,
                    protocol,
                    header_received,
                } => match Pin::new(&mut io).poll_flush(cx)? {
                    Poll::Ready(()) =>
                        *this.state = State::AwaitProtocol {
                            io,
                            protocol,
                            header_received,
                        },
                    Poll::Pending => {
                        *this.state = State::FlushProtocol {
                            io,
                            protocol,
                            header_received,
                        };
                        return Poll::Pending;
                    }
                },

                State::AwaitProtocol {
                    mut io,
                    protocol,
                    header_received,
                } => {
                    let msg = match Pin::new(&mut io).poll_next(cx)? {
                        Poll::Ready(Some(msg)) => msg,
                        Poll::Pending => {
                            *this.state = State::AwaitProtocol {
                                io,
                                protocol,
                                header_received,
                            };
                            return Poll::Pending;
                        }
                        // Treat EOF error as [`NegotiationError::Failed`], not as
                        // [`NegotiationError::ProtocolError`], allowing dropping or closing an I/O
                        // stream as a permissible way to "gracefully" fail a negotiation.
                        Poll::Ready(None) => return Poll::Ready(Err(NegotiationError::Failed)),
                    };

                    match msg {
                        Message::Header(v)
                            if v == HeaderLine::from(*this.version) && !header_received =>
                        {
                            *this.state = State::AwaitProtocol {
                                io,
                                protocol,
                                header_received: true,
                            };
                        }
                        Message::Protocol(ref p) if p.as_ref() == protocol.as_ref() => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                "Dialer: Received confirmation for protocol: {}",
                                p
                            );
                            let io = Negotiated::completed(io.into_inner());
                            return Poll::Ready(Ok((protocol, io)));
                        }
                        Message::NotAvailable => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                "Dialer: Received rejection of protocol: {}",
                                String::from_utf8_lossy(protocol.as_ref())
                            );
                            let protocol = this.protocols.next().ok_or(NegotiationError::Failed)?;
                            *this.state = State::SendProtocol {
                                io,
                                protocol,
                                header_received,
                            }
                        }
                        _ => return Poll::Ready(Err(ProtocolError::InvalidMessage.into())),
                    }
                }

                State::Done => panic!("State::poll called after completion"),
            }
        }
    }
}

/// `multistream-select` handshake result for dialer.
#[derive(Debug, PartialEq, Eq)]
pub enum HandshakeResult {
    /// Handshake is not complete, data missing.
    NotReady,

    /// Handshake has succeeded.
    ///
    /// The returned tuple contains the negotiated protocol and response
    /// that must be sent to remote peer.
    Succeeded(ProtocolName),
}

/// Handshake state.
#[derive(Debug)]
enum HandshakeState {
    /// Waiting to receive any response from remote peer.
    WaitingResponse,

    /// Waiting to receive the actual application protocol from remote peer.
    WaitingProtocol,
}

/// `multistream-select` dialer handshake state.
#[derive(Debug)]
pub struct WebRtcDialerState {
    /// Proposed main protocol.
    protocol: ProtocolName,

    /// Fallback names of the main protocol.
    fallback_names: Vec<ProtocolName>,

    /// Dialer handshake state.
    state: HandshakeState,
}

impl WebRtcDialerState {
    /// Propose protocol to remote peer.
    ///
    /// Return [`WebRtcDialerState`] which is used to drive forward the negotiation and an encoded
    /// `multistream-select` message that contains the protocol proposal for the substream.
    pub fn propose(
        protocol: ProtocolName,
        fallback_names: Vec<ProtocolName>,
    ) -> crate::Result<(Self, Vec<u8>)> {
        let message = webrtc_encode_multistream_message(
            std::iter::once(protocol.clone())
                .chain(fallback_names.clone())
                .filter_map(|protocol| Protocol::try_from(protocol.as_ref()).ok())
                .map(Message::Protocol),
        )?
        .freeze()
        .to_vec();

        Ok((
            Self {
                protocol,
                fallback_names,
                state: HandshakeState::WaitingResponse,
            },
            message,
        ))
    }

    /// Register response to [`WebRtcDialerState`].
    pub fn register_response(
        &mut self,
        payload: Vec<u8>,
    ) -> Result<HandshakeResult, crate::error::NegotiationError> {
        // All multistream-select messages are length-prefixed. Since this code path is not using
        // multistream_select::protocol::MessageIO, we need to decode and remove the length here.
        let remaining: &[u8] = &payload;
        let (len, tail) = unsigned_varint::decode::usize(remaining).map_err(|error| {
            tracing::debug!(
                    target: LOG_TARGET,
                    ?error,
                    message = ?payload,
                    "Failed to decode length-prefix in multistream message");
            error::NegotiationError::ParseError(ParseError::InvalidData)
        })?;

        let len_size = remaining.len() - tail.len();
        let bytes = Bytes::from(payload);
        let payload = bytes.slice(len_size..len_size + len);
        let remaining = bytes.slice(len_size + len..);
        let message = Message::decode(payload);

        tracing::trace!(
            target: LOG_TARGET,
            ?message,
            "Decoded message while registering response",
        );

        let mut protocols = match message {
            Ok(Message::Header(HeaderLine::V1)) => {
                vec![PROTO_MULTISTREAM_1_0]
            }
            Ok(Message::Protocol(protocol)) => vec![protocol],
            Ok(Message::Protocols(protocols)) => protocols,
            Ok(Message::NotAvailable) =>
                return match &self.state {
                    HandshakeState::WaitingProtocol => Err(
                        error::NegotiationError::MultistreamSelectError(NegotiationError::Failed),
                    ),
                    _ => Err(error::NegotiationError::StateMismatch),
                },
            Ok(Message::ListProtocols) => return Err(error::NegotiationError::StateMismatch),
            Err(_) => return Err(error::NegotiationError::ParseError(ParseError::InvalidData)),
        };

        match drain_trailing_protocols(remaining) {
            Ok(protos) => protocols.extend(protos),
            Err(error) => return Err(error),
        }

        let mut protocol_iter = protocols.into_iter();
        loop {
            match (&self.state, protocol_iter.next()) {
                (HandshakeState::WaitingResponse, None) =>
                    return Err(crate::error::NegotiationError::StateMismatch),
                (HandshakeState::WaitingResponse, Some(protocol)) => {
                    if protocol == PROTO_MULTISTREAM_1_0 {
                        self.state = HandshakeState::WaitingProtocol;
                    } else {
                        return Err(crate::error::NegotiationError::MultistreamSelectError(
                            NegotiationError::Failed,
                        ));
                    }
                }
                (HandshakeState::WaitingProtocol, Some(protocol)) => {
                    if protocol == PROTO_MULTISTREAM_1_0 {
                        return Err(crate::error::NegotiationError::StateMismatch);
                    }

                    if self.protocol.as_bytes() == protocol.as_ref() {
                        return Ok(HandshakeResult::Succeeded(self.protocol.clone()));
                    }

                    for fallback in &self.fallback_names {
                        if fallback.as_bytes() == protocol.as_ref() {
                            return Ok(HandshakeResult::Succeeded(fallback.clone()));
                        }
                    }

                    return Err(crate::error::NegotiationError::MultistreamSelectError(
                        NegotiationError::Failed,
                    ));
                }
                (HandshakeState::WaitingProtocol, None) => {
                    return Ok(HandshakeResult::NotReady);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multistream_select::{listener_select_proto, protocol::MSG_MULTISTREAM_1_0};
    use bytes::BufMut;
    use std::time::Duration;
    #[tokio::test]
    async fn select_proto_basic() {
        async fn run(version: Version) {
            let (client_connection, server_connection) = futures_ringbuf::Endpoint::pair(100, 100);

            let server: tokio::task::JoinHandle<Result<(), ()>> = tokio::spawn(async move {
                let protos = vec!["/proto1", "/proto2"];
                let (proto, mut io) =
                    listener_select_proto(server_connection, protos).await.unwrap();
                assert_eq!(proto, "/proto2");

                let mut out = vec![0; 32];
                let n = io.read(&mut out).await.unwrap();
                out.truncate(n);
                assert_eq!(out, b"ping");

                io.write_all(b"pong").await.unwrap();
                io.flush().await.unwrap();

                Ok(())
            });

            let client: tokio::task::JoinHandle<Result<(), ()>> = tokio::spawn(async move {
                let protos = vec!["/proto3", "/proto2"];
                let (proto, mut io) =
                    dialer_select_proto(client_connection, protos, version).await.unwrap();
                assert_eq!(proto, "/proto2");

                io.write_all(b"ping").await.unwrap();
                io.flush().await.unwrap();

                let mut out = vec![0; 32];
                let n = io.read(&mut out).await.unwrap();
                out.truncate(n);
                assert_eq!(out, b"pong");

                Ok(())
            });

            server.await.unwrap();
            client.await.unwrap();
        }

        run(Version::V1).await;
        run(Version::V1Lazy).await;
    }

    /// Tests the expected behaviour of failed negotiations.
    #[tokio::test]
    async fn negotiation_failed() {
        async fn run(
            version: Version,
            dial_protos: Vec<&'static str>,
            dial_payload: Vec<u8>,
            listen_protos: Vec<&'static str>,
        ) {
            let (client_connection, server_connection) = futures_ringbuf::Endpoint::pair(100, 100);

            let server: tokio::task::JoinHandle<Result<(), ()>> = tokio::spawn(async move {
                let io = match tokio::time::timeout(
                    Duration::from_secs(2),
                    listener_select_proto(server_connection, listen_protos),
                )
                .await
                .unwrap()
                {
                    Ok((_, io)) => io,
                    Err(NegotiationError::Failed) => return Ok(()),
                    Err(NegotiationError::ProtocolError(e)) => {
                        panic!("Unexpected protocol error {e}")
                    }
                };
                match io.complete().await {
                    Err(NegotiationError::Failed) => {}
                    _ => panic!(),
                }

                Ok(())
            });

            let client: tokio::task::JoinHandle<Result<(), ()>> = tokio::spawn(async move {
                let mut io = match tokio::time::timeout(
                    Duration::from_secs(2),
                    dialer_select_proto(client_connection, dial_protos, version),
                )
                .await
                .unwrap()
                {
                    Err(NegotiationError::Failed) => return Ok(()),
                    Ok((_, io)) => io,
                    Err(_) => panic!(),
                };

                // The dialer may write a payload that is even sent before it
                // got confirmation of the last proposed protocol, when `V1Lazy`
                // is used.
                io.write_all(&dial_payload).await.unwrap();
                match io.complete().await {
                    Err(NegotiationError::Failed) => {}
                    _ => panic!(),
                }

                Ok(())
            });

            server.await.unwrap();
            client.await.unwrap();
        }

        // Incompatible protocols.
        run(Version::V1, vec!["/proto1"], vec![1], vec!["/proto2"]).await;
        run(Version::V1Lazy, vec!["/proto1"], vec![1], vec!["/proto2"]).await;
    }

    #[tokio::test]
    async fn v1_lazy_do_not_wait_for_negotiation_on_poll_close() {
        let (client_connection, _server_connection) =
            futures_ringbuf::Endpoint::pair(1024 * 1024, 1);

        let client = tokio::spawn(async move {
            // Single protocol to allow for lazy (or optimistic) protocol negotiation.
            let protos = vec!["/proto1"];
            let (proto, mut io) =
                dialer_select_proto(client_connection, protos, Version::V1Lazy).await.unwrap();
            assert_eq!(proto, "/proto1");

            // In Libp2p the lazy negotation of protocols can be closed at any time,
            // even if the negotiation is not yet done.

            // However, for the Litep2p the negotation must conclude before closing the
            // lazy negotation of protocol. We'll wait for the close until the
            // server has produced a message, in this test that means forever.
            io.close().await.unwrap();
        });

        assert!(tokio::time::timeout(Duration::from_secs(10), client).await.is_ok());
    }

    #[tokio::test]
    async fn low_level_negotiate() {
        async fn run(version: Version) {
            let (client_connection, mut server_connection) =
                futures_ringbuf::Endpoint::pair(100, 100);

            let server = tokio::spawn(async move {
                let protos = ["/proto2"];

                let multistream = b"/multistream/1.0.0\n";
                let len = multistream.len();
                let proto = b"/proto2\n";
                let proto_len = proto.len();

                // Check that our implementation writes optimally
                // the multistream ++ protocol in a single message.
                let mut expected_message = Vec::new();
                expected_message.push(len as u8);
                expected_message.extend_from_slice(multistream);
                expected_message.push(proto_len as u8);
                expected_message.extend_from_slice(proto);

                if version == Version::V1Lazy {
                    expected_message.extend_from_slice(b"ping");
                }

                let mut out = vec![0; 64];
                let n = server_connection.read(&mut out).await.unwrap();
                out.truncate(n);
                assert_eq!(out, expected_message);

                // We must send the back the multistream packet.
                let mut send_message = Vec::new();
                send_message.push(len as u8);
                send_message.extend_from_slice(multistream);

                server_connection.write_all(&mut send_message).await.unwrap();

                let mut send_message = Vec::new();
                send_message.push(proto_len as u8);
                send_message.extend_from_slice(proto);
                server_connection.write_all(&mut send_message).await.unwrap();

                // Handle handshake.
                match version {
                    Version::V1 => {
                        let mut out = vec![0; 64];
                        let n = server_connection.read(&mut out).await.unwrap();
                        out.truncate(n);
                        assert_eq!(out, b"ping");

                        server_connection.write_all(b"pong").await.unwrap();
                    }
                    Version::V1Lazy => {
                        // Ping (handshake) payload expected in the initial message.
                        server_connection.write_all(b"pong").await.unwrap();
                    }
                }
            });

            let client = tokio::spawn(async move {
                let protos = vec!["/proto2"];
                let (proto, mut io) =
                    dialer_select_proto(client_connection, protos, version).await.unwrap();
                assert_eq!(proto, "/proto2");

                io.write_all(b"ping").await.unwrap();
                io.flush().await.unwrap();

                let mut out = vec![0; 32];
                let n = io.read(&mut out).await.unwrap();
                out.truncate(n);
                assert_eq!(out, b"pong");
            });

            server.await.unwrap();
            client.await.unwrap();
        }

        run(Version::V1).await;
        run(Version::V1Lazy).await;
    }

    #[tokio::test]
    async fn v1_low_level_negotiate_multiple_headers() {
        let (client_connection, mut server_connection) = futures_ringbuf::Endpoint::pair(100, 100);

        let server = tokio::spawn(async move {
            let protos = ["/proto2"];

            let multistream = b"/multistream/1.0.0\n";
            let len = multistream.len();
            let proto = b"/proto2\n";
            let proto_len = proto.len();

            // Check that our implementation writes optimally
            // the multistream ++ protocol in a single message.
            let mut expected_message = Vec::new();
            expected_message.push(len as u8);
            expected_message.extend_from_slice(multistream);
            expected_message.push(proto_len as u8);
            expected_message.extend_from_slice(proto);

            let mut out = vec![0; 64];
            let n = server_connection.read(&mut out).await.unwrap();
            out.truncate(n);
            assert_eq!(out, expected_message);

            // We must send the back the multistream packet.
            let mut send_message = Vec::new();
            send_message.push(len as u8);
            send_message.extend_from_slice(multistream);

            server_connection.write_all(&mut send_message).await.unwrap();

            // We must send the back the multistream packet again.
            let mut send_message = Vec::new();
            send_message.push(len as u8);
            send_message.extend_from_slice(multistream);

            server_connection.write_all(&mut send_message).await.unwrap();
        });

        let client = tokio::spawn(async move {
            let protos = vec!["/proto2"];

            // Negotiation fails because the protocol receives the `/multistream/1.0.0` header
            // multiple times.
            let result =
                dialer_select_proto(client_connection, protos, Version::V1).await.unwrap_err();
            match result {
                NegotiationError::ProtocolError(ProtocolError::InvalidMessage) => {}
                _ => panic!("unexpected error: {:?}", result),
            };
        });

        server.await.unwrap();
        client.await.unwrap();
    }

    #[tokio::test]
    async fn v1_lazy_low_level_negotiate_multiple_headers() {
        let (client_connection, mut server_connection) = futures_ringbuf::Endpoint::pair(100, 100);

        let server = tokio::spawn(async move {
            let protos = ["/proto2"];

            let multistream = b"/multistream/1.0.0\n";
            let len = multistream.len();
            let proto = b"/proto2\n";
            let proto_len = proto.len();

            // Check that our implementation writes optimally
            // the multistream ++ protocol in a single message.
            let mut expected_message = Vec::new();
            expected_message.push(len as u8);
            expected_message.extend_from_slice(multistream);
            expected_message.push(proto_len as u8);
            expected_message.extend_from_slice(proto);

            let mut out = vec![0; 64];
            let n = server_connection.read(&mut out).await.unwrap();
            out.truncate(n);
            assert_eq!(out, expected_message);

            // We must send the back the multistream packet.
            let mut send_message = Vec::new();
            send_message.push(len as u8);
            send_message.extend_from_slice(multistream);

            server_connection.write_all(&mut send_message).await.unwrap();

            // We must send the back the multistream packet again.
            let mut send_message = Vec::new();
            send_message.push(len as u8);
            send_message.extend_from_slice(multistream);

            server_connection.write_all(&mut send_message).await.unwrap();
        });

        let client = tokio::spawn(async move {
            let protos = vec!["/proto2"];

            // Negotiation fails because the protocol receives the `/multistream/1.0.0` header
            // multiple times.
            let (proto, to_negociate) =
                dialer_select_proto(client_connection, protos, Version::V1Lazy).await.unwrap();
            assert_eq!(proto, "/proto2");

            let result = to_negociate.complete().await.unwrap_err();

            match result {
                NegotiationError::ProtocolError(ProtocolError::InvalidMessage) => {}
                _ => panic!("unexpected error: {:?}", result),
            };
        });

        server.await.unwrap();
        client.await.unwrap();
    }

    #[test]
    fn propose() {
        let (mut dialer_state, message) =
            WebRtcDialerState::propose(ProtocolName::from("/13371338/proto/1"), vec![]).unwrap();

        let mut bytes = BytesMut::with_capacity(32);
        bytes.put_u8(MSG_MULTISTREAM_1_0.len() as u8);
        let _ = Message::Header(HeaderLine::V1).encode(&mut bytes).unwrap();

        let proto = Protocol::try_from(&b"/13371338/proto/1"[..]).expect("valid protocol name");
        bytes.put_u8((proto.as_ref().len() + 1) as u8); // + 1 for \n
        let _ = Message::Protocol(proto).encode(&mut bytes).unwrap();

        let expected_message = bytes.freeze().to_vec();

        assert_eq!(message, expected_message);
    }

    #[test]
    fn propose_with_fallback() {
        let (mut dialer_state, message) = WebRtcDialerState::propose(
            ProtocolName::from("/13371338/proto/1"),
            vec![ProtocolName::from("/sup/proto/1")],
        )
        .unwrap();

        let mut bytes = BytesMut::with_capacity(32);
        bytes.put_u8(MSG_MULTISTREAM_1_0.len() as u8);
        let _ = Message::Header(HeaderLine::V1).encode(&mut bytes).unwrap();

        let proto1 = Protocol::try_from(&b"/13371338/proto/1"[..]).expect("valid protocol name");
        bytes.put_u8((proto1.as_ref().len() + 1) as u8); // + 1 for \n
        let _ = Message::Protocol(proto1).encode(&mut bytes).unwrap();

        let proto2 = Protocol::try_from(&b"/sup/proto/1"[..]).expect("valid protocol name");
        bytes.put_u8((proto2.as_ref().len() + 1) as u8); // + 1 for \n
        let _ = Message::Protocol(proto2).encode(&mut bytes).unwrap();

        let expected_message = bytes.freeze().to_vec();

        assert_eq!(message, expected_message);
    }

    #[test]
    fn register_response_header_only() {
        let mut bytes = BytesMut::with_capacity(32);
        bytes.put_u8(MSG_MULTISTREAM_1_0.len() as u8);

        let message = Message::Header(HeaderLine::V1);
        message.encode(&mut bytes).map_err(|_| Error::InvalidData).unwrap();

        let (mut dialer_state, _message) =
            WebRtcDialerState::propose(ProtocolName::from("/13371338/proto/1"), vec![]).unwrap();

        match dialer_state.register_response(bytes.freeze().to_vec()) {
            Ok(HandshakeResult::NotReady) => {}
            Err(err) => panic!("unexpected error: {:?}", err),
            event => panic!("invalid event: {event:?}"),
        }
    }

    #[test]
    fn header_line_missing() {
        // header line missing
        let proto = b"/13371338/proto/1";
        let mut bytes = BytesMut::with_capacity(proto.len() + 2);
        bytes.put_u8((proto.len() + 1) as u8);

        let response = Message::Protocol(Protocol::try_from(&proto[..]).unwrap())
            .encode(&mut bytes)
            .expect("valid message encodes");

        let response = bytes.freeze().to_vec();

        let (mut dialer_state, _message) =
            WebRtcDialerState::propose(ProtocolName::from("/13371338/proto/1"), vec![]).unwrap();

        match dialer_state.register_response(response) {
            Err(error::NegotiationError::MultistreamSelectError(NegotiationError::Failed)) => {}
            event => panic!("invalid event: {event:?}"),
        }
    }

    #[test]
    fn negotiate_main_protocol() {
        let message = webrtc_encode_multistream_message(vec![Message::Protocol(
            Protocol::try_from(&b"/13371338/proto/1"[..]).unwrap(),
        )])
        .unwrap()
        .freeze();

        let (mut dialer_state, _message) = WebRtcDialerState::propose(
            ProtocolName::from("/13371338/proto/1"),
            vec![ProtocolName::from("/sup/proto/1")],
        )
        .unwrap();

        match dialer_state.register_response(message.to_vec()) {
            Ok(HandshakeResult::Succeeded(negotiated)) => {
                assert_eq!(negotiated, ProtocolName::from("/13371338/proto/1"))
            }
            event => panic!("invalid event {event:?}"),
        }
    }

    #[test]
    fn negotiate_fallback_protocol() {
        let message = webrtc_encode_multistream_message(vec![Message::Protocol(
            Protocol::try_from(&b"/sup/proto/1"[..]).unwrap(),
        )])
        .unwrap()
        .freeze();

        let (mut dialer_state, _message) = WebRtcDialerState::propose(
            ProtocolName::from("/13371338/proto/1"),
            vec![ProtocolName::from("/sup/proto/1")],
        )
        .unwrap();

        match dialer_state.register_response(message.to_vec()) {
            Ok(HandshakeResult::Succeeded(negotiated)) => {
                assert_eq!(negotiated, ProtocolName::from("/sup/proto/1"))
            }
            _ => panic!("invalid event"),
        }
    }
}
