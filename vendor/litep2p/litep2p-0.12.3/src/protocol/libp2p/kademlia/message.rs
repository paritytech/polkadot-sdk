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
    protocol::libp2p::kademlia::{
        record::{ContentProvider, Key as RecordKey, Record},
        schema,
        types::{ConnectionType, KademliaPeer},
    },
    PeerId,
};

use bytes::{Bytes, BytesMut};
use enum_display::EnumDisplay;
use prost::Message;
use std::time::{Duration, Instant};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::ipfs::kademlia::message";

/// Kademlia message.
#[derive(Debug, Clone, EnumDisplay)]
pub enum KademliaMessage {
    /// `FIND_NODE` message.
    FindNode {
        /// Query target.
        target: Vec<u8>,

        /// Found peers.
        peers: Vec<KademliaPeer>,
    },

    /// Kademlia `PUT_VALUE` message.
    PutValue {
        /// Record.
        record: Record,
    },

    /// `GET_VALUE` message.
    GetRecord {
        /// Key.
        key: Option<RecordKey>,

        /// Record.
        record: Option<Record>,

        /// Peers closer to the key.
        peers: Vec<KademliaPeer>,
    },

    /// `ADD_PROVIDER` message.
    AddProvider {
        /// Key.
        key: RecordKey,

        /// Peers, providing the data for `key`. Must contain exactly one peer matching the sender
        /// of the message.
        providers: Vec<KademliaPeer>,
    },

    /// `GET_PROVIDERS` message.
    GetProviders {
        /// Key. `None` in response.
        key: Option<RecordKey>,

        /// Peers closer to the key.
        peers: Vec<KademliaPeer>,

        /// Peers, providing the data for `key`.
        providers: Vec<KademliaPeer>,
    },
}

impl KademliaMessage {
    /// Create `FIND_NODE` message for `peer`.
    pub fn find_node<T: Into<Vec<u8>>>(key: T) -> Bytes {
        let message = schema::kademlia::Message {
            key: key.into(),
            r#type: schema::kademlia::MessageType::FindNode.into(),
            cluster_level_raw: 10,
            ..Default::default()
        };

        let mut buf = BytesMut::with_capacity(message.encoded_len());
        message.encode(&mut buf).expect("Vec<u8> to provide needed capacity");

        buf.freeze()
    }

    /// Create `PUT_VALUE` message for `record`.
    pub fn put_value(record: Record) -> Bytes {
        let message = schema::kademlia::Message {
            key: record.key.clone().into(),
            r#type: schema::kademlia::MessageType::PutValue.into(),
            record: Some(record_to_schema(record)),
            cluster_level_raw: 10,
            ..Default::default()
        };

        let mut buf = BytesMut::with_capacity(message.encoded_len());
        message.encode(&mut buf).expect("BytesMut to provide needed capacity");

        buf.freeze()
    }

    /// Create `GET_VALUE` message for `record`.
    pub fn get_record(key: RecordKey) -> Bytes {
        let message = schema::kademlia::Message {
            key: key.clone().into(),
            r#type: schema::kademlia::MessageType::GetValue.into(),
            cluster_level_raw: 10,
            ..Default::default()
        };

        let mut buf = BytesMut::with_capacity(message.encoded_len());
        message.encode(&mut buf).expect("BytesMut to provide needed capacity");

        buf.freeze()
    }

    /// Create `FIND_NODE` response.
    pub fn find_node_response<K: AsRef<[u8]>>(key: K, peers: Vec<KademliaPeer>) -> Vec<u8> {
        let message = schema::kademlia::Message {
            key: key.as_ref().to_vec(),
            cluster_level_raw: 10,
            r#type: schema::kademlia::MessageType::FindNode.into(),
            closer_peers: peers.iter().map(|peer| peer.into()).collect(),
            ..Default::default()
        };

        let mut buf = Vec::with_capacity(message.encoded_len());
        message.encode(&mut buf).expect("Vec<u8> to provide needed capacity");

        buf
    }

    /// Create `PUT_VALUE` response.
    pub fn put_value_response(key: RecordKey, value: Vec<u8>) -> Bytes {
        let message = schema::kademlia::Message {
            key: key.to_vec(),
            cluster_level_raw: 10,
            r#type: schema::kademlia::MessageType::PutValue.into(),
            record: Some(schema::kademlia::Record {
                key: key.to_vec(),
                value,
                ..Default::default()
            }),
            ..Default::default()
        };

        let mut buf = BytesMut::with_capacity(message.encoded_len());
        message.encode(&mut buf).expect("BytesMut to provide needed capacity");

        buf.freeze()
    }

    /// Create `GET_VALUE` response.
    pub fn get_value_response(
        key: RecordKey,
        peers: Vec<KademliaPeer>,
        record: Option<Record>,
    ) -> Vec<u8> {
        let message = schema::kademlia::Message {
            key: key.to_vec(),
            cluster_level_raw: 10,
            r#type: schema::kademlia::MessageType::GetValue.into(),
            closer_peers: peers.iter().map(|peer| peer.into()).collect(),
            record: record.map(record_to_schema),
            ..Default::default()
        };

        let mut buf = Vec::with_capacity(message.encoded_len());
        message.encode(&mut buf).expect("Vec<u8> to provide needed capacity");

        buf
    }

    /// Create `ADD_PROVIDER` message with `provider`.
    pub fn add_provider(provided_key: RecordKey, provider: ContentProvider) -> Bytes {
        let peer = KademliaPeer::new(
            provider.peer,
            provider.addresses,
            ConnectionType::CanConnect, // ignored by message recipient
        );
        let message = schema::kademlia::Message {
            key: provided_key.clone().to_vec(),
            cluster_level_raw: 10,
            r#type: schema::kademlia::MessageType::AddProvider.into(),
            provider_peers: std::iter::once((&peer).into()).collect(),
            ..Default::default()
        };

        let mut buf = BytesMut::with_capacity(message.encoded_len());
        message.encode(&mut buf).expect("BytesMut to provide needed capacity");

        buf.freeze()
    }

    /// Create `GET_PROVIDERS` request for `key`.
    pub fn get_providers_request(key: RecordKey) -> Bytes {
        let message = schema::kademlia::Message {
            key: key.to_vec(),
            cluster_level_raw: 10,
            r#type: schema::kademlia::MessageType::GetProviders.into(),
            ..Default::default()
        };

        let mut buf = BytesMut::with_capacity(message.encoded_len());
        message.encode(&mut buf).expect("BytesMut to provide needed capacity");

        buf.freeze()
    }

    /// Create `GET_PROVIDERS` response.
    pub fn get_providers_response(
        providers: Vec<ContentProvider>,
        closer_peers: &[KademliaPeer],
    ) -> Vec<u8> {
        let provider_peers = providers
            .into_iter()
            .map(|p| {
                KademliaPeer::new(
                    p.peer,
                    p.addresses,
                    // `ConnectionType` is ignored by a recipient
                    ConnectionType::NotConnected,
                )
            })
            .map(|p| (&p).into())
            .collect();

        let message = schema::kademlia::Message {
            cluster_level_raw: 10,
            r#type: schema::kademlia::MessageType::GetProviders.into(),
            closer_peers: closer_peers.iter().map(Into::into).collect(),
            provider_peers,
            ..Default::default()
        };

        let mut buf = Vec::with_capacity(message.encoded_len());
        message.encode(&mut buf).expect("Vec<u8> to provide needed capacity");

        buf
    }

    /// Get [`KademliaMessage`] from bytes.
    pub fn from_bytes(bytes: BytesMut, replication_factor: usize) -> Option<Self> {
        match schema::kademlia::Message::decode(bytes) {
            Ok(message) => match message.r#type {
                // FIND_NODE
                4 => {
                    let peers = message
                        .closer_peers
                        .iter()
                        .filter_map(|peer| KademliaPeer::try_from(peer).ok())
                        .take(replication_factor)
                        .collect();

                    Some(Self::FindNode {
                        target: message.key,
                        peers,
                    })
                }
                // PUT_VALUE
                0 => {
                    let record = message.record?;

                    Some(Self::PutValue {
                        record: record_from_schema(record)?,
                    })
                }
                // GET_VALUE
                1 => {
                    let key = match message.key.is_empty() {
                        true => message.record.as_ref().and_then(|record| {
                            (!record.key.is_empty()).then_some(RecordKey::from(record.key.clone()))
                        }),
                        false => Some(RecordKey::from(message.key.clone())),
                    };

                    let record = if let Some(record) = message.record {
                        Some(record_from_schema(record)?)
                    } else {
                        None
                    };

                    Some(Self::GetRecord {
                        key,
                        record,
                        peers: message
                            .closer_peers
                            .iter()
                            .filter_map(|peer| KademliaPeer::try_from(peer).ok())
                            .take(replication_factor)
                            .collect(),
                    })
                }
                // ADD_PROVIDER
                2 => {
                    let key = (!message.key.is_empty()).then_some(message.key.into())?;
                    let providers = message
                        .provider_peers
                        .iter()
                        .filter_map(|peer| KademliaPeer::try_from(peer).ok())
                        .take(replication_factor)
                        .collect();

                    Some(Self::AddProvider { key, providers })
                }
                // GET_PROVIDERS
                3 => {
                    let key = (!message.key.is_empty()).then_some(message.key.into());
                    let peers = message
                        .closer_peers
                        .iter()
                        .filter_map(|peer| KademliaPeer::try_from(peer).ok())
                        .take(replication_factor)
                        .collect();
                    let providers = message
                        .provider_peers
                        .iter()
                        .filter_map(|peer| KademliaPeer::try_from(peer).ok())
                        .take(replication_factor)
                        .collect();

                    Some(Self::GetProviders {
                        key,
                        peers,
                        providers,
                    })
                }
                message_type => {
                    tracing::warn!(target: LOG_TARGET, ?message_type, "unhandled message");
                    None
                }
            },
            Err(error) => {
                tracing::debug!(target: LOG_TARGET, ?error, "failed to decode message");
                None
            }
        }
    }
}

fn record_to_schema(record: Record) -> schema::kademlia::Record {
    schema::kademlia::Record {
        key: record.key.into(),
        value: record.value,
        time_received: String::new(),
        publisher: record.publisher.map(|peer_id| peer_id.to_bytes()).unwrap_or_default(),
        ttl: record
            .expires
            .map(|expires| {
                let now = Instant::now();
                if expires > now {
                    u32::try_from((expires - now).as_secs()).unwrap_or(u32::MAX)
                } else {
                    1 // because 0 means "does not expire"
                }
            })
            .unwrap_or(0),
    }
}

fn record_from_schema(record: schema::kademlia::Record) -> Option<Record> {
    Some(Record {
        key: record.key.into(),
        value: record.value,
        publisher: if !record.publisher.is_empty() {
            Some(PeerId::from_bytes(&record.publisher).ok()?)
        } else {
            None
        },
        expires: if record.ttl > 0 {
            Some(Instant::now() + Duration::from_secs(record.ttl as u64))
        } else {
            None
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_publisher_and_ttl_are_preserved() {
        let expires = Instant::now() + Duration::from_secs(3600);

        let record = Record {
            key: vec![1, 2, 3].into(),
            value: vec![17],
            publisher: Some(PeerId::random()),
            expires: Some(expires),
        };

        let got_record = record_from_schema(record_to_schema(record.clone())).unwrap();

        assert_eq!(got_record.key, record.key);
        assert_eq!(got_record.value, record.value);
        assert_eq!(got_record.publisher, record.publisher);

        // Check that the expiration time is sane.
        let got_expires = got_record.expires.unwrap();
        assert!(got_expires - expires >= Duration::ZERO);
        assert!(got_expires - expires < Duration::from_secs(10));
    }

    #[test]
    fn empty_publisher_and_ttl_are_preserved() {
        let record = Record {
            key: vec![1, 2, 3].into(),
            value: vec![17],
            publisher: None,
            expires: None,
        };

        let got_record = record_from_schema(record_to_schema(record.clone())).unwrap();

        assert_eq!(got_record, record);
    }
}
