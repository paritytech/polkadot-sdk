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

use crate::transport::manager::{address::AddressStore, peer_state::PeerState};

/// Supported protocols.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum SupportedTransport {
    /// TCP.
    Tcp,

    /// QUIC.
    #[cfg(feature = "quic")]
    Quic,

    /// WebRTC
    #[cfg(feature = "webrtc")]
    WebRtc,

    /// WebSocket
    #[cfg(feature = "websocket")]
    WebSocket,
}

/// Peer context.
#[derive(Debug)]
pub struct PeerContext {
    /// Peer state.
    pub state: PeerState,

    /// Known addresses of peer.
    pub addresses: AddressStore,
}

impl Default for PeerContext {
    fn default() -> Self {
        Self {
            state: PeerState::Disconnected { dial_record: None },
            addresses: AddressStore::new(),
        }
    }
}
