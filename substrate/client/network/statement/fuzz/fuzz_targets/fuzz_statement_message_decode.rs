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
extern crate sp_statement_store;

use codec::{Decode, Encode};
use libfuzzer_sys::fuzz_target;
use sc_network_statement::StatementMessage;

// Fuzz StatementMessage::decode() with raw bytes, simulating an untrusted V2
// protocol notification from a peer. This is the top-level decode entry point
// for all incoming V2 messages in the statement gossip protocol.
//
// The enum dispatches to:
// - variant 0: Vec<Statement> (each Statement has a custom SCALE decoder)
// - variant 1: AffinityFilter (custom decode with bloom filter validation)
// - any other variant byte: should return a decode error
//
// Any panic here is a DoS vector — a malicious peer could crash any node
// running the statement protocol.
fuzz_target!(|data: &[u8]| {
	let result = StatementMessage::decode(&mut &data[..]);

	if let Ok(msg) = result {
		// Re-encoding must not panic
		let encoded = msg.encode();

		// Re-decoding must succeed
		let redecoded = StatementMessage::decode(&mut &encoded[..])
			.expect("re-encoding a valid StatementMessage must produce decodable output");

		// Re-encoding must be deterministic
		let reencoded = redecoded.encode();
		assert_eq!(encoded, reencoded, "roundtrip through encode/decode must be stable");

		// Variant-specific accessor checks — must never panic
		match msg {
			StatementMessage::Statements(ref stmts) => {
				for stmt in stmts {
					let _ = stmt.proof();
					let _ = stmt.account_id();
					let _ = stmt.channel();
					let _ = stmt.expiry();
					let _ = stmt.data();
					let _ = stmt.data_len();
					let _ = stmt.topics();
				}
			},
			StatementMessage::ExplicitTopicAffinity(ref filter) => {
				let test_topic: [u8; 32] = [0xAA; 32];
				let _ = filter.contains(&test_topic);
			},
		}
	}
});
