

use std::{
    borrow::Borrow,
	collections::{hash_map::Entry, HashMap},
	io, iter,
	ops::Deref,
	pin::Pin,
	sync::Arc,
	task::{Context, Poll},
	time::{Duration, Instant},
};
use bytes::Bytes;
use crate::{PeerId, multihash::Multihash, multiaddr::Multiaddr};

/// The (opaque) key of a record.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
        let mut expires: Option<Instant> = None;
        if let Some(x) = out.publisher{
            publisher = Some(x.into());
        }
        Record {key, value: out.value, publisher, expires: out.expires}
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
        PeerRecord {peer: peer, record}

        
    }
}