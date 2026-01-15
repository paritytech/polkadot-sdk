// Copyright 2018 Parity Technologies (UK) Ltd.
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

use futures::{Stream, StreamExt};
use libp2p::{
    identity, ping,
    swarm::{keep_alive, NetworkBehaviour, SwarmBuilder, SwarmEvent},
    PeerId, Swarm,
};
use litep2p::{
    config::ConfigBuilder,
    crypto::ed25519::Keypair,
    protocol::libp2p::ping::{Config as PingConfig, PingEvent},
    transport::tcp::config::Config as TcpConfig,
    Litep2p,
};

#[derive(NetworkBehaviour, Default)]
struct Behaviour {
    keep_alive: keep_alive::Behaviour,
    ping: ping::Behaviour,
}

// initialize litep2p with ping support
fn initialize_litep2p() -> (Litep2p, Box<dyn Stream<Item = PingEvent> + Send + Unpin>) {
    let keypair = Keypair::generate();
    let (ping_config, ping_event_stream) = PingConfig::default();
    let litep2p = Litep2p::new(
        ConfigBuilder::new()
            .with_keypair(keypair)
            .with_tcp(TcpConfig {
                listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
                ..Default::default()
            })
            .with_libp2p_ping(ping_config)
            .build(),
    )
    .unwrap();

    (litep2p, ping_event_stream)
}

fn initialize_libp2p() -> Swarm<Behaviour> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    tracing::debug!("Local peer id: {local_peer_id:?}");

    let transport = libp2p::tokio_development_transport(local_key).unwrap();
    let mut swarm =
        SwarmBuilder::with_tokio_executor(transport, Behaviour::default(), local_peer_id).build();

    swarm.listen_on("/ip6/::1/tcp/0".parse().unwrap()).unwrap();

    swarm
}

#[tokio::test]
async fn libp2p_dials() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let mut libp2p = initialize_libp2p();
    let (mut litep2p, mut ping_event_stream) = initialize_litep2p();
    let address = litep2p.listen_addresses().next().unwrap().clone();

    libp2p.dial(address).unwrap();

    tokio::spawn(async move {
        loop {
            let _ = litep2p.next_event().await;
        }
    });

    let mut libp2p_done = false;
    let mut litep2p_done = false;

    loop {
        tokio::select! {
            event = libp2p.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        tracing::info!("Listening on {address:?}")
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Ping(_)) => {
                        libp2p_done = true;

                        if libp2p_done && litep2p_done {
                            break
                        }
                    }
                    _ => {}
                }
            }
            _event = ping_event_stream.next() => {
                litep2p_done = true;

                if libp2p_done && litep2p_done {
                    break
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                panic!("failed to receive ping in time");
            }
        }
    }
}

#[tokio::test]
async fn litep2p_dials() {}

#[tokio::test]
async fn libp2p_doesnt_support_ping() {}

#[tokio::test]
async fn litep2p_doesnt_support_ping() {}
