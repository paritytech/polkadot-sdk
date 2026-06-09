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

//! Benchmarks for the statement store.
//!
//! Every benchmark drives the store through its public `StatementStore` API only, so the same file
//! compiles and runs against any revision of the crate (the in-memory index of stage 1, the
//! on-disk index of stage 2a, and the disk-backed write index of stage 2b). To compare two
//! revisions, run the baseline first and then the candidate:
//!
//! ```text
//! git checkout master           && cargo bench -p sc-statement-store -- --save-baseline before
//! git checkout <feature-branch> && cargo bench -p sc-statement-store -- --baseline before
//! ```
//!
//! If the candidate adds benchmarks the baseline lacks, copy this file onto the baseline first
//! (`git checkout <branch> -- substrate/client/statement-store/benches/statement_store.rs`) so both
//! revisions expose the same benchmark ids.
//!
//! The groups added for the on-disk index target what moving it to disk actually costs:
//! - `read_scaling`: read latency as a function of store size (flat in RAM, grows on disk);
//! - `cache`: LRU cache hit (RAM) vs miss (disk scan) for topic queries;
//! - `contention_read_under_write`: read latency while writers run concurrently.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use sc_statement_store::Store;
use sp_core::Pair;
use sp_runtime::codec::Encode;
use sp_statement_store::{
	DecryptionKey, Statement, StatementSource, StatementStore, SubmitResult, Topic,
};
use std::sync::Arc;

type Extrinsic = sp_runtime::OpaqueExtrinsic;
type Hash = sp_core::H256;
type Hashing = sp_runtime::traits::BlakeTwo256;
type BlockNumber = u64;
type Header = sp_runtime::generic::Header<BlockNumber, Hashing>;
type Block = sp_runtime::generic::Block<Header, Extrinsic>;

const CORRECT_BLOCK_HASH: [u8; 32] = [1u8; 32];
const STATEMENT_DATA_SIZE: usize = 256;
const INITIAL_STATEMENTS: usize = 1_000;
const NUM_THREADS: usize = 64;
const OPS_PER_THREAD: usize = 10;
const TOTAL_OPS: usize = NUM_THREADS * OPS_PER_THREAD;

/// Store sizes (pre-loaded statements) for the `read_scaling` group.
const SCALING_SIZES: &[usize] = &[1_000, 10_000, 50_000];

/// Distinct topics used by the `cache` group. Chosen above the read cache capacity so that cycling
/// through them forces evictions, and therefore real cache misses.
const CACHE_TOPICS: usize = 5_000;
/// Statements per topic in the `cache` group.
const CACHE_PER_TOPIC: usize = 8;

/// Reader / writer thread counts for the contention benchmark.
const CONTENTION_READERS: usize = 32;
const CONTENTION_WRITERS: usize = 32;
/// Statements pre-loaded before the contention benchmark runs.
const CONTENTION_PRELOAD: usize = 2_000;

#[derive(Clone)]
struct TestClient;

type TestBackend = sc_client_api::in_mem::Backend<Block>;

impl sc_client_api::StorageProvider<Block, TestBackend> for TestClient {
	fn storage(
		&self,
		_hash: Hash,
		_key: &sc_client_api::StorageKey,
	) -> sp_blockchain::Result<Option<sc_client_api::StorageData>> {
		// Generous per-account allowance so the large pre-loads in the scaling/cache benchmarks
		// are not capped by eviction (count, then size in bytes).
		Ok(Some(sc_client_api::StorageData((2_000_000, 256 * 1024 * 1024).encode())))
	}

	fn storage_hash(
		&self,
		_hash: Hash,
		_key: &sc_client_api::StorageKey,
	) -> sp_blockchain::Result<Option<Hash>> {
		unimplemented!()
	}

	fn storage_keys(
		&self,
		_hash: Hash,
		_prefix: Option<&sc_client_api::StorageKey>,
		_start_key: Option<&sc_client_api::StorageKey>,
	) -> sp_blockchain::Result<
		sc_client_api::backend::KeysIter<
			<TestBackend as sc_client_api::Backend<Block>>::State,
			Block,
		>,
	> {
		unimplemented!()
	}

	fn storage_pairs(
		&self,
		_hash: Hash,
		_prefix: Option<&sc_client_api::StorageKey>,
		_start_key: Option<&sc_client_api::StorageKey>,
	) -> sp_blockchain::Result<
		sc_client_api::backend::PairsIter<
			<TestBackend as sc_client_api::Backend<Block>>::State,
			Block,
		>,
	> {
		unimplemented!()
	}

	fn child_storage(
		&self,
		_hash: Hash,
		_child_info: &sc_client_api::ChildInfo,
		_key: &sc_client_api::StorageKey,
	) -> sp_blockchain::Result<Option<sc_client_api::StorageData>> {
		unimplemented!()
	}

	fn child_storage_keys(
		&self,
		_hash: Hash,
		_child_info: sc_client_api::ChildInfo,
		_prefix: Option<&sc_client_api::StorageKey>,
		_start_key: Option<&sc_client_api::StorageKey>,
	) -> sp_blockchain::Result<
		sc_client_api::backend::KeysIter<
			<TestBackend as sc_client_api::Backend<Block>>::State,
			Block,
		>,
	> {
		unimplemented!()
	}

	fn child_storage_hash(
		&self,
		_hash: Hash,
		_child_info: &sc_client_api::ChildInfo,
		_key: &sc_client_api::StorageKey,
	) -> sp_blockchain::Result<Option<Hash>> {
		unimplemented!()
	}

	fn closest_merkle_value(
		&self,
		_hash: Hash,
		_key: &sc_client_api::StorageKey,
	) -> sp_blockchain::Result<Option<sc_client_api::MerkleValue<Hash>>> {
		unimplemented!()
	}

	fn child_closest_merkle_value(
		&self,
		_hash: Hash,
		_child_info: &sc_client_api::ChildInfo,
		_key: &sc_client_api::StorageKey,
	) -> sp_blockchain::Result<Option<sc_client_api::MerkleValue<Hash>>> {
		unimplemented!()
	}
}

impl sp_blockchain::HeaderBackend<Block> for TestClient {
	fn header(&self, _hash: Hash) -> sp_blockchain::Result<Option<Header>> {
		unimplemented!()
	}
	fn info(&self) -> sp_blockchain::Info<Block> {
		sp_blockchain::Info {
			best_hash: CORRECT_BLOCK_HASH.into(),
			best_number: 0,
			genesis_hash: Default::default(),
			finalized_hash: CORRECT_BLOCK_HASH.into(),
			finalized_number: 1,
			finalized_state: None,
			number_leaves: 0,
			block_gap: None,
		}
	}
	fn status(&self, _hash: Hash) -> sp_blockchain::Result<sp_blockchain::BlockStatus> {
		unimplemented!()
	}
	fn number(&self, _hash: Hash) -> sp_blockchain::Result<Option<BlockNumber>> {
		unimplemented!()
	}
	fn hash(&self, _number: BlockNumber) -> sp_blockchain::Result<Option<Hash>> {
		unimplemented!()
	}
}

fn topic(data: u64) -> Topic {
	let mut bytes = [0u8; 32];
	bytes[0..8].copy_from_slice(&data.to_le_bytes());
	Topic::from(bytes)
}

fn dec_key(data: u64) -> DecryptionKey {
	let mut dec_key: DecryptionKey = Default::default();
	dec_key[0..8].copy_from_slice(&data.to_le_bytes());
	dec_key
}

fn create_signed_statement(
	id: u64,
	topics: &[Topic],
	dec_key: Option<DecryptionKey>,
	keypair: &sp_core::ed25519::Pair,
) -> Statement {
	let mut statement = Statement::new();
	let mut data = vec![0u8; STATEMENT_DATA_SIZE];
	data[0..8].copy_from_slice(&id.to_le_bytes());
	statement.set_plain_data(data);

	for (i, topic) in topics.iter().enumerate() {
		statement.set_topic(i, *topic);
	}

	if let Some(key) = dec_key {
		statement.set_decryption_key(key);
	}

	// Far-future expiry so the statement is accepted: the default expiry is 0, which the store
	// treats as already-expired and rejects.
	statement.set_expiry(u64::MAX);
	statement.sign_ed25519_private(keypair);
	statement
}

fn setup_store(keypair: &sp_core::ed25519::Pair) -> (Store, tempfile::TempDir) {
	let temp_dir = tempfile::Builder::new().tempdir().expect("Error creating test dir");
	let client = Arc::new(TestClient);
	let mut path: std::path::PathBuf = temp_dir.path().into();
	path.push("db");
	let keystore = Arc::new(sc_keystore::LocalKeystore::in_memory());
	let store = Store::new::<Block, TestClient, TestBackend>(
		&path,
		Default::default(),
		client,
		keystore,
		None,
		Box::new(sp_core::testing::TaskExecutor::new()),
	)
	.unwrap();

	for i in 0..INITIAL_STATEMENTS {
		let topics = if i % 10 == 0 { vec![topic(0), topic(1)] } else { vec![] };
		// Disjoint from the topic set, so `broadcasts` (no key) actually matches the topic-bearing
		// statements instead of being shadowed by a decryption key.
		let dec_key = if i % 10 == 5 { Some(dec_key(42)) } else { None };
		let statement = create_signed_statement(i as u64, &topics, dec_key, &keypair);
		store.submit(statement, StatementSource::Local);
	}

	(store, temp_dir)
}

/// Creates an empty store backed by a fresh temporary directory.
fn empty_store() -> (Store, tempfile::TempDir) {
	let temp_dir = tempfile::Builder::new().tempdir().expect("Error creating test dir");
	let client = Arc::new(TestClient);
	let mut path: std::path::PathBuf = temp_dir.path().into();
	path.push("db");
	let keystore = Arc::new(sc_keystore::LocalKeystore::in_memory());
	let store = Store::new::<Block, TestClient, TestBackend>(
		&path,
		Default::default(),
		client,
		keystore,
		None,
		Box::new(sp_core::testing::TaskExecutor::new()),
	)
	.unwrap();
	(store, temp_dir)
}

/// Builds a store with `n` statements: every 10th carries the broadcast topics `[0, 1]` (no
/// decryption key), and a disjoint set carries decryption key 42. So `broadcasts(&[0, 1])` and
/// `posted(.., 42)` each match ~`n / 10` statements, growing with the store size.
fn setup_scaled(keypair: &sp_core::ed25519::Pair, n: usize) -> (Store, tempfile::TempDir) {
	let (store, temp) = empty_store();
	for i in 0..n {
		let topics: Vec<Topic> = if i % 10 == 0 { vec![topic(0), topic(1)] } else { vec![] };
		let key = if i % 10 == 5 { Some(dec_key(42)) } else { None };
		let statement = create_signed_statement(i as u64, &topics, key, keypair);
		assert!(matches!(store.submit(statement, StatementSource::Local), SubmitResult::New));
	}
	(store, temp)
}

/// Builds a store where `topics` distinct topics each appear on `per_topic` statements (no
/// decryption key). Used by the cache benchmarks to exercise hit/miss behaviour.
fn setup_distinct_topics(
	keypair: &sp_core::ed25519::Pair,
	topics: usize,
	per_topic: usize,
) -> (Store, tempfile::TempDir) {
	let (store, temp) = empty_store();
	let mut id = 0u64;
	for t in 0..topics {
		for _ in 0..per_topic {
			let statement = create_signed_statement(id, &[topic(t as u64)], None, keypair);
			assert!(matches!(store.submit(statement, StatementSource::Local), SubmitResult::New));
			id += 1;
		}
	}
	(store, temp)
}

fn bench_submit(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let statements: Vec<_> = (INITIAL_STATEMENTS..INITIAL_STATEMENTS + TOTAL_OPS)
		.map(|i| create_signed_statement(i as u64, &[], None, &keypair))
		.collect();

	c.bench_function("submit", |b| {
		b.iter_batched(
			|| {
				let (store, _temp) = setup_store(&keypair);
				(Arc::new(store), _temp)
			},
			|(store, _temp)| {
				std::thread::scope(|s| {
					for thread_id in 0..NUM_THREADS {
						let store = store.clone();
						let start = thread_id * OPS_PER_THREAD;
						let end = start + OPS_PER_THREAD;
						let thread_statements = statements[start..end].to_vec();
						s.spawn(move || {
							for statement in thread_statements {
								let result = store.submit(statement, StatementSource::Local);
								assert!(matches!(result, SubmitResult::New));
							}
						});
					}
				});
			},
			criterion::BatchSize::LargeInput,
		)
	});
}

fn bench_remove(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();

	c.bench_function("remove", |b| {
		b.iter_batched(
			|| {
				let (store, _temp) = setup_store(&keypair);
				let hashes: Vec<_> = store
					.statements()
					.unwrap()
					.into_iter()
					.take(TOTAL_OPS)
					.map(|(hash, _)| hash)
					.collect();
				(Arc::new(store), hashes, _temp)
			},
			|(store, hashes, _temp)| {
				std::thread::scope(|s| {
					for thread_id in 0..NUM_THREADS {
						let store = store.clone();
						let start = thread_id * OPS_PER_THREAD;
						let end = start + OPS_PER_THREAD;
						let thread_hashes = hashes[start..end].to_vec();
						s.spawn(move || {
							for hash in thread_hashes {
								let _ = store.remove(&hash);
							}
						});
					}
				});
			},
			criterion::BatchSize::LargeInput,
		)
	});
}

fn bench_statement_lookup(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();

	c.bench_function("statement_lookup", |b| {
		b.iter_batched(
			|| {
				let (store, _temp) = setup_store(&keypair);
				let hashes: Vec<_> = store
					.statements()
					.unwrap()
					.into_iter()
					.take(TOTAL_OPS)
					.map(|(hash, _)| hash)
					.collect();
				(Arc::new(store), hashes, _temp)
			},
			|(store, hashes, _temp)| {
				std::thread::scope(|s| {
					for thread_id in 0..NUM_THREADS {
						let store = store.clone();
						let start = thread_id * OPS_PER_THREAD;
						let end = start + OPS_PER_THREAD;
						let thread_hashes = hashes[start..end].to_vec();
						s.spawn(move || {
							for hash in thread_hashes {
								let _ = store.statement(&hash);
							}
						});
					}
				});
			},
			criterion::BatchSize::LargeInput,
		)
	});
}

fn bench_statements_all(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let (store, _temp) = setup_store(&keypair);
	let store = Arc::new(store);

	c.bench_function("statements_all", |b| {
		b.iter(|| {
			std::thread::scope(|s| {
				for _ in 0..NUM_THREADS {
					let store = store.clone();
					s.spawn(move || {
						for _ in 0..OPS_PER_THREAD {
							let _ = store.statements();
						}
					});
				}
			});
		})
	});
}

fn bench_broadcasts(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let (store, _temp) = setup_store(&keypair);
	let store = Arc::new(store);
	let topics = vec![topic(0), topic(1)];

	c.bench_function("broadcasts", |b| {
		b.iter(|| {
			std::thread::scope(|s| {
				for _ in 0..NUM_THREADS {
					let store = store.clone();
					let topics = topics.clone();
					s.spawn(move || {
						for _ in 0..OPS_PER_THREAD {
							let _ = store.broadcasts(&topics);
						}
					});
				}
			});
		})
	});
}

fn bench_posted(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let (store, _temp) = setup_store(&keypair);
	let store = Arc::new(store);
	let key = dec_key(42);

	c.bench_function("posted", |b| {
		b.iter(|| {
			std::thread::scope(|s| {
				for _ in 0..NUM_THREADS {
					let store = store.clone();
					s.spawn(move || {
						for _ in 0..OPS_PER_THREAD {
							let _ = store.posted(&[], key);
						}
					});
				}
			});
		})
	});
}

fn bench_maintain(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();

	c.bench_function("maintain", |b| {
		b.iter_batched(
			|| {
				let (store, _temp) = setup_store(&keypair);
				// Mark statements for expiration by removing them
				let hashes: Vec<_> = store
					.statements()
					.unwrap()
					.into_iter()
					.take(TOTAL_OPS)
					.map(|(hash, _)| hash)
					.collect();
				for hash in hashes {
					let _ = store.remove(&hash);
				}
				(store, _temp)
			},
			|(store, _temp)| {
				store.maintain();
			},
			criterion::BatchSize::LargeInput,
		)
	});
}

fn bench_mixed_workload(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let statements: Vec<_> = (INITIAL_STATEMENTS..INITIAL_STATEMENTS + TOTAL_OPS)
		.map(|i| create_signed_statement(i as u64, &[topic(0), topic(1)], None, &keypair))
		.collect();

	c.bench_function("mixed_workload", |b| {
		b.iter_batched(
			|| {
				let (store, _temp) = setup_store(&keypair);
				(Arc::new(store), _temp)
			},
			|(store, _temp)| {
				std::thread::scope(|s| {
					for thread_id in 0..NUM_THREADS {
						let store = store.clone();
						let start = thread_id * OPS_PER_THREAD;
						let end = start + OPS_PER_THREAD;
						let thread_statements = statements[start..end].to_vec();
						let topics = vec![topic(0), topic(1)];
						s.spawn(move || {
							for statement in thread_statements {
								// Submit a statement
								let result = store.submit(statement, StatementSource::Local);
								assert!(matches!(result, SubmitResult::New));

								// Query broadcasts
								let _ = store.broadcasts(&topics);
							}
						});
					}
				});
			},
			criterion::BatchSize::LargeInput,
		)
	});
}

/// Read latency as a function of store size. In-memory indexes stay roughly flat; an on-disk index
/// grows with the data it has to scan, so this is the primary "cost of disk" axis.
fn bench_read_scaling(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let topics = vec![topic(0), topic(1)];
	// A statement that is present at every size (id 0 carries topics [0, 1], no key).
	let known_hash = create_signed_statement(0, &topics, None, &keypair).hash();

	let mut group = c.benchmark_group("read_scaling");
	for &size in SCALING_SIZES {
		let (store, _temp) = setup_scaled(&keypair, size);
		// Full topic query (index scan + body fetch); its result set grows with the store.
		group.bench_with_input(BenchmarkId::new("broadcasts", size), &size, |b, _| {
			b.iter(|| store.broadcasts(&topics))
		});
		// Point existence check (pure index lookup, constant-size result).
		group.bench_with_input(BenchmarkId::new("has_statement", size), &size, |b, _| {
			b.iter(|| store.has_statement(&known_hash))
		});
	}
	group.finish();
}

/// LRU cache hit (served from RAM) vs miss (served from a disk scan) for topic queries. On an
/// all-in-memory index both arms behave the same; the hit/miss gap is the cost of a cache miss.
fn bench_cache_hit_vs_miss(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let (store, _temp) = setup_distinct_topics(&keypair, CACHE_TOPICS, CACHE_PER_TOPIC);

	let mut group = c.benchmark_group("cache");
	// Hit: always the same topic, so after the first call it stays in the cache.
	let hit_topics = vec![topic(0)];
	group.bench_function("hit", |b| b.iter(|| store.broadcasts(&hit_topics)));
	// Miss: cycle through more topics than the cache holds, forcing a lookup each time.
	group.bench_function("miss", |b| {
		let mut i = 0usize;
		b.iter(|| {
			i = (i + 1) % CACHE_TOPICS;
			store.broadcasts(&[topic(i as u64)])
		})
	});
	group.finish();
}

/// Read latency while writers run concurrently. Measures how much the write/constraint path
/// interferes with reads — the contention that splitting the index (stage 1) and moving reads off
/// the write lock (stage 2a) is meant to reduce.
fn bench_contention(c: &mut Criterion) {
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let topics = vec![topic(0), topic(1)];

	c.bench_function("contention_read_under_write", |b| {
		b.iter_batched(
			|| {
				let (store, temp) = setup_scaled(&keypair, CONTENTION_PRELOAD);
				let writes: Vec<Statement> = (CONTENTION_PRELOAD..
					CONTENTION_PRELOAD + CONTENTION_WRITERS * OPS_PER_THREAD)
					.map(|i| {
						create_signed_statement(i as u64, &[topic(0), topic(1)], None, &keypair)
					})
					.collect();
				(Arc::new(store), writes, temp)
			},
			|(store, writes, _temp)| {
				std::thread::scope(|s| {
					for w in 0..CONTENTION_WRITERS {
						let store = store.clone();
						let start = w * OPS_PER_THREAD;
						let chunk = writes[start..start + OPS_PER_THREAD].to_vec();
						s.spawn(move || {
							for statement in chunk {
								let _ = store.submit(statement, StatementSource::Local);
							}
						});
					}
					for _ in 0..CONTENTION_READERS {
						let store = store.clone();
						let topics = topics.clone();
						s.spawn(move || {
							for _ in 0..OPS_PER_THREAD {
								let _ = store.broadcasts(&topics);
							}
						});
					}
				});
			},
			criterion::BatchSize::LargeInput,
		)
	});
}

criterion_group!(
	benches,
	bench_submit,
	bench_remove,
	bench_statement_lookup,
	bench_statements_all,
	bench_broadcasts,
	bench_posted,
	bench_maintain,
	bench_mixed_workload,
	bench_read_scaling,
	bench_cache_hit_vs_miss,
	bench_contention
);
criterion_main!(benches);
