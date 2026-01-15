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

//! This example demonstrates how application using `litep2p` might structure itself
//! to implement, e.g, a syncing protocol using notification and request-response protocols

use litep2p::{
    config::ConfigBuilder,
    protocol::{
        notification::{
            Config as NotificationConfig, ConfigBuilder as NotificationConfigBuilder,
            NotificationHandle,
        },
        request_response::{
            Config as RequestResponseConfig, ConfigBuilder as RequestResponseConfigBuilder,
            RequestResponseHandle,
        },
    },
    transport::quic::config::Config as QuicConfig,
    types::protocol::ProtocolName,
    Litep2p,
};

use futures::StreamExt;

/// Object responsible for syncing the blockchain.
struct SyncingEngine {
    /// Notification handle used to send and receive notifications.
    block_announce_handle: NotificationHandle,

    /// Request-response handle used to send and receive block requests/responses.
    block_sync_handle: RequestResponseHandle,

    /// Request-response handle used to send and receive state requests/responses.
    state_sync_handle: RequestResponseHandle,
}

impl SyncingEngine {
    /// Create new [`SyncingEngine`].
    fn new() -> (
        Self,
        NotificationConfig,
        RequestResponseConfig,
        RequestResponseConfig,
    ) {
        let (block_announce_config, block_announce_handle) = Self::init_block_announce();
        let (block_sync_config, block_sync_handle) = Self::init_block_sync();
        let (state_sync_config, state_sync_handle) = Self::init_state_sync();

        (
            Self {
                block_announce_handle,
                block_sync_handle,
                state_sync_handle,
            },
            block_announce_config,
            block_sync_config,
            state_sync_config,
        )
    }

    /// Initialize notification protocol for block announcements
    fn init_block_announce() -> (NotificationConfig, NotificationHandle) {
        NotificationConfigBuilder::new(ProtocolName::from("/notif/block-announce/1"))
            .with_max_size(1024usize)
            .with_handshake(vec![1, 2, 3, 4])
            .build()
    }

    /// Initialize request-response protocol for block syncing.
    fn init_block_sync() -> (RequestResponseConfig, RequestResponseHandle) {
        RequestResponseConfigBuilder::new(ProtocolName::from("/sync/block/1"))
            .with_max_size(1024 * 1024)
            .build()
    }

    /// Initialize request-response protocol for state syncing.
    fn init_state_sync() -> (RequestResponseConfig, RequestResponseHandle) {
        RequestResponseConfigBuilder::new(ProtocolName::from("/sync/state/1"))
            .with_max_size(1024 * 1024)
            .build()
    }

    /// Start event loop for [`SyncingEngine`].
    async fn run(mut self) {
        loop {
            tokio::select! {
                _ = self.block_announce_handle.next() => {}
                _ = self.block_sync_handle.next() => {}
                _ = self.state_sync_handle.next() => {}
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // create `SyncingEngine` and get configs for the protocols that it will use.
    let (engine, block_announce_config, block_sync_config, state_sync_config) =
        SyncingEngine::new();

    // build `Litep2pConfig`
    let config = ConfigBuilder::new()
        .with_quic(QuicConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()],
            ..Default::default()
        })
        .with_notification_protocol(block_announce_config)
        .with_request_response_protocol(block_sync_config)
        .with_request_response_protocol(state_sync_config)
        .build();

    // create `Litep2p` object and start internal protocol handlers and the QUIC transport
    let mut litep2p = Litep2p::new(config).unwrap();

    // spawn `SyncingEngine` in the background
    tokio::spawn(engine.run());

    // poll `litep2p` to allow connection-related activity to make progress
    loop {
        match litep2p.next_event().await.unwrap() {
            _ => {}
        }
    }
}
