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

use criterion::{criterion_group, criterion_main, Criterion};
use sc_statement_store::Store;
use sp_core::Pair;
use sp_statement_store::{
	runtime_api::{InvalidStatement, ValidStatement, ValidateStatement},
	DecryptionKey, Proof, SignatureVerificationResult, Statement, StatementSource, StatementStore,
	SubmitResult, Topic,
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

#[derive(Clone)]
struct TestClient;

struct RuntimeApi {
	_inner: TestClient,
}

impl sp_api::ProvideRuntimeApi<Block> for TestClient {
	type Api = RuntimeApi;
	fn runtime_api(&self) -> sp_api::ApiRef<Self::Api> {
		RuntimeApi { _inner: self.clone() }.into()
	}
}

sp_api::mock_impl_runtime_apis! {
	impl ValidateStatement<Block> for RuntimeApi {
		fn validate_statement(
			_source: StatementSource,
			statement: Statement,
		) -> std::result::Result<ValidStatement, InvalidStatement> {
			match statement.verify_signature() {
				SignatureVerificationResult::Valid(_) =>
					Ok(ValidStatement { max_count: 100_000, max_size: 1_000_000 }),
				SignatureVerificationResult::Invalid => Err(InvalidStatement::BadProof),
				SignatureVerificationResult::NoSignature => {
					if let Some(Proof::OnChain { block_hash, .. }) = statement.proof() {
						if block_hash == &CORRECT_BLOCK_HASH {
							Ok(ValidStatement { max_count: 100_000, max_size: 1_000_000 })
						} else {
							Err(InvalidStatement::BadProof)
						}
					} else {
						Err(InvalidStatement::BadProof)
					}
				},
			}
		}
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
	let mut topic: Topic = Default::default();
	topic[0..8].copy_from_slice(&data.to_le_bytes());
	topic
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

	statement.sign_ed25519_private(keypair);
	statement
}

fn setup_store(keypair: &sp_core::ed25519::Pair) -> (Store, tempfile::TempDir) {
	let temp_dir = tempfile::Builder::new().tempdir().expect("Error creating test dir");
	let client = Arc::new(TestClient);
	let mut path: std::path::PathBuf = temp_dir.path().into();
	path.push("db");
	let keystore = Arc::new(sc_keystore::LocalKeystore::in_memory());
	let store = Store::new(&path, Default::default(), client, keystore, None).unwrap();

	for i in 0..INITIAL_STATEMENTS {
		let topics = if i % 10 == 0 { vec![topic(0), topic(1)] } else { vec![] };
		let dec_key = if i % 5 == 0 { Some(dec_key(42)) } else { None };
		let statement = create_signed_statement(i as u64, &topics, dec_key, &keypair);
		store.submit(statement, StatementSource::Local);
	}

	(store, temp_dir)
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
								assert!(matches!(result, SubmitResult::New(_)));
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
								assert!(matches!(result, SubmitResult::New(_)));

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

criterion_group!(
	benches,
	bench_submit,
	bench_remove,
	bench_statement_lookup,
	bench_statements_all,
	bench_broadcasts,
	bench_posted,
	bench_maintain,
	bench_mixed_workload
);
criterion_main!(benches);
