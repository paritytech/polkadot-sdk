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
use sc_statement_store::test_utils::get_keypair;
use serde::{Deserialize, Serialize};
use sp_core::{blake2_256, bounded_vec::BoundedVec, sr25519, Bytes, ConstU32, Pair};
use sp_statement_store::{Statement, StatementEvent, SubmitResult, Topic, TopicFilter};
use std::{
	collections::{HashMap, HashSet},
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::Duration,
};
use tokio::{sync::Barrier, time::timeout};

#[derive(Parser, Debug)]
#[command(name = "statement-latency-bench")]
#[command(about = "Distributed statement store latency benchmark", long_about = None)]
struct Args {
	/// Comma-separated list of RPC WebSocket endpoints (e.g., `ws://node1:9944,ws://node2:9944`)
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

	/// Statement expiry time in milliseconds (default: 10 minutes)
	#[arg(long, default_value_t = 600_000)]
	statement_expiry_ms: u64,

	/// Stop immediately on first round failure instead of continuing
	#[arg(long, default_value = "false")]
	fail_fast: bool,

	/// Optional SURI / seed phrase for single-account mode
	#[arg(long)]
	seed: Option<String>,
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

struct Stats {
	min: f64,
	avg: f64,
	max: f64,
}

/// Closed set of failure categories. Used as the grouping discriminant in the
/// aggregate report; per-instance diagnostic detail is logged at failure time
/// and not retained.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum FailureKind {
	TooManyTopics,
	SubscribeFailed,
	SubmitFailed,
	PropagationTimeout,
	SubscriptionClosed,
	SubscriptionStreamError,
	PeerFailed,
	InterRoundSyncTimeout,
	StartupSyncTimeout,
	TaskPanicked,
}

impl std::fmt::Display for FailureKind {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(match self {
			Self::TooManyTopics => "Too many topics",
			Self::SubscribeFailed => "Failed to open RPC subscription",
			Self::SubmitFailed => "Failed to submit statement via RPC",
			Self::PropagationTimeout => "Statement propagation timeout",
			Self::SubscriptionClosed => "Subscription closed by server",
			Self::SubscriptionStreamError => "Subscription stream error",
			Self::PeerFailed => "Peer failed; stopping early",
			Self::InterRoundSyncTimeout => "Inter-round sync timed out",
			Self::StartupSyncTimeout => "Startup sync timed out",
			Self::TaskPanicked => "Task panicked",
		})
	}
}

fn fail(
	client_id: impl std::fmt::Display,
	round_info: Option<(usize, usize)>,
	error: FailureKind,
	detail: impl std::fmt::Display,
) -> FailureKind {
	let round_info = match round_info {
		Some((round, num_rounds)) => format!("Round {round}/{num_rounds}: "),
		None => String::new(),
	};
	warn!("Client {client_id}: {round_info}{error} ({detail})");

	error
}

struct ClientResult {
	successes: Vec<RoundStats>,
	failures: Vec<FailureKind>,
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
	statement_expiry_ms: u64,
	fail_fast: bool,
	seed: Option<String>,
}

async fn execute_round(
	round: usize,
	config: &ClientConfig,
	rpc_client: &WsClient,
	keyring: &sp_core::sr25519::Pair,
) -> Result<RoundStats, FailureKind> {
	let &ClientConfig {
		client_id,
		neighbour_id,
		num_rounds,
		test_run_id,
		ref messages_pattern,
		receive_timeout_ms,
		statement_expiry_ms,
		..
	} = config;

	let expected_count = messages_per_client(messages_pattern) as u32;
	let round_start = std::time::Instant::now();
	let mut sent_count: u32 = 0;

	let expected_topics: Vec<Topic> = (0..expected_count)
		.map(|idx| generate_topic(test_run_id, neighbour_id, round, idx).into())
		.collect();

	let bounded_topics: BoundedVec<Topic, ConstU32<128>> =
		expected_topics.try_into().map_err(|_| {
			fail(
				client_id,
				Some((round, num_rounds)),
				FailureKind::TooManyTopics,
				format!("max 128, got {expected_count}"),
			)
		})?;

	let mut subscription: Subscription<StatementEvent> = rpc_client
		.subscribe(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAny(bounded_topics)],
			"statement_unsubscribeStatement",
		)
		.await
		.map_err(|e| fail(client_id, Some((round, num_rounds)), FailureKind::SubscribeFailed, e))?;

	for &(count, size) in messages_pattern {
		for _ in 0..count {
			let topic = generate_topic(test_run_id, client_id, round, sent_count);
			let channel = blake2_256(sent_count.to_le_bytes().as_ref());

			let expiry_timestamp = (std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.expect("System clock before UNIX_EPOCH") +
				Duration::from_millis(statement_expiry_ms))
			.as_secs() as u32;

			let mut statement = Statement::new();
			statement.set_channel(channel);
			statement.set_expiry_from_parts(expiry_timestamp, (sent_count + 1) * (round as u32));
			statement.set_topic(0, topic.into());
			statement.set_plain_data(vec![0u8; size]);
			statement.sign_sr25519_private(keyring);

			let encoded: Bytes = statement.encode().into();
			let result: SubmitResult =
				rpc_client.request("statement_submit", rpc_params![encoded]).await.map_err(
					|e| fail(client_id, Some((round, num_rounds)), FailureKind::SubmitFailed, e),
				)?;

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
		let result = timeout(Duration::from_millis(receive_timeout_ms), subscription.next()).await;

		match result {
			Ok(Some(Ok(StatementEvent::NewStatements { statements, .. }))) => {
				received_count += statements.len() as u32;
				if is_leader(client_id) {
					debug!(
						"Round {}/{}. Received {} statement(s) (batch of {})",
						round,
						num_rounds,
						received_count,
						statements.len()
					);
				}
			},
			Err(_) => {
				return Err(fail(
					client_id,
					Some((round, num_rounds)),
					FailureKind::PropagationTimeout,
					format!(
						"received {received_count}/{expected_count} after {receive_timeout_ms}ms"
					),
				));
			},
			Ok(None) => {
				return Err(fail(
					client_id,
					Some((round, num_rounds)),
					FailureKind::SubscriptionClosed,
					format!("received {received_count}/{expected_count}"),
				));
			},
			Ok(Some(Err(e))) => {
				return Err(fail(
					client_id,
					Some((round, num_rounds)),
					FailureKind::SubscriptionStreamError,
					format!("received {received_count}/{expected_count}, error: {e}"),
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

	Ok(RoundStats {
		round,
		sent_count,
		received_count,
		send_duration_secs: send_duration.as_secs_f64(),
		receive_duration_secs: receive_duration.as_secs_f64(),
		full_latency_secs: full_latency.as_secs_f64(),
	})
}

async fn run_client(
	config: ClientConfig,
	rpc_client: Arc<WsClient>,
	barrier: Arc<Barrier>,
	peer_failed: Arc<AtomicBool>,
	sync_start: std::time::Instant,
) -> ClientResult {
	let &ClientConfig {
		client_id,
		num_clients,
		num_rounds,
		receive_timeout_ms,
		interval_ms,
		fail_fast,
		..
	} = &config;

	let keyring = match &config.seed {
		Some(suri) => sr25519::Pair::from_string(suri, None).expect("--seed validated at startup"),
		None => get_keypair(client_id),
	};

	// Same cancel-safety caveat as the inter-round barrier: if any peer never reaches
	// this point, the rest would block forever without a timeout.
	if timeout(Duration::from_millis(receive_timeout_ms), barrier.wait())
		.await
		.is_err()
	{
		peer_failed.store(true, Ordering::Relaxed);
		return ClientResult {
			successes: Vec::new(),
			failures: vec![fail(
				client_id,
				None,
				FailureKind::StartupSyncTimeout,
				"another client likely failed before reaching the barrier",
			)],
		};
	}

	if is_leader(client_id) {
		info!(
			"All {} tasks synchronized and starting in {:.3}s",
			num_clients,
			sync_start.elapsed().as_secs_f64()
		);
	}

	// Apply jitter to distribute connection load (using prime multiplier for better distribution)
	let submission_jitter = ((client_id * 7) % 1000) as u64;
	tokio::time::sleep(Duration::from_millis(submission_jitter)).await;

	let mut successes = Vec::with_capacity(num_rounds);
	let mut failures = Vec::new();

	// Use human 1-based round numbering for logging
	for round in 1..(num_rounds + 1) {
		let round_start = std::time::Instant::now();
		let round_result = execute_round(round, &config, &rpc_client, &keyring).await;
		match round_result {
			Ok(stats) => successes.push(stats),
			Err(failure) => {
				// `fail` already emitted the per-client warn; just record and decide.
				failures.push(failure);
				peer_failed.store(true, Ordering::Relaxed);
				if fail_fast {
					break;
				}
				// Skip the inter-round barrier so this round isn't double-counted as a
				// sync timeout in addition to its real failure.
				continue;
			},
		}

		if round < num_rounds {
			let elapsed = round_start.elapsed();
			let interval = Duration::from_millis(interval_ms);
			if elapsed < interval {
				tokio::time::sleep(interval - elapsed).await;
			} else if is_leader(client_id) {
				warn!(
					"Client {client_id}: Round {} took longer ({}ms) than target ({}ms)",
					round,
					elapsed.as_millis(),
					interval.as_millis()
				);
			}
			if peer_failed.load(Ordering::Relaxed) {
				failures.push(FailureKind::PeerFailed);
				break;
			}
			if timeout(Duration::from_millis(receive_timeout_ms), barrier.wait())
				.await
				.is_err()
			{
				// tokio::sync::Barrier::wait is not cancel-safe: a timed-out waiter leaves
				// `arrived` incremented, so remaining waiters block forever.
				failures.push(fail(
					client_id,
					Some((round, num_rounds)),
					FailureKind::InterRoundSyncTimeout,
					"another client likely failed",
				));
				peer_failed.store(true, Ordering::Relaxed);
				break;
			}
		}
	}

	ClientResult { successes, failures }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Generate unique test run ID to avoid interference with old data
	let test_run_id: u64 = rand::random();

	let args = Args::parse();
	let messages_pattern = parse_messages_pattern(&args.messages_pattern)?;

	if args.rpc_endpoints.is_empty() {
		return Err(anyhow!(
			"At least one RPC endpoint must be provided. Example: --rpc-endpoints ws://localhost:9944"
		));
	}

	if let Some(seed) = &args.seed {
		if args.num_clients != 1 {
			return Err(anyhow!(
				"--seed requires --num-clients=1 (single-account quota model); got num_clients={}",
				args.num_clients
			));
		}
		sr25519::Pair::from_string(seed, None)
			.map_err(|e| anyhow!("Invalid --seed SURI: {e:?}"))?;
	}

	log_configuration(&args, &messages_pattern);

	let rpc_clients = connect_to_endpoints(&args.rpc_endpoints).await?;

	info!("Spawning {} client tasks... {}", args.num_clients, test_run_id);
	let sync_start = std::time::Instant::now();
	let barrier = Arc::new(Barrier::new(args.num_clients as usize));
	let peer_failed = Arc::new(AtomicBool::new(false));

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
				statement_expiry_ms: args.statement_expiry_ms,
				fail_fast: args.fail_fast,
				seed: args.seed.clone(),
			};
			let node_idx = (client_id as usize) % rpc_clients.len();
			let rpc_client = Arc::clone(&rpc_clients[node_idx]);
			let barrier = Arc::clone(&barrier);
			let peer_failed = Arc::clone(&peer_failed);

			tokio::spawn(run_client(config, rpc_client, barrier, peer_failed, sync_start))
		})
		.collect();

	debug!("Waiting for all clients to complete...");

	let (all_successes, all_failures) = collect_results(handles).await;
	report_results(&all_successes, &all_failures, args.num_clients, args.num_rounds);

	if !all_failures.is_empty() && all_successes.is_empty() {
		return Err(anyhow!("Benchmark failed: no rounds completed successfully"));
	}

	Ok(())
}

fn log_configuration(args: &Args, messages_pattern: &[(usize, usize)]) {
	let endpoints = args.rpc_endpoints.join(", ");
	let pattern_str = messages_pattern
		.iter()
		.map(|(count, size)| format!("{count}x{size}B"))
		.collect::<Vec<_>>()
		.join(", ");
	info!(
		"Starting Statement Store Latency Benchmark: \
		 endpoints=[{endpoints}] clients={} rounds={} interval={}ms pattern=[{pattern_str}]",
		args.num_clients, args.num_rounds, args.interval_ms
	);
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
	handles: Vec<tokio::task::JoinHandle<ClientResult>>,
) -> (Vec<RoundStats>, Vec<FailureKind>) {
	let mut all_successes = Vec::new();
	let mut all_failures = Vec::new();

	for (i, handle) in handles.into_iter().enumerate() {
		match handle.await {
			Ok(result) => {
				all_successes.extend(result.successes);
				all_failures.extend(result.failures);
			},
			Err(e) => {
				all_failures.push(fail(i, None, FailureKind::TaskPanicked, e));
			},
		}
	}

	(all_successes, all_failures)
}

fn report_results(
	successes: &[RoundStats],
	failures: &[FailureKind],
	num_clients: u32,
	num_rounds: usize,
) {
	if !failures.is_empty() {
		let mut counts: HashMap<FailureKind, u32> = HashMap::new();
		for &kind in failures {
			*counts.entry(kind).or_default() += 1;
		}
		let mut counts: Vec<_> = counts.into_iter().collect();
		counts.sort_by(|a, b| b.1.cmp(&a.1));
		let errors_str = counts
			.iter()
			.map(|(kind, count)| format!("{kind} ({count})"))
			.collect::<Vec<_>>()
			.join("; ");

		warn!(
			"Benchmark Failed: failed_clients={} total_clients={num_clients} errors=[{errors_str}]",
			failures.len()
		);
	}

	if !successes.is_empty() {
		print_statistics(successes);
	}

	let rounds_with_any_success = successes.iter().map(|s| s.round).collect::<HashSet<_>>().len();

	info!(
		"Benchmark Finished: rounds_with_any_success={rounds_with_any_success} \
		 total_rounds={num_rounds} total_clients={num_clients}"
	);
}

fn print_statistics(stats: &[RoundStats]) {
	let send_stats = calc_stats(stats.iter().map(|s| s.send_duration_secs));
	let receive_stats = calc_stats(stats.iter().map(|s| s.receive_duration_secs));
	let latency_stats = calc_stats(stats.iter().map(|s| s.full_latency_secs));

	info!(
		"Benchmark Results: \
		 send_min={:.3}s send_avg={:.3}s send_max={:.3}s \
		 receive_min={:.3}s receive_avg={:.3}s receive_max={:.3}s \
		 latency_min={:.3}s latency_avg={:.3}s latency_max={:.3}s",
		send_stats.min,
		send_stats.avg,
		send_stats.max,
		receive_stats.min,
		receive_stats.avg,
		receive_stats.max,
		latency_stats.min,
		latency_stats.avg,
		latency_stats.max
	);
}
