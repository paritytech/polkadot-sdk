// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Benchmarking statement store performance

use anyhow::anyhow;
use codec::{Decode, Encode};
use log::{debug, info, trace};
use sc_statement_store::{DEFAULT_MAX_TOTAL_SIZE, DEFAULT_MAX_TOTAL_STATEMENTS};
use sp_core::{blake2_256, sr25519, Bytes, Pair};
use sp_statement_store::{Channel, Statement, Topic};
use std::{cell::Cell, collections::HashMap, time::Duration};
use tokio::time::timeout;
use zombienet_sdk::{
	subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params},
	LocalFileSystem, Network, NetworkConfigBuilder,
};

const GROUP_SIZE: u32 = 6;
const PARTICIPANT_SIZE: u32 = GROUP_SIZE * 8333; // Target ~50,000 total
const MESSAGE_SIZE: usize = 512;
const MESSAGE_COUNT: usize = 1;
const MAX_RETRIES: u32 = 100;
const RETRY_DELAY_MS: u64 = 500;
const PROPAGATION_DELAY_MS: u64 = 2000;
const TIMEOUT_MS: u64 = 3000;

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_one_node_bench() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let collator_names = ["alice", "bob"];
	let network = spawn_network(&collator_names).await?;

	info!("Starting statement store benchmark with {} participants", PARTICIPANT_SIZE);

	let target_node = collator_names[0];
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

	let collator_names = ["alice", "bob", "charlie", "dave", "eve", "ferdie"];
	let network = spawn_network(&collator_names).await?;

	info!("Starting statement store benchmark with {} participants", PARTICIPANT_SIZE);

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

	let collator_names = ["alice", "bob", "charlie", "dave", "eve", "ferdie"];
	let network = spawn_network(&collator_names).await?;

	let target_node = collator_names[0];
	let node = network.get_node(target_node)?;
	let rpc_client = node.rpc().await?;
	info!("Created single RPC client for target node: {}", target_node);

	let total_tasks = 64 * 1024;
	let payload_size = 1024;
	let submit_capacity =
		DEFAULT_MAX_TOTAL_STATEMENTS.min(DEFAULT_MAX_TOTAL_SIZE / payload_size) as u64;
	let statements_per_task = submit_capacity / total_tasks as u64;
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
					blake2_256(format!("{idx}{statement_count}{public:?}").as_bytes())
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

async fn spawn_network(collators: &[&str]) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	assert!(collators.len() >= 2);
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
			let p = p
				.with_id(2400)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain_spec_path("tests/zombie_ci/people-rococo-spec.json")
				.with_default_args(vec![
					"--force-authoring".into(),
					"-lstatement-store=info,statement-gossip=info,error".into(),
					"--enable-statement-store".into(),
					"--rpc-max-connections=50000".into(),
				])
				// Have to set outside of the loop below, so that `p` has the right type.
				.with_collator(|n| n.with_name(collators[0]));

			collators[1..]
				.iter()
				.fold(p, |acc, &name| acc.with_collator(|n| n.with_name(name)))
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
struct StatementMessage {
	message_id: u32,
	data: Vec<u8>,
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
	participants: u32,
	sent: u32,
	received: u32,
	min_time: Duration,
	max_time: Duration,
	avg_time: Duration,
	min_retries: u32,
	max_retries: u32,
	avg_retries: u32,
}

impl ParticipantStats {
	fn aggregate(all_stats: Vec<ParticipantStats>) -> AggregatedStats {
		let participants = all_stats.len() as u32;
		let sent = all_stats.iter().map(|s| s.sent_count).sum::<u32>() / participants;
		let received = all_stats.iter().map(|s| s.received_count).sum::<u32>() / participants;

		let min_time = all_stats.iter().map(|s| s.total_time).min().unwrap_or(Duration::ZERO);
		let max_time = all_stats.iter().map(|s| s.total_time).max().unwrap_or(Duration::ZERO);
		let avg_time = Duration::from_secs_f64(
			all_stats.iter().map(|s| s.total_time.as_secs_f64()).sum::<f64>() / participants as f64,
		);

		let min_retries = all_stats.iter().map(|s| s.retry_count).min().unwrap_or(0);
		let max_retries = all_stats.iter().map(|s| s.retry_count).max().unwrap_or(0);
		let avg_retries = all_stats.iter().map(|s| s.retry_count).sum::<u32>() / participants;

		AggregatedStats {
			participants,
			sent,
			received,
			min_time,
			max_time,
			avg_time,
			min_retries,
			max_retries,
			avg_retries,
		}
	}
}

impl AggregatedStats {
	fn log_summary(&self) {
		info!("Statement store benchmark completed with {} participants", self.participants);
		info!(
			"Participants: {}, each sent: {}, received: {}",
			self.participants, self.sent, self.received
		);
		info!("Summary        min       avg       max");
		info!(
			" {:<8} {:>8}  {:>8}  {:>8}",
			"time, s",
			self.min_time.as_secs(),
			self.avg_time.as_secs(),
			self.max_time.as_secs(),
		);
		info!(
			" {:<8} {:>8}  {:>8}  {:>8}",
			"retries", self.min_retries, self.avg_retries, self.max_retries
		);
	}
}

struct Participant {
	idx: u32,
	keyring: sr25519::Pair,
	session_key: sr25519::Pair,
	group_members: Vec<u32>,
	session_keys: HashMap<u32, sr25519::Public>,
	sent_messages: HashMap<(u32, u32), bool>,
	received_messages: HashMap<(u32, u32), bool>,
	sent_count: u32,
	received_count: u32,
	pending_messages: HashMap<u32, Option<u32>>,
	retry_count: u32,
	rpc_client: RpcClient,
}

impl Participant {
	fn log_target(&self) -> String {
		format!("participant_{}", self.idx)
	}

	fn new(idx: u32, rpc_client: RpcClient) -> Self {
		debug!(target: &format!("participant_{idx}"), "Initializing participant {}", idx);
		let (keyring, _) = sr25519::Pair::generate();
		let (session_key, _) = sr25519::Pair::generate();

		let group_start = (idx / GROUP_SIZE) * GROUP_SIZE;
		let group_end = group_start + GROUP_SIZE;
		let group_members: Vec<u32> = (group_start..group_end).filter(|&i| i != idx).collect();

		Self {
			keyring,
			session_key,
			idx,
			group_members,
			session_keys: HashMap::new(),
			sent_messages: HashMap::new(),
			received_messages: HashMap::new(),
			pending_messages: HashMap::new(),
			sent_count: 0,
			received_count: 0,
			retry_count: 0,
			rpc_client,
		}
	}

	async fn wait_for_retry(&mut self) -> Result<(), anyhow::Error> {
		if self.retry_count >= MAX_RETRIES {
			return Err(anyhow!("No more retry attempts for participant {}", self.idx))
		}

		self.retry_count += 1;
		if self.retry_count % 10 == 0 {
			debug!(target: &self.log_target(), "Retry attempt {}", self.retry_count);
		}
		tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_DELAY_MS)).await;

		Ok(())
	}

	async fn wait_for_propagation(&mut self) {
		trace!(target: &self.log_target(), "Waiting {}ms for propagation", PROPAGATION_DELAY_MS);
		tokio::time::sleep(tokio::time::Duration::from_millis(PROPAGATION_DELAY_MS)).await;
	}

	async fn statement_submit(&mut self, statement: Statement) -> Result<(), anyhow::Error> {
		let statement_bytes: Bytes = statement.encode().into();
		let _: () = self
			.rpc_client
			.request("statement_submit", rpc_params![statement_bytes])
			.await?;

		self.sent_count += 1;
		trace!(target: &self.log_target(), "Submitted statement (counter: {})", self.sent_count);

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
		trace!(target: &self.log_target(), "Received {} statements (counter: {})", decoded_statements.len(), self.received_count);

		Ok(decoded_statements)
	}

	fn create_session_key_statement(&self) -> Statement {
		let mut statement = Statement::new();
		statement.set_channel(channel_public_key());
		statement.set_priority(self.sent_count);
		statement.set_topic(0, topic_public_key());
		statement.set_topic(1, topic_idx(self.idx));
		statement.set_plain_data(self.session_key.public().to_vec());
		statement.sign_sr25519_private(&self.keyring);

		statement
	}

	fn create_message_statement(&mut self, receiver_idx: u32) -> Option<(Statement, u32)> {
		let receiver_session_key = self.session_keys.get(&receiver_idx)?;

		let message_id = self.sent_count;
		let mut data = vec![0u8; MESSAGE_SIZE];
		for (i, byte) in data.iter_mut().enumerate() {
			*byte = (i % 256) as u8; // Simple pattern for testing
		}

		let request = StatementMessage { message_id, data };
		let request_data = request.encode();
		let mut statement = Statement::new();

		let topic0 = topic_message();
		let topic1 = topic_pair(&self.session_key.public(), receiver_session_key);
		let channel = channel_message(&self.session_key.public(), receiver_session_key);

		statement.set_topic(0, topic0);
		statement.set_topic(1, topic1);
		statement.set_channel(channel);
		statement.set_priority(self.sent_count);
		statement.set_plain_data(request_data);
		statement.sign_sr25519_private(&self.keyring);

		Some((statement, message_id))
	}

	async fn send_session_key(&mut self) -> Result<(), anyhow::Error> {
		let statement = self.create_session_key_statement();
		self.statement_submit(statement).await
	}

	async fn receive_session_keys(&mut self) -> Result<(), anyhow::Error> {
		let mut pending = self.group_members.clone();

		trace!(target: &self.log_target(), "Pending session keys to receive: {:?}", pending.len());
		loop {
			let mut completed_this_round = Vec::new();
			for &idx in &pending {
				match timeout(
					Duration::from_millis(TIMEOUT_MS),
					self.statement_broadcasts_statement(vec![topic_public_key(), topic_idx(idx)]),
				)
				.await
				{
					Ok(Ok(statements)) if !statements.is_empty() => {
						if let Some(statement) = statements.first() {
							let data = statement.data().expect("Must contain session_key");
							let session_key = sr25519::Public::from_raw(data[..].try_into()?);
							self.session_keys.insert(idx, session_key);
							completed_this_round.push(idx);
						}
					},
					res => {
						debug!(target: &self.log_target(), "No statements received for idx {:?}: {:?}", idx, res);
					},
				}
			}

			pending.retain(|x| !completed_this_round.contains(x));
			if pending.is_empty() {
				break;
			}
			trace!(target: &self.log_target(), "Session keys left to receive: {:?}, waiting {}ms for retry", pending.len(), RETRY_DELAY_MS);
			self.wait_for_retry().await?;
		}

		assert_eq!(
			self.session_keys.len(),
			self.group_members.len(),
			"Not every session key received"
		);

		Ok(())
	}

	async fn send_messages(&mut self, round: usize) -> Result<(), anyhow::Error> {
		let group_members = self.group_members.clone();
		for receiver_idx in group_members {
			let (statement, request_id) =
				self.create_message_statement(receiver_idx).expect("Receiver must present");
			self.statement_submit(statement).await?;
			self.sent_messages.insert((receiver_idx, request_id), false);
		}

		assert_eq!(
			self.sent_messages.len(),
			self.group_members.len() * (round + 1),
			"Not every request sent"
		);

		Ok(())
	}

	async fn receive_messages(&mut self, round: usize) -> Result<(), anyhow::Error> {
		let mut pending: Vec<(u32, sr25519::Public)> =
			self.session_keys.iter().map(|(&idx, &key)| (idx, key)).collect();
		let own_session_key = self.session_key.public();

		trace!(target: &self.log_target(), "Pending messages to receive: {:?}", pending.len());
		loop {
			let mut completed_this_round = Vec::new();
			for &(sender_idx, sender_session_key) in &pending {
				match timeout(
					Duration::from_millis(TIMEOUT_MS),
					self.statement_broadcasts_statement(vec![
						topic_message(),
						topic_pair(&sender_session_key, &own_session_key),
					]),
				)
				.await
				{
					Ok(Ok(statements)) if !statements.is_empty() => {
						if let Some(statement) = statements.first() {
							let data = statement.data().expect("Must contain request");
							let req = StatementMessage::decode(&mut &data[..])?;

							if let std::collections::hash_map::Entry::Vacant(e) = self.received_messages.entry((sender_idx, req.message_id)) {
								e.insert(false);
								self.pending_messages.insert(sender_idx, Some(req.message_id));
								completed_this_round.push((sender_idx, sender_session_key));
							}
						}
					},
					res => {
						debug!(target: &self.log_target(), "No statements received for sender {:?}: {:?}", sender_idx, res);
					},
				}
			}

			pending.retain(|x| !completed_this_round.contains(x));
			if pending.is_empty() {
				break;
			}
			trace!(target: &self.log_target(), "Messages left to receive: {:?}, waiting {}ms for retry", pending.len(), RETRY_DELAY_MS);
			self.wait_for_retry().await?;
		}

		assert_eq!(
			self.received_messages.len(),
			self.group_members.len() * (round + 1),
			"Not every request received"
		);
		assert_eq!(
			self.pending_messages.values().filter(|i| i.is_some()).count(),
			self.group_members.len(),
			"Not every request received"
		);

		Ok(())
	}

	async fn run(&mut self) -> Result<ParticipantStats, anyhow::Error> {
		let start_time = std::time::Instant::now();

		debug!(target: &self.log_target(), "Session keys exchange");
		self.send_session_key().await?;
		trace!(target: &self.log_target(), "Session keys sent");
		self.wait_for_propagation().await;
		trace!(target: &self.log_target(), "Session keys requests started");
		self.receive_session_keys().await?;
		trace!(target: &self.log_target(), "Session keys received");

		for round in 0..MESSAGE_COUNT {
			debug!(target: &self.log_target(), "Messages exchange, round {}", round + 1);
			self.send_messages(round).await?;
			trace!(target: &self.log_target(), "Messages sent");
			self.wait_for_propagation().await;
			trace!(target: &self.log_target(), "Messages requests started");
			self.receive_messages(round).await?;
			trace!(target: &self.log_target(), "Messages received");
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
	blake2_256(b"public key")
}

fn topic_idx(idx: u32) -> Topic {
	blake2_256(&idx.to_le_bytes())
}

fn topic_pair(sender: &sr25519::Public, receiver: &sr25519::Public) -> Topic {
	let mut data = Vec::new();
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	blake2_256(&data)
}

fn topic_message() -> Topic {
	blake2_256(b"message")
}

fn channel_public_key() -> Channel {
	[0u8; 32]
}

fn channel_message(sender: &sr25519::Public, receiver: &sr25519::Public) -> Channel {
	let mut data = Vec::new();
	data.extend_from_slice(b"message");
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	blake2_256(&data)
}
