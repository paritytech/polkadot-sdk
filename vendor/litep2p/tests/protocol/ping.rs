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

use futures::StreamExt;
use litep2p::{
    config::ConfigBuilder, protocol::libp2p::ping::ConfigBuilder as PingConfigBuilder, Litep2p,
};

use crate::common::{add_transport, Transport};

#[tokio::test]
async fn ping_supported_tcp() {
    ping_supported(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn ping_supported_websocket() {
    ping_supported(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn ping_supported_quic() {
    ping_supported(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

async fn ping_supported(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, mut ping_event_stream1) =
        PingConfigBuilder::new().with_max_failure(3usize).build();
    let config1 = ConfigBuilder::new().with_libp2p_ping(ping_config1);
    let config1 = add_transport(config1, transport1).build();

    let (ping_config2, mut ping_event_stream2) = PingConfigBuilder::new().build();
    let config2 = ConfigBuilder::new().with_libp2p_ping(ping_config2);
    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let address = litep2p2.listen_addresses().next().unwrap().clone();

    litep2p1.dial_address(address).await.unwrap();

    let mut litep2p1_done = false;
    let mut litep2p2_done = false;

    loop {
        tokio::select! {
            _event = litep2p1.next_event() => {}
            _event = litep2p2.next_event() => {}
            event = ping_event_stream1.next() => {
                tracing::trace!("ping event for litep2p1: {event:?}");

                litep2p1_done = true;
                if litep2p1_done && litep2p2_done {
                    break
                }
            }
            event = ping_event_stream2.next() => {
                tracing::trace!("ping event for litep2p2: {event:?}");

                litep2p2_done = true;
                if litep2p1_done && litep2p2_done {
                    break
                }
            }
        }
    }
}
