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

use crate::{
    error::{Error, ParseError, SubstreamError},
    multistream_select::{
        webrtc_listener_negotiate, HandshakeResult, ListenerSelectResult, WebRtcDialerState,
    },
    protocol::{Direction, Permit, ProtocolCommand, ProtocolSet},
    substream::Substream,
    transport::{
        webrtc::{
            substream::{Event as SubstreamEvent, Substream as WebRtcSubstream, SubstreamHandle},
            util::WebRtcMessage,
        },
        Endpoint,
    },
    types::{protocol::ProtocolName, SubstreamId},
    PeerId,
};

use futures::{Stream, StreamExt};
use indexmap::IndexMap;
use str0m::{
    channel::{ChannelConfig, ChannelId},
    net::{Protocol as Str0mProtocol, Receive},
    Event, IceConnectionState, Input, Output, Rtc,
};
use tokio::{net::UdpSocket, sync::mpsc::Receiver};

use std::{
    collections::HashMap,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::webrtc::connection";

/// Channel context.
#[derive(Debug)]
struct ChannelContext {
    /// Protocol name.
    protocol: ProtocolName,

    /// Fallback names.
    fallback_names: Vec<ProtocolName>,

    /// Substream ID.
    substream_id: SubstreamId,

    /// Permit which keeps the connection open.
    permit: Permit,
}

/// Set of [`SubstreamHandle`]s.
struct SubstreamHandleSet {
    /// Current index.
    index: usize,

    /// Substream handles.
    handles: IndexMap<ChannelId, SubstreamHandle>,
}

impl SubstreamHandleSet {
    /// Create new [`SubstreamHandleSet`].
    pub fn new() -> Self {
        Self {
            index: 0usize,
            handles: IndexMap::new(),
        }
    }

    /// Get mutable access to `SubstreamHandle`.
    pub fn get_mut(&mut self, key: &ChannelId) -> Option<&mut SubstreamHandle> {
        self.handles.get_mut(key)
    }

    /// Insert new handle to [`SubstreamHandleSet`].
    pub fn insert(&mut self, key: ChannelId, handle: SubstreamHandle) {
        assert!(self.handles.insert(key, handle).is_none());
    }

    /// Remove handle from [`SubstreamHandleSet`].
    pub fn remove(&mut self, key: &ChannelId) -> Option<SubstreamHandle> {
        self.handles.shift_remove(key)
    }
}

impl Stream for SubstreamHandleSet {
    type Item = (ChannelId, Option<SubstreamEvent>);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let len = match self.handles.len() {
            0 => return Poll::Pending,
            len => len,
        };
        let start_index = self.index;

        loop {
            let index = self.index % len;
            self.index += 1;

            let (key, stream) = self.handles.get_index_mut(index).expect("handle to exist");
            match stream.poll_next_unpin(cx) {
                Poll::Pending => {}
                Poll::Ready(event) => return Poll::Ready(Some((*key, event))),
            }

            if self.index == start_index + len {
                break Poll::Pending;
            }
        }
    }
}

/// Channel state.
#[derive(Debug)]
enum ChannelState {
    /// Channel is closing.
    Closing,

    /// Inbound channel is opening.
    InboundOpening,

    /// Outbound channel is opening.
    OutboundOpening {
        /// Channel context.
        context: ChannelContext,

        /// `multistream-select` dialer state.
        dialer_state: WebRtcDialerState,
    },

    /// Channel is open.
    Open {
        /// Substream ID.
        substream_id: SubstreamId,

        /// Channel ID.
        channel_id: ChannelId,

        /// Connection permit.
        permit: Permit,
    },
}

/// WebRTC connection.
pub struct WebRtcConnection {
    /// `str0m` WebRTC object.
    rtc: Rtc,

    /// Protocol set.
    protocol_set: ProtocolSet,

    /// Remote peer ID.
    peer: PeerId,

    /// Endpoint.
    endpoint: Endpoint,

    /// Peer address
    peer_address: SocketAddr,

    /// Local address.
    local_address: SocketAddr,

    /// Transport socket.
    socket: Arc<UdpSocket>,

    /// RX channel for receiving datagrams from the transport.
    dgram_rx: Receiver<Vec<u8>>,

    /// Pending outbound channels.
    pending_outbound: HashMap<ChannelId, ChannelContext>,

    /// Open channels.
    channels: HashMap<ChannelId, ChannelState>,

    /// Substream handles.
    handles: SubstreamHandleSet,
}

impl WebRtcConnection {
    /// Create new [`WebRtcConnection`].
    pub fn new(
        rtc: Rtc,
        peer: PeerId,
        peer_address: SocketAddr,
        local_address: SocketAddr,
        socket: Arc<UdpSocket>,
        protocol_set: ProtocolSet,
        endpoint: Endpoint,
        dgram_rx: Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            rtc,
            protocol_set,
            peer,
            peer_address,
            local_address,
            socket,
            endpoint,
            dgram_rx,
            pending_outbound: HashMap::new(),
            channels: HashMap::new(),
            handles: SubstreamHandleSet::new(),
        }
    }

    /// Handle opened channel.
    ///
    /// If the channel is inbound, nothing is done because we have to wait for data
    /// `multistream-select` handshake to be received from remote peer before anything
    /// else can be done.
    ///
    /// If the channel is outbound, send `multistream-select` handshake to remote peer.
    async fn on_channel_opened(
        &mut self,
        channel_id: ChannelId,
        channel_name: String,
    ) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            ?channel_id,
            ?channel_name,
            "channel opened",
        );

        let Some(mut context) = self.pending_outbound.remove(&channel_id) else {
            tracing::trace!(
                target: LOG_TARGET,
                peer = ?self.peer,
                ?channel_id,
                "inbound channel opened, wait for `multistream-select` message",
            );

            self.channels.insert(channel_id, ChannelState::InboundOpening);
            return Ok(());
        };

        let fallback_names = std::mem::take(&mut context.fallback_names);
        let (dialer_state, message) =
            WebRtcDialerState::propose(context.protocol.clone(), fallback_names)?;
        let message = WebRtcMessage::encode(message);

        self.rtc
            .channel(channel_id)
            .ok_or(Error::ChannelDoesntExist)?
            .write(true, message.as_ref())
            .map_err(Error::WebRtc)?;

        self.channels.insert(
            channel_id,
            ChannelState::OutboundOpening {
                context,
                dialer_state,
            },
        );

        Ok(())
    }

    /// Handle closed channel.
    async fn on_channel_closed(&mut self, channel_id: ChannelId) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            ?channel_id,
            "channel closed",
        );

        self.pending_outbound.remove(&channel_id);
        self.channels.remove(&channel_id);
        self.handles.remove(&channel_id);

        Ok(())
    }

    /// Handle data received to an opening inbound channel.
    ///
    /// The first message received over an inbound channel is the `multistream-select` handshake.
    /// This handshake contains the protocol (and potentially fallbacks for that protocol) that
    /// remote peer wants to use for this channel. Parse the handshake and check if any of the
    /// proposed protocols are supported by the local node. If not, send rejection to remote peer
    /// and close the channel. If the local node supports one of the protocols, send confirmation
    /// for the protocol to remote peer and report an opened substream to the selected protocol.
    async fn on_inbound_opening_channel_data(
        &mut self,
        channel_id: ChannelId,
        data: Vec<u8>,
    ) -> crate::Result<(SubstreamId, SubstreamHandle, Permit)> {
        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            ?channel_id,
            "handle opening inbound substream",
        );

        let payload = WebRtcMessage::decode(&data)?.payload.ok_or(Error::InvalidData)?;
        let (response, negotiated) = match webrtc_listener_negotiate(
            &mut self.protocol_set.protocols().iter(),
            payload.into(),
        )? {
            ListenerSelectResult::Accepted { protocol, message } => (message, Some(protocol)),
            ListenerSelectResult::Rejected { message } => (message, None),
        };

        self.rtc
            .channel(channel_id)
            .ok_or(Error::ChannelDoesntExist)?
            .write(true, WebRtcMessage::encode(response.to_vec()).as_ref())
            .map_err(Error::WebRtc)?;

        let protocol = negotiated.ok_or(Error::SubstreamDoesntExist)?;
        let substream_id = self.protocol_set.next_substream_id();
        let codec = self.protocol_set.protocol_codec(&protocol);
        let permit = self.protocol_set.try_get_permit().ok_or(Error::ConnectionClosed)?;
        let (substream, handle) = WebRtcSubstream::new();
        let substream = Substream::new_webrtc(self.peer, substream_id, substream, codec);

        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            ?channel_id,
            ?substream_id,
            ?protocol,
            "inbound substream opened",
        );

        self.protocol_set
            .report_substream_open(self.peer, protocol.clone(), Direction::Inbound, substream)
            .await
            .map(|_| (substream_id, handle, permit))
            .map_err(Into::into)
    }

    /// Handle data received to an opening outbound channel.
    ///
    /// When an outbound channel is opened, the first message the local node sends it the
    /// `multistream-select` handshake which contains the protocol (and any fallbacks for that
    /// protocol) that the local node wants to use to negotiate for the channel. When a message is
    /// received from a remote peer for a channel in state [`ChannelState::OutboundOpening`], parse
    /// the `multistream-select` handshake response. The response either contains a rejection which
    /// causes the substream to be closed, a partial response, or a full response. If a partial
    /// response is heard, e.g., only the header line is received, the handshake cannot be concluded
    /// and the channel is placed back in the [`ChannelState::OutboundOpening`] state to wait for
    /// the rest of the handshake. If a full response is received (or rest of the partial response),
    /// the protocol confirmation is verified and the substream is reported to the protocol.
    ///
    /// If the substream fails to open for whatever reason, since this is an outbound substream,
    /// the protocol is notified of the failure.
    async fn on_outbound_opening_channel_data(
        &mut self,
        channel_id: ChannelId,
        data: Vec<u8>,
        mut dialer_state: WebRtcDialerState,
        context: ChannelContext,
    ) -> Result<Option<(SubstreamId, SubstreamHandle, Permit)>, SubstreamError> {
        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            ?channel_id,
            data_len = ?data.len(),
            "handle opening outbound substream",
        );

        let rtc_message = WebRtcMessage::decode(&data)
            .map_err(|err| SubstreamError::NegotiationError(err.into()))?;
        let message = rtc_message.payload.ok_or(SubstreamError::NegotiationError(
            ParseError::InvalidData.into(),
        ))?;

        let HandshakeResult::Succeeded(protocol) = dialer_state.register_response(message)? else {
            tracing::trace!(
                target: LOG_TARGET,
                peer = ?self.peer,
                ?channel_id,
                "multistream-select handshake not ready",
            );

            self.channels.insert(
                channel_id,
                ChannelState::OutboundOpening {
                    context,
                    dialer_state,
                },
            );

            return Ok(None);
        };

        let ChannelContext {
            substream_id,
            permit,
            ..
        } = context;
        let codec = self.protocol_set.protocol_codec(&protocol);
        let (substream, handle) = WebRtcSubstream::new();
        let substream = Substream::new_webrtc(self.peer, substream_id, substream, codec);

        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            ?channel_id,
            ?substream_id,
            ?protocol,
            "outbound substream opened",
        );

        self.protocol_set
            .report_substream_open(
                self.peer,
                protocol.clone(),
                Direction::Outbound(substream_id),
                substream,
            )
            .await
            .map(|_| Some((substream_id, handle, permit)))
    }

    /// Handle data received from an open channel.
    async fn on_open_channel_data(
        &mut self,
        channel_id: ChannelId,
        data: Vec<u8>,
    ) -> crate::Result<()> {
        let message = WebRtcMessage::decode(&data)?;

        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            ?channel_id,
            flags = message.flags,
            data_len = message.payload.as_ref().map_or(0usize, |payload| payload.len()),
            "handle inbound message",
        );

        self.handles
            .get_mut(&channel_id)
            .ok_or_else(|| {
                tracing::warn!(
                    target: LOG_TARGET,
                    peer = ?self.peer,
                    ?channel_id,
                    "data received from an unknown channel",
                );
                debug_assert!(false);
                Error::InvalidState
            })?
            .on_message(message)
            .await
    }

    /// Handle data received from a channel.
    async fn on_inbound_data(&mut self, channel_id: ChannelId, data: Vec<u8>) -> crate::Result<()> {
        let Some(state) = self.channels.remove(&channel_id) else {
            tracing::warn!(
                target: LOG_TARGET,
                peer = ?self.peer,
                ?channel_id,
                "data received over a channel that doesn't exist",
            );
            debug_assert!(false);
            return Err(Error::InvalidState);
        };

        match state {
            ChannelState::InboundOpening => {
                match self.on_inbound_opening_channel_data(channel_id, data).await {
                    Ok((substream_id, handle, permit)) => {
                        self.handles.insert(channel_id, handle);
                        self.channels.insert(
                            channel_id,
                            ChannelState::Open {
                                substream_id,
                                channel_id,
                                permit,
                            },
                        );
                    }
                    Err(error) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer = ?self.peer,
                            ?channel_id,
                            ?error,
                            "failed to handle opening inbound substream",
                        );

                        self.channels.insert(channel_id, ChannelState::Closing);
                        self.rtc.direct_api().close_data_channel(channel_id);
                    }
                }
            }
            ChannelState::OutboundOpening {
                context,
                dialer_state,
            } => {
                let protocol = context.protocol.clone();
                let substream_id = context.substream_id;

                match self
                    .on_outbound_opening_channel_data(channel_id, data, dialer_state, context)
                    .await
                {
                    Ok(Some((substream_id, handle, permit))) => {
                        self.handles.insert(channel_id, handle);
                        self.channels.insert(
                            channel_id,
                            ChannelState::Open {
                                substream_id,
                                channel_id,
                                permit,
                            },
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer = ?self.peer,
                            ?channel_id,
                            ?error,
                            "failed to handle opening outbound substream",
                        );

                        let _ = self
                            .protocol_set
                            .report_substream_open_failure(protocol, substream_id, error)
                            .await;

                        self.rtc.direct_api().close_data_channel(channel_id);
                        self.channels.insert(channel_id, ChannelState::Closing);
                    }
                }
            }
            ChannelState::Open {
                substream_id,
                channel_id,
                permit,
            } => match self.on_open_channel_data(channel_id, data).await {
                Ok(()) => {
                    self.channels.insert(
                        channel_id,
                        ChannelState::Open {
                            substream_id,
                            channel_id,
                            permit,
                        },
                    );
                }
                Err(error) => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        peer = ?self.peer,
                        ?channel_id,
                        ?error,
                        "failed to handle data for an open channel",
                    );

                    self.rtc.direct_api().close_data_channel(channel_id);
                    self.channels.insert(channel_id, ChannelState::Closing);
                }
            },
            ChannelState::Closing => {
                tracing::debug!(
                    target: LOG_TARGET,
                    peer = ?self.peer,
                    ?channel_id,
                    "channel closing, discarding received data",
                );
                self.channels.insert(channel_id, ChannelState::Closing);
            }
        }

        Ok(())
    }

    /// Handle outbound data.
    fn on_outbound_data(&mut self, channel_id: ChannelId, data: Vec<u8>) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            ?channel_id,
            data_len = ?data.len(),
            "send data",
        );

        self.rtc
            .channel(channel_id)
            .ok_or(Error::ChannelDoesntExist)?
            .write(true, WebRtcMessage::encode(data).as_ref())
            .map_err(Error::WebRtc)
            .map(|_| ())
    }

    /// Open outbound substream.
    fn on_open_substream(
        &mut self,
        protocol: ProtocolName,
        fallback_names: Vec<ProtocolName>,
        substream_id: SubstreamId,
        permit: Permit,
    ) {
        let channel_id = self.rtc.direct_api().create_data_channel(ChannelConfig {
            label: "".to_string(),
            ordered: false,
            reliability: Default::default(),
            negotiated: None,
            protocol: protocol.to_string(),
        });

        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            ?channel_id,
            ?substream_id,
            ?protocol,
            ?fallback_names,
            "open data channel",
        );

        self.pending_outbound.insert(
            channel_id,
            ChannelContext {
                protocol,
                fallback_names,
                substream_id,
                permit,
            },
        );
    }

    /// Connection to peer has been closed.
    async fn on_connection_closed(&mut self) {
        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            "connection closed",
        );

        let _ = self
            .protocol_set
            .report_connection_closed(self.peer, self.endpoint.connection_id())
            .await;
    }

    /// Start running event loop of [`WebRtcConnection`].
    pub async fn run(mut self) {
        tracing::trace!(
            target: LOG_TARGET,
            peer = ?self.peer,
            "start webrtc connection event loop",
        );

        let _ = self
            .protocol_set
            .report_connection_established(self.peer, self.endpoint.clone())
            .await;

        loop {
            // poll output until we get a timeout
            let timeout = match self.rtc.poll_output().unwrap() {
                Output::Timeout(v) => v,
                Output::Transmit(v) => {
                    tracing::trace!(
                        target: LOG_TARGET,
                        peer = ?self.peer,
                        datagram_len = ?v.contents.len(),
                        "transmit data",
                    );

                    self.socket.try_send_to(&v.contents, v.destination).unwrap();
                    continue;
                }
                Output::Event(v) => match v {
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            peer = ?self.peer,
                            "ice connection state changed to closed",
                        );
                        return self.on_connection_closed().await;
                    }
                    Event::ChannelOpen(channel_id, name) => {
                        if let Err(error) = self.on_channel_opened(channel_id, name).await {
                            tracing::debug!(
                                target: LOG_TARGET,
                                peer = ?self.peer,
                                ?channel_id,
                                ?error,
                                "failed to handle opened channel",
                            );
                        }

                        continue;
                    }
                    Event::ChannelClose(channel_id) => {
                        if let Err(error) = self.on_channel_closed(channel_id).await {
                            tracing::debug!(
                                target: LOG_TARGET,
                                peer = ?self.peer,
                                ?channel_id,
                                ?error,
                                "failed to handle closed channel",
                            );
                        }

                        continue;
                    }
                    Event::ChannelData(info) => {
                        if let Err(error) = self.on_inbound_data(info.id, info.data).await {
                            tracing::debug!(
                                target: LOG_TARGET,
                                peer = ?self.peer,
                                channel_id = ?info.id,
                                ?error,
                                "failed to handle channel data",
                            );
                        }

                        continue;
                    }
                    event => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer = ?self.peer,
                            ?event,
                            "unhandled event",
                        );
                        continue;
                    }
                },
            };

            let duration = timeout - Instant::now();
            if duration.is_zero() {
                self.rtc.handle_input(Input::Timeout(Instant::now())).unwrap();
                continue;
            }

            tokio::select! {
                biased;
                datagram = self.dgram_rx.recv() => match datagram {
                    Some(datagram) => {
                        let input = Input::Receive(
                            Instant::now(),
                            Receive {
                                proto: Str0mProtocol::Udp,
                                source: self.peer_address,
                                destination: self.local_address,
                                contents: datagram.as_slice().try_into().unwrap(),
                            },
                        );

                        self.rtc.handle_input(input).unwrap();
                    }
                    None => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            peer = ?self.peer,
                            "read `None` from `dgram_rx`",
                        );
                        return self.on_connection_closed().await;
                    }
                },
                event = self.handles.next() => match event {
                    None => unreachable!(),
                    Some((channel_id, None | Some(SubstreamEvent::Close))) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            peer = ?self.peer,
                            ?channel_id,
                            "channel closed",
                        );

                        self.rtc.direct_api().close_data_channel(channel_id);
                        self.channels.insert(channel_id, ChannelState::Closing);
                        self.handles.remove(&channel_id);
                    }
                    Some((channel_id, Some(SubstreamEvent::Message(data)))) => {
                        if let Err(error) = self.on_outbound_data(channel_id, data) {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?channel_id,
                                ?error,
                                "failed to send data to remote peer",
                            );
                        }
                    }
                    Some((_, Some(SubstreamEvent::RecvClosed))) => {}
                },
                command = self.protocol_set.next() => match command {
                    None | Some(ProtocolCommand::ForceClose) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            peer = ?self.peer,
                            ?command,
                            "`ProtocolSet` instructed to close connection",
                        );
                        return self.on_connection_closed().await;
                    }
                    Some(ProtocolCommand::OpenSubstream { protocol, fallback_names, substream_id, permit, .. }) => {
                        self.on_open_substream(protocol, fallback_names, substream_id, permit);
                    }
                },
                _ = tokio::time::sleep(duration) => {
                    self.rtc.handle_input(Input::Timeout(Instant::now())).unwrap();
                }
            }
        }
    }
}
