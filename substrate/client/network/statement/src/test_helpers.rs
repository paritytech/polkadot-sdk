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

//! Fixtures shared across the crate's test modules.

use crate::{
	affinity::AffinityFilter, config::DEFAULT_BLOOM_FALSE_POS_RATE,
	v2dht::peers_topology::PeersTopologyConfig,
};
use sc_network_types::PeerId;
use sp_statement_store::{Statement, Topic};
use std::num::NonZeroUsize;

/// A topic whose 32 bytes are all `n`.
pub(crate) fn topic(n: u8) -> Topic {
	Topic([n; 32])
}

/// A `NonZeroUsize`, for terse test fixtures.
pub(crate) fn nz(n: usize) -> NonZeroUsize {
	NonZeroUsize::new(n).expect("non-zero")
}

/// A deterministic peer whose identity multihash is seeded by `seed`.
pub(crate) fn peer(seed: u8) -> PeerId {
	let mut bytes = [seed; 34];
	bytes[0] = 0;
	bytes[1] = 32;
	PeerId::from_bytes(&bytes).expect("identity multihash peer id")
}

/// A statement carrying the single `topic`.
pub(crate) fn statement_on(topic: Topic) -> Statement {
	let mut stmt = Statement::new();
	stmt.set_plain_data(b"data".to_vec());
	stmt.set_topic(0, topic);
	stmt
}

/// A filter advertising the given `topics`.
pub(crate) fn filter_over(topics: &[Topic]) -> AffinityFilter {
	AffinityFilter::from_topics(
		topics.iter().map(|topic| topic.as_ref()),
		0,
		DEFAULT_BLOOM_FALSE_POS_RATE,
	)
}

/// A topology config with the given `replication_factor` and `gossip_target`.
pub(crate) fn topology_config(
	replication_factor: usize,
	gossip_target: usize,
) -> PeersTopologyConfig {
	PeersTopologyConfig {
		replication_factor: NonZeroUsize::new(replication_factor).expect("non-zero"),
		gossip_target: NonZeroUsize::new(gossip_target).expect("non-zero"),
	}
}
