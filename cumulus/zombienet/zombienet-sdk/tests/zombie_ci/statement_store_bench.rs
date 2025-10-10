// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Benchmarking statement store performance

use anyhow::anyhow;
use codec::{Decode, Encode};
use log::{debug, info, trace};
use sc_statement_store::{DEFAULT_MAX_TOTAL_SIZE, DEFAULT_MAX_TOTAL_STATEMENTS};
use sp_core::{blake2_256, sr25519, Bytes, Pair};
use sp_statement_store::{Statement, Topic};
use std::{
	collections::HashMap,
	sync::atomic::{AtomicBool, AtomicU64, Ordering},
	time::Duration,
};
use tokio::time::timeout;
use zombienet_sdk::{
	subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params},
	LocalFileSystem, Network, NetworkConfigBuilder,
};

const GROUP_SIZE: u32 = 6;
const PARTICIPANT_SIZE: u32 = GROUP_SIZE * 8333; // Target ~50,000 total
const MESSAGE_SIZE: usize = 5 * 1024; // 5KiB
const MESSAGE_COUNT: usize = 0;
const MAX_RETRIES: u32 = 100;
const RETRY_DELAY_MS: u64 = 500;
const RECEIVE_DELAY_MS: u64 = 1000;
const TIMEOUT_MS: u64 = 3000;

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_one_node_bench() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = spawn_network().await?;

	info!("Starting statement store benchmark with {} participants", PARTICIPANT_SIZE);

	let target_node = "alice";
	let node = network.get_node(target_node)?;
	let rpc_client = node.rpc().await?;
	info!("Created single RPC client for target node: {}", target_node);

	let mut participants = Vec::with_capacity(PARTICIPANT_SIZE as usize);
	for i in 0..(PARTICIPANT_SIZE) as usize {
		participants.push(Participant::new(i as u32, rpc_client.clone()));
	}

	let handles: Vec<_> = participants
		.into_iter()
		.map(|mut p| tokio::spawn(async move { p.run().await }))
		.collect();

	let mut all_stats = Vec::new();
	for handle in handles {
		let stats = handle.await??;
		all_stats.push(stats);
	}

	let aggregated_stats = ParticipantStats::aggregate(all_stats);
	aggregated_stats.log_summary();

	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_many_nodes_bench() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = spawn_network().await?;

	info!("Starting statement store benchmark with {} participants", PARTICIPANT_SIZE);

	let collator_names = ["alice", "bob", "charlie", "dave", "eve", "ferdie"];
	let mut rpc_clients = Vec::new();
	for &name in &collator_names {
		let node = network.get_node(name)?;
		let rpc_client = node.rpc().await?;
		rpc_clients.push(rpc_client);
	}
	info!("Created RPC clients for {} collator nodes", rpc_clients.len());

	let mut participants = Vec::with_capacity(PARTICIPANT_SIZE as usize);
	for i in 0..(PARTICIPANT_SIZE) as usize {
		let client_idx = i % collator_names.len();
		participants.push(Participant::new(i as u32, rpc_clients[client_idx].clone()));
	}
	info!(
		"{} participants were distributed across {} nodes: {} participants per node",
		PARTICIPANT_SIZE,
		collator_names.len(),
		PARTICIPANT_SIZE as usize / collator_names.len()
	);

	let handles: Vec<_> = participants
		.into_iter()
		.map(|mut participant| tokio::spawn(async move { participant.run().await }))
		.collect();

	let mut all_stats = Vec::new();
	for handle in handles {
		let stats = handle.await??;
		all_stats.push(stats);
	}

	let aggregated_stats = ParticipantStats::aggregate(all_stats);
	aggregated_stats.log_summary();

	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_memory_stress_bench() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = spawn_network().await?;

	let target_node = "alice";
	let node = network.get_node(target_node)?;
	let rpc_client = node.rpc().await?;
	info!("Created single RPC client for target node: {}", target_node);

	let total_tasks = 64 * 1024;
	let payload_size = 1024;
	let submit_capacity =
		DEFAULT_MAX_TOTAL_STATEMENTS.min(DEFAULT_MAX_TOTAL_SIZE / payload_size) as u64;
	let statements_per_task = submit_capacity / total_tasks as u64;
	let collator_names = ["alice", "bob", "charlie", "dave", "eve", "ferdie"];
	let num_collators = collator_names.len() as u64;
	let propogation_capacity = submit_capacity * (num_collators - 1); // 5x per node
	let start_time = std::time::Instant::now();

	info!("Starting memory stress benchmark with {} tasks, each submitting {} statements of {}B payload, total submit capacity per node: {}, total propagation capacity: {}",
		total_tasks, statements_per_task, payload_size, submit_capacity, propogation_capacity);

	for _ in 0..total_tasks {
		let rpc_client = rpc_client.clone();
		tokio::spawn(async move {
			let (keyring, _) = sr25519::Pair::generate();
			let public = keyring.public().0;

			for statement_count in 0..statements_per_task {
				let mut statement = Statement::new();
				let topic = |idx: usize| {
					blake2_256(format!("{}{}{:?}", idx, statement_count, public).as_bytes())
				};
				statement.set_topic(0, topic(0));
				statement.set_topic(1, topic(1));
				statement.set_topic(2, topic(2));
				statement.set_topic(3, topic(3));
				statement.set_plain_data(vec![0u8; payload_size]);
				statement.sign_sr25519_private(&keyring);

				loop {
					let statement_bytes: Bytes = statement.encode().into();
					let Err(err) = rpc_client
						.request::<()>("statement_submit", rpc_params![statement_bytes])
						.await
					else {
						break; // Successfully submitted
					};

					if err.to_string().contains("Statement store error: Store is full") {
						info!("Statement store is full, {}/{} statements submitted, `statements_per_task` overestimated", statement_count, statements_per_task);
						break;
					}

					info!(
						"Failed to submit statement, retrying in {}ms: {:?}",
						RETRY_DELAY_MS, err
					);
					tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
				}
			}
		});
	}

	info!("All {} tasks spawned in {:.2}s", total_tasks, start_time.elapsed().as_secs_f64());

	let mut prev_submitted: HashMap<&str, u64> = HashMap::new();
	let mut prev_propagated: HashMap<&str, u64> = HashMap::new();
	for &name in &collator_names {
		prev_submitted.insert(name, 0);
		prev_propagated.insert(name, 0);
	}

	loop {
		let interval = 5;
		tokio::time::sleep(Duration::from_secs(interval)).await;
		let elapsed = start_time.elapsed().as_secs();

		// Collect submitted metrics
		let mut submitted_metrics = Vec::new();
		for &name in &collator_names {
			let node = network.get_node(name)?;
			let prev_count = prev_submitted.get(name).copied().unwrap_or(0);

			let current_count = Cell::new(0.0f64);
			node.wait_metric_with_timeout(
				"substrate_sub_statement_store_submitted_statements",
				|count| {
					current_count.set(count);
					true
				},
				30u64,
			)
			.await?;

			let count = current_count.get() as u64;
			let delta = count - prev_count;
			let rate = delta / interval;
			submitted_metrics.push((name, count, rate));
			prev_submitted.insert(name, count);
		}

		// Collect propagated metrics
		let mut propagated_metrics = Vec::new();
		for &name in &collator_names {
			let node = network.get_node(name)?;
			let prev_count = prev_propagated.get(name).copied().unwrap_or(0);

			let current_count = Cell::new(0.0f64);
			node.wait_metric_with_timeout(
				"substrate_sync_propagated_statements",
				|count| {
					current_count.set(count);
					true
				},
				30u64,
			)
			.await?;

			let count = current_count.get() as u64;
			let delta = count - prev_count;
			let rate = delta / interval;
			propagated_metrics.push((name, count, rate));
			prev_propagated.insert(name, count);
		}

		info!("[{:>3}s]  Statements  submitted                 propagated", elapsed);
		for i in 0..collator_names.len() {
			let (sub_name, sub_count, sub_rate) = submitted_metrics[i];
			let (prop_name, prop_count, prop_rate) = propagated_metrics[i];
			assert_eq!(sub_name, prop_name);

			let sub_percentage = sub_count * 100 / submit_capacity;
			let prop_percentage = prop_count * 100 / propogation_capacity;

			info!(
				"         {:<8}  {:>8} {:>3}% {:>8}/s   {:>8} {:>3}% {:>8}/s",
				sub_name,
				sub_count,
				sub_percentage,
				sub_rate,
				prop_count,
				prop_percentage,
				prop_rate
			);
		}

		let total_submitted: u64 = submitted_metrics.iter().map(|(_, count, _)| *count).sum();
		if total_submitted == submit_capacity * num_collators {
			info!("Reached total submit capacity of {} statements per node in {}s, benchmark completed successfully", submit_capacity, elapsed);
			break;
		}
	}

	Ok(())
}

async fn spawn_network() -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_node(|node| node.with_name("validator-0"))
				.with_node(|node| node.with_name("validator-1"))
		})
		.with_parachain(|p| {
			p.with_id(2400)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain_spec_path("tests/zombie_ci/people-rococo-spec.json")
				.with_default_args(vec![
					"--force-authoring".into(),
					"-lstatement-store=info".into(),
					"--enable-statement-store".into(),
					"--rpc-max-connections=50000".into(),
				])
				.with_collator(|n| n.with_name("alice"))
				.with_collator(|n| n.with_name("bob"))
				.with_collator(|n| n.with_name("charlie"))
				.with_collator(|n| n.with_name("dave"))
				.with_collator(|n| n.with_name("eve"))
				.with_collator(|n| n.with_name("ferdie"))
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

	Ok(network)
}

#[derive(Encode, Decode, Debug, Clone)]
struct StatementRequest {
	request_id: u32,
	data: Vec<u8>,
}

#[derive(Encode, Decode, Debug, Clone)]
struct StatementResponse {
	request_id: u32,
	response_code: u8,
}

#[derive(Encode, Decode, Debug, Clone)]
enum StatementAcknowledge {
	SymmetricKeyReceived { sender_idx: u32 },
	RequestReceived { sender_idx: u32, request_id: u32 },
	ResponseReceived { sender_idx: u32, request_id: u32 },
}

#[derive(Debug, Clone)]
struct ParticipantStats {
	total_time: Duration,
	sent_count: u32,
	received_count: u32,
	retry_count: u32,
}

#[derive(Debug)]
struct AggregatedStats {
	total_participants: u32,
	min_time: Duration,
	max_time: Duration,
	avg_time: Duration,
	min_sent: u32,
	max_sent: u32,
	avg_sent: u32,
	min_received: u32,
	max_received: u32,
	avg_received: u32,
	min_retries: u32,
	max_retries: u32,
	avg_retries: u32,
}

impl ParticipantStats {
	fn aggregate(all_stats: Vec<ParticipantStats>) -> AggregatedStats {
		let total_participants = all_stats.len() as u32;
		let total_sent: u32 = all_stats.iter().map(|s| s.sent_count).sum();
		let total_received: u32 = all_stats.iter().map(|s| s.received_count).sum();
		let total_retries: u32 = all_stats.iter().map(|s| s.retry_count).sum();

		let min_time = all_stats.iter().map(|s| s.total_time).min().unwrap_or(Duration::ZERO);
		let max_time = all_stats.iter().map(|s| s.total_time).max().unwrap_or(Duration::ZERO);
		let avg_time = Duration::from_secs_f64(
			all_stats.iter().map(|s| s.total_time.as_secs_f64()).sum::<f64>() /
				total_participants as f64,
		);

		let min_sent = all_stats.iter().map(|s| s.sent_count).min().unwrap_or(0);
		let max_sent = all_stats.iter().map(|s| s.sent_count).max().unwrap_or(0);
		let avg_sent = total_sent / total_participants;

		let min_received = all_stats.iter().map(|s| s.received_count).min().unwrap_or(0);
		let max_received = all_stats.iter().map(|s| s.received_count).max().unwrap_or(0);
		let avg_received = total_received / total_participants;

		let min_retries = all_stats.iter().map(|s| s.retry_count).min().unwrap_or(0);
		let max_retries = all_stats.iter().map(|s| s.retry_count).max().unwrap_or(0);
		let avg_retries = total_retries / total_participants;

		AggregatedStats {
			total_participants,
			min_time,
			max_time,
			avg_time,
			min_sent,
			max_sent,
			avg_sent,
			min_received,
			max_received,
			avg_received,
			min_retries,
			max_retries,
			avg_retries,
		}
	}
}

impl AggregatedStats {
	fn log_summary(&self) {
		info!("Statement store benchmark completed successfully");
		info!("Participants: {}", self.total_participants);
		info!(
			"Messages sent - Min: {}, Max: {}, Avg: {}",
			self.min_sent, self.max_sent, self.avg_sent
		);
		info!(
			"Messages received - Min: {}, Max: {}, Avg: {}",
			self.min_received, self.max_received, self.avg_received
		);
		info!(
			"Retries - Min: {}, Max: {}, Avg: {}",
			self.min_retries, self.max_retries, self.avg_retries
		);
		info!(
			"Time - Min: {:.2}s, Max: {:.2}s, Avg: {:.2}s",
			self.min_time.as_secs_f64(),
			self.max_time.as_secs_f64(),
			self.avg_time.as_secs_f64()
		);
	}
}

struct Participant {
	idx: u32,
	keyring: sr25519::Pair,
	session_key: sr25519::Pair,
	group_members: Vec<u32>,
	session_keys: HashMap<u32, sr25519::Public>,
	symmetric_keys: HashMap<u32, sr25519::Public>,
	sent_symmetric_key: HashMap<u32, bool>,
	received_symmetric_key: HashMap<u32, bool>,
	sent_req: HashMap<(u32, u32), bool>,
	received_req: HashMap<(u32, u32), bool>,
	sent_res: HashMap<(u32, u32), bool>,
	received_res: HashMap<(u32, u32), bool>,
	sent_count: u32,
	received_count: u32,
	pending_res: HashMap<u32, Option<u32>>,
	retry_count: u32,
	rpc_client: RpcClient,
}

impl Participant {
	fn log_target(&self) -> String {
		format!("participant_{}", self.idx)
	}

	fn new(idx: u32, rpc_client: RpcClient) -> Self {
		debug!(target: &format!("participant_{}", idx), "Initializing participant {}", idx);
		let (keyring, _) = sr25519::Pair::generate();
		let (session_key, _) = sr25519::Pair::generate();

		let group_start = (idx / GROUP_SIZE) * GROUP_SIZE;
		let group_end = group_start + GROUP_SIZE;
		let group_members: Vec<u32> = (group_start..group_end).filter(|&i| i != idx).collect();

		let mut symmetric_keys = HashMap::new();
		for &other_idx in &group_members {
			if other_idx > idx {
				let (pair, _) = sr25519::Pair::generate();
				symmetric_keys.insert(other_idx, pair.public());
			}
		}

		Self {
			keyring,
			session_key,
			idx,
			group_members,
			session_keys: HashMap::new(),
			symmetric_keys,
			sent_symmetric_key: HashMap::new(),
			received_symmetric_key: HashMap::new(),
			sent_req: HashMap::new(),
			received_req: HashMap::new(),
			sent_res: HashMap::new(),
			received_res: HashMap::new(),
			pending_res: HashMap::new(),
			sent_count: 0,
			received_count: 0,
			retry_count: 0,
			rpc_client,
		}
	}

	async fn retry_sleep(&mut self) -> Result<(), anyhow::Error> {
		if self.retry_count >= MAX_RETRIES {
			return Err(anyhow!("[{}] No more retry attempts", self.idx))
		}

		self.retry_count += 1;
		if self.retry_count % 10 == 0 {
			debug!(target: &self.log_target(), "[{}] Retry attempt {}", self.idx, self.retry_count);
		}
		let delay_ms = std::cmp::min(
			RETRY_DELAY_MS * (1 << std::cmp::min(self.retry_count / 5, 4)),
			TIMEOUT_MS,
		);
		tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;

		Ok(())
	}

	async fn receive_sleep(&mut self) {
		tokio::time::sleep(tokio::time::Duration::from_millis(RECEIVE_DELAY_MS)).await;
	}

	async fn receive_statements_with_retry<T, F, R>(
		&mut self,
		mut pending: Vec<T>,
		topic_generator: F,
		result_processor: R,
	) -> Result<(), anyhow::Error>
	where
		T: Clone + PartialEq + std::fmt::Debug,
		F: Fn(&T) -> Vec<Topic>,
		R: Fn(&mut Self, &T, &Statement) -> Result<bool, anyhow::Error>,
	{
		while !pending.is_empty() {
			let mut completed_this_round = Vec::new();

			for item in &pending {
				match timeout(
					Duration::from_millis(TIMEOUT_MS),
					self.statement_broadcasts_statement(topic_generator(item)),
				)
				.await
				{
					Ok(Ok(statements)) if !statements.is_empty() => {
						if let Some(statement) = statements.first() {
							match result_processor(self, item, statement) {
								Ok(true) => completed_this_round.push(item.clone()),
								Ok(false) => {}, // Continue waiting for this item
								Err(e) => return Err(e),
							}
						}
					},
					Ok(Ok(statements)) if statements.is_empty() => {
						debug!(target: &self.log_target(), "[{}] No statements received for item {:?}", self.idx, item);
					},
					err => {
						debug!(target: &self.log_target(), "[{}] Cannot receive statements for item {:?}, err: {:?}", self.idx, item, err);
					},
				}
			}

			for completed_item in completed_this_round {
				pending.retain(|x| x != &completed_item);
			}

			if !pending.is_empty() {
				self.retry_sleep().await?;
			}
		}

		Ok(())
	}

	async fn send_session_key(&mut self) -> Result<(), anyhow::Error> {
		let statement = self.public_key_statement();
		self.statement_submit(statement).await
	}

	async fn receive_session_keys(&mut self) -> Result<(), anyhow::Error> {
		let group_members = self.group_members.clone();

		self.receive_statements_with_retry(
			group_members,
			|&idx| topics_session_key(idx),
			|participant, &idx, statement| {
				let data = statement.data().expect("Must contain session_key");
				let session_key = sr25519::Public::from_raw(data[..].try_into()?);
				participant.session_keys.insert(idx, session_key);
				Ok(true)
			},
		)
		.await?;

		assert_eq!(
			self.session_keys.len(),
			self.group_members.len(),
			"Not every session key received"
		);

		Ok(())
	}

	async fn send_symmetric_keys(&mut self) -> Result<(), anyhow::Error> {
		let group_members = self.group_members.clone();
		for receiver_idx in group_members {
			let Some(statement) = self.symmetric_key_statement(receiver_idx) else { continue };
			self.statement_submit(statement).await?;
			self.sent_symmetric_key.insert(receiver_idx, false);
		}

		assert_eq!(
			self.sent_symmetric_key.len(),
			self.group_members.iter().filter(|&i| *i > self.idx).count(),
			"Not every symmetric key sent"
		);

		Ok(())
	}

	async fn receive_symmetric_keys(&mut self) -> Result<(), anyhow::Error> {
		let session_keys = self.session_keys.clone();
		let pending: Vec<(u32, sr25519::Public)> = session_keys
			.into_iter()
			.filter(|(sender_idx, _)| *sender_idx < self.idx)
			.collect();

		let own_session_key = self.session_key.public();
		self.receive_statements_with_retry(
			pending,
			|&(_, sender_session_key)| topics_symmetric_key(&sender_session_key, &own_session_key),
			|participant, &(sender_idx, _), statement| {
				let data = statement.data().expect("Must contain symmetric key");
				let symmetric_key = sr25519::Public::from_raw(
					data.as_slice()
						.try_into()
						.map_err(|e| anyhow!("Failed to parse symmetric key: {}", e))?,
				);
				participant.symmetric_keys.insert(sender_idx, symmetric_key);
				participant.received_symmetric_key.insert(sender_idx, false);
				Ok(true)
			},
		)
		.await?;

		assert_eq!(
			self.symmetric_keys.len(),
			self.group_members.len(),
			"Not every symmetric key received"
		);
		assert_eq!(
			self.received_symmetric_key.len(),
			self.group_members.iter().filter(|&i| *i < self.idx).count(),
			"Not every symmetric key received"
		);

		Ok(())
	}

	async fn send_symmetric_key_acknowledgments(&mut self) -> Result<(), anyhow::Error> {
		let received_keys: Vec<u32> = self
			.received_symmetric_key
			.iter()
			.filter(|(_, &sent)| !sent)
			.map(|(&idx, _)| idx)
			.collect();

		for sender_idx in received_keys {
			if let Some(statement) = self.create_symmetric_key_ack_statement(sender_idx) {
				self.statement_submit(statement).await?;
				self.received_symmetric_key.insert(sender_idx, true);
			}
		}

		assert!(
			self.received_symmetric_key.values().all(|ack| *ack),
			"Not every symmetric key ack sent"
		);

		Ok(())
	}

	async fn receive_symmetric_key_acknowledgments(&mut self) -> Result<(), anyhow::Error> {
		let pending: Vec<(u32, sr25519::Public)> = self
			.sent_symmetric_key
			.iter()
			.filter(|(_, &received)| !received)
			.map(|(&receiver_idx, _)| {
				let receiver_session_key =
					*self.session_keys.get(&receiver_idx).expect("Receiver already exists");
				(receiver_idx, receiver_session_key)
			})
			.collect();

		let own_session_key = self.session_key.public();
		self.receive_statements_with_retry(
			pending,
			|&(_, receiver_session_key)| topics_ack(&receiver_session_key, &own_session_key),
			|participant, &(receiver_idx, _), statement| {
				let data = statement.data().expect("Must contain acknowledgment");
				let ack = StatementAcknowledge::decode(&mut &data[..])?;

				match ack {
					StatementAcknowledge::SymmetricKeyReceived { sender_idx: ack_sender_idx } =>
						if ack_sender_idx == receiver_idx {
							participant.sent_symmetric_key.insert(receiver_idx, true);
							Ok(true)
						} else {
							Ok(false)
						},
					_ => Ok(false),
				}
			},
		)
		.await?;

		assert!(
			self.sent_symmetric_key.values().all(|ack| *ack),
			"Not every symmetric key ack received"
		);

		Ok(())
	}

	async fn statement_submit(&mut self, statement: Statement) -> Result<(), anyhow::Error> {
		let statement_bytes: Bytes = statement.encode().into();
		let _: () = self
			.rpc_client
			.request("statement_submit", rpc_params![statement_bytes])
			.await?;

		self.sent_count += 1;
		trace!(target: &self.log_target(), "[{}] Submitted statement (counter: {})", self.idx, self.sent_count);

		Ok(())
	}

	async fn statement_broadcasts_statement(
		&mut self,
		topics: Vec<Topic>,
	) -> Result<Vec<Statement>, anyhow::Error> {
		let statements: Vec<Bytes> = self
			.rpc_client
			.request("statement_broadcastsStatement", rpc_params![topics])
			.await?;

		let mut decoded_statements = Vec::new();
		for statement_bytes in &statements {
			let statement = Statement::decode(&mut &statement_bytes[..])?;
			decoded_statements.push(statement);
		}

		self.received_count += decoded_statements.len() as u32;
		trace!(target: &self.log_target(), "[{}] Received {} statements (counter: {})", self.idx, decoded_statements.len(), self.received_count);

		Ok(decoded_statements)
	}

	fn public_key_statement(&self) -> Statement {
		let mut statement = Statement::new();
		statement.set_channel([0u8; 32]);
		statement.set_priority(self.sent_count);
		statement.set_topic(0, topic_public_key());
		statement.set_topic(1, topic_idx(self.idx));
		statement.set_plain_data(self.session_key.public().to_vec());
		statement.sign_sr25519_private(&self.keyring);

		statement
	}

	fn symmetric_key_statement(&self, receiver_idx: u32) -> Option<Statement> {
		let (Some(symmetric_key), Some(receiver_session_key)) =
			(self.symmetric_keys.get(&receiver_idx), self.session_keys.get(&receiver_idx))
		else {
			return None
		};

		let mut statement = Statement::new();

		let topic = topic_pair(&self.session_key.public(), receiver_session_key);
		let channel = channel_pair(&self.session_key.public(), receiver_session_key);

		statement.set_channel(channel);
		statement.set_priority(self.sent_count);
		statement.set_topic(0, topic);
		statement.set_plain_data(symmetric_key.to_vec());
		statement.sign_sr25519_private(&self.keyring);

		Some(statement)
	}

	fn create_request_statement(&mut self, receiver_idx: u32) -> Option<(Statement, u32)> {
		let (Some(_symmetric_key), Some(receiver_session_key)) =
			(self.symmetric_keys.get(&receiver_idx), self.session_keys.get(&receiver_idx))
		else {
			return None
		};

		let request_id = self.sent_count;

		// Create 5KiB payload
		let mut data = vec![0u8; MESSAGE_SIZE];
		for (i, byte) in data.iter_mut().enumerate() {
			*byte = (i % 256) as u8; // Simple pattern for testing
		}

		let request = StatementRequest { request_id, data };
		let request_data = request.encode();
		let mut statement = Statement::new();

		let topic0 = blake2_256(b"request");
		let topic1 = topic_pair(&self.session_key.public(), receiver_session_key);
		let channel = channel_request(&self.session_key.public(), receiver_session_key);

		statement.set_topic(0, topic0);
		statement.set_topic(1, topic1);
		statement.set_channel(channel);
		statement.set_priority(self.sent_count);
		statement.set_plain_data(request_data);
		statement.sign_sr25519_private(&self.keyring);

		Some((statement, request_id))
	}

	fn create_response_statement(
		&mut self,
		request_id: u32,
		receiver_idx: u32,
	) -> Option<Statement> {
		let (Some(_symmetric_key), Some(receiver_session_key)) =
			(self.symmetric_keys.get(&receiver_idx), self.session_keys.get(&receiver_idx))
		else {
			return None
		};

		let response = StatementResponse { request_id, response_code: 0 };
		let response_data = response.encode();

		let mut statement = Statement::new();

		let topic0 = blake2_256(b"response");
		let topic1 = topic_pair(&self.session_key.public(), receiver_session_key);
		let channel = channel_response(&self.session_key.public(), receiver_session_key);

		statement.set_topic(0, topic0);
		statement.set_topic(1, topic1);
		statement.set_channel(channel);
		statement.set_priority(self.sent_count);
		statement.set_plain_data(response_data);
		statement.sign_sr25519_private(&self.keyring);

		Some(statement)
	}

	fn create_symmetric_key_ack_statement(&self, sender_idx: u32) -> Option<Statement> {
		let sender_session_key = self.session_keys.get(&sender_idx)?;

		let ack = StatementAcknowledge::SymmetricKeyReceived { sender_idx: self.idx };
		let ack_data = ack.encode();

		let mut statement = Statement::new();

		let topic0 = topic_ack();
		let topic1 = topic_pair(&self.session_key.public(), sender_session_key);
		let channel = channel_pair(&self.session_key.public(), sender_session_key);

		statement.set_topic(0, topic0);
		statement.set_topic(1, topic1);
		statement.set_channel(channel);
		statement.set_priority(self.sent_count);
		statement.set_plain_data(ack_data);
		statement.sign_sr25519_private(&self.keyring);

		Some(statement)
	}

	fn create_request_ack_statement(&self, sender_idx: u32, request_id: u32) -> Option<Statement> {
		let sender_session_key = self.session_keys.get(&sender_idx)?;

		let ack = StatementAcknowledge::RequestReceived { sender_idx: self.idx, request_id };
		let ack_data = ack.encode();

		let mut statement = Statement::new();

		let topic0 = topic_ack();
		let topic1 = topic_pair(&self.session_key.public(), sender_session_key);
		let channel = channel_pair(&self.session_key.public(), sender_session_key);

		statement.set_topic(0, topic0);
		statement.set_topic(1, topic1);
		statement.set_channel(channel);
		statement.set_priority(self.sent_count);
		statement.set_plain_data(ack_data);
		statement.sign_sr25519_private(&self.keyring);

		Some(statement)
	}

	fn create_response_ack_statement(&self, sender_idx: u32, request_id: u32) -> Option<Statement> {
		let sender_session_key = self.session_keys.get(&sender_idx)?;

		let ack = StatementAcknowledge::ResponseReceived { sender_idx: self.idx, request_id };
		let ack_data = ack.encode();

		let mut statement = Statement::new();

		let topic0 = topic_ack();
		let topic1 = topic_pair(&self.session_key.public(), sender_session_key);
		let channel = channel_pair(&self.session_key.public(), sender_session_key);

		statement.set_topic(0, topic0);
		statement.set_topic(1, topic1);
		statement.set_channel(channel);
		statement.set_priority(self.sent_count);
		statement.set_plain_data(ack_data);
		statement.sign_sr25519_private(&self.keyring);

		Some(statement)
	}

	async fn send_requests(&mut self, round: usize) -> Result<(), anyhow::Error> {
		let group_members = self.group_members.clone();
		for receiver_idx in group_members {
			let (statement, request_id) =
				self.create_request_statement(receiver_idx).expect("Receiver must present");
			self.statement_submit(statement).await?;
			self.sent_req.insert((receiver_idx, request_id), false);
		}

		assert_eq!(
			self.sent_req.len(),
			self.group_members.len() * (round + 1),
			"Not every request sent"
		);

		Ok(())
	}

	async fn receive_requests(&mut self, round: usize) -> Result<(), anyhow::Error> {
		let session_keys = self.session_keys.clone();
		let pending: Vec<(u32, sr25519::Public)> =
			session_keys.iter().map(|(&idx, &key)| (idx, key)).collect();

		let own_session_key = self.session_key.public();
		self.receive_statements_with_retry(
			pending,
			|&(_, sender_session_key)| topics_request(&sender_session_key, &own_session_key),
			|participant, &(sender_idx, _), statement| {
				let data = statement.data().expect("Must contain request");
				let req = StatementRequest::decode(&mut &data[..])?;

				if !participant.received_req.contains_key(&(sender_idx, req.request_id)) {
					participant.received_req.insert((sender_idx, req.request_id), false);
					participant.pending_res.insert(sender_idx, Some(req.request_id));
					Ok(true)
				} else {
					Ok(false)
				}
			},
		)
		.await?;

		assert_eq!(
			self.received_req.len(),
			self.group_members.len() * (round + 1),
			"Not every request received"
		);
		assert_eq!(
			self.pending_res.values().filter(|i| i.is_some()).count(),
			self.group_members.len(),
			"Not every request received"
		);

		Ok(())
	}

	async fn send_responses(&mut self, round: usize) -> Result<(), anyhow::Error> {
		let group_members = self.group_members.clone();
		for receiver_idx in group_members {
			if let Some(req_id) = self.pending_res.get_mut(&receiver_idx).and_then(|r| r.take()) {
				let statement = self
					.create_response_statement(req_id, receiver_idx)
					.expect("Receiver must present");
				self.statement_submit(statement).await?;
				self.sent_res.insert((receiver_idx, req_id), false);
			}
		}

		assert_eq!(
			self.sent_res.len(),
			self.group_members.len() * (round + 1),
			"Not every response sent"
		);
		assert!(self.pending_res.values().all(|i| i.is_none()), "Not every response sent");

		Ok(())
	}

	async fn send_request_acknowledgments(&mut self) -> Result<(), anyhow::Error> {
		let received_req: Vec<(u32, u32)> = self
			.received_req
			.iter()
			.filter(|(_, &sent)| !sent)
			.map(|(&(sender_idx, request_id), _)| (sender_idx, request_id))
			.collect();

		for (sender_idx, request_id) in received_req {
			if let Some(statement) = self.create_request_ack_statement(sender_idx, request_id) {
				self.statement_submit(statement).await?;
				self.received_req.insert((sender_idx, request_id), true);
			}
		}

		assert!(self.received_req.values().all(|ack| *ack), "Not every request ack sent");

		Ok(())
	}

	async fn receive_request_acknowledgments(&mut self) -> Result<(), anyhow::Error> {
		let pending: Vec<(u32, u32, sr25519::Public)> = self
			.sent_req
			.iter()
			.filter(|(_, &received)| !received)
			.map(|(&(receiver_idx, request_id), _)| {
				let receiver_session_key =
					*self.session_keys.get(&receiver_idx).expect("Receiver already exists");
				(receiver_idx, request_id, receiver_session_key)
			})
			.collect();

		let own_session_key = self.session_key.public();
		self.receive_statements_with_retry(
			pending,
			|&(_, _, receiver_session_key)| topics_ack(&receiver_session_key, &own_session_key),
			|participant, &(receiver_idx, request_id, _), statement| {
				let data = statement.data().expect("Must contain acknowledgment");
				let ack = StatementAcknowledge::decode(&mut &data[..])?;
				match ack {
					StatementAcknowledge::RequestReceived {
						sender_idx: ack_sender_idx,
						request_id: ack_request_id,
					} =>
						if ack_sender_idx == receiver_idx && ack_request_id == request_id {
							participant.sent_req.insert((receiver_idx, request_id), true);
							Ok(true)
						} else {
							Ok(false)
						},
					_ => Ok(false),
				}
			},
		)
		.await?;

		assert!(self.sent_req.values().all(|ack| *ack), "Not every request ack received");

		Ok(())
	}

	async fn receive_responses(&mut self, round: usize) -> Result<(), anyhow::Error> {
		let session_keys = self.session_keys.clone();
		let pending: Vec<(u32, sr25519::Public)> =
			session_keys.iter().map(|(&idx, &key)| (idx, key)).collect();

		let own_session_key = self.session_key.public();
		self.receive_statements_with_retry(
			pending,
			|&(_, sender_session_key)| topics_response(&sender_session_key, &own_session_key),
			|participant, &(sender_idx, _), statement| {
				let data = statement.data().expect("Must contain response");
				let res = StatementResponse::decode(&mut &data[..])?;
				if !participant.received_res.contains_key(&(sender_idx, res.request_id)) {
					participant.received_res.insert((sender_idx, res.request_id), false);
					Ok(true)
				} else {
					Ok(false)
				}
			},
		)
		.await?;

		assert_eq!(
			self.received_res.len(),
			self.group_members.len() * (round + 1),
			"Not every response received"
		);

		Ok(())
	}

	async fn send_response_acknowledgments(&mut self) -> Result<(), anyhow::Error> {
		let received_res: Vec<(u32, u32)> = self
			.received_res
			.iter()
			.filter(|(_, &sent)| !sent)
			.map(|(&(sender_idx, request_id), _)| (sender_idx, request_id))
			.collect();

		for (sender_idx, request_id) in received_res {
			if let Some(statement) = self.create_response_ack_statement(sender_idx, request_id) {
				self.statement_submit(statement).await?;
				self.received_res.insert((sender_idx, request_id), true);
			}
		}

		assert!(self.received_res.values().all(|ack| *ack), "Not every response ack sent");

		Ok(())
	}

	async fn receive_response_acknowledgments(&mut self) -> Result<(), anyhow::Error> {
		let pending: Vec<(u32, u32, sr25519::Public)> = self
			.sent_res
			.iter()
			.filter(|(_, &received)| !received)
			.map(|(&(receiver_idx, request_id), _)| {
				let receiver_session_key =
					*self.session_keys.get(&receiver_idx).expect("Receiver already exists");
				(receiver_idx, request_id, receiver_session_key)
			})
			.collect();

		let own_session_key = self.session_key.public();
		self.receive_statements_with_retry(
			pending,
			|&(_, _, receiver_session_key)| topics_ack(&receiver_session_key, &own_session_key),
			|participant, &(receiver_idx, request_id, _), statement| {
				let data = statement.data().expect("Must contain acknowledgment");
				let ack = StatementAcknowledge::decode(&mut &data[..])?;
				match ack {
					StatementAcknowledge::ResponseReceived {
						sender_idx: ack_sender_idx,
						request_id: ack_request_id,
					} =>
						if ack_sender_idx == receiver_idx && ack_request_id == request_id {
							participant.sent_res.insert((receiver_idx, request_id), true);
							Ok(true)
						} else {
							Ok(false)
						},
					_ => Ok(false),
				}
			},
		)
		.await?;

		assert!(self.sent_res.values().all(|ack| *ack), "Not every response ack received");

		Ok(())
	}

	async fn run(&mut self) -> Result<ParticipantStats, anyhow::Error> {
		let start_time = std::time::Instant::now();
		debug!(target: &self.log_target(), "[{}] Session keys exchange", self.idx);
		self.send_session_key().await?;
		self.receive_sleep().await;
		self.receive_session_keys().await?;

		debug!(target: &self.log_target(), "[{}] Symmetric keys exchange", self.idx);
		self.send_symmetric_keys().await?;
		self.receive_sleep().await;
		self.receive_symmetric_keys().await?;

		debug!(target: &self.log_target(), "[{}] Symmetric key acknowledgments exchange", self.idx);
		self.send_symmetric_key_acknowledgments().await?;
		self.receive_sleep().await;
		self.receive_symmetric_key_acknowledgments().await?;

		debug!(target: &self.log_target(), "[{}] Preparation finished", self.idx);

		for round in 0..MESSAGE_COUNT {
			debug!(target: &self.log_target(), "[{}] Requests exchange, round {}", self.idx, round + 1);
			self.send_requests(round).await?;
			self.receive_sleep().await;
			self.receive_requests(round).await?;

			debug!(target: &self.log_target(), "[{}] Request acknowledgments exchange, round {}", self.idx, round + 1);
			self.send_request_acknowledgments().await?;
			self.receive_sleep().await;
			self.receive_request_acknowledgments().await?;

			debug!(target: &self.log_target(), "[{}] Responses exchange, round {}", self.idx, round + 1);
			self.send_responses(round).await?;
			self.receive_sleep().await;
			self.receive_responses(round).await?;

			debug!(target: &self.log_target(), "[{}] Response acknowledgments exchange, round {}", self.idx, round + 1);
			self.send_response_acknowledgments().await?;
			self.receive_sleep().await;
			self.receive_response_acknowledgments().await?;
		}

		let elapsed = start_time.elapsed();

		Ok(ParticipantStats {
			total_time: elapsed,
			sent_count: self.sent_count,
			received_count: self.received_count,
			retry_count: self.retry_count,
		})
	}
}

fn topic_public_key() -> Topic {
	let mut topic = [0u8; 32];
	let source = b"public key";
	let len = source.len().min(32);
	topic[..len].copy_from_slice(&source[..len]);
	topic
}

fn topic_ack() -> Topic {
	blake2_256(b"ack")
}

fn topic_idx(idx: u32) -> Topic {
	let mut topic = [0u8; 32];
	topic[..4].copy_from_slice(&idx.to_le_bytes());
	topic
}

fn topic_pair(sender: &sr25519::Public, receiver: &sr25519::Public) -> Topic {
	let mut data = Vec::new();
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	blake2_256(&data)
}

fn topics_session_key(idx: u32) -> Vec<Topic> {
	vec![topic_public_key(), topic_idx(idx)]
}

fn topics_symmetric_key(
	sender_key: &sr25519::Public,
	receiver_key: &sr25519::Public,
) -> Vec<Topic> {
	vec![topic_pair(sender_key, receiver_key)]
}

fn topics_request(sender_key: &sr25519::Public, receiver_key: &sr25519::Public) -> Vec<Topic> {
	vec![blake2_256(b"request"), topic_pair(sender_key, receiver_key)]
}

fn topics_response(sender_key: &sr25519::Public, receiver_key: &sr25519::Public) -> Vec<Topic> {
	vec![blake2_256(b"response"), topic_pair(sender_key, receiver_key)]
}

fn topics_ack(sender_key: &sr25519::Public, receiver_key: &sr25519::Public) -> Vec<Topic> {
	vec![topic_ack(), topic_pair(sender_key, receiver_key)]
}

fn channel_pair(sender: &sr25519::Public, receiver: &sr25519::Public) -> Topic {
	let mut data = Vec::new();
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	blake2_256(&data)
}

fn channel_request(sender: &sr25519::Public, receiver: &sr25519::Public) -> Topic {
	let mut data = Vec::new();
	data.extend_from_slice(b"request");
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	blake2_256(&data)
}

fn channel_response(sender: &sr25519::Public, receiver: &sr25519::Public) -> Topic {
	let mut data = Vec::new();
	data.extend_from_slice(b"response");
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	blake2_256(&data)
}
