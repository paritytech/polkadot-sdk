// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Thin v1-specific entry on top of the shared dynamic-fixture machinery in
//! `snowbridge_pallet_ethereum_client_fixtures::dynamic`.
//!
//! All the SSZ / merkle / receipts-trie work lives in the shared crate; this module just
//! supplies the v1 gateway address, topics, and `data` builder.
#![cfg(feature = "runtime-benchmarks")]

extern crate alloc;

use alloc::{boxed::Box, vec, vec::Vec};
use snowbridge_core::ChannelId;
use snowbridge_pallet_ethereum_client_fixtures::dynamic::{
	build_dynamic_fixture_with_log, LogTemplate,
};
use sp_core::H256;

pub use snowbridge_pallet_ethereum_client_fixtures::dynamic::DynamicFixture;

const GATEWAY_ADDRESS: [u8; 20] = hex_literal::hex!("eda338e4dc46038493b885327842fd3e301cab39");
const CHANNEL_ID: H256 =
	H256(hex_literal::hex!("c173fac324158e77fb5840738a1a541f633cbec8884c6a601c567d2b376a0539"));
/// Same value as `CHANNEL_ID`, typed as a [`ChannelId`]. The runtime helper registers a
/// matching channel in `EthereumSystem::Channels` so that `submit`'s `ChannelLookup`
/// resolves successfully under the benchmark fixture.
pub const CHANNEL_ID_AS_CHANNEL_ID: ChannelId = ChannelId::new(hex_literal::hex!(
	"c173fac324158e77fb5840738a1a541f633cbec8884c6a601c567d2b376a0539"
));
const MESSAGE_ID: H256 =
	H256(hex_literal::hex!("5f7060e971b0dc81e63f0aa41831091847d97c1a4693ac450cc128c7214e65e0"));
/// Topic0 of the OutboundMessageAccepted event.
const TOPIC0: H256 =
	H256(hex_literal::hex!("7153f9357c8ea496bba60bf82e67143e27b64462b49041f8e689e1b05728f84f"));

/// Build a synthetic v1 `EventFixture` whose receipt proof has `n` trie nodes and whose
/// receipt body is approximately `s` bytes. The caller is responsible for storing the
/// returned `finalized_block_root` and matching `CompactBeaconState` in
/// ethereum-client storage.
pub fn build_dynamic_fixture(n: u32, s: u32) -> DynamicFixture {
	build_dynamic_fixture_with_log(n, s, default_v1_log_template())
}

/// Default v1 `LogTemplate` — matches the gateway address and event topology the
/// inbound-queue v1 verifier expects (channel id at topic1, message id at topic2,
/// `RegisterToken` versioned message inside `data`).
pub fn default_v1_log_template() -> LogTemplate {
	LogTemplate {
		gateway: GATEWAY_ADDRESS,
		topics: vec![TOPIC0, CHANNEL_ID, MESSAGE_ID],
		data_builder: Box::new(|target_len| {
			// V1 just byte-compares Log::data; pad a fixed RegisterToken VersionedMessage
			// prefix with trailing zero bytes to reach `target_len`.
			let prefix = build_outbound_message_data();
			let mut data = Vec::with_capacity(target_len.max(prefix.len()));
			data.extend_from_slice(&prefix);
			if target_len > data.len() {
				data.resize(target_len, 0);
			}
			data
		}),
		filler_log_data_builder: None,
	}
}

/// 96 bytes of payload that decodes as a `RegisterToken` versioned message. Used as the
/// initial bytes of every receipt's log data so the inbound-queue's `decode_all` of the
/// `VersionedMessage` succeeds.
fn build_outbound_message_data() -> Vec<u8> {
	hex_literal::hex!(
		"00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000002e00a736aa00000000000087d1f7fdfee7f651fabc8bfcb6e086c278b77a7d00e40b54020000000000000000000000000000000000000000000000000000000000"
	)
	.to_vec()
}
