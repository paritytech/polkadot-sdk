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

#![no_main]

extern crate arbitrary;
extern crate codec;
extern crate libfuzzer_sys;
extern crate sc_network_statement;

use arbitrary::Arbitrary;
use codec::{Decode, Encode};
use libfuzzer_sys::fuzz_target;
use sc_network_statement::affinity::AffinityFilter;

#[derive(Arbitrary, Debug)]
struct FuzzInput {
	seed: u128,
	/// Clamped to [1, 10_000] inside the target
	expected_items: u16,
	/// Topics to insert into the bloom filter
	topics: Vec<[u8; 32]>,
	/// Topics to query after roundtrip (may or may not be in the filter)
	query_topics: Vec<[u8; 32]>,
}

// Fuzz the AffinityFilter encode/decode roundtrip with structured input.
// Constructs a valid bloom filter, inserts topics, encodes, decodes, and verifies
// that membership queries are preserved.
fuzz_target!(|input: FuzzInput| {
	let expected_items = (input.expected_items as usize).max(1).min(10_000);

	let topics: Vec<_> = input.topics.into_iter().take(1_000).collect();
	let query_topics: Vec<_> = input.query_topics.into_iter().take(100).collect();

	let mut filter = AffinityFilter::new(input.seed, 0.01, expected_items);

	for topic in &topics {
		filter.insert(topic);
	}

	// Record membership answers before roundtrip
	let inserted_answers: Vec<bool> = topics.iter().map(|t| filter.contains(t)).collect();
	let query_answers: Vec<bool> = query_topics.iter().map(|t| filter.contains(t)).collect();

	// All inserted topics must be present (bloom filter: no false negatives)
	for (i, &present) in inserted_answers.iter().enumerate() {
		assert!(present);
	}

	// Encode then decode
	let encoded = filter.encode();
	let decoded = AffinityFilter::decode(&mut &encoded[..])
		.expect("decoding a freshly encoded AffinityFilter must succeed");

	// Membership answers must be identical after roundtrip
	for (i, topic) in topics.iter().enumerate() {
		assert_eq!(
			decoded.contains(topic),
			inserted_answers[i],
			"inserted topic {i} membership changed after roundtrip"
		);
	}
	for (i, topic) in query_topics.iter().enumerate() {
		assert_eq!(decoded.contains(topic), query_answers[i]);
	}

	assert_eq!(encoded, decoded.encode());
});
