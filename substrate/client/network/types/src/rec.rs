// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::{
    borrow::Borrow,
	time:: Instant,
};
use bytes::Bytes;
use crate::{PeerId, multihash::Multihash};

/// The (opaque) key of a record.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    /// Key of the record.
    pub key: Key,
    /// Value of the record.
    pub value: Vec<u8>,
    /// The (original) publisher of the record.
    pub publisher: Option<PeerId>,
    /// The expiration time as measured by a local, monotonic clock.
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
        self.expires.map_or(false, |t| now >= t)
    }
}

impl From<libp2p_kad::Record> for Record {
    fn from(out: libp2p_kad::Record) -> Self {
        let vec: Vec<u8> = out.key.to_vec();
        let key: Key = vec.into();
        let mut publisher: Option<PeerId> = None;
        if let Some(x) = out.publisher{
            publisher = Some(x.into());
        }
        Record {key, value: out.value, publisher, expires: out.expires}
    }
}
impl From<Record> for libp2p_kad::Record {
    fn from(a:Record) -> libp2p_kad::Record {
        //let key: KademliaKey = a.key.to_vec().into();
        let mut peer: Option<libp2p_identity::PeerId> = None;
        if let Some(x) = a.publisher {
            peer = Some(x.into()); 
        } 
        libp2p_kad::Record {key: a.key.to_vec().into(), value: a.value, publisher: peer, expires: a.expires}
    }
}

/// A record either received by the given peer or retrieved from the local
/// record store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRecord {
    /// The peer from whom the record was received. `None` if the record was
    /// retrieved from local storage.
    pub peer: Option<PeerId>,
    pub record: Record,
}

impl From<libp2p_kad::PeerRecord> for PeerRecord {
    fn from(out: libp2p_kad::PeerRecord) -> Self {
        let mut peer: Option<PeerId> = None;
        if let Some(x) = out.peer{
            peer = Some(x.into());            
        }
        let record = out.record.into();
        PeerRecord {peer, record}

        
    }
}