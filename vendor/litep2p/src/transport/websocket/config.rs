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

//! WebSocket transport configuration.

use crate::{
    crypto::noise::{MAX_READ_AHEAD_FACTOR, MAX_WRITE_BUFFER_SIZE},
    transport::{CONNECTION_OPEN_TIMEOUT, SUBSTREAM_OPEN_TIMEOUT},
};

/// WebSocket transport configuration.
#[derive(Debug)]
pub struct Config {
    /// Listen address address for the transport.
    ///
    /// Default listen addreses are ["/ip4/0.0.0.0/tcp/0/ws", "/ip6/::/tcp/0/ws"].
    pub listen_addresses: Vec<multiaddr::Multiaddr>,

    /// Whether to set `SO_REUSEPORT` and bind a socket to the listen address port for outbound
    /// connections.
    ///
    /// Note that `SO_REUSEADDR` is always set on listening sockets.
    ///
    /// Defaults to `true`.
    pub reuse_port: bool,

    /// Enable `TCP_NODELAY`.
    ///
    /// Defaults to `false`.
    pub nodelay: bool,

    /// Yamux configuration.
    pub yamux_config: crate::yamux::Config,

    /// Noise read-ahead frame count.
    ///
    /// Specifies how many Noise frames are read per call to the underlying socket.
    ///
    /// By default this is configured to `5` so each call to the underlying socket can read up
    /// to `5` Noise frame per call. Fewer frames may be read if there isn't enough data in the
    /// socket. Each Noise frame is `65 KB` so the default setting allocates `65 KB * 5 = 325 KB`
    /// per connection.
    pub noise_read_ahead_frame_count: usize,

    /// Noise write buffer size.
    ///
    /// Specifes how many Noise frames are tried to be coalesced into a single system call.
    /// By default the value is set to `2` which means that the `NoiseSocket` will allocate
    /// `130 KB` for each outgoing connection.
    ///
    /// The write buffer size is separate from the read-ahead frame count so by default
    /// the Noise code will allocate `2 * 65 KB + 5 * 65 KB = 455 KB` per connection.
    pub noise_write_buffer_size: usize,

    /// Connection open timeout.
    ///
    /// How long should litep2p wait for a connection to be opened before the host
    /// is deemed unreachable.
    pub connection_open_timeout: std::time::Duration,

    /// Substream open timeout.
    ///
    /// How long should litep2p wait for a substream to be opened before considering
    /// the substream rejected.
    pub substream_open_timeout: std::time::Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/0/ws".parse().expect("valid address"),
                "/ip6/::/tcp/0/ws".parse().expect("valid address"),
            ],
            reuse_port: true,
            nodelay: false,
            yamux_config: Default::default(),
            noise_read_ahead_frame_count: MAX_READ_AHEAD_FACTOR,
            noise_write_buffer_size: MAX_WRITE_BUFFER_SIZE,
            connection_open_timeout: CONNECTION_OPEN_TIMEOUT,
            substream_open_timeout: SUBSTREAM_OPEN_TIMEOUT,
        }
    }
}
