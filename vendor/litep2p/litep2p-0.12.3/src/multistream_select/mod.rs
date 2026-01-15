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

#![allow(unused)]
#![allow(clippy::derivable_impls)]

//! # Multistream-select Protocol Negotiation
//!
//! This crate implements the `multistream-select` protocol, which is the protocol
//! used by libp2p to negotiate which application-layer protocol to use with the
//! remote on a connection or substream.
//!
//! > **Note**: This crate is used primarily by core components of *libp2p* and it
//! > is usually not used directly on its own.
//!
//! ## Roles
//!
//! Two peers using the multistream-select negotiation protocol on an I/O stream
//! are distinguished by their role as a _dialer_ (or _initiator_) or as a _listener_
//! (or _responder_). Thereby the dialer plays the active part, driving the protocol,
//! whereas the listener reacts to the messages received.
//!
//! The dialer has two options: it can either pick a protocol from the complete list
//! of protocols that the listener supports, or it can directly suggest a protocol.
//! Either way, a selected protocol is sent to the listener who can either accept (by
//! echoing the same protocol) or reject (by responding with a message stating
//! "not available"). If a suggested protocol is not available, the dialer may
//! suggest another protocol. This process continues until a protocol is agreed upon,
//! yielding a [`Negotiated`] stream, or the dialer has run out of
//! alternatives.
//!
//! See [`dialer_select_proto`] and [`listener_select_proto`].
//!
//! ## [`Negotiated`]
//!
//! A `Negotiated` represents an I/O stream that has settled on a protocol
//! to use. By default, with [`Version::V1`], protocol negotiation is always
//! at least one dedicated round-trip message exchange, before application
//! data for the negotiated protocol can be sent by the dialer. There is
//! a variant [`Version::V1Lazy`] that permits 0-RTT negotiation if the
//! dialer only supports a single protocol. In that case, when a dialer
//! settles on a protocol to use, the [`DialerSelectFuture`] yields a
//! [`Negotiated`] I/O stream before the negotiation
//! data has been flushed. It is then expecting confirmation for that protocol
//! as the first messages read from the stream. This behaviour allows the dialer
//! to immediately send data relating to the negotiated protocol together with the
//! remaining negotiation message(s). Note, however, that a dialer that performs
//! multiple 0-RTT negotiations in sequence for different protocols layered on
//! top of each other may trigger undesirable behaviour for a listener not
//! supporting one of the intermediate protocols. See
//! [`dialer_select_proto`] and the documentation of [`Version::V1Lazy`] for further details.

#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

mod dialer_select;
mod length_delimited;
mod listener_select;
mod negotiated;
mod protocol;

use crate::error::{self, ParseError};
pub use crate::multistream_select::{
    dialer_select::{dialer_select_proto, DialerSelectFuture, HandshakeResult, WebRtcDialerState},
    listener_select::{
        listener_select_proto, webrtc_listener_negotiate, ListenerSelectFuture,
        ListenerSelectResult,
    },
    negotiated::{Negotiated, NegotiatedComplete, NegotiationError},
    protocol::{HeaderLine, Message, Protocol, ProtocolError, PROTO_MULTISTREAM_1_0},
};

use bytes::Bytes;

const LOG_TARGET: &str = "litep2p::multistream-select";

/// Supported multistream-select versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    /// Version 1 of the multistream-select protocol. See [1] and [2].
    ///
    /// [1]: https://github.com/libp2p/specs/blob/master/connections/README.md#protocol-negotiation
    /// [2]: https://github.com/multiformats/multistream-select
    V1,
    /// A "lazy" variant of version 1 that is identical on the wire but whereby
    /// the dialer delays flushing protocol negotiation data in order to combine
    /// it with initial application data, thus performing 0-RTT negotiation.
    ///
    /// This strategy is only applicable for the node with the role of "dialer"
    /// in the negotiation and only if the dialer supports just a single
    /// application protocol. In that case the dialer immedidately "settles"
    /// on that protocol, buffering the negotiation messages to be sent
    /// with the first round of application protocol data (or an attempt
    /// is made to read from the `Negotiated` I/O stream).
    ///
    /// A listener will behave identically to `V1`. This ensures interoperability with `V1`.
    /// Notably, it will immediately send the multistream header as well as the protocol
    /// confirmation, resulting in multiple frames being sent on the underlying transport.
    /// Nevertheless, if the listener supports the protocol that the dialer optimistically
    /// settled on, it can be a 0-RTT negotiation.
    ///
    /// > **Note**: `V1Lazy` is specific to `rust-libp2p`. The wire protocol is identical to `V1`
    /// > and generally interoperable with peers only supporting `V1`. Nevertheless, there is a
    /// > pitfall that is rarely encountered: When nesting multiple protocol negotiations, the
    /// > listener should either be known to support all of the dialer's optimistically chosen
    /// > protocols or there is must be no intermediate protocol without a payload and none of
    /// > the protocol payloads must have the potential for being mistaken for a multistream-select
    /// > protocol message. This avoids rare edge-cases whereby the listener may not recognize
    /// > upgrade boundaries and erroneously process a request despite not supporting one of
    /// > the intermediate protocols that the dialer committed to. See [1] and [2].
    ///
    /// [1]: https://github.com/multiformats/go-multistream/issues/20
    /// [2]: https://github.com/libp2p/rust-libp2p/pull/1212
    V1Lazy,
    // Draft: https://github.com/libp2p/specs/pull/95
    // V2,
}

impl Default for Version {
    fn default() -> Self {
        Version::V1
    }
}

// This function is only used in the WebRTC transport. It expects one or more multistream-select
// messages in `remaining` and returns a list of protocols that were decoded from them.
fn drain_trailing_protocols(
    mut remaining: Bytes,
) -> Result<Vec<Protocol>, error::NegotiationError> {
    let mut protocols = vec![];

    loop {
        if remaining.is_empty() {
            break;
        }

        let (len, tail) = unsigned_varint::decode::usize(&remaining).map_err(|error| {
            tracing::debug!(
                    target: LOG_TARGET,
                    ?error,
                    message = ?remaining,
                    "Failed to decode length-prefix in multistream message");
            error::NegotiationError::ParseError(ParseError::InvalidData)
        })?;

        if len > tail.len() {
            tracing::debug!(
                target: LOG_TARGET,
                message = ?tail,
                length_prefix = len,
                actual_length = tail.len(),
                "Truncated multistream message",
            );

            return Err(error::NegotiationError::ParseError(ParseError::InvalidData));
        }

        let len_size = remaining.len() - tail.len();
        let payload = remaining.slice(len_size..len_size + len);
        let res = Message::decode(payload);

        match res {
            Ok(Message::Header(HeaderLine::V1)) => protocols.push(PROTO_MULTISTREAM_1_0),
            Ok(Message::Protocol(protocol)) => protocols.push(protocol),
            Ok(Message::Protocols(_)) =>
                return Err(error::NegotiationError::ParseError(ParseError::InvalidData)),
            Err(error) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?error,
                    message = ?tail[..len],
                    "Failed to decode multistream message",
                );
                return Err(error::NegotiationError::ParseError(ParseError::InvalidData));
            }
            _ => return Err(error::NegotiationError::StateMismatch),
        }

        remaining = remaining.slice(len_size + len..);
    }

    Ok(protocols)
}
