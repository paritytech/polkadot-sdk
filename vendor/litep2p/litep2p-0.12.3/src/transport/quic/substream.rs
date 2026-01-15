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

use crate::{error::SubstreamError, BandwidthSink};

use bytes::Bytes;
use futures::{AsyncRead, AsyncWrite};
use quinn::{RecvStream, SendStream};
use tokio::io::{AsyncRead as TokioAsyncRead, AsyncWrite as TokioAsyncWrite};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use crate::protocol::Permit;

/// QUIC substream.
#[derive(Debug)]
pub struct Substream {
    _permit: Permit,
    bandwidth_sink: BandwidthSink,
    send_stream: SendStream,
    recv_stream: RecvStream,
}

impl Substream {
    /// Create new [`Substream`].
    pub fn new(
        _permit: Permit,
        send_stream: SendStream,
        recv_stream: RecvStream,
        bandwidth_sink: BandwidthSink,
    ) -> Self {
        Self {
            _permit,
            send_stream,
            recv_stream,
            bandwidth_sink,
        }
    }

    /// Write `buffers` to the underlying socket.
    pub async fn write_all_chunks(&mut self, buffers: &mut [Bytes]) -> Result<(), SubstreamError> {
        let nwritten = buffers.iter().fold(0usize, |acc, buffer| acc + buffer.len());

        match self
            .send_stream
            .write_all_chunks(buffers)
            .await
            .map_err(|_| SubstreamError::ConnectionClosed)
        {
            Ok(()) => {
                self.bandwidth_sink.increase_outbound(nwritten);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

impl TokioAsyncRead for Substream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match futures::ready!(Pin::new(&mut self.recv_stream).poll_read(cx, buf)) {
            Err(error) => Poll::Ready(Err(error)),
            Ok(res) => {
                self.bandwidth_sink.increase_inbound(buf.filled().len());
                Poll::Ready(Ok(res))
            }
        }
    }
}

impl TokioAsyncWrite for Substream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match futures::ready!(Pin::new(&mut self.send_stream).poll_write(cx, buf)) {
            Err(error) => Poll::Ready(Err(error)),
            Ok(nwritten) => {
                self.bandwidth_sink.increase_outbound(nwritten);
                Poll::Ready(Ok(nwritten))
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.send_stream).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.send_stream).poll_shutdown(cx)
    }
}

/// Substream pair used to negotiate a protocol for the connection.
pub struct NegotiatingSubstream {
    recv_stream: Compat<RecvStream>,
    send_stream: Compat<SendStream>,
}

impl NegotiatingSubstream {
    /// Create new [`NegotiatingSubstream`].
    pub fn new(send_stream: SendStream, recv_stream: RecvStream) -> Self {
        Self {
            recv_stream: TokioAsyncReadCompatExt::compat(recv_stream),
            send_stream: TokioAsyncWriteCompatExt::compat_write(send_stream),
        }
    }

    /// Deconstruct [`NegotiatingSubstream`] into parts.
    pub fn into_parts(self) -> (SendStream, RecvStream) {
        let sender = self.send_stream.into_inner();
        let receiver = self.recv_stream.into_inner();

        (sender, receiver)
    }
}

impl AsyncRead for NegotiatingSubstream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.recv_stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for NegotiatingSubstream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.send_stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send_stream).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send_stream).poll_close(cx)
    }
}
