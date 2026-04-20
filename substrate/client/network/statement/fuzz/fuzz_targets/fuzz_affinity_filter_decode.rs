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

extern crate codec;
extern crate libfuzzer_sys;
extern crate sc_network_statement;

use codec::{Decode, Encode};
use libfuzzer_sys::fuzz_target;
use sc_network_statement::affinity::AffinityFilter;

// Fuzz AffinityFilter::decode() with raw bytes, simulating untrusted network input
// from a V2 peer sending an ExplicitTopicAffinity message. The decode path validates
// bloom filter parameters (bits length, num_hashes range).
fuzz_target!(|data: &[u8]| {
	let result = AffinityFilter::decode(&mut &data[..]);

	if let Ok(filter) = result {
		// contains() must not panic on any successfully decoded filter
		let test_topic: [u8; 32] = [0xAA; 32];
		let _ = filter.contains(&test_topic);

		// Re-encoding must not panic
		let encoded = filter.encode();

		// Re-decoding the re-encoded filter must succeed
		let redecoded = AffinityFilter::decode(&mut &encoded[..])
			.expect("re-encoding a valid AffinityFilter must produce decodable output");

		let reencoded = redecoded.encode();
		assert_eq!(encoded, reencoded);
	}
});
