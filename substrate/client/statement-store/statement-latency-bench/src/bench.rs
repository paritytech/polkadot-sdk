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
use sp_core::{blake2_256, bounded_vec::BoundedVec, Bytes, ConstU32};
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

	/// Skip time synchronization (for local testing)
	#[arg(long, default_value = "false")]
	skip_sync: bool,

	/// Statement expiry time in milliseconds (default: 10 minutes)
	#[arg(long, default_value_t = 600_000)]
	statement_expiry_ms: u64,

	/// Stop immediately on first round failure instead of continuing
	#[arg(long, default_value = "false")]
	fail_fast: bool,
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

struct RoundFailure {
	round: usize,
	/// Groupable error description (same for all clients hitting the same issue)
	error: String,
	/// Per-client detail (e.g., how many statements were received)
	detail: String,
}

struct ClientResult {
	successes: Vec<RoundStats>,
	failures: Vec<RoundFailure>,
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
}

#[allow(clippy::too_many_arguments)]
async fn execute_round(
	client_id: u32,
	round: usize,
	num_rounds: usize,
	test_run_id: u64,
	neighbour_id: u32,
	expected_count: u32,
	messages_pattern: &[(usize, usize)],
	rpc_client: &WsClient,
	keyring: &sp_core::sr25519::Pair,
	receive_timeout_ms: u64,
	statement_expiry_ms: u64,
) -> Result<RoundStats, RoundFailure> {
	let round_start = std::time::Instant::now();
	let mut sent_count: u32 = 0;

	let expected_topics: Vec<Topic> = (0..expected_count)
		.map(|idx| generate_topic(test_run_id, neighbour_id, round, idx).into())
		.collect();

	let bounded_topics: BoundedVec<Topic, ConstU32<128>> =
		expected_topics.try_into().map_err(|_| RoundFailure {
			round,
			error: "Too many topics".into(),
			detail: format!("max 128, got {expected_count}"),
		})?;

	let mut subscription: Subscription<StatementEvent> = rpc_client
		.subscribe(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAny(bounded_topics)],
			"statement_unsubscribeStatement",
		)
		.await
		.map_err(|e| RoundFailure {
			round,
			error: "Failed to open RPC subscription".into(),
			detail: format!("{e}"),
		})?;

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
			let result: SubmitResult = rpc_client
				.request("statement_submit", rpc_params![encoded])
				.await
				.map_err(|e| RoundFailure {
					round,
					error: "Failed to submit statement via RPC".into(),
					detail: format!("{e}"),
				})?;

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
				return Err(RoundFailure {
					round,
					error: "Statement propagation timeout".into(),
					detail: format!(
						"received {received_count}/{expected_count} after {receive_timeout_ms}ms"
					),
				});
			},
			Ok(None) => {
				return Err(RoundFailure {
					round,
					error: "Subscription closed by server".into(),
					detail: format!("received {received_count}/{expected_count}"),
				});
			},
			Ok(Some(Err(e))) => {
				return Err(RoundFailure {
					round,
					error: "Subscription stream error".into(),
					detail: format!("received {received_count}/{expected_count}, error: {e}"),
				});
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
	let ClientConfig {
		client_id,
		neighbour_id,
		num_clients,
		num_rounds,
		test_run_id,
		messages_pattern,
		receive_timeout_ms,
		interval_ms,
		statement_expiry_ms,
		fail_fast,
	} = config;

	let keyring = get_keypair(client_id);
	let expected_count = messages_per_client(&messages_pattern) as u32;

	// Same cancel-safety caveat as the inter-round barrier: if any peer never reaches
	// this point, the rest would block forever without a timeout.
	if timeout(Duration::from_millis(receive_timeout_ms), barrier.wait())
		.await
		.is_err()
	{
		warn!(
			"Client {client_id}: Startup sync timed out \
			 (another client likely failed before reaching the barrier)"
		);
		peer_failed.store(true, Ordering::Relaxed);
		return ClientResult {
			successes: Vec::new(),
			failures: vec![RoundFailure {
				round: 0,
				error: "Startup sync timed out".into(),
				detail: String::new(),
			}],
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

		let round_result = execute_round(
			client_id,
			round,
			num_rounds,
			test_run_id,
			neighbour_id,
			expected_count,
			&messages_pattern,
			&rpc_client,
			&keyring,
			receive_timeout_ms,
			statement_expiry_ms,
		)
		.await;

		match round_result {
			Ok(stats) => successes.push(stats),
			Err(failure) => {
				if failure.detail.is_empty() {
					warn!("Client {client_id}: Round {round}/{num_rounds}: {}", failure.error);
				} else {
					warn!(
						"Client {client_id}: Round {round}/{num_rounds}: {} ({})",
						failure.error, failure.detail
					);
				}
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
				failures.push(RoundFailure {
					round,
					error: "Peer failed; stopping early".into(),
					detail: String::new(),
				});
				break;
			}
			if timeout(Duration::from_millis(receive_timeout_ms), barrier.wait())
				.await
				.is_err()
			{
				// tokio::sync::Barrier::wait is not cancel-safe: a timed-out waiter leaves
				// `arrived` incremented, so remaining waiters block forever.
				warn!(
					"Client {client_id}: Round {round}/{num_rounds}: \
					 Inter-round sync timed out (another client likely failed)"
				);
				failures.push(RoundFailure {
					round,
					error: "Inter-round sync timed out".into(),
					detail: String::new(),
				});
				peer_failed.store(true, Ordering::Relaxed);
				break;
			}
		}
	}

	ClientResult { successes, failures }
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
	let test_run_id: u64 = rand::random();

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
) -> (Vec<RoundStats>, Vec<RoundFailure>) {
	let mut all_successes = Vec::new();
	let mut all_failures = Vec::new();

	for (i, handle) in handles.into_iter().enumerate() {
		match handle.await {
			Ok(result) => {
				all_successes.extend(result.successes);
				all_failures.extend(result.failures);
			},
			Err(e) => {
				warn!("Client {i}: Task panicked: {e}");
				all_failures.push(RoundFailure {
					round: 0,
					error: "Task panicked".into(),
					detail: format!("{e}"),
				});
			},
		}
	}

	(all_successes, all_failures)
}

fn report_results(
	successes: &[RoundStats],
	failures: &[RoundFailure],
	num_clients: u32,
	num_rounds: usize,
) {
	// Aggregate report only retains the `error` discriminant — per-instance `detail`
	// is emitted at failure time via `warn!` and not summarised here. If a future
	// run needs richer aggregation, fold detail into the per-round group below.
	//
	// `round == 0` is reserved for task-level failures (panics, startup-sync
	// timeouts) that are not tied to a specific round; partition them out so the
	// "Round Failed:" report stays clean.
	let mut failures_by_round: HashMap<usize, Vec<&str>> = HashMap::new();
	let mut task_failures: Vec<&str> = Vec::new();
	for f in failures {
		if f.round == 0 {
			task_failures.push(&f.error);
		} else {
			failures_by_round.entry(f.round).or_default().push(&f.error);
		}
	}
	let mut failed_rounds: Vec<_> = failures_by_round.keys().copied().collect();
	failed_rounds.sort();

	let group_errors = |errors: &[&str]| -> String {
		let mut counts: HashMap<&str, u32> = HashMap::new();
		for error in errors {
			*counts.entry(error).or_default() += 1;
		}
		let mut counts: Vec<_> = counts.into_iter().collect();
		counts.sort_by(|a, b| b.1.cmp(&a.1));
		counts
			.iter()
			.map(|(msg, count)| format!("{msg} ({count})"))
			.collect::<Vec<_>>()
			.join("; ")
	};

	for round in &failed_rounds {
		let errors = &failures_by_round[round];
		let failed_clients = errors.len();
		let errors_str = group_errors(errors);

		warn!(
			"Round Failed: round={round} failed_clients={failed_clients} \
			 total_clients={num_clients} errors=[{errors_str}]"
		);
	}

	if !task_failures.is_empty() {
		let errors_str = group_errors(&task_failures);
		warn!(
			"Task Failed: failed_clients={} total_clients={num_clients} errors=[{errors_str}]",
			task_failures.len()
		);
	}

	if !successes.is_empty() {
		print_statistics(successes);
	}

	// `rounds_with_any_success` counts distinct round numbers in which at least one
	// client succeeded — not "rounds where every client succeeded". A round shows up
	// here even if only one of N clients made it through.
	let rounds_with_any_success: HashSet<usize> = successes.iter().map(|s| s.round).collect();
	let rounds_with_any_success = rounds_with_any_success.len();
	let rounds_with_failures = failed_rounds.len();

	info!(
		"Benchmark Finished: rounds_with_any_success={rounds_with_any_success} \
		 rounds_with_failures={rounds_with_failures} total_rounds={num_rounds} \
		 total_clients={num_clients}"
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
