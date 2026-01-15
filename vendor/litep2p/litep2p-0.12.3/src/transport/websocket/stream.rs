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

//! Stream implementation for `tokio_tungstenite::WebSocketStream` that implements
//! `AsyncRead + AsyncWrite`

use bytes::{Buf, Bytes};
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use std::{
    pin::Pin,
    task::{Context, Poll},
};

const LOG_TARGET: &str = "litep2p::transport::websocket::stream";

/// Buffered stream which implements `AsyncRead + AsyncWrite`
#[derive(Debug)]
pub(super) struct BufferedStream<S: AsyncRead + AsyncWrite + Unpin> {
    /// Read buffer.
    ///
    /// The buffer is taken directly from the WebSocket stream.
    read_buffer: Bytes,

    /// Underlying WebSocket stream.
    stream: WebSocketStream<S>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> BufferedStream<S> {
    /// Create new [`BufferedStream`].
    pub(super) fn new(stream: WebSocketStream<S>) -> Self {
        Self {
            read_buffer: Bytes::new(),
            stream,
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> futures::AsyncWrite for BufferedStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match futures::ready!(self.stream.poll_ready_unpin(cx)) {
            Ok(()) => {
                let message = Message::Binary(Bytes::copy_from_slice(buf));

                if let Err(err) = self.stream.start_send_unpin(message) {
                    tracing::debug!(target: LOG_TARGET, "Error during start send: {:?}", err);
                    return Poll::Ready(Err(std::io::ErrorKind::UnexpectedEof.into()));
                }

                Poll::Ready(Ok(buf.len()))
            }
            Err(err) => {
                tracing::debug!(target: LOG_TARGET, "Error during poll ready: {:?}", err);
                Poll::Ready(Err(std::io::ErrorKind::UnexpectedEof.into()))
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.stream.poll_flush_unpin(cx).map_err(|err| {
            tracing::debug!(target: LOG_TARGET, "Error during poll flush: {:?}", err);
            std::io::ErrorKind::UnexpectedEof.into()
        })
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.stream.poll_close_unpin(cx).map_err(|err| {
            tracing::debug!(target: LOG_TARGET, "Error during poll close: {:?}", err);
            std::io::ErrorKind::PermissionDenied.into()
        })
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> futures::AsyncRead for BufferedStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        loop {
            if self.read_buffer.is_empty() {
                let next_chunk = match self.stream.poll_next_unpin(cx) {
                    Poll::Ready(Some(Ok(chunk))) => match chunk {
                        Message::Binary(chunk) => chunk,
                        _event => return Poll::Ready(Err(std::io::ErrorKind::Unsupported.into())),
                    },
                    Poll::Ready(Some(Err(_error))) =>
                        return Poll::Ready(Err(std::io::ErrorKind::UnexpectedEof.into())),
                    Poll::Ready(None) => return Poll::Ready(Ok(0)),
                    Poll::Pending => return Poll::Pending,
                };

                self.read_buffer = next_chunk;
                continue;
            }

            let len = std::cmp::min(self.read_buffer.len(), buf.len());
            buf[..len].copy_from_slice(&self.read_buffer[..len]);
            self.read_buffer.advance(len);
            return Poll::Ready(Ok(len));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{AsyncRead, AsyncReadExt, AsyncWriteExt};
    use tokio::io::DuplexStream;
    use tokio_tungstenite::{tungstenite::protocol::Role, WebSocketStream};

    async fn create_test_stream() -> (BufferedStream<DuplexStream>, BufferedStream<DuplexStream>) {
        let (client, server) = tokio::io::duplex(1024);

        (
            BufferedStream::new(WebSocketStream::from_raw_socket(client, Role::Client, None).await),
            BufferedStream::new(WebSocketStream::from_raw_socket(server, Role::Server, None).await),
        )
    }

    #[tokio::test]
    async fn test_write_to_buffer() {
        let (mut stream, mut _server) = create_test_stream().await;
        let data = b"hello";

        let bytes_written = stream.write(data).await.unwrap();
        assert_eq!(bytes_written, data.len());
    }

    #[tokio::test]
    async fn test_flush_empty_buffer() {
        let (mut stream, mut _server) = create_test_stream().await;
        assert!(stream.flush().await.is_ok());
    }

    #[tokio::test]
    async fn test_write_and_flush() {
        let (mut stream, mut _server) = create_test_stream().await;
        let data = b"hello world";

        stream.write_all(data).await.unwrap();
        assert!(stream.flush().await.is_ok());
    }

    #[tokio::test]
    async fn test_close_stream() {
        let (mut stream, mut _server) = create_test_stream().await;
        assert!(stream.close().await.is_ok());
    }

    #[tokio::test]
    async fn test_ping_pong_stream() {
        let (mut stream, mut server) = create_test_stream().await;
        stream.write(b"hello").await.unwrap();
        assert!(stream.flush().await.is_ok());

        let mut message = [0u8; 5];
        server.read(&mut message).await.unwrap();
        assert_eq!(&message, b"hello");

        server.write(b"world").await.unwrap();
        assert!(server.flush().await.is_ok());

        stream.read(&mut message).await.unwrap();
        assert_eq!(&message, b"world");

        assert!(stream.close().await.is_ok());
        drop(stream);

        assert!(server.write(b"world").await.is_ok());
        match server.flush().await {
            Err(error) => if error.kind() == std::io::ErrorKind::UnexpectedEof {},
            state => panic!("Unexpected state {state:?}"),
        };
    }

    #[tokio::test]
    async fn test_read_poll_pending() {
        let (mut stream, mut _server) = create_test_stream().await;

        let mut buffer = [0u8; 10];
        let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());
        let pin_stream = Pin::new(&mut stream);

        assert!(matches!(
            pin_stream.poll_read(&mut cx, &mut buffer),
            Poll::Pending
        ));
    }

    #[tokio::test]
    async fn test_read_from_internal_buffers() {
        let (mut stream, server) = create_test_stream().await;
        drop(server);

        stream.read_buffer = Bytes::from_static(b"hello world");

        let mut buffer = [0u8; 32];
        let bytes_read = stream.read(&mut buffer).await.unwrap();
        assert_eq!(bytes_read, 11);
        assert_eq!(&buffer[..bytes_read], b"hello world");
    }
}
