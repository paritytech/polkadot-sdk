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
    codec::ProtocolCodec, protocol::libp2p::ping::PingEvent, types::protocol::ProtocolName,
    DEFAULT_CHANNEL_SIZE,
};

use futures::Stream;
use tokio::sync::mpsc::{channel, Sender};
use tokio_stream::wrappers::ReceiverStream;

/// IPFS Ping protocol name as a string.
pub const PROTOCOL_NAME: &str = "/ipfs/ping/1.0.0";

/// Size for `/ipfs/ping/1.0.0` payloads.
const PING_PAYLOAD_SIZE: usize = 32;

/// Maximum PING failures.
const MAX_FAILURES: usize = 3;

/// Ping configuration.
pub struct Config {
    /// Protocol name.
    pub(crate) protocol: ProtocolName,

    /// Codec used by the protocol.
    pub(crate) codec: ProtocolCodec,

    /// Maximum failures before the peer is considered unreachable.
    pub(crate) max_failures: usize,

    /// TX channel for sending events to the user protocol.
    pub(crate) tx_event: Sender<PingEvent>,
}

impl Config {
    /// Create new [`Config`] with default values.
    ///
    /// Returns a config that is given to `Litep2pConfig` and an event stream for [`PingEvent`]s.
    pub fn default() -> (Self, Box<dyn Stream<Item = PingEvent> + Send + Unpin>) {
        let (tx_event, rx_event) = channel(DEFAULT_CHANNEL_SIZE);

        (
            Self {
                tx_event,
                max_failures: MAX_FAILURES,
                protocol: ProtocolName::from(PROTOCOL_NAME),
                codec: ProtocolCodec::Identity(PING_PAYLOAD_SIZE),
            },
            Box::new(ReceiverStream::new(rx_event)),
        )
    }
}

/// Ping configuration builder.
pub struct ConfigBuilder {
    /// Protocol name.
    protocol: ProtocolName,

    /// Codec used by the protocol.
    codec: ProtocolCodec,

    /// Maximum failures before the peer is considered unreachable.
    max_failures: usize,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    /// Create new default [`Config`] which can be modified by the user.
    pub fn new() -> Self {
        Self {
            max_failures: MAX_FAILURES,
            protocol: ProtocolName::from(PROTOCOL_NAME),
            codec: ProtocolCodec::Identity(PING_PAYLOAD_SIZE),
        }
    }

    /// Set maximum failures the protocol.
    pub fn with_max_failure(mut self, max_failures: usize) -> Self {
        self.max_failures = max_failures;
        self
    }

    /// Build [`Config`].
    pub fn build(self) -> (Config, Box<dyn Stream<Item = PingEvent> + Send + Unpin>) {
        let (tx_event, rx_event) = channel(DEFAULT_CHANNEL_SIZE);

        (
            Config {
                tx_event,
                max_failures: self.max_failures,
                protocol: self.protocol,
                codec: self.codec,
            },
            Box::new(ReceiverStream::new(rx_event)),
        )
    }
}
