// Benchmark for StorageKeyDeltaTracker.
//
// Run:  cargo bench -p sp-state-machine --bench delta_tracker

use criterion::{criterion_group, criterion_main, Criterion};
use sp_state_machine::overlayed_changes::storage_key_delta_tracker::{
	DeltaKeyOp, StorageKeyDeltaTracker,
};

// --- Tunable constants ---

const TRANSACTIONS: usize = 2_000;
const UNIQUE_KEYS_PER_TX: usize = 20;
const SHARED_KEYS: usize = 10;
const SNAPSHOTS_PER_TX: usize = 3;
const NESTING_DEPTH: usize = 3;
const DELETE_EVERY_NTH: usize = 10;

// --- Key generation (deterministic, no RNG dep) ---

fn make_key(seed: usize) -> Vec<u8> {
	let mut key = Vec::with_capacity(80);
	for i in 0..10u64 {
		let val = (seed as u64)
			.wrapping_mul(6364136223846793005)
			.wrapping_add(i.wrapping_mul(1442695040888963407));
		key.extend_from_slice(&val.to_le_bytes());
	}
	key
}

fn op_for_index(i: usize) -> DeltaKeyOp {
	if i % DELETE_EVERY_NTH == 0 {
		DeltaKeyOp::Deleted
	} else {
		DeltaKeyOp::Updated
	}
}

type Tracker = StorageKeyDeltaTracker<Vec<u8>>;

// --- Benchmark workloads ---

/// Full block simulation: 2k txs, nested layers, mixed ops, multiple snapshots.
fn block_simulation(shared_keys: &[Vec<u8>], unique_keys: &[Vec<u8>]) {
	let mut tracker = Tracker::default();

	for tx in 0..TRANSACTIONS {
		for _ in 0..NESTING_DEPTH {
			tracker.start_transaction();
		}

		let unique_offset = tx * UNIQUE_KEYS_PER_TX;
		let mut key_idx = 0;

		for snap in 0..SNAPSHOTS_PER_TX {
			let unique_start = unique_offset + snap * (UNIQUE_KEYS_PER_TX / SNAPSHOTS_PER_TX);
			let unique_end = if snap == SNAPSHOTS_PER_TX - 1 {
				unique_offset + UNIQUE_KEYS_PER_TX
			} else {
				unique_offset + (snap + 1) * (UNIQUE_KEYS_PER_TX / SNAPSHOTS_PER_TX)
			};
			for i in unique_start..unique_end {
				tracker.add_key(unique_keys[i].clone(), op_for_index(key_idx));
				key_idx += 1;
			}
			for k in shared_keys {
				tracker.add_key(k.clone(), op_for_index(key_idx));
				key_idx += 1;
			}
			let _delta = tracker.take_delta();
		}

		for _ in 0..NESTING_DEPTH {
			tracker.commit_transaction();
		}
	}
}

/// Dedup-heavy: 50 shared keys across all 2k txs, 5 unique per tx.
fn heavy_dedup(shared: &[Vec<u8>], unique: &[Vec<u8>]) {
	let mut tracker = Tracker::default();

	for tx in 0..TRANSACTIONS {
		tracker.start_transaction();
		for k in shared {
			tracker.add_key(k.clone(), DeltaKeyOp::Updated);
		}
		for i in 0..5 {
			tracker.add_key(unique[tx * 5 + i].clone(), DeltaKeyOp::Updated);
		}
		let _delta = tracker.take_delta();
		tracker.commit_transaction();
	}
}

/// Hammers Deleted→Updated suppression: 100 keys deleted, then 500 rounds of Updated.
fn delete_then_update(keys: &[Vec<u8>]) {
	let mut tracker = Tracker::default();

	for k in keys {
		tracker.add_key(k.clone(), DeltaKeyOp::Deleted);
	}
	let _delta = tracker.take_delta();

	for _ in 0..500 {
		tracker.start_transaction();
		for k in keys {
			tracker.add_key(k.clone(), DeltaKeyOp::Updated);
		}
		let _delta = tracker.take_delta();
		tracker.commit_transaction();
	}
}

// --- Criterion harness ---

fn bench_delta_tracker(c: &mut Criterion) {
	let shared_keys: Vec<Vec<u8>> = (0..SHARED_KEYS).map(make_key).collect();
	let unique_keys: Vec<Vec<u8>> =
		(0..TRANSACTIONS * UNIQUE_KEYS_PER_TX).map(|i| make_key(SHARED_KEYS + i)).collect();

	c.bench_function("block_simulation", |b| {
		b.iter(|| block_simulation(&shared_keys, &unique_keys))
	});

	let dedup_shared: Vec<Vec<u8>> = (0..50).map(make_key).collect();
	let dedup_unique: Vec<Vec<u8>> =
		(0..TRANSACTIONS * 5).map(|i| make_key(1000 + i)).collect();

	c.bench_function("heavy_dedup", |b| {
		b.iter(|| heavy_dedup(&dedup_shared, &dedup_unique))
	});

	let del_keys: Vec<Vec<u8>> = (0..100).map(make_key).collect();

	c.bench_function("delete_then_update", |b| b.iter(|| delete_then_update(&del_keys)));
}

criterion_group!(benches, bench_delta_tracker);
criterion_main!(benches);
