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

use crate::{protocol::Permit, BandwidthSink};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::compat::Compat;

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

/// Substream that holds the inner substream provided by the transport
/// and a permit which keeps the connection open.
#[derive(Debug)]
pub struct Substream {
    /// Underlying socket.
    io: Compat<crate::yamux::Stream>,

    /// Bandwidth sink.
    bandwidth_sink: BandwidthSink,

    /// Connection permit.
    _permit: Permit,
}

impl Substream {
    /// Create new [`Substream`].
    pub fn new(
        io: Compat<crate::yamux::Stream>,
        bandwidth_sink: BandwidthSink,
        _permit: Permit,
    ) -> Self {
        Self {
            io,
            bandwidth_sink,
            _permit,
        }
    }
}

impl AsyncRead for Substream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let len = buf.filled().len();
        match futures::ready!(Pin::new(&mut self.io).poll_read(cx, buf)) {
            Err(error) => Poll::Ready(Err(error)),
            Ok(res) => {
                let inbound_size = buf.filled().len().saturating_sub(len);
                self.bandwidth_sink.increase_inbound(inbound_size);
                Poll::Ready(Ok(res))
            }
        }
    }
}

impl AsyncWrite for Substream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match futures::ready!(Pin::new(&mut self.io).poll_write(cx, buf)) {
            Err(error) => Poll::Ready(Err(error)),
            Ok(nwritten) => {
                self.bandwidth_sink.increase_outbound(nwritten);
                Poll::Ready(Ok(nwritten))
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}
