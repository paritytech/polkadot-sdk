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

#![cfg(feature = "webrtc")]

use futures::StreamExt;
use litep2p::{
    config::ConfigBuilder as Litep2pConfigBuilder,
    crypto::ed25519::Keypair,
    protocol::{libp2p::ping, notification::ConfigBuilder},
    transport::webrtc::config::Config,
    types::protocol::ProtocolName,
    Litep2p,
};

#[tokio::test]
#[ignore]
async fn webrtc_test() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config, mut ping_event_stream) = ping::Config::default();
    let (notif_config, mut notif_event_stream) = ConfigBuilder::new(ProtocolName::from(
        // Westend block-announces protocol name.
        "/e143f23803ac50e8f6f8e62695d1ce9e4e1d68aa36c1cd2cfd15340213f3423e/block-announces/1",
    ))
    .with_max_size(5 * 1024 * 1024)
    .with_handshake(vec![1, 2, 3, 4])
    .with_auto_accept_inbound(true)
    .build();

    let config = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_webrtc(Config {
            listen_addresses: vec!["/ip4/192.168.1.170/udp/8888/webrtc-direct".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config)
        .with_notification_protocol(notif_config)
        .build();

    let mut litep2p = Litep2p::new(config).unwrap();
    let address = litep2p.listen_addresses().next().unwrap().clone();

    tracing::info!("listen address: {address:?}");

    loop {
        tokio::select! {
            event = litep2p.next_event() => {
                tracing::debug!("litep2p event received: {event:?}");
            }
            event = ping_event_stream.next() => {
                if std::matches!(event, None) {
                    tracing::error!("ping event stream terminated");
                    break
                }
                tracing::error!("ping event received: {event:?}");
            }
            _event = notif_event_stream.next() => {
                // tracing::error!("notification event received: {event:?}");
            }
        }
    }
}
