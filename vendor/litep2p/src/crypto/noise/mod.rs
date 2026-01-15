// Copyright 2019 Parity Technologies (UK) Ltd.
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

//! Noise handshake and transport implementations.

use crate::{
    config::Role,
    crypto::{ed25519::Keypair, PublicKey, RemotePublicKey},
    error::{NegotiationError, ParseError},
    PeerId,
};

use bytes::{Buf, Bytes, BytesMut};
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use prost::Message;
use snow::{Builder, HandshakeState, TransportState};

use std::{
    fmt, io,
    pin::Pin,
    task::{Context, Poll},
};

mod protocol;
mod x25519_spec;

mod handshake_schema {
    include!(concat!(env!("OUT_DIR"), "/noise.rs"));
}

/// Noise parameters.
const NOISE_PARAMETERS: &str = "Noise_XX_25519_ChaChaPoly_SHA256";

/// Prefix of static key signatures for domain separation.
pub(crate) const STATIC_KEY_DOMAIN: &str = "noise-libp2p-static-key:";

/// Maximum Noise message size.
const MAX_NOISE_MSG_LEN: usize = 65536;

/// Space given to the encryption buffer to hold key material.
const NOISE_EXTRA_ENCRYPT_SPACE: usize = 16;

/// Max read ahead factor for the noise socket.
///
/// Specifies how many multiples of `MAX_NOISE_MESSAGE_LEN` are read from the socket
/// using one call to `poll_read()`.
pub(crate) const MAX_READ_AHEAD_FACTOR: usize = 5;

/// Maximum write buffer size.
pub(crate) const MAX_WRITE_BUFFER_SIZE: usize = 2;

/// Max. length for Noise protocol message payloads.
pub const MAX_FRAME_LEN: usize = MAX_NOISE_MSG_LEN - NOISE_EXTRA_ENCRYPT_SPACE;

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::crypto::noise";

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum NoiseState {
    Handshake(HandshakeState),
    Transport(TransportState),
}

pub struct NoiseContext {
    keypair: snow::Keypair,
    noise: NoiseState,
    role: Role,
    pub payload: Vec<u8>,
}

impl fmt::Debug for NoiseContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoiseContext")
            .field("public", &self.noise)
            .field("payload", &self.payload)
            .field("role", &self.role)
            .finish()
    }
}

impl NoiseContext {
    /// Assemble Noise payload and return [`NoiseContext`].
    fn assemble(
        noise: snow::HandshakeState,
        keypair: snow::Keypair,
        id_keys: &Keypair,
        role: Role,
    ) -> Result<Self, NegotiationError> {
        let noise_payload = handshake_schema::NoiseHandshakePayload {
            identity_key: Some(PublicKey::Ed25519(id_keys.public()).to_protobuf_encoding()),
            identity_sig: Some(
                id_keys.sign(&[STATIC_KEY_DOMAIN.as_bytes(), keypair.public.as_ref()].concat()),
            ),
            ..Default::default()
        };

        let mut payload = Vec::with_capacity(noise_payload.encoded_len());
        noise_payload.encode(&mut payload).map_err(ParseError::from)?;

        Ok(Self {
            noise: NoiseState::Handshake(noise),
            keypair,
            payload,
            role,
        })
    }

    pub fn new(keypair: &Keypair, role: Role) -> Result<Self, NegotiationError> {
        tracing::trace!(target: LOG_TARGET, ?role, "create new noise configuration");

        let builder: Builder<'_> = Builder::with_resolver(
            NOISE_PARAMETERS.parse().expect("qed; Valid noise pattern"),
            Box::new(protocol::Resolver),
        );

        let dh_keypair = builder.generate_keypair()?;
        let static_key = &dh_keypair.private;

        let noise = match role {
            Role::Dialer => builder.local_private_key(static_key).build_initiator()?,
            Role::Listener => builder.local_private_key(static_key).build_responder()?,
        };

        Self::assemble(noise, dh_keypair, keypair, role)
    }

    /// Create new [`NoiseContext`] with prologue.
    #[cfg(feature = "webrtc")]
    pub fn with_prologue(id_keys: &Keypair, prologue: Vec<u8>) -> Result<Self, NegotiationError> {
        let noise: Builder<'_> = Builder::with_resolver(
            NOISE_PARAMETERS.parse().expect("qed; Valid noise pattern"),
            Box::new(protocol::Resolver),
        );

        let keypair = noise.generate_keypair()?;

        let noise = noise
            .local_private_key(&keypair.private)
            .prologue(&prologue)
            .build_initiator()?;

        Self::assemble(noise, keypair, id_keys, Role::Dialer)
    }

    /// Get remote peer ID from the received Noise payload.
    #[cfg(feature = "webrtc")]
    pub fn get_remote_peer_id(&mut self, reply: &[u8]) -> Result<PeerId, NegotiationError> {
        if reply.len() < 2 {
            tracing::error!(target: LOG_TARGET, "reply too short to contain length prefix");
            return Err(NegotiationError::ParseError(ParseError::InvalidReplyLength));
        }

        let (len_slice, reply) = reply.split_at(2);
        let len = u16::from_be_bytes(
            len_slice
                .try_into()
                .map_err(|_| NegotiationError::ParseError(ParseError::InvalidPublicKey))?,
        ) as usize;

        let mut buffer = vec![0u8; len];

        let NoiseState::Handshake(ref mut noise) = self.noise else {
            tracing::error!(target: LOG_TARGET, "invalid state to read the second handshake message");
            debug_assert!(false);
            return Err(NegotiationError::StateMismatch);
        };

        let res = noise.read_message(reply, &mut buffer)?;
        buffer.truncate(res);

        let payload = handshake_schema::NoiseHandshakePayload::decode(buffer.as_slice())
            .map_err(|err| NegotiationError::ParseError(err.into()))?;

        let identity = payload.identity_key.ok_or(NegotiationError::PeerIdMissing)?;
        Ok(PeerId::from_public_key_protobuf(&identity))
    }

    /// Get first message.
    ///
    /// Listener only sends one message (the payload)
    pub fn first_message(&mut self, role: Role) -> Result<Vec<u8>, NegotiationError> {
        match role {
            Role::Dialer => {
                tracing::trace!(target: LOG_TARGET, "get noise dialer first message");

                let NoiseState::Handshake(ref mut noise) = self.noise else {
                    tracing::error!(target: LOG_TARGET, "invalid state to read the first handshake message");
                    debug_assert!(false);
                    return Err(NegotiationError::StateMismatch);
                };

                let mut buffer = vec![0u8; 256];
                let nwritten = noise.write_message(&[], &mut buffer)?;
                buffer.truncate(nwritten);

                let size = nwritten as u16;
                let mut size = size.to_be_bytes().to_vec();
                size.append(&mut buffer);

                Ok(size)
            }
            Role::Listener => self.second_message(),
        }
    }

    /// Get second message.
    ///
    /// Only the dialer sends the second message.
    pub fn second_message(&mut self) -> Result<Vec<u8>, NegotiationError> {
        tracing::trace!(target: LOG_TARGET, "get noise paylod message");

        let NoiseState::Handshake(ref mut noise) = self.noise else {
            tracing::error!(target: LOG_TARGET, "invalid state to read the first handshake message");
            debug_assert!(false);
            return Err(NegotiationError::StateMismatch);
        };

        let mut buffer = vec![0u8; 2048];
        let nwritten = noise.write_message(&self.payload, &mut buffer)?;
        buffer.truncate(nwritten);

        let size = nwritten as u16;
        let mut size = size.to_be_bytes().to_vec();
        size.append(&mut buffer);

        Ok(size)
    }

    /// Read handshake message.
    async fn read_handshake_message<T: AsyncRead + AsyncWrite + Unpin>(
        &mut self,
        io: &mut T,
    ) -> Result<Bytes, NegotiationError> {
        let mut size = BytesMut::zeroed(2);
        io.read_exact(&mut size).await?;
        let size = size.get_u16();

        let mut message = BytesMut::zeroed(size as usize);
        io.read_exact(&mut message).await?;

        // TODO: https://github.com/paritytech/litep2p/issues/332 use correct overhead.
        let mut out = BytesMut::new();
        out.resize(message.len() + 200, 0u8);

        let NoiseState::Handshake(ref mut noise) = self.noise else {
            tracing::error!(target: LOG_TARGET, "invalid state to read handshake message");
            debug_assert!(false);
            return Err(NegotiationError::StateMismatch);
        };

        let nread = noise.read_message(&message, &mut out)?;
        out.truncate(nread);

        Ok(out.freeze())
    }

    fn read_message(&mut self, message: &[u8], out: &mut [u8]) -> Result<usize, snow::Error> {
        match self.noise {
            NoiseState::Handshake(ref mut noise) => noise.read_message(message, out),
            NoiseState::Transport(ref mut noise) => noise.read_message(message, out),
        }
    }

    fn write_message(&mut self, message: &[u8], out: &mut [u8]) -> Result<usize, snow::Error> {
        match self.noise {
            NoiseState::Handshake(ref mut noise) => noise.write_message(message, out),
            NoiseState::Transport(ref mut noise) => noise.write_message(message, out),
        }
    }

    fn get_handshake_dh_remote_pubkey(&self) -> Result<&[u8], NegotiationError> {
        let NoiseState::Handshake(ref noise) = self.noise else {
            tracing::error!(target: LOG_TARGET, "invalid state to get remote public key");
            return Err(NegotiationError::StateMismatch);
        };

        let Some(dh_remote_pubkey) = noise.get_remote_static() else {
            tracing::error!(target: LOG_TARGET, "expected remote public key at the end of XX session");
            return Err(NegotiationError::IoError(std::io::ErrorKind::InvalidData));
        };

        Ok(dh_remote_pubkey)
    }

    /// Convert Noise into transport mode.
    fn into_transport(self) -> Result<NoiseContext, NegotiationError> {
        let transport = match self.noise {
            NoiseState::Handshake(noise) => noise.into_transport_mode()?,
            NoiseState::Transport(_) => return Err(NegotiationError::StateMismatch),
        };

        Ok(NoiseContext {
            keypair: self.keypair,
            payload: self.payload,
            role: self.role,
            noise: NoiseState::Transport(transport),
        })
    }
}

enum ReadState {
    ReadData {
        max_read: usize,
    },
    ReadFrameLen,
    ProcessNextFrame {
        pending: Option<Vec<u8>>,
        offset: usize,
        size: usize,
        frame_size: usize,
    },
}

enum WriteState {
    Ready {
        offset: usize,
        size: usize,
        encrypted_size: usize,
    },
    WriteFrame {
        offset: usize,
        size: usize,
        encrypted_size: usize,
    },
}

pub struct NoiseSocket<S: AsyncRead + AsyncWrite + Unpin> {
    io: S,
    noise: NoiseContext,
    current_frame_size: Option<usize>,
    write_state: WriteState,
    encrypt_buffer: Vec<u8>,
    offset: usize,
    nread: usize,
    read_state: ReadState,
    read_buffer: Vec<u8>,
    canonical_max_read: usize,
    decrypt_buffer: Option<Vec<u8>>,
    peer: PeerId,
    ty: HandshakeTransport,
}

impl<S: AsyncRead + AsyncWrite + Unpin> NoiseSocket<S> {
    fn new(
        io: S,
        noise: NoiseContext,
        max_read_ahead_factor: usize,
        max_write_buffer_size: usize,
        peer: PeerId,
        ty: HandshakeTransport,
    ) -> Self {
        Self {
            io,
            noise,
            read_buffer: vec![
                0u8;
                max_read_ahead_factor * MAX_NOISE_MSG_LEN + (2 + MAX_NOISE_MSG_LEN)
            ],
            nread: 0usize,
            offset: 0usize,
            current_frame_size: None,
            write_state: WriteState::Ready {
                offset: 0usize,
                size: 0usize,
                encrypted_size: 0usize,
            },
            encrypt_buffer: vec![0u8; max_write_buffer_size * (MAX_NOISE_MSG_LEN + 2)],
            decrypt_buffer: Some(vec![0u8; MAX_FRAME_LEN]),
            read_state: ReadState::ReadData {
                max_read: max_read_ahead_factor * MAX_NOISE_MSG_LEN,
            },
            canonical_max_read: max_read_ahead_factor * MAX_NOISE_MSG_LEN,
            peer,
            ty,
        }
    }

    fn reset_read_state(&mut self, remaining: usize) {
        match remaining {
            0 => {
                self.nread = 0;
            }
            1 => {
                self.read_buffer[0] = self.read_buffer[self.nread - 1];
                self.nread = 1;
            }
            _ => panic!("invalid state"),
        }

        self.offset = 0;
        self.read_state = ReadState::ReadData {
            max_read: self.canonical_max_read,
        };
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for NoiseSocket<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = Pin::into_inner(self);

        loop {
            match this.read_state {
                ReadState::ReadData { max_read } => {
                    let nread = match Pin::new(&mut this.io)
                        .poll_read(cx, &mut this.read_buffer[this.nread..max_read])
                    {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                        Poll::Ready(Ok(nread)) => match nread == 0 {
                            true => return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into())),
                            false => nread,
                        },
                    };

                    tracing::trace!(
                        target: LOG_TARGET,
                        ?nread,
                        ty = ?this.ty,
                        peer = ?this.peer,
                        "read data from socket"
                    );

                    this.nread += nread;
                    this.read_state = ReadState::ReadFrameLen;
                }
                ReadState::ReadFrameLen => {
                    let mut remaining = match this.nread.checked_sub(this.offset) {
                        Some(remaining) => remaining,
                        None => {
                            tracing::error!(
                                target: LOG_TARGET,
                                ty = ?this.ty,
                                peer = ?this.peer,
                                nread = ?this.nread,
                                offset = ?this.offset,
                                "offset is larger than the number of bytes read"
                            );
                            return Poll::Ready(Err(io::ErrorKind::PermissionDenied.into()));
                        }
                    };

                    if remaining < 2 {
                        tracing::trace!(
                            target: LOG_TARGET,
                            ty = ?this.ty,
                            peer = ?this.peer,
                            "reset read buffer"
                        );
                        this.reset_read_state(remaining);
                        continue;
                    }

                    // get frame size, either from current or previous iteration
                    let frame_size = match this.current_frame_size.take() {
                        Some(frame_size) => frame_size,
                        None => {
                            let frame_size = (this.read_buffer[this.offset] as u16) << 8
                                | this.read_buffer[this.offset + 1] as u16;
                            this.offset += 2;
                            remaining -= 2;
                            frame_size as usize
                        }
                    };

                    tracing::trace!(
                        target: LOG_TARGET,
                        ty = ?this.ty,
                        peer = ?this.peer,
                        "current frame size = {frame_size}"
                    );

                    if remaining < frame_size {
                        // `read_buffer` can fit the full frame size.
                        if this.nread + frame_size < this.canonical_max_read {
                            tracing::trace!(
                                target: LOG_TARGET,
                                ty = ?this.ty,
                                peer = ?this.peer,
                                max_size = ?this.canonical_max_read,
                                next_frame_size = ?(this.nread + frame_size),
                                "read buffer can fit the full frame",
                            );

                            this.current_frame_size = Some(frame_size);
                            this.read_state = ReadState::ReadData {
                                max_read: this.canonical_max_read,
                            };
                            continue;
                        }

                        tracing::trace!(
                            target: LOG_TARGET,
                            ty = ?this.ty,
                            peer = ?this.peer,
                            "use auxiliary buffer extension"
                        );

                        // use the auxiliary memory at the end of the read buffer for reading the
                        // frame
                        this.current_frame_size = Some(frame_size);
                        this.read_state = ReadState::ReadData {
                            max_read: this.nread + frame_size - remaining,
                        };
                        continue;
                    }

                    if frame_size <= NOISE_EXTRA_ENCRYPT_SPACE {
                        tracing::error!(
                            target: LOG_TARGET,
                            ty = ?this.ty,
                            peer = ?this.peer,
                            ?frame_size,
                            max_size = ?NOISE_EXTRA_ENCRYPT_SPACE,
                            "invalid frame size",
                        );
                        return Poll::Ready(Err(io::ErrorKind::InvalidData.into()));
                    }

                    this.current_frame_size = Some(frame_size);
                    this.read_state = ReadState::ProcessNextFrame {
                        pending: None,
                        offset: 0usize,
                        size: 0usize,
                        frame_size: 0usize,
                    };
                }
                ReadState::ProcessNextFrame {
                    ref mut pending,
                    offset,
                    size,
                    frame_size,
                } => match pending.take() {
                    Some(pending) => match buf.len() >= pending[offset..size].len() {
                        true => {
                            let copy_size = pending[offset..size].len();
                            buf[..copy_size].copy_from_slice(&pending[offset..copy_size + offset]);

                            this.read_state = ReadState::ReadFrameLen;
                            this.decrypt_buffer = Some(pending);
                            this.offset += frame_size;
                            return Poll::Ready(Ok(copy_size));
                        }
                        false => {
                            buf.copy_from_slice(&pending[offset..buf.len() + offset]);

                            this.read_state = ReadState::ProcessNextFrame {
                                pending: Some(pending),
                                offset: offset + buf.len(),
                                size,
                                frame_size,
                            };
                            return Poll::Ready(Ok(buf.len()));
                        }
                    },
                    None => {
                        let frame_size =
                            this.current_frame_size.take().expect("`frame_size` to exist");

                        match buf.len() >= frame_size - NOISE_EXTRA_ENCRYPT_SPACE {
                            true => match this.noise.read_message(
                                &this.read_buffer[this.offset..this.offset + frame_size],
                                buf,
                            ) {
                                Err(error) => {
                                    tracing::error!(
                                        target: LOG_TARGET,
                                        ty = ?this.ty,
                                        peer = ?this.peer,
                                        buf_len = ?buf.len(),
                                        frame_size = ?frame_size,
                                        ?error,
                                        "failed to decrypt message"
                                    );

                                    return Poll::Ready(Err(io::ErrorKind::InvalidData.into()));
                                }
                                Ok(nread) => {
                                    this.offset += frame_size;
                                    this.read_state = ReadState::ReadFrameLen;
                                    return Poll::Ready(Ok(nread));
                                }
                            },
                            false => {
                                let mut buffer =
                                    this.decrypt_buffer.take().expect("buffer to exist");

                                match this.noise.read_message(
                                    &this.read_buffer[this.offset..this.offset + frame_size],
                                    &mut buffer,
                                ) {
                                    Err(error) => {
                                        tracing::error!(
                                            target: LOG_TARGET,
                                            ty = ?this.ty,
                                            peer = ?this.peer,
                                            buf_len = ?buf.len(),
                                            frame_size = ?frame_size,
                                            ?error,
                                            "failed to decrypt message for smaller buffer"
                                        );

                                        return Poll::Ready(Err(io::ErrorKind::InvalidData.into()));
                                    }
                                    Ok(nread) => {
                                        buf.copy_from_slice(&buffer[..buf.len()]);
                                        this.read_state = ReadState::ProcessNextFrame {
                                            pending: Some(buffer),
                                            offset: buf.len(),
                                            size: nread,
                                            frame_size,
                                        };
                                        return Poll::Ready(Ok(buf.len()));
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for NoiseSocket<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = Pin::into_inner(self);
        let mut chunks = buf.chunks(MAX_FRAME_LEN).peekable();

        loop {
            match this.write_state {
                WriteState::Ready {
                    offset,
                    size,
                    encrypted_size,
                } => {
                    let Some(chunk) = chunks.next() else {
                        break;
                    };

                    match this.noise.write_message(chunk, &mut this.encrypt_buffer[offset + 2..]) {
                        Err(error) => {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?error,
                                ty = ?this.ty,
                                peer = ?this.peer,
                                "failed to encrypt message"
                            );

                            return Poll::Ready(Err(io::ErrorKind::InvalidData.into()));
                        }
                        Ok(nwritten) => {
                            this.encrypt_buffer[offset] = (nwritten >> 8) as u8;
                            this.encrypt_buffer[offset + 1] = (nwritten & 0xff) as u8;

                            if let Some(next_chunk) = chunks.peek() {
                                if next_chunk.len() + NOISE_EXTRA_ENCRYPT_SPACE + 2
                                    <= this.encrypt_buffer[offset + nwritten + 2..].len()
                                {
                                    this.write_state = WriteState::Ready {
                                        offset: offset + nwritten + 2,
                                        size: size + chunk.len(),
                                        encrypted_size: encrypted_size + nwritten + 2,
                                    };
                                    continue;
                                }
                            }

                            this.write_state = WriteState::WriteFrame {
                                offset: 0usize,
                                size: size + chunk.len(),
                                encrypted_size: encrypted_size + nwritten + 2,
                            };
                        }
                    }
                }
                WriteState::WriteFrame {
                    ref mut offset,
                    size,
                    encrypted_size,
                } => loop {
                    match futures::ready!(Pin::new(&mut this.io)
                        .poll_write(cx, &this.encrypt_buffer[*offset..encrypted_size]))
                    {
                        Ok(nwritten) => {
                            *offset += nwritten;

                            if offset == &encrypted_size {
                                this.write_state = WriteState::Ready {
                                    offset: 0usize,
                                    size: 0usize,
                                    encrypted_size: 0usize,
                                };
                                return Poll::Ready(Ok(size));
                            }
                        }
                        Err(error) => return Poll::Ready(Err(error)),
                    }
                },
            }
        }

        Poll::Ready(Ok(0))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_close(cx)
    }
}

/// Parse the `PeerId` from received `NoiseHandshakePayload` and verify the payload signature.
fn parse_and_verify_peer_id(
    payload: handshake_schema::NoiseHandshakePayload,
    dh_remote_pubkey: &[u8],
) -> Result<PeerId, NegotiationError> {
    let identity = payload.identity_key.ok_or(NegotiationError::PeerIdMissing)?;
    let remote_public_key = RemotePublicKey::from_protobuf_encoding(&identity)?;
    let remote_key_signature =
        payload.identity_sig.ok_or(NegotiationError::BadSignature).inspect_err(|_err| {
            tracing::debug!(target: LOG_TARGET, "payload without signature");
        })?;

    let peer_id = PeerId::from_public_key_protobuf(&identity);

    if !remote_public_key.verify(
        &[STATIC_KEY_DOMAIN.as_bytes(), dh_remote_pubkey].concat(),
        &remote_key_signature,
    ) {
        tracing::debug!(
            target: LOG_TARGET,
            ?peer_id,
            "failed to verify remote public key signature"
        );

        return Err(NegotiationError::BadSignature);
    }

    Ok(peer_id)
}

/// The type of the transport used for the crypto/noise protocol.
///
/// This is used for logging purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeTransport {
    Tcp,
    #[cfg(feature = "websocket")]
    WebSocket,
}

/// Perform Noise handshake.
pub async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(
    mut io: S,
    keypair: &Keypair,
    role: Role,
    max_read_ahead_factor: usize,
    max_write_buffer_size: usize,
    timeout: std::time::Duration,
    ty: HandshakeTransport,
) -> Result<(NoiseSocket<S>, PeerId), NegotiationError> {
    let handle_handshake = async move {
        tracing::debug!(target: LOG_TARGET, ?role, ?ty, "start noise handshake");

        let mut noise = NoiseContext::new(keypair, role)?;
        let payload = match role {
            Role::Dialer => {
                // write initial message
                let first_message = noise.first_message(Role::Dialer)?;
                let _ = io.write(&first_message).await?;
                io.flush().await?;

                // read back response which contains the remote peer id
                let message = noise.read_handshake_message(&mut io).await?;
                // Decode the remote identity message.
                let payload = handshake_schema::NoiseHandshakePayload::decode(message)
                .map_err(ParseError::from)
                .map_err(|err| {
                    tracing::error!(target: LOG_TARGET, ?err, ?ty, "failed to decode remote identity message");
                    err
                })?;

                // send the final message which contains local peer id
                let second_message = noise.second_message()?;
                let _ = io.write(&second_message).await?;
                io.flush().await?;

                payload
            }
            Role::Listener => {
                // read remote's first message
                let _ = noise.read_handshake_message(&mut io).await?;

                // send local peer id.
                let second_message = noise.second_message()?;
                let _ = io.write(&second_message).await?;
                io.flush().await?;

                // read remote's second message which contains their peer id
                let message = noise.read_handshake_message(&mut io).await?;
                // Decode the remote identity message.
                handshake_schema::NoiseHandshakePayload::decode(message)
                    .map_err(ParseError::from)?
            }
        };

        let dh_remote_pubkey = noise.get_handshake_dh_remote_pubkey()?;
        let peer = parse_and_verify_peer_id(payload, dh_remote_pubkey)?;

        Ok((
            NoiseSocket::new(
                io,
                noise.into_transport()?,
                max_read_ahead_factor,
                max_write_buffer_size,
                peer,
                ty,
            ),
            peer,
        ))
    };

    match tokio::time::timeout(timeout, handle_handshake).await {
        Err(_) => Err(NegotiationError::Timeout),
        Ok(result) => result,
    }
}

// TODO: https://github.com/paritytech/litep2p/issues/125 add more tests
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::net::{TcpListener, TcpStream};
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    #[tokio::test]
    async fn noise_handshake() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();

        let peer1_id = PeerId::from_public_key(&keypair1.public().into());
        let peer2_id = PeerId::from_public_key(&keypair2.public().into());

        let listener = TcpListener::bind("[::1]:0".parse::<SocketAddr>().unwrap()).await.unwrap();

        let (stream1, stream2) = tokio::join!(
            TcpStream::connect(listener.local_addr().unwrap()),
            listener.accept()
        );
        let (io1, io2) = {
            let io1 = TokioAsyncReadCompatExt::compat(stream1.unwrap()).into_inner();
            let io1 = Box::new(TokioAsyncWriteCompatExt::compat_write(io1));
            let io2 = TokioAsyncReadCompatExt::compat(stream2.unwrap().0).into_inner();
            let io2 = Box::new(TokioAsyncWriteCompatExt::compat_write(io2));

            (io1, io2)
        };

        let (res1, res2) = tokio::join!(
            handshake(
                io1,
                &keypair1,
                Role::Dialer,
                MAX_READ_AHEAD_FACTOR,
                MAX_WRITE_BUFFER_SIZE,
                std::time::Duration::from_secs(10),
                HandshakeTransport::Tcp,
            ),
            handshake(
                io2,
                &keypair2,
                Role::Listener,
                MAX_READ_AHEAD_FACTOR,
                MAX_WRITE_BUFFER_SIZE,
                std::time::Duration::from_secs(10),
                HandshakeTransport::Tcp,
            )
        );
        let (mut res1, mut res2) = (res1.unwrap(), res2.unwrap());

        assert_eq!(res1.1, peer2_id);
        assert_eq!(res2.1, peer1_id);

        // verify the connection works by reading a string
        let mut buf = vec![0u8; 512];
        let sent = res1.0.write(b"hello, world").await.unwrap();
        res2.0.read_exact(&mut buf[..sent]).await.unwrap();

        assert_eq!(std::str::from_utf8(&buf[..sent]), Ok("hello, world"));
    }

    #[test]
    fn invalid_peer_id_schema() {
        let payload = handshake_schema::NoiseHandshakePayload {
            identity_key: Some(vec![1, 2, 3, 4]),
            identity_sig: None,
            extensions: None,
        };
        match parse_and_verify_peer_id(payload, &[0]).unwrap_err() {
            NegotiationError::ParseError(_) => {}
            _ => panic!("invalid error"),
        }
    }
}
