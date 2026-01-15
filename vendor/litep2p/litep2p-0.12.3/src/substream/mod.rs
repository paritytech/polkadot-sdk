// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! Substream-related helper code.

use crate::{
    codec::ProtocolCodec, error::SubstreamError, transport::tcp, types::SubstreamId, PeerId,
};

#[cfg(feature = "quic")]
use crate::transport::quic;
#[cfg(feature = "webrtc")]
use crate::transport::webrtc;
#[cfg(feature = "websocket")]
use crate::transport::websocket;

use bytes::{Buf, Bytes, BytesMut};
use futures::{Sink, Stream};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use unsigned_varint::{decode, encode};

use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    fmt,
    hash::Hash,
    io::ErrorKind,
    pin::Pin,
    task::{Context, Poll},
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::substream";

macro_rules! poll_flush {
    ($substream:expr, $cx:ident) => {{
        match $substream {
            SubstreamType::Tcp(substream) => Pin::new(substream).poll_flush($cx),
            #[cfg(feature = "websocket")]
            SubstreamType::WebSocket(substream) => Pin::new(substream).poll_flush($cx),
            #[cfg(feature = "quic")]
            SubstreamType::Quic(substream) => Pin::new(substream).poll_flush($cx),
            #[cfg(feature = "webrtc")]
            SubstreamType::WebRtc(substream) => Pin::new(substream).poll_flush($cx),
            #[cfg(test)]
            SubstreamType::Mock(_) => unreachable!(),
        }
    }};
}

macro_rules! poll_write {
    ($substream:expr, $cx:ident, $frame:expr) => {{
        match $substream {
            SubstreamType::Tcp(substream) => Pin::new(substream).poll_write($cx, $frame),
            #[cfg(feature = "websocket")]
            SubstreamType::WebSocket(substream) => Pin::new(substream).poll_write($cx, $frame),
            #[cfg(feature = "quic")]
            SubstreamType::Quic(substream) => Pin::new(substream).poll_write($cx, $frame),
            #[cfg(feature = "webrtc")]
            SubstreamType::WebRtc(substream) => Pin::new(substream).poll_write($cx, $frame),
            #[cfg(test)]
            SubstreamType::Mock(_) => unreachable!(),
        }
    }};
}

macro_rules! poll_read {
    ($substream:expr, $cx:ident, $buffer:expr) => {{
        match $substream {
            SubstreamType::Tcp(substream) => Pin::new(substream).poll_read($cx, $buffer),
            #[cfg(feature = "websocket")]
            SubstreamType::WebSocket(substream) => Pin::new(substream).poll_read($cx, $buffer),
            #[cfg(feature = "quic")]
            SubstreamType::Quic(substream) => Pin::new(substream).poll_read($cx, $buffer),
            #[cfg(feature = "webrtc")]
            SubstreamType::WebRtc(substream) => Pin::new(substream).poll_read($cx, $buffer),
            #[cfg(test)]
            SubstreamType::Mock(_) => unreachable!(),
        }
    }};
}

macro_rules! poll_shutdown {
    ($substream:expr, $cx:ident) => {{
        match $substream {
            SubstreamType::Tcp(substream) => Pin::new(substream).poll_shutdown($cx),
            #[cfg(feature = "websocket")]
            SubstreamType::WebSocket(substream) => Pin::new(substream).poll_shutdown($cx),
            #[cfg(feature = "quic")]
            SubstreamType::Quic(substream) => Pin::new(substream).poll_shutdown($cx),
            #[cfg(feature = "webrtc")]
            SubstreamType::WebRtc(substream) => Pin::new(substream).poll_shutdown($cx),
            #[cfg(test)]
            SubstreamType::Mock(substream) => {
                let _ = Pin::new(substream).poll_close($cx);
                todo!();
            }
        }
    }};
}

macro_rules! delegate_poll_next {
    ($substream:expr, $cx:ident) => {{
        #[cfg(test)]
        if let SubstreamType::Mock(inner) = $substream {
            return Pin::new(inner).poll_next($cx);
        }
    }};
}

macro_rules! delegate_poll_ready {
    ($substream:expr, $cx:ident) => {{
        #[cfg(test)]
        if let SubstreamType::Mock(inner) = $substream {
            return Pin::new(inner).poll_ready($cx);
        }
    }};
}

macro_rules! delegate_start_send {
    ($substream:expr, $item:ident) => {{
        #[cfg(test)]
        if let SubstreamType::Mock(inner) = $substream {
            return Pin::new(inner).start_send($item);
        }
    }};
}

macro_rules! delegate_poll_flush {
    ($substream:expr, $cx:ident) => {{
        #[cfg(test)]
        if let SubstreamType::Mock(inner) = $substream {
            return Pin::new(inner).poll_flush($cx);
        }
    }};
}

macro_rules! check_size {
    ($max_size:expr, $size:expr) => {{
        if let Some(max_size) = $max_size {
            if $size > max_size {
                return Err(SubstreamError::IoError(ErrorKind::PermissionDenied).into());
            }
        }
    }};
}

/// Substream type.
enum SubstreamType {
    Tcp(tcp::Substream),
    #[cfg(feature = "websocket")]
    WebSocket(websocket::Substream),
    #[cfg(feature = "quic")]
    Quic(quic::Substream),
    #[cfg(feature = "webrtc")]
    WebRtc(webrtc::Substream),
    #[cfg(test)]
    Mock(Box<dyn crate::mock::substream::Substream>),
}

impl fmt::Debug for SubstreamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp(_) => write!(f, "Tcp"),
            #[cfg(feature = "websocket")]
            Self::WebSocket(_) => write!(f, "WebSocket"),
            #[cfg(feature = "quic")]
            Self::Quic(_) => write!(f, "Quic"),
            #[cfg(feature = "webrtc")]
            Self::WebRtc(_) => write!(f, "WebRtc"),
            #[cfg(test)]
            Self::Mock(_) => write!(f, "Mock"),
        }
    }
}

/// Backpressure boundary for `Sink`.
const BACKPRESSURE_BOUNDARY: usize = 65536;

/// `Litep2p` substream type.
///
/// Implements [`tokio::io::AsyncRead`]/[`tokio::io::AsyncWrite`] traits which can be wrapped
/// in a `Framed` to implement a custom codec.
///
/// In case a codec for the protocol was specified,
/// [`Sink::send()`](futures::Sink)/[`Stream::next()`](futures::Stream) are also provided which
/// implement the necessary framing to read/write codec-encoded messages from the underlying socket.
pub struct Substream {
    /// Remote peer ID.
    peer: PeerId,

    // Inner substream.
    substream: SubstreamType,

    /// Substream ID.
    substream_id: SubstreamId,

    /// Protocol codec.
    codec: ProtocolCodec,

    pending_out_frames: VecDeque<Bytes>,
    pending_out_bytes: usize,
    pending_out_frame: Option<Bytes>,

    read_buffer: BytesMut,
    offset: usize,
    pending_frames: VecDeque<BytesMut>,
    current_frame_size: Option<usize>,

    size_vec: BytesMut,
}

impl fmt::Debug for Substream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Substream")
            .field("peer", &self.peer)
            .field("substream_id", &self.substream_id)
            .field("codec", &self.codec)
            .field("protocol", &self.substream)
            .finish()
    }
}

impl Substream {
    /// Create new [`Substream`].
    fn new(
        peer: PeerId,
        substream_id: SubstreamId,
        substream: SubstreamType,
        codec: ProtocolCodec,
    ) -> Self {
        Self {
            peer,
            substream,
            codec,
            substream_id,
            read_buffer: BytesMut::zeroed(1024),
            offset: 0usize,
            pending_frames: VecDeque::new(),
            current_frame_size: None,
            pending_out_bytes: 0usize,
            pending_out_frames: VecDeque::new(),
            pending_out_frame: None,
            size_vec: BytesMut::zeroed(10),
        }
    }

    /// Create new [`Substream`] for TCP.
    pub(crate) fn new_tcp(
        peer: PeerId,
        substream_id: SubstreamId,
        substream: tcp::Substream,
        codec: ProtocolCodec,
    ) -> Self {
        tracing::trace!(target: LOG_TARGET, ?peer, ?codec, "create new substream for tcp");

        Self::new(peer, substream_id, SubstreamType::Tcp(substream), codec)
    }

    /// Create new [`Substream`] for WebSocket.
    #[cfg(feature = "websocket")]
    pub(crate) fn new_websocket(
        peer: PeerId,
        substream_id: SubstreamId,
        substream: websocket::Substream,
        codec: ProtocolCodec,
    ) -> Self {
        tracing::trace!(target: LOG_TARGET, ?peer, ?codec, "create new substream for websocket");

        Self::new(
            peer,
            substream_id,
            SubstreamType::WebSocket(substream),
            codec,
        )
    }

    /// Create new [`Substream`] for QUIC.
    #[cfg(feature = "quic")]
    pub(crate) fn new_quic(
        peer: PeerId,
        substream_id: SubstreamId,
        substream: quic::Substream,
        codec: ProtocolCodec,
    ) -> Self {
        tracing::trace!(target: LOG_TARGET, ?peer, ?codec, "create new substream for quic");

        Self::new(peer, substream_id, SubstreamType::Quic(substream), codec)
    }

    /// Create new [`Substream`] for WebRTC.
    #[cfg(feature = "webrtc")]
    pub(crate) fn new_webrtc(
        peer: PeerId,
        substream_id: SubstreamId,
        substream: webrtc::Substream,
        codec: ProtocolCodec,
    ) -> Self {
        tracing::trace!(target: LOG_TARGET, ?peer, ?codec, "create new substream for webrtc");

        Self::new(peer, substream_id, SubstreamType::WebRtc(substream), codec)
    }

    /// Create new [`Substream`] for mocking.
    #[cfg(test)]
    pub(crate) fn new_mock(
        peer: PeerId,
        substream_id: SubstreamId,
        substream: Box<dyn crate::mock::substream::Substream>,
    ) -> Self {
        tracing::trace!(target: LOG_TARGET, ?peer, "create new substream for mocking");

        Self::new(
            peer,
            substream_id,
            SubstreamType::Mock(substream),
            ProtocolCodec::Unspecified,
        )
    }

    /// Close the substream.
    pub async fn close(self) {
        let _ = match self.substream {
            SubstreamType::Tcp(mut substream) => substream.shutdown().await,
            #[cfg(feature = "websocket")]
            SubstreamType::WebSocket(mut substream) => substream.shutdown().await,
            #[cfg(feature = "quic")]
            SubstreamType::Quic(mut substream) => substream.shutdown().await,
            #[cfg(feature = "webrtc")]
            SubstreamType::WebRtc(mut substream) => substream.shutdown().await,
            #[cfg(test)]
            SubstreamType::Mock(mut substream) => {
                let _ = futures::SinkExt::close(&mut substream).await;
                Ok(())
            }
        };
    }

    /// Send identity payload to remote peer.
    async fn send_identity_payload<T: AsyncWrite + Unpin>(
        io: &mut T,
        payload_size: usize,
        payload: Bytes,
    ) -> Result<(), SubstreamError> {
        if payload.len() != payload_size {
            return Err(SubstreamError::IoError(ErrorKind::PermissionDenied));
        }

        io.write_all(&payload).await.map_err(|_| SubstreamError::ConnectionClosed)?;

        // Flush the stream.
        io.flush().await.map_err(From::from)
    }

    /// Send unsigned varint payload to remote peer.
    async fn send_unsigned_varint_payload<T: AsyncWrite + Unpin>(
        io: &mut T,
        bytes: Bytes,
        max_size: Option<usize>,
    ) -> Result<(), SubstreamError> {
        if let Some(max_size) = max_size {
            if bytes.len() > max_size {
                return Err(SubstreamError::IoError(ErrorKind::PermissionDenied));
            }
        }

        // Write the length of the frame.
        let mut buffer = unsigned_varint::encode::usize_buffer();
        let encoded_len = unsigned_varint::encode::usize(bytes.len(), &mut buffer).len();
        io.write_all(&buffer[..encoded_len]).await?;

        // Write the frame.
        io.write_all(bytes.as_ref()).await?;

        // Flush the stream.
        io.flush().await.map_err(From::from)
    }

    /// Send framed data to remote peer.
    ///
    /// This function may be faster than the provided [`futures::Sink`] implementation for
    /// [`Substream`] as it has direct access to the API of the underlying socket as opposed
    /// to going through [`tokio::io::AsyncWrite`].
    ///
    /// # Cancel safety
    ///
    /// This method is not cancellation safe. If that is required, use the provided
    /// [`futures::Sink`] implementation.
    ///
    /// # Panics
    ///
    /// Panics if no codec is provided.
    pub async fn send_framed(&mut self, bytes: Bytes) -> Result<(), SubstreamError> {
        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            codec = ?self.codec,
            frame_len = ?bytes.len(),
            "send framed"
        );

        match &mut self.substream {
            #[cfg(test)]
            SubstreamType::Mock(ref mut substream) =>
                futures::SinkExt::send(substream, bytes).await,
            SubstreamType::Tcp(ref mut substream) => match self.codec {
                ProtocolCodec::Unspecified => panic!("codec is unspecified"),
                ProtocolCodec::Identity(payload_size) =>
                    Self::send_identity_payload(substream, payload_size, bytes).await,
                ProtocolCodec::UnsignedVarint(max_size) =>
                    Self::send_unsigned_varint_payload(substream, bytes, max_size).await,
            },
            #[cfg(feature = "websocket")]
            SubstreamType::WebSocket(ref mut substream) => match self.codec {
                ProtocolCodec::Unspecified => panic!("codec is unspecified"),
                ProtocolCodec::Identity(payload_size) =>
                    Self::send_identity_payload(substream, payload_size, bytes).await,
                ProtocolCodec::UnsignedVarint(max_size) =>
                    Self::send_unsigned_varint_payload(substream, bytes, max_size).await,
            },
            #[cfg(feature = "quic")]
            SubstreamType::Quic(ref mut substream) => match self.codec {
                ProtocolCodec::Unspecified => panic!("codec is unspecified"),
                ProtocolCodec::Identity(payload_size) =>
                    Self::send_identity_payload(substream, payload_size, bytes).await,
                ProtocolCodec::UnsignedVarint(max_size) => {
                    check_size!(max_size, bytes.len());

                    let mut buffer = unsigned_varint::encode::usize_buffer();
                    let len = unsigned_varint::encode::usize(bytes.len(), &mut buffer);
                    let len = BytesMut::from(len);

                    substream.write_all_chunks(&mut [len.freeze(), bytes]).await
                }
            },
            #[cfg(feature = "webrtc")]
            SubstreamType::WebRtc(ref mut substream) => match self.codec {
                ProtocolCodec::Unspecified => panic!("codec is unspecified"),
                ProtocolCodec::Identity(payload_size) =>
                    Self::send_identity_payload(substream, payload_size, bytes).await,
                ProtocolCodec::UnsignedVarint(max_size) =>
                    Self::send_unsigned_varint_payload(substream, bytes, max_size).await,
            },
        }
    }
}

impl tokio::io::AsyncRead for Substream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        poll_read!(&mut self.substream, cx, buf)
    }
}

impl tokio::io::AsyncWrite for Substream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        poll_write!(&mut self.substream, cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        poll_flush!(&mut self.substream, cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        poll_shutdown!(&mut self.substream, cx)
    }
}

enum ReadError {
    Overflow,
    NotEnoughBytes,
    DecodeError,
}

// Return the payload size and the number of bytes it took to encode it
fn read_payload_size(buffer: &[u8]) -> Result<(usize, usize), ReadError> {
    let max_len = encode::usize_buffer().len();

    for i in 0..std::cmp::min(buffer.len(), max_len) {
        if decode::is_last(buffer[i]) {
            match decode::usize(&buffer[..=i]) {
                Err(_) => return Err(ReadError::DecodeError),
                Ok(size) => return Ok((size.0, i + 1)),
            }
        }
    }

    match buffer.len() < max_len {
        true => Err(ReadError::NotEnoughBytes),
        false => Err(ReadError::Overflow),
    }
}

impl Stream for Substream {
    type Item = Result<BytesMut, SubstreamError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = Pin::into_inner(self);

        // `MockSubstream` implements `Stream` so calls to `poll_next()` must be delegated
        delegate_poll_next!(&mut this.substream, cx);

        loop {
            match this.codec {
                ProtocolCodec::Identity(payload_size) => {
                    let mut read_buf =
                        ReadBuf::new(&mut this.read_buffer[this.offset..payload_size]);

                    match futures::ready!(poll_read!(&mut this.substream, cx, &mut read_buf)) {
                        Ok(_) => {
                            let nread = read_buf.filled().len();
                            if nread == 0 {
                                tracing::trace!(
                                    target: LOG_TARGET,
                                    peer = ?this.peer,
                                    "read zero bytes, substream closed"
                                );
                                return Poll::Ready(None);
                            }

                            if nread == payload_size {
                                let mut payload = std::mem::replace(
                                    &mut this.read_buffer,
                                    BytesMut::zeroed(payload_size),
                                );
                                payload.truncate(payload_size);
                                this.offset = 0usize;

                                return Poll::Ready(Some(Ok(payload)));
                            } else {
                                this.offset += read_buf.filled().len();
                            }
                        }
                        Err(error) => return Poll::Ready(Some(Err(error.into()))),
                    }
                }
                ProtocolCodec::UnsignedVarint(max_size) => {
                    loop {
                        // return all pending frames first
                        if let Some(frame) = this.pending_frames.pop_front() {
                            return Poll::Ready(Some(Ok(frame)));
                        }

                        match this.current_frame_size.take() {
                            Some(frame_size) => {
                                let mut read_buf =
                                    ReadBuf::new(&mut this.read_buffer[this.offset..]);
                                this.current_frame_size = Some(frame_size);

                                match futures::ready!(poll_read!(
                                    &mut this.substream,
                                    cx,
                                    &mut read_buf
                                )) {
                                    Err(_error) => return Poll::Ready(None),
                                    Ok(_) => {
                                        let nread = match read_buf.filled().len() {
                                            0 => return Poll::Ready(None),
                                            nread => nread,
                                        };

                                        this.offset += nread;

                                        if this.offset == frame_size {
                                            let out_frame = std::mem::replace(
                                                &mut this.read_buffer,
                                                BytesMut::new(),
                                            );
                                            this.offset = 0;
                                            this.current_frame_size = None;

                                            return Poll::Ready(Some(Ok(out_frame)));
                                        } else {
                                            this.current_frame_size = Some(frame_size);
                                            continue;
                                        }
                                    }
                                }
                            }
                            None => {
                                let mut read_buf =
                                    ReadBuf::new(&mut this.size_vec[this.offset..this.offset + 1]);

                                match futures::ready!(poll_read!(
                                    &mut this.substream,
                                    cx,
                                    &mut read_buf
                                )) {
                                    Err(_error) => return Poll::Ready(None),
                                    Ok(_) => {
                                        if read_buf.filled().is_empty() {
                                            return Poll::Ready(None);
                                        }
                                        this.offset += 1;

                                        match read_payload_size(&this.size_vec[..this.offset]) {
                                            Err(ReadError::NotEnoughBytes) => continue,
                                            Err(_) =>
                                                return Poll::Ready(Some(Err(
                                                    SubstreamError::ReadFailure(Some(
                                                        this.substream_id,
                                                    )),
                                                ))),
                                            Ok((size, num_bytes)) => {
                                                debug_assert_eq!(num_bytes, this.offset);

                                                if let Some(max_size) = max_size {
                                                    if size > max_size {
                                                        return Poll::Ready(Some(Err(
                                                            SubstreamError::ReadFailure(Some(
                                                                this.substream_id,
                                                            )),
                                                        )));
                                                    }
                                                }

                                                this.offset = 0;
                                                // Handle empty payloads detected as 0-length frame.
                                                // The offset must be cleared to 0 to not interfere
                                                // with next framing.
                                                if size == 0 {
                                                    return Poll::Ready(Some(Ok(BytesMut::new())));
                                                }

                                                this.current_frame_size = Some(size);
                                                this.read_buffer = BytesMut::zeroed(size);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                ProtocolCodec::Unspecified => panic!("codec is unspecified"),
            }
        }
    }
}

// TODO: https://github.com/paritytech/litep2p/issues/341 this code can definitely be optimized
impl Sink<Bytes> for Substream {
    type Error = SubstreamError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // `MockSubstream` implements `Sink` so calls to `poll_ready()` must be delegated
        delegate_poll_ready!(&mut self.substream, cx);

        if self.pending_out_bytes >= BACKPRESSURE_BOUNDARY {
            return poll_flush!(&mut self.substream, cx).map_err(From::from);
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        // `MockSubstream` implements `Sink` so calls to `start_send()` must be delegated
        delegate_start_send!(&mut self.substream, item);

        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            substream_id = ?self.substream_id,
            data_len = item.len(),
            "Substream::start_send()",
        );

        match self.codec {
            ProtocolCodec::Identity(payload_size) => {
                if item.len() != payload_size {
                    return Err(SubstreamError::IoError(ErrorKind::PermissionDenied));
                }

                self.pending_out_bytes += item.len();
                self.pending_out_frames.push_back(item);
            }
            ProtocolCodec::UnsignedVarint(max_size) => {
                check_size!(max_size, item.len());

                let len = {
                    let mut buffer = unsigned_varint::encode::usize_buffer();
                    let len = unsigned_varint::encode::usize(item.len(), &mut buffer);
                    BytesMut::from(len)
                };

                self.pending_out_bytes += len.len() + item.len();
                self.pending_out_frames.push_back(len.freeze());
                self.pending_out_frames.push_back(item);
            }
            ProtocolCodec::Unspecified => panic!("codec is unspecified"),
        }

        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // `MockSubstream` implements `Sink` so calls to `poll_flush()` must be delegated
        delegate_poll_flush!(&mut self.substream, cx);

        loop {
            let mut pending_frame = match self.pending_out_frame.take() {
                Some(frame) => frame,
                None => match self.pending_out_frames.pop_front() {
                    Some(frame) => frame,
                    None => break,
                },
            };

            match poll_write!(&mut self.substream, cx, &pending_frame) {
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error.into())),
                Poll::Pending => {
                    self.pending_out_frame = Some(pending_frame);
                    break;
                }
                Poll::Ready(Ok(nwritten)) => {
                    pending_frame.advance(nwritten);

                    if !pending_frame.is_empty() {
                        self.pending_out_frame = Some(pending_frame);
                    }
                }
            }
        }

        poll_flush!(&mut self.substream, cx).map_err(From::from)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        poll_shutdown!(&mut self.substream, cx).map_err(From::from)
    }
}

/// Substream set key.
pub trait SubstreamSetKey: Hash + Unpin + fmt::Debug + PartialEq + Eq + Copy {}

impl<K: Hash + Unpin + fmt::Debug + PartialEq + Eq + Copy> SubstreamSetKey for K {}

/// Substream set.
// TODO: https://github.com/paritytech/litep2p/issues/342 remove this.
#[derive(Debug, Default)]
pub struct SubstreamSet<K, S>
where
    K: SubstreamSetKey,
    S: Stream<Item = Result<BytesMut, SubstreamError>> + Unpin,
{
    substreams: HashMap<K, S>,
}

impl<K, S> SubstreamSet<K, S>
where
    K: SubstreamSetKey,
    S: Stream<Item = Result<BytesMut, SubstreamError>> + Unpin,
{
    /// Create new [`SubstreamSet`].
    pub fn new() -> Self {
        Self {
            substreams: HashMap::new(),
        }
    }

    /// Add new substream to the set.
    pub fn insert(&mut self, key: K, substream: S) {
        match self.substreams.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(substream);
            }
            Entry::Occupied(_) => {
                tracing::error!(?key, "substream already exists");
                debug_assert!(false);
            }
        }
    }

    /// Remove substream from the set.
    pub fn remove(&mut self, key: &K) -> Option<S> {
        self.substreams.remove(key)
    }

    /// Get mutable reference to stored substream.
    #[cfg(test)]
    pub fn get_mut(&mut self, key: &K) -> Option<&mut S> {
        self.substreams.get_mut(key)
    }

    /// Get size of [`SubstreamSet`].
    pub fn len(&self) -> usize {
        self.substreams.len()
    }

    /// Check if [`SubstreamSet`] is empty.
    pub fn is_empty(&self) -> bool {
        self.substreams.is_empty()
    }
}

impl<K, S> Stream for SubstreamSet<K, S>
where
    K: SubstreamSetKey,
    S: Stream<Item = Result<BytesMut, SubstreamError>> + Unpin,
{
    type Item = (K, <S as Stream>::Item);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = Pin::into_inner(self);

        for (key, mut substream) in inner.substreams.iter_mut() {
            match Pin::new(&mut substream).poll_next(cx) {
                Poll::Pending => continue,
                Poll::Ready(Some(data)) => return Poll::Ready(Some((*key, data))),
                Poll::Ready(None) =>
                    return Poll::Ready(Some((*key, Err(SubstreamError::ConnectionClosed)))),
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::substream::MockSubstream, PeerId};
    use futures::{SinkExt, StreamExt};

    #[test]
    fn add_substream() {
        let mut set = SubstreamSet::<PeerId, MockSubstream>::new();

        let peer = PeerId::random();
        let substream = MockSubstream::new();
        set.insert(peer, substream);

        let peer = PeerId::random();
        let substream = MockSubstream::new();
        set.insert(peer, substream);
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn add_same_peer_twice() {
        let mut set = SubstreamSet::<PeerId, MockSubstream>::new();

        let peer = PeerId::random();
        let substream1 = MockSubstream::new();
        let substream2 = MockSubstream::new();

        set.insert(peer, substream1);
        set.insert(peer, substream2);
    }

    #[test]
    fn remove_substream() {
        let mut set = SubstreamSet::<PeerId, MockSubstream>::new();

        let peer1 = PeerId::random();
        let substream1 = MockSubstream::new();
        set.insert(peer1, substream1);

        let peer2 = PeerId::random();
        let substream2 = MockSubstream::new();
        set.insert(peer2, substream2);

        assert!(set.remove(&peer1).is_some());
        assert!(set.remove(&peer2).is_some());
        assert!(set.remove(&PeerId::random()).is_none());
    }

    #[tokio::test]
    async fn poll_data_from_substream() {
        let mut set = SubstreamSet::<PeerId, MockSubstream>::new();

        let peer = PeerId::random();
        let mut substream = MockSubstream::new();
        substream
            .expect_poll_next()
            .times(1)
            .return_once(|_| Poll::Ready(Some(Ok(BytesMut::from(&b"hello"[..])))));
        substream
            .expect_poll_next()
            .times(1)
            .return_once(|_| Poll::Ready(Some(Ok(BytesMut::from(&b"world"[..])))));
        substream.expect_poll_next().returning(|_| Poll::Pending);
        set.insert(peer, substream);

        let value = set.next().await.unwrap();
        assert_eq!(value.0, peer);
        assert_eq!(value.1.unwrap(), BytesMut::from(&b"hello"[..]));

        let value = set.next().await.unwrap();
        assert_eq!(value.0, peer);
        assert_eq!(value.1.unwrap(), BytesMut::from(&b"world"[..]));

        assert!(futures::poll!(set.next()).is_pending());
    }

    #[tokio::test]
    async fn substream_closed() {
        let mut set = SubstreamSet::<PeerId, MockSubstream>::new();

        let peer = PeerId::random();
        let mut substream = MockSubstream::new();
        substream
            .expect_poll_next()
            .times(1)
            .return_once(|_| Poll::Ready(Some(Ok(BytesMut::from(&b"hello"[..])))));
        substream.expect_poll_next().times(1).return_once(|_| Poll::Ready(None));
        substream.expect_poll_next().returning(|_| Poll::Pending);
        set.insert(peer, substream);

        let value = set.next().await.unwrap();
        assert_eq!(value.0, peer);
        assert_eq!(value.1.unwrap(), BytesMut::from(&b"hello"[..]));

        match set.next().await {
            Some((exited_peer, Err(SubstreamError::ConnectionClosed))) => {
                assert_eq!(peer, exited_peer);
            }
            _ => panic!("inavlid event received"),
        }
    }

    #[tokio::test]
    async fn get_mut_substream() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut set = SubstreamSet::<PeerId, MockSubstream>::new();

        let peer = PeerId::random();
        let mut substream = MockSubstream::new();
        substream
            .expect_poll_next()
            .times(1)
            .return_once(|_| Poll::Ready(Some(Ok(BytesMut::from(&b"hello"[..])))));
        substream.expect_poll_ready().times(1).return_once(|_| Poll::Ready(Ok(())));
        substream.expect_start_send().times(1).return_once(|_| Ok(()));
        substream.expect_poll_flush().times(1).return_once(|_| Poll::Ready(Ok(())));
        substream
            .expect_poll_next()
            .times(1)
            .return_once(|_| Poll::Ready(Some(Ok(BytesMut::from(&b"world"[..])))));
        substream.expect_poll_next().returning(|_| Poll::Pending);
        set.insert(peer, substream);

        let value = set.next().await.unwrap();
        assert_eq!(value.0, peer);
        assert_eq!(value.1.unwrap(), BytesMut::from(&b"hello"[..]));

        let substream = set.get_mut(&peer).unwrap();
        substream.send(vec![1, 2, 3, 4].into()).await.unwrap();

        let value = set.next().await.unwrap();
        assert_eq!(value.0, peer);
        assert_eq!(value.1.unwrap(), BytesMut::from(&b"world"[..]));

        // try to get non-existent substream
        assert!(set.get_mut(&PeerId::random()).is_none());
    }

    #[tokio::test]
    async fn poll_data_from_two_substreams() {
        let mut set = SubstreamSet::<PeerId, MockSubstream>::new();

        // prepare first substream
        let peer1 = PeerId::random();
        let mut substream1 = MockSubstream::new();
        substream1
            .expect_poll_next()
            .times(1)
            .return_once(|_| Poll::Ready(Some(Ok(BytesMut::from(&b"hello"[..])))));
        substream1
            .expect_poll_next()
            .times(1)
            .return_once(|_| Poll::Ready(Some(Ok(BytesMut::from(&b"world"[..])))));
        substream1.expect_poll_next().returning(|_| Poll::Pending);
        set.insert(peer1, substream1);

        // prepare second substream
        let peer2 = PeerId::random();
        let mut substream2 = MockSubstream::new();
        substream2
            .expect_poll_next()
            .times(1)
            .return_once(|_| Poll::Ready(Some(Ok(BytesMut::from(&b"siip"[..])))));
        substream2
            .expect_poll_next()
            .times(1)
            .return_once(|_| Poll::Ready(Some(Ok(BytesMut::from(&b"huup"[..])))));
        substream2.expect_poll_next().returning(|_| Poll::Pending);
        set.insert(peer2, substream2);

        let expected: Vec<Vec<(PeerId, BytesMut)>> = vec![
            vec![
                (peer1, BytesMut::from(&b"hello"[..])),
                (peer1, BytesMut::from(&b"world"[..])),
                (peer2, BytesMut::from(&b"siip"[..])),
                (peer2, BytesMut::from(&b"huup"[..])),
            ],
            vec![
                (peer1, BytesMut::from(&b"hello"[..])),
                (peer2, BytesMut::from(&b"siip"[..])),
                (peer1, BytesMut::from(&b"world"[..])),
                (peer2, BytesMut::from(&b"huup"[..])),
            ],
            vec![
                (peer2, BytesMut::from(&b"siip"[..])),
                (peer2, BytesMut::from(&b"huup"[..])),
                (peer1, BytesMut::from(&b"hello"[..])),
                (peer1, BytesMut::from(&b"world"[..])),
            ],
            vec![
                (peer1, BytesMut::from(&b"hello"[..])),
                (peer2, BytesMut::from(&b"siip"[..])),
                (peer2, BytesMut::from(&b"huup"[..])),
                (peer1, BytesMut::from(&b"world"[..])),
            ],
        ];

        // poll values
        let mut values = Vec::new();

        for _ in 0..4 {
            let value = set.next().await.unwrap();
            values.push((value.0, value.1.unwrap()));
        }

        let mut correct_found = false;

        for set in expected {
            if values == set {
                correct_found = true;
                break;
            }
        }

        if !correct_found {
            panic!("invalid set generated");
        }

        // rest of the calls return `Poll::Pending`
        for _ in 0..10 {
            assert!(futures::poll!(set.next()).is_pending());
        }
    }
}
