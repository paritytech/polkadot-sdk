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

use crate::{error::DialError, PeerId};

use multiaddr::{Multiaddr, Protocol};
use multihash::Multihash;

use std::collections::{hash_map::Entry, HashMap};

/// Maximum number of addresses tracked for a peer.
const MAX_ADDRESSES: usize = 64;

/// Scores for address records.
pub mod scores {
    /// Score indicating that the connection was successfully established.
    pub const CONNECTION_ESTABLISHED: i32 = 100i32;

    /// Score for failing to connect due to an invalid or unreachable address.
    pub const CONNECTION_FAILURE: i32 = -100i32;

    /// Score for providing an invalid address.
    ///
    /// This address can never be reached.
    pub const ADDRESS_FAILURE: i32 = i32::MIN;
}

#[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Debug, Clone, Hash)]
pub struct AddressRecord {
    /// Address score.
    score: i32,

    /// Address.
    address: Multiaddr,
}

impl AsRef<Multiaddr> for AddressRecord {
    fn as_ref(&self) -> &Multiaddr {
        &self.address
    }
}

impl AddressRecord {
    /// Create new `AddressRecord` and if `address` doesn't contain `P2p`,
    /// append the provided `PeerId` to the address.
    pub fn new(peer: &PeerId, address: Multiaddr, score: i32) -> Self {
        let address = if !std::matches!(address.iter().last(), Some(Protocol::P2p(_))) {
            address.with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).expect("valid peer id"),
            ))
        } else {
            address
        };

        Self { address, score }
    }

    /// Create `AddressRecord` from `Multiaddr`.
    ///
    /// If `address` doesn't contain `PeerId`, return `None` to indicate that this
    /// an invalid `Multiaddr` from the perspective of the `TransportManager`.
    pub fn from_multiaddr(address: Multiaddr) -> Option<AddressRecord> {
        if !std::matches!(address.iter().last(), Some(Protocol::P2p(_))) {
            return None;
        }

        Some(AddressRecord {
            address,
            score: 0i32,
        })
    }

    /// Create `AddressRecord` from `Multiaddr`.
    ///
    /// This method does not check if the address contains `PeerId`.
    ///
    /// Please consider using [`Self::from_multiaddr`] from the transport manager code.
    pub fn from_raw_multiaddr(address: Multiaddr) -> AddressRecord {
        AddressRecord {
            address,
            score: 0i32,
        }
    }

    /// Create `AddressRecord` from `Multiaddr`.
    ///
    /// This method does not check if the address contains `PeerId`.
    ///
    /// Please consider using [`Self::from_multiaddr`] from the transport manager code.
    pub fn from_raw_multiaddr_with_score(address: Multiaddr, score: i32) -> AddressRecord {
        AddressRecord { address, score }
    }

    /// Get address score.
    #[cfg(test)]
    pub fn score(&self) -> i32 {
        self.score
    }

    /// Get address.
    pub fn address(&self) -> &Multiaddr {
        &self.address
    }

    /// Update score of an address.
    pub fn update_score(&mut self, score: i32) {
        self.score = self.score.saturating_add(score);
    }
}

impl PartialEq for AddressRecord {
    fn eq(&self, other: &Self) -> bool {
        self.score.eq(&other.score)
    }
}

impl Eq for AddressRecord {}

impl PartialOrd for AddressRecord {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AddressRecord {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score.cmp(&other.score)
    }
}

/// Store for peer addresses.
#[derive(Debug, Clone, Default)]
pub struct AddressStore {
    /// Addresses available.
    pub addresses: HashMap<Multiaddr, AddressRecord>,
    /// Maximum capacity of the address store.
    max_capacity: usize,
}

impl FromIterator<Multiaddr> for AddressStore {
    fn from_iter<T: IntoIterator<Item = Multiaddr>>(iter: T) -> Self {
        let mut store = AddressStore::new();
        for address in iter {
            if let Some(record) = AddressRecord::from_multiaddr(address) {
                store.insert(record);
            }
        }

        store
    }
}

impl FromIterator<AddressRecord> for AddressStore {
    fn from_iter<T: IntoIterator<Item = AddressRecord>>(iter: T) -> Self {
        let mut store = AddressStore::new();
        for record in iter {
            store.insert(record);
        }

        store
    }
}

impl Extend<AddressRecord> for AddressStore {
    fn extend<T: IntoIterator<Item = AddressRecord>>(&mut self, iter: T) {
        for record in iter {
            self.insert(record)
        }
    }
}

impl<'a> Extend<&'a AddressRecord> for AddressStore {
    fn extend<T: IntoIterator<Item = &'a AddressRecord>>(&mut self, iter: T) {
        for record in iter {
            self.insert(record.clone())
        }
    }
}

impl AddressStore {
    /// Create new [`AddressStore`].
    pub fn new() -> Self {
        Self {
            addresses: HashMap::with_capacity(MAX_ADDRESSES),
            max_capacity: MAX_ADDRESSES,
        }
    }

    /// Get the score for a given error.
    pub fn error_score(error: &DialError) -> i32 {
        match error {
            DialError::AddressError(_) => scores::ADDRESS_FAILURE,
            _ => scores::CONNECTION_FAILURE,
        }
    }

    /// Check if [`AddressStore`] is empty.
    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }

    /// Insert the address record into [`AddressStore`] with the provided score.
    ///
    /// If the address is not in the store, it will be inserted.
    /// Otherwise, the score and connection ID will be updated.
    pub fn insert(&mut self, record: AddressRecord) {
        if let Entry::Occupied(mut occupied) = self.addresses.entry(record.address.clone()) {
            occupied.get_mut().update_score(record.score);
            return;
        }

        // The eviction algorithm favours addresses with higher scores.
        //
        // This algorithm has the following implications:
        //  - it keeps the best addresses in the store.
        //  - if the store is at capacity, the worst address will be evicted.
        //  - an address that is not dialed yet (with score zero) will be preferred over an address
        //  that already failed (with negative score).
        if self.addresses.len() >= self.max_capacity {
            let min_record = self
                .addresses
                .values()
                .min()
                .cloned()
                .expect("There is at least one element checked above; qed");

            // The lowest score is better than the new record.
            if record.score < min_record.score {
                return;
            }
            self.addresses.remove(min_record.address());
        }

        // Insert the record.
        self.addresses.insert(record.address.clone(), record);
    }

    /// Return the available addresses sorted by score.
    pub fn addresses(&self, limit: usize) -> Vec<Multiaddr> {
        let mut records = self.addresses.values().cloned().collect::<Vec<_>>();
        records.sort_by(|lhs, rhs| rhs.score.cmp(&lhs.score));
        records.into_iter().take(limit).map(|record| record.address).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        net::{Ipv4Addr, SocketAddrV4},
    };

    use super::*;
    use rand::{rngs::ThreadRng, Rng};

    fn tcp_address_record(rng: &mut ThreadRng) -> AddressRecord {
        let peer = PeerId::random();
        let address = std::net::SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(
                rng.gen_range(1..=255),
                rng.gen_range(0..=255),
                rng.gen_range(0..=255),
                rng.gen_range(0..=255),
            ),
            rng.gen_range(1..=65535),
        ));
        let score: i32 = rng.gen_range(10..=200);

        AddressRecord::new(
            &peer,
            Multiaddr::empty()
                .with(Protocol::from(address.ip()))
                .with(Protocol::Tcp(address.port())),
            score,
        )
    }

    fn ws_address_record(rng: &mut ThreadRng) -> AddressRecord {
        let peer = PeerId::random();
        let address = std::net::SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(
                rng.gen_range(1..=255),
                rng.gen_range(0..=255),
                rng.gen_range(0..=255),
                rng.gen_range(0..=255),
            ),
            rng.gen_range(1..=65535),
        ));
        let score: i32 = rng.gen_range(10..=200);

        AddressRecord::new(
            &peer,
            Multiaddr::empty()
                .with(Protocol::from(address.ip()))
                .with(Protocol::Tcp(address.port()))
                .with(Protocol::Ws(std::borrow::Cow::Owned("/".to_string()))),
            score,
        )
    }

    fn quic_address_record(rng: &mut ThreadRng) -> AddressRecord {
        let peer = PeerId::random();
        let address = std::net::SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(
                rng.gen_range(1..=255),
                rng.gen_range(0..=255),
                rng.gen_range(0..=255),
                rng.gen_range(0..=255),
            ),
            rng.gen_range(1..=65535),
        ));
        let score: i32 = rng.gen_range(10..=200);

        AddressRecord::new(
            &peer,
            Multiaddr::empty()
                .with(Protocol::from(address.ip()))
                .with(Protocol::Udp(address.port()))
                .with(Protocol::QuicV1),
            score,
        )
    }

    #[test]
    fn take_multiple_records() {
        let mut store = AddressStore::new();
        let mut rng = rand::thread_rng();

        for _ in 0..rng.gen_range(1..5) {
            store.insert(tcp_address_record(&mut rng));
        }
        for _ in 0..rng.gen_range(1..5) {
            store.insert(ws_address_record(&mut rng));
        }
        for _ in 0..rng.gen_range(1..5) {
            store.insert(quic_address_record(&mut rng));
        }

        let known_addresses = store.addresses.len();
        assert!(known_addresses >= 3);

        let taken = store.addresses(known_addresses - 2);
        assert_eq!(known_addresses - 2, taken.len());
        assert!(!store.is_empty());

        let mut prev: Option<AddressRecord> = None;
        for address in taken {
            // Addresses are still in the store.
            assert!(store.addresses.contains_key(&address));

            let record = store.addresses.get(&address).unwrap().clone();

            if let Some(previous) = prev {
                assert!(previous.score >= record.score);
            }

            prev = Some(record);
        }
    }

    #[test]
    fn attempt_to_take_excess_records() {
        let mut store = AddressStore::new();
        let mut rng = rand::thread_rng();

        store.insert(tcp_address_record(&mut rng));
        store.insert(ws_address_record(&mut rng));
        store.insert(quic_address_record(&mut rng));

        assert_eq!(store.addresses.len(), 3);

        let taken = store.addresses(8usize);
        assert_eq!(taken.len(), 3);

        let mut prev: Option<AddressRecord> = None;
        for record in taken {
            let record = store.addresses.get(&record).unwrap().clone();

            if prev.is_none() {
                prev = Some(record);
            } else {
                assert!(prev.unwrap().score >= record.score);
                prev = Some(record);
            }
        }
    }

    #[test]
    fn extend_from_iterator() {
        let mut store = AddressStore::new();
        let mut rng = rand::thread_rng();

        let records = (0..10)
            .map(|i| {
                if i % 2 == 0 {
                    tcp_address_record(&mut rng)
                } else if i % 3 == 0 {
                    quic_address_record(&mut rng)
                } else {
                    ws_address_record(&mut rng)
                }
            })
            .collect::<Vec<_>>();

        assert!(store.is_empty());
        let cloned = records
            .iter()
            .cloned()
            .map(|record| (record.address().clone(), record))
            .collect::<HashMap<_, _>>();
        store.extend(records);

        for record in store.addresses.values() {
            let stored = cloned.get(record.address()).unwrap();
            assert_eq!(stored.score(), record.score());
            assert_eq!(stored.address(), record.address());
        }
    }

    #[test]
    fn extend_from_iterator_ref() {
        let mut store = AddressStore::new();
        let mut rng = rand::thread_rng();

        let records = (0..10)
            .map(|i| {
                if i % 2 == 0 {
                    let record = tcp_address_record(&mut rng);
                    (record.address().clone(), record)
                } else if i % 3 == 0 {
                    let record = quic_address_record(&mut rng);
                    (record.address().clone(), record)
                } else {
                    let record = ws_address_record(&mut rng);
                    (record.address().clone(), record)
                }
            })
            .collect::<Vec<_>>();

        assert!(store.is_empty());
        let cloned = records.iter().cloned().collect::<HashMap<_, _>>();
        store.extend(records.iter().map(|(_, record)| record));

        for record in store.addresses.values() {
            let stored = cloned.get(record.address()).unwrap();
            assert_eq!(stored.score(), record.score());
            assert_eq!(stored.address(), record.address());
        }
    }

    #[test]
    fn insert_record() {
        let mut store = AddressStore::new();
        let mut rng = rand::thread_rng();

        let mut record = tcp_address_record(&mut rng);
        record.score = 10;

        store.insert(record.clone());

        assert_eq!(store.addresses.len(), 1);
        assert_eq!(store.addresses.get(record.address()).unwrap(), &record);

        // This time the record is updated.
        store.insert(record.clone());

        assert_eq!(store.addresses.len(), 1);
        let store_record = store.addresses.get(record.address()).unwrap();
        assert_eq!(store_record.score, record.score * 2);
    }

    #[test]
    fn evict_on_capacity() {
        let mut store = AddressStore {
            addresses: HashMap::new(),
            max_capacity: 2,
        };

        let mut rng = rand::thread_rng();
        let mut first_record = tcp_address_record(&mut rng);
        first_record.score = scores::CONNECTION_ESTABLISHED;
        let mut second_record = ws_address_record(&mut rng);
        second_record.score = 0;

        store.insert(first_record.clone());
        store.insert(second_record.clone());

        assert_eq!(store.addresses.len(), 2);

        // We have better addresses, ignore this one.
        let mut third_record = quic_address_record(&mut rng);
        third_record.score = scores::CONNECTION_FAILURE;
        store.insert(third_record.clone());
        assert_eq!(store.addresses.len(), 2);
        assert!(store.addresses.contains_key(first_record.address()));
        assert!(store.addresses.contains_key(second_record.address()));

        // Evict the address with the lowest score.
        // Store contains scores: [100, 0].
        let mut fourth_record = quic_address_record(&mut rng);
        fourth_record.score = 1;
        store.insert(fourth_record.clone());

        assert_eq!(store.addresses.len(), 2);
        assert!(store.addresses.contains_key(first_record.address()));
        assert!(store.addresses.contains_key(fourth_record.address()));
    }
}
