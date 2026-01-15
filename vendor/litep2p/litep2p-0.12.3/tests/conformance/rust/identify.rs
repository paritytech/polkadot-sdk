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

#![allow(clippy::large_enum_variant)]

use futures::{Stream, StreamExt};
use libp2p::{
    identify, identity, ping,
    swarm::{NetworkBehaviour, SwarmBuilder, SwarmEvent},
    PeerId, Swarm,
};
use litep2p::{
    config::ConfigBuilder,
    crypto::ed25519::Keypair,
    protocol::libp2p::{
        identify::{Config as IdentifyConfig, IdentifyEvent},
        ping::{Config as PingConfig, PingEvent},
    },
    transport::tcp::config::Config as TcpConfig,
    Litep2p,
};

// We create a custom network behaviour that combines gossipsub, ping and identify.
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MyBehaviourEvent")]
struct MyBehaviour {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

enum MyBehaviourEvent {
    Identify(identify::Event),
    Ping(ping::Event),
}

impl From<identify::Event> for MyBehaviourEvent {
    fn from(event: identify::Event) -> Self {
        MyBehaviourEvent::Identify(event)
    }
}

impl From<ping::Event> for MyBehaviourEvent {
    fn from(event: ping::Event) -> Self {
        MyBehaviourEvent::Ping(event)
    }
}

// initialize litep2p with ping support
fn initialize_litep2p() -> (
    Litep2p,
    Box<dyn Stream<Item = PingEvent> + Send + Unpin>,
    Box<dyn Stream<Item = IdentifyEvent> + Send + Unpin>,
) {
    let keypair = Keypair::generate();
    let (ping_config, ping_event_stream) = PingConfig::default();
    let (identify_config, identify_event_stream) =
        IdentifyConfig::new("proto v1".to_string(), None);

    let litep2p = Litep2p::new(
        ConfigBuilder::new()
            .with_keypair(keypair)
            .with_tcp(TcpConfig {
                listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
                ..Default::default()
            })
            .with_libp2p_ping(ping_config)
            .with_libp2p_identify(identify_config)
            .build(),
    )
    .unwrap();

    (litep2p, ping_event_stream, identify_event_stream)
}

fn initialize_libp2p() -> Swarm<MyBehaviour> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    tracing::debug!("Local peer id: {local_peer_id:?}");

    let transport = libp2p::tokio_development_transport(local_key.clone()).unwrap();
    let behaviour = MyBehaviour {
        identify: identify::Behaviour::new(
            identify::Config::new("/ipfs/1.0.0".into(), local_key.public())
                .with_agent_version("libp2p agent".to_string()),
        ),
        ping: Default::default(),
    };
    let mut swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id).build();

    swarm.listen_on("/ip6/::1/tcp/0".parse().unwrap()).unwrap();

    swarm
}

#[tokio::test]
async fn identify_works() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let mut libp2p = initialize_libp2p();
    let (mut litep2p, _ping_event_stream, mut identify_event_stream) = initialize_litep2p();
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
                    SwarmEvent::Behaviour(MyBehaviourEvent::Ping(_event)) => {},
                    SwarmEvent::Behaviour(MyBehaviourEvent::Identify(event)) => if let identify::Event::Received { info, .. } = event {
                        libp2p_done = true;

                        assert_eq!(info.protocol_version, "proto v1");
                        assert_eq!(info.agent_version, "litep2p/1.0.0");

                        if libp2p_done && litep2p_done {
                            break
                        }
                    }
                    _ => {}
                }
            },
            event = identify_event_stream.next() => match event {
                Some(IdentifyEvent::PeerIdentified { protocol_version, user_agent, .. }) => {
                    litep2p_done = true;

                    assert_eq!(protocol_version, Some("/ipfs/1.0.0".to_string()));
                    assert_eq!(user_agent, Some("libp2p agent".to_string()));

                    if libp2p_done && litep2p_done {
                        break
                    }
                }
                None => panic!("identify exited"),
            },
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                panic!("failed to receive identify in time");
            }
        }
    }
}
