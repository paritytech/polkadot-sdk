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

use codec::Encode;
use criterion::{criterion_group, criterion_main, Criterion};
use futures::{stream, FutureExt, Stream, StreamExt};
use sc_network::{
	service::traits::{NotificationEvent, NotificationService},
	utils::LruHashSet,
	NetworkPeers, ObservedRole,
};
use sc_network_statement::{
	config::{MAX_KNOWN_STATEMENTS, MAX_PENDING_STATEMENTS},
	OnStatementsRequest, Peer, PeersState, PendingState, StatementHandler,
	StatementHandlerPrototype, StatementProcessResult, WorkerEvent,
};
use sc_network_sync::{SyncEvent, SyncEventStream};
use sc_network_types::PeerId;
use sc_statement_store::Store;
use sp_core::Pair;
use sp_statement_store::{Hash, Statement, StatementSource, StatementStore};
use std::{
	collections::HashMap,
	num::NonZeroUsize,
	pin::Pin,
	sync::{Arc, RwLock},
	time::Duration,
};
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
		unimplemented!()
	}
	fn is_offline(&self) -> bool {
		unimplemented!()
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

/// A test message sink for sending notifications to a specific peer in benchmarks.
struct BenchMessageSink {
	sender: Option<async_channel::Sender<Vec<u8>>>,
}

#[async_trait::async_trait]
impl sc_network::service::traits::MessageSink for BenchMessageSink {
	fn send_sync_notification(&self, _notification: Vec<u8>) {
		unimplemented!()
	}

	async fn send_async_notification(
		&self,
		notification: Vec<u8>,
	) -> Result<(), sc_network::error::Error> {
		if let Some(ref sender) = self.sender {
			sender
				.send(notification)
				.await
				.map_err(|_| sc_network::error::Error::ChannelClosed)?;
		}
		Ok(())
	}
}

/// A unified test notification service that supports:
/// - Simple benchmarks with no-op notifications
/// - Per-peer bounded channels for backpressure simulation
/// - Optional event injection via `next_event()`
#[derive(Debug)]
struct TestNotificationService {
	/// Optional per-peer bounded channel senders for async notifications
	peer_senders: Option<HashMap<PeerId, async_channel::Sender<Vec<u8>>>>,
	/// Optional receiver for injected events (peer connect/disconnect, incoming statements).
	/// None for simple tests and clones; Some(...) for tests with event injection.
	event_receiver: Option<async_channel::Receiver<NotificationEvent>>,
}

impl Clone for TestNotificationService {
	fn clone(&self) -> Self {
		Self {
			peer_senders: self.peer_senders.clone(),
			event_receiver: None, // Clones don't receive events
		}
	}
}

impl TestNotificationService {
	/// Create a simple test notification service for basic benchmarks.
	fn new() -> Self {
		Self { peer_senders: None, event_receiver: None }
	}

	/// Create a test notification service with per-peer bounded channels
	/// for backpressure simulation.
	fn with_per_peer_channels(
		peer_configs: &[(PeerId, PeerConfig)],
	) -> (
		Self,
		async_channel::Sender<NotificationEvent>,
		HashMap<PeerId, async_channel::Receiver<Vec<u8>>>,
	) {
		let (event_sender, event_receiver) = async_channel::unbounded();

		let mut peer_senders = HashMap::new();
		let mut peer_receivers = HashMap::new();

		for (peer_id, config) in peer_configs {
			let (sender, receiver) = async_channel::bounded(config.channel_capacity);
			peer_senders.insert(*peer_id, sender);
			peer_receivers.insert(*peer_id, receiver);
		}

		(
			Self { peer_senders: Some(peer_senders), event_receiver: Some(event_receiver) },
			event_sender,
			peer_receivers,
		)
	}
}

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
		peer: &PeerId,
		notification: Vec<u8>,
	) -> Result<(), sc_network::error::Error> {
		if let Some(ref senders) = self.peer_senders {
			if let Some(sender) = senders.get(peer) {
				sender
					.send(notification)
					.await
					.map_err(|_| sc_network::error::Error::ChannelClosed)?;
			} else {
				return Err(sc_network::error::Error::ChannelClosed);
			}
		}
		Ok(())
	}
	async fn set_handshake(&mut self, _handshake: Vec<u8>) -> Result<(), ()> {
		unimplemented!()
	}
	fn try_set_handshake(&mut self, _handshake: Vec<u8>) -> Result<(), ()> {
		unimplemented!()
	}
	async fn next_event(&mut self) -> Option<NotificationEvent> {
		match &mut self.event_receiver {
			Some(receiver) => receiver.recv().await.ok(),
			None => None,
		}
	}
	fn clone(&mut self) -> Result<Box<dyn NotificationService>, ()> {
		unimplemented!()
	}
	fn protocol(&self) -> &sc_network::types::ProtocolName {
		unimplemented!()
	}
	fn message_sink(
		&self,
		peer: &PeerId,
	) -> Option<Box<dyn sc_network::service::traits::MessageSink>> {
		let sender = self.peer_senders.as_ref().and_then(|senders| senders.get(peer).cloned());
		Some(Box::new(BenchMessageSink { sender }))
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
) -> (StatementHandler<TestNetwork, TestSync>, PeerId, tempfile::TempDir) {
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

	let (queue_sender, queue_receiver) =
		async_channel::bounded::<(Hash, Statement)>(MAX_PENDING_STATEMENTS);

	let network = TestNetwork::new();
	let peer_id = PeerId::random();
	let peers: PeersState = Arc::new(RwLock::new(HashMap::new()));
	peers.write().unwrap().insert(
		peer_id,
		Peer::new_for_testing(
			LruHashSet::new(NonZeroUsize::new(MAX_KNOWN_STATEMENTS).unwrap()),
			ObservedRole::Full,
		),
	);

	// Channel for worker events back to main loop.
	let (worker_event_sender, worker_event_receiver) =
		async_channel::bounded::<WorkerEvent>(MAX_PENDING_STATEMENTS);

	// Spawn validation workers that send results directly to main loop.
	for _ in 0..num_threads {
		let store = statement_store.clone();
		let receiver = queue_receiver.clone();
		let event_sender = worker_event_sender.clone();
		executor(Box::pin(async move {
			loop {
				let task = receiver.recv().await;
				match task {
					Ok((hash, statement)) => {
						let result = store.submit(statement, StatementSource::Network);
						let event = WorkerEvent::StatementResult(StatementProcessResult {
							hash,
							result: Some(result),
						});
						if event_sender.send(event).await.is_err() {
							return;
						}
					},
					Err(_) => return,
				}
			}
		}));
	}

	let (on_statements_sender, on_statements_receiver) =
		async_channel::bounded::<OnStatementsRequest>(MAX_PENDING_STATEMENTS);

	// Shared state for pending statements.
	let pending_state = Arc::new(RwLock::new(PendingState::new()));

	let worker_pending_state = pending_state.clone();
	let worker_statement_store = statement_store.clone();
	let worker_peers = peers.clone();
	// Spawn statements processing workers.
	for _ in 0..num_threads {
		executor(
			StatementHandlerPrototype::run_on_statements_worker(
				on_statements_receiver.clone(),
				queue_sender.clone(),
				worker_event_sender.clone(),
				worker_pending_state.clone(),
				worker_statement_store.clone(),
				worker_peers.clone(),
				None, // metrics
			)
			.boxed(),
		);
	}

	let handler = StatementHandler::new_for_testing(
		"/statement/1".into(),
		Box::new(TestNotificationService::new()),
		(Box::pin(stream::pending()) as Pin<Box<dyn Stream<Item = ()> + Send>>).fuse(),
		network.clone(),
		TestSync::new(),
		(Box::pin(stream::pending()) as Pin<Box<dyn Stream<Item = SyncEvent> + Send>>).fuse(),
		peers,
		statement_store,
		on_statements_sender,
		worker_event_receiver,
		pending_state,
	);
	(handler, peer_id, temp_dir)
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
	let statement_counts = [100, 500, 1000, 2000];
	let thread_counts = [1, 2, 4, 8];
	let peer_counts = [1, 2, 4, 8, 16];
	let max_runtime_instances = 8;
	let executor_types = [("blocking", true), ("non_blocking", false)];
	let num_chunks = 20;

	let keypair = sp_core::ed25519::Pair::from_string("//Bench", None).unwrap();
	let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
	let handle = runtime.handle();

	for &num_statements in &statement_counts {
		for &num_threads in &thread_counts {
			for &(executor_name, is_blocking) in &executor_types {
				for num_peers in &peer_counts {
					let statements: Vec<Statement> =
						(0..num_statements).map(|i| create_signed_statement(i, &keypair)).collect();
					let executor = if is_blocking {
						blocking_executor(&handle)
					} else {
						non_blocking_executor(&handle)
					};

					let benchmark_name = format!(
						"on_statements/statements_{}/peers_{}/threads_{}/{}",
						num_statements, num_peers, num_threads, executor_name
					);

					c.bench_function(&benchmark_name, |b| {
						b.iter_batched(
							|| build_handler(executor.clone(), num_threads, max_runtime_instances),
							|(mut handler, peer_id, _temp_dir)| {
								// The number of peers determines how many times we might receive a
								// statement.
								let chunks_size = statements.len() / num_chunks;
								let chunks = statements
									.chunks(chunks_size)
									.into_iter()
									.map(|chunk| chunk.to_vec())
									.collect::<Vec<_>>();
								for chunk in chunks {
									for _ in 0..*num_peers {
										handler
											.statements_queue_sender
											.send_blocking(OnStatementsRequest {
												notification: chunk.encode(),
												who: peer_id,
											})
											.unwrap();
									}
								}

								let mut count = 0;
								runtime.block_on(async {
									while let Some(event) =
										handler.results_queue_receiver.next().await
									{
										if !matches!(event, WorkerEvent::StatementResult(_)) {
											continue;
										}
										count += 1;
										if count == num_statements {
											break;
										}
									}
								});

								while handler.statements_queue_sender.len() > 0 {
									std::thread::sleep(Duration::from_millis(1));
								}
								assert_eq!(count, num_statements);
							},
							criterion::BatchSize::LargeInput,
						)
					});
				}
			}
		}
	}
}

/// Configuration for a peer in the benchmark
#[derive(Clone, Debug)]
struct PeerConfig {
	/// Channel capacity for this peer
	channel_capacity: usize,
	/// Delay between reading each notification (simulates slow peer)
	read_delay: std::time::Duration,
}

impl PeerConfig {
	fn fast(channel_capacity: usize) -> Self {
		Self { channel_capacity, read_delay: std::time::Duration::ZERO }
	}

	fn slow(channel_capacity: usize, read_delay: std::time::Duration) -> Self {
		Self { channel_capacity, read_delay }
	}
}

/// Sync oracle for benchmarks - not syncing so statements get propagated
struct BenchSync;

impl SyncEventStream for BenchSync {
	fn event_stream(&self, _name: &'static str) -> Pin<Box<dyn Stream<Item = SyncEvent> + Send>> {
		Box::pin(stream::pending())
	}
}

impl sp_consensus::SyncOracle for BenchSync {
	fn is_major_syncing(&self) -> bool {
		false // Not syncing, so statements will be propagated
	}
	fn is_offline(&self) -> bool {
		false
	}
}

impl sc_network::NetworkEventStream for BenchSync {
	fn event_stream(
		&self,
		_name: &'static str,
	) -> Pin<Box<dyn Stream<Item = sc_network::Event> + Send>> {
		Box::pin(stream::pending())
	}
}

/// Test statement store for benchmark that tracks recent statements
#[derive(Clone)]
struct BenchStatementStore {
	statements: Arc<std::sync::Mutex<HashMap<sp_statement_store::Hash, Statement>>>,
	recent_statements: Arc<std::sync::Mutex<HashMap<sp_statement_store::Hash, Statement>>>,
}

impl BenchStatementStore {
	fn new() -> Self {
		Self {
			statements: Arc::new(std::sync::Mutex::new(HashMap::new())),
			recent_statements: Arc::new(std::sync::Mutex::new(HashMap::new())),
		}
	}
}

impl sp_statement_store::StatementStore for BenchStatementStore {
	fn statements(&self) -> sp_statement_store::Result<Vec<(sp_statement_store::Hash, Statement)>> {
		Ok(self.statements.lock().unwrap().iter().map(|(h, s)| (*h, s.clone())).collect())
	}

	fn take_recent_statements(
		&self,
	) -> sp_statement_store::Result<Vec<(sp_statement_store::Hash, Statement)>> {
		Ok(self.recent_statements.lock().unwrap().drain().collect())
	}

	fn statement(
		&self,
		_hash: &sp_statement_store::Hash,
	) -> sp_statement_store::Result<Option<Statement>> {
		unimplemented!()
	}

	fn has_statement(&self, hash: &sp_statement_store::Hash) -> bool {
		self.statements.lock().unwrap().contains_key(hash)
	}

	fn statement_hashes(&self) -> Vec<sp_statement_store::Hash> {
		self.statements.lock().unwrap().keys().cloned().collect()
	}

	fn statements_by_hashes(
		&self,
		hashes: &[sp_statement_store::Hash],
		filter: &mut dyn FnMut(
			&sp_statement_store::Hash,
			&[u8],
			&Statement,
		) -> sp_statement_store::FilterDecision,
	) -> sp_statement_store::Result<(Vec<(sp_statement_store::Hash, Statement)>, usize)> {
		use codec::Encode;
		use sp_statement_store::FilterDecision;
		let statements = self.statements.lock().unwrap();
		let mut result = Vec::new();
		let mut processed = 0;
		for hash in hashes {
			let Some(stmt) = statements.get(hash) else {
				processed += 1;
				continue;
			};
			let encoded = stmt.encode();
			match filter(hash, &encoded, stmt) {
				FilterDecision::Skip => processed += 1,
				FilterDecision::Take => {
					processed += 1;
					result.push((*hash, stmt.clone()));
				},
				FilterDecision::Abort => break,
			}
		}
		Ok((result, processed))
	}

	fn broadcasts(
		&self,
		_match_all_topics: &[sp_statement_store::Topic],
	) -> sp_statement_store::Result<Vec<Vec<u8>>> {
		unimplemented!()
	}

	fn posted(
		&self,
		_match_all_topics: &[sp_statement_store::Topic],
		_dest: [u8; 32],
	) -> sp_statement_store::Result<Vec<Vec<u8>>> {
		unimplemented!()
	}

	fn posted_clear(
		&self,
		_match_all_topics: &[sp_statement_store::Topic],
		_dest: [u8; 32],
	) -> sp_statement_store::Result<Vec<Vec<u8>>> {
		unimplemented!()
	}

	fn broadcasts_stmt(
		&self,
		_match_all_topics: &[sp_statement_store::Topic],
	) -> sp_statement_store::Result<Vec<Vec<u8>>> {
		unimplemented!()
	}

	fn posted_stmt(
		&self,
		_match_all_topics: &[sp_statement_store::Topic],
		_dest: [u8; 32],
	) -> sp_statement_store::Result<Vec<Vec<u8>>> {
		unimplemented!()
	}

	fn posted_clear_stmt(
		&self,
		_match_all_topics: &[sp_statement_store::Topic],
		_dest: [u8; 32],
	) -> sp_statement_store::Result<Vec<Vec<u8>>> {
		unimplemented!()
	}

	fn submit(
		&self,
		_statement: Statement,
		_source: StatementSource,
	) -> sp_statement_store::SubmitResult {
		unimplemented!()
	}

	fn remove(&self, _hash: &sp_statement_store::Hash) -> sp_statement_store::Result<()> {
		unimplemented!()
	}

	fn remove_by(&self, _who: [u8; 32]) -> sp_statement_store::Result<()> {
		unimplemented!()
	}
}

/// Network mock for benchmark that tracks peer roles
#[derive(Clone)]
struct BenchNetwork {
	peer_roles: Arc<std::sync::Mutex<HashMap<PeerId, ObservedRole>>>,
}

impl BenchNetwork {
	fn new() -> Self {
		Self { peer_roles: Arc::new(std::sync::Mutex::new(HashMap::new())) }
	}

	fn set_peer_role(&self, peer: PeerId, role: ObservedRole) {
		self.peer_roles.lock().unwrap().insert(peer, role);
	}
}

#[async_trait::async_trait]
impl NetworkPeers for BenchNetwork {
	fn set_authorized_peers(&self, _: std::collections::HashSet<PeerId>) {}
	fn set_authorized_only(&self, _: bool) {}
	fn add_known_address(&self, _: PeerId, _: sc_network::Multiaddr) {}
	fn report_peer(&self, _peer_id: PeerId, _cost_benefit: sc_network::ReputationChange) {}
	fn peer_reputation(&self, _: &PeerId) -> i32 {
		0
	}
	fn disconnect_peer(&self, _: PeerId, _: sc_network::ProtocolName) {}
	fn accept_unreserved_peers(&self) {}
	fn deny_unreserved_peers(&self) {}
	fn add_reserved_peer(&self, _: sc_network::config::MultiaddrWithPeerId) -> Result<(), String> {
		Ok(())
	}
	fn remove_reserved_peer(&self, _: PeerId) {}
	fn set_reserved_peers(
		&self,
		_: sc_network::ProtocolName,
		_: std::collections::HashSet<sc_network::Multiaddr>,
	) -> Result<(), String> {
		Ok(())
	}
	fn add_peers_to_reserved_set(
		&self,
		_: sc_network::ProtocolName,
		_: std::collections::HashSet<sc_network::Multiaddr>,
	) -> Result<(), String> {
		Ok(())
	}
	fn remove_peers_from_reserved_set(
		&self,
		_: sc_network::ProtocolName,
		_: Vec<PeerId>,
	) -> Result<(), String> {
		Ok(())
	}
	fn sync_num_connected(&self) -> usize {
		0
	}
	fn peer_role(&self, peer: PeerId, _: Vec<u8>) -> Option<sc_network::ObservedRole> {
		self.peer_roles.lock().unwrap().get(&peer).copied()
	}
	async fn reserved_peers(&self) -> Result<Vec<PeerId>, ()> {
		Ok(vec![])
	}
}

impl sc_network::NetworkEventStream for BenchNetwork {
	fn event_stream(
		&self,
		_name: &'static str,
	) -> Pin<Box<dyn Stream<Item = sc_network::Event> + Send>> {
		Box::pin(stream::pending())
	}
}

/// Setup a handler for run loop benchmarking with per-peer channel configuration
#[allow(clippy::type_complexity)]
fn setup_run_loop_bench_per_peer(
	peer_configs: Vec<PeerConfig>,
	num_statements: usize,
) -> (
	StatementHandler<BenchNetwork, BenchSync>,
	BenchStatementStore,
	HashMap<PeerId, (async_channel::Receiver<Vec<u8>>, PeerConfig)>,
	Vec<PeerId>,
	// Keep senders alive for duration of benchmark
	async_channel::Sender<NotificationEvent>,
	async_channel::Sender<WorkerEvent>,
) {
	let statement_store = BenchStatementStore::new();
	let (statements_queue_sender, _statements_queue_receiver) = async_channel::bounded(1000);
	let (worker_event_sender, worker_event_receiver) =
		async_channel::bounded::<WorkerEvent>(MAX_PENDING_STATEMENTS);
	let pending_state = Arc::new(RwLock::new(PendingState::new()));
	let network = BenchNetwork::new();

	// Create peers with their configurations
	let peers: PeersState = Arc::new(RwLock::new(HashMap::new()));
	let mut peer_ids = Vec::new();
	let mut peer_configs_with_ids = Vec::new();

	for config in peer_configs {
		let peer_id = PeerId::random();
		peers.write().unwrap().insert(
			peer_id,
			Peer::new_for_testing(
				LruHashSet::new(NonZeroUsize::new(MAX_KNOWN_STATEMENTS).unwrap()),
				ObservedRole::Full,
			),
		);
		network.set_peer_role(peer_id, ObservedRole::Full);
		peer_ids.push(peer_id);
		peer_configs_with_ids.push((peer_id, config));
	}

	let (notification_service, event_sender, peer_receivers) =
		TestNotificationService::with_per_peer_channels(&peer_configs_with_ids);

	// Combine receivers with their configs
	let peer_receivers_with_configs: HashMap<
		PeerId,
		(async_channel::Receiver<Vec<u8>>, PeerConfig),
	> = peer_receivers
		.into_iter()
		.map(|(peer_id, receiver)| {
			let config = peer_configs_with_ids
				.iter()
				.find(|(id, _)| *id == peer_id)
				.map(|(_, c)| c.clone())
				.unwrap();
			(peer_id, (receiver, config))
		})
		.collect();

	// Create statements (~2KB each)
	for i in 0..num_statements {
		let mut stmt = Statement::new();
		let mut data = vec![0u8; 2048];
		data[0] = (i % 256) as u8;
		data[1] = (i / 256) as u8;
		stmt.set_plain_data(data);
		let hash = stmt.hash();
		statement_store.recent_statements.lock().unwrap().insert(hash, stmt);
	}

	// Use immediate-firing interval for benchmark
	let propagate_timeout = stream::once(async { () });

	let handler = StatementHandler::new_for_testing(
		"/statement/1".into(),
		Box::new(notification_service),
		(Box::pin(propagate_timeout) as Pin<Box<dyn Stream<Item = ()> + Send>>).fuse(),
		network.clone(),
		BenchSync,
		(Box::pin(stream::pending()) as Pin<Box<dyn Stream<Item = SyncEvent> + Send>>).fuse(),
		peers,
		Arc::new(statement_store.clone()),
		statements_queue_sender,
		worker_event_receiver,
		pending_state,
	);

	(
		handler,
		statement_store,
		peer_receivers_with_configs,
		peer_ids,
		event_sender,
		worker_event_sender,
	)
}

/// Size of each statement's data in the benchmark (~2KB)
const BENCH_STATEMENT_DATA_SIZE: usize = 2048;
/// Approximate encoded size per statement (data + encoding overhead)
const APPROX_ENCODED_STATEMENT_SIZE: usize = BENCH_STATEMENT_DATA_SIZE + 100;
/// MAX_STATEMENT_NOTIFICATION_SIZE from config (1MB)
const MAX_NOTIFICATION_SIZE: usize = 1024 * 1024;
/// Minimum number of notifications each peer should receive
const MIN_NOTIFICATIONS_PER_PEER: usize = 10;

/// Calculate the minimum number of statements needed to generate at least
/// `min_notifications` notification chunks.
fn statements_for_notifications(min_notifications: usize) -> usize {
	let statements_per_chunk = MAX_NOTIFICATION_SIZE / APPROX_ENCODED_STATEMENT_SIZE;
	min_notifications * statements_per_chunk
}

/// Calculate expected number of notification chunks for given statement count.
fn expected_chunks(num_statements: usize) -> usize {
	let statements_per_chunk = MAX_NOTIFICATION_SIZE / APPROX_ENCODED_STATEMENT_SIZE;
	(num_statements + statements_per_chunk - 1) / statements_per_chunk // ceiling division
}

/// Result of a peer consumer task with timing information
struct PeerConsumerResult {
	_peer_id: PeerId,
	count: usize,
	duration: std::time::Duration,
	is_slow: bool,
}

/// Spawn a consumer task for a peer that reads notifications with optional delay
/// Returns the peer ID, count, and time taken
fn spawn_peer_consumer(
	peer_id: PeerId,
	receiver: async_channel::Receiver<Vec<u8>>,
	config: PeerConfig,
	expected_count: usize,
) -> tokio::task::JoinHandle<PeerConsumerResult> {
	let is_slow = !config.read_delay.is_zero();
	tokio::spawn(async move {
		let start = std::time::Instant::now();
		let mut count = 0;
		let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(120);

		while count < expected_count && tokio::time::Instant::now() < deadline {
			match tokio::time::timeout(std::time::Duration::from_millis(500), receiver.recv()).await
			{
				Ok(Ok(_)) => {
					count += 1;
					// Apply read delay to simulate slow peer
					if !config.read_delay.is_zero() {
						tokio::time::sleep(config.read_delay).await;
					}
				},
				Ok(Err(_)) => break, // Channel closed
				Err(_) => {
					// Timeout waiting for next notification, continue to check deadline
				},
			}
		}
		let duration = start.elapsed();
		PeerConsumerResult { _peer_id: peer_id, count, duration, is_slow }
	})
}

/// Metrics collected from a benchmark run
struct BenchMetrics {
	/// Time for the fastest peer to complete
	min_peer_time: std::time::Duration,
	/// Average time across all peers
	avg_peer_time: std::time::Duration,
	/// Median time across all peers
	median_peer_time: std::time::Duration,
	/// Time for the slowest peer to complete
	max_peer_time: std::time::Duration,
	/// Time for the fastest fast peer (when there are slow peers)
	fast_peer_min: Option<std::time::Duration>,
	/// Time for the slowest fast peer (when there are slow peers)
	fast_peer_max: Option<std::time::Duration>,
}

/// Run the benchmark scenario and collect metrics
fn run_benchmark_scenario(
	runtime: &tokio::runtime::Runtime,
	handler: StatementHandler<BenchNetwork, BenchSync>,
	peer_receivers_with_configs: HashMap<PeerId, (async_channel::Receiver<Vec<u8>>, PeerConfig)>,
	chunks_per_peer: usize,
) -> BenchMetrics {
	runtime.block_on(async {
		let handle = tokio::spawn(handler.run());

		let consumer_handles: Vec<_> = peer_receivers_with_configs
			.into_iter()
			.map(|(peer_id, (receiver, config))| {
				spawn_peer_consumer(peer_id, receiver, config, chunks_per_peer)
			})
			.collect();

		let mut results = Vec::new();
		for consumer in consumer_handles {
			let result = consumer.await.expect("Consumer task panicked");
			assert_eq!(
				result.count, chunks_per_peer,
				"Peer should receive {} notifications, got {}",
				chunks_per_peer, result.count
			);
			results.push(result);
		}

		handle.abort();

		// Calculate metrics
		let mut all_times: Vec<_> = results.iter().map(|r| r.duration).collect();
		let min_peer_time = *all_times.iter().min().unwrap();
		let max_peer_time = *all_times.iter().max().unwrap();
		let avg_peer_time = all_times.iter().sum::<std::time::Duration>() / all_times.len() as u32;

		// Calculate median
		all_times.sort();
		let median_peer_time = if all_times.len() % 2 == 0 {
			let mid = all_times.len() / 2;
			(all_times[mid - 1] + all_times[mid]) / 2
		} else {
			all_times[all_times.len() / 2]
		};

		// Separate fast peer metrics if there are slow peers
		let fast_times: Vec<_> =
			results.iter().filter(|r| !r.is_slow).map(|r| r.duration).collect();
		let (fast_peer_min, fast_peer_max) = if fast_times.len() < results.len() {
			// There are slow peers, so report fast peer metrics separately
			(fast_times.iter().min().copied(), fast_times.iter().max().copied())
		} else {
			(None, None)
		};

		BenchMetrics {
			min_peer_time,
			avg_peer_time,
			median_peer_time,
			max_peer_time,
			fast_peer_min,
			fast_peer_max,
		}
	})
}

fn bench_run_loop_propagation(c: &mut Criterion) {
	use criterion::BenchmarkId;

	// Fixed configuration: 16 peers
	let num_peers = 16;
	// Use statement count that ensures at least MIN_NOTIFICATIONS_PER_PEER chunks per peer
	let num_statements = statements_for_notifications(MIN_NOTIFICATIONS_PER_PEER);
	let chunks_per_peer = expected_chunks(num_statements);

	// Channel capacity for "no backpressure" = large enough for all chunks
	let no_backpressure_capacity = chunks_per_peer * 2;
	// Channel capacity for "some backpressure" = half of needed
	let backpressure_capacity = chunks_per_peer / 2;

	// Slow peer delay
	let slow_peer_delay = std::time::Duration::from_millis(10);

	let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

	let mut group =
		c.benchmark_group(format!("run_loop_propagation/16_peers/{}_statements", num_statements));

	// Define scenarios
	let scenarios = [
		("all_fast_no_backpressure", false, no_backpressure_capacity, 0),
		("all_fast_with_backpressure", false, backpressure_capacity, 0),
		("2_slow_with_backpressure", true, backpressure_capacity, 2),
	];

	// For each scenario, benchmark different metrics
	let metrics = [
		"slowest_peer",
		"fastest_peer",
		"avg_peer",
		"median_peer",
		"fastest_fast_peer",
		"slowest_fast_peer",
	];

	for (scenario_name, has_slow_peers, capacity, num_slow) in &scenarios {
		for metric in &metrics {
			// Skip fast_peer metrics for scenarios without slow peers
			if (*metric == "fastest_fast_peer" || *metric == "slowest_fast_peer") && !has_slow_peers
			{
				continue;
			}

			let bench_id = BenchmarkId::new(*scenario_name, *metric);

			group.bench_function(bench_id, |b| {
				b.iter_custom(|iters| {
					let mut total = std::time::Duration::ZERO;

					for _ in 0..iters {
						// Setup
						let peer_configs: Vec<PeerConfig> = (0..num_peers)
							.map(|i| {
								if i < *num_slow {
									PeerConfig::slow(*capacity, slow_peer_delay)
								} else {
									PeerConfig::fast(*capacity)
								}
							})
							.collect();
						let (
							handler,
							_store,
							peer_receivers_with_configs,
							_peer_ids,
							_event_sender,
							_worker_event_sender,
						) = setup_run_loop_bench_per_peer(peer_configs, num_statements);

						// Run and collect metrics
						let metrics = run_benchmark_scenario(
							&runtime,
							handler,
							peer_receivers_with_configs,
							chunks_per_peer,
						);

						// Select the metric to report
						let duration = match *metric {
							"slowest_peer" => metrics.max_peer_time,
							"fastest_peer" => metrics.min_peer_time,
							"avg_peer" => metrics.avg_peer_time,
							"median_peer" => metrics.median_peer_time,
							"fastest_fast_peer" =>
								metrics.fast_peer_min.unwrap_or(metrics.min_peer_time),
							"slowest_fast_peer" =>
								metrics.fast_peer_max.unwrap_or(metrics.max_peer_time),
							_ => metrics.max_peer_time,
						};

						total += duration;
					}

					total
				});
			});
		}
	}

	group.finish();
}

criterion_group!(benches, bench_on_statements, bench_run_loop_propagation);
criterion_main!(benches);
