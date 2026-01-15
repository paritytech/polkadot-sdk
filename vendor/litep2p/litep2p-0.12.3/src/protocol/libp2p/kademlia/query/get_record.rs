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
        record::{Key as RecordKey, PeerRecord, Record},
        types::{Distance, KademliaPeer, Key},
        Quorum,
    },
    PeerId,
};

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::ipfs::kademlia::query::get_record";

/// The configuration needed to instantiate a new [`GetRecordContext`].
#[derive(Debug)]
pub struct GetRecordConfig {
    /// Local peer ID.
    pub local_peer_id: PeerId,

    /// How many records we already know about (ie extracted from storage).
    ///
    /// This can either be 0 or 1 when the record is extracted local storage.
    pub known_records: usize,

    /// Quorum for the query.
    pub quorum: Quorum,

    /// Replication factor.
    pub replication_factor: usize,

    /// Parallelism factor.
    pub parallelism_factor: usize,

    /// Query ID.
    pub query: QueryId,

    /// Target key.
    pub target: Key<RecordKey>,
}

impl GetRecordConfig {
    /// Checks if the found number of records meets the specified quorum.
    ///
    /// Used to determine if the query found enough records to stop.
    fn sufficient_records(&self, records: usize) -> bool {
        // The total number of known records is the sum of the records we knew about before starting
        // the query and the records we found along the way.
        let total_known = self.known_records + records;

        match self.quorum {
            Quorum::All => total_known >= self.replication_factor,
            Quorum::One => total_known >= 1,
            Quorum::N(needed_responses) => total_known >= needed_responses.get(),
        }
    }
}

#[derive(Debug)]
pub struct GetRecordContext {
    /// Query immutable config.
    pub config: GetRecordConfig,

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

    /// Number of found records.
    pub found_records: usize,

    /// Records to propagate as next query action.
    pub records: VecDeque<PeerRecord>,
}

impl GetRecordContext {
    /// Create new [`GetRecordContext`].
    pub fn new(
        config: GetRecordConfig,
        in_peers: VecDeque<KademliaPeer>,
        local_record: bool,
    ) -> Self {
        let mut candidates = BTreeMap::new();

        for candidate in &in_peers {
            let distance = config.target.distance(&candidate.key);
            candidates.insert(distance, candidate.clone());
        }

        let kad_message = KademliaMessage::get_record(config.target.clone().into_preimage());

        Self {
            config,
            kad_message,

            candidates,
            pending: HashMap::new(),
            queried: HashSet::new(),
            found_records: if local_record { 1 } else { 0 },
            records: VecDeque::new(),
        }
    }

    /// Register response failure for `peer`.
    pub fn register_response_failure(&mut self, peer: PeerId) {
        let Some(peer) = self.pending.remove(&peer) else {
            tracing::debug!(
                target: LOG_TARGET,
                query = ?self.config.query,
                ?peer,
                "`GetRecordContext`: pending peer doesn't exist",
            );
            return;
        };

        self.queried.insert(peer.peer);
    }

    /// Register `GET_VALUE` response from `peer`.
    ///
    /// Returns some if the response should be propagated to the user.
    pub fn register_response(
        &mut self,
        peer: PeerId,
        record: Option<Record>,
        peers: Vec<KademliaPeer>,
    ) {
        tracing::trace!(
            target: LOG_TARGET,
            query = ?self.config.query,
            ?peer,
            "`GetRecordContext`: received response from peer",
        );

        let Some(peer) = self.pending.remove(&peer) else {
            tracing::debug!(
                target: LOG_TARGET,
                query = ?self.config.query,
                ?peer,
                "`GetRecordContext`: received response from peer but didn't expect it",
            );
            return;
        };

        if let Some(record) = record {
            if !record.is_expired(std::time::Instant::now()) {
                self.records.push_back(PeerRecord {
                    peer: peer.peer,
                    record,
                });

                self.found_records += 1;
            }
        }

        // Add the queried peer to `queried` and all new peers which haven't been
        // queried to `candidates`
        self.queried.insert(peer.peer);

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

    /// Register a failure of sending a `GET_VALUE` request to `peer`.
    pub fn register_send_failure(&mut self, _peer: PeerId) {
        // In case of a send failure, `register_response_failure` is called as well.
        // Failure is handled there.
    }

    /// Register a success of sending a `GET_VALUE` request to `peer`.
    pub fn register_send_success(&mut self, _peer: PeerId) {
        // `GET_VALUE` requests are compound request-response pairs of messages,
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
            "`GetRecordContext`: get next peer",
        );

        let (_, candidate) = self.candidates.pop_first()?;
        let peer = candidate.peer;

        tracing::trace!(
            target: LOG_TARGET,
            query = ?self.config.query,
            ?peer,
            "`GetRecordContext`: current candidate",
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

    /// Get next action for a `GET_VALUE` query.
    pub fn next_action(&mut self) -> Option<QueryAction> {
        // Drain the records first.
        if let Some(record) = self.records.pop_front() {
            return Some(QueryAction::GetRecordPartialResult {
                query_id: self.config.query,
                record,
            });
        }

        // These are the records we knew about before starting the query and
        // the records we found along the way.
        let known_records = self.config.known_records + self.found_records;

        // If we cannot make progress, return the final result.
        // A query failed when we are not able to identify one single record.
        if self.is_done() {
            return if known_records == 0 {
                Some(QueryAction::QueryFailed {
                    query: self.config.query,
                })
            } else {
                Some(QueryAction::QuerySucceeded {
                    query: self.config.query,
                })
            };
        }

        // Check if enough records have been found
        let sufficient_records = self.config.sufficient_records(self.found_records);
        if sufficient_records {
            return Some(QueryAction::QuerySucceeded {
                query: self.config.query,
            });
        }

        // At this point, we either have pending responses or candidates to query; and we need more
        // records. Ensure we do not exceed the parallelism factor.
        if self.pending.len() == self.config.parallelism_factor {
            return None;
        }

        self.schedule_next_peer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::libp2p::kademlia::types::ConnectionType;

    fn default_config() -> GetRecordConfig {
        GetRecordConfig {
            local_peer_id: PeerId::random(),
            quorum: Quorum::All,
            known_records: 0,
            replication_factor: 20,
            parallelism_factor: 10,
            query: QueryId(0),
            target: Key::new(vec![1, 2, 3].into()),
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

    #[test]
    fn config_check() {
        // Quorum::All with no known records.
        let config = GetRecordConfig {
            quorum: Quorum::All,
            known_records: 0,
            replication_factor: 20,
            ..default_config()
        };
        assert!(config.sufficient_records(20));
        assert!(!config.sufficient_records(19));

        // Quorum::All with 1 known records.
        let config = GetRecordConfig {
            quorum: Quorum::All,
            known_records: 1,
            replication_factor: 20,
            ..default_config()
        };
        assert!(config.sufficient_records(19));
        assert!(!config.sufficient_records(18));

        // Quorum::One with no known records.
        let config = GetRecordConfig {
            quorum: Quorum::One,
            known_records: 0,
            ..default_config()
        };
        assert!(config.sufficient_records(1));
        assert!(!config.sufficient_records(0));

        // Quorum::One with known records.
        let config = GetRecordConfig {
            quorum: Quorum::One,
            known_records: 1,
            ..default_config()
        };
        assert!(config.sufficient_records(1));
        assert!(config.sufficient_records(0));

        // Quorum::N with no known records.
        let config = GetRecordConfig {
            quorum: Quorum::N(std::num::NonZeroUsize::new(10).expect("valid; qed")),
            known_records: 0,
            ..default_config()
        };
        assert!(config.sufficient_records(10));
        assert!(!config.sufficient_records(9));

        // Quorum::N with known records.
        let config = GetRecordConfig {
            quorum: Quorum::N(std::num::NonZeroUsize::new(10).expect("valid; qed")),
            known_records: 1,
            ..default_config()
        };
        assert!(config.sufficient_records(9));
        assert!(!config.sufficient_records(8));
    }

    #[test]
    fn completes_when_no_candidates() {
        let config = default_config();
        let mut context = GetRecordContext::new(config, VecDeque::new(), false);
        assert!(context.is_done());
        let event = context.next_action().unwrap();
        match event {
            QueryAction::QueryFailed { query } => {
                assert_eq!(query, QueryId(0));
            }
            _ => panic!("Unexpected event"),
        }

        let config = GetRecordConfig {
            known_records: 1,
            ..default_config()
        };
        let mut context = GetRecordContext::new(config, VecDeque::new(), false);
        assert!(context.is_done());
        let event = context.next_action().unwrap();
        match event {
            QueryAction::QuerySucceeded { query } => {
                assert_eq!(query, QueryId(0));
            }
            _ => panic!("Unexpected event"),
        }
    }

    #[test]
    fn fulfill_parallelism() {
        let config = GetRecordConfig {
            parallelism_factor: 3,
            ..default_config()
        };

        let in_peers_set: HashSet<_> =
            [PeerId::random(), PeerId::random(), PeerId::random()].into_iter().collect();
        assert_eq!(in_peers_set.len(), 3);

        let in_peers = in_peers_set.iter().map(|peer| peer_to_kad(*peer)).collect();
        let mut context = GetRecordContext::new(config, in_peers, false);

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
    fn completes_when_responses() {
        let key = vec![1, 2, 3];
        let config = GetRecordConfig {
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
        let mut context = GetRecordContext::new(config, in_peers, false);

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

        let mut found_records = Vec::new();
        // Provide responses back.
        let record = Record::new(key.clone(), vec![1, 2, 3]);
        context.register_response(peer_a, Some(record), vec![]);
        // Check propagated action.
        let record = context.next_action().unwrap();
        match record {
            QueryAction::GetRecordPartialResult { query_id, record } => {
                assert_eq!(query_id, QueryId(0));
                assert_eq!(record.peer, peer_a);
                assert_eq!(record.record, Record::new(key.clone(), vec![1, 2, 3]));

                found_records.push(record);
            }
            _ => panic!("Unexpected event"),
        }

        assert_eq!(context.pending.len(), 2);
        assert_eq!(context.queried.len(), 1);
        assert_eq!(context.found_records, 1);

        // Provide different response from peer b with peer d as candidate.
        let record = Record::new(key.clone(), vec![4, 5, 6]);
        context.register_response(peer_b, Some(record), vec![peer_to_kad(peer_d)]);
        // Check propagated action.
        let record = context.next_action().unwrap();
        match record {
            QueryAction::GetRecordPartialResult { query_id, record } => {
                assert_eq!(query_id, QueryId(0));
                assert_eq!(record.peer, peer_b);
                assert_eq!(record.record, Record::new(key.clone(), vec![4, 5, 6]));

                found_records.push(record);
            }
            _ => panic!("Unexpected event"),
        }

        assert_eq!(context.pending.len(), 1);
        assert_eq!(context.queried.len(), 2);
        assert_eq!(context.found_records, 2);
        assert_eq!(context.candidates.len(), 1);

        // Peer C fails.
        context.register_response_failure(peer_c);
        assert!(context.pending.is_empty());
        assert_eq!(context.queried.len(), 3);
        assert_eq!(context.found_records, 2);

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
        let record = Record::new(key.clone(), vec![4, 5, 6]);
        context.register_response(peer_d, Some(record), vec![]);
        // Check propagated action.
        let record = context.next_action().unwrap();
        match record {
            QueryAction::GetRecordPartialResult { query_id, record } => {
                assert_eq!(query_id, QueryId(0));
                assert_eq!(record.peer, peer_d);
                assert_eq!(record.record, Record::new(key.clone(), vec![4, 5, 6]));

                found_records.push(record);
            }
            _ => panic!("Unexpected event"),
        }

        // Produces the result.
        let event = context.next_action().unwrap();
        match event {
            QueryAction::QuerySucceeded { query } => {
                assert_eq!(query, QueryId(0));
            }
            _ => panic!("Unexpected event"),
        }

        // Check results.
        assert_eq!(
            found_records,
            vec![
                PeerRecord {
                    peer: peer_a,
                    record: Record::new(key.clone(), vec![1, 2, 3]),
                },
                PeerRecord {
                    peer: peer_b,
                    record: Record::new(key.clone(), vec![4, 5, 6]),
                },
                PeerRecord {
                    peer: peer_d,
                    record: Record::new(key.clone(), vec![4, 5, 6]),
                },
            ]
        );
    }
}
