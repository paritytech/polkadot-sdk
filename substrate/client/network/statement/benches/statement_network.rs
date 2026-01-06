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
use futures::{stream, Stream, StreamExt};
use sc_network::{
	service::traits::{NotificationEvent, NotificationService},
	utils::LruHashSet,
	NetworkPeers, ObservedRole,
};
use sc_network_statement::{
	config::{MAX_KNOWN_STATEMENTS, MAX_PENDING_STATEMENTS},
	Peer, StatementHandler,
};
use sc_network_sync::{SyncEvent, SyncEventStream};
use sc_network_types::PeerId;
use sc_statement_store::Store;
use sp_core::Pair;
use sp_statement_store::{Statement, StatementSource, StatementStore};
use std::{collections::HashMap, num::NonZeroUsize, pin::Pin, sync::Arc};
use substrate_test_runtime_client::{sc_executor::WasmExecutor, DefaultTestClientBuilderExt};

const STATEMENT_DATA_SIZE: usize = 256;

#[derive(Clone)]
struct TestNetwork;

impl TestNetwork {
	fn new() -> Self {
		Self
	}
}

#[async_trait::async_trait]
impl NetworkPeers for TestNetwork {
	fn set_authorized_peers(&self, _: std::collections::HashSet<PeerId>) {}
	fn set_authorized_only(&self, _: bool) {}
	fn add_known_address(&self, _: PeerId, _: sc_network::Multiaddr) {}
	fn report_peer(&self, _peer_id: PeerId, _cost_benefit: sc_network::ReputationChange) {}
	fn peer_reputation(&self, _: &PeerId) -> i32 {
		unimplemented!()
	}
	fn disconnect_peer(&self, _: PeerId, _: sc_network::ProtocolName) {}
	fn accept_unreserved_peers(&self) {}
	fn deny_unreserved_peers(&self) {}
	fn add_reserved_peer(&self, _: sc_network::config::MultiaddrWithPeerId) -> Result<(), String> {
		unimplemented!()
	}
	fn remove_reserved_peer(&self, _: PeerId) {}
	fn set_reserved_peers(
		&self,
		_: sc_network::ProtocolName,
		_: std::collections::HashSet<sc_network::Multiaddr>,
	) -> Result<(), String> {
		unimplemented!()
	}
	fn add_peers_to_reserved_set(
		&self,
		_: sc_network::ProtocolName,
		_: std::collections::HashSet<sc_network::Multiaddr>,
	) -> Result<(), String> {
		unimplemented!()
	}
	fn remove_peers_from_reserved_set(
		&self,
		_: sc_network::ProtocolName,
		_: Vec<PeerId>,
	) -> Result<(), String> {
		unimplemented!()
	}
	fn sync_num_connected(&self) -> usize {
		unimplemented!()
	}
	fn peer_role(&self, _: PeerId, _: Vec<u8>) -> Option<sc_network::ObservedRole> {
		unimplemented!()
	}
	async fn reserved_peers(&self) -> Result<Vec<PeerId>, ()> {
		unimplemented!()
	}
}

struct TestSync {}

impl TestSync {
	fn new() -> Self {
		Self {}
	}
}

impl SyncEventStream for TestSync {
	fn event_stream(&self, _name: &'static str) -> Pin<Box<dyn Stream<Item = SyncEvent> + Send>> {
		unimplemented!()
	}
}

impl sp_consensus::SyncOracle for TestSync {
	fn is_major_syncing(&self) -> bool {
		false
	}
	fn is_offline(&self) -> bool {
		false
	}
}

impl sc_network::NetworkEventStream for TestNetwork {
	fn event_stream(
		&self,
		_name: &'static str,
	) -> Pin<Box<dyn Stream<Item = sc_network::Event> + Send>> {
		unimplemented!()
	}
}

#[derive(Debug, Clone)]
struct TestNotificationService;

#[async_trait::async_trait]
impl NotificationService for TestNotificationService {
	async fn open_substream(&mut self, _peer: PeerId) -> Result<(), ()> {
		unimplemented!()
	}
	async fn close_substream(&mut self, _peer: PeerId) -> Result<(), ()> {
		unimplemented!()
	}
	fn send_sync_notification(&mut self, _peer: &PeerId, _notification: Vec<u8>) {}
	async fn send_async_notification(
		&mut self,
		_peer: &PeerId,
		_notification: Vec<u8>,
	) -> Result<(), sc_network::error::Error> {
		unimplemented!()
	}
	async fn set_handshake(&mut self, _handshake: Vec<u8>) -> Result<(), ()> {
		unimplemented!()
	}
	fn try_set_handshake(&mut self, _handshake: Vec<u8>) -> Result<(), ()> {
		unimplemented!()
	}
	async fn next_event(&mut self) -> Option<NotificationEvent> {
		unimplemented!()
	}
	fn clone(&mut self) -> Result<Box<dyn NotificationService>, ()> {
		unimplemented!()
	}
	fn protocol(&self) -> &sc_network::types::ProtocolName {
		unimplemented!()
	}
	fn message_sink(
		&self,
		_peer: &PeerId,
	) -> Option<Box<dyn sc_network::service::traits::MessageSink>> {
		unimplemented!()
	}
}

fn create_signed_statement(id: usize, keypair: &sp_core::ed25519::Pair) -> Statement {
	let mut statement = Statement::new();
	let mut data = vec![0u8; STATEMENT_DATA_SIZE];
	data[0..8].copy_from_slice(&id.to_le_bytes());
	statement.set_plain_data(data);

	statement.sign_ed25519_private(keypair);
	statement
}

fn build_handler(
	executor: Arc<
		dyn Fn(Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>) + Send + Sync,
	>,
	num_threads: usize,
	max_runtime_instances: usize,
	batch_limit: usize,
) -> (StatementHandler<TestNetwork, TestSync>, PeerId, tempfile::TempDir, Arc<Store>) {
	let temp_dir = tempfile::Builder::new().tempdir().expect("Error creating test dir");
	let mut path: std::path::PathBuf = temp_dir.path().into();
	path.push("db");

	let wasm_executor = WasmExecutor::builder()
		.with_max_runtime_instances(max_runtime_instances)
		.build();
	let (client, _) = substrate_test_runtime_client::TestClientBuilder::new()
		.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
		Some(wasm_executor),
	);
	let client = Arc::new(client);
	let keystore = Arc::new(sc_keystore::LocalKeystore::in_memory());
	let statement_store = Store::new(&path, Default::default(), client, keystore, None).unwrap();
	let statement_store = Arc::new(statement_store);

	let (queue_sender, queue_receiver) = async_channel::bounded::<(
		Statement,
		futures::channel::oneshot::Sender<sp_statement_store::SubmitResult>,
	)>(MAX_PENDING_STATEMENTS);
	let (batch_queue_sender, batch_queue_receiver) = async_channel::bounded::<(
		Vec<Statement>,
		futures::channel::oneshot::Sender<sp_statement_store::SubmitResult>,
	)>(MAX_PENDING_STATEMENTS);

	let network = TestNetwork::new();
	let peer_id = PeerId::random();
	let mut peers = HashMap::new();
	peers.insert(
		peer_id,
		Peer::new_for_testing(
			LruHashSet::new(NonZeroUsize::new(MAX_KNOWN_STATEMENTS).unwrap()),
			ObservedRole::Full,
		),
	);

	for _ in 0..num_threads {
		let store = statement_store.clone();
		let receiver = queue_receiver.clone();
		executor(Box::pin(async move {
			let mut counter = 0;
			loop {
				let task = receiver.recv().await;
				match task {
					Ok((statement, completion)) => {
						counter += 1;
						let result = store.submit(statement, StatementSource::Network);
						if counter % 2000 == 0 {
							println!(
								"Processed {} statements validation {} ms insert {} ms locking {}
							ms", 	counter,
								store
									.time_spent_validation
									.load(std::sync::atomic::Ordering::Relaxed) /
									1_000_000,
								store
									.time_spent_inserting
									.load(std::sync::atomic::Ordering::Relaxed) /
									1_000_000,
								store.time_spent_locking.load(std::sync::atomic::Ordering::Relaxed)
							/ 		1_000_000
							);
							println!(
								"Runtime time {} ms, Validate time {} ms",
								store.time_spent_runtime.load(std::sync::atomic::Ordering::Relaxed)
							/ 		1_000_000,
								store
									.time_spent_validate
									.load(std::sync::atomic::Ordering::Relaxed) /
									1_000_000,
							);
						}
						let _ = completion.send(result);
					},
					Err(_) => return,
				}
			}
		}));
	}
	for _ in 0..num_threads {
		let store = statement_store.clone();
		let receiver = batch_queue_receiver.clone();
		executor(Box::pin(async move {
			loop {
				let task = receiver.recv().await;
				match task {
					Ok((statements, completion)) => {
						let result = store.submit_batch(statements, StatementSource::Network);
						let _ = completion.send(result);
					},
					Err(_) => return,
				}
			}
		}));
	}

	let handler = StatementHandler::new_for_testing(
		"/statement/1".into(),
		Box::new(TestNotificationService),
		(Box::pin(stream::pending()) as Pin<Box<dyn Stream<Item = ()> + Send>>).fuse(),
		network.clone(),
		TestSync::new(),
		(Box::pin(stream::pending()) as Pin<Box<dyn Stream<Item = SyncEvent> + Send>>).fuse(),
		peers,
		statement_store.clone(),
		queue_sender,
		batch_queue_sender,
		batch_limit,
	);
	(handler, peer_id, temp_dir, statement_store)
}

fn non_blocking_executor(
	handle: &tokio::runtime::Handle,
) -> Arc<dyn Fn(Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>) + Send + Sync> {
	let executor: Arc<
		dyn Fn(Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>) + Send + Sync,
	> = Arc::new({
		let h = handle.clone();
		move |fut| {
			h.spawn(fut);
		}
	});
	executor
}

fn blocking_executor(
	handle: &tokio::runtime::Handle,
) -> Arc<dyn Fn(Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>) + Send + Sync> {
	let executor: Arc<
		dyn Fn(Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>) + Send + Sync,
	> = Arc::new({
		let h = handle.clone();
		move |fut| {
			h.spawn_blocking({
				let h = h.clone();
				move || h.block_on(fut)
			});
		}
	});
	executor
}

fn bench_on_statements(c: &mut Criterion) {
	let statement_counts = [2000];
	let thread_counts = [1, 4];
	let max_runtime_instances = 8;
	let executor_types = [("blocking", true)];

	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
	let handle = runtime.handle();

	for &num_statements in &statement_counts {
		for &num_threads in &thread_counts {
			for &(executor_name, is_blocking) in &executor_types {
				let statements: Vec<Statement> =
					(0..num_statements).map(|i| create_signed_statement(i, &keypair)).collect();
				let executor = if is_blocking {
					blocking_executor(&handle)
				} else {
					non_blocking_executor(&handle)
				};

				let benchmark_name = format!(
					"on_statements/statements_{}/threads_{}/{}",
					num_statements, num_threads, executor_name
				);

				c.bench_function(&benchmark_name, |b| {
					b.iter_batched(
						|| build_handler(executor.clone(), num_threads, max_runtime_instances, 1024),
						|(mut handler, peer_id, _temp_dir, _store)| {
							handler.on_statements(peer_id, statements.clone());

							runtime.block_on(async {
								while handler.pending_statements_mut().next().await.is_some() {}
							});

							let pending = handler.pending_statements_mut();
							assert!(
								pending.is_empty(),
								"Pending statements not empty: {}",
								pending.len()
							);
							// println!("Completed processing {} ns statements",
							// handler.time_hashing);
						},
						criterion::BatchSize::LargeInput,
					)
				});
			}
		}
	}
}

fn bench_on_batch_statements(c: &mut Criterion) {
	let statement_counts = [2000];
	let thread_counts = [1,  4];
	let executor_types = [("blocking", true)];
	let batch_limits = [1, 50, 200, 1000];
	let max_runtime_instances = 8;
	
	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
	let handle = runtime.handle();

	for &num_statements in &statement_counts {
		for &num_threads in &thread_counts {
			for &(executor_name, is_blocking) in &executor_types {
				for &batch_limit in &batch_limits {
					let statements: Vec<Statement> =
						(0..num_statements).map(|i| create_signed_statement(i, &keypair)).collect();
					let executor = if is_blocking {
						blocking_executor(&handle)
					} else {
						non_blocking_executor(&handle)
					};

					let benchmark_name = format!(
						"on_batch_statements/statements_{}/threads_{}/batch_{}/{}",
						num_statements, num_threads, batch_limit, executor_name
					);

					c.bench_function(&benchmark_name, |b| {
						b.iter_batched(
							|| build_handler(executor.clone(), num_threads, max_runtime_instances, batch_limit),
							|(mut handler, peer_id, _temp_dir, _store)| {
								handler.on_batch_statements(peer_id, statements.clone());

								runtime.block_on(async {
									while handler.pending_batches_mut().next().await.is_some() {}
								});

								let pending = handler.pending_batches_mut();
								assert!(
									pending.is_empty(),
									"Pending batches not empty: {}",
									pending.len()
								);
							},
							criterion::BatchSize::LargeInput,
						)
					});
				}
			}
		}
	}
}

criterion_group!(benches, bench_on_statements, bench_on_batch_statements);
criterion_main!(benches);
