// Copyright 2024 litep2p developers
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

use bytes::Bytes;

use crate::{
    protocol::libp2p::kademlia::{
        message::KademliaMessage,
        query::{QueryAction, QueryId},
        record::{ContentProvider, Key as RecordKey},
        types::{Distance, KademliaPeer, Key},
    },
    types::multiaddr::Multiaddr,
    PeerId,
};

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::ipfs::kademlia::query::get_providers";

/// The configuration needed to instantiate a new [`GetProvidersContext`].
#[derive(Debug)]
pub struct GetProvidersConfig {
    /// Local peer ID.
    pub local_peer_id: PeerId,

    /// Parallelism factor.
    pub parallelism_factor: usize,

    /// Query ID.
    pub query: QueryId,

    /// Target key.
    pub target: Key<RecordKey>,

    /// Known providers from the local store.
    pub known_providers: Vec<KademliaPeer>,
}

#[derive(Debug)]
pub struct GetProvidersContext {
    /// Query immutable config.
    pub config: GetProvidersConfig,

    /// Cached Kademlia message to send.
    kad_message: Bytes,

    /// Peers from whom the `QueryEngine` is waiting to hear a response.
    pub pending: HashMap<PeerId, KademliaPeer>,

    /// Queried candidates.
    ///
    /// These are the peers for whom the query has already been sent
    /// and who have either returned their closest peers or failed to answer.
    pub queried: HashSet<PeerId>,

    /// Candidates.
    pub candidates: BTreeMap<Distance, KademliaPeer>,

    /// Found providers.
    pub found_providers: Vec<KademliaPeer>,
}

impl GetProvidersContext {
    /// Create new [`GetProvidersContext`].
    pub fn new(config: GetProvidersConfig, candidate_peers: VecDeque<KademliaPeer>) -> Self {
        let mut candidates = BTreeMap::new();

        for peer in &candidate_peers {
            let distance = config.target.distance(&peer.key);
            candidates.insert(distance, peer.clone());
        }

        let kad_message =
            KademliaMessage::get_providers_request(config.target.clone().into_preimage());

        Self {
            config,
            kad_message,
            candidates,
            pending: HashMap::new(),
            queried: HashSet::new(),
            found_providers: Vec::new(),
        }
    }

    /// Get the found providers.
    pub fn found_providers(self) -> Vec<ContentProvider> {
        Self::merge_and_sort_providers(
            self.config.known_providers.into_iter().chain(self.found_providers),
            self.config.target,
        )
    }

    fn merge_and_sort_providers(
        found_providers: impl IntoIterator<Item = KademliaPeer>,
        target: Key<RecordKey>,
    ) -> Vec<ContentProvider> {
        // Merge addresses of different provider records of the same peer.
        let mut providers = HashMap::<PeerId, HashSet<Multiaddr>>::new();
        found_providers.into_iter().for_each(|provider| {
            providers.entry(provider.peer).or_default().extend(provider.addresses())
        });

        // Convert into `Vec<KademliaPeer>`
        let mut providers = providers
            .into_iter()
            .map(|(peer, addresses)| ContentProvider {
                peer,
                addresses: addresses.into_iter().collect(),
            })
            .collect::<Vec<_>>();

        // Sort by the provider distance to the target key.
        providers.sort_unstable_by(|p1, p2| {
            Key::from(p1.peer).distance(&target).cmp(&Key::from(p2.peer).distance(&target))
        });

        providers
    }

    /// Register response failure for `peer`.
    pub fn register_response_failure(&mut self, peer: PeerId) {
        let Some(peer) = self.pending.remove(&peer) else {
            tracing::debug!(
                target: LOG_TARGET,
                query = ?self.config.query,
                ?peer,
                "`GetProvidersContext`: pending peer doesn't exist",
            );
            return;
        };

        self.queried.insert(peer.peer);
    }

    /// Register `GET_PROVIDERS` response from `peer`.
    pub fn register_response(
        &mut self,
        peer: PeerId,
        providers: impl IntoIterator<Item = KademliaPeer>,
        closer_peers: impl IntoIterator<Item = KademliaPeer>,
    ) {
        tracing::trace!(
            target: LOG_TARGET,
            query = ?self.config.query,
            ?peer,
            "`GetProvidersContext`: received response from peer",
        );

        let Some(peer) = self.pending.remove(&peer) else {
            tracing::debug!(
                target: LOG_TARGET,
                query = ?self.config.query,
                ?peer,
                "`GetProvidersContext`: received response from peer but didn't expect it",
            );
            return;
        };

        self.found_providers.extend(providers);

        // Add the queried peer to `queried` and all new peers which haven't been
        // queried to `candidates`
        self.queried.insert(peer.peer);

        let to_query_candidate = closer_peers.into_iter().filter_map(|peer| {
            // Peer already produced a response.
            if self.queried.contains(&peer.peer) {
                return None;
            }

            // Peer was queried, awaiting response.
            if self.pending.contains_key(&peer.peer) {
                return None;
            }

            // Local node.
            if self.config.local_peer_id == peer.peer {
                return None;
            }

            Some(peer)
        });

        for candidate in to_query_candidate {
            let distance = self.config.target.distance(&candidate.key);
            self.candidates.insert(distance, candidate);
        }
    }

    /// Register a failure of sending a `GET_PROVIDERS` request to `peer`.
    pub fn register_send_failure(&mut self, _peer: PeerId) {
        // In case of a send failure, `register_response_failure` is called as well.
        // Failure is handled there.
    }

    /// Register a success of sending a `GET_PROVIDERS` request to `peer`.
    pub fn register_send_success(&mut self, _peer: PeerId) {
        // `GET_PROVIDERS` requests are compound request-response pairs of messages,
        // so we handle final success/failure in `register_response`/`register_response_failure`.
    }

    /// Get next action for `peer`.
    // TODO: https://github.com/paritytech/litep2p/issues/40 remove this and store the next action to `PeerAction`
    pub fn next_peer_action(&mut self, peer: &PeerId) -> Option<QueryAction> {
        self.pending.contains_key(peer).then_some(QueryAction::SendMessage {
            query: self.config.query,
            peer: *peer,
            message: self.kad_message.clone(),
        })
    }

    /// Schedule next peer for outbound `GET_VALUE` query.
    fn schedule_next_peer(&mut self) -> Option<QueryAction> {
        tracing::trace!(
            target: LOG_TARGET,
            query = ?self.config.query,
            "`GetProvidersContext`: get next peer",
        );

        let (_, candidate) = self.candidates.pop_first()?;
        let peer = candidate.peer;

        tracing::trace!(
            target: LOG_TARGET,
            query = ?self.config.query,
            ?peer,
            "`GetProvidersContext`: current candidate",
        );
        self.pending.insert(candidate.peer, candidate);

        Some(QueryAction::SendMessage {
            query: self.config.query,
            peer,
            message: self.kad_message.clone(),
        })
    }

    /// Check if the query cannot make any progress.
    ///
    /// Returns true when there are no pending responses and no candidates to query.
    fn is_done(&self) -> bool {
        self.pending.is_empty() && self.candidates.is_empty()
    }

    /// Get next action for a `GET_PROVIDERS` query.
    pub fn next_action(&mut self) -> Option<QueryAction> {
        if self.is_done() {
            // If we cannot make progress, return the final result.
            // A query failed when we are not able to find any providers.
            if self.found_providers.is_empty() {
                Some(QueryAction::QueryFailed {
                    query: self.config.query,
                })
            } else {
                Some(QueryAction::QuerySucceeded {
                    query: self.config.query,
                })
            }
        } else if self.pending.len() == self.config.parallelism_factor {
            // At this point, we either have pending responses or candidates to query; and we need
            // more records. Ensure we do not exceed the parallelism factor.
            None
        } else {
            self.schedule_next_peer()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::libp2p::kademlia::types::ConnectionType;
    use multiaddr::multiaddr;

    fn default_config() -> GetProvidersConfig {
        GetProvidersConfig {
            local_peer_id: PeerId::random(),
            parallelism_factor: 3,
            query: QueryId(0),
            target: Key::new(vec![1, 2, 3].into()),
            known_providers: vec![],
        }
    }

    fn peer_to_kad(peer: PeerId) -> KademliaPeer {
        KademliaPeer {
            peer,
            key: Key::from(peer),
            address_store: Default::default(),
            connection: ConnectionType::NotConnected,
        }
    }

    fn peer_to_kad_with_addresses(peer: PeerId, addresses: Vec<Multiaddr>) -> KademliaPeer {
        KademliaPeer::new(peer, addresses, ConnectionType::NotConnected)
    }

    #[test]
    fn completes_when_no_candidates() {
        let config = default_config();

        let mut context = GetProvidersContext::new(config, VecDeque::new());
        assert!(context.is_done());

        let event = context.next_action().unwrap();
        match event {
            QueryAction::QueryFailed { query, .. } => {
                assert_eq!(query, QueryId(0));
            }
            _ => panic!("Unexpected event"),
        }
    }

    #[test]
    fn fulfill_parallelism() {
        let config = GetProvidersConfig {
            parallelism_factor: 3,
            ..default_config()
        };

        let candidate_peer_set: HashSet<_> =
            [PeerId::random(), PeerId::random(), PeerId::random()].into_iter().collect();
        assert_eq!(candidate_peer_set.len(), 3);

        let candidate_peers = candidate_peer_set.iter().map(|peer| peer_to_kad(*peer)).collect();
        let mut context = GetProvidersContext::new(config, candidate_peers);

        for num in 0..3 {
            let event = context.next_action().unwrap();
            match event {
                QueryAction::SendMessage { query, peer, .. } => {
                    assert_eq!(query, QueryId(0));
                    // Added as pending.
                    assert_eq!(context.pending.len(), num + 1);
                    assert!(context.pending.contains_key(&peer));

                    // Check the peer is the one provided.
                    assert!(candidate_peer_set.contains(&peer));
                }
                _ => panic!("Unexpected event"),
            }
        }

        // Fulfilled parallelism.
        assert!(context.next_action().is_none());
    }

    #[test]
    fn completes_when_responses() {
        let config = GetProvidersConfig {
            parallelism_factor: 3,
            ..default_config()
        };

        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let peer_c = PeerId::random();

        let candidate_peer_set: HashSet<_> = [peer_a, peer_b, peer_c].into_iter().collect();
        assert_eq!(candidate_peer_set.len(), 3);

        let candidate_peers =
            [peer_a, peer_b, peer_c].iter().map(|peer| peer_to_kad(*peer)).collect();
        let mut context = GetProvidersContext::new(config, candidate_peers);

        let [provider1, provider2, provider3, provider4] = (0..4)
            .map(|_| ContentProvider {
                peer: PeerId::random(),
                addresses: vec![],
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        // Schedule peer queries.
        for num in 0..3 {
            let event = context.next_action().unwrap();
            match event {
                QueryAction::SendMessage { query, peer, .. } => {
                    assert_eq!(query, QueryId(0));
                    // Added as pending.
                    assert_eq!(context.pending.len(), num + 1);
                    assert!(context.pending.contains_key(&peer));

                    // Check the peer is the one provided.
                    assert!(candidate_peer_set.contains(&peer));
                }
                _ => panic!("Unexpected event"),
            }
        }

        // Checks a failed query that was not initiated.
        let peer_d = PeerId::random();
        context.register_response_failure(peer_d);
        assert_eq!(context.pending.len(), 3);
        assert!(context.queried.is_empty());

        // Provide responses back.
        let providers = vec![provider1.clone().into(), provider2.clone().into()];
        context.register_response(peer_a, providers, vec![]);
        assert_eq!(context.pending.len(), 2);
        assert_eq!(context.queried.len(), 1);
        assert_eq!(context.found_providers.len(), 2);

        // Provide different response from peer b with peer d as candidate.
        let providers = vec![provider2.clone().into(), provider3.clone().into()];
        let candidates = vec![peer_to_kad(peer_d)];
        context.register_response(peer_b, providers, candidates);
        assert_eq!(context.pending.len(), 1);
        assert_eq!(context.queried.len(), 2);
        assert_eq!(context.found_providers.len(), 4);
        assert_eq!(context.candidates.len(), 1);

        // Peer C fails.
        context.register_response_failure(peer_c);
        assert!(context.pending.is_empty());
        assert_eq!(context.queried.len(), 3);
        assert_eq!(context.found_providers.len(), 4);

        // Drain the last candidate.
        let event = context.next_action().unwrap();
        match event {
            QueryAction::SendMessage { query, peer, .. } => {
                assert_eq!(query, QueryId(0));
                // Added as pending.
                assert_eq!(context.pending.len(), 1);
                assert_eq!(peer, peer_d);
            }
            _ => panic!("Unexpected event"),
        }

        // Peer D responds.
        let providers = vec![provider4.clone().into()];
        context.register_response(peer_d, providers, vec![]);

        // Produces the result.
        let event = context.next_action().unwrap();
        match event {
            QueryAction::QuerySucceeded { query, .. } => {
                assert_eq!(query, QueryId(0));
            }
            _ => panic!("Unexpected event"),
        }

        // Check results.
        let found_providers = context.found_providers();
        assert_eq!(found_providers.len(), 4);
        assert!(found_providers.contains(&provider1));
        assert!(found_providers.contains(&provider2));
        assert!(found_providers.contains(&provider3));
        assert!(found_providers.contains(&provider4));
    }

    #[test]
    fn providers_sorted_by_distance() {
        let target = Key::new(vec![1, 2, 3].into());

        let mut peers = (0..10).map(|_| PeerId::random()).collect::<Vec<_>>();
        let providers = peers.iter().map(|peer| peer_to_kad(*peer)).collect::<Vec<_>>();

        let found_providers =
            GetProvidersContext::merge_and_sort_providers(providers, target.clone());

        peers.sort_by(|p1, p2| {
            Key::from(*p1).distance(&target).cmp(&Key::from(*p2).distance(&target))
        });

        assert!(
            std::iter::zip(found_providers.into_iter(), peers.into_iter())
                .all(|(provider, peer)| provider.peer == peer)
        );
    }

    #[test]
    fn provider_addresses_merged() {
        let peer = PeerId::random();

        let address1 = multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10000u16));
        let address2 = multiaddr!(Ip4([192, 168, 0, 1]), Tcp(10000u16));
        let address3 = multiaddr!(Ip4([10, 0, 0, 1]), Tcp(10000u16));
        let address4 = multiaddr!(Ip4([1, 1, 1, 1]), Tcp(10000u16));
        let address5 = multiaddr!(Ip4([8, 8, 8, 8]), Tcp(10000u16));

        let provider1 = peer_to_kad_with_addresses(peer, vec![address1.clone()]);
        let provider2 = peer_to_kad_with_addresses(
            peer,
            vec![address2.clone(), address3.clone(), address4.clone()],
        );
        let provider3 = peer_to_kad_with_addresses(peer, vec![address4.clone(), address5.clone()]);

        let providers = vec![provider1, provider2, provider3];

        let found_providers = GetProvidersContext::merge_and_sort_providers(
            providers,
            Key::new(vec![1, 2, 3].into()),
        );

        assert_eq!(found_providers.len(), 1);

        let addresses = &found_providers.first().unwrap().addresses;
        assert_eq!(addresses.len(), 5);
        assert!(addresses.contains(&address1));
        assert!(addresses.contains(&address2));
        assert!(addresses.contains(&address3));
        assert!(addresses.contains(&address4));
        assert!(addresses.contains(&address5));
    }
}
