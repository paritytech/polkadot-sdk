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

//! QUIC transport configuration.

use crate::transport::{CONNECTION_OPEN_TIMEOUT, SUBSTREAM_OPEN_TIMEOUT};

use multiaddr::Multiaddr;

use std::time::Duration;

/// QUIC transport configuration.
#[derive(Debug)]
pub struct Config {
    /// Listen address for the transport.
    ///
    /// Default listen addres is `/ip4/127.0.0.1/udp/0/quic-v1`.
    pub listen_addresses: Vec<Multiaddr>,

    /// Connection open timeout.
    ///
    /// How long should litep2p wait for a connection to be opend before the host
    /// is deemed unreachable.
    pub connection_open_timeout: Duration,

    /// Substream open timeout.
    ///
    /// How long should litep2p wait for a substream to be opened before considering
    /// the substream rejected.
    pub substream_open_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_addresses: vec!["/ip4/127.0.0.1/udp/0/quic-v1".parse().expect("valid address")],
            connection_open_timeout: CONNECTION_OPEN_TIMEOUT,
            substream_open_timeout: SUBSTREAM_OPEN_TIMEOUT,
        }
    }
}
