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

use futures::StreamExt;
use libp2p::{
    identify, identity,
    kad::{
        self, store::RecordStore, AddProviderOk, GetProvidersOk, InboundRequest,
        KademliaEvent as Libp2pKademliaEvent, QueryResult,
    },
    swarm::{keep_alive, AddressScore, NetworkBehaviour, SwarmBuilder, SwarmEvent},
    PeerId, Swarm,
};
use litep2p::{
    config::ConfigBuilder as Litep2pConfigBuilder,
    crypto::ed25519::Keypair,
    protocol::libp2p::kademlia::{
        ConfigBuilder, KademliaEvent, KademliaHandle, Quorum, Record, RecordKey,
    },
    transport::tcp::config::Config as TcpConfig,
    types::multiaddr::{Multiaddr, Protocol},
    Litep2p,
};
use std::time::Duration;

#[derive(NetworkBehaviour)]
struct Behaviour {
    keep_alive: keep_alive::Behaviour,
    kad: kad::Kademlia<kad::store::MemoryStore>,
    identify: identify::Behaviour,
}

// initialize litep2p with ping support
fn initialize_litep2p() -> (Litep2p, KademliaHandle) {
    let keypair = Keypair::generate();
    let (kad_config, kad_handle) = ConfigBuilder::new().build();

    let litep2p = Litep2p::new(
        Litep2pConfigBuilder::new()
            .with_keypair(keypair)
            .with_tcp(TcpConfig {
                listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
                ..Default::default()
            })
            .with_libp2p_kademlia(kad_config)
            .build(),
    )
    .unwrap();

    (litep2p, kad_handle)
}

fn initialize_libp2p() -> Swarm<Behaviour> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    tracing::debug!("Local peer id: {local_peer_id:?}");

    let transport = libp2p::tokio_development_transport(local_key.clone()).unwrap();
    let behaviour = {
        let config = kad::KademliaConfig::default();
        let store = kad::store::MemoryStore::new(local_peer_id);

        Behaviour {
            kad: kad::Kademlia::with_config(local_peer_id, store, config),
            keep_alive: Default::default(),
            identify: identify::Behaviour::new(identify::Config::new(
                "/ipfs/1.0.0".into(),
                local_key.public(),
            )),
        }
    };
    let mut swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id).build();

    swarm.listen_on("/ip6/::1/tcp/0".parse().unwrap()).unwrap();

    swarm
}

#[tokio::test]
async fn find_node() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let mut addresses = vec![];
    let mut peer_ids = vec![];
    for _ in 0..3 {
        let mut libp2p = initialize_libp2p();

        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = libp2p.select_next_some().await {
                addresses.push(address);
                peer_ids.push(*libp2p.local_peer_id());
                break;
            }
        }

        tokio::spawn(async move {
            loop {
                let _ = libp2p.select_next_some().await;
            }
        });
    }

    let mut libp2p = initialize_libp2p();
    let (mut litep2p, mut kad_handle) = initialize_litep2p();
    let address = litep2p.listen_addresses().next().unwrap().clone();

    for i in 0..addresses.len() {
        libp2p.dial(addresses[i].clone()).unwrap();
        let _ = libp2p.behaviour_mut().kad.add_address(&peer_ids[i], addresses[i].clone());
    }
    libp2p.dial(address).unwrap();

    tokio::spawn(async move {
        loop {
            let _ = litep2p.next_event().await;
        }
    });

    #[allow(unused)]
    let mut listen_addr = None;
    let peer_id = *libp2p.local_peer_id();

    tracing::error!("local peer id: {peer_id}");

    loop {
        if let SwarmEvent::NewListenAddr { address, .. } = libp2p.select_next_some().await {
            listen_addr = Some(address);
            break;
        }
    }

    tokio::spawn(async move {
        loop {
            let _ = libp2p.select_next_some().await;
        }
    });

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let listen_addr = listen_addr.unwrap().with(Protocol::P2p(peer_id.into()));

    kad_handle
        .add_known_peer(
            litep2p::PeerId::from_bytes(&peer_id.to_bytes()).unwrap(),
            vec![listen_addr],
        )
        .await;

    let target = litep2p::PeerId::random();
    let _ = kad_handle.find_node(target).await;

    loop {
        if let Some(KademliaEvent::FindNodeSuccess {
            target: query_target,
            peers,
            ..
        }) = kad_handle.next().await
        {
            assert_eq!(target, query_target);
            assert!(!peers.is_empty());
            break;
        }
    }
}

#[tokio::test]
async fn put_record() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let mut addresses = vec![];
    let mut peer_ids = vec![];
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0usize));

    for _ in 0..3 {
        let mut libp2p = initialize_libp2p();

        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = libp2p.select_next_some().await {
                addresses.push(address);
                peer_ids.push(*libp2p.local_peer_id());
                break;
            }
        }

        let counter_copy = std::sync::Arc::clone(&counter);
        tokio::spawn(async move {
            let mut record_found = false;

            loop {
                tokio::select! {
                    _ = libp2p.select_next_some() => {}
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                        let store = libp2p.behaviour_mut().kad.store_mut();
                        if store.get(&libp2p::kad::record::Key::new(&vec![1, 2, 3, 4])).is_some() && !record_found {
                            counter_copy.fetch_add(1usize, std::sync::atomic::Ordering::SeqCst);
                            record_found = true;
                        }
                    }
                }
            }
        });
    }

    let mut libp2p = initialize_libp2p();
    let (mut litep2p, mut kad_handle) = initialize_litep2p();
    let address = litep2p.listen_addresses().next().unwrap().clone();

    for i in 0..addresses.len() {
        libp2p.dial(addresses[i].clone()).unwrap();
        let _ = libp2p.behaviour_mut().kad.add_address(&peer_ids[i], addresses[i].clone());
    }
    libp2p.dial(address).unwrap();

    tokio::spawn(async move {
        loop {
            let _ = litep2p.next_event().await;
        }
    });

    #[allow(unused)]
    let mut listen_addr = None;
    let peer_id = *libp2p.local_peer_id();

    tracing::error!("local peer id: {peer_id}");

    loop {
        if let SwarmEvent::NewListenAddr { address, .. } = libp2p.select_next_some().await {
            listen_addr = Some(address);
            break;
        }
    }

    let counter_copy = std::sync::Arc::clone(&counter);
    tokio::spawn(async move {
        let mut record_found = false;

        loop {
            tokio::select! {
                _ = libp2p.select_next_some() => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    let store = libp2p.behaviour_mut().kad.store_mut();
                    if store.get(&libp2p::kad::record::Key::new(&vec![1, 2, 3, 4])).is_some() && !record_found {
                        counter_copy.fetch_add(1usize, std::sync::atomic::Ordering::SeqCst);
                        record_found = true;
                    }
                }
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let listen_addr = listen_addr.unwrap().with(Protocol::P2p(peer_id.into()));

    kad_handle
        .add_known_peer(
            litep2p::PeerId::from_bytes(&peer_id.to_bytes()).unwrap(),
            vec![listen_addr],
        )
        .await;

    let record_key = RecordKey::new(&vec![1, 2, 3, 4]);
    let record = Record::new(record_key, vec![1, 3, 3, 7, 1, 3, 3, 8]);

    let _ = kad_handle.put_record(record, Quorum::All).await;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        if counter.load(std::sync::atomic::Ordering::SeqCst) == 4 {
            break;
        }
    }
}

#[tokio::test]
async fn get_record() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let mut addresses = vec![];
    let mut peer_ids = vec![];
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0usize));

    for _ in 0..3 {
        let mut libp2p = initialize_libp2p();

        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = libp2p.select_next_some().await {
                addresses.push(address);
                peer_ids.push(*libp2p.local_peer_id());
                break;
            }
        }

        let counter_copy = std::sync::Arc::clone(&counter);
        tokio::spawn(async move {
            let mut record_found = false;

            loop {
                tokio::select! {
                    _ = libp2p.select_next_some() => {}
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                        let store = libp2p.behaviour_mut().kad.store_mut();
                        if store.get(&libp2p::kad::record::Key::new(&vec![1, 2, 3, 4])).is_some() && !record_found {
                            counter_copy.fetch_add(1usize, std::sync::atomic::Ordering::SeqCst);
                            record_found = true;
                        }
                    }
                }
            }
        });
    }

    let mut libp2p = initialize_libp2p();
    let (mut litep2p, mut kad_handle) = initialize_litep2p();
    let address = litep2p.listen_addresses().next().unwrap().clone();

    for i in 0..addresses.len() {
        libp2p.dial(addresses[i].clone()).unwrap();
        let _ = libp2p.behaviour_mut().kad.add_address(&peer_ids[i], addresses[i].clone());
    }

    // publish record on the network
    let record = libp2p::kad::Record {
        key: libp2p::kad::RecordKey::new(&vec![1, 2, 3, 4]),
        value: vec![13, 37, 13, 38],
        publisher: None,
        expires: None,
    };
    libp2p.behaviour_mut().kad.put_record(record, libp2p::kad::Quorum::All).unwrap();

    #[allow(unused)]
    let mut listen_addr = None;

    loop {
        tokio::select! {
            event = libp2p.select_next_some() => if let SwarmEvent::NewListenAddr { address, .. } = event {
                listen_addr = Some(address);
            },
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                if counter.load(std::sync::atomic::Ordering::SeqCst) == 3 {
                    break;
                }
            }
        }
    }

    libp2p.dial(address).unwrap();

    tokio::spawn(async move {
        loop {
            let _ = litep2p.next_event().await;
        }
    });

    let peer_id = *libp2p.local_peer_id();

    tokio::spawn(async move {
        loop {
            let _ = libp2p.select_next_some().await;
        }
    });

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let listen_addr = listen_addr.unwrap().with(Protocol::P2p(peer_id.into()));

    kad_handle
        .add_known_peer(
            litep2p::PeerId::from_bytes(&peer_id.to_bytes()).unwrap(),
            vec![listen_addr],
        )
        .await;

    let _ = kad_handle.get_record(RecordKey::new(&vec![1, 2, 3, 4]), Quorum::All).await;

    loop {
        match kad_handle.next().await.unwrap() {
            KademliaEvent::GetRecordPartialResult { record, .. } => {
                assert_eq!(record.record.key.as_ref(), vec![1, 2, 3, 4]);
                assert_eq!(record.record.value, vec![13, 37, 13, 38]);
                break;
            }
            KademliaEvent::GetRecordSuccess { .. } => break,
            KademliaEvent::RoutingTableUpdate { .. } => {}
            event => panic!("invalid event received {event:?}"),
        }
    }
}

#[tokio::test]
async fn litep2p_add_provider_to_libp2p() {
    let (mut litep2p, mut litep2p_kad) = initialize_litep2p();
    let mut libp2p = initialize_libp2p();

    // Drive libp2p a little bit to get the listen address.
    let get_libp2p_listen_addr = async {
        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = libp2p.select_next_some().await {
                break address;
            }
        }
    };
    let libp2p_listen_addr = tokio::time::timeout(Duration::from_secs(10), get_libp2p_listen_addr)
        .await
        .expect("didn't get libp2p listen address in 10 seconds");

    let litep2p_public_addr: Multiaddr = "/ip6/::1/tcp/10000".parse().unwrap();
    litep2p.public_addresses().add_address(litep2p_public_addr.clone()).unwrap();
    // Get public address with peer ID.
    let litep2p_public_addr = litep2p.public_addresses().get_addresses().pop().unwrap();

    let libp2p_peer_id = litep2p::PeerId::from_bytes(&libp2p.local_peer_id().to_bytes()).unwrap();
    litep2p_kad.add_known_peer(libp2p_peer_id, vec![libp2p_listen_addr]).await;

    let litep2p_peer_id = PeerId::from_bytes(&litep2p.local_peer_id().to_bytes()).unwrap();
    let key = vec![1u8, 2u8, 3u8];
    litep2p_kad.start_providing(RecordKey::new(&key), Quorum::All).await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                panic!("provider was not added in 10 secs")
            }
            _ = litep2p.next_event() => {}
            _ = litep2p_kad.next() => {}
            event = libp2p.select_next_some() => {
                if let SwarmEvent::Behaviour(BehaviourEvent::Kad(event)) = event {
                    if let Libp2pKademliaEvent::InboundRequest{request} = event {
                        if let InboundRequest::AddProvider{..} = request {
                            let store = libp2p.behaviour_mut().kad.store_mut();
                            let mut providers = store.providers(&key.clone().into());
                            assert_eq!(providers.len(), 1);
                            let record = providers.pop().unwrap();

                            assert_eq!(record.key.as_ref(), key);
                            assert_eq!(record.provider, litep2p_peer_id);
                            assert_eq!(record.addresses, vec![litep2p_public_addr.clone()]);
                            break
                        }
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn libp2p_add_provider_to_litep2p() {
    let (mut litep2p, mut litep2p_kad) = initialize_litep2p();
    let mut libp2p = initialize_libp2p();

    let libp2p_peerid = litep2p::PeerId::from_bytes(&libp2p.local_peer_id().to_bytes()).unwrap();
    let libp2p_public_addr: Multiaddr = "/ip4/1.1.1.1/tcp/10000".parse().unwrap();
    libp2p.add_external_address(libp2p_public_addr.clone(), AddressScore::Infinite);

    let litep2p_peerid = PeerId::from_bytes(&litep2p.local_peer_id().to_bytes()).unwrap();
    let litep2p_address = litep2p.listen_addresses().next().unwrap().clone();
    libp2p.behaviour_mut().kad.add_address(&litep2p_peerid, litep2p_address);

    // Start providing
    let key = vec![1u8, 2u8, 3u8];
    libp2p.behaviour_mut().kad.start_providing(key.clone().into()).unwrap();

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                panic!("provider was not added in 10 secs")
            }
            _ = litep2p.next_event() => {}
            _ = libp2p.select_next_some() => {}
            event = litep2p_kad.next() => {
                if let Some(KademliaEvent::IncomingProvider{ provided_key, provider }) = event {
                    assert_eq!(provided_key, key.clone().into());
                    assert_eq!(provider.peer, libp2p_peerid);
                    assert_eq!(provider.addresses, vec![libp2p_public_addr]);

                    break
                }
            }
        }
    }
}

#[tokio::test]
async fn litep2p_get_providers_from_libp2p() {
    let (mut litep2p, mut litep2p_kad) = initialize_litep2p();
    let mut libp2p = initialize_libp2p();

    let libp2p_peerid = litep2p::PeerId::from_bytes(&libp2p.local_peer_id().to_bytes()).unwrap();
    let libp2p_public_addr: Multiaddr = "/ip4/1.1.1.1/tcp/10000".parse().unwrap();
    libp2p.add_external_address(libp2p_public_addr.clone(), AddressScore::Infinite);

    // Start providing
    let key = vec![1u8, 2u8, 3u8];
    let query_id = libp2p.behaviour_mut().kad.start_providing(key.clone().into()).unwrap();

    let mut libp2p_listen_addr = None;
    let mut provider_stored = false;

    // Drive libp2p a little bit to get listen address and make sure the provider was store
    // loacally.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match libp2p.select_next_some().await {
                SwarmEvent::Behaviour(BehaviourEvent::Kad(
                    Libp2pKademliaEvent::OutboundQueryProgressed { id, result, .. },
                )) => {
                    assert_eq!(id, query_id);
                    assert!(
                        matches!(result, QueryResult::StartProviding(Ok(AddProviderOk { key }))
                                if key == key.clone())
                    );

                    provider_stored = true;

                    if libp2p_listen_addr.is_some() {
                        break;
                    }
                }
                SwarmEvent::NewListenAddr { address, .. } => {
                    libp2p_listen_addr = Some(address);

                    if provider_stored {
                        break;
                    }
                }
                _ => {}
            }
        }
    })
    .await
    .expect("failed to store provider and get listen address in 10 seconds");

    let libp2p_listen_addr = libp2p_listen_addr.unwrap();

    // `GET_PROVIDERS`
    litep2p_kad
        .add_known_peer(libp2p_peerid, vec![libp2p_listen_addr.clone()])
        .await;
    let original_query_id = litep2p_kad.get_providers(key.clone().into()).await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                panic!("provider was not added in 10 secs")
            }
            _ = litep2p.next_event() => {}
            _ = libp2p.select_next_some() => {}
            event = litep2p_kad.next() => {
                if let Some(KademliaEvent::GetProvidersSuccess {
                    query_id,
                    provided_key,
                    mut providers,
                }) = event {
                    assert_eq!(query_id, original_query_id);
                    assert_eq!(provided_key, key.clone().into());
                    assert_eq!(providers.len(), 1);

                    let provider = providers.pop().unwrap();
                    assert_eq!(provider.peer, libp2p_peerid);
                    assert_eq!(provider.addresses.len(), 2);
                    assert!(provider.addresses.contains(&libp2p_listen_addr));
                    assert!(provider.addresses.contains(&libp2p_public_addr));

                    break
                }
            }
        }
    }
}

#[tokio::test]
async fn libp2p_get_providers_from_litep2p() {
    let (mut litep2p, mut litep2p_kad) = initialize_litep2p();
    let mut libp2p = initialize_libp2p();

    let litep2p_peerid = PeerId::from_bytes(&litep2p.local_peer_id().to_bytes()).unwrap();
    let litep2p_listen_address = litep2p.listen_addresses().next().unwrap().clone();
    let litep2p_public_address: Multiaddr = "/ip4/1.1.1.1/tcp/10000".parse().unwrap();
    litep2p.public_addresses().add_address(litep2p_public_address).unwrap();

    // Store provider locally in litep2p.
    let original_key = vec![1u8, 2u8, 3u8];
    litep2p_kad.start_providing(original_key.clone().into(), Quorum::All).await;

    // Drive litep2p a little bit to make sure the provider record is stored and no `ADD_PROVIDER`
    // requests are generated (because no peers are know yet).
    tokio::time::timeout(Duration::from_secs(2), async {
        litep2p.next_event().await;
    })
    .await
    .unwrap_err();

    libp2p.behaviour_mut().kad.add_address(&litep2p_peerid, litep2p_listen_address);
    let query_id = libp2p.behaviour_mut().kad.get_providers(original_key.clone().into());

    loop {
        tokio::select! {
            event = libp2p.select_next_some() => {
                if let SwarmEvent::Behaviour(BehaviourEvent::Kad(
                        Libp2pKademliaEvent::OutboundQueryProgressed { id, result, .. })
                    ) = event {
                    assert_eq!(id, query_id);
                    if let QueryResult::GetProviders(Ok(
                        GetProvidersOk::FoundProviders { key, providers }
                    )) = result {
                        assert_eq!(key, original_key.clone().into());
                        assert_eq!(providers.len(), 1);
                        assert!(providers.contains(&litep2p_peerid));
                        // It looks like `libp2p` discards the cached provider addresses received
                        // in `GET_PROVIDERS` response, so we can't check it here.
                        // The addresses are neither used to extend the `libp2p` routing table.
                        break
                    } else {
                        panic!("invalid query result")
                    }
                }
            }
            _ = litep2p.next_event() => {}
            _ = litep2p_kad.next() => {}
        }
    }
}
