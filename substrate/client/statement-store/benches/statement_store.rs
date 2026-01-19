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

use codec::Decode;
use criterion::{criterion_group, criterion_main, Criterion};
use sc_client_api::{backend::Backend as BackendT, HeaderBackend};
use sc_statement_store::Store;
use sp_api::ProvideRuntimeApi;
use sp_keyring::Sr25519Keyring;
use sp_state_machine::Backend as StateBackend;
use sp_statement_store::{
	runtime_api::ValidateStatement, DecryptionKey, Statement, StatementSource, StatementStore,
	SubmitResult, Topic,
};
use std::sync::Arc;
use substrate_test_runtime_client::{
	DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
};

const STATEMENT_DATA_SIZE: usize = 256;
const INITIAL_STATEMENTS: usize = 1_000;
const NUM_THREADS: usize = 64;
const OPS_PER_THREAD: usize = 10;
const TOTAL_OPS: usize = NUM_THREADS * OPS_PER_THREAD;

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
	keypair: &sp_core::sr25519::Pair,
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

	statement.sign_sr25519_private(keypair);
	statement
}

fn setup_store(keypair: &sp_core::sr25519::Pair) -> (Store, tempfile::TempDir) {
	let temp_dir = tempfile::Builder::new().tempdir().expect("Error creating test dir");
	let client = Arc::new(TestClientBuilder::new().build());
	let mut path: std::path::PathBuf = temp_dir.path().into();
	path.push("db");
	let keystore = Arc::new(sc_keystore::LocalKeystore::in_memory());
	let store = Store::new(&path, Default::default(), client, keystore, None).unwrap();

	for i in 0..INITIAL_STATEMENTS {
		let topics = if i % 10 == 0 { vec![topic(0), topic(1)] } else { vec![] };
		let dec_key = if i % 5 == 0 { Some(dec_key(42)) } else { None };
		let statement = create_signed_statement(i as u64, &topics, dec_key, keypair);
		store.submit(statement, StatementSource::Local);
	}

	(store, temp_dir)
}

fn bench_submit(c: &mut Criterion) {
	let keypair = Sr25519Keyring::Alice.pair();
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
								assert!(
									matches!(result, SubmitResult::New),
									"Submit failed: {:?}",
									result
								);
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
	let keypair = Sr25519Keyring::Alice.pair();

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
	let keypair = Sr25519Keyring::Alice.pair();

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
	let keypair = Sr25519Keyring::Alice.pair();
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
	let keypair = Sr25519Keyring::Alice.pair();
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
	let keypair = Sr25519Keyring::Alice.pair();
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
	let keypair = Sr25519Keyring::Alice.pair();

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
	let keypair = Sr25519Keyring::Alice.pair();
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

fn bench_validate(c: &mut Criterion) {
	let keypair = Sr25519Keyring::Alice.pair();
	let client = Arc::new(TestClientBuilder::new().build());
	let block_hash = client.info().best_hash;

	let statement = create_signed_statement(0, &[], None, &keypair);

	c.bench_function("validate_runtime", |b| {
		b.iter(|| {
			let api = client.runtime_api();
			let result =
				api.validate_statement(block_hash, StatementSource::Local, statement.clone());
			assert!(result.is_ok(), "Validation failed: {:?}", result);
		})
	});
}

fn build_system_account_key(account: &[u8; 32]) -> Vec<u8> {
	let pallet_prefix = sp_io::hashing::twox_128(b"System");
	let storage_prefix = sp_io::hashing::twox_128(b"Account");
	let key_hash = sp_io::hashing::blake2_128(account);

	let mut key = Vec::with_capacity(16 + 16 + 16 + 32);
	key.extend_from_slice(&pallet_prefix);
	key.extend_from_slice(&storage_prefix);
	key.extend_from_slice(&key_hash);
	key.extend_from_slice(account);
	key
}

fn bench_validate_native(c: &mut Criterion) {
	use sp_statement_store::SignatureVerificationResult;

	let keypair = Sr25519Keyring::Alice.pair();
	let (client, backend) = TestClientBuilder::new().build_with_backend();
	let block_hash = client.info().best_hash;

	let statement = create_signed_statement(0, &[], None, &keypair);

	let state = backend
		.state_at(block_hash, sc_client_api::TrieCacheContext::Untrusted)
		.expect("State should exist");

	type Balance = u64;
	let statement_cost: Balance = substrate_test_runtime::currency::DOLLARS / 1000;
	let min_allowed_statements: u32 = 4;
	let max_allowed_statements: u32 = 100_000;

	let account = match statement.verify_signature() {
		SignatureVerificationResult::Valid(account) => account,
		SignatureVerificationResult::Invalid => panic!("Invalid signature"),
		SignatureVerificationResult::NoSignature => panic!("No signature"),
	};

	let storage_key = build_system_account_key(&account);
	state
		.storage(&storage_key)
		.expect("Storage access failed")
		.expect("Account should exist in genesis");

	c.bench_function("validate_native", |b| {
		b.iter(|| {
			let account = match statement.verify_signature() {
				SignatureVerificationResult::Valid(account) => account,
				SignatureVerificationResult::Invalid => panic!("Invalid signature"),
				SignatureVerificationResult::NoSignature => panic!("No signature"),
			};

			let storage_key = build_system_account_key(&account);
			let encoded = state.storage(&storage_key).expect("Storage access failed");
			let account_info: frame_system::AccountInfo<
				u64,
				pallet_balances::AccountData<Balance>,
			> = Decode::decode(&mut encoded.as_ref().expect("Account should exist").as_slice())
				.expect("Failed to decode AccountInfo");

			let balance = account_info.data.free;
			let max_count = (balance / statement_cost) as u32;
			let _max_count = max_count.clamp(min_allowed_statements, max_allowed_statements);
		})
	});
}

fn build_signature_material(statement: &Statement) -> Vec<u8> {
	use codec::{CompactLen, Encode};

	let mut without_proof = statement.clone();
	without_proof.remove_proof();

	let encoded = without_proof.encode();
	if encoded.is_empty() {
		return Vec::new();
	}
	let (len_bytes, _) = codec::Compact::<u32>::decode(&mut encoded.as_slice())
		.map(|c| {
			let len = codec::Compact::<u32>::compact_len(&c.0);
			(len, c.0)
		})
		.unwrap_or((0, 0));

	encoded[len_bytes..].to_vec()
}

fn bench_validate_hybrid(c: &mut Criterion) {
	use sp_core::sr25519::{Public, Signature};
	use sp_runtime::traits::Verify;
	use sp_statement_store::Proof;
	use substrate_test_runtime::TestAPI;

	let keypair = Sr25519Keyring::Alice.pair();
	let client = Arc::new(TestClientBuilder::new().build());
	let block_hash = client.info().best_hash;

	let statement = create_signed_statement(0, &[], None, &keypair);
	let encoded_statement = codec::Encode::encode(&statement);

	c.bench_function("validate_hybrid", |b| {
		b.iter(|| {
			let statement =
				Statement::decode(&mut encoded_statement.as_slice()).expect("Decode failed");

			let proof = statement.proof().expect("Statement should have proof");
			let (sig_bytes, signer_bytes) = match proof {
				Proof::Sr25519 { signature, signer } => (*signature, *signer),
				_ => panic!("Expected Sr25519 proof"),
			};

			let signature = Signature::from(sig_bytes);
			let public = Public::from(signer_bytes);
			let msg = build_signature_material(&statement);
			assert!(signature.verify(msg.as_slice(), &public), "Signature verification failed");

			let api = client.runtime_api();
			let _limits = api
				.statement_account_limits(block_hash, signer_bytes.into())
				.expect("Runtime API call failed");
		})
	});
}

fn bench_runtime_api_only(c: &mut Criterion) {
	use substrate_test_runtime::TestAPI;

	let client = Arc::new(TestClientBuilder::new().build());
	let block_hash = client.info().best_hash;

	let account: [u8; 32] = Sr25519Keyring::Alice.public().into();

	c.bench_function("runtime_api_only", |b| {
		b.iter(|| {
			let api = client.runtime_api();
			let _limits = api
				.statement_account_limits(block_hash, account.into())
				.expect("Runtime API call failed");
		})
	});
}

fn bench_sig_verify_only(c: &mut Criterion) {
	use sp_core::sr25519::{Public, Signature};
	use sp_runtime::traits::Verify;
	use sp_statement_store::Proof;

	let keypair = Sr25519Keyring::Alice.pair();
	let statement = create_signed_statement(0, &[], None, &keypair);
	let encoded_statement = codec::Encode::encode(&statement);

	c.bench_function("sig_verify_only", |b| {
		b.iter(|| {
			let statement =
				Statement::decode(&mut encoded_statement.as_slice()).expect("Decode failed");

			let proof = statement.proof().expect("Statement should have proof");
			let (sig_bytes, signer_bytes) = match proof {
				Proof::Sr25519 { signature, signer } => (*signature, *signer),
				_ => panic!("Expected Sr25519 proof"),
			};

			let signature = Signature::from(sig_bytes);
			let public = Public::from(signer_bytes);
			let msg = build_signature_material(&statement);
			assert!(signature.verify(msg.as_slice(), &public), "Signature verification failed");
		})
	});
}

fn bench_validate_direct_executor(c: &mut Criterion) {
	use codec::Encode;
	use sc_executor::WasmExecutor;
	use sp_core::traits::{CallContext, CodeExecutor};
	use sp_state_machine::{backend::AsTrieBackend, Ext, OverlayedChanges};

	let keypair = Sr25519Keyring::Alice.pair();
	let (client, backend) = TestClientBuilder::new().build_with_backend();
	let block_hash = client.info().best_hash;

	let statement = create_signed_statement(0, &[], None, &keypair);

	let call_data = (StatementSource::Local, statement).encode();

	let executor =
		WasmExecutor::<substrate_test_runtime_client::TestHostFunctions>::builder().build();

	let state = backend
		.state_at(block_hash, sc_client_api::TrieCacheContext::Untrusted)
		.expect("State should exist");

	let trie_backend = state.as_trie_backend();

	let runtime_code_backend = sp_state_machine::backend::BackendRuntimeCode::new(trie_backend);
	let runtime_code = runtime_code_backend.runtime_code().expect("Failed to get runtime code");

	c.bench_function("validate_direct_executor", |b| {
		b.iter(|| {
			let mut overlay = OverlayedChanges::default();
			let mut ext = Ext::new(&mut overlay, trie_backend, None);
			let (result, _) = executor.call(
				&mut ext,
				&runtime_code,
				"ValidateStatement_validate_statement",
				&call_data,
				CallContext::Offchain,
			);
			assert!(result.is_ok());
		})
	});
}

fn bench_validate_reused_instance(c: &mut Criterion) {
	use codec::Encode;
	use sc_allocator::FreeingBumpHeapAllocator;
	use sc_executor_common::runtime_blob::RuntimeBlob;
	use sc_executor_wasmtime::WasmtimeRuntime;
	use sp_externalities::set_and_run_with_externalities;
	use sp_state_machine::{backend::AsTrieBackend, Ext, OverlayedChanges};

	let keypair = Sr25519Keyring::Alice.pair();
	let (client, backend) = TestClientBuilder::new().build_with_backend();
	let block_hash = client.info().best_hash;

	let statement = create_signed_statement(0, &[], None, &keypair);
	let call_data = (StatementSource::Local, statement).encode();

	let wasm_code = substrate_test_runtime::wasm_binary_unwrap();
	let blob = RuntimeBlob::uncompress_if_needed(wasm_code).expect("Failed to uncompress runtime");

	let config = sc_executor_wasmtime::Config {
		allow_missing_func_imports: true,
		cache_path: None,
		semantics: sc_executor_wasmtime::Semantics {
			heap_alloc_strategy: sc_executor_common::wasm_runtime::DEFAULT_HEAP_ALLOC_STRATEGY,
			instantiation_strategy: sc_executor_wasmtime::InstantiationStrategy::PoolingCopyOnWrite,
			deterministic_stack_limit: None,
			canonicalize_nans: false,
			parallel_compilation: true,
			wasm_multi_value: false,
			wasm_bulk_memory: false,
			wasm_reference_types: false,
			wasm_simd: false,
		},
	};

	let module: WasmtimeRuntime = sc_executor_wasmtime::create_runtime::<
		substrate_test_runtime_client::TestHostFunctions,
	>(blob, config)
	.expect("Failed to create runtime");

	let mut instance_wrapper = module.create_instance_wrapper().expect("Failed to create instance");
	let heap_base = instance_wrapper.extract_heap_base().expect("Failed to get heap base");
	let entrypoint = instance_wrapper
		.resolve_entrypoint("ValidateStatement_validate_statement")
		.expect("Failed to resolve entrypoint");

	let state = backend
		.state_at(block_hash, sc_client_api::TrieCacheContext::Untrusted)
		.expect("State should exist");

	let trie_backend = state.as_trie_backend();

	c.bench_function("validate_reused_instance", |b| {
		b.iter(|| {
			let mut overlay = OverlayedChanges::default();
			let mut ext = Ext::new(&mut overlay, trie_backend, None);
			let allocator = FreeingBumpHeapAllocator::new(heap_base);
			let result = set_and_run_with_externalities(&mut ext, || {
				sc_executor_wasmtime::perform_call(
					&call_data,
					&mut instance_wrapper,
					entrypoint.clone(),
					allocator,
					&mut None,
				)
				.expect("Call failed")
			});
			assert!(!result.is_empty());
		})
	});
}

fn bench_validate_reused_instance_with_reset(c: &mut Criterion) {
	use codec::Encode;
	use sc_allocator::FreeingBumpHeapAllocator;
	use sc_executor_common::runtime_blob::RuntimeBlob;
	use sc_executor_wasmtime::WasmtimeRuntime;
	use sp_externalities::set_and_run_with_externalities;
	use sp_state_machine::{backend::AsTrieBackend, Ext, OverlayedChanges};

	let keypair = Sr25519Keyring::Alice.pair();
	let (client, backend) = TestClientBuilder::new().build_with_backend();
	let block_hash = client.info().best_hash;

	let statement = create_signed_statement(0, &[], None, &keypair);
	let call_data = (StatementSource::Local, statement).encode();

	let wasm_code = substrate_test_runtime::wasm_binary_unwrap();
	let blob = RuntimeBlob::uncompress_if_needed(wasm_code).expect("Failed to uncompress runtime");

	let config = sc_executor_wasmtime::Config {
		allow_missing_func_imports: true,
		cache_path: None,
		semantics: sc_executor_wasmtime::Semantics {
			heap_alloc_strategy: sc_executor_common::wasm_runtime::DEFAULT_HEAP_ALLOC_STRATEGY,
			instantiation_strategy: sc_executor_wasmtime::InstantiationStrategy::PoolingCopyOnWrite,
			deterministic_stack_limit: None,
			canonicalize_nans: false,
			parallel_compilation: true,
			wasm_multi_value: false,
			wasm_bulk_memory: false,
			wasm_reference_types: false,
			wasm_simd: false,
		},
	};

	let module: WasmtimeRuntime = sc_executor_wasmtime::create_runtime::<
		substrate_test_runtime_client::TestHostFunctions,
	>(blob, config)
	.expect("Failed to create runtime");

	let mut instance_wrapper = module.create_instance_wrapper().expect("Failed to create instance");
	let heap_base = instance_wrapper.extract_heap_base().expect("Failed to get heap base");
	let entrypoint = instance_wrapper
		.resolve_entrypoint("ValidateStatement_validate_statement")
		.expect("Failed to resolve entrypoint");

	let state = backend
		.state_at(block_hash, sc_client_api::TrieCacheContext::Untrusted)
		.expect("State should exist");

	let trie_backend = state.as_trie_backend();

	c.bench_function("validate_reused_with_reset", |b| {
		b.iter(|| {
			instance_wrapper.reset_heap(heap_base);
			let mut overlay = OverlayedChanges::default();
			let mut ext = Ext::new(&mut overlay, trie_backend, None);
			let allocator = FreeingBumpHeapAllocator::new(heap_base);
			let result = set_and_run_with_externalities(&mut ext, || {
				sc_executor_wasmtime::perform_call(
					&call_data,
					&mut instance_wrapper,
					entrypoint.clone(),
					allocator,
					&mut None,
				)
				.expect("Call failed")
			});
			assert!(!result.is_empty());
		})
	});
}

criterion_group!(
	benches,
	bench_validate,
	bench_validate_native,
	bench_validate_hybrid,
	bench_runtime_api_only,
	bench_sig_verify_only,
	bench_validate_direct_executor,
	bench_validate_reused_instance,
	bench_validate_reused_instance_with_reset,
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
