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

#![allow(unused)]
use bytes::Bytes;
use futures::StreamExt;
use litep2p::{
    config::ConfigBuilder,
    crypto::ed25519::Keypair,
    protocol::libp2p::kademlia::{
        ConfigBuilder as KademliaConfigBuilder, ContentProvider, IncomingRecordValidationMode,
        KademliaEvent, PeerRecord, Quorum, Record, RecordKey,
    },
    transport::tcp::config::Config as TcpConfig,
    types::multiaddr::{Multiaddr, Protocol},
    Litep2p, PeerId,
};

fn spawn_litep2p(port: u16) {
    let (kad_config1, _kad_handle1) = KademliaConfigBuilder::new().build();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            listen_addresses: vec![format!("/ip6/::1/tcp/{port}").parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config1)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();

    tokio::spawn(async move { while let Some(_) = litep2p1.next_event().await {} });
}

#[tokio::test]
#[ignore]
async fn kademlia_supported() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (kad_config1, _kad_handle1) = KademliaConfigBuilder::new().build();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config1)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();

    for port in 9000..9003 {
        spawn_litep2p(port);
    }

    loop {
        tokio::select! {
            event = litep2p1.next_event() => {
                tracing::info!("litep2p event received: {event:?}");
            }
            // event = kad_handle1.next() => {
            //     tracing::info!("kademlia event received: {event:?}");
            // }
        }
    }
}

#[tokio::test]
#[ignore]
async fn put_value() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (kad_config1, mut kad_handle1) = KademliaConfigBuilder::new().build();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config1)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();

    for i in 0..10 {
        kad_handle1
            .add_known_peer(
                PeerId::random(),
                vec![format!("/ip6/::/tcp/{i}").parse().unwrap()],
            )
            .await;
    }

    // let key = RecordKey::new(&Bytes::from(vec![1, 3, 3, 7]));
    // kad_handle1.put_value(key, vec![1, 2, 3, 4]).await;

    // loop {
    //     tokio::select! {
    //         event = litep2p1.next_event() => {
    //             tracing::info!("litep2p event received: {event:?}");
    //         }
    //         event = kad_handle1.next() => {
    //             tracing::info!("kademlia event received: {event:?}");
    //         }
    //     }
    // }
}

#[tokio::test]
async fn records_are_stored_automatically() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (kad_config1, mut kad_handle1) = KademliaConfigBuilder::new().build();
    let (kad_config2, mut kad_handle2) = KademliaConfigBuilder::new().build();

    let config1 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config1)
        .build();

    let config2 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config2)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    kad_handle1
        .add_known_peer(
            *litep2p2.local_peer_id(),
            litep2p2.listen_addresses().cloned().collect(),
        )
        .await;

    // Publish the record.
    let record = Record::new(vec![1, 2, 3], vec![0x01]);
    let query_id = kad_handle1.put_record(record.clone(), Quorum::All).await;
    let mut records = Vec::new();

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                panic!("record was not stored in 10 secs")
            }
            _ = litep2p1.next_event() => {}
            _ = litep2p2.next_event() => {}
            event = kad_handle1.next() => {
                match event {
                    Some(KademliaEvent::PutRecordSuccess { query_id: got_query_id, key }) => {
                        assert_eq!(got_query_id, query_id);
                        assert_eq!(key, record.key);

                        // Check if the record was stored.
                        let _ = kad_handle2
                            .get_record(RecordKey::from(vec![1, 2, 3]), Quorum::One).await;
                    }
                    Some(KademliaEvent::QueryFailed { query_id: got_query_id }) => {
                        assert_eq!(got_query_id, query_id);

                        panic!("query failed")
                    }
                    _ => {}
                }
            }
            event = kad_handle2.next() => {
                match event {
                    Some(KademliaEvent::IncomingRecord { record: got_record }) => {
                        assert_eq!(got_record.key, record.key);
                        assert_eq!(got_record.value, record.value);
                        assert_eq!(got_record.publisher.unwrap(), *litep2p1.local_peer_id());
                        assert!(got_record.expires.is_some());
                    }
                    Some(KademliaEvent::GetRecordPartialResult { query_id: _, record }) => {
                        records.push(record);
                    }
                    Some(KademliaEvent::GetRecordSuccess { query_id: _ }) => {
                        assert_eq!(records.len(), 1);
                        let got_record = records.first().unwrap();
                        // Record retrieved from local storage.
                        assert_eq!(got_record.peer, *litep2p2.local_peer_id());
                        assert_eq!(got_record.record.key, record.key);
                        assert_eq!(got_record.record.value, record.value);
                        assert_eq!(got_record.record.publisher.unwrap(), *litep2p1.local_peer_id());
                        assert!(got_record.record.expires.is_some());
                        break
                    }
                    _ => {}
                }
            }
        }
    }
}

#[tokio::test]
async fn records_are_stored_manually() {
    let (kad_config1, mut kad_handle1) = KademliaConfigBuilder::new()
        .with_incoming_records_validation_mode(IncomingRecordValidationMode::Manual)
        .build();
    let (kad_config2, mut kad_handle2) = KademliaConfigBuilder::new()
        .with_incoming_records_validation_mode(IncomingRecordValidationMode::Manual)
        .build();

    let config1 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config1)
        .build();

    let config2 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config2)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    kad_handle1
        .add_known_peer(
            *litep2p2.local_peer_id(),
            litep2p2.listen_addresses().cloned().collect(),
        )
        .await;

    // Publish the record.
    let mut record = Record::new(vec![1, 2, 3], vec![0x01]);
    let query_id = kad_handle1.put_record(record.clone(), Quorum::All).await;
    let mut records = Vec::new();
    let mut put_record_success = false;
    let mut get_record_success = false;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                panic!("record was not stored in 10 secs")
            }
            _ = litep2p1.next_event() => {}
            _ = litep2p2.next_event() => {}
            event = kad_handle1.next() => {
                match event {
                    Some(KademliaEvent::PutRecordSuccess { query_id: got_query_id, key }) => {
                        assert_eq!(got_query_id, query_id);
                        assert_eq!(key, record.key);

                        // Due to manual validation, the record will be stored later, so we request
                        // it in `kad_handle2` after receiving the incoming record
                        put_record_success = true;

                        if get_record_success {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            event = kad_handle2.next() => {
                match event {
                    Some(KademliaEvent::IncomingRecord { record: got_record }) => {
                        assert_eq!(got_record.key, record.key);
                        assert_eq!(got_record.value, record.value);
                        assert_eq!(got_record.publisher.unwrap(), *litep2p1.local_peer_id());
                        assert!(got_record.expires.is_some());

                        kad_handle2.store_record(got_record).await;

                        // Check if the record was stored.
                        let _ = kad_handle2
                            .get_record(RecordKey::from(vec![1, 2, 3]), Quorum::One).await;
                    }
                    Some(KademliaEvent::GetRecordPartialResult { query_id: _, record }) => {
                        records.push(record);
                    }
                    Some(KademliaEvent::GetRecordSuccess { query_id: _ }) => {
                        assert_eq!(records.len(), 1);
                        let got_record = records.first().unwrap();
                        // Record retrieved from local storage.
                        assert_eq!(got_record.peer, *litep2p2.local_peer_id());
                        assert_eq!(got_record.record.key, record.key);
                        assert_eq!(got_record.record.value, record.value);
                        assert_eq!(got_record.record.publisher.unwrap(), *litep2p1.local_peer_id());
                        assert!(got_record.record.expires.is_some());

                        get_record_success = true;

                        if put_record_success {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[tokio::test]
async fn not_validated_records_are_not_stored() {
    let (kad_config1, mut kad_handle1) = KademliaConfigBuilder::new()
        .with_incoming_records_validation_mode(IncomingRecordValidationMode::Manual)
        .build();
    let (kad_config2, mut kad_handle2) = KademliaConfigBuilder::new()
        .with_incoming_records_validation_mode(IncomingRecordValidationMode::Manual)
        .build();

    let config1 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config1)
        .build();

    let config2 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config2)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    kad_handle1
        .add_known_peer(
            *litep2p2.local_peer_id(),
            litep2p2.listen_addresses().cloned().collect(),
        )
        .await;

    // Publish the record.
    let record = Record::new(vec![1, 2, 3], vec![0x01]);
    let query_id = kad_handle1.put_record(record.clone(), Quorum::All).await;
    let mut records = Vec::new();
    let mut get_record_query_id = None;
    let mut put_record_success = false;
    let mut get_record_success = false;
    let mut query_failed = false;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                panic!("query has not failed in 10 secs")
            }
            event = litep2p1.next_event() => {}
            event = litep2p2.next_event() => {}
            event = kad_handle1.next() => {
                match event {
                    Some(KademliaEvent::PutRecordSuccess { query_id: got_query_id, key }) => {
                        assert_eq!(got_query_id, query_id);
                        assert_eq!(key, record.key);

                        put_record_success = true;

                        if get_record_success || query_failed {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            event = kad_handle2.next() => {
                match event {
                    Some(KademliaEvent::IncomingRecord { record: got_record }) => {
                        assert_eq!(got_record.key, record.key);
                        assert_eq!(got_record.value, record.value);
                        assert_eq!(got_record.publisher.unwrap(), *litep2p1.local_peer_id());
                        assert!(got_record.expires.is_some());
                        // Do not call `kad_handle2.store_record(record).await`.

                        // Check if the record was stored.
                        let query_id = kad_handle2
                            .get_record(RecordKey::from(vec![1, 2, 3]), Quorum::One).await;
                        get_record_query_id = Some(query_id);
                    }
                    Some(KademliaEvent::GetRecordPartialResult { query_id, record }) => {
                        assert_eq!(query_id, get_record_query_id.unwrap());
                        records.push(record);
                    }
                    Some(KademliaEvent::GetRecordSuccess { query_id: _ }) => {
                        assert_eq!(records.len(), 1);
                        let got_record = records.first().unwrap();
                        // The record was not stored at litep2p2.
                        assert_eq!(got_record.peer, *litep2p1.local_peer_id());

                        get_record_success = true;

                        if put_record_success {
                            break
                        }
                    }
                    Some(KademliaEvent::QueryFailed { query_id }) => {
                        assert_eq!(query_id, get_record_query_id.unwrap());

                        query_failed = true;

                        if put_record_success {
                            break
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[tokio::test]
async fn get_record_retrieves_remote_records() {
    let (kad_config1, mut kad_handle1) = KademliaConfigBuilder::new()
        .with_incoming_records_validation_mode(IncomingRecordValidationMode::Manual)
        .build();
    let (kad_config2, mut kad_handle2) = KademliaConfigBuilder::new()
        .with_incoming_records_validation_mode(IncomingRecordValidationMode::Manual)
        .build();

    let config1 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config1)
        .build();

    let config2 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config2)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    // Store the record on `litep2p1``.
    let original_record = Record::new(vec![1, 2, 3], vec![0x01]);
    let query1 = kad_handle1.put_record(original_record.clone(), Quorum::All).await;

    let mut records = Vec::new();
    let mut query2 = None;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                panic!("record was not retrieved in 10 secs")
            }
            event = litep2p1.next_event() => {}
            event = litep2p2.next_event() => {}
            event = kad_handle1.next() => {
                if let Some(KademliaEvent::QueryFailed { query_id }) = event {
                    // Query failed, but the record was stored locally.
                    assert_eq!(query_id, query1);

                    // Let peer2 know about peer1.
                    kad_handle2
                        .add_known_peer(
                            *litep2p1.local_peer_id(),
                            litep2p1.listen_addresses().cloned().collect(),
                        )
                        .await;

                    // Let peer2 get record from peer1.
                    let query_id = kad_handle2
                        .get_record(RecordKey::from(vec![1, 2, 3]), Quorum::One).await;
                    query2 = Some(query_id);
                }
            }
            event = kad_handle2.next() => {
                match event {
                    Some(KademliaEvent::GetRecordPartialResult { query_id: _, record }) => {
                        records.push(record);
                    }
                    Some(KademliaEvent::GetRecordSuccess { query_id: _ }) => {
                        assert_eq!(records.len(), 1);
                        let got_record = records.first().unwrap();
                        assert_eq!(got_record.peer, *litep2p1.local_peer_id());
                        assert_eq!(got_record.record.key, original_record.key);
                        assert_eq!(got_record.record.value, original_record.value);
                        assert_eq!(got_record.record.publisher.unwrap(), *litep2p1.local_peer_id());
                        assert!(got_record.record.expires.is_some());
                        break
                    }
                    Some(KademliaEvent::QueryFailed { query_id: _ }) => {
                        panic!("query failed")
                    }
                    _ => {}
                }
            }
        }
    }
}

#[tokio::test]
async fn get_record_retrieves_local_and_remote_records() {
    let (kad_config1, mut kad_handle1) = KademliaConfigBuilder::new().build();
    let (kad_config2, mut kad_handle2) = KademliaConfigBuilder::new().build();

    let config1 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config1)
        .build();

    let config2 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config2)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    // Let peers jnow about each other
    kad_handle1
        .add_known_peer(
            *litep2p2.local_peer_id(),
            litep2p2.listen_addresses().cloned().collect(),
        )
        .await;
    kad_handle2
        .add_known_peer(
            *litep2p1.local_peer_id(),
            litep2p1.listen_addresses().cloned().collect(),
        )
        .await;

    // Store the record on `litep2p1``.
    let original_record = Record::new(vec![1, 2, 3], vec![0x01]);
    let query1 = kad_handle1.put_record(original_record.clone(), Quorum::All).await;

    let (mut peer1_stored, mut peer2_stored) = (false, false);
    let mut query3 = None;
    let mut records = Vec::new();
    let mut put_record_success = false;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                panic!("record was not retrieved in 10 secs")
            }
            event = litep2p1.next_event() => {}
            event = litep2p2.next_event() => {}
            event = kad_handle1.next() => {
                match event {
                    Some(KademliaEvent::PutRecordSuccess { query_id: got_query_id, key }) => {
                        assert_eq!(got_query_id, query1);
                        assert_eq!(key, original_record.key);

                        // Due to manual validation, the record will be stored later, so we request
                        // it in `kad_handle2` after receiving the incoming record
                        put_record_success = true;
                    }
                    _ => {}
                }
            }
            event = kad_handle2.next() => {
                match event {
                    Some(KademliaEvent::IncomingRecord { record: got_record }) => {
                        assert_eq!(got_record.key, original_record.key);
                        assert_eq!(got_record.value, original_record.value);
                        assert_eq!(got_record.publisher.unwrap(), *litep2p1.local_peer_id());
                        assert!(got_record.expires.is_some());

                        // Get record.
                        let query_id = kad_handle2
                            .get_record(RecordKey::from(vec![1, 2, 3]), Quorum::All).await;
                        query3 = Some(query_id);
                    }
                    Some(KademliaEvent::GetRecordPartialResult { query_id: _, record }) => {
                        records.push(record);
                    }
                    Some(KademliaEvent::GetRecordSuccess { query_id: _ }) => {
                        assert_eq!(records.len(), 2);

                        // Locally retrieved record goes first.
                        assert_eq!(records[0].peer, *litep2p2.local_peer_id());
                        assert_eq!(records[0].record.key, original_record.key);
                        assert_eq!(records[0].record.value, original_record.value);
                        assert_eq!(records[0].record.publisher.unwrap(), *litep2p1.local_peer_id());
                        assert!(records[0].record.expires.is_some());

                        // Remote record from peer 1.
                        assert_eq!(records[1].peer, *litep2p1.local_peer_id());
                        assert_eq!(records[1].record.key, original_record.key);
                        assert_eq!(records[1].record.value, original_record.value);
                        assert_eq!(records[1].record.publisher.unwrap(), *litep2p1.local_peer_id());
                        assert!(records[1].record.expires.is_some());

                        break
                    }
                    Some(KademliaEvent::QueryFailed { query_id: _ }) => {
                        panic!("peer2 query failed")
                    }
                    _ => {}
                }
            }
        }
    }

    assert!(
        put_record_success,
        "Publisher was not notified that the record was received",
    );
}

#[tokio::test]
async fn provider_retrieved_by_remote_node() {
    let (kad_config1, mut kad_handle1) = KademliaConfigBuilder::new().build();
    let (kad_config2, mut kad_handle2) = KademliaConfigBuilder::new().build();

    let config1 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config1)
        .build();

    let config2 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config2)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    // Register at least one public address.
    let peer1 = *litep2p1.local_peer_id();
    let peer1_public_address = "/ip4/192.168.0.1/tcp/10000"
        .parse::<Multiaddr>()
        .unwrap()
        .with(Protocol::P2p(peer1.into()));
    litep2p1.public_addresses().add_address(peer1_public_address.clone());
    assert_eq!(
        litep2p1.public_addresses().get_addresses(),
        vec![peer1_public_address.clone()],
    );

    // Store provider locally.
    let key = RecordKey::new(&vec![1, 2, 3]);
    let query0 = kad_handle1.start_providing(key.clone(), Quorum::All).await;

    // This is the expected provider.
    let expected_provider = ContentProvider {
        peer: peer1,
        addresses: vec![peer1_public_address],
    };

    let mut query1 = None;
    let mut query2 = None;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                panic!("provider was not retrieved in 10 secs")
            }
            event = litep2p1.next_event() => {}
            event = litep2p2.next_event() => {}
            event = kad_handle1.next() => {
                if let Some(KademliaEvent::QueryFailed { query_id }) = event {
                    // Publishing the provider failed, because the nodes are not connected.
                    assert_eq!(query_id, query0);
                    // This request to get provider should fail because the nodes are still
                    // not connected.
                    query1 = Some(kad_handle2.get_providers(key.clone()).await);
                }
            }
            event = kad_handle2.next() => {
                match event {
                    Some(KademliaEvent::QueryFailed { query_id }) => {
                        // Query failed, because the nodes don't know about each other yet.
                        assert_eq!(Some(query_id), query1);

                        // Let the node know about `litep2p1`.
                        kad_handle2
                            .add_known_peer(
                                *litep2p1.local_peer_id(),
                                litep2p1.listen_addresses().cloned().collect(),
                            )
                            .await;

                        // And request providers again.
                        query2 = Some(kad_handle2.get_providers(key.clone()).await);
                    }
                    Some(KademliaEvent::GetProvidersSuccess {
                        query_id,
                        provided_key,
                        providers,
                    }) => {
                        assert_eq!(query_id, query2.unwrap());
                        assert_eq!(provided_key, key);
                        assert_eq!(providers.len(), 1);
                        assert_eq!(providers.first().unwrap(), &expected_provider);

                        break
                    }
                    _ => {}
                }
            }
        }
    }
}

#[tokio::test]
async fn provider_added_to_remote_node() {
    let (kad_config1, mut kad_handle1) = KademliaConfigBuilder::new().build();
    let (kad_config2, mut kad_handle2) = KademliaConfigBuilder::new().build();

    let config1 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config1)
        .build();

    let config2 = ConfigBuilder::new()
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_kademlia(kad_config2)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    // Register at least one public address.
    let peer1 = *litep2p1.local_peer_id();
    let peer1_public_address = "/ip4/192.168.0.1/tcp/10000"
        .parse::<Multiaddr>()
        .unwrap()
        .with(Protocol::P2p(peer1.into()));
    litep2p1.public_addresses().add_address(peer1_public_address.clone());
    assert_eq!(
        litep2p1.public_addresses().get_addresses(),
        vec![peer1_public_address.clone()],
    );

    // Let peer1 know about peer2.
    kad_handle1
        .add_known_peer(
            *litep2p2.local_peer_id(),
            litep2p2.listen_addresses().cloned().collect(),
        )
        .await;

    // Start provodong.
    let key = RecordKey::new(&vec![1, 2, 3]);
    let query = kad_handle1.start_providing(key.clone(), Quorum::All).await;
    let mut add_provider_success = false;
    let mut incoming_provider = false;

    // This is the expected provider.
    let expected_provider = ContentProvider {
        peer: peer1,
        addresses: vec![peer1_public_address],
    };

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                panic!("provider was not retrieved in 10 secs")
            }
            event = litep2p1.next_event() => {}
            event = litep2p2.next_event() => {}
            event = kad_handle1.next() => {
                if let Some(KademliaEvent::AddProviderSuccess { query_id, provided_key }) = event {
                    assert_eq!(query_id, query);
                    assert_eq!(provided_key, key);
                    add_provider_success = true;
                    if incoming_provider {
                        break
                    }
                }
            }
            event = kad_handle2.next() => {
                if let Some(KademliaEvent::IncomingProvider { provided_key, provider }) = event {
                    assert_eq!(provided_key, key);
                    assert_eq!(provider, expected_provider);
                    incoming_provider = true;
                    if add_provider_success {
                        break
                    }
                }
            }
        }
    }
}
