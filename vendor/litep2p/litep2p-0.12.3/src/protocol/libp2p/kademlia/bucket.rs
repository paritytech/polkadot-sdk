// Copyright 2018-2019 Parity Technologies (UK) Ltd.
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

//! Kademlia k-bucket implementation.

use crate::{
    protocol::libp2p::kademlia::types::{ConnectionType, KademliaPeer, Key},
    PeerId,
};

/// K-bucket entry.
#[derive(Debug)]
pub enum KBucketEntry<'a> {
    /// Entry points to local node.
    LocalNode,

    /// Occupied entry to a connected node.
    Occupied(&'a mut KademliaPeer),

    /// Vacant entry.
    Vacant(&'a mut KademliaPeer),

    /// Entry not found and any present entry cannot be replaced.
    NoSlot,
}

impl<'a> KBucketEntry<'a> {
    /// Insert new entry into the entry if possible.
    pub fn insert(&'a mut self, new: KademliaPeer) {
        if let KBucketEntry::Vacant(old) = self {
            old.peer = new.peer;
            old.key = Key::from(new.peer);
            old.address_store = new.address_store;
            old.connection = new.connection;
        }
    }
}

/// Kademlia k-bucket.
pub struct KBucket {
    // TODO: https://github.com/paritytech/litep2p/issues/335
    // store peers in a btreemap with increasing distance from local key?
    nodes: Vec<KademliaPeer>,
}

impl KBucket {
    /// Create new [`KBucket`].
    pub fn new() -> Self {
        Self {
            nodes: Vec::with_capacity(20),
        }
    }

    /// Get entry into the bucket.
    // TODO: https://github.com/paritytech/litep2p/pull/184 should optimize this
    pub fn entry<K: Clone>(&mut self, key: Key<K>) -> KBucketEntry<'_> {
        for i in 0..self.nodes.len() {
            if self.nodes[i].key == key {
                return KBucketEntry::Occupied(&mut self.nodes[i]);
            }
        }

        if self.nodes.len() < 20 {
            self.nodes.push(KademliaPeer::new(
                PeerId::random(),
                vec![],
                ConnectionType::NotConnected,
            ));
            let len = self.nodes.len() - 1;
            return KBucketEntry::Vacant(&mut self.nodes[len]);
        }

        for i in 0..self.nodes.len() {
            match self.nodes[i].connection {
                ConnectionType::NotConnected | ConnectionType::CannotConnect => {
                    return KBucketEntry::Vacant(&mut self.nodes[i]);
                }
                _ => continue,
            }
        }

        KBucketEntry::NoSlot
    }

    /// Get iterator over the k-bucket, sorting the k-bucket entries in increasing order
    /// by distance.
    pub fn closest_iter<K: Clone>(&self, target: &Key<K>) -> impl Iterator<Item = &KademliaPeer> {
        let mut nodes: Vec<_> = self.nodes.iter().collect();
        nodes.sort_by(|a, b| target.distance(&a.key).cmp(&target.distance(&b.key)));
        nodes.into_iter().filter(|peer| !peer.address_store.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_iter() {
        let mut bucket = KBucket::new();

        // add some random nodes to the bucket
        let _ = (0..10)
            .map(|_| {
                let peer = PeerId::random();
                bucket.nodes.push(KademliaPeer::new(peer, vec![], ConnectionType::Connected));

                peer
            })
            .collect::<Vec<_>>();

        let target = Key::from(PeerId::random());
        let iter = bucket.closest_iter(&target);
        let mut prev = None;

        for node in iter {
            if let Some(distance) = prev {
                assert!(distance < target.distance(&node.key));
            }

            prev = Some(target.distance(&node.key));
        }
    }

    #[test]
    fn ignore_peers_with_no_addresses() {
        let mut bucket = KBucket::new();

        // add peers with no addresses to the bucket
        let _ = (0..10)
            .map(|_| {
                let peer = PeerId::random();
                bucket.nodes.push(KademliaPeer::new(
                    peer,
                    vec![],
                    ConnectionType::NotConnected,
                ));

                peer
            })
            .collect::<Vec<_>>();

        // add three peers with an address
        let _ = (0..3)
            .map(|_| {
                let peer = PeerId::random();
                bucket.nodes.push(KademliaPeer::new(
                    peer,
                    vec!["/ip6/::/tcp/0".parse().unwrap()],
                    ConnectionType::Connected,
                ));

                peer
            })
            .collect::<Vec<_>>();

        let target = Key::from(PeerId::random());
        let iter = bucket.closest_iter(&target);
        let mut prev = None;
        let mut num_peers = 0usize;

        for node in iter {
            if let Some(distance) = prev {
                assert!(distance < target.distance(&node.key));
            }

            num_peers += 1;
            prev = Some(target.distance(&node.key));
        }

        assert_eq!(num_peers, 3usize);
    }
}
