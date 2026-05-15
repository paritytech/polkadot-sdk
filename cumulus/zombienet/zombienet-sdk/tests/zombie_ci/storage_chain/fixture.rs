// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use super::common::ParachainSnapshots;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(feature = "generate-snapshots")]
use std::path::Path;
use std::path::PathBuf;

pub const FIXTURE_RETENTION_PERIOD: u32 = 200;
pub const TIP_SYNC_TARGET_BLOCKS: u64 = 100;
pub const N_STORES: u32 = 30;

pub const PAYLOAD_SIZE_MIN: usize = 512 * 1024;
pub const PAYLOAD_SIZE_MAX: usize = 1536 * 1024;

pub const SNAPSHOT_METADATA_FILE: &str = "tip-sync-100-metadata.json";

pub const TIP_SYNC_SNAPSHOT_ENV: &str = "STORAGE_CHAIN_TIP_SYNC_SNAPSHOT";
pub const RELAY_SNAPSHOT_ENV: &str = "STORAGE_CHAIN_RELAY_SNAPSHOT";
pub const RAW_CHAIN_SPEC_ENV: &str = "STORAGE_CHAIN_RAW_CHAIN_SPEC";
pub const RAW_RELAY_CHAIN_SPEC_ENV: &str = "STORAGE_CHAIN_RAW_RELAY_CHAIN_SPEC";
pub const TIP_SYNC_METADATA_ENV: &str = "STORAGE_CHAIN_TIP_SYNC_METADATA";

const DEFAULT_TIP_SYNC_SNAPSHOT: &str =
	"https://storage.googleapis.com/zombienet-db-snaps/zombienet/storage_chain_tip_sync_db/tip-sync-100.tgz";
const DEFAULT_RELAY_SNAPSHOT: &str =
	"https://storage.googleapis.com/zombienet-db-snaps/zombienet/storage_chain_tip_sync_db/relay.tgz";
const SNAPSHOT_DIR: &str = "tests/zombie_ci/storage_chain/fixtures/test-databases";

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HashingAlgorithm {
	Blake2b256,
	Sha2_256,
	Keccak256,
}

impl HashingAlgorithm {
	pub fn hash(&self, data: &[u8]) -> [u8; 32] {
		match self {
			Self::Blake2b256 => sp_crypto_hashing::blake2_256(data),
			Self::Sha2_256 => sp_crypto_hashing::sha2_256(data),
			Self::Keccak256 => sp_crypto_hashing::keccak_256(data),
		}
	}

	pub const fn multihash_code(&self) -> u64 {
		match self {
			Self::Blake2b256 => 0xb220,
			Self::Sha2_256 => 0x12,
			Self::Keccak256 => 0x1b,
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
	pub total_blocks: u64,
	pub retention_period: u32,
	pub n_stores: u32,
	pub payload_size_min: usize,
	pub payload_size_max: usize,
	pub snapshot_height: u64,
	pub first_store_block: u64,
	pub last_store_block: u64,
}

pub struct ResolvedSnapshots {
	pub collator: PathBuf,
	pub relay: PathBuf,
	pub chain_spec: PathBuf,
	pub relay_chain_spec: PathBuf,
	pub metadata: PathBuf,
}

impl ResolvedSnapshots {
	pub fn load() -> Result<Self> {
		let collator =
			fixture_from_env_or_default(TIP_SYNC_SNAPSHOT_ENV, DEFAULT_TIP_SYNC_SNAPSHOT);
		let relay = fixture_from_env_or_default(RELAY_SNAPSHOT_ENV, DEFAULT_RELAY_SNAPSHOT);
		let chain_spec = fixture_from_env_or_local(RAW_CHAIN_SPEC_ENV, raw_chain_spec_path())?;
		let relay_chain_spec =
			fixture_from_env_or_local(RAW_RELAY_CHAIN_SPEC_ENV, raw_relay_chain_spec_path())?;
		let metadata = fixture_from_env_or_local(TIP_SYNC_METADATA_ENV, metadata_path())?;

		Ok(Self { collator, relay, chain_spec, relay_chain_spec, metadata })
	}

	pub fn as_parachain_snapshots(&self) -> ParachainSnapshots<'_> {
		ParachainSnapshots {
			collator: self.collator.to_str().expect("non-utf8 path"),
			relay: self.relay.to_str().expect("non-utf8 path"),
			chain_spec: self.chain_spec.to_str().expect("non-utf8 path"),
			relay_chain_spec: self.relay_chain_spec.to_str().expect("non-utf8 path"),
		}
	}

	pub fn load_metadata(&self) -> Result<SnapshotMetadata> {
		let file = std::fs::File::open(&self.metadata)
			.with_context(|| format!("Failed to open {}", self.metadata.display()))?;
		serde_json::from_reader(file)
			.with_context(|| format!("Failed to decode {}", self.metadata.display()))
	}
}

#[cfg(feature = "generate-snapshots")]
pub fn snapshot_metadata_path(output_dir: &Path) -> PathBuf {
	output_dir.join(SNAPSHOT_METADATA_FILE)
}

pub fn fixture_snapshot_dir() -> PathBuf {
	PathBuf::from(SNAPSHOT_DIR)
}

pub fn raw_chain_spec_path() -> PathBuf {
	fixture_snapshot_dir().join("raw-chain-spec.json")
}

pub fn raw_relay_chain_spec_path() -> PathBuf {
	fixture_snapshot_dir().join("raw-relay-chain-spec.json")
}

pub fn metadata_path() -> PathBuf {
	fixture_snapshot_dir().join(SNAPSHOT_METADATA_FILE)
}

fn fixture_from_env_or_default(env_var: &str, default_url: &str) -> PathBuf {
	std::env::var(env_var)
		.map(PathBuf::from)
		.unwrap_or_else(|_| PathBuf::from(default_url))
}

fn fixture_from_env_or_local(env_var: &str, local_path: PathBuf) -> Result<PathBuf> {
	match std::env::var(env_var) {
		Ok(path) => Ok(PathBuf::from(path)),
		Err(_) => std::fs::canonicalize(&local_path)
			.with_context(|| format!("checked-in fixture not found: {}", local_path.display())),
	}
}

pub fn algorithm(i: u32) -> HashingAlgorithm {
	match i % 2 {
		0 => HashingAlgorithm::Blake2b256,
		_ => HashingAlgorithm::Sha2_256,
	}
}

pub fn payload(i: u32) -> Vec<u8> {
	let span = (PAYLOAD_SIZE_MAX - PAYLOAD_SIZE_MIN + 1) as u32;
	let size = PAYLOAD_SIZE_MIN + (xorshift32_seeded(i.wrapping_add(0xA53C7B91)) % span) as usize;

	let mut state = i.wrapping_add(0x9E3779B9);
	let mut data = Vec::with_capacity(size);
	while data.len() < size {
		state = xorshift32(state);
		let remaining = size - data.len();
		if remaining >= 4 {
			data.extend_from_slice(&state.to_le_bytes());
		} else {
			data.extend_from_slice(&state.to_le_bytes()[..remaining]);
		}
	}
	data
}

pub fn content_hash(i: u32) -> [u8; 32] {
	algorithm(i).hash(&payload(i))
}

pub fn hash_to_cid(hash: &[u8; 32], algo: HashingAlgorithm) -> String {
	use cid::Cid;
	use multihash::Multihash;
	const RAW_CODEC: u64 = 0x55;
	let mh = Multihash::<64>::wrap(algo.multihash_code(), hash).expect("Valid multihash");
	Cid::new_v1(RAW_CODEC, mh).to_string()
}

fn xorshift32(mut x: u32) -> u32 {
	x ^= x << 13;
	x ^= x >> 17;
	x ^= x << 5;
	x
}

fn xorshift32_seeded(seed: u32) -> u32 {
	let s = if seed == 0 { 1 } else { seed };
	xorshift32(s)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn payload_is_deterministic_per_index() {
		for i in 0..50u32 {
			assert_eq!(payload(i), payload(i), "payload({i}) not deterministic");
		}
	}

	#[test]
	fn payload_sizes_within_bounds() {
		for i in 0..200u32 {
			let len = payload(i).len();
			assert!(
				(PAYLOAD_SIZE_MIN..=PAYLOAD_SIZE_MAX).contains(&len),
				"payload({i}).len()={} out of bounds [{}, {}]",
				len,
				PAYLOAD_SIZE_MIN,
				PAYLOAD_SIZE_MAX,
			);
		}
	}

	#[test]
	fn content_hashes_are_unique_for_first_n_stores() {
		use std::collections::HashSet;
		let mut seen = HashSet::new();
		for i in 0..N_STORES {
			let h = content_hash(i);
			assert!(seen.insert(h), "duplicate content hash at i={i}");
		}
	}

	#[test]
	fn algorithm_round_robin_blake_sha() {
		assert_eq!(algorithm(0), HashingAlgorithm::Blake2b256);
		assert_eq!(algorithm(1), HashingAlgorithm::Sha2_256);
		assert_eq!(algorithm(2), HashingAlgorithm::Blake2b256);
		assert_eq!(algorithm(3), HashingAlgorithm::Sha2_256);
	}
}
