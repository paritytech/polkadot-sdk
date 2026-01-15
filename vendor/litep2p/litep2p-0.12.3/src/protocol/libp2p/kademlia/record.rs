// Copyright 2019 Parity Technologies (UK) Ltd.
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
    protocol::libp2p::kademlia::types::{
        ConnectionType, Distance, KademliaPeer, Key as KademliaKey,
    },
    transport::manager::address::{AddressRecord, AddressStore},
    Multiaddr, PeerId,
};

use bytes::Bytes;
use multihash::Multihash;

use std::{borrow::Borrow, time::Instant};

/// The (opaque) key of a record.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub struct Key(Bytes);

impl Key {
    /// Creates a new key from the bytes of the input.
    pub fn new<K: AsRef<[u8]>>(key: &K) -> Self {
        Key(Bytes::copy_from_slice(key.as_ref()))
    }

    /// Copies the bytes of the key into a new vector.
    pub fn to_vec(&self) -> Vec<u8> {
        Vec::from(&self.0[..])
    }
}

impl From<Key> for Vec<u8> {
    fn from(k: Key) -> Vec<u8> {
        Vec::from(&k.0[..])
    }
}

impl Borrow<[u8]> for Key {
    fn borrow(&self) -> &[u8] {
        &self.0[..]
    }
}

impl AsRef<[u8]> for Key {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl From<Vec<u8>> for Key {
    fn from(v: Vec<u8>) -> Key {
        Key(Bytes::from(v))
    }
}

impl From<Multihash> for Key {
    fn from(m: Multihash) -> Key {
        Key::from(m.to_bytes())
    }
}

/// A record stored in the DHT.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub struct Record {
    /// Key of the record.
    pub key: Key,

    /// Value of the record.
    pub value: Vec<u8>,

    /// The (original) publisher of the record.
    pub publisher: Option<PeerId>,

    /// The expiration time as measured by a local, monotonic clock.
    #[cfg_attr(feature = "fuzz", serde(with = "serde_millis"))]
    pub expires: Option<Instant>,
}

impl Record {
    /// Creates a new record for insertion into the DHT.
    pub fn new<K>(key: K, value: Vec<u8>) -> Self
    where
        K: Into<Key>,
    {
        Record {
            key: key.into(),
            value,
            publisher: None,
            expires: None,
        }
    }

    /// Checks whether the record is expired w.r.t. the given `Instant`.
    pub fn is_expired(&self, now: Instant) -> bool {
        self.expires.is_some_and(|t| now >= t)
    }
}

/// A record received by the given peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRecord {
    /// The peer from whom the record was received
    pub peer: PeerId,

    /// The provided record.
    pub record: Record,
}

/// A record keeping information about a content provider.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ProviderRecord {
    /// Key of the record.
    pub key: Key,

    /// Key of the provider, based on its peer ID.
    pub provider: PeerId,

    /// Cached addresses of the provider.
    pub addresses: Vec<Multiaddr>,

    /// The expiration time of the record. The provider records must always have the expiration
    /// time.
    pub expires: Instant,
}

impl ProviderRecord {
    /// The distance from the provider's peer ID to the provided key.
    pub fn distance(&self) -> Distance {
        // Note that the record key is raw (opaque bytes). In order to calculate the distance from
        // the provider's peer ID to this key we must first hash both.
        KademliaKey::from(self.provider).distance(&KademliaKey::new(self.key.clone()))
    }

    /// Checks whether the record is expired w.r.t. the given `Instant`.
    pub fn is_expired(&self, now: Instant) -> bool {
        now >= self.expires
    }
}

/// A user-facing provider type.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ContentProvider {
    // Peer ID of the provider.
    pub peer: PeerId,

    // Cached addresses of the provider.
    pub addresses: Vec<Multiaddr>,
}

impl From<ContentProvider> for KademliaPeer {
    fn from(provider: ContentProvider) -> Self {
        let mut address_store = AddressStore::new();
        for address in provider.addresses.iter() {
            address_store.insert(AddressRecord::from_raw_multiaddr(address.clone()));
        }

        Self {
            key: KademliaKey::from(provider.peer),
            peer: provider.peer,
            address_store,
            connection: ConnectionType::NotConnected,
        }
    }
}
