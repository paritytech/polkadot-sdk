// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Builder for synthesizing dynamically-sized outbound-queue v2 benchmark fixtures.
//!
//! Reuses the SSZ / merkle / receipts-trie machinery from
//! `snowbridge_pallet_ethereum_client_fixtures::dynamic` by supplying a [`LogTemplate`]
//! whose `data_builder` produces an ABI-encoded `InboundMessageDispatched` event payload.
//!
//! Unlike the v2 inbound event, `InboundMessageDispatched` has fixed-size data (96 bytes:
//! `bytes32 + bool + bytes32`). The matching log cannot grow, so we use
//! `LogTemplate::filler_log_data_builder` to attach a second zero-address log whose data
//! field scales with the requested receipt size `s`. The verifier finds the matching
//! `InboundMessageDispatched` log at index 0 and returns success; the per-byte cost the
//! benchmark observes still scales with `s` because the receipt envelope decoder reads
//! the entire RLP blob, including the filler log.
#![cfg(feature = "runtime-benchmarks")]

use alloc::{boxed::Box, vec, vec::Vec};
use alloy_core::{primitives::U256, sol_types::SolEvent};
use snowbridge_outbound_queue_primitives::v2::InboundMessageDispatched;
use snowbridge_pallet_ethereum_client_fixtures::dynamic::{
	build_dynamic_fixture_with_log, DynamicFixture, LogTemplate,
};
use sp_core::H256;

extern crate alloc;

/// Gateway address used by the v2 outbound benchmark — matches the address baked into
/// the existing static fixture and the runtime's `EthereumGatewayAddress`.
const GATEWAY_ADDRESS: [u8; 20] = hex_literal::hex!("b1185ede04202fe62d38f5db72f71e38ff3e8305");

/// Nonce used by the dynamic fixture. The benchmark inserts a `PendingOrder` keyed by this
/// nonce before invoking `submit_delivery_receipt`.
pub const BENCH_NONCE: u64 = 1;

/// Build a synthetic `EventFixture` for the outbound v2 `submit_delivery_receipt`
/// benchmark. The returned `EventFixture` carries an `InboundMessageDispatched` log
/// for [`BENCH_NONCE`]; the caller must register a matching `PendingOrder` and prime
/// `FinalizedBeaconState` + `LatestFinalizedBlockRoot` (the
/// [`build_dynamic_fixture_with_log`] caller in the runtime helper does this).
pub fn build_dynamic_fixture(n: u32, s: u32) -> DynamicFixture {
	build_dynamic_fixture_with_log(n, s, outbound_v2_log_template())
}

/// `LogTemplate` for an `InboundMessageDispatched` event. Topics are
/// `[SIGNATURE_HASH, padded_nonce]`. Data is fixed at 96 bytes (the ABI encoding of
/// `topic, success, reward_address`); the receipt is grown with a filler log instead.
fn outbound_v2_log_template() -> LogTemplate {
	let topic0 = InboundMessageDispatched::SIGNATURE_HASH;
	let topic0_h256 = H256::from_slice(topic0.as_slice());
	let mut nonce_topic = [0u8; 32];
	nonce_topic[24..32].copy_from_slice(&BENCH_NONCE.to_be_bytes());
	LogTemplate {
		gateway: GATEWAY_ADDRESS,
		topics: vec![topic0_h256, H256(nonce_topic)],
		// Matching log's data is always the fixed 96-byte payload.
		data_builder: Box::new(|_| build_inbound_message_dispatched_data()),
		// Receipt size scales with a filler log's data.
		filler_log_data_builder: Some(Box::new(|target_len| vec![0u8; target_len])),
	}
}

/// ABI-encode the non-indexed fields of `InboundMessageDispatched`: `topic`, `success`,
/// `reward_address`. The encoding is `bytes32 || bool-as-bytes32 || bytes32 = 96 bytes`.
fn build_inbound_message_dispatched_data() -> Vec<u8> {
	let mut out = Vec::with_capacity(96);
	// topic = bytes32(0)
	out.extend_from_slice(&[0u8; 32]);
	// success = bool(true) -> 32-byte big-endian 1
	let success_word = U256::from(1u8).to_be_bytes::<32>();
	out.extend_from_slice(&success_word);
	// reward_address = bytes32(0) (relayer == origin signer)
	out.extend_from_slice(&[0u8; 32]);
	out
}

// Keep the U256 import referenced under any feature combination.
const _: fn() = || {
	let _ = U256::from(0u8);
};
