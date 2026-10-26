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

//! Per-statement decisions of the v2 DHT topology: forwarding targets of a topic and the local
//! node's retention affinity, on a topology the size of a live network.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::{rngs::StdRng, Rng, SeedableRng};
use sc_network_statement::{DhtAffinity, PeersTopology, PeersTopologyConfig};
use sc_network_types::PeerId;
use sp_statement_store::{Statement, Topic};
use std::num::NonZeroUsize;

const CONNECTED_PEERS: [usize; 3] = [50, 200, 1000];
const KNOWN_PEERS: [usize; 3] = [512, 1024, 2048];
const ROUTING_KNOWN_PEERS: usize = 2048;
const TOPICS: usize = 64;
const REPLICATION_FACTOR: usize = 20;
const GOSSIP_TARGET: usize = 3;

fn nz(n: usize) -> NonZeroUsize {
	NonZeroUsize::new(n).expect("non-zero")
}

fn peer(rng: &mut StdRng) -> PeerId {
	let mut bytes = [0u8; 34];
	bytes[1] = 32;
	rng.fill(&mut bytes[2..]);
	PeerId::from_bytes(&bytes).expect("identity multihash peer id")
}

fn topics(rng: &mut StdRng) -> Vec<Topic> {
	(0..TOPICS).map(|_| Topic(rng.gen())).collect()
}

fn topology(rng: &mut StdRng, known: usize, connected: usize) -> PeersTopology {
	let config = PeersTopologyConfig {
		replication_factor: nz(REPLICATION_FACTOR),
		gossip_target: nz(GOSSIP_TARGET),
	};
	let mut topology = PeersTopology::new(peer(rng), config);
	for i in 0..known {
		let peer = peer(rng);
		if i < connected {
			topology.on_substream_opened(peer);
		} else {
			topology.on_peer_identified(peer, true);
		}
	}
	topology
}

fn routing_targets(c: &mut Criterion) {
	let mut group = c.benchmark_group("routing_targets");
	group.throughput(Throughput::Elements(TOPICS as u64));
	for connected in CONNECTED_PEERS {
		let mut rng = StdRng::seed_from_u64(connected as u64);
		let topology = topology(&mut rng, ROUTING_KNOWN_PEERS, connected);
		let topics = topics(&mut rng);
		let expected: Vec<usize> =
			topics.iter().map(|topic| topology.routing_targets(*topic).len()).collect();
		assert!(expected.iter().sum::<usize>() > 0, "fixture must yield some targets");
		assert!(expected.iter().all(|len| *len <= REPLICATION_FACTOR + GOSSIP_TARGET));

		group.bench_with_input(BenchmarkId::from_parameter(connected), &topology, |b, topology| {
			b.iter(|| {
				for (topic, expected) in topics.iter().zip(&expected) {
					assert_eq!(topology.routing_targets(*topic).len(), *expected);
				}
			})
		});
	}
	group.finish();
}

fn is_affine(c: &mut Criterion) {
	let mut group = c.benchmark_group("is_affine");
	group.throughput(Throughput::Elements(TOPICS as u64));
	for known in KNOWN_PEERS {
		let mut rng = StdRng::seed_from_u64(known as u64);
		let oracle: DhtAffinity = topology(&mut rng, known, 0).dht_affinity();
		let statements: Vec<Statement> = topics(&mut rng)
			.into_iter()
			.map(|topic| {
				let mut statement = Statement::new();
				statement.set_topic(0, topic);
				statement
			})
			.collect();
		let expected: Vec<bool> = statements.iter().map(|stmt| oracle.is_affine(stmt)).collect();

		group.bench_with_input(BenchmarkId::from_parameter(known), &oracle, |b, oracle| {
			b.iter(|| {
				for (statement, expected) in statements.iter().zip(&expected) {
					assert_eq!(oracle.is_affine(statement), *expected);
				}
			})
		});
	}
	group.finish();
}

criterion_group!(benches, routing_targets, is_affine);
criterion_main!(benches);
