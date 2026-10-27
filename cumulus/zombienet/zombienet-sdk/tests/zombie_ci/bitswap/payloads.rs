// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Deterministic bulletin payloads and their CIDs, shared by the bitswap consumer test (`e2e`)
//! and the snapshot generator (`generate_snapshot`).
//!
//! Ported from smoldot's `e2e-tests/src/bulletin.rs` so both repos inject the same content and
//! agree on the resulting CIDs. The one deviation: CID construction here uses the `cid` crate +
//! `sp-crypto-hashing` (smoldot uses its own `smoldot::libp2p::cid`). The unit test at the bottom
//! pins that our CIDs reproduce the values smoldot serves, so the two implementations can't drift.
//!
//! A CID served by `pallet-transaction-storage` over bitswap is `CIDv1(raw, blake2b-256)` of the
//! stored bytes — that is what [`predicted_cid`] reconstructs.

#![allow(dead_code)] // Some items are only used by the generator, others only by the consumer.

use cid::{multihash::Multihash, Cid};

/// Para id of the bulletin chain (matches smoldot's snapshots).
pub const PARA_ID: u32 = 2487;

/// Relay chain spec name.
pub const RELAY_CHAIN: &str = "westend-local";
/// Polkadot relay binary expected on `$PATH`.
pub const RELAY_BINARY: &str = "polkadot";
/// Parachain binary expected on `$PATH`. Loads the bulletin runtime from the chain spec.
pub const PARA_BINARY: &str = "polkadot-parachain";

/// Snapshot height target for the generator. Must exceed 1000 blocks.
pub const DEFAULT_SNAPSHOT_HEIGHT: u64 = 1024;

/// `raw` multicodec — the codec `pallet-transaction-storage` content is served under.
const RAW_CODEC: u64 = 0x55;
/// blake2b-256 multihash code.
const BLAKE2B_256: u64 = 0xb220;

// Canonical CID strings, cross-checked against smoldot in `cids_match_smoldot` below. The consumer
// test imports these instead of hardcoding its own copies.
/// CID of `payloads()[0]` (26-byte payload).
pub const CID_26B: &str = "bafk2bzacec6y4g7jkuw4a56nhgwujo64ajczzr6eijlsjb47ydcmoit4qcwqc";
/// CID of `payloads()[1]` (4 KiB payload).
pub const CID_4KIB: &str = "bafk2bzacebtgbe4obl6uzfoykcsigmounzfvycajptfeqjasfyukzjzxp5nli";
/// CID of `payloads()[2]` (31-byte payload).
pub const CID_31B: &str = "bafk2bzaceakzpr62fygyiyigr3thmkgfeyh5l3dlotse7pmwhbvtapx6yp4ow";

/// Expected payload sizes (bytes). Hex envelope on the wire is `0x` + 2 chars / byte.
pub const CID_26B_BYTES: usize = 26;
pub const CID_4KIB_BYTES: usize = 4 * 1024;

/// One injected payload. The generator must `transactionStorage::authorize_account` the submitting
/// account before any `store` extrinsic succeeds. Per-tx ceiling is 2 MiB.
pub struct Payload {
	pub label: &'static str,
	pub content: &'static [u8],
}

impl Payload {
	/// CIDv1(raw, blake2b-256) of the content — the CID under which it is served over bitswap.
	pub fn predicted_cid(&self) -> String {
		predicted_cid(self.content)
	}

	pub fn size(&self) -> u64 {
		self.content.len() as u64
	}
}

/// Build the `CIDv1(raw, blake2b-256)` string for arbitrary content.
pub fn predicted_cid(content: &[u8]) -> String {
	let digest = sp_crypto_hashing::blake2_256(content);
	let mh =
		Multihash::<64>::wrap(BLAKE2B_256, &digest).expect("32-byte digest fits Multihash<64>");
	Cid::new_v1(RAW_CODEC, mh).to_string()
}

/// Deterministic payloads the generator injects and the CI tests assert on.
pub fn payloads() -> Vec<Payload> {
	vec![
		Payload { label: "payload-26b", content: b"smoldot-bitswap-both-small" },
		Payload { label: "payload-4kib", content: rand_4k() },
		Payload { label: "payload-31b", content: b"smoldot-bitswap-full-only-small" },
		Payload { label: "payload-1mib", content: rand_1m() },
	]
}

/// 4 KiB pseudo-random payload, deterministic from a fixed seed.
fn rand_4k() -> &'static [u8] {
	static BUF: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
	BUF.get_or_init(|| xorshift_fill(0xdead_beef_dead_beef_u64, 4 * 1024))
		.as_slice()
}

/// 1 MiB pseudo-random payload, deterministic from a different seed.
fn rand_1m() -> &'static [u8] {
	static BUF: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
	BUF.get_or_init(|| xorshift_fill(0xfeed_face_cafe_babe_u64, 1024 * 1024))
		.as_slice()
}

/// xorshift64 stream, matching smoldot's `bulletin.rs` byte-for-byte so the CIDs line up.
fn xorshift_fill(seed: u64, len: usize) -> Vec<u8> {
	let mut state = seed.max(1);
	let mut out = Vec::with_capacity(len);
	while out.len() < len {
		state ^= state << 13;
		state ^= state >> 7;
		state ^= state << 17;
		out.extend_from_slice(&state.to_le_bytes());
	}
	out.truncate(len);
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Guards that our `cid`-crate CID construction reproduces exactly the CIDs smoldot serves for
	/// the same payloads. If this fails, the generator and consumer disagree with the bulletin
	/// chain and every downstream assertion is meaningless.
	#[test]
	fn cids_match_smoldot() {
		let p = payloads();
		assert_eq!(p[0].predicted_cid(), CID_26B, "26b payload CID");
		assert_eq!(p[1].predicted_cid(), CID_4KIB, "4kib payload CID");
		assert_eq!(p[2].predicted_cid(), CID_31B, "31b payload CID");

		assert_eq!(p[0].size(), CID_26B_BYTES as u64);
		assert_eq!(p[1].size(), CID_4KIB_BYTES as u64);
	}
}
