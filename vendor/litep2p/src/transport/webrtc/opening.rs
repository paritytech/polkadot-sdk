// Copyright 2023-2024 litep2p developers
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

//! WebRTC handshaking code for an opening connection.

use crate::{
    config::Role,
    crypto::{ed25519::Keypair, noise::NoiseContext},
    transport::{webrtc::util::WebRtcMessage, Endpoint},
    types::ConnectionId,
    Error, PeerId,
};

use multiaddr::{multihash::Multihash, Multiaddr, Protocol};
use str0m::{
    channel::ChannelId,
    config::Fingerprint,
    net::{DatagramRecv, DatagramSend, Protocol as Str0mProtocol, Receive},
    Event, IceConnectionState, Input, Output, Rtc,
};

use std::{net::SocketAddr, time::Instant};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::webrtc::connection";

/// Create Noise prologue.
fn noise_prologue(local_fingerprint: Vec<u8>, remote_fingerprint: Vec<u8>) -> Vec<u8> {
    const PREFIX: &[u8] = b"libp2p-webrtc-noise:";
    let mut prologue =
        Vec::with_capacity(PREFIX.len() + local_fingerprint.len() + remote_fingerprint.len());
    prologue.extend_from_slice(PREFIX);
    prologue.extend_from_slice(&remote_fingerprint);
    prologue.extend_from_slice(&local_fingerprint);

    prologue
}

/// WebRTC connection event.
#[derive(Debug)]
pub enum WebRtcEvent {
    /// Register timeout for the connection.
    Timeout {
        /// Timeout.
        timeout: Instant,
    },

    /// Transmit data to remote peer.
    Transmit {
        /// Destination.
        destination: SocketAddr,

        /// Datagram to transmit.
        datagram: DatagramSend,
    },

    /// Connection closed.
    ConnectionClosed,

    /// Connection established.
    ConnectionOpened {
        /// Remote peer ID.
        peer: PeerId,

        /// Endpoint.
        endpoint: Endpoint,
    },
}

/// Opening WebRTC connection.
///
/// This object is used to track an opening connection which starts with a Noise handshake.
/// After the handshake is done, this object is destroyed and a new WebRTC connection object
/// is created which implements a normal connection event loop dealing with substreams.
pub struct OpeningWebRtcConnection {
    /// WebRTC object
    rtc: Rtc,

    /// Connection state.
    state: State,

    /// Connection ID.
    connection_id: ConnectionId,

    /// Noise channel ID.
    noise_channel_id: ChannelId,

    /// Local keypair.
    id_keypair: Keypair,

    /// Peer address
    peer_address: SocketAddr,

    /// Local address.
    local_address: SocketAddr,
}

/// Connection state.
#[derive(Debug)]
enum State {
    /// Connection is poisoned.
    Poisoned,

    /// Connection is closed.
    Closed,

    /// Connection has been opened.
    Opened {
        /// Noise context.
        context: NoiseContext,
    },

    /// Local Noise handshake has been sent to peer and the connection
    /// is waiting for an answer.
    HandshakeSent {
        /// Noise context.
        context: NoiseContext,
    },

    /// Response to local Noise handshake has been received and the connection
    /// is being validated by `TransportManager`.
    Validating {
        /// Noise context.
        context: NoiseContext,
    },
}

impl OpeningWebRtcConnection {
    /// Create new [`OpeningWebRtcConnection`].
    pub fn new(
        rtc: Rtc,
        connection_id: ConnectionId,
        noise_channel_id: ChannelId,
        id_keypair: Keypair,
        peer_address: SocketAddr,
        local_address: SocketAddr,
    ) -> OpeningWebRtcConnection {
        tracing::trace!(
            target: LOG_TARGET,
            ?connection_id,
            ?peer_address,
            "new connection opened",
        );

        Self {
            rtc,
            state: State::Closed,
            connection_id,
            noise_channel_id,
            id_keypair,
            peer_address,
            local_address,
        }
    }

    /// Get remote fingerprint to bytes.
    fn remote_fingerprint(&mut self) -> Vec<u8> {
        let fingerprint = self
            .rtc
            .direct_api()
            .remote_dtls_fingerprint()
            .expect("fingerprint to exist")
            .clone();
        Self::fingerprint_to_bytes(&fingerprint)
    }

    /// Get local fingerprint as bytes.
    fn local_fingerprint(&mut self) -> Vec<u8> {
        Self::fingerprint_to_bytes(self.rtc.direct_api().local_dtls_fingerprint())
    }

    /// Convert `Fingerprint` to bytes.
    fn fingerprint_to_bytes(fingerprint: &Fingerprint) -> Vec<u8> {
        const MULTIHASH_SHA256_CODE: u64 = 0x12;
        Multihash::wrap(MULTIHASH_SHA256_CODE, &fingerprint.bytes)
            .expect("fingerprint's len to be 32 bytes")
            .to_bytes()
    }

    /// Once a Noise data channel has been opened, even though the light client was the dialer,
    /// the WebRTC server will act as the dialer as per the specification.
    ///
    /// Create the first Noise handshake message and send it to remote peer.
    fn on_noise_channel_open(&mut self) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, "send initial noise handshake");

        let State::Opened { mut context } = std::mem::replace(&mut self.state, State::Poisoned)
        else {
            return Err(Error::InvalidState);
        };

        // create first noise handshake and send it to remote peer
        let payload = WebRtcMessage::encode(context.first_message(Role::Dialer)?);

        self.rtc
            .channel(self.noise_channel_id)
            .ok_or(Error::ChannelDoesntExist)?
            .write(true, payload.as_slice())
            .map_err(Error::WebRtc)?;

        self.state = State::HandshakeSent { context };
        Ok(())
    }

    /// Handle timeout.
    pub fn on_timeout(&mut self) -> crate::Result<()> {
        if let Err(error) = self.rtc.handle_input(Input::Timeout(Instant::now())) {
            tracing::error!(
                target: LOG_TARGET,
                ?error,
                "failed to handle timeout for `Rtc`"
            );

            self.rtc.disconnect();
            return Err(Error::Disconnected);
        }

        Ok(())
    }

    /// Handle Noise handshake response.
    ///
    /// The message contains remote's peer ID which is used by the `TransportManager` to validate
    /// the connection. Note the Noise handshake requires one more messages to be sent by the dialer
    /// (us) but the inbound connection must first be verified by the `TransportManager` which will
    /// either accept or reject the connection.
    ///
    /// If the peer is accepted, [`OpeningWebRtcConnection::on_accept()`] is called which creates
    /// the final Noise message and sends it to the remote peer, concluding the handshake.
    fn on_noise_channel_data(&mut self, data: Vec<u8>) -> crate::Result<WebRtcEvent> {
        tracing::trace!(target: LOG_TARGET, "handle noise handshake reply");

        let State::HandshakeSent { mut context } =
            std::mem::replace(&mut self.state, State::Poisoned)
        else {
            return Err(Error::InvalidState);
        };

        let message = WebRtcMessage::decode(&data)?.payload.ok_or(Error::InvalidData)?;
        let remote_peer_id = context.get_remote_peer_id(&message)?;

        tracing::trace!(
            target: LOG_TARGET,
            ?remote_peer_id,
            "remote reply parsed successfully",
        );

        self.state = State::Validating { context };

        let remote_fingerprint = self
            .rtc
            .direct_api()
            .remote_dtls_fingerprint()
            .expect("fingerprint to exist")
            .clone()
            .bytes;

        const MULTIHASH_SHA256_CODE: u64 = 0x12;
        let certificate = Multihash::wrap(MULTIHASH_SHA256_CODE, &remote_fingerprint)
            .expect("fingerprint's len to be 32 bytes");

        let address = Multiaddr::empty()
            .with(Protocol::from(self.peer_address.ip()))
            .with(Protocol::Udp(self.peer_address.port()))
            .with(Protocol::WebRTC)
            .with(Protocol::Certhash(certificate))
            .with(Protocol::P2p(remote_peer_id.into()));

        Ok(WebRtcEvent::ConnectionOpened {
            peer: remote_peer_id,
            endpoint: Endpoint::listener(address, self.connection_id),
        })
    }

    /// Accept connection by sending the final Noise handshake message
    /// and return the `Rtc` object for further use.
    pub fn on_accept(mut self) -> crate::Result<Rtc> {
        tracing::trace!(target: LOG_TARGET, "accept webrtc connection");

        let State::Validating { mut context } = std::mem::replace(&mut self.state, State::Poisoned)
        else {
            return Err(Error::InvalidState);
        };

        // create second noise handshake message and send it to remote
        let payload = WebRtcMessage::encode(context.second_message()?);

        let mut channel =
            self.rtc.channel(self.noise_channel_id).ok_or(Error::ChannelDoesntExist)?;

        channel.write(true, payload.as_slice()).map_err(Error::WebRtc)?;
        self.rtc.direct_api().close_data_channel(self.noise_channel_id);

        Ok(self.rtc)
    }

    /// Handle input from peer.
    pub fn on_input(&mut self, buffer: DatagramRecv) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer_address,
            "handle input from peer",
        );

        let message = Input::Receive(
            Instant::now(),
            Receive {
                source: self.peer_address,
                proto: Str0mProtocol::Udp,
                destination: self.local_address,
                contents: buffer,
            },
        );

        match self.rtc.accepts(&message) {
            true => self.rtc.handle_input(message).map_err(|error| {
                tracing::debug!(target: LOG_TARGET, source = ?self.peer_address, ?error, "failed to handle data");
                Error::InputRejected
            }),
            false => {
                tracing::warn!(
                    target: LOG_TARGET,
                    peer = ?self.peer_address,
                    "input rejected",
                );
                Err(Error::InputRejected)
            }
        }
    }

    /// Progress the state of [`OpeningWebRtcConnection`].
    pub fn poll_process(&mut self) -> WebRtcEvent {
        if !self.rtc.is_alive() {
            tracing::debug!(
                target: LOG_TARGET,
                "`Rtc` is not alive, closing `WebRtcConnection`"
            );

            return WebRtcEvent::ConnectionClosed;
        }

        loop {
            let output = match self.rtc.poll_output() {
                Ok(output) => output,
                Err(error) => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        connection_id = ?self.connection_id,
                        ?error,
                        "`WebRtcConnection::poll_process()` failed",
                    );

                    return WebRtcEvent::ConnectionClosed;
                }
            };

            match output {
                Output::Transmit(transmit) => {
                    tracing::trace!(
                        target: LOG_TARGET,
                        "transmit data",
                    );

                    return WebRtcEvent::Transmit {
                        destination: transmit.destination,
                        datagram: transmit.contents,
                    };
                }
                Output::Timeout(timeout) => return WebRtcEvent::Timeout { timeout },
                Output::Event(e) => match e {
                    Event::IceConnectionStateChange(v) =>
                        if v == IceConnectionState::Disconnected {
                            tracing::trace!(target: LOG_TARGET, "ice connection closed");
                            return WebRtcEvent::ConnectionClosed;
                        },
                    Event::ChannelOpen(channel_id, name) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            connection_id = ?self.connection_id,
                            ?channel_id,
                            ?name,
                            "channel opened",
                        );

                        if channel_id != self.noise_channel_id {
                            tracing::warn!(
                                target: LOG_TARGET,
                                connection_id = ?self.connection_id,
                                ?channel_id,
                                "ignoring opened channel",
                            );
                            continue;
                        }

                        // TODO: https://github.com/paritytech/litep2p/issues/350 no expect
                        self.on_noise_channel_open().expect("to succeed");
                    }
                    Event::ChannelData(data) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            "data received over channel",
                        );

                        if data.id != self.noise_channel_id {
                            tracing::warn!(
                                target: LOG_TARGET,
                                channel_id = ?data.id,
                                connection_id = ?self.connection_id,
                                "ignoring data from channel",
                            );
                            continue;
                        }

                        // TODO: https://github.com/paritytech/litep2p/issues/350 no expect
                        return self.on_noise_channel_data(data.data).expect("to succeed");
                    }
                    Event::ChannelClose(channel_id) => {
                        tracing::debug!(target: LOG_TARGET, ?channel_id, "channel closed");
                    }
                    Event::Connected => match std::mem::replace(&mut self.state, State::Poisoned) {
                        State::Closed => {
                            let remote_fingerprint = self.remote_fingerprint();
                            let local_fingerprint = self.local_fingerprint();

                            let context = match NoiseContext::with_prologue(
                                &self.id_keypair,
                                noise_prologue(local_fingerprint, remote_fingerprint),
                            ) {
                                Ok(context) => context,
                                Err(err) => {
                                    tracing::error!(
                                        target: LOG_TARGET,
                                        peer = ?self.peer_address,
                                        "NoiseContext failed with error {err}",
                                    );

                                    return WebRtcEvent::ConnectionClosed;
                                }
                            };

                            tracing::debug!(
                                target: LOG_TARGET,
                                peer = ?self.peer_address,
                                "connection opened",
                            );

                            self.state = State::Opened { context };
                        }
                        state => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                peer = ?self.peer_address,
                                ?state,
                                "invalid state for connection"
                            );
                            return WebRtcEvent::ConnectionClosed;
                        }
                    },
                    event => {
                        tracing::warn!(target: LOG_TARGET, ?event, "unhandled event");
                    }
                },
            }
        }
    }
}
