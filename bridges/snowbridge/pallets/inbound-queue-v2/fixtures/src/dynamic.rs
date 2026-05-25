// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Builder for synthesizing dynamically-sized inbound-queue v2 benchmark fixtures.
//!
//! Reuses the SSZ / merkle / receipts-trie machinery from the v1 dynamic fixture by
//! supplying a [`LogTemplate`] whose `data_builder` produces an ABI-encoded
//! `IGatewayV2::OutboundMessageAccepted` payload of the requested length.
//!
//! v2 differs from v1 in two important ways:
//! - The log topics array is just `[OutboundMessageAccepted::SIGNATURE_HASH]`.
//! - The `data` field must decode via `decode_raw_log_validate`; we cannot just append zero bytes
//!   to a fixed prefix. Instead the builder grows the variable-length `xcm.data` field inside the
//!   payload to reach the target byte count, which keeps the ABI encoding valid throughout.
#![cfg(feature = "runtime-benchmarks")]

use alloc::{boxed::Box, vec, vec::Vec};
use alloy_core::sol_types::SolEvent;
use alloy_primitives::{Address, Bytes, B256};
use snowbridge_inbound_queue_primitives::v2::IGatewayV2;
use snowbridge_pallet_ethereum_client_fixtures::dynamic::{
	build_dynamic_fixture_with_log, DynamicFixture, LogTemplate,
};
use sp_core::H256;

extern crate alloc;

/// Gateway address used by the v2 benchmark — matches the address baked into the
/// generated `make_register_token_message` v2 fixture.
const GATEWAY_ADDRESS: [u8; 20] = hex_literal::hex!("b1185ede04202fe62d38f5db72f71e38ff3e8305");

/// Build a synthetic v2 `EventFixture` whose receipt proof has `n` trie nodes and whose
/// receipt body is approximately `s` bytes. The caller is responsible for storing the
/// returned `finalized_block_root` and matching `CompactBeaconState` (slot
/// `BENCH_SLOT + 1`, `block_roots_root` derived from the proof) in ethereum-client storage.
pub fn build_dynamic_fixture(n: u32, s: u32) -> DynamicFixture {
	build_dynamic_fixture_with_log(n, s, v2_log_template())
}

/// `LogTemplate` that produces ABI-encoded `IGatewayV2::OutboundMessageAccepted` payloads
/// of arbitrary size by padding the variable-length `xcm.data` field.
fn v2_log_template() -> LogTemplate {
	let topic0 = IGatewayV2::OutboundMessageAccepted::SIGNATURE_HASH;
	let topic0_h256 = H256::from_slice(topic0.as_slice());
	LogTemplate {
		gateway: GATEWAY_ADDRESS,
		topics: vec![topic0_h256],
		data_builder: Box::new(|target_len| build_v2_payload_data(target_len)),
		filler_log_data_builder: None,
	}
}

/// Build an ABI-encoded `OutboundMessageAccepted` event whose total length is roughly
/// `target_len` bytes by sizing the `xcm.data` field. The `decode_raw_log_validate` call
/// in the v2 verifier expects a syntactically-valid ABI payload; we cannot pad with raw
/// zeros, but we can grow `xcm.data` (a variable-length `bytes` field).
fn build_v2_payload_data(target_len: usize) -> Vec<u8> {
	// First, compute the ABI overhead of the payload around an empty `xcm.data`. This
	// gives us the constant header cost; the rest is xcm.data plus the bytes-length
	// prefix (which itself is 32-byte-padded).
	let baseline = encode_payload_with_xcm_data_len(0);
	if target_len <= baseline.len() {
		return baseline;
	}

	// Each extra `target_len - baseline.len()` byte adds one byte to xcm.data, plus a
	// 32-byte rounding overhead at boundaries. Iterate to converge on the exact size.
	let mut xcm_data_len = target_len.saturating_sub(baseline.len());
	let mut last = encode_payload_with_xcm_data_len(xcm_data_len);
	for _ in 0..16 {
		if last.len() == target_len {
			return last;
		}
		if last.len() < target_len {
			xcm_data_len = xcm_data_len.saturating_add(target_len - last.len());
		} else {
			xcm_data_len = xcm_data_len.saturating_sub(last.len() - target_len);
		}
		last = encode_payload_with_xcm_data_len(xcm_data_len);
	}
	last
}

/// Encode a `OutboundMessageAccepted` event whose `xcm.data` field is `xcm_data_len` zero
/// bytes. The `executionFee` is set to a non-zero placeholder because the v2 message-to-XCM
/// converter rejects zero-fee messages (it constructs a fungible XCM asset out of the fee
/// and the v5 `Asset` constructor asserts non-zero amount).
fn encode_payload_with_xcm_data_len(xcm_data_len: usize) -> Vec<u8> {
	let xcm_bytes = vec![0u8; xcm_data_len];
	let event = IGatewayV2::OutboundMessageAccepted {
		nonce: 1u64,
		payload: IGatewayV2::Payload {
			origin: Address::from([0u8; 20]),
			assets: vec![],
			xcm: IGatewayV2::Xcm { kind: 0, data: Bytes::from(xcm_bytes) },
			claimer: Bytes::new(),
			value: 0,
			executionFee: 1,
			relayerFee: 0,
		},
	};
	event.encode_data()
}

// Re-export for convenience: callers in runtimes can grab the same gateway address used
// by the benchmark fixture.
pub const fn gateway_address() -> [u8; 20] {
	GATEWAY_ADDRESS
}

// Use `Address::from([u8; 20])` and `B256` so they aren't reported as unused under any
// feature combination.
const _: fn() = || {
	let _: Address = Address::from([0u8; 20]);
	let _: B256 = B256::ZERO;
};
