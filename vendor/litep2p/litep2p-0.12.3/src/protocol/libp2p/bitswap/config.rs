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
    protocol::libp2p::bitswap::{BitswapCommand, BitswapEvent, BitswapHandle},
    types::protocol::ProtocolName,
    DEFAULT_CHANNEL_SIZE,
};

use tokio::sync::mpsc::{channel, Receiver, Sender};

/// IPFS Bitswap protocol name as a string.
pub const PROTOCOL_NAME: &str = "/ipfs/bitswap/1.2.0";

/// Maximum Size for `/ipfs/bitswap/1.2.0` payloads.
const MAX_PAYLOAD_SIZE: usize = 2_097_152;

/// Bitswap configuration.
#[derive(Debug)]
pub struct Config {
    /// Protocol name.
    pub(crate) protocol: ProtocolName,

    /// Protocol codec.
    pub(crate) codec: ProtocolCodec,

    /// TX channel for sending events to the user protocol.
    pub(super) event_tx: Sender<BitswapEvent>,

    /// RX channel for receiving commands from the user.
    pub(super) cmd_rx: Receiver<BitswapCommand>,
}

impl Config {
    /// Create new [`Config`].
    pub fn new() -> (Self, BitswapHandle) {
        let (event_tx, event_rx) = channel(DEFAULT_CHANNEL_SIZE);
        let (cmd_tx, cmd_rx) = channel(DEFAULT_CHANNEL_SIZE);

        (
            Self {
                cmd_rx,
                event_tx,
                protocol: ProtocolName::from(PROTOCOL_NAME),
                codec: ProtocolCodec::UnsignedVarint(Some(MAX_PAYLOAD_SIZE)),
            },
            BitswapHandle::new(event_rx, cmd_tx),
        )
    }
}
