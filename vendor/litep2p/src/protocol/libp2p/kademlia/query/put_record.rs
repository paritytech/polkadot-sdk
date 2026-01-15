// Copyright 2025 litep2p developers
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
    protocol::libp2p::kademlia::{handle::Quorum, query::QueryAction, QueryId, RecordKey},
    PeerId,
};

use std::{cmp, collections::HashSet};

/// Logging target for this file.
const LOG_TARGET: &str = "litep2p::ipfs::kademlia::query::put_record";

/// Context for tracking `PUT_VALUE` responses from peers.
#[derive(Debug)]
pub struct PutRecordToFoundNodesContext {
    /// Query ID.
    pub query: QueryId,

    /// Record key.
    pub key: RecordKey,

    /// Quorum that needs to be reached for the query to succeed.
    peers_to_succeed: usize,

    /// Peers we're waiting for responses from.
    pending_peers: HashSet<PeerId>,

    /// Number of successfully responded peers.
    n_succeeded: usize,
}

impl PutRecordToFoundNodesContext {
    /// Create new [`PutRecordToFoundNodesContext`].
    pub fn new(query: QueryId, key: RecordKey, peers: Vec<PeerId>, quorum: Quorum) -> Self {
        Self {
            query,
            key,
            peers_to_succeed: match quorum {
                Quorum::One => 1,
                // Clamp by the number of discovered peers. This should ever be relevant on
                // small networks with fewer peers than the replication factor. Without such
                // clamping the query would always fail in small testnets.
                Quorum::N(n) => cmp::min(n.get(), cmp::max(peers.len(), 1)),
                Quorum::All => cmp::max(peers.len(), 1),
            },
            pending_peers: peers.into_iter().collect(),
            n_succeeded: 0,
        }
    }

    /// Register successful response from peer.
    pub fn register_response(&mut self, peer: PeerId) {
        if self.pending_peers.remove(&peer) {
            self.n_succeeded += 1;

            tracing::trace!(
                target: LOG_TARGET,
                query = ?self.query,
                ?peer,
                "successful `PUT_VALUE` to peer",
            );
        } else {
            tracing::debug!(
                target: LOG_TARGET,
                query = ?self.query,
                ?peer,
                "`PutRecordToFoundNodesContext::register_response`: pending peer does not exist",
            );
        }
    }

    /// Register failed response from peer.
    pub fn register_response_failure(&mut self, peer: PeerId) {
        if self.pending_peers.remove(&peer) {
            tracing::trace!(
                target: LOG_TARGET,
                query = ?self.query,
                ?peer,
                "failed `PUT_VALUE` to peer",
            );
        } else {
            tracing::debug!(
                target: LOG_TARGET,
                query = ?self.query,
                ?peer,
                "`PutRecordToFoundNodesContext::register_response_failure`: pending peer does not exist",
            );
        }
    }

    /// Check if all responses have been received.
    pub fn is_finished(&self) -> bool {
        self.pending_peers.is_empty()
    }

    /// Check if all requests were successful.
    pub fn is_succeded(&self) -> bool {
        self.n_succeeded >= self.peers_to_succeed
    }

    /// Get next action if the context is finished.
    pub fn next_action(&self) -> Option<QueryAction> {
        if self.is_finished() {
            if self.is_succeded() {
                Some(QueryAction::QuerySucceeded { query: self.query })
            } else {
                Some(QueryAction::QueryFailed { query: self.query })
            }
        } else {
            None
        }
    }
}
