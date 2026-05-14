// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Benchmarking statement store performance through the unstable RPC API

use anyhow::anyhow;
use log::{debug, info};
use sc_statement_store::test_utils::get_keypair;
use sp_core::blake2_256;
use sp_statement_store::{Statement, Topic, TopicFilter};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Barrier, time::timeout};
use zombienet_sdk::subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::client::RpcSubscription};

use super::common::{
	add_filter_unstable, spawn_network_with_injected_allowances, submit_statement_unstable,
	subscribe_unstable, unstable_subscription_id, UnstableAddFilterResponse,
	UnstableStatementEvent, RPC_POOL_SIZE,
};

struct LatencyBenchConfig {
	num_rounds: usize,
	num_nodes: usize,
	num_clients: u32,
	max_retries: u32,
	interval_ms: u64,
	req_timeout_ms: u64,
	messages_pattern: &'static [(usize, usize)],
}

impl LatencyBenchConfig {
	fn messages_per_client(&self) -> usize {
		self.messages_pattern.iter().map(|(count, _)| count).sum()
	}
}

#[derive(Debug, Clone)]
struct RoundStats {
	send_duration: Duration,
	receive_duration: Duration,
	full_latency: Duration,
	sent_count: u32,
	received_count: u32,
	receive_attempts: u32,
}

async fn add_match_all_filter_unstable(
	rpc: &RpcClient,
	subscription_id: &str,
	topic: Topic,
) -> Result<String, anyhow::Error> {
	match add_filter_unstable(
		rpc,
		subscription_id,
		TopicFilter::MatchAll(vec![topic].try_into().expect("Single topic")),
	)
	.await?
	{
		UnstableAddFilterResponse::Ok(filter_id) => Ok(filter_id),
		UnstableAddFilterResponse::LimitReached(result) => {
			Err(anyhow!("Unexpected unstable filter limit response: {result:?}"))
		},
	}
}

async fn collect_empty_unstable_replay(
	subscription: &mut RpcSubscription<UnstableStatementEvent>,
	filter_id: &str,
) -> Result<(), anyhow::Error> {
	loop {
		let event = timeout(Duration::from_secs(20), subscription.next())
			.await
			.map_err(|_| anyhow!("Timeout waiting for unstable replayDone"))?
			.ok_or_else(|| anyhow!("Unstable statement subscription ended during replay"))?
			.map_err(|e| anyhow!("Unstable statement subscription error: {e}"))?;

		match event {
			UnstableStatementEvent::ReplayStatements { filter_id: id, statements }
				if id == filter_id =>
			{
				if !statements.is_empty() {
					return Err(anyhow!(
						"Expected empty unstable replay for filter {filter_id}, got {} statement(s)",
						statements.len()
					));
				}
			},
			UnstableStatementEvent::ReplayDone { filter_id: id } if id == filter_id => {
				return Ok(())
			},
			event => {
				return Err(anyhow!(
					"Unexpected unstable event while draining replay for filter {filter_id}: {event:?}"
				));
			},
		}
	}
}

async fn next_unstable_statements(
	subscription: &mut RpcSubscription<UnstableStatementEvent>,
	expected_count: u32,
	total_timeout: Duration,
) -> Result<u32, anyhow::Error> {
	let deadline = tokio::time::Instant::now() + total_timeout;
	let mut received_count = 0;

	while received_count < expected_count {
		let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
		if remaining.is_zero() {
			return Err(anyhow!(
				"Timeout waiting for unstable statements: received {received_count}/{expected_count}"
			));
		}

		let event = timeout(remaining, subscription.next())
			.await
			.map_err(|_| {
				anyhow!(
					"Timeout waiting for unstable statements: received {received_count}/{expected_count}"
				)
			})?
			.ok_or_else(|| anyhow!("Unstable statement subscription ended unexpectedly"))?
			.map_err(|e| anyhow!("Unstable statement subscription error: {e}"))?;

		match event {
			UnstableStatementEvent::NewStatements { statements } => {
				for entry in &statements {
					if entry.filter_ids.is_empty() {
						return Err(anyhow!("Unstable newStatements entry has no filter ids"));
					}
				}
				received_count += statements.len() as u32;
			},
			UnstableStatementEvent::Stop => {
				return Err(anyhow!(
					"Unstable statement subscription stopped after {received_count}/{expected_count}"
				));
			},
			event => return Err(anyhow!("Unexpected unstable event while receiving: {event:?}")),
		}
	}

	Ok(received_count)
}

#[tokio::test(flavor = "multi_thread")]
async fn statement_store_unstable_latency_bench() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = Arc::new(LatencyBenchConfig {
		num_nodes: 5,
		num_clients: 50000,
		interval_ms: 10000,
		num_rounds: 1,
		messages_pattern: &[(5, 1024 / 2)],
		max_retries: 500,
		req_timeout_ms: 3000,
	});

	let collator_names: Vec<String> =
		(0..config.num_nodes).map(|i| format!("collator{i}")).collect();
	let collator_names: Vec<&str> = collator_names.iter().map(|s| s.as_str()).collect();

	let network =
		spawn_network_with_injected_allowances(&collator_names, config.num_clients).await?;

	info!("Starting unstable Latency benchmark");
	info!("");
	info!("Clients: {}", config.num_clients);
	info!("Nodes: {}", config.num_nodes);
	info!("Rounds: {}", config.num_rounds);
	info!("Interval, ms: {}", config.interval_ms);
	info!("Messages, per round: {}", config.messages_per_client() as u32 * config.num_clients);
	info!("Message pattern:");
	for &(count, size) in config.messages_pattern {
		info!(" - {} messages {} bytes", count, size);
	}
	info!("");

	let clients_per_node = config.num_clients as usize / config.num_nodes;
	let pool_size_per_node = RPC_POOL_SIZE.min(clients_per_node);
	let mut rpc_pools: Vec<Vec<RpcClient>> = Vec::new();
	for &name in &collator_names {
		let node = network.get_node(name)?;
		let mut pool = Vec::with_capacity(pool_size_per_node);
		for _ in 0..pool_size_per_node {
			pool.push(node.rpc().await?);
		}
		rpc_pools.push(pool);
	}
	info!(
		"Created unstable RPC connection pool: {} connections x {} nodes = {} total",
		pool_size_per_node,
		collator_names.len(),
		pool_size_per_node * collator_names.len()
	);

	let barrier = Arc::new(Barrier::new(config.num_clients as usize));
	let sync_start = std::time::Instant::now();

	let test_run_id = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_micros() as u64;

	let handles: Vec<_> = (0..config.num_clients)
		.map(|client_id| {
			let config = Arc::clone(&config);
			let barrier = Arc::clone(&barrier);
			let keyring = get_keypair(client_id);
			let node_idx = (client_id as usize) % config.num_nodes;
			let conn_idx = (client_id as usize / config.num_nodes) % pool_size_per_node;
			let rpc_client = rpc_pools[node_idx][conn_idx].clone();
			let neighbour_id = (client_id + 1) % config.num_clients;
			let neighbour_node_idx = (neighbour_id as usize) % config.num_nodes;
			if node_idx == neighbour_node_idx && config.num_nodes > 1 {
				panic!(
					"Client {client_id} and neighbour {neighbour_id} are on the same node {node_idx}!"
				);
			}

			tokio::spawn(async move {
				barrier.wait().await;

				if client_id == 0 {
					let sync_time = sync_start.elapsed();
					debug!(
						"All {} unstable tasks synchronized and starting work in {:.3}s",
						config.num_clients,
						sync_time.as_secs_f64()
					);
				}

				let submission_jitter = (client_id % 1000) as u64;
				tokio::time::sleep(Duration::from_millis(submission_jitter)).await;

				let mut rounds_stats = Vec::new();
				for round in 0..config.num_rounds {
					let round_start = std::time::Instant::now();

					if client_id == 0 {
						info!("Creating unstable subscription and filters for expected messages");
					}

					let mut subscription = subscribe_unstable(&rpc_client).await?;
					let subscription_id = unstable_subscription_id(&subscription)?;
					let mut filter_ids = Vec::new();
					for msg_idx in 0..config.messages_per_client() as u32 {
						let topic_str = format!("{test_run_id}-{client_id}-{round}-{msg_idx}");

						if client_id == 0 {
							info!("Adding unstable filter for {msg_idx} message(s) {topic_str:?}");
						}

						let topic: Topic = blake2_256(topic_str.as_bytes()).into();
						let filter_id =
							add_match_all_filter_unstable(&rpc_client, &subscription_id, topic)
								.await?;
						collect_empty_unstable_replay(&mut subscription, &filter_id).await?;
						filter_ids.push((msg_idx, topic_str, filter_id));
					}

					if client_id == 0 {
						info!("Created {} unstable filters", filter_ids.len());
					}

					let mut msg_idx: u32 = 0;

					if client_id == 0 {
						info!("Start sending messages via unstable submit");
					}

					for &(count, size) in config.messages_pattern {
						for _ in 0..count {
							let mut statement = Statement::new();

							let topic_str = format!("{test_run_id}-{client_id}-{round}-{msg_idx}");
							let topic = blake2_256(topic_str.as_bytes());
							let channel = blake2_256(msg_idx.to_le_bytes().as_ref());

							let timestamp_ms = std::time::SystemTime::now()
								.duration_since(std::time::UNIX_EPOCH)
								.unwrap()
								.as_millis() as u32;

							statement.set_channel(channel);
							statement.set_expiry_from_parts(u32::MAX, timestamp_ms);
							statement.set_topic(0, topic.into());
							statement.set_plain_data(vec![0u8; size]);
							statement.sign_sr25519_private(&keyring);

							let result = submit_statement_unstable(&rpc_client, &statement).await?;

							msg_idx += 1;
							if client_id == 0 {
								info!(
									"Sent {msg_idx} unstable message(s) {topic_str:?}, {result:?}"
								);
							}
						}
					}

					let sent_count = msg_idx;
					let send_duration = round_start.elapsed();

					let receive_start = std::time::Instant::now();
					let expected_count = config.messages_per_client() as u32;
					let total_timeout =
						Duration::from_millis(config.req_timeout_ms * config.max_retries as u64);
					let received_count =
						next_unstable_statements(&mut subscription, expected_count, total_timeout)
							.await?;

					let receive_duration = receive_start.elapsed();
					let full_latency = round_start.elapsed();
					if full_latency < Duration::from_millis(config.interval_ms) {
						tokio::time::sleep(
							Duration::from_millis(config.interval_ms) - full_latency,
						)
						.await;
					}

					rounds_stats.push(RoundStats {
						send_duration,
						receive_duration,
						full_latency,
						sent_count,
						received_count,
						receive_attempts: filter_ids.len() as u32,
					});
				}

				let expected_count = config.messages_per_client() as u32;
				for stats in &rounds_stats {
					if stats.sent_count != expected_count {
						return Err(anyhow!(
							"Client {}: Expected {} unstable messages sent, but got {}",
							client_id,
							expected_count,
							stats.sent_count
						));
					}
					if stats.received_count != expected_count {
						return Err(anyhow!(
							"Client {}: Expected {} unstable messages received, but got {}",
							client_id,
							expected_count,
							stats.received_count
						));
					}
				}

				Ok::<_, anyhow::Error>(rounds_stats)
			})
		})
		.collect();

	let mut all_round_stats = Vec::new();
	for handle in handles {
		let stats = handle.await??;
		all_round_stats.extend(stats);
	}

	let calc_stats = |values: Vec<f64>| -> (f64, f64, f64) {
		let min = values.iter().copied().fold(f64::INFINITY, f64::min);
		let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
		let avg = values.iter().sum::<f64>() / values.len() as f64;
		(min, avg, max)
	};

	let send_s =
		calc_stats(all_round_stats.iter().map(|s| s.send_duration.as_secs_f64()).collect());
	let read_s =
		calc_stats(all_round_stats.iter().map(|s| s.receive_duration.as_secs_f64()).collect());
	let latency_s =
		calc_stats(all_round_stats.iter().map(|s| s.full_latency.as_secs_f64()).collect());
	let attempts = calc_stats(all_round_stats.iter().map(|s| s.receive_attempts as f64).collect());
	let attempts_per_msg = (
		attempts.0 / config.messages_per_client() as f64,
		attempts.1 / config.messages_per_client() as f64,
		attempts.2 / config.messages_per_client() as f64,
	);

	info!("");
	info!("                      Min       Avg       Max");
	info!("Send, s             {:>8.3}  {:>8.3}  {:>8.3}", send_s.0, send_s.1, send_s.2);
	info!("Receive, s          {:>8.3}  {:>8.3}  {:>8.3}", read_s.0, read_s.1, read_s.2);
	info!("Latency, s          {:>8.3}  {:>8.3}  {:>8.3}", latency_s.0, latency_s.1, latency_s.2);
	info!(
		"Attempts, per msg   {:>8.1}  {:>8.1}  {:>8.1}",
		attempts_per_msg.0, attempts_per_msg.1, attempts_per_msg.2
	);

	Ok(())
}
