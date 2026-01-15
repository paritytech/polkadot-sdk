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

//! Memory store implementation for Kademlia.

use crate::{
    protocol::libp2p::kademlia::{
        config::{
            DEFAULT_MAX_PROVIDERS_PER_KEY, DEFAULT_MAX_PROVIDER_ADDRESSES,
            DEFAULT_MAX_PROVIDER_KEYS, DEFAULT_MAX_RECORDS, DEFAULT_MAX_RECORD_SIZE_BYTES,
            DEFAULT_PROVIDER_REFRESH_INTERVAL, DEFAULT_PROVIDER_TTL,
        },
        record::{ContentProvider, Key, ProviderRecord, Record},
        types::Key as KademliaKey,
        Quorum,
    },
    utils::futures_stream::FuturesStream,
    PeerId,
};

use futures::{future::BoxFuture, StreamExt};
use std::{
    collections::{hash_map::Entry, HashMap},
    time::Duration,
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::ipfs::kademlia::store";

/// Memory store events.
#[derive(Debug, PartialEq, Eq)]
pub enum MemoryStoreAction {
    RefreshProvider {
        provided_key: Key,
        provider: ContentProvider,
        quorum: Quorum,
    },
}

/// Memory store.
pub struct MemoryStore {
    /// Local peer ID. Used to track local providers.
    local_peer_id: PeerId,
    /// Configuration.
    config: MemoryStoreConfig,
    /// Records.
    records: HashMap<Key, Record>,
    /// Provider records.
    provider_keys: HashMap<Key, Vec<ProviderRecord>>,
    /// Local providers.
    local_providers: HashMap<Key, (ContentProvider, Quorum)>,
    /// Futures to signal it's time to republish a local provider.
    pending_provider_refresh: FuturesStream<BoxFuture<'static, Key>>,
}

impl MemoryStore {
    /// Create new [`MemoryStore`].
    #[cfg(test)]
    pub fn new(local_peer_id: PeerId) -> Self {
        Self {
            local_peer_id,
            config: MemoryStoreConfig::default(),
            records: HashMap::new(),
            provider_keys: HashMap::new(),
            local_providers: HashMap::new(),
            pending_provider_refresh: FuturesStream::new(),
        }
    }

    /// Create new [`MemoryStore`] with the provided configuration.
    pub fn with_config(local_peer_id: PeerId, config: MemoryStoreConfig) -> Self {
        Self {
            local_peer_id,
            config,
            records: HashMap::new(),
            provider_keys: HashMap::new(),
            local_providers: HashMap::new(),
            pending_provider_refresh: FuturesStream::new(),
        }
    }

    /// Try to get record from local store for `key`.
    pub fn get(&mut self, key: &Key) -> Option<&Record> {
        let is_expired = self
            .records
            .get(key)
            .is_some_and(|record| record.is_expired(std::time::Instant::now()));

        if is_expired {
            self.records.remove(key);
            None
        } else {
            self.records.get(key)
        }
    }

    /// Store record.
    pub fn put(&mut self, record: Record) {
        if record.value.len() >= self.config.max_record_size_bytes {
            tracing::warn!(
                target: LOG_TARGET,
                key = ?record.key,
                publisher = ?record.publisher,
                size = record.value.len(),
                max_size = self.config.max_record_size_bytes,
                "discarding a DHT record that exceeds the configured size limit",
            );
            return;
        }

        let len = self.records.len();
        match self.records.entry(record.key.clone()) {
            Entry::Occupied(mut entry) => {
                // Lean towards the new record.
                if let (Some(stored_record_ttl), Some(new_record_ttl)) =
                    (entry.get().expires, record.expires)
                {
                    if stored_record_ttl > new_record_ttl {
                        return;
                    }
                }

                entry.insert(record);
            }

            Entry::Vacant(entry) => {
                if len >= self.config.max_records {
                    tracing::warn!(
                        target: LOG_TARGET,
                        max_records = self.config.max_records,
                        "discarding a DHT record, because maximum memory store size reached",
                    );
                    return;
                }

                entry.insert(record);
            }
        }
    }

    /// Try to get providers from local store for `key`.
    ///
    /// Returns a non-empty list of providers, if any.
    pub fn get_providers(&mut self, key: &Key) -> Vec<ContentProvider> {
        let drop_key = self.provider_keys.get_mut(key).is_some_and(|providers| {
            let now = std::time::Instant::now();
            providers.retain(|p| !p.is_expired(now));

            providers.is_empty()
        });

        if drop_key {
            self.provider_keys.remove(key);

            Vec::default()
        } else {
            self.provider_keys
                .get(key)
                .cloned()
                .unwrap_or_else(Vec::default)
                .into_iter()
                .map(|p| ContentProvider {
                    peer: p.provider,
                    addresses: p.addresses,
                })
                .collect()
        }
    }

    /// Try to add a provider for `key`. If there are already `max_providers_per_key` for
    /// this `key`, the new provider is only inserted if its closer to `key` than
    /// the furthest already inserted provider. The furthest provider is then discarded.
    ///
    /// Returns `true` if the provider was added, `false` otherwise.
    ///
    /// `quorum` is only relevant for local providers.
    pub fn put_provider(&mut self, key: Key, provider: ContentProvider) -> bool {
        // Make sure we have no more than `max_provider_addresses`.
        let provider_record = {
            let mut record = ProviderRecord {
                key,
                provider: provider.peer,
                addresses: provider.addresses,
                expires: std::time::Instant::now() + self.config.provider_ttl,
            };
            record.addresses.truncate(self.config.max_provider_addresses);
            record
        };

        let can_insert_new_key = self.provider_keys.len() < self.config.max_provider_keys;

        match self.provider_keys.entry(provider_record.key.clone()) {
            Entry::Vacant(entry) =>
                if can_insert_new_key {
                    entry.insert(vec![provider_record]);

                    true
                } else {
                    tracing::warn!(
                        target: LOG_TARGET,
                        max_provider_keys = self.config.max_provider_keys,
                        "discarding a provider record, because the provider key limit reached",
                    );

                    false
                },
            Entry::Occupied(mut entry) => {
                let providers = entry.get_mut();

                // Providers under every key are sorted by distance from the provided key, with
                // equal distances meaning peer IDs (more strictly, their hashes)
                // are equal.
                let provider_position =
                    providers.binary_search_by(|p| p.distance().cmp(&provider_record.distance()));

                match provider_position {
                    Ok(i) => {
                        // Update the provider in place.
                        providers[i] = provider_record.clone();

                        true
                    }
                    Err(i) => {
                        // `Err(i)` contains the insertion point.
                        if i == self.config.max_providers_per_key {
                            tracing::trace!(
                                target: LOG_TARGET,
                                key = ?provider_record.key,
                                provider = ?provider_record.provider,
                                max_providers_per_key = self.config.max_providers_per_key,
                                "discarding a provider record, because it's further than \
                                 existing `max_providers_per_key`",
                            );

                            false
                        } else {
                            if providers.len() == self.config.max_providers_per_key {
                                providers.pop();
                            }

                            providers.insert(i, provider_record.clone());

                            true
                        }
                    }
                }
            }
        }
    }

    /// Try to add ourself as a provider for `key`.
    ///
    /// Returns `true` if the provider was added, `false` otherwise.
    pub fn put_local_provider(&mut self, key: Key, quorum: Quorum) -> bool {
        let provider = ContentProvider {
            peer: self.local_peer_id,
            // For local providers addresses are populated when replying to `GET_PROVIDERS`
            // requests.
            addresses: vec![],
        };

        if self.put_provider(key.clone(), provider.clone()) {
            let refresh_interval = self.config.provider_refresh_interval;
            self.local_providers.insert(key.clone(), (provider, quorum));
            self.pending_provider_refresh.push(Box::pin(async move {
                tokio::time::sleep(refresh_interval).await;
                key
            }));

            true
        } else {
            false
        }
    }

    /// Remove local provider for `key`.
    pub fn remove_local_provider(&mut self, key: Key) {
        if self.local_providers.remove(&key).is_none() {
            tracing::warn!(?key, "trying to remove nonexistent local provider",);
            return;
        };

        match self.provider_keys.entry(key.clone()) {
            Entry::Vacant(_) => {
                tracing::error!(?key, "local provider key not found during removal",);
                debug_assert!(false);
            }
            Entry::Occupied(mut entry) => {
                let providers = entry.get_mut();

                // Providers are sorted by distance.
                let local_provider_distance =
                    KademliaKey::from(self.local_peer_id).distance(&KademliaKey::new(key.clone()));
                let provider_position =
                    providers.binary_search_by(|p| p.distance().cmp(&local_provider_distance));

                match provider_position {
                    Ok(i) => {
                        providers.remove(i);
                    }
                    Err(_) => {
                        tracing::error!(?key, "local provider not found during removal",);
                        debug_assert!(false);
                        return;
                    }
                }

                if providers.is_empty() {
                    entry.remove();
                }
            }
        };
    }

    /// Poll next action from the store.
    pub async fn next_action(&mut self) -> Option<MemoryStoreAction> {
        // [`FuturesStream`] never terminates, so `and_then()` below is always triggered.
        self.pending_provider_refresh.next().await.and_then(|key| {
            if let Some((provider, quorum)) = self.local_providers.get(&key).cloned() {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?key,
                    "refresh provider"
                );

                Some(MemoryStoreAction::RefreshProvider {
                    provided_key: key,
                    provider,
                    quorum,
                })
            } else {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?key,
                    "it's time to refresh a provider, but we do not provide this key anymore",
                );

                None
            }
        })
    }
}

#[derive(Debug)]
pub struct MemoryStoreConfig {
    /// Maximum number of records to store.
    pub max_records: usize,

    /// Maximum size of a record in bytes.
    pub max_record_size_bytes: usize,

    /// Maximum number of provider keys this node stores.
    pub max_provider_keys: usize,

    /// Maximum number of cached addresses per provider.
    pub max_provider_addresses: usize,

    /// Maximum number of providers per key. Only providers with peer IDs closest to the key are
    /// kept.
    pub max_providers_per_key: usize,

    /// Local providers republish interval.
    pub provider_refresh_interval: Duration,

    /// Provider record TTL.
    pub provider_ttl: Duration,
}

impl Default for MemoryStoreConfig {
    fn default() -> Self {
        Self {
            max_records: DEFAULT_MAX_RECORDS,
            max_record_size_bytes: DEFAULT_MAX_RECORD_SIZE_BYTES,
            max_provider_keys: DEFAULT_MAX_PROVIDER_KEYS,
            max_provider_addresses: DEFAULT_MAX_PROVIDER_ADDRESSES,
            max_providers_per_key: DEFAULT_MAX_PROVIDERS_PER_KEY,
            provider_refresh_interval: DEFAULT_PROVIDER_REFRESH_INTERVAL,
            provider_ttl: DEFAULT_PROVIDER_TTL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{protocol::libp2p::kademlia::types::Key as KademliaKey, PeerId};
    use multiaddr::multiaddr;

    #[test]
    fn put_get_record() {
        let mut store = MemoryStore::new(PeerId::random());
        let key = Key::from(vec![1, 2, 3]);
        let record = Record::new(key.clone(), vec![4, 5, 6]);

        store.put(record.clone());
        assert_eq!(store.get(&key), Some(&record));
    }

    #[test]
    fn max_records() {
        let mut store = MemoryStore::with_config(
            PeerId::random(),
            MemoryStoreConfig {
                max_records: 1,
                max_record_size_bytes: 1024,
                ..Default::default()
            },
        );

        let key1 = Key::from(vec![1, 2, 3]);
        let key2 = Key::from(vec![4, 5, 6]);
        let record1 = Record::new(key1.clone(), vec![4, 5, 6]);
        let record2 = Record::new(key2.clone(), vec![7, 8, 9]);

        store.put(record1.clone());
        store.put(record2.clone());

        assert_eq!(store.get(&key1), Some(&record1));
        assert_eq!(store.get(&key2), None);
    }

    #[test]
    fn expired_record_removed() {
        let mut store = MemoryStore::new(PeerId::random());
        let key = Key::from(vec![1, 2, 3]);
        let record = Record {
            key: key.clone(),
            value: vec![4, 5, 6],
            publisher: None,
            expires: Some(std::time::Instant::now() - std::time::Duration::from_secs(5)),
        };
        // Record is already expired.
        assert!(record.is_expired(std::time::Instant::now()));

        store.put(record.clone());
        assert_eq!(store.get(&key), None);
    }

    #[test]
    fn new_record_overwrites() {
        let mut store = MemoryStore::new(PeerId::random());
        let key = Key::from(vec![1, 2, 3]);
        let record1 = Record {
            key: key.clone(),
            value: vec![4, 5, 6],
            publisher: None,
            expires: Some(std::time::Instant::now() + std::time::Duration::from_secs(100)),
        };
        let record2 = Record {
            key: key.clone(),
            value: vec![4, 5, 6],
            publisher: None,
            expires: Some(std::time::Instant::now() + std::time::Duration::from_secs(1000)),
        };

        store.put(record1.clone());
        assert_eq!(store.get(&key), Some(&record1));

        store.put(record2.clone());
        assert_eq!(store.get(&key), Some(&record2));
    }

    #[test]
    fn max_record_size() {
        let mut store = MemoryStore::with_config(
            PeerId::random(),
            MemoryStoreConfig {
                max_records: 1024,
                max_record_size_bytes: 2,
                ..Default::default()
            },
        );

        let key = Key::from(vec![1, 2, 3]);
        let record = Record::new(key.clone(), vec![4, 5]);
        store.put(record.clone());
        assert_eq!(store.get(&key), None);

        let record = Record::new(key.clone(), vec![4]);
        store.put(record.clone());
        assert_eq!(store.get(&key), Some(&record));
    }

    #[test]
    fn put_get_provider() {
        let mut store = MemoryStore::new(PeerId::random());
        let key = Key::from(vec![1, 2, 3]);
        let provider = ContentProvider {
            peer: PeerId::random(),
            addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
        };

        store.put_provider(key.clone(), provider.clone());
        assert_eq!(store.get_providers(&key), vec![provider]);
    }

    #[test]
    fn multiple_providers_per_key() {
        let mut store = MemoryStore::new(PeerId::random());
        let key = Key::from(vec![1, 2, 3]);
        let provider1 = ContentProvider {
            peer: PeerId::random(),
            addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
        };
        let provider2 = ContentProvider {
            peer: PeerId::random(),
            addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
        };

        store.put_provider(key.clone(), provider1.clone());
        store.put_provider(key.clone(), provider2.clone());

        let got_providers = store.get_providers(&key);
        assert_eq!(got_providers.len(), 2);
        assert!(got_providers.contains(&provider1));
        assert!(got_providers.contains(&provider2));
    }

    #[test]
    fn providers_sorted_by_distance() {
        let mut store = MemoryStore::new(PeerId::random());
        let key = Key::from(vec![1, 2, 3]);
        let providers = (0..10)
            .map(|_| ContentProvider {
                peer: PeerId::random(),
                addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
            })
            .collect::<Vec<_>>();

        providers.iter().for_each(|p| {
            store.put_provider(key.clone(), p.clone());
        });

        let sorted_providers = {
            let target = KademliaKey::new(key.clone());
            let mut providers = providers;
            providers.sort_by(|p1, p2| {
                KademliaKey::from(p1.peer)
                    .distance(&target)
                    .cmp(&KademliaKey::from(p2.peer).distance(&target))
            });
            providers
        };

        assert_eq!(store.get_providers(&key), sorted_providers);
    }

    #[test]
    fn max_providers_per_key() {
        let mut store = MemoryStore::with_config(
            PeerId::random(),
            MemoryStoreConfig {
                max_providers_per_key: 10,
                ..Default::default()
            },
        );
        let key = Key::from(vec![1, 2, 3]);
        let providers = (0..20)
            .map(|_| ContentProvider {
                peer: PeerId::random(),
                addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
            })
            .collect::<Vec<_>>();

        providers.iter().for_each(|p| {
            store.put_provider(key.clone(), p.clone());
        });
        assert_eq!(store.get_providers(&key).len(), 10);
    }

    #[test]
    fn closest_providers_kept() {
        let mut store = MemoryStore::with_config(
            PeerId::random(),
            MemoryStoreConfig {
                max_providers_per_key: 10,
                ..Default::default()
            },
        );
        let key = Key::from(vec![1, 2, 3]);
        let providers = (0..20)
            .map(|_| ContentProvider {
                peer: PeerId::random(),
                addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
            })
            .collect::<Vec<_>>();

        providers.iter().for_each(|p| {
            store.put_provider(key.clone(), p.clone());
        });

        let closest_providers = {
            let target = KademliaKey::new(key.clone());
            let mut providers = providers;
            providers.sort_by(|p1, p2| {
                KademliaKey::from(p1.peer)
                    .distance(&target)
                    .cmp(&KademliaKey::from(p2.peer).distance(&target))
            });
            providers.truncate(10);
            providers
        };

        assert_eq!(store.get_providers(&key), closest_providers);
    }

    #[test]
    fn furthest_provider_discarded() {
        let mut store = MemoryStore::with_config(
            PeerId::random(),
            MemoryStoreConfig {
                max_providers_per_key: 10,
                ..Default::default()
            },
        );
        let key = Key::from(vec![1, 2, 3]);
        let providers = (0..11)
            .map(|_| ContentProvider {
                peer: PeerId::random(),
                addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
            })
            .collect::<Vec<_>>();

        let sorted_providers = {
            let target = KademliaKey::new(key.clone());
            let mut providers = providers;
            providers.sort_by(|p1, p2| {
                KademliaKey::from(p1.peer)
                    .distance(&target)
                    .cmp(&KademliaKey::from(p2.peer).distance(&target))
            });
            providers
        };

        // First 10 providers are inserted.
        for i in 0..10 {
            assert!(store.put_provider(key.clone(), sorted_providers[i].clone()));
        }
        assert_eq!(store.get_providers(&key), sorted_providers[..10]);

        // The furthests provider doesn't fit.
        assert!(!store.put_provider(key.clone(), sorted_providers[10].clone()));
        assert_eq!(store.get_providers(&key), sorted_providers[..10]);
    }

    #[test]
    fn update_provider_in_place() {
        let mut store = MemoryStore::with_config(
            PeerId::random(),
            MemoryStoreConfig {
                max_providers_per_key: 10,
                ..Default::default()
            },
        );
        let key = Key::from(vec![1, 2, 3]);
        let peer_ids = (0..10).map(|_| PeerId::random()).collect::<Vec<_>>();
        let peer_id0 = peer_ids[0];
        let providers = peer_ids
            .iter()
            .map(|peer_id| ContentProvider {
                peer: *peer_id,
                addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
            })
            .collect::<Vec<_>>();

        providers.iter().for_each(|p| {
            store.put_provider(key.clone(), p.clone());
        });

        let sorted_providers = {
            let target = KademliaKey::new(key.clone());
            let mut providers = providers;
            providers.sort_by(|p1, p2| {
                KademliaKey::from(p1.peer)
                    .distance(&target)
                    .cmp(&KademliaKey::from(p2.peer).distance(&target))
            });
            providers
        };

        assert_eq!(store.get_providers(&key), sorted_providers);

        let provider0_new = ContentProvider {
            peer: peer_id0,
            addresses: vec![multiaddr!(Ip4([192, 168, 0, 1]), Tcp(20000u16))],
        };

        // Provider is updated in place.
        assert!(store.put_provider(key.clone(), provider0_new.clone()));

        let providers_new = sorted_providers
            .into_iter()
            .map(|p| {
                if p.peer == peer_id0 {
                    provider0_new.clone()
                } else {
                    p
                }
            })
            .collect::<Vec<_>>();

        assert_eq!(store.get_providers(&key), providers_new);
    }

    #[tokio::test]
    async fn provider_record_expires() {
        let mut store = MemoryStore::with_config(
            PeerId::random(),
            MemoryStoreConfig {
                provider_ttl: std::time::Duration::from_secs(1),
                ..Default::default()
            },
        );
        let key = Key::from(vec![1, 2, 3]);
        let provider = ContentProvider {
            peer: PeerId::random(),
            addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
        };

        store.put_provider(key.clone(), provider.clone());

        // Provider does not instantly expire.
        assert_eq!(store.get_providers(&key), vec![provider]);

        // Provider expires after 2 seconds.
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(store.get_providers(&key), vec![]);
    }

    #[tokio::test]
    async fn individual_provider_record_expires() {
        let mut store = MemoryStore::with_config(
            PeerId::random(),
            MemoryStoreConfig {
                provider_ttl: std::time::Duration::from_secs(8),
                ..Default::default()
            },
        );
        let key = Key::from(vec![1, 2, 3]);
        let provider1 = ContentProvider {
            peer: PeerId::random(),
            addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
        };
        let provider2 = ContentProvider {
            peer: PeerId::random(),
            addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16))],
        };

        store.put_provider(key.clone(), provider1.clone());
        tokio::time::sleep(Duration::from_secs(4)).await;
        store.put_provider(key.clone(), provider2.clone());

        // Providers do not instantly expire.
        let got_providers = store.get_providers(&key);
        assert_eq!(got_providers.len(), 2);
        assert!(got_providers.contains(&provider1));
        assert!(got_providers.contains(&provider2));

        // First provider expires.
        tokio::time::sleep(Duration::from_secs(6)).await;
        assert_eq!(store.get_providers(&key), vec![provider2]);

        // Second provider expires.
        tokio::time::sleep(Duration::from_secs(4)).await;
        assert_eq!(store.get_providers(&key), vec![]);
    }

    #[test]
    fn max_addresses_per_provider() {
        let mut store = MemoryStore::with_config(
            PeerId::random(),
            MemoryStoreConfig {
                max_provider_addresses: 2,
                ..Default::default()
            },
        );
        let key = Key::from(vec![1, 2, 3]);
        let provider = ContentProvider {
            peer: PeerId::random(),
            addresses: vec![
                multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16)),
                multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10001u16)),
                multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10002u16)),
                multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10003u16)),
                multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10004u16)),
            ],
        };

        store.put_provider(key.clone(), provider);

        let got_providers = store.get_providers(&key);
        assert_eq!(got_providers.len(), 1);
        assert_eq!(got_providers.first().unwrap().addresses.len(), 2);
    }

    #[test]
    fn max_provider_keys() {
        let mut store = MemoryStore::with_config(
            PeerId::random(),
            MemoryStoreConfig {
                max_provider_keys: 2,
                ..Default::default()
            },
        );

        let key1 = Key::from(vec![1, 1, 1]);
        let provider1 = ContentProvider {
            peer: PeerId::random(),
            addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10001u16))],
        };
        let key2 = Key::from(vec![2, 2, 2]);
        let provider2 = ContentProvider {
            peer: PeerId::random(),
            addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10002u16))],
        };
        let key3 = Key::from(vec![3, 3, 3]);
        let provider3 = ContentProvider {
            peer: PeerId::random(),
            addresses: vec![multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10003u16))],
        };

        assert!(store.put_provider(key1.clone(), provider1.clone()));
        assert!(store.put_provider(key2.clone(), provider2.clone()));
        assert!(!store.put_provider(key3.clone(), provider3.clone()));

        assert_eq!(store.get_providers(&key1), vec![provider1]);
        assert_eq!(store.get_providers(&key2), vec![provider2]);
        assert_eq!(store.get_providers(&key3), vec![]);
    }

    #[test]
    fn local_provider_registered() {
        let local_peer_id = PeerId::random();
        let mut store = MemoryStore::new(local_peer_id);

        let key = Key::from(vec![1, 2, 3]);
        let local_provider = ContentProvider {
            peer: local_peer_id,
            addresses: vec![],
        };
        let quorum = Quorum::All;

        assert!(store.local_providers.is_empty());
        assert_eq!(store.pending_provider_refresh.len(), 0);

        assert!(store.put_local_provider(key.clone(), quorum));

        assert_eq!(
            store.local_providers.get(&key),
            Some(&(local_provider, quorum)),
        );
        assert_eq!(store.pending_provider_refresh.len(), 1);
    }

    #[test]
    fn local_provider_registered_after_remote_provider() {
        let local_peer_id = PeerId::random();
        let mut store = MemoryStore::new(local_peer_id);

        let key = Key::from(vec![1, 2, 3]);

        let remote_peer_id = PeerId::random();
        let remote_provider = ContentProvider {
            peer: remote_peer_id,
            addresses: vec![multiaddr!(Ip4([192, 168, 0, 1]), Tcp(10000u16))],
        };

        let local_provider = ContentProvider {
            peer: local_peer_id,
            addresses: vec![],
        };
        let quorum = Quorum::N(5.try_into().unwrap());

        assert!(store.local_providers.is_empty());
        assert_eq!(store.pending_provider_refresh.len(), 0);

        assert!(store.put_provider(key.clone(), remote_provider.clone()));
        assert!(store.put_local_provider(key.clone(), quorum));

        let got_providers = store.get_providers(&key);
        assert_eq!(got_providers.len(), 2);
        assert!(got_providers.contains(&remote_provider));
        assert!(got_providers.contains(&local_provider));

        assert_eq!(
            store.local_providers.get(&key),
            Some(&(local_provider, quorum))
        );
        assert_eq!(store.pending_provider_refresh.len(), 1);
    }

    #[test]
    fn local_provider_removed() {
        let local_peer_id = PeerId::random();
        let mut store = MemoryStore::new(local_peer_id);

        let key = Key::from(vec![1, 2, 3]);
        let local_provider = ContentProvider {
            peer: local_peer_id,
            addresses: vec![],
        };
        let quorum = Quorum::One;

        assert!(store.local_providers.is_empty());

        assert!(store.put_local_provider(key.clone(), quorum));

        assert_eq!(
            store.local_providers.get(&key),
            Some(&(local_provider, quorum))
        );

        store.remove_local_provider(key.clone());

        assert!(store.get_providers(&key).is_empty());
        assert!(store.local_providers.is_empty());
    }

    #[test]
    fn local_provider_removed_when_remote_providers_present() {
        let local_peer_id = PeerId::random();
        let mut store = MemoryStore::new(local_peer_id);

        let key = Key::from(vec![1, 2, 3]);

        let remote_peer_id = PeerId::random();
        let remote_provider = ContentProvider {
            peer: remote_peer_id,
            addresses: vec![multiaddr!(Ip4([192, 168, 0, 1]), Tcp(10000u16))],
        };

        let local_provider = ContentProvider {
            peer: local_peer_id,
            addresses: vec![],
        };
        let quorum = Quorum::One;

        assert!(store.put_provider(key.clone(), remote_provider.clone()));
        assert!(store.put_local_provider(key.clone(), quorum));

        let got_providers = store.get_providers(&key);
        assert_eq!(got_providers.len(), 2);
        assert!(got_providers.contains(&remote_provider));
        assert!(got_providers.contains(&local_provider));

        assert_eq!(
            store.local_providers.get(&key),
            Some(&(local_provider, quorum))
        );

        store.remove_local_provider(key.clone());

        assert_eq!(store.get_providers(&key), vec![remote_provider]);
        assert!(store.local_providers.is_empty());
    }

    #[tokio::test]
    async fn local_provider_refresh() {
        let local_peer_id = PeerId::random();
        let mut store = MemoryStore::with_config(
            local_peer_id,
            MemoryStoreConfig {
                provider_refresh_interval: Duration::from_secs(5),
                ..Default::default()
            },
        );

        let key = Key::from(vec![1, 2, 3]);
        let local_provider = ContentProvider {
            peer: local_peer_id,
            addresses: vec![],
        };
        let quorum = Quorum::One;

        assert!(store.put_local_provider(key.clone(), quorum));

        assert_eq!(store.get_providers(&key), vec![local_provider.clone()]);
        assert_eq!(
            store.local_providers.get(&key),
            Some(&(local_provider.clone(), quorum))
        );

        // No actions are instantly generated.
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(1), store.next_action()).await,
            Err(_),
        ));
        // The local provider is refreshed.
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), store.next_action())
                .await
                .unwrap(),
            Some(MemoryStoreAction::RefreshProvider {
                provided_key: key,
                provider: local_provider,
                quorum,
            }),
        );
    }

    #[tokio::test]
    async fn local_provider_inserted_after_remote_provider_refresh() {
        let local_peer_id = PeerId::random();
        let mut store = MemoryStore::with_config(
            local_peer_id,
            MemoryStoreConfig {
                provider_refresh_interval: Duration::from_secs(5),
                ..Default::default()
            },
        );

        let key = Key::from(vec![1, 2, 3]);

        let remote_peer_id = PeerId::random();
        let remote_provider = ContentProvider {
            peer: remote_peer_id,
            addresses: vec![multiaddr!(Ip4([192, 168, 0, 1]), Tcp(10000u16))],
        };

        let local_provider = ContentProvider {
            peer: local_peer_id,
            addresses: vec![],
        };
        let quorum = Quorum::One;

        assert!(store.put_provider(key.clone(), remote_provider.clone()));
        assert!(store.put_local_provider(key.clone(), quorum));

        let got_providers = store.get_providers(&key);
        assert_eq!(got_providers.len(), 2);
        assert!(got_providers.contains(&remote_provider));
        assert!(got_providers.contains(&local_provider));

        assert_eq!(
            store.local_providers.get(&key),
            Some(&(local_provider.clone(), quorum))
        );

        // No actions are instantly generated.
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(1), store.next_action()).await,
            Err(_),
        ));
        // The local provider is refreshed.
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), store.next_action())
                .await
                .unwrap(),
            Some(MemoryStoreAction::RefreshProvider {
                provided_key: key,
                provider: local_provider,
                quorum,
            }),
        );
    }

    #[tokio::test]
    async fn removed_local_provider_not_refreshed() {
        let local_peer_id = PeerId::random();
        let mut store = MemoryStore::with_config(
            local_peer_id,
            MemoryStoreConfig {
                provider_refresh_interval: Duration::from_secs(1),
                ..Default::default()
            },
        );

        let key = Key::from(vec![1, 2, 3]);
        let local_provider = ContentProvider {
            peer: local_peer_id,
            addresses: vec![],
        };
        let quorum = Quorum::One;

        assert!(store.put_local_provider(key.clone(), quorum));

        assert_eq!(store.get_providers(&key), vec![local_provider.clone()]);
        assert_eq!(
            store.local_providers.get(&key),
            Some(&(local_provider, quorum))
        );

        store.remove_local_provider(key);

        // The local provider is not refreshed in 10 secs (future fires at 1 sec and yields `None`).
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), store.next_action()).await,
            Ok(None),
        );
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(5), store.next_action()).await,
            Err(_),
        ));
    }
}
