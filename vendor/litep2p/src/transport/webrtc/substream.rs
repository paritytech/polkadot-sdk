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

use crate::{
    transport::webrtc::{schema::webrtc::message::Flag, util::WebRtcMessage},
    Error,
};

use bytes::{Buf, BufMut, BytesMut};
use futures::{Future, Stream};
use parking_lot::Mutex;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// Maximum frame size.
const MAX_FRAME_SIZE: usize = 16384;

/// Substream event.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// Receiver closed.
    RecvClosed,

    /// Send/receive message.
    Message(Vec<u8>),

    /// Close substream.
    Close,
}

/// Substream stream.
enum State {
    /// Substream is fully open.
    Open,

    /// Remote is no longer interested in receiving anything.
    SendClosed,
}

/// Channel-backedn substream.
pub struct Substream {
    /// Substream state.
    state: Arc<Mutex<State>>,

    /// Read buffer.
    read_buffer: BytesMut,

    /// TX channel for sending messages to `peer`.
    tx: Sender<Event>,

    /// RX channel for receiving messages from `peer`.
    rx: Receiver<Event>,
}

impl Substream {
    /// Create new [`Substream`].
    pub fn new() -> (Self, SubstreamHandle) {
        let (outbound_tx, outbound_rx) = channel(256);
        let (inbound_tx, inbound_rx) = channel(256);
        let state = Arc::new(Mutex::new(State::Open));
        let handle = SubstreamHandle {
            tx: inbound_tx,
            rx: outbound_rx,
            state: Arc::clone(&state),
        };

        (
            Self {
                state,
                tx: outbound_tx,
                rx: inbound_rx,
                read_buffer: BytesMut::new(),
            },
            handle,
        )
    }
}

/// Substream handle that is given to the transport backend.
pub struct SubstreamHandle {
    state: Arc<Mutex<State>>,

    /// TX channel for sending messages to `peer`.
    tx: Sender<Event>,

    /// RX channel for receiving messages from `peer`.
    rx: Receiver<Event>,
}

impl SubstreamHandle {
    /// Handle message received from a remote peer.
    ///
    /// If the message contains any flags, handle them first and appropriately close the correct
    /// side of the substream. If the message contained any payload, send it to the protocol for
    /// further processing.
    pub async fn on_message(&self, message: WebRtcMessage) -> crate::Result<()> {
        if let Some(flags) = message.flags {
            if flags == Flag::Fin as i32 {
                self.tx.send(Event::RecvClosed).await?;
            }

            if flags & 1 == Flag::StopSending as i32 {
                *self.state.lock() = State::SendClosed;
            }

            if flags & 2 == Flag::ResetStream as i32 {
                return Err(Error::ConnectionClosed);
            }
        }

        if let Some(payload) = message.payload {
            if !payload.is_empty() {
                return self.tx.send(Event::Message(payload)).await.map_err(From::from);
            }
        }

        Ok(())
    }
}

impl Stream for SubstreamHandle {
    type Item = Event;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl tokio::io::AsyncRead for Substream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // if there are any remaining bytes from a previous read, consume them first
        if self.read_buffer.remaining() > 0 {
            let num_bytes = std::cmp::min(self.read_buffer.remaining(), buf.remaining());

            buf.put_slice(&self.read_buffer[..num_bytes]);
            self.read_buffer.advance(num_bytes);

            // TODO: optimize by trying to read more data from substream and not exiting early
            return Poll::Ready(Ok(()));
        }

        match futures::ready!(self.rx.poll_recv(cx)) {
            None | Some(Event::Close) | Some(Event::RecvClosed) =>
                Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into())),
            Some(Event::Message(message)) => {
                if message.len() > MAX_FRAME_SIZE {
                    return Poll::Ready(Err(std::io::ErrorKind::PermissionDenied.into()));
                }

                match buf.remaining() >= message.len() {
                    true => buf.put_slice(&message),
                    false => {
                        let remaining = buf.remaining();
                        buf.put_slice(&message[..remaining]);
                        self.read_buffer.put_slice(&message[remaining..]);
                    }
                }

                Poll::Ready(Ok(()))
            }
        }
    }
}

impl tokio::io::AsyncWrite for Substream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        if let State::SendClosed = *self.state.lock() {
            return Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()));
        }

        // TODO: try to coalesce multiple calls to `poll_write()` into single `Event::Message`

        let num_bytes = std::cmp::min(MAX_FRAME_SIZE, buf.len());
        let future = self.tx.reserve();
        futures::pin_mut!(future);

        let permit = match futures::ready!(future.poll(cx)) {
            Err(_) => return Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into())),
            Ok(permit) => permit,
        };

        let frame = buf[..num_bytes].to_vec();
        permit.send(Event::Message(frame));

        Poll::Ready(Ok(num_bytes))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let future = self.tx.reserve();
        futures::pin_mut!(future);

        let permit = match futures::ready!(future.poll(cx)) {
            Err(_) => return Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into())),
            Ok(permit) => permit,
        };
        permit.send(Event::Close);

        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

    #[tokio::test]
    async fn write_small_frame() {
        let (mut substream, mut handle) = Substream::new();

        substream.write_all(&vec![0u8; 1337]).await.unwrap();

        assert_eq!(handle.next().await, Some(Event::Message(vec![0u8; 1337])));

        futures::future::poll_fn(|cx| match handle.poll_next_unpin(cx) {
            Poll::Pending => Poll::Ready(()),
            Poll::Ready(_) => panic!("invalid event"),
        })
        .await;
    }

    #[tokio::test]
    async fn write_large_frame() {
        let (mut substream, mut handle) = Substream::new();

        substream.write_all(&vec![0u8; (2 * MAX_FRAME_SIZE) + 1]).await.unwrap();

        assert_eq!(
            handle.rx.recv().await,
            Some(Event::Message(vec![0u8; MAX_FRAME_SIZE]))
        );
        assert_eq!(
            handle.rx.recv().await,
            Some(Event::Message(vec![0u8; MAX_FRAME_SIZE]))
        );
        assert_eq!(handle.rx.recv().await, Some(Event::Message(vec![0u8; 1])));

        futures::future::poll_fn(|cx| match handle.poll_next_unpin(cx) {
            Poll::Pending => Poll::Ready(()),
            Poll::Ready(_) => panic!("invalid event"),
        })
        .await;
    }

    #[tokio::test]
    async fn try_to_write_to_closed_substream() {
        let (mut substream, handle) = Substream::new();
        *handle.state.lock() = State::SendClosed;

        match substream.write_all(&vec![0u8; 1337]).await {
            Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe),
            _ => panic!("invalid event"),
        }
    }

    #[tokio::test]
    async fn substream_shutdown() {
        let (mut substream, mut handle) = Substream::new();

        substream.write_all(&vec![1u8; 1337]).await.unwrap();
        substream.shutdown().await.unwrap();

        assert_eq!(handle.next().await, Some(Event::Message(vec![1u8; 1337])));
        assert_eq!(handle.next().await, Some(Event::Close));
    }

    #[tokio::test]
    async fn try_to_read_from_closed_substream() {
        let (mut substream, handle) = Substream::new();
        handle
            .on_message(WebRtcMessage {
                payload: None,
                flags: Some(0i32),
            })
            .await
            .unwrap();

        match substream.read(&mut vec![0u8; 256]).await {
            Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe),
            _ => panic!("invalid event"),
        }
    }

    #[tokio::test]
    async fn read_small_frame() {
        let (mut substream, handle) = Substream::new();
        handle.tx.send(Event::Message(vec![1u8; 256])).await.unwrap();

        let mut buf = vec![0u8; 2048];

        match substream.read(&mut buf).await {
            Ok(nread) => {
                assert_eq!(nread, 256);
                assert_eq!(buf[..nread], vec![1u8; 256]);
            }
            Err(error) => panic!("invalid event: {error:?}"),
        }

        let mut read_buf = ReadBuf::new(&mut buf);
        futures::future::poll_fn(|cx| {
            match Pin::new(&mut substream).poll_read(cx, &mut read_buf) {
                Poll::Pending => Poll::Ready(()),
                _ => panic!("invalid event"),
            }
        })
        .await;
    }

    #[tokio::test]
    async fn read_small_frame_in_two_reads() {
        let (mut substream, handle) = Substream::new();
        let mut first = vec![1u8; 256];
        first.extend_from_slice(&vec![2u8; 256]);

        handle.tx.send(Event::Message(first)).await.unwrap();

        let mut buf = vec![0u8; 256];

        match substream.read(&mut buf).await {
            Ok(nread) => {
                assert_eq!(nread, 256);
                assert_eq!(buf[..nread], vec![1u8; 256]);
            }
            Err(error) => panic!("invalid event: {error:?}"),
        }

        match substream.read(&mut buf).await {
            Ok(nread) => {
                assert_eq!(nread, 256);
                assert_eq!(buf[..nread], vec![2u8; 256]);
            }
            Err(error) => panic!("invalid event: {error:?}"),
        }

        let mut read_buf = ReadBuf::new(&mut buf);
        futures::future::poll_fn(|cx| {
            match Pin::new(&mut substream).poll_read(cx, &mut read_buf) {
                Poll::Pending => Poll::Ready(()),
                _ => panic!("invalid event"),
            }
        })
        .await;
    }

    #[tokio::test]
    async fn read_frames() {
        let (mut substream, handle) = Substream::new();
        let mut first = vec![1u8; 256];
        first.extend_from_slice(&vec![2u8; 256]);

        handle.tx.send(Event::Message(first)).await.unwrap();
        handle.tx.send(Event::Message(vec![4u8; 2048])).await.unwrap();

        let mut buf = vec![0u8; 256];

        match substream.read(&mut buf).await {
            Ok(nread) => {
                assert_eq!(nread, 256);
                assert_eq!(buf[..nread], vec![1u8; 256]);
            }
            Err(error) => panic!("invalid event: {error:?}"),
        }

        let mut buf = vec![0u8; 128];

        match substream.read(&mut buf).await {
            Ok(nread) => {
                assert_eq!(nread, 128);
                assert_eq!(buf[..nread], vec![2u8; 128]);
            }
            Err(error) => panic!("invalid event: {error:?}"),
        }

        let mut buf = vec![0u8; 128];

        match substream.read(&mut buf).await {
            Ok(nread) => {
                assert_eq!(nread, 128);
                assert_eq!(buf[..nread], vec![2u8; 128]);
            }
            Err(error) => panic!("invalid event: {error:?}"),
        }

        let mut buf = vec![0u8; MAX_FRAME_SIZE];

        match substream.read(&mut buf).await {
            Ok(nread) => {
                assert_eq!(nread, 2048);
                assert_eq!(buf[..nread], vec![4u8; 2048]);
            }
            Err(error) => panic!("invalid event: {error:?}"),
        }

        let mut read_buf = ReadBuf::new(&mut buf);
        futures::future::poll_fn(|cx| {
            match Pin::new(&mut substream).poll_read(cx, &mut read_buf) {
                Poll::Pending => Poll::Ready(()),
                _ => panic!("invalid event"),
            }
        })
        .await;
    }

    #[tokio::test]
    async fn backpressure_works() {
        let (mut substream, _handle) = Substream::new();

        // use all available bandwidth which by default is `256 * MAX_FRAME_SIZE`,
        for _ in 0..128 {
            substream.write_all(&vec![0u8; 2 * MAX_FRAME_SIZE]).await.unwrap();
        }

        // try to write one more byte but since all available bandwidth
        // is taken the call will block
        futures::future::poll_fn(
            |cx| match Pin::new(&mut substream).poll_write(cx, &[0u8; 1]) {
                Poll::Pending => Poll::Ready(()),
                _ => panic!("invalid event"),
            },
        )
        .await;
    }
}
