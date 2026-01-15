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

use bytes::Bytes;

use crate::{
    protocol::libp2p::kademlia::{
        message::KademliaMessage,
        query::{QueryAction, QueryId},
        types::{Distance, KademliaPeer, Key},
    },
    PeerId,
};

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::ipfs::kademlia::query::find_node";

/// Default timeout for a peer to respond to a query.
const DEFAULT_PEER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The configuration needed to instantiate a new [`FindNodeContext`].
#[derive(Debug, Clone)]
pub struct FindNodeConfig<T: Clone + Into<Vec<u8>>> {
    /// Local peer ID.
    pub local_peer_id: PeerId,

    /// Replication factor.
    pub replication_factor: usize,

    /// Parallelism factor.
    pub parallelism_factor: usize,

    /// Query ID.
    pub query: QueryId,

    /// Target key.
    pub target: Key<T>,
}

/// Context for `FIND_NODE` queries.
#[derive(Debug)]
pub struct FindNodeContext<T: Clone + Into<Vec<u8>>> {
    /// Query immutable config.
    pub config: FindNodeConfig<T>,

    /// Cached Kademlia message to send.
    kad_message: Bytes,

    /// Peers from whom the `QueryEngine` is waiting to hear a response.
    pub pending: HashMap<PeerId, (KademliaPeer, std::time::Instant)>,

    /// Queried candidates.
    ///
    /// These are the peers for whom the query has already been sent
    /// and who have either returned their closest peers or failed to answer.
    pub queried: HashSet<PeerId>,

    /// Candidates.
    pub candidates: BTreeMap<Distance, KademliaPeer>,

    /// Responses.
    pub responses: BTreeMap<Distance, KademliaPeer>,

    /// The timeout after which the pending request is no longer
    /// counting towards the parallelism factor.
    ///
    /// This is used to prevent the query from getting stuck when a peer
    /// is slow or fails to respond in due time.
    peer_timeout: std::time::Duration,
    /// The number of pending responses that count towards the parallelism factor.
    ///
    /// These represent the number of peers added to the `Self::pending` minus the number of peers
    /// that have failed to respond within the `Self::peer_timeout`
    pending_responses: usize,
}

impl<T: Clone + Into<Vec<u8>>> FindNodeContext<T> {
    /// Create new [`FindNodeContext`].
    pub fn new(config: FindNodeConfig<T>, in_peers: VecDeque<KademliaPeer>) -> Self {
        let mut candidates = BTreeMap::new();

        for candidate in &in_peers {
            let distance = config.target.distance(&candidate.key);
            candidates.insert(distance, candidate.clone());
        }

        let kad_message = KademliaMessage::find_node(config.target.clone().into_preimage());

        Self {
            config,
            kad_message,

            candidates,
            pending: HashMap::new(),
            queried: HashSet::new(),
            responses: BTreeMap::new(),

            peer_timeout: DEFAULT_PEER_TIMEOUT,
            pending_responses: 0,
        }
    }

    /// Register response failure for `peer`.
    pub fn register_response_failure(&mut self, peer: PeerId) {
        let Some((peer, instant)) = self.pending.remove(&peer) else {
            tracing::debug!(target: LOG_TARGET, query = ?self.config.query, ?peer, "pending peer doesn't exist during response failure");
            return;
        };
        self.pending_responses = self.pending_responses.saturating_sub(1);

        tracing::trace!(target: LOG_TARGET, query = ?self.config.query, ?peer, elapsed = ?instant.elapsed(), "peer failed to respond");

        self.queried.insert(peer.peer);
    }

    /// Register `FIND_NODE` response from `peer`.
    pub fn register_response(&mut self, peer: PeerId, peers: Vec<KademliaPeer>) {
        let Some((peer, instant)) = self.pending.remove(&peer) else {
            tracing::debug!(target: LOG_TARGET, query = ?self.config.query, ?peer, "received response from peer but didn't expect it");
            return;
        };
        self.pending_responses = self.pending_responses.saturating_sub(1);

        tracing::trace!(target: LOG_TARGET, query = ?self.config.query, ?peer, elapsed = ?instant.elapsed(), "received response from peer");

        // calculate distance for `peer` from target and insert it if
        //  a) the map doesn't have 20 responses
        //  b) it can replace some other peer that has a higher distance
        let distance = self.config.target.distance(&peer.key);

        // always mark the peer as queried to prevent it getting queried again
        self.queried.insert(peer.peer);

        if self.responses.len() < self.config.replication_factor {
            self.responses.insert(distance, peer);
        } else {
            // Update the furthest peer if this response is closer.
            // Find the furthest distance.
            let furthest_distance =
                self.responses.last_entry().map(|entry| *entry.key()).unwrap_or(distance);

            // The response received from the peer is closer than the furthest response.
            if distance < furthest_distance {
                self.responses.insert(distance, peer);

                // Remove the furthest entry.
                if self.responses.len() > self.config.replication_factor {
                    self.responses.pop_last();
                }
            }
        }

        let to_query_candidate = peers.into_iter().filter_map(|peer| {
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

    /// Register a failure of sending `FIN_NODE` request to `peer`.
    pub fn register_send_failure(&mut self, _peer: PeerId) {
        // In case of a send failure, `register_response_failure` is called as well.
        // Failure is handled there.
    }

    /// Register a success of sending `FIND_NODE` request to `peer`.
    pub fn register_send_success(&mut self, _peer: PeerId) {
        // `FIND_NODE` requests are compound request-response pairs of messages,
        // so we handle final success/failure in `register_response`/`register_response_failure`.
    }

    /// Get next action for `peer`.
    pub fn next_peer_action(&mut self, peer: &PeerId) -> Option<QueryAction> {
        self.pending.contains_key(peer).then_some(QueryAction::SendMessage {
            query: self.config.query,
            peer: *peer,
            message: self.kad_message.clone(),
        })
    }

    /// Schedule next peer for outbound `FIND_NODE` query.
    fn schedule_next_peer(&mut self) -> Option<QueryAction> {
        tracing::trace!(target: LOG_TARGET, query = ?self.config.query, "get next peer");

        let (_, candidate) = self.candidates.pop_first()?;
        let peer = candidate.peer;

        tracing::trace!(target: LOG_TARGET, query = ?self.config.query, ?peer, "current candidate");
        self.pending.insert(candidate.peer, (candidate, std::time::Instant::now()));
        self.pending_responses = self.pending_responses.saturating_add(1);

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

    /// Get next action for a `FIND_NODE` query.
    pub fn next_action(&mut self) -> Option<QueryAction> {
        // If we cannot make progress, return the final result.
        // A query failed when we are not able to identify one single peer.
        if self.is_done() {
            tracing::trace!(
                target: LOG_TARGET,
                query = ?self.config.query,
                pending = self.pending.len(),
                candidates = self.candidates.len(),
                "query finished"
            );

            return if self.responses.is_empty() {
                Some(QueryAction::QueryFailed {
                    query: self.config.query,
                })
            } else {
                Some(QueryAction::QuerySucceeded {
                    query: self.config.query,
                })
            };
        }

        for (peer, instant) in self.pending.values() {
            if instant.elapsed() > self.peer_timeout {
                tracing::trace!(
                    target: LOG_TARGET,
                    query = ?self.config.query,
                    ?peer,
                    elapsed = ?instant.elapsed(),
                    "peer no longer counting towards parallelism factor"
                );
                self.pending_responses = self.pending_responses.saturating_sub(1);
            }
        }

        // At this point, we either have pending responses or candidates to query; and we need more
        // results. Ensure we do not exceed the parallelism factor.
        if self.pending_responses == self.config.parallelism_factor {
            return None;
        }

        // Schedule the next peer to fill up the responses.
        if self.responses.len() < self.config.replication_factor {
            return self.schedule_next_peer();
        }

        // We can finish the query here, but check if there is a better candidate for the query.
        match (
            self.candidates.first_key_value(),
            self.responses.last_key_value(),
        ) {
            (Some((_, candidate_peer)), Some((worst_response_distance, _))) => {
                let first_candidate_distance = self.config.target.distance(&candidate_peer.key);
                if first_candidate_distance < *worst_response_distance {
                    return self.schedule_next_peer();
                }
            }

            _ => (),
        }

        // We have found enough responses and there are no better candidates to query.
        Some(QueryAction::QuerySucceeded {
            query: self.config.query,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::libp2p::kademlia::types::ConnectionType;

    fn default_config() -> FindNodeConfig<Vec<u8>> {
        FindNodeConfig {
            local_peer_id: PeerId::random(),
            replication_factor: 20,
            parallelism_factor: 10,
            query: QueryId(0),
            target: Key::new(vec![1, 2, 3]),
        }
    }

    fn peer_to_kad(peer: PeerId) -> KademliaPeer {
        KademliaPeer {
            peer,
            key: Key::from(peer),
            address_store: Default::default(),
            connection: ConnectionType::Connected,
        }
    }

    fn setup_closest_responses() -> (PeerId, PeerId, FindNodeConfig<PeerId>) {
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let target = PeerId::random();

        let distance_a = Key::from(peer_a).distance(&Key::from(target));
        let distance_b = Key::from(peer_b).distance(&Key::from(target));

        let (closest, furthest) = if distance_a < distance_b {
            (peer_a, peer_b)
        } else {
            (peer_b, peer_a)
        };

        let config = FindNodeConfig {
            parallelism_factor: 1,
            replication_factor: 1,
            target: Key::from(target),
            local_peer_id: PeerId::random(),
            query: QueryId(0),
        };

        (closest, furthest, config)
    }

    #[test]
    fn completes_when_no_candidates() {
        let config = default_config();
        let mut context = FindNodeContext::new(config, VecDeque::new());
        assert!(context.is_done());
        let event = context.next_action().unwrap();
        match event {
            QueryAction::QueryFailed { query, .. } => {
                assert_eq!(query, QueryId(0));
            }
            _ => panic!("Unexpected event"),
        };
    }

    #[test]
    fn fulfill_parallelism() {
        let config = FindNodeConfig {
            parallelism_factor: 3,
            ..default_config()
        };

        let in_peers_set = (0..3).map(|_| PeerId::random()).collect::<HashSet<_>>();
        let in_peers = in_peers_set.iter().map(|peer| peer_to_kad(*peer)).collect();
        let mut context = FindNodeContext::new(config, in_peers);

        for num in 0..3 {
            let event = context.next_action().unwrap();
            match event {
                QueryAction::SendMessage { query, peer, .. } => {
                    assert_eq!(query, QueryId(0));
                    // Added as pending.
                    assert_eq!(context.pending.len(), num + 1);
                    assert!(context.pending.contains_key(&peer));

                    // Check the peer is the one provided.
                    assert!(in_peers_set.contains(&peer));
                }
                _ => panic!("Unexpected event"),
            }
        }

        // Fulfilled parallelism.
        assert!(context.next_action().is_none());
    }

    #[test]
    fn fulfill_parallelism_with_timeout_optimization() {
        let config = FindNodeConfig {
            parallelism_factor: 3,
            ..default_config()
        };

        let in_peers_set = (0..4).map(|_| PeerId::random()).collect::<HashSet<_>>();
        let in_peers = in_peers_set.iter().map(|peer| peer_to_kad(*peer)).collect();
        let mut context = FindNodeContext::new(config, in_peers);
        // Test overwrite.
        context.peer_timeout = std::time::Duration::from_secs(1);

        for num in 0..3 {
            let event = context.next_action().unwrap();
            match event {
                QueryAction::SendMessage { query, peer, .. } => {
                    assert_eq!(query, QueryId(0));
                    // Added as pending.
                    assert_eq!(context.pending.len(), num + 1);
                    assert!(context.pending.contains_key(&peer));

                    // Check the peer is the one provided.
                    assert!(in_peers_set.contains(&peer));
                }
                _ => panic!("Unexpected event"),
            }
        }

        // Fulfilled parallelism.
        assert!(context.next_action().is_none());

        // Sleep more than 1 second.
        std::thread::sleep(std::time::Duration::from_secs(2));

        // The pending responses are reset only on the next query action.
        assert_eq!(context.pending_responses, 3);
        assert_eq!(context.pending.len(), 3);

        // This allows other peers to be queried.
        let event = context.next_action().unwrap();
        match event {
            QueryAction::SendMessage { query, peer, .. } => {
                assert_eq!(query, QueryId(0));
                // Added as pending.
                assert_eq!(context.pending.len(), 4);
                assert!(context.pending.contains_key(&peer));

                // Check the peer is the one provided.
                assert!(in_peers_set.contains(&peer));
            }
            _ => panic!("Unexpected event"),
        }

        assert_eq!(context.pending_responses, 1);
        assert_eq!(context.pending.len(), 4);
    }

    #[test]
    fn completes_when_responses() {
        let config = FindNodeConfig {
            parallelism_factor: 3,
            replication_factor: 3,
            ..default_config()
        };

        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let peer_c = PeerId::random();

        let in_peers_set: HashSet<_> = [peer_a, peer_b, peer_c].into_iter().collect();
        assert_eq!(in_peers_set.len(), 3);

        let in_peers = [peer_a, peer_b, peer_c].iter().map(|peer| peer_to_kad(*peer)).collect();
        let mut context = FindNodeContext::new(config, in_peers);

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
                    assert!(in_peers_set.contains(&peer));
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
        context.register_response(peer_a, vec![]);
        assert_eq!(context.pending.len(), 2);
        assert_eq!(context.queried.len(), 1);
        assert_eq!(context.responses.len(), 1);

        // Provide different response from peer b with peer d as candidate.
        context.register_response(peer_b, vec![peer_to_kad(peer_d)]);
        assert_eq!(context.pending.len(), 1);
        assert_eq!(context.queried.len(), 2);
        assert_eq!(context.responses.len(), 2);
        assert_eq!(context.candidates.len(), 1);

        // Peer C fails.
        context.register_response_failure(peer_c);
        assert!(context.pending.is_empty());
        assert_eq!(context.queried.len(), 3);
        assert_eq!(context.responses.len(), 2);

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
        context.register_response(peer_d, vec![]);

        // Produces the result.
        let event = context.next_action().unwrap();
        match event {
            QueryAction::QuerySucceeded { query, .. } => {
                assert_eq!(query, QueryId(0));
            }
            _ => panic!("Unexpected event"),
        };
    }

    #[test]
    fn offers_closest_responses() {
        let (closest, furthest, config) = setup_closest_responses();

        // Scenario where we should return with the number of responses.
        let in_peers = vec![peer_to_kad(furthest), peer_to_kad(closest)];
        let mut context = FindNodeContext::new(config.clone(), in_peers.into_iter().collect());

        let event = context.next_action().unwrap();
        match event {
            QueryAction::SendMessage { query, peer, .. } => {
                assert_eq!(query, QueryId(0));
                // Added as pending.
                assert_eq!(context.pending.len(), 1);
                assert!(context.pending.contains_key(&peer));

                // The closest should be queried first regardless of the input order.
                assert_eq!(closest, peer);
            }
            _ => panic!("Unexpected event"),
        }

        context.register_response(closest, vec![]);

        let event = context.next_action().unwrap();
        match event {
            QueryAction::QuerySucceeded { query } => {
                assert_eq!(query, QueryId(0));
            }
            _ => panic!("Unexpected event"),
        };
    }

    #[test]
    fn offers_closest_responses_with_better_candidates() {
        let (closest, furthest, config) = setup_closest_responses();

        // Scenario where the query is fulfilled however it continues because
        // there is a closer peer to query.
        let in_peers = vec![peer_to_kad(furthest)];
        let mut context = FindNodeContext::new(config, in_peers.into_iter().collect());

        let event = context.next_action().unwrap();
        match event {
            QueryAction::SendMessage { query, peer, .. } => {
                assert_eq!(query, QueryId(0));
                // Added as pending.
                assert_eq!(context.pending.len(), 1);
                assert!(context.pending.contains_key(&peer));

                // Furthest is the only peer available.
                assert_eq!(furthest, peer);
            }
            _ => panic!("Unexpected event"),
        }

        // Furthest node produces a response with the closest node.
        // Even if we reach a total of 1 (parallelism factor) replies, we should continue.
        context.register_response(furthest, vec![peer_to_kad(closest)]);

        let event = context.next_action().unwrap();
        match event {
            QueryAction::SendMessage { query, peer, .. } => {
                assert_eq!(query, QueryId(0));
                // Added as pending.
                assert_eq!(context.pending.len(), 1);
                assert!(context.pending.contains_key(&peer));

                // Furthest provided another peer that is closer.
                assert_eq!(closest, peer);
            }
            _ => panic!("Unexpected event"),
        }

        // Even if we have the total number of responses, we have at least one
        // inflight query which might be closer to the target.
        assert!(context.next_action().is_none());

        // Query finishes when receiving the response back.
        context.register_response(closest, vec![]);

        let event = context.next_action().unwrap();
        match event {
            QueryAction::QuerySucceeded { query, .. } => {
                assert_eq!(query, QueryId(0));
            }
            _ => panic!("Unexpected event"),
        };
    }

    #[test]
    fn keep_k_best_results() {
        let mut peers = (0..6).map(|_| PeerId::random()).collect::<Vec<_>>();
        let target = Key::from(PeerId::random());
        // Sort the peers by their distance to the target in descending order.
        peers.sort_by_key(|peer| std::cmp::Reverse(target.distance(&Key::from(*peer))));

        let config = FindNodeConfig {
            parallelism_factor: 3,
            replication_factor: 3,
            target,
            local_peer_id: PeerId::random(),
            query: QueryId(0),
        };

        let in_peers = vec![peers[0], peers[1], peers[2]]
            .iter()
            .map(|peer| peer_to_kad(*peer))
            .collect();
        let mut context = FindNodeContext::new(config, in_peers);

        // Schedule peer queries.
        for num in 0..3 {
            let event = context.next_action().unwrap();
            match event {
                QueryAction::SendMessage { query, peer, .. } => {
                    assert_eq!(query, QueryId(0));
                    // Added as pending.
                    assert_eq!(context.pending.len(), num + 1);
                    assert!(context.pending.contains_key(&peer));
                }
                _ => panic!("Unexpected event"),
            }
        }

        // Each peer responds with a better (closer) peer.
        context.register_response(peers[0], vec![peer_to_kad(peers[3])]);
        context.register_response(peers[1], vec![peer_to_kad(peers[4])]);
        context.register_response(peers[2], vec![peer_to_kad(peers[5])]);

        // Must schedule better peers.
        for num in 0..3 {
            let event = context.next_action().unwrap();
            match event {
                QueryAction::SendMessage { query, peer, .. } => {
                    assert_eq!(query, QueryId(0));
                    // Added as pending.
                    assert_eq!(context.pending.len(), num + 1);
                    assert!(context.pending.contains_key(&peer));
                }
                _ => panic!("Unexpected event"),
            }
        }

        context.register_response(peers[3], vec![]);
        context.register_response(peers[4], vec![]);
        context.register_response(peers[5], vec![]);

        // Produces the result.
        let event = context.next_action().unwrap();
        match event {
            QueryAction::QuerySucceeded { query } => {
                assert_eq!(query, QueryId(0));
            }
            _ => panic!("Unexpected event"),
        };

        // Because the FindNode query keeps a window of the best K (3 in this case) peers,
        // we expect to produce the best K peers. As opposed to having only the last entry
        // updated, which would have produced [peer[0], peer[1], peer[5]].

        // Check the responses.
        let responses = context.responses.values().map(|peer| peer.peer).collect::<Vec<_>>();
        // Note: peers are returned in order closest to the target, our `peers` input is sorted in
        // decreasing order.
        assert_eq!(responses, [peers[5], peers[4], peers[3]]);
    }
}
