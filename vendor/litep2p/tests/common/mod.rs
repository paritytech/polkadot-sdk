// Copyright 2024 litep2p developers
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

use litep2p::{config::ConfigBuilder, transport::tcp::config::Config as TcpConfig};

#[cfg(feature = "quic")]
use litep2p::transport::quic::config::Config as QuicConfig;
#[cfg(feature = "websocket")]
use litep2p::transport::websocket::config::Config as WebSocketConfig;

pub(crate) enum Transport {
    Tcp(TcpConfig),
    #[cfg(feature = "quic")]
    Quic(QuicConfig),
    #[cfg(feature = "websocket")]
    WebSocket(WebSocketConfig),
}

pub(crate) fn add_transport(config: ConfigBuilder, transport: Transport) -> ConfigBuilder {
    match transport {
        Transport::Tcp(transport) => config.with_tcp(transport),
        #[cfg(feature = "quic")]
        Transport::Quic(transport) => config.with_quic(transport),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(transport) => config.with_websocket(transport),
    }
}
