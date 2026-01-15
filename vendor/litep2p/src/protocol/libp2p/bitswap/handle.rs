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

//! Bitswap handle for communicating with the bitswap protocol implementation.

use crate::{
    protocol::libp2p::bitswap::{BlockPresenceType, WantType},
    PeerId,
};

use cid::Cid;
use tokio::sync::mpsc::{Receiver, Sender};

use std::{
    pin::Pin,
    task::{Context, Poll},
};

/// Events emitted by the bitswap protocol.
#[derive(Debug)]
pub enum BitswapEvent {
    /// Bitswap request.
    Request {
        /// Peer ID.
        peer: PeerId,

        /// Requested CIDs.
        cids: Vec<(Cid, WantType)>,
    },
}

/// Response type for received bitswap request.
#[derive(Debug)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub enum ResponseType {
    /// Block.
    Block {
        /// CID.
        cid: Cid,

        /// Found block.
        block: Vec<u8>,
    },

    /// Presense.
    Presence {
        /// CID.
        cid: Cid,

        /// Whether the requested block exists or not.
        presence: BlockPresenceType,
    },
}

/// Commands sent from the user to `Bitswap`.
#[derive(Debug)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub enum BitswapCommand {
    /// Send bitswap response.
    SendResponse {
        /// Peer ID.
        peer: PeerId,

        /// CIDs.
        responses: Vec<ResponseType>,
    },
}

/// Handle for communicating with the bitswap protocol.
pub struct BitswapHandle {
    /// RX channel for receiving bitswap events.
    event_rx: Receiver<BitswapEvent>,

    /// TX channel for sending commads to `Bitswap`.
    cmd_tx: Sender<BitswapCommand>,
}

impl BitswapHandle {
    /// Create new [`BitswapHandle`].
    pub(super) fn new(event_rx: Receiver<BitswapEvent>, cmd_tx: Sender<BitswapCommand>) -> Self {
        Self { event_rx, cmd_tx }
    }

    /// Send `request` to `peer`.
    ///
    /// Not supported by the current implementation.
    pub async fn send_request(&self, _peer: PeerId, _request: Vec<u8>) {
        unimplemented!("bitswap requests are not supported");
    }

    /// Send `response` to `peer`.
    pub async fn send_response(&self, peer: PeerId, responses: Vec<ResponseType>) {
        let _ = self.cmd_tx.send(BitswapCommand::SendResponse { peer, responses }).await;
    }

    #[cfg(feature = "fuzz")]
    /// Expose functionality for fuzzing
    pub async fn fuzz_send_message(&mut self, command: BitswapCommand) -> crate::Result<()> {
        let _ = self.cmd_tx.try_send(command);
        Ok(())
    }
}

impl futures::Stream for BitswapHandle {
    type Item = BitswapEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.event_rx).poll_recv(cx)
    }
}
