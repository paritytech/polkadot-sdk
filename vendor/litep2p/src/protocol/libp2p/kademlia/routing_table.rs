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

//! Kademlia routing table implementation.

use crate::{
    protocol::libp2p::kademlia::{
        bucket::{KBucket, KBucketEntry},
        types::{ConnectionType, Distance, KademliaPeer, Key, U256},
    },
    transport::{
        manager::address::{scores, AddressRecord},
        Endpoint,
    },
    PeerId,
};

use multiaddr::{Multiaddr, Protocol};
use multihash::Multihash;

/// Number of k-buckets.
const NUM_BUCKETS: usize = 256;

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::ipfs::kademlia::routing_table";

pub struct RoutingTable {
    /// Local key.
    local_key: Key<PeerId>,

    /// K-buckets.
    buckets: Vec<KBucket>,
}

/// A (type-safe) index into a `KBucketsTable`, i.e. a non-negative integer in the
/// interval `[0, NUM_BUCKETS)`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct BucketIndex(usize);

impl BucketIndex {
    /// Creates a new `BucketIndex` for a `Distance`.
    ///
    /// The given distance is interpreted as the distance from a `local_key` of
    /// a `KBucketsTable`. If the distance is zero, `None` is returned, in
    /// recognition of the fact that the only key with distance `0` to a
    /// `local_key` is the `local_key` itself, which does not belong in any
    /// bucket.
    fn new(d: &Distance) -> Option<BucketIndex> {
        d.ilog2().map(|i| BucketIndex(i as usize))
    }

    /// Gets the index value as an unsigned integer.
    fn get(&self) -> usize {
        self.0
    }

    /// Returns the minimum inclusive and maximum inclusive [`Distance`]
    /// included in the bucket for this index.
    fn _range(&self) -> (Distance, Distance) {
        let min = Distance(U256::pow(U256::from(2), U256::from(self.0)));
        if self.0 == usize::from(u8::MAX) {
            (min, Distance(U256::MAX))
        } else {
            let max = Distance(U256::pow(U256::from(2), U256::from(self.0 + 1)) - 1);
            (min, max)
        }
    }

    /// Generates a random distance that falls into the bucket for this index.
    #[cfg(test)]
    fn rand_distance(&self, rng: &mut impl rand::Rng) -> Distance {
        let mut bytes = [0u8; 32];
        let quot = self.0 / 8;
        for i in 0..quot {
            bytes[31 - i] = rng.gen();
        }
        let rem = (self.0 % 8) as u32;
        let lower = usize::pow(2, rem);
        let upper = usize::pow(2, rem + 1);
        bytes[31 - quot] = rng.gen_range(lower..upper) as u8;
        Distance(U256::from_big_endian(&bytes))
    }
}

impl RoutingTable {
    /// Create new [`RoutingTable`].
    pub fn new(local_key: Key<PeerId>) -> Self {
        RoutingTable {
            local_key,
            buckets: (0..NUM_BUCKETS).map(|_| KBucket::new()).collect(),
        }
    }

    /// Returns the local key.
    pub fn _local_key(&self) -> &Key<PeerId> {
        &self.local_key
    }

    /// Get an entry for `peer` into a k-bucket.
    pub fn entry(&mut self, key: Key<PeerId>) -> KBucketEntry<'_> {
        let Some(index) = BucketIndex::new(&self.local_key.distance(&key)) else {
            return KBucketEntry::LocalNode;
        };

        self.buckets[index.get()].entry(key)
    }

    /// Update the addresses of the peer on dial failures.
    ///
    /// The addresses are updated with a negative score making them subject to removal.
    pub fn on_dial_failure(&mut self, key: Key<PeerId>, addresses: &[Multiaddr]) {
        tracing::trace!(
            target: LOG_TARGET,
            ?key,
            ?addresses,
            "on dial failure"
        );

        if let KBucketEntry::Occupied(entry) = self.entry(key) {
            for address in addresses {
                entry.address_store.insert(AddressRecord::from_raw_multiaddr_with_score(
                    address.clone(),
                    scores::CONNECTION_FAILURE,
                ));
            }
        }
    }

    /// Update the status of the peer on connection established.
    ///
    /// If the peer exists in the routing table, the connection is set to `Connected`.
    /// If the endpoint represents an address we have dialed, the address score
    /// is updated in the store of the peer, making it more likely to be used in the future.
    pub fn on_connection_established(&mut self, key: Key<PeerId>, endpoint: Endpoint) {
        tracing::trace!(target: LOG_TARGET, ?key, ?endpoint, "on connection established");

        if let KBucketEntry::Occupied(entry) = self.entry(key) {
            entry.connection = ConnectionType::Connected;

            if let Endpoint::Dialer { address, .. } = endpoint {
                entry.address_store.insert(AddressRecord::from_raw_multiaddr_with_score(
                    address,
                    scores::CONNECTION_ESTABLISHED,
                ));
            }
        }
    }

    /// Add known peer to [`RoutingTable`].
    ///
    /// In order to bootstrap the lookup process, the routing table must be aware of
    /// at least one node and of its addresses.
    ///
    /// The operation is ignored when:
    ///  - the provided addresses are empty
    ///  - the local node is being added
    ///  - the routing table is full
    pub fn add_known_peer(
        &mut self,
        peer: PeerId,
        addresses: Vec<Multiaddr>,
        connection: ConnectionType,
    ) {
        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            ?addresses,
            ?connection,
            "add known peer"
        );

        // TODO: https://github.com/paritytech/litep2p/issues/337 this has to be moved elsewhere at some point
        let addresses: Vec<Multiaddr> = addresses
            .into_iter()
            .filter_map(|address| {
                let last = address.iter().last();
                if std::matches!(last, Some(Protocol::P2p(_))) {
                    Some(address)
                } else {
                    Some(address.with(Protocol::P2p(Multihash::from_bytes(&peer.to_bytes()).ok()?)))
                }
            })
            .collect();

        if addresses.is_empty() {
            tracing::debug!(
                target: LOG_TARGET,
                ?peer,
                "tried to add zero addresses to the routing table"
            );
            return;
        }

        match self.entry(Key::from(peer)) {
            KBucketEntry::Occupied(entry) => {
                entry.push_addresses(addresses);
                entry.connection = connection;
            }
            mut entry @ KBucketEntry::Vacant(_) => {
                entry.insert(KademliaPeer::new(peer, addresses, connection));
            }
            KBucketEntry::LocalNode => tracing::warn!(
                target: LOG_TARGET,
                ?peer,
                "tried to add local node to routing table",
            ),
            KBucketEntry::NoSlot => tracing::trace!(
                target: LOG_TARGET,
                ?peer,
                "routing table full, cannot add new entry",
            ),
        }
    }

    /// Get `limit` closest peers to `target` from the k-buckets.
    pub fn closest<K: Clone>(&mut self, target: &Key<K>, limit: usize) -> Vec<KademliaPeer> {
        ClosestBucketsIter::new(self.local_key.distance(&target))
            .flat_map(|index| self.buckets[index.get()].closest_iter(target))
            .take(limit)
            .cloned()
            .collect()
    }
}

/// An iterator over the bucket indices, in the order determined by the `Distance` of a target from
/// the `local_key`, such that the entries in the buckets are incrementally further away from the
/// target, starting with the bucket covering the target.
/// The original implementation is taken from `rust-libp2p`, see [issue#1117][1] for the explanation
/// of the algorithm used.
///
///  [1]: https://github.com/libp2p/rust-libp2p/pull/1117#issuecomment-494694635
struct ClosestBucketsIter {
    /// The distance to the `local_key`.
    distance: Distance,
    /// The current state of the iterator.
    state: ClosestBucketsIterState,
}

/// Operating states of a `ClosestBucketsIter`.
enum ClosestBucketsIterState {
    /// The starting state of the iterator yields the first bucket index and
    /// then transitions to `ZoomIn`.
    Start(BucketIndex),
    /// The iterator "zooms in" to to yield the next bucket cotaining nodes that
    /// are incrementally closer to the local node but further from the `target`.
    /// These buckets are identified by a `1` in the corresponding bit position
    /// of the distance bit string. When bucket `0` is reached, the iterator
    /// transitions to `ZoomOut`.
    ZoomIn(BucketIndex),
    /// Once bucket `0` has been reached, the iterator starts "zooming out"
    /// to buckets containing nodes that are incrementally further away from
    /// both the local key and the target. These are identified by a `0` in
    /// the corresponding bit position of the distance bit string. When bucket
    /// `255` is reached, the iterator transitions to state `Done`.
    ZoomOut(BucketIndex),
    /// The iterator is in this state once it has visited all buckets.
    Done,
}

impl ClosestBucketsIter {
    fn new(distance: Distance) -> Self {
        let state = match BucketIndex::new(&distance) {
            Some(i) => ClosestBucketsIterState::Start(i),
            None => ClosestBucketsIterState::Start(BucketIndex(0)),
        };
        Self { distance, state }
    }

    fn next_in(&self, i: BucketIndex) -> Option<BucketIndex> {
        (0..i.get())
            .rev()
            .find_map(|i| self.distance.0.bit(i).then_some(BucketIndex(i)))
    }

    fn next_out(&self, i: BucketIndex) -> Option<BucketIndex> {
        (i.get() + 1..NUM_BUCKETS).find_map(|i| (!self.distance.0.bit(i)).then_some(BucketIndex(i)))
    }
}

impl Iterator for ClosestBucketsIter {
    type Item = BucketIndex;

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            ClosestBucketsIterState::Start(i) => {
                self.state = ClosestBucketsIterState::ZoomIn(i);
                Some(i)
            }
            ClosestBucketsIterState::ZoomIn(i) =>
                if let Some(i) = self.next_in(i) {
                    self.state = ClosestBucketsIterState::ZoomIn(i);
                    Some(i)
                } else {
                    let i = BucketIndex(0);
                    self.state = ClosestBucketsIterState::ZoomOut(i);
                    Some(i)
                },
            ClosestBucketsIterState::ZoomOut(i) =>
                if let Some(i) = self.next_out(i) {
                    self.state = ClosestBucketsIterState::ZoomOut(i);
                    Some(i)
                } else {
                    self.state = ClosestBucketsIterState::Done;
                    None
                },
            ClosestBucketsIterState::Done => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::libp2p::kademlia::types::ConnectionType;

    #[test]
    fn closest_peers() {
        let own_peer_id = PeerId::random();
        let own_key = Key::from(own_peer_id);
        let mut table = RoutingTable::new(own_key.clone());

        for _ in 0..60 {
            let peer = PeerId::random();
            let key = Key::from(peer);
            let mut entry = table.entry(key.clone());
            entry.insert(KademliaPeer::new(peer, vec![], ConnectionType::Connected));
        }

        let target = Key::from(PeerId::random());
        let closest = table.closest(&target, 60usize);
        let mut prev = None;

        for peer in &closest {
            if let Some(value) = prev {
                assert!(value < target.distance(&peer.key));
            }

            prev = Some(target.distance(&peer.key));
        }
    }

    // generate random peer that falls in to specified k-bucket.
    //
    // NOTE: the preimage of the generated `Key` doesn't match the `Key` itself
    fn random_peer(
        rng: &mut impl rand::Rng,
        own_key: Key<PeerId>,
        bucket_index: usize,
    ) -> (Key<PeerId>, PeerId) {
        let peer = PeerId::random();
        let distance = BucketIndex(bucket_index).rand_distance(rng);
        let key_bytes = own_key.for_distance(distance);

        (Key::from_bytes(key_bytes, peer), peer)
    }

    #[test]
    fn add_peer_to_empty_table() {
        let own_peer_id = PeerId::random();
        let own_key = Key::from(own_peer_id);
        let mut table = RoutingTable::new(own_key.clone());

        // verify that local peer id resolves to special entry
        match table.entry(own_key.clone()) {
            KBucketEntry::LocalNode => {}
            state => panic!("invalid state for `KBucketEntry`: {state:?}"),
        };

        let peer = PeerId::random();
        let key = Key::from(peer);
        let mut test = table.entry(key.clone());
        let addresses = vec![];

        assert!(std::matches!(test, KBucketEntry::Vacant(_)));
        test.insert(KademliaPeer::new(
            peer,
            addresses.clone(),
            ConnectionType::Connected,
        ));

        match table.entry(key.clone()) {
            KBucketEntry::Occupied(entry) => {
                assert_eq!(entry.key, key);
                assert_eq!(entry.peer, peer);
                assert_eq!(entry.addresses(), addresses);
                assert_eq!(entry.connection, ConnectionType::Connected);
            }
            state => panic!("invalid state for `KBucketEntry`: {state:?}"),
        };

        // Set the connection state
        match table.entry(key.clone()) {
            KBucketEntry::Occupied(entry) => {
                entry.connection = ConnectionType::NotConnected;
            }
            state => panic!("invalid state for `KBucketEntry`: {state:?}"),
        }

        match table.entry(key.clone()) {
            KBucketEntry::Occupied(entry) => {
                assert_eq!(entry.key, key);
                assert_eq!(entry.peer, peer);
                assert_eq!(entry.addresses(), addresses);
                assert_eq!(entry.connection, ConnectionType::NotConnected);
            }
            state => panic!("invalid state for `KBucketEntry`: {state:?}"),
        };
    }

    #[test]
    fn full_k_bucket() {
        let mut rng = rand::thread_rng();
        let own_peer_id = PeerId::random();
        let own_key = Key::from(own_peer_id);
        let mut table = RoutingTable::new(own_key.clone());

        // add 20 nodes to the same k-bucket
        for _ in 0..20 {
            let (key, peer) = random_peer(&mut rng, own_key.clone(), 254);
            let mut entry = table.entry(key.clone());

            assert!(std::matches!(entry, KBucketEntry::Vacant(_)));
            entry.insert(KademliaPeer::new(peer, vec![], ConnectionType::Connected));
        }

        // try to add another peer and verify the peer is rejected
        // because the k-bucket is full of connected nodes
        let peer = PeerId::random();
        let distance = BucketIndex(254).rand_distance(&mut rng);
        let key_bytes = own_key.for_distance(distance);
        let key = Key::from_bytes(key_bytes, peer);

        let entry = table.entry(key.clone());
        assert!(std::matches!(entry, KBucketEntry::NoSlot));
    }

    #[test]
    #[ignore]
    fn peer_disconnects_and_is_evicted() {
        let mut rng = rand::thread_rng();
        let own_peer_id = PeerId::random();
        let own_key = Key::from(own_peer_id);
        let mut table = RoutingTable::new(own_key.clone());

        // add 20 nodes to the same k-bucket
        let peers = (0..20)
            .map(|_| {
                let (key, peer) = random_peer(&mut rng, own_key.clone(), 253);
                let mut entry = table.entry(key.clone());

                assert!(std::matches!(entry, KBucketEntry::Vacant(_)));
                entry.insert(KademliaPeer::new(peer, vec![], ConnectionType::Connected));

                (peer, key)
            })
            .collect::<Vec<_>>();

        // try to add another peer and verify the peer is rejected
        // because the k-bucket is full of connected nodes
        let peer = PeerId::random();
        let distance = BucketIndex(253).rand_distance(&mut rng);
        let key_bytes = own_key.for_distance(distance);
        let key = Key::from_bytes(key_bytes, peer);

        let entry = table.entry(key.clone());
        assert!(std::matches!(entry, KBucketEntry::NoSlot));

        // disconnect random peer
        match table.entry(peers[3].1.clone()) {
            KBucketEntry::Occupied(entry) => {
                entry.connection = ConnectionType::NotConnected;
            }
            _ => panic!("invalid state for node"),
        }

        // try to add the previously rejected peer again and verify it's added
        let mut entry = table.entry(key.clone());
        assert!(std::matches!(entry, KBucketEntry::Vacant(_)));
        entry.insert(KademliaPeer::new(
            peer,
            vec!["/ip6/::1/tcp/8888".parse().unwrap()],
            ConnectionType::CanConnect,
        ));

        // verify the node is still there
        let entry = table.entry(key.clone());
        let addresses = vec!["/ip6/::1/tcp/8888".parse().unwrap()];

        match entry {
            KBucketEntry::Occupied(entry) => {
                assert_eq!(entry.key, key);
                assert_eq!(entry.peer, peer);
                assert_eq!(entry.addresses(), addresses);
                assert_eq!(entry.connection, ConnectionType::CanConnect);
            }
            state => panic!("invalid state for `KBucketEntry`: {state:?}"),
        }
    }

    #[test]
    fn disconnected_peers_are_not_evicted_if_there_is_capacity() {
        let mut rng = rand::thread_rng();
        let own_peer_id = PeerId::random();
        let own_key = Key::from(own_peer_id);
        let mut table = RoutingTable::new(own_key.clone());

        // add 19 disconnected nodes to the same k-bucket
        let _peers = (0..19)
            .map(|_| {
                let (key, peer) = random_peer(&mut rng, own_key.clone(), 252);
                let mut entry = table.entry(key.clone());

                assert!(std::matches!(entry, KBucketEntry::Vacant(_)));
                entry.insert(KademliaPeer::new(
                    peer,
                    vec![],
                    ConnectionType::NotConnected,
                ));

                (peer, key)
            })
            .collect::<Vec<_>>();

        // try to add another peer and verify it's accepted as there is
        // still room in the k-bucket for the node
        let peer = PeerId::random();
        let distance = BucketIndex(252).rand_distance(&mut rng);
        let key_bytes = own_key.for_distance(distance);
        let key = Key::from_bytes(key_bytes, peer);

        let mut entry = table.entry(key.clone());
        assert!(std::matches!(entry, KBucketEntry::Vacant(_)));
        entry.insert(KademliaPeer::new(
            peer,
            vec!["/ip6/::1/tcp/8888".parse().unwrap()],
            ConnectionType::CanConnect,
        ));
    }

    #[test]
    fn closest_buckets_iterator_set_lsb() {
        // Test zooming-in & zooming-out of the iterator using a toy example with set LSB.
        let d = Distance(U256::from(0b10011011));
        let mut iter = ClosestBucketsIter::new(d);
        // Note that bucket 0 is visited twice. This is, technically, a bug, but to not complicate
        // the implementation and keep it consistent with `libp2p` it's kept as is. There are
        // virtually no practical consequences of this, because to have bucket 0 populated we have
        // to encounter two sha256 hash values differing only in one least significant bit.
        let expected_buckets =
            vec![7, 4, 3, 1, 0, 0, 2, 5, 6].into_iter().chain(8..=255).map(BucketIndex);
        for expected in expected_buckets {
            let got = iter.next().unwrap();
            assert_eq!(got, expected);
        }
        assert!(iter.next().is_none());
    }

    #[test]
    fn closest_buckets_iterator_unset_lsb() {
        // Test zooming-in & zooming-out of the iterator using a toy example with unset LSB.
        let d = Distance(U256::from(0b01011010));
        let mut iter = ClosestBucketsIter::new(d);
        let expected_buckets =
            vec![6, 4, 3, 1, 0, 2, 5, 7].into_iter().chain(8..=255).map(BucketIndex);
        for expected in expected_buckets {
            let got = iter.next().unwrap();
            assert_eq!(got, expected);
        }
        assert!(iter.next().is_none());
    }
}
