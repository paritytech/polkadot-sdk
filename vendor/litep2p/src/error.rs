// Copyright 2019 Parity Technologies (UK) Ltd.
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

#![allow(clippy::enum_variant_names)]

//! [`Litep2p`](`crate::Litep2p`) error types.

use crate::{
    protocol::Direction,
    transport::manager::limits::ConnectionLimitsError,
    types::{protocol::ProtocolName, ConnectionId, SubstreamId},
    PeerId,
};

use multiaddr::Multiaddr;
use multihash::{Multihash, MultihashGeneric};

use std::io::{self, ErrorKind};

// TODO: https://github.com/paritytech/litep2p/issues/204 clean up the overarching error.
// Please note that this error is not propagated directly to the user.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Peer `{0}` does not exist")]
    PeerDoesntExist(PeerId),
    #[error("Peer `{0}` already exists")]
    PeerAlreadyExists(PeerId),
    #[error("Protocol `{0}` not supported")]
    ProtocolNotSupported(String),
    #[error("Address error: `{0}`")]
    AddressError(#[from] AddressError),
    #[error("Parse error: `{0}`")]
    ParseError(ParseError),
    #[error("I/O error: `{0}`")]
    IoError(ErrorKind),
    #[error("Negotiation error: `{0}`")]
    NegotiationError(#[from] NegotiationError),
    #[error("Substream error: `{0}`")]
    SubstreamError(#[from] SubstreamError),
    #[error("Substream error: `{0}`")]
    NotificationError(NotificationError),
    #[error("Essential task closed")]
    EssentialTaskClosed,
    #[error("Unknown error occurred")]
    Unknown,
    #[error("Cannot dial self: `{0}`")]
    CannotDialSelf(Multiaddr),
    #[error("Transport not supported")]
    TransportNotSupported(Multiaddr),
    #[error("Yamux error for substream `{0:?}`: `{1}`")]
    YamuxError(Direction, crate::yamux::ConnectionError),
    #[error("Operation not supported: `{0}`")]
    NotSupported(String),
    #[error("Other error occurred: `{0}`")]
    Other(String),
    #[error("Protocol already exists: `{0:?}`")]
    ProtocolAlreadyExists(ProtocolName),
    #[error("Operation timed out")]
    Timeout,
    #[error("Invalid state transition")]
    InvalidState,
    #[error("DNS address resolution failed")]
    DnsAddressResolutionFailed,
    #[error("Transport error: `{0}`")]
    TransportError(String),
    #[cfg(feature = "quic")]
    #[error("Failed to generate certificate: `{0}`")]
    CertificateGeneration(#[from] crate::crypto::tls::certificate::GenError),
    #[error("Invalid data")]
    InvalidData,
    #[error("Input rejected")]
    InputRejected,
    #[cfg(feature = "websocket")]
    #[error("WebSocket error: `{0}`")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::error::Error),
    #[error("Insufficient peers")]
    InsufficientPeers,
    #[error("Substream doens't exist")]
    SubstreamDoesntExist,
    #[cfg(feature = "webrtc")]
    #[error("`str0m` error: `{0}`")]
    WebRtc(#[from] str0m::RtcError),
    #[error("Remote peer disconnected")]
    Disconnected,
    #[error("Channel does not exist")]
    ChannelDoesntExist,
    #[error("Tried to dial self")]
    TriedToDialSelf,
    #[error("Litep2p is already connected to the peer")]
    AlreadyConnected,
    #[error("No addres available for `{0}`")]
    NoAddressAvailable(PeerId),
    #[error("Connection closed")]
    ConnectionClosed,
    #[cfg(feature = "quic")]
    #[error("Quinn error: `{0}`")]
    Quinn(quinn::ConnectionError),
    #[error("Invalid certificate")]
    InvalidCertificate,
    #[error("Peer ID mismatch: expected `{0}`, got `{1}`")]
    PeerIdMismatch(PeerId, PeerId),
    #[error("Channel is clogged")]
    ChannelClogged,
    #[error("Connection doesn't exist: `{0:?}`")]
    ConnectionDoesntExist(ConnectionId),
    #[error("Exceeded connection limits `{0:?}`")]
    ConnectionLimit(ConnectionLimitsError),
    #[error("Failed to dial peer immediately")]
    ImmediateDialError(#[from] ImmediateDialError),
    #[error("Cannot read system DNS config: `{0}`")]
    CannotReadSystemDnsConfig(hickory_resolver::ResolveError),
}

/// Error type for address parsing.
#[derive(Debug, thiserror::Error)]
pub enum AddressError {
    /// The provided address does not correspond to the transport protocol.
    ///
    /// For example, this can happen when the address used the UDP protocol but
    /// the handling transport only allows TCP connections.
    #[error("Invalid address for protocol")]
    InvalidProtocol,
    /// The provided address is not a valid URL.
    #[error("Invalid URL")]
    InvalidUrl,
    /// The provided address does not include a peer ID.
    #[error("`PeerId` missing from the address")]
    PeerIdMissing,
    /// No address is available for the provided peer ID.
    #[error("Address not available")]
    AddressNotAvailable,
    /// The provided address contains an invalid multihash.
    #[error("Multihash does not contain a valid peer ID : `{0:?}`")]
    InvalidPeerId(Multihash),
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ParseError {
    /// The provided probuf message cannot be decoded.
    #[error("Failed to decode protobuf message: `{0:?}`")]
    ProstDecodeError(#[from] prost::DecodeError),
    /// The provided protobuf message cannot be encoded.
    #[error("Failed to encode protobuf message: `{0:?}`")]
    ProstEncodeError(#[from] prost::EncodeError),
    /// The protobuf message contains an unexpected key type.
    ///
    /// This error can happen when:
    ///  - The provided key type is not recognized.
    ///  - The provided key type is recognized but not supported.
    #[error("Unknown key type from protobuf message: `{0}`")]
    UnknownKeyType(i32),
    /// The public key bytes are invalid and cannot be parsed.
    ///
    /// This error can happen when:
    ///  - The received number of bytes is not equal to the expected number of bytes (32 bytes).
    ///  - The bytes are not a valid Ed25519 public key.
    ///  - Length of the public key is not represented by 2 bytes (WebRTC specific).
    #[error("Invalid public key")]
    InvalidPublicKey,
    /// The provided date has an invalid format.
    ///
    /// This error is protocol specific.
    #[error("Invalid data")]
    InvalidData,
    /// The provided reply length is not valid
    #[error("Invalid reply length")]
    InvalidReplyLength,
}

#[derive(Debug, thiserror::Error)]
pub enum SubstreamError {
    #[error("Connection closed")]
    ConnectionClosed,
    #[error("Connection channel clogged")]
    ChannelClogged,
    #[error("Connection to peer does not exist: `{0}`")]
    PeerDoesNotExist(PeerId),
    #[error("I/O error: `{0}`")]
    IoError(ErrorKind),
    #[error("yamux error: `{0}`")]
    YamuxError(crate::yamux::ConnectionError, Direction),
    #[error("Failed to read from substream, substream id `{0:?}`")]
    ReadFailure(Option<SubstreamId>),
    #[error("Failed to write to substream, substream id `{0:?}`")]
    WriteFailure(Option<SubstreamId>),
    #[error("Negotiation error: `{0:?}`")]
    NegotiationError(#[from] NegotiationError),
}

// Libp2p yamux does not implement PartialEq for ConnectionError.
impl PartialEq for SubstreamError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ConnectionClosed, Self::ConnectionClosed) => true,
            (Self::ChannelClogged, Self::ChannelClogged) => true,
            (Self::PeerDoesNotExist(lhs), Self::PeerDoesNotExist(rhs)) => lhs == rhs,
            (Self::IoError(lhs), Self::IoError(rhs)) => lhs == rhs,
            (Self::YamuxError(lhs, lhs_1), Self::YamuxError(rhs, rhs_1)) => {
                if lhs_1 != rhs_1 {
                    return false;
                }

                match (lhs, rhs) {
                    (
                        crate::yamux::ConnectionError::Io(lhs),
                        crate::yamux::ConnectionError::Io(rhs),
                    ) => lhs.kind() == rhs.kind(),
                    (
                        crate::yamux::ConnectionError::Decode(lhs),
                        crate::yamux::ConnectionError::Decode(rhs),
                    ) => match (lhs, rhs) {
                        (
                            crate::yamux::FrameDecodeError::Io(lhs),
                            crate::yamux::FrameDecodeError::Io(rhs),
                        ) => lhs.kind() == rhs.kind(),
                        (
                            crate::yamux::FrameDecodeError::FrameTooLarge(lhs),
                            crate::yamux::FrameDecodeError::FrameTooLarge(rhs),
                        ) => lhs == rhs,
                        (
                            crate::yamux::FrameDecodeError::Header(lhs),
                            crate::yamux::FrameDecodeError::Header(rhs),
                        ) => match (lhs, rhs) {
                            (
                                crate::yamux::HeaderDecodeError::Version(lhs),
                                crate::yamux::HeaderDecodeError::Version(rhs),
                            ) => lhs == rhs,
                            (
                                crate::yamux::HeaderDecodeError::Type(lhs),
                                crate::yamux::HeaderDecodeError::Type(rhs),
                            ) => lhs == rhs,
                            _ => false,
                        },
                        _ => false,
                    },
                    (
                        crate::yamux::ConnectionError::NoMoreStreamIds,
                        crate::yamux::ConnectionError::NoMoreStreamIds,
                    ) => true,
                    (
                        crate::yamux::ConnectionError::Closed,
                        crate::yamux::ConnectionError::Closed,
                    ) => true,
                    (
                        crate::yamux::ConnectionError::TooManyStreams,
                        crate::yamux::ConnectionError::TooManyStreams,
                    ) => true,
                    _ => false,
                }
            }

            (Self::ReadFailure(lhs), Self::ReadFailure(rhs)) => lhs == rhs,
            (Self::WriteFailure(lhs), Self::WriteFailure(rhs)) => lhs == rhs,
            (Self::NegotiationError(lhs), Self::NegotiationError(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

/// Error during the negotiation phase.
#[derive(Debug, thiserror::Error)]
pub enum NegotiationError {
    /// Error occurred during the multistream-select phase of the negotiation.
    #[error("multistream-select error: `{0:?}`")]
    MultistreamSelectError(#[from] crate::multistream_select::NegotiationError),
    /// Error occurred during the Noise handshake negotiation.
    #[error("multistream-select error: `{0:?}`")]
    SnowError(#[from] snow::Error),
    /// The peer ID was not provided by the noise handshake.
    #[error("`PeerId` missing from Noise handshake")]
    PeerIdMissing,
    /// The remote peer ID is not the same as the one expected.
    #[error("The signature of the remote identity's public key does not verify")]
    BadSignature,
    /// The negotiation operation timed out.
    #[error("Operation timed out")]
    Timeout,
    /// The message provided over the wire has an invalid format or is unsupported.
    #[error("Parse error: `{0}`")]
    ParseError(#[from] ParseError),
    /// An I/O error occurred during the negotiation process.
    #[error("I/O error: `{0}`")]
    IoError(ErrorKind),
    /// Expected a different state during the negotiation process.
    #[error("Expected a different state")]
    StateMismatch,
    /// The noise handshake provided a different peer ID than the one expected in the dialing
    /// address.
    #[error("Peer ID mismatch: expected `{0}`, got `{1}`")]
    PeerIdMismatch(PeerId, PeerId),
    /// Error specific to the QUIC transport.
    #[cfg(feature = "quic")]
    #[error("QUIC error: `{0}`")]
    Quic(#[from] QuicError),
    /// Error specific to the WebSocket transport.
    #[cfg(feature = "websocket")]
    #[error("WebSocket error: `{0}`")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::error::Error),
}

impl PartialEq for NegotiationError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MultistreamSelectError(lhs), Self::MultistreamSelectError(rhs)) => lhs == rhs,
            (Self::SnowError(lhs), Self::SnowError(rhs)) => lhs == rhs,
            (Self::ParseError(lhs), Self::ParseError(rhs)) => lhs == rhs,
            (Self::IoError(lhs), Self::IoError(rhs)) => lhs == rhs,
            (Self::PeerIdMismatch(lhs, lhs_1), Self::PeerIdMismatch(rhs, rhs_1)) =>
                lhs == rhs && lhs_1 == rhs_1,
            #[cfg(feature = "quic")]
            (Self::Quic(lhs), Self::Quic(rhs)) => lhs == rhs,
            #[cfg(feature = "websocket")]
            (Self::WebSocket(lhs), Self::WebSocket(rhs)) =>
                core::mem::discriminant(lhs) == core::mem::discriminant(rhs),
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("Peer already exists")]
    PeerAlreadyExists,
    #[error("Peer is in invalid state")]
    InvalidState,
    #[error("Notifications clogged")]
    NotificationsClogged,
    #[error("Notification stream closed")]
    NotificationStreamClosed(PeerId),
}

/// The error type for dialing a peer.
///
/// This error is reported via the litep2p events after performing
/// a network dialing operation.
#[derive(Debug, thiserror::Error)]
pub enum DialError {
    /// The dialing operation timed out.
    ///
    /// This error indicates that the `connection_open_timeout` from the protocol configuration
    /// was exceeded.
    #[error("Dial timed out")]
    Timeout,
    /// The provided address for dialing is invalid.
    #[error("Address error: `{0}`")]
    AddressError(#[from] AddressError),
    /// An error occurred during DNS lookup operation.
    ///
    /// The address provided may be valid, however it failed to resolve to a concrete IP address.
    /// This error may be recoverable.
    #[error("DNS lookup error for `{0}`")]
    DnsError(#[from] DnsError),
    /// An error occurred during the negotiation process.
    #[error("Negotiation error: `{0}`")]
    NegotiationError(#[from] NegotiationError),
}

/// Dialing resulted in an immediate error before performing any network operations.
#[derive(Debug, thiserror::Error, Copy, Clone, Eq, PartialEq)]
pub enum ImmediateDialError {
    /// The provided address does not include a peer ID.
    #[error("`PeerId` missing from the address")]
    PeerIdMissing,
    /// The peer ID provided in the address is the same as the local peer ID.
    #[error("Tried to dial self")]
    TriedToDialSelf,
    /// Cannot dial an already connected peer.
    #[error("Already connected to peer")]
    AlreadyConnected,
    /// Cannot dial a peer that does not have any address available.
    #[error("No address available for peer")]
    NoAddressAvailable,
    /// The essential task was closed.
    #[error("TaskClosed")]
    TaskClosed,
    /// The channel is clogged.
    #[error("Connection channel clogged")]
    ChannelClogged,
}

/// Error during the QUIC transport negotiation.
#[cfg(feature = "quic")]
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum QuicError {
    /// The provided certificate is invalid.
    #[error("Invalid certificate")]
    InvalidCertificate,
    /// The connection was lost.
    #[error("Failed to negotiate QUIC: `{0}`")]
    ConnectionError(#[from] quinn::ConnectionError),
    /// The connection could not be established.
    #[error("Failed to connect to peer: `{0}`")]
    ConnectError(#[from] quinn::ConnectError),
}

/// Error during DNS resolution.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DnsError {
    /// The DNS resolution failed to resolve the provided URL.
    #[error("DNS failed to resolve url `{0}`")]
    ResolveError(String),
    /// The DNS expected a different IP address version.
    ///
    /// For example, DNSv4 was expected but DNSv6 was provided.
    #[error("DNS type is different from the provided IP address")]
    IpVersionMismatch,
}

impl From<MultihashGeneric<64>> for Error {
    fn from(hash: MultihashGeneric<64>) -> Self {
        Error::AddressError(AddressError::InvalidPeerId(hash))
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::IoError(error.kind())
    }
}

impl From<io::Error> for SubstreamError {
    fn from(error: io::Error) -> SubstreamError {
        SubstreamError::IoError(error.kind())
    }
}

impl From<io::Error> for DialError {
    fn from(error: io::Error) -> Self {
        DialError::NegotiationError(NegotiationError::IoError(error.kind()))
    }
}

impl From<crate::multistream_select::NegotiationError> for Error {
    fn from(error: crate::multistream_select::NegotiationError) -> Error {
        Error::NegotiationError(NegotiationError::MultistreamSelectError(error))
    }
}

impl From<snow::Error> for Error {
    fn from(error: snow::Error) -> Self {
        Error::NegotiationError(NegotiationError::SnowError(error))
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for Error {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Error::EssentialTaskClosed
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for Error {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        Error::EssentialTaskClosed
    }
}

impl From<prost::DecodeError> for Error {
    fn from(error: prost::DecodeError) -> Self {
        Error::ParseError(ParseError::ProstDecodeError(error))
    }
}

impl From<prost::EncodeError> for Error {
    fn from(error: prost::EncodeError) -> Self {
        Error::ParseError(ParseError::ProstEncodeError(error))
    }
}

impl From<io::Error> for NegotiationError {
    fn from(error: io::Error) -> Self {
        NegotiationError::IoError(error.kind())
    }
}

impl From<ParseError> for Error {
    fn from(error: ParseError) -> Self {
        Error::ParseError(error)
    }
}

impl From<MultihashGeneric<64>> for AddressError {
    fn from(hash: MultihashGeneric<64>) -> Self {
        AddressError::InvalidPeerId(hash)
    }
}

#[cfg(feature = "quic")]
impl From<quinn::ConnectionError> for Error {
    fn from(error: quinn::ConnectionError) -> Self {
        match error {
            quinn::ConnectionError::TimedOut => Error::Timeout,
            error => Error::Quinn(error),
        }
    }
}

#[cfg(feature = "quic")]
impl From<quinn::ConnectionError> for DialError {
    fn from(error: quinn::ConnectionError) -> Self {
        match error {
            quinn::ConnectionError::TimedOut => DialError::Timeout,
            error => DialError::NegotiationError(NegotiationError::Quic(error.into())),
        }
    }
}

#[cfg(feature = "quic")]
impl From<quinn::ConnectError> for DialError {
    fn from(error: quinn::ConnectError) -> Self {
        DialError::NegotiationError(NegotiationError::Quic(error.into()))
    }
}

impl From<ConnectionLimitsError> for Error {
    fn from(error: ConnectionLimitsError) -> Self {
        Error::ConnectionLimit(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::{channel, Sender};

    #[tokio::test]
    async fn try_from_errors() {
        let (tx, rx) = channel(1);
        drop(rx);

        async fn test(tx: Sender<()>) -> crate::Result<()> {
            tx.send(()).await.map_err(From::from)
        }

        match test(tx).await.unwrap_err() {
            Error::EssentialTaskClosed => {}
            _ => panic!("invalid error"),
        }
    }
}
