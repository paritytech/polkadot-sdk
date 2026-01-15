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
    codec::ProtocolCodec,
    protocol::request_response::{
        handle::{InnerRequestResponseEvent, RequestResponseCommand, RequestResponseHandle},
        REQUEST_TIMEOUT,
    },
    types::protocol::ProtocolName,
    DEFAULT_CHANNEL_SIZE,
};

use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::{
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

/// Request-response protocol configuration.
pub struct Config {
    /// Protocol name.
    pub(crate) protocol_name: ProtocolName,

    /// Fallback names for the main protocol name.
    pub(crate) fallback_names: Vec<ProtocolName>,

    /// Timeout for outbound requests.
    pub(crate) timeout: Duration,

    /// Codec used by the protocol.
    pub(crate) codec: ProtocolCodec,

    /// TX channel for sending events to the user protocol.
    pub(super) event_tx: Sender<InnerRequestResponseEvent>,

    /// RX channel for receiving commands from the user protocol.
    pub(crate) command_rx: Receiver<RequestResponseCommand>,

    /// Next ephemeral request ID.
    pub(crate) next_request_id: Arc<AtomicUsize>,

    /// Maximum number of concurrent inbound requests.
    pub(crate) max_concurrent_inbound_request: Option<usize>,
}

impl Config {
    /// Create new [`Config`].
    pub fn new(
        protocol_name: ProtocolName,
        fallback_names: Vec<ProtocolName>,
        max_message_size: usize,
        timeout: Duration,
        max_concurrent_inbound_request: Option<usize>,
    ) -> (Self, RequestResponseHandle) {
        let (event_tx, event_rx) = channel(DEFAULT_CHANNEL_SIZE);
        let (command_tx, command_rx) = channel(DEFAULT_CHANNEL_SIZE);
        let next_request_id = Default::default();
        let handle = RequestResponseHandle::new(event_rx, command_tx, Arc::clone(&next_request_id));

        (
            Self {
                event_tx,
                command_rx,
                protocol_name,
                fallback_names,
                next_request_id,
                timeout,
                max_concurrent_inbound_request,
                codec: ProtocolCodec::UnsignedVarint(Some(max_message_size)),
            },
            handle,
        )
    }

    /// Get protocol name.
    pub(crate) fn protocol_name(&self) -> &ProtocolName {
        &self.protocol_name
    }
}

/// Builder for [`Config`].
pub struct ConfigBuilder {
    /// Protocol name.
    pub(crate) protocol_name: ProtocolName,

    /// Fallback names for the main protocol name.
    pub(crate) fallback_names: Vec<ProtocolName>,

    /// Maximum message size.
    max_message_size: Option<usize>,

    /// Timeout for outbound requests.
    timeout: Option<Duration>,

    /// Maximum number of concurrent inbound requests.
    max_concurrent_inbound_request: Option<usize>,
}

impl ConfigBuilder {
    /// Create new [`ConfigBuilder`].
    pub fn new(protocol_name: ProtocolName) -> Self {
        Self {
            protocol_name,
            fallback_names: Vec::new(),
            max_message_size: None,
            timeout: Some(REQUEST_TIMEOUT),
            max_concurrent_inbound_request: None,
        }
    }

    /// Set maximum message size.
    pub fn with_max_size(mut self, max_message_size: usize) -> Self {
        self.max_message_size = Some(max_message_size);
        self
    }

    /// Set fallback names.
    pub fn with_fallback_names(mut self, fallback_names: Vec<ProtocolName>) -> Self {
        self.fallback_names = fallback_names;
        self
    }

    /// Set timeout for outbound requests.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Specify the maximum number of concurrent inbound requests. By default the number of inbound
    /// requests is not limited.
    ///
    /// If a new request is received while the number of inbound requests is already at a maximum,
    /// the request is dropped.
    pub fn with_max_concurrent_inbound_requests(
        mut self,
        max_concurrent_inbound_requests: usize,
    ) -> Self {
        self.max_concurrent_inbound_request = Some(max_concurrent_inbound_requests);
        self
    }

    /// Build [`Config`].
    pub fn build(mut self) -> (Config, RequestResponseHandle) {
        Config::new(
            self.protocol_name,
            self.fallback_names,
            self.max_message_size.take().expect("maximum message size to be set"),
            self.timeout.take().expect("timeout to exist"),
            self.max_concurrent_inbound_request,
        )
    }
}
