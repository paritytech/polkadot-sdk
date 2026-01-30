// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! CLI tool for distributed statement-store latency benchmarking.
//!
//! This tool is designed to run as a Kubernetes Job, with multiple instances
//! running concurrently to simulate realistic load on statement-store nodes.
//!
//! # Usage
//!
//! ```bash
//! statement-latency-bench \
//!   --rpc-endpoints ws://node1:9944,ws://node2:9944,ws://node3:9944 \
//!   --num-clients 1000 \
//!   --messages-pattern "5:512"
//! ```

use anyhow::{anyhow, Context};
use clap::Parser;
use codec::Encode;
use jsonrpsee::{
	core::client::{ClientT, Subscription, SubscriptionClientT},
	rpc_params,
	ws_client::{WsClient, WsClientBuilder},
};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use sp_core::{blake2_256, bounded_vec::BoundedVec, sr25519, Bytes, ConstU32, Pair};
use sp_statement_store::{Statement, SubmitResult, TopicFilter};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Barrier, time::timeout};

#[derive(Parser, Debug)]
#[command(name = "statement-latency-bench")]
#[command(about = "Distributed statement store latency benchmark", long_about = None)]
struct Args {
	/// Comma-separated list of RPC WebSocket endpoints (e.g., ws://node1:9944,ws://node2:9944)
	#[arg(long, value_delimiter = ',', required = true)]
	rpc_endpoints: Vec<String>,

	/// Number of clients to spawn in this Job instance
	#[arg(long, default_value = "100")]
	num_clients: u32,

	/// Message pattern: comma-separated "count:size" pairs (e.g., "5:512" or "5:512,3:1024")
	/// This specifies how many messages of each size to send
	#[arg(long, default_value = "5:512")]
	messages_pattern: String,

	/// Timeout for receiving messages in a batch (milliseconds)
	#[arg(long, default_value = "5000")]
	receive_timeout_ms: u64,

	/// Number of benchmark rounds
	#[arg(long, default_value = "1")]
	num_rounds: usize,

	/// Interval between rounds in milliseconds
	#[arg(long, default_value = "10000")]
	interval_ms: u64,

	/// Skip time synchronization (for local testing)
	#[arg(long, default_value = "false")]
	skip_sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoundStats {
	round: usize,
	send_duration_secs: f64,
	receive_duration_secs: f64,
	full_latency_secs: f64,
	sent_count: u32,
	received_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stats {
	min: f64,
	avg: f64,
	max: f64,
}

fn parse_messages_pattern(pattern: &str) -> Result<Vec<(usize, usize)>, anyhow::Error> {
	pattern
		.split(',')
		.map(|part| {
			let part = part.trim();
			let (count_str, size_str) = part
				.split_once(':')
				.ok_or_else(|| anyhow!("Invalid pattern '{part}'. Expected 'count:size'"))?;

			let count = count_str
				.parse::<usize>()
				.with_context(|| format!("Invalid count '{count_str}' in pattern '{part}'"))?;
			let size = size_str
				.parse::<usize>()
				.with_context(|| format!("Invalid size '{size_str}' in pattern '{part}'"))?;

			Ok((count, size))
		})
		.collect()
}

fn messages_per_client(pattern: &[(usize, usize)]) -> usize {
	pattern.iter().map(|(count, _)| count).sum()
}

fn calc_stats(values: impl Iterator<Item = f64>) -> Stats {
	let values: Vec<_> = values.collect();
	let min = values.iter().copied().fold(f64::INFINITY, f64::min);
	let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
	let avg = values.iter().sum::<f64>() / values.len() as f64;
	Stats { min, avg, max }
}

fn is_leader(client_id: u32) -> bool {
	client_id == 0
}

fn generate_topic(test_run_id: u64, client_id: u32, round: usize, msg_idx: u32) -> [u8; 32] {
	let topic_str = format!("{test_run_id}-{client_id}-{round}-{msg_idx}");
	blake2_256(topic_str.as_bytes())
}

struct ClientConfig {
	client_id: u32,
	neighbour_id: u32,
	num_clients: u32,
	num_rounds: usize,
	test_run_id: u64,
	messages_pattern: Vec<(usize, usize)>,
	receive_timeout_ms: u64,
	interval_ms: u64,
}

async fn run_client(
	config: ClientConfig,
	rpc_client: Arc<WsClient>,
	barrier: Arc<Barrier>,
	sync_start: std::time::Instant,
) -> Result<Vec<RoundStats>, anyhow::Error> {
	let ClientConfig {
		client_id,
		neighbour_id,
		num_clients,
		num_rounds,
		test_run_id,
		messages_pattern,
		receive_timeout_ms,
		interval_ms,
	} = config;

	let (keyring, _) = sr25519::Pair::generate();
	let expected_count = messages_per_client(&messages_pattern) as u32;

	barrier.wait().await;

	if is_leader(client_id) {
		debug!(
			"All {} tasks synchronized and starting in {:.3}s",
			num_clients,
			sync_start.elapsed().as_secs_f64()
		);
	}

	// Apply jitter to distribute connection load (using prime multiplier for better distribution)
	let submission_jitter = ((client_id * 7) % 1000) as u64;
	tokio::time::sleep(Duration::from_millis(submission_jitter)).await;

	let mut all_round_stats = Vec::with_capacity(num_rounds);

	// Use human 1-based round numbering for logging
	for round in 1..(num_rounds + 1) {
		let round_start = std::time::Instant::now();
		let mut sent_count: u32 = 0;

		let expected_topics: Vec<Bytes> = (0..expected_count)
			.map(|idx| generate_topic(test_run_id, neighbour_id, round, idx).to_vec().into())
			.collect();

		let bounded_topics: BoundedVec<Bytes, ConstU32<128>> = expected_topics
			.try_into()
			.map_err(|_| anyhow!("Client {client_id}: Too many topics (max 128)"))?;

		let mut subscription: Subscription<Bytes> = rpc_client
			.subscribe(
				"statement_subscribeStatement",
				rpc_params![TopicFilter::MatchAny(bounded_topics)],
				"statement_unsubscribeStatement",
			)
			.await
			.with_context(|| format!("Client {client_id}: Failed to subscribe"))?;

		for &(count, size) in &messages_pattern {
			for _ in 0..count {
				let topic = generate_topic(test_run_id, client_id, round, sent_count);
				let channel = blake2_256(sent_count.to_le_bytes().as_ref());

				let mut statement = Statement::new();
				statement.set_channel(channel);
				statement.set_expiry_from_parts(u32::MAX, (sent_count + 1) * (round as u32));
				statement.set_topic(0, topic);
				statement.set_plain_data(vec![0u8; size]);
				statement.sign_sr25519_private(&keyring);

				let encoded: Bytes = statement.encode().into();
				let result: SubmitResult = rpc_client
					.request("statement_submit", rpc_params![encoded])
					.await
					.with_context(|| format!("Client {client_id}: Failed to submit statement"))?;

				sent_count += 1;
				if is_leader(client_id) {
					debug!(
						"Round {}/{}. Sent {} statement(s): {:?}",
						round, num_rounds, sent_count, result
					);
				}
			}
		}

		let send_duration = round_start.elapsed();
		let mut received_count: u32 = 0;
		while received_count < expected_count {
			let result =
				timeout(Duration::from_millis(receive_timeout_ms), subscription.next()).await;

			match result {
				Ok(Some(Ok(_))) => {
					received_count += 1;
					if is_leader(client_id) {
						debug!(
							"Round {}/{}. Received {} statement(s)",
							round, num_rounds, received_count
						);
					}
				},
				other => {
					return Err(anyhow!(
						"Client {client_id}: Round {}: Error receiving ({other:?}), got {received_count}/{expected_count}",
						round
					));
				},
			}
		}
		drop(subscription);

		let full_latency = round_start.elapsed();
		let receive_duration = full_latency - send_duration;

		if is_leader(client_id) {
			debug!(
				"Round {}/{} complete. Send: {:.3}s, Receive: {:.3}s, Total: {:.3}s",
				round,
				num_rounds,
				send_duration.as_secs_f64(),
				receive_duration.as_secs_f64(),
				full_latency.as_secs_f64()
			);
		}

		let stats = RoundStats {
			round,
			sent_count,
			received_count,
			send_duration_secs: send_duration.as_secs_f64(),
			receive_duration_secs: receive_duration.as_secs_f64(),
			full_latency_secs: full_latency.as_secs_f64(),
		};

		assert_eq!(stats.sent_count, expected_count);
		assert_eq!(stats.received_count, expected_count);

		all_round_stats.push(stats);

		if round < num_rounds {
			let elapsed = round_start.elapsed();
			let interval = Duration::from_millis(interval_ms);
			if elapsed < interval {
				tokio::time::sleep(interval - elapsed).await;
			} else {
				warn!(
					"Client {client_id}: Round {} took longer ({}ms) than target ({}ms)",
					round,
					elapsed.as_millis(),
					interval.as_millis()
				);
			}
			barrier.wait().await;
		}
	}

	Ok(all_round_stats)
}

/// Wait until the next sync boundary for synchronized start across multiple machines.
///
/// Uses a 10-minute sync interval. If less than 2 minutes remain until the next boundary,
/// skip it and wait for the following one. This ensures all jobs starting within a
/// 2-minute window will synchronize to the same boundary.
///
/// Example:
/// - Job starts at 10:00 → 10 min until 10:10 (>= 2) → wait until 10:10
/// - Job starts at 10:07 → 3 min until 10:10 (>= 2) → wait until 10:10
/// - Job starts at 10:08 → 2 min until 10:10 (>= 2) → wait until 10:10
/// - Job starts at 10:09 → 1 min until 10:10 (< 2) → wait until 10:20
/// - Job starts at 10:10 → 10 min until 10:20 (>= 2) → wait until 10:20
async fn wait_for_sync_time() {
	let now_secs = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.expect("System time is before UNIX epoch")
		.as_secs();

	// Sync interval in seconds (10 minutes)
	const SYNC_INTERVAL_SECS: u64 = 10 * 60;
	// Minimum wait time: if less than this remains, skip to next boundary (2 minutes)
	const MIN_WAIT_SECS: u64 = 2 * 60;

	let secs_in_current_interval = now_secs % SYNC_INTERVAL_SECS;
	let secs_until_next_boundary = SYNC_INTERVAL_SECS - secs_in_current_interval;

	// If less than MIN_WAIT_SECS until next boundary, wait for the one after
	let wait_secs = if secs_until_next_boundary < MIN_WAIT_SECS {
		secs_until_next_boundary + SYNC_INTERVAL_SECS
	} else {
		secs_until_next_boundary
	};

	let target_timestamp = now_secs + wait_secs;
	info!("Waiting {}s for sync time (target UNIX timestamp: {})", wait_secs, target_timestamp);

	tokio::time::sleep(Duration::from_secs(wait_secs)).await;
	info!("Sync time reached, starting benchmark");
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Generate unique test run ID to avoid interference with old data
	let test_run_id = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.expect("System time is before UNIX epoch")
		.as_secs();

	let args = Args::parse();
	let messages_pattern = parse_messages_pattern(&args.messages_pattern)?;

	if args.rpc_endpoints.is_empty() {
		return Err(anyhow!(
			"At least one RPC endpoint must be provided. Example: --rpc-endpoints ws://localhost:9944"
		));
	}

	log_configuration(&args, &messages_pattern);

	if !args.skip_sync {
		wait_for_sync_time().await;
	}

	let rpc_clients = connect_to_endpoints(&args.rpc_endpoints).await?;

	debug!("Spawning {} client tasks...", args.num_clients);
	let sync_start = std::time::Instant::now();
	let barrier = Arc::new(Barrier::new(args.num_clients as usize));

	let handles: Vec<_> = (0..args.num_clients)
		.map(|client_id| {
			let config = ClientConfig {
				client_id,
				neighbour_id: (client_id + 1) % args.num_clients,
				num_clients: args.num_clients,
				num_rounds: args.num_rounds,
				test_run_id,
				messages_pattern: messages_pattern.clone(),
				receive_timeout_ms: args.receive_timeout_ms,
				interval_ms: args.interval_ms,
			};
			let node_idx = (client_id as usize) % rpc_clients.len();
			let rpc_client = Arc::clone(&rpc_clients[node_idx]);
			let barrier = Arc::clone(&barrier);

			tokio::spawn(run_client(config, rpc_client, barrier, sync_start))
		})
		.collect();

	debug!("Waiting for all clients to complete...");

	let all_round_stats = collect_results(handles).await?;
	print_statistics(&all_round_stats);

	Ok(())
}

fn log_configuration(args: &Args, messages_pattern: &[(usize, usize)]) {
	let endpoints = args.rpc_endpoints.join(", ");
	let pattern_str = messages_pattern
		.iter()
		.map(|(count, size)| format!("{count}x{size}B"))
		.collect::<Vec<_>>()
		.join(", ");
	info!("Starting Statement Store Latency Benchmark: endpoints=[{endpoints}] clients={} rounds={} interval={}ms pattern=[{pattern_str}]", args.num_clients, args.num_rounds, args.interval_ms);
}

async fn connect_to_endpoints(endpoints: &[String]) -> Result<Vec<Arc<WsClient>>, anyhow::Error> {
	let mut clients = Vec::with_capacity(endpoints.len());

	for endpoint in endpoints {
		let client = WsClientBuilder::default()
			.max_concurrent_requests(10000)
			.build(endpoint)
			.await
			.with_context(|| format!("Failed to connect to {endpoint}"))?;
		clients.push(Arc::new(client));
		debug!("Connected to {}", endpoint);
	}

	Ok(clients)
}

async fn collect_results(
	handles: Vec<tokio::task::JoinHandle<Result<Vec<RoundStats>, anyhow::Error>>>,
) -> Result<Vec<RoundStats>, anyhow::Error> {
	let mut all_stats = Vec::new();

	for (i, handle) in handles.into_iter().enumerate() {
		match handle.await {
			Ok(Ok(client_stats)) => all_stats.extend(client_stats),
			Ok(Err(e)) => return Err(e.context(format!("Client {i} failed"))),
			Err(e) => return Err(anyhow!("Client {i} task panicked: {e}")),
		}
	}

	Ok(all_stats)
}

fn print_statistics(stats: &[RoundStats]) {
	let send_stats = calc_stats(stats.iter().map(|s| s.send_duration_secs));
	let receive_stats = calc_stats(stats.iter().map(|s| s.receive_duration_secs));
	let latency_stats = calc_stats(stats.iter().map(|s| s.full_latency_secs));

	info!("Benchmark Results: send_min={:.3}s send_avg={:.3}s send_max={:.3}s receive_min={:.3}s receive_avg={:.3}s receive_max={:.3}s latency_min={:.3}s latency_avg={:.3}s latency_max={:.3}s",
		send_stats.min, send_stats.avg, send_stats.max,
		receive_stats.min, receive_stats.avg, receive_stats.max,
		latency_stats.min, latency_stats.avg, latency_stats.max
	);
}
