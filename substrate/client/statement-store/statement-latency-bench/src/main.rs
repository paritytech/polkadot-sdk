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
use serde::{Deserialize, Serialize};
use sp_core::{blake2_256, bounded_vec::BoundedVec, sr25519, Bytes, ConstU32, Pair};
use sp_statement_store::{
	Statement, StatementAllowance, StatementEvent, SubmitResult, Topic, TopicFilter,
};
use std::{any::Any, str::FromStr, sync::Arc, time::Duration};
use subxt::{
	config::{
		transaction_extensions::{
			AnyOf, ChargeAssetTxPayment, ChargeTransactionPayment, CheckGenesis, CheckMetadataHash,
			CheckMortality, CheckNonce, CheckSpecVersion, CheckTxVersion, TransactionExtension,
			VerifySignatureDetails,
		},
		Config, DefaultExtrinsicParamsBuilder, ExtrinsicParams, ExtrinsicParamsEncoder,
	},
	ext::scale_value::{value, Value},
	utils::Static,
	OnlineClient, PolkadotConfig,
};
use subxt_signer::{sr25519::Keypair as SubxtKeypair, SecretUri};
use tokio::{sync::Barrier, time::timeout};

#[derive(Debug, Clone, clap::ValueEnum)]
enum Scenario {
	/// Use custom CLI arguments (backward-compatible default)
	Custom,
	/// Sustained peak rate (~125 stmts/sec) for throughput validation
	Throughput,
	/// High-volume test pushing 1M statements for capacity stress
	Volume,
	/// Viral burst: 120K statements with periodic spikes
	Burst,
	/// Full event simulation with mixed message sizes and workloads
	Event,
	/// Near-limit capacity test pushing ~3.8M statements (95% of 4M max)
	CapacityMax,
}

struct ScenarioParams {
	num_clients: u32,
	messages_pattern: String,
	num_rounds: usize,
	interval_ms: u64,
	receive_timeout_ms: u64,
	statement_expiry_ms: u64,
}

/// Returns preset parameters for a given scenario, or None for Custom.
fn resolve_scenario(scenario: &Scenario) -> Option<ScenarioParams> {
	match scenario {
		Scenario::Custom => None,
		Scenario::Throughput => Some(ScenarioParams {
			num_clients: 500,
			messages_pattern: "1:384".to_string(),
			num_rounds: 30,
			interval_ms: 4_000,
			receive_timeout_ms: 10_000,
			statement_expiry_ms: 600_000,
		}),
		Scenario::Volume => Some(ScenarioParams {
			num_clients: 2000,
			messages_pattern: "50:192".to_string(),
			num_rounds: 10,
			interval_ms: 30_000,
			receive_timeout_ms: 30_000,
			statement_expiry_ms: 1_800_000,
		}),
		Scenario::Burst => Some(ScenarioParams {
			num_clients: 200,
			messages_pattern: "5:1024".to_string(),
			num_rounds: 120,
			interval_ms: 8_000,
			receive_timeout_ms: 15_000,
			statement_expiry_ms: 600_000,
		}),
		Scenario::Event => Some(ScenarioParams {
			num_clients: 1000,
			messages_pattern: "3:384,1:1024,1:128".to_string(),
			num_rounds: 120,
			interval_ms: 40_000,
			receive_timeout_ms: 30_000,
			statement_expiry_ms: 3_600_000,
		}),
		Scenario::CapacityMax => Some(ScenarioParams {
			num_clients: 2000,
			messages_pattern: "100:192".to_string(),
			num_rounds: 19,
			interval_ms: 30_000,
			receive_timeout_ms: 60_000,
			statement_expiry_ms: 1_800_000,
		}),
	}
}

pub struct VerifyMultiSignature<T: Config>(VerifySignatureDetails<T>);

impl<T: Config> ExtrinsicParams<T> for VerifyMultiSignature<T> {
	type Params = ();

	fn new(
		_client: &subxt::client::ClientState<T>,
		_params: Self::Params,
	) -> Result<Self, subxt::config::ExtrinsicParamsError> {
		Ok(VerifyMultiSignature(VerifySignatureDetails::Disabled))
	}
}

impl<T: Config> ExtrinsicParamsEncoder for VerifyMultiSignature<T> {
	fn encode_value_to(&self, v: &mut Vec<u8>) {
		self.0.encode_to(v);
	}

	fn inject_signature(&mut self, account: &dyn Any, signature: &dyn Any) {
		let account = account
			.downcast_ref::<T::AccountId>()
			.expect("A T::AccountId should have been provided")
			.clone();
		let signature = signature
			.downcast_ref::<T::Signature>()
			.expect("A T::Signature should have been provided")
			.clone();
		self.0 = VerifySignatureDetails::Signed { signature, account };
	}
}

impl<T: Config> TransactionExtension<T> for VerifyMultiSignature<T> {
	type Decoded = Static<VerifySignatureDetails<T>>;

	fn matches(identifier: &str, _type_id: u32, _types: &::scale_info::PortableRegistry) -> bool {
		identifier == "VerifyMultiSignature" || identifier == "VerifySignature"
	}
}

fn statement_allowance_key(account_id: impl AsRef<[u8]>) -> Vec<u8> {
	let mut key = b":statement_allowance:".to_vec();
	key.extend_from_slice(account_id.as_ref());
	key
}

/// Check whether a type requires 0 bytes to encode (mirrors subxt's internal `is_type_empty`)
///
/// Empty types are automatically skipped by `AnyOf`, so our catch-all handlers must not claim
/// them - otherwise they waste a slot that could be used for a non-empty unknown extension
fn is_type_empty(type_id: u32, types: &::scale_info::PortableRegistry) -> bool {
	use scale_info::TypeDef;
	let Some(ty) = types.resolve(type_id) else {
		return false;
	};
	match &ty.type_def {
		TypeDef::Composite(c) => c.fields.iter().all(|f| is_type_empty(f.ty.id, types)),
		TypeDef::Array(a) => a.len == 0 || is_type_empty(a.type_param.id, types),
		TypeDef::Tuple(t) => t.fields.iter().all(|f| is_type_empty(f.id, types)),
		_ => false,
	}
}

macro_rules! define_skip_unknown_extensions {
	($($name:ident),+ $(,)?) => { $(
		pub struct $name;

		impl<T: Config> ExtrinsicParams<T> for $name {
			type Params = ();

			fn new(
				_client: &subxt::client::ClientState<T>,
				_params: Self::Params,
			) -> Result<Self, subxt::config::ExtrinsicParamsError> {
				Ok($name)
			}
		}

		impl ExtrinsicParamsEncoder for $name {
			fn encode_value_to(&self, v: &mut Vec<u8>) {
				v.push(0x00);
			}
		}

		impl<T: Config> TransactionExtension<T> for $name {
			type Decoded = Static<u8>;

			fn matches(
				_identifier: &str,
				type_id: u32,
				types: &::scale_info::PortableRegistry,
			) -> bool {
				!is_type_empty(type_id, types)
			}
		}
	)+ };
}

define_skip_unknown_extensions!(
	SkipUnknown1,
	SkipUnknown2,
	SkipUnknown3,
	SkipUnknown4,
	SkipUnknown5,
	SkipUnknown6,
	SkipUnknown7,
	SkipUnknown8,
);

type BenchExtrinsicParams<T> = AnyOf<
	T,
	(
		VerifyMultiSignature<T>,
		CheckSpecVersion,
		CheckTxVersion,
		CheckNonce,
		CheckGenesis<T>,
		CheckMortality<T>,
		ChargeAssetTxPayment<T>,
		ChargeTransactionPayment,
		CheckMetadataHash,
		SkipUnknown1,
		SkipUnknown2,
		SkipUnknown3,
		SkipUnknown4,
		SkipUnknown5,
		SkipUnknown6,
		SkipUnknown7,
		SkipUnknown8,
	),
>;

/// Custom subxt [`Config`] identical to [`PolkadotConfig`] but using [`BenchExtrinsicParams`].
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum BenchConfig {}

impl Config for BenchConfig {
	type AccountId = <PolkadotConfig as Config>::AccountId;
	type Address = <PolkadotConfig as Config>::Address;
	type Signature = <PolkadotConfig as Config>::Signature;
	type Hasher = <PolkadotConfig as Config>::Hasher;
	type Header = <PolkadotConfig as Config>::Header;
	type ExtrinsicParams = BenchExtrinsicParams<Self>;
	type AssetId = <PolkadotConfig as Config>::AssetId;
}

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

	/// Statement expiry time in milliseconds (default: 10 minutes)
	#[arg(long, default_value_t = 600_000)]
	statement_expiry_ms: u64,
	/// Sudo seed/SURI for setting statement allowances (e.g., "//Alice" or mnemonic phrase).
	/// When provided, deterministic accounts are used and allowances are set on-chain.
	#[arg(long)]
	sudo_seed: Option<String>,

	/// Number of accounts per allowance-setting transaction (default: 100).
	#[arg(long, default_value = "100")]
	allowance_batch_size: u32,

	/// Predefined scenario profile (overrides num-clients, messages-pattern, etc.)
	#[arg(long, value_enum, default_value = "custom")]
	scenario: Scenario,

	/// Path to write JSON benchmark report.
	#[arg(long)]
	report_json: Option<String>,

	/// Maximum acceptable full latency in seconds. Exceeding this fails the benchmark
	#[arg(long)]
	max_latency_secs: Option<f64>,

	/// Minimum acceptable success rate (0.0 to 1.0). Below this fails the benchmark
	#[arg(long)]
	min_success_rate: Option<f64>,

	/// Minimum acceptable throughput in statements per second
	#[arg(long)]
	min_throughput: Option<f64>,

	/// Delay in seconds before starting the benchmark. Allows expired statements to be cleaned
	/// up.
	#[arg(long, default_value_t = 0)]
	warmup_delay_secs: u64,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkReport {
	scenario: String,
	started_at: String,
	total_duration_secs: f64,
	num_clients: u32,
	num_rounds: usize,
	total_sent: u64,
	total_received: u64,
	total_bytes_sent: u64,
	success_rate: f64,
	throughput_stmts_per_sec: f64,
	throughput_bytes_per_sec: f64,
	send_latency: Stats,
	receive_latency: Stats,
	full_latency: Stats,
	rounds: Vec<RoundReport>,
	passed: bool,
	failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoundReport {
	round: usize,
	total_sent: u64,
	total_received: u64,
	send_duration_secs: Stats,
	receive_duration_secs: Stats,
	full_latency_secs: Stats,
	throughput_stmts_per_sec: f64,
}

/// Builds a BenchmarkReport from collected per-client RoundStats
fn generate_report(
	scenario_name: &str,
	started_at: &str,
	total_duration_secs: f64,
	num_clients: u32,
	num_rounds: usize,
	messages_pattern: &[(usize, usize)],
	all_round_stats: &[RoundStats],
) -> BenchmarkReport {
	let total_sent: u64 = all_round_stats.iter().map(|s| s.sent_count as u64).sum();
	let total_received: u64 = all_round_stats.iter().map(|s| s.received_count as u64).sum();

	// Calculate total bytes sent based on message pattern sizes.
	let bytes_per_client: u64 =
		messages_pattern.iter().map(|(count, size)| (*count as u64) * (*size as u64)).sum();
	let total_bytes_sent = bytes_per_client * num_clients as u64 * num_rounds as u64;

	let success_rate = if total_sent > 0 { total_received as f64 / total_sent as f64 } else { 0.0 };
	let throughput_stmts = if total_duration_secs > 0.0 {
		total_sent as f64 / total_duration_secs
	} else {
		0.0
	};
	let throughput_bytes = if total_duration_secs > 0.0 {
		total_bytes_sent as f64 / total_duration_secs
	} else {
		0.0
	};

	// Aggregate per-round stats across all clients
	let mut round_reports = Vec::new();
	for round_num in 1..=num_rounds {
		let round_data: Vec<&RoundStats> =
			all_round_stats.iter().filter(|s| s.round == round_num).collect();
		if round_data.is_empty() {
			continue;
		}

		let round_sent: u64 = round_data.iter().map(|s| s.sent_count as u64).sum();
		let round_received: u64 = round_data.iter().map(|s| s.received_count as u64).sum();

		// Round throughput: total statements sent in this round / max full latency across clients.
		let max_full_latency = round_data
			.iter()
			.map(|s| s.full_latency_secs)
			.fold(0.0_f64, f64::max);

		let round_throughput =
			if max_full_latency > 0.0 { round_sent as f64 / max_full_latency } else { 0.0 };

		round_reports.push(RoundReport {
			round: round_num,
			total_sent: round_sent,
			total_received: round_received,
			send_duration_secs: calc_stats(round_data.iter().map(|s| s.send_duration_secs)),
			receive_duration_secs: calc_stats(round_data.iter().map(|s| s.receive_duration_secs)),
			full_latency_secs: calc_stats(round_data.iter().map(|s| s.full_latency_secs)),
			throughput_stmts_per_sec: round_throughput,
		});
	}

	BenchmarkReport {
		scenario: scenario_name.to_string(),
		started_at: started_at.to_string(),
		total_duration_secs,
		num_clients,
		num_rounds,
		total_sent,
		total_received,
		total_bytes_sent,
		success_rate,
		throughput_stmts_per_sec: throughput_stmts,
		throughput_bytes_per_sec: throughput_bytes,
		send_latency: calc_stats(all_round_stats.iter().map(|s| s.send_duration_secs)),
		receive_latency: calc_stats(all_round_stats.iter().map(|s| s.receive_duration_secs)),
		full_latency: calc_stats(all_round_stats.iter().map(|s| s.full_latency_secs)),
		rounds: round_reports,
		passed: true,
		failures: Vec::new(),
	}
}

fn check_thresholds(
	report: &mut BenchmarkReport,
	max_latency_secs: Option<f64>,
	min_success_rate: Option<f64>,
	min_throughput: Option<f64>,
) {
	if let Some(max_lat) = max_latency_secs {
		if report.full_latency.max > max_lat {
			report.passed = false;
			report.failures.push(format!(
				"full latency {:.3}s exceeds max {:.3}s",
				report.full_latency.max, max_lat
			));
		}
	}

	if let Some(min_sr) = min_success_rate {
		if report.success_rate < min_sr {
			report.passed = false;
			report.failures.push(format!(
				"success rate {:.4} below min {:.4}",
				report.success_rate, min_sr
			));
		}
	}

	if let Some(min_tp) = min_throughput {
		if report.throughput_stmts_per_sec < min_tp {
			report.passed = false;
			report.failures.push(format!(
				"throughput {:.1} stmts/sec below min {:.1}",
				report.throughput_stmts_per_sec, min_tp
			));
		}
	}
}

fn write_json_report(report: &BenchmarkReport, path: &str) -> Result<(), anyhow::Error> {
	let json = serde_json::to_string_pretty(report)
		.with_context(|| "Failed to serialize benchmark report")?;
	std::fs::write(path, json).with_context(|| format!("Failed to write report to {path}"))?;
	info!("Benchmark report written to {path}");
	Ok(())
}

fn is_leader(client_id: u32) -> bool {
	client_id == 0
}

fn generate_topic(test_run_id: u64, client_id: u32, round: usize, msg_idx: u32) -> [u8; 32] {
	let topic_str = format!("{test_run_id}-{client_id}-{round}-{msg_idx}");
	blake2_256(topic_str.as_bytes())
}

/// Generate a deterministic keypair for a given client index.
///
/// Uses the same derivation path as the zombienet statement-store benchmarks
/// so that accounts are identical across runs.
fn get_keypair(idx: u32) -> sr25519::Pair {
	sr25519::Pair::from_string(&format!("//StatementBench//{idx}"), None)
		.expect("Derivation path is always valid; qed")
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
		statement_expiry_ms,
	} = config;

	let keyring = get_keypair(client_id);
	let expected_count = messages_per_client(&messages_pattern) as u32;

	barrier.wait().await;

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

	let mut all_round_stats = Vec::with_capacity(num_rounds);

	// Use human 1-based round numbering for logging
	for round in 1..(num_rounds + 1) {
		let round_start = std::time::Instant::now();
		let mut sent_count: u32 = 0;

		let expected_topics: Vec<Topic> = (0..expected_count)
			.map(|idx| generate_topic(test_run_id, neighbour_id, round, idx).into())
			.collect();

		let bounded_topics: BoundedVec<Topic, ConstU32<128>> = expected_topics
			.try_into()
			.map_err(|_| anyhow!("Client {client_id}: Too many topics (max 128)"))?;

		let mut subscription: Subscription<StatementEvent> = rpc_client
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
				let channel_str = format!("{test_run_id}-{sent_count}");
				let channel = blake2_256(channel_str.as_bytes());

				let expiry_timestamp = (std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.unwrap_or_default() +
					Duration::from_millis(statement_expiry_ms))
				.as_secs() as u32;

				let mut statement = Statement::new();
				statement.set_channel(channel);
				statement
					.set_expiry_from_parts(expiry_timestamp, (sent_count + 1) * (round as u32));
				statement.set_topic(0, topic.into());
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

/// Set statement allowances for all deterministic benchmark accounts in a single
/// `Sudo(Utility(batch_all { calls: [System(set_storage { items }), ...] }))` transaction.
///
/// Storage items are grouped into inner `set_storage` calls of `batch_size` each to keep
/// individual call payloads small, but all inner calls are submitted atomically in one
/// `batch_all` wrapped in `Sudo`.
async fn set_allowances(
	rpc_url: &str,
	rpc_client: &WsClient,
	sudo_seed: &str,
	num_clients: u32,
	batch_size: u32,
) -> Result<(), anyhow::Error> {
	let client = OnlineClient::<BenchConfig>::from_insecure_url(rpc_url).await?;

	let uri = SecretUri::from_str(sudo_seed).map_err(|e| anyhow!("Invalid sudo seed URI: {e}"))?;
	let sudo_key =
		SubxtKeypair::from_uri(&uri).map_err(|e| anyhow!("Failed to derive sudo keypair: {e}"))?;

	let allowance_value = StatementAllowance::new(100_000, 1_000_000).encode();

	let storage_calls: Vec<Value> = (0..num_clients)
		.step_by(batch_size as usize)
		.map(|chunk_start| {
			let chunk_end = std::cmp::min(chunk_start + batch_size, num_clients);

			let items: Vec<Value> = (chunk_start..chunk_end)
				.map(|i| {
					let pub_key = get_keypair(i).public();
					let storage_key = statement_allowance_key(pub_key.as_ref() as &[u8]);

					let hex_key: String = storage_key.iter().map(|b| format!("{b:02x}")).collect();
					info!("Account {i}: pubkey={pub_key} storage_key=0x{hex_key}");

					Value::unnamed_composite([
						Value::from_bytes(storage_key),
						Value::from_bytes(allowance_value.clone()),
					])
				})
				.collect();

			value! { System(set_storage { items: items }) }
		})
		.collect();

	let num_inner_calls = storage_calls.len();
	info!(
		"Submitting {} set_storage calls for {} accounts in a single Sudo(batch_all) transaction",
		num_inner_calls, num_clients
	);

	let batch_call = value! { Utility(batch_all { calls: storage_calls }) };
	let tx = subxt::tx::dynamic("Sudo", "sudo", vec![batch_call]);
	let dp = DefaultExtrinsicParamsBuilder::<BenchConfig>::new().immortal().build();
	let extensions =
		(dp.0, dp.1, dp.2, dp.3, dp.4, dp.5, dp.6, dp.7, dp.8, (), (), (), (), (), (), (), ());

	let mut progress = client
		.tx()
		.create_signed(&tx, &sudo_key, extensions)
		.await?
		.submit_and_watch()
		.await?;

	use subxt::tx::TxStatus;
	while let Some(status) = progress.next().await.transpose()? {
		match status {
			TxStatus::InFinalizedBlock(tx_in_block) => {
				tx_in_block.wait_for_success().await?;
				info!(
					"All {} account allowances finalized in block {:#?}",
					num_clients,
					tx_in_block.block_hash()
				);
				break;
			},
			TxStatus::Error { message } |
			TxStatus::Invalid { message } |
			TxStatus::Dropped { message } => {
				return Err(anyhow!("Allowance tx failed: {message}"));
			},
			_ => continue,
		}
	}

	// Verify that allowances were actually written to storage.
	// The statement store reads allowances from the FINALIZED block
	let finalized_hash: String = rpc_client
		.request("chain_getFinalizedHead", rpc_params![])
		.await
		.context("Failed to get finalized head")?;
	info!("Finalized head for verification: {finalized_hash}");

	for i in 0..num_clients {
		let pub_key = get_keypair(i).public();
		let storage_key = statement_allowance_key(pub_key.as_ref() as &[u8]);
		let hex_key: String = storage_key.iter().map(|b| format!("{b:02x}")).collect();

		// Check at best block
		let result_best: Option<String> = rpc_client
			.request("state_getStorage", rpc_params![format!("0x{hex_key}")])
			.await
			.with_context(|| format!("Failed to verify allowance for account {i} at best"))?;

		// Check at finalized block
		let result_finalized: Option<String> = rpc_client
			.request("state_getStorage", rpc_params![format!("0x{hex_key}"), &finalized_hash])
			.await
			.with_context(|| format!("Failed to verify allowance for account {i} at finalized"))?;

		info!(
			"Account {i}: allowance at best={:?}, at finalized={:?}",
			result_best, result_finalized
		);

		if result_finalized.is_none() {
			return Err(anyhow!(
				"Account {i}: allowance NOT found at finalized block {finalized_hash}"
			));
		}
	}

	Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// Generate unique test run ID to avoid interference with old data.
	let test_run_id: u64 = rand::random();

	let args = Args::parse();

	// Resolve effective parameters: scenario presets override CLI args
	let scenario_params = resolve_scenario(&args.scenario);
	let scenario_name = format!("{:?}", args.scenario).to_lowercase();

	let eff_num_clients = scenario_params.as_ref().map_or(args.num_clients, |p| p.num_clients);
	let eff_messages_pattern_str = scenario_params
		.as_ref()
		.map_or(args.messages_pattern.clone(), |p| p.messages_pattern.clone());
	let eff_num_rounds = scenario_params.as_ref().map_or(args.num_rounds, |p| p.num_rounds);
	let eff_interval_ms = scenario_params.as_ref().map_or(args.interval_ms, |p| p.interval_ms);
	let eff_receive_timeout_ms =
		scenario_params.as_ref().map_or(args.receive_timeout_ms, |p| p.receive_timeout_ms);
	let eff_statement_expiry_ms =
		scenario_params.as_ref().map_or(args.statement_expiry_ms, |p| p.statement_expiry_ms);

	let messages_pattern = parse_messages_pattern(&eff_messages_pattern_str)?;

	if args.rpc_endpoints.is_empty() {
		return Err(anyhow!(
			"At least one RPC endpoint must be provided. Example: --rpc-endpoints ws://localhost:9944"
		));
	}

	log_effective_configuration(
		&scenario_name,
		&args.rpc_endpoints,
		eff_num_clients,
		eff_num_rounds,
		eff_interval_ms,
		&messages_pattern,
	);

	if !args.skip_sync {
		wait_for_sync_time().await;
	}

	// Warmup delay: sleep to allow enforce_limits() to clear expired statements.
	if args.warmup_delay_secs > 0 {
		info!("Warmup delay: sleeping {}s before starting benchmark", args.warmup_delay_secs);
		tokio::time::sleep(Duration::from_secs(args.warmup_delay_secs)).await;
		info!("Warmup delay complete, proceeding with benchmark");
	}

	let rpc_clients = connect_to_endpoints(&args.rpc_endpoints).await?;

	if let Some(ref sudo_seed) = args.sudo_seed {
		set_allowances(
			&args.rpc_endpoints[0],
			&rpc_clients[0],
			sudo_seed,
			eff_num_clients,
			args.allowance_batch_size,
		)
		.await?;
	}

	let started_at = chrono_now_iso8601();

	info!("Spawning {} client tasks... {}", eff_num_clients, test_run_id);
	let benchmark_start = std::time::Instant::now();
	let barrier = Arc::new(Barrier::new(eff_num_clients as usize));

	let handles: Vec<_> = (0..eff_num_clients)
		.map(|client_id| {
			let config = ClientConfig {
				client_id,
				neighbour_id: (client_id + 1) % eff_num_clients,
				num_clients: eff_num_clients,
				num_rounds: eff_num_rounds,
				test_run_id,
				messages_pattern: messages_pattern.clone(),
				receive_timeout_ms: eff_receive_timeout_ms,
				interval_ms: eff_interval_ms,
				statement_expiry_ms: eff_statement_expiry_ms,
			};
			let node_idx = (client_id as usize) % rpc_clients.len();
			let rpc_client = Arc::clone(&rpc_clients[node_idx]);
			let barrier = Arc::clone(&barrier);

			tokio::spawn(run_client(config, rpc_client, barrier, benchmark_start))
		})
		.collect();

	debug!("Waiting for all clients to complete...");

	let all_round_stats = collect_results(handles).await?;
	let total_duration_secs = benchmark_start.elapsed().as_secs_f64();

	// Print backward-compatible statistics
	print_statistics(&all_round_stats);

	// Generate structured report.
	let mut report = generate_report(
		&scenario_name,
		&started_at,
		total_duration_secs,
		eff_num_clients,
		eff_num_rounds,
		&messages_pattern,
		&all_round_stats,
	);

	// Print throughput summary
	info!(
		"Throughput: {:.1} stmts/sec, {:.1} bytes/sec, success_rate={:.4}, duration={:.1}s",
		report.throughput_stmts_per_sec,
		report.throughput_bytes_per_sec,
		report.success_rate,
		report.total_duration_secs
	);

	// Evaluate pass/fail thresholds
	check_thresholds(
		&mut report,
		args.max_latency_secs,
		args.min_success_rate,
		args.min_throughput,
	);

	if report.passed {
		info!("Benchmark PASSED");
	} else {
		for failure in &report.failures {
			warn!("Threshold violation: {failure}");
		}
		warn!("Benchmark FAILED");
	}

	// Write JSON report if requested.
	if let Some(ref path) = args.report_json {
		write_json_report(&report, path)?;
	}

	if !report.passed {
		return Err(anyhow!("Benchmark failed threshold checks: {:?}", report.failures));
	}

	Ok(())
}

fn chrono_now_iso8601() -> String {
	let duration = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap_or_default();
	let secs = duration.as_secs();
	let days = secs / 86400;
	let day_secs = secs % 86400;
	let hours = day_secs / 3600;
	let minutes = (day_secs % 3600) / 60;
	let seconds = day_secs % 60;

	// Calculate year/month/day from days since epoch (1970-01-01)
	let (year, month, day) = days_to_ymd(days);
	format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Converts days since Unix epoch to (year, month, day)
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
	// Algorithm from http://howardhinnant.github.io/date_algorithms.html
	let z = days + 719468;
	let era = z / 146097;
	let doe = z - era * 146097;
	let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
	let y = yoe + era * 400;
	let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
	let mp = (5 * doy + 2) / 153;
	let d = doy - (153 * mp + 2) / 5 + 1;
	let m = if mp < 10 { mp + 3 } else { mp - 9 };
	let y = if m <= 2 { y + 1 } else { y };
	(y, m, d)
}

fn log_effective_configuration(
	scenario_name: &str,
	rpc_endpoints: &[String],
	num_clients: u32,
	num_rounds: usize,
	interval_ms: u64,
	messages_pattern: &[(usize, usize)],
) {
	let endpoints = rpc_endpoints.join(", ");
	let pattern_str = messages_pattern
		.iter()
		.map(|(count, size)| format!("{count}x{size}B"))
		.collect::<Vec<_>>()
		.join(", ");
	let msgs_per_client = messages_per_client(messages_pattern);
	let total_stmts = num_clients as u64 * msgs_per_client as u64 * num_rounds as u64;
	info!(
		"Starting Statement Store Latency Benchmark: scenario={scenario_name} endpoints=[{endpoints}] clients={num_clients} rounds={num_rounds} interval={interval_ms}ms pattern=[{pattern_str}] total_stmts={total_stmts}"
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

#[cfg(test)]
mod tests {
	use super::*;

	fn make_round_stats(round: usize, sent: u32, received: u32, full_latency: f64) -> RoundStats {
		RoundStats {
			round,
			sent_count: sent,
			received_count: received,
			send_duration_secs: full_latency * 0.3,
			receive_duration_secs: full_latency * 0.7,
			full_latency_secs: full_latency,
		}
	}

	#[test]
	fn test_parse_scenario_params() {
		// Custom returns None (uses CLI args).
		assert!(resolve_scenario(&Scenario::Custom).is_none());

		// Non-custom scenarios return Some with expected values.
		let throughput = resolve_scenario(&Scenario::Throughput).unwrap();
		assert_eq!(throughput.num_clients, 500);
		assert_eq!(throughput.messages_pattern, "1:384");
		assert_eq!(throughput.num_rounds, 30);
		assert_eq!(throughput.interval_ms, 4_000);

		let volume = resolve_scenario(&Scenario::Volume).unwrap();
		assert_eq!(volume.num_clients, 2000);
		assert_eq!(volume.messages_pattern, "50:192");
		assert_eq!(volume.num_rounds, 10);
		assert_eq!(volume.statement_expiry_ms, 1_800_000);

		let burst = resolve_scenario(&Scenario::Burst).unwrap();
		assert_eq!(burst.num_clients, 200);
		assert_eq!(burst.messages_pattern, "5:1024");
		assert_eq!(burst.num_rounds, 120);

		let event = resolve_scenario(&Scenario::Event).unwrap();
		assert_eq!(event.num_clients, 1000);
		assert_eq!(event.messages_pattern, "3:384,1:1024,1:128");
		assert_eq!(event.num_rounds, 120);
		assert_eq!(event.statement_expiry_ms, 3_600_000);

		let cap_max = resolve_scenario(&Scenario::CapacityMax).unwrap();
		assert_eq!(cap_max.num_clients, 2000);
		assert_eq!(cap_max.messages_pattern, "100:192");
		assert_eq!(cap_max.num_rounds, 19);
	}

	#[test]
	fn test_generate_report() {
		// Simulate 10 clients x 5 msgs(512B) x 2 rounds.
		let pattern = vec![(5, 512)];
		let mut stats = Vec::new();
		for round in 1..=2 {
			for _ in 0..10 {
				stats.push(make_round_stats(round, 5, 5, 2.0));
			}
		}

		let report = generate_report("test", "2025-01-01T00:00:00Z", 10.0, 10, 2, &pattern, &stats);

		// 10 clients * 5 msgs * 2 rounds = 100 total sent.
		assert_eq!(report.total_sent, 100);
		assert_eq!(report.total_received, 100);
		assert_eq!(report.total_bytes_sent, 10 * 5 * 512 * 2);
		assert!((report.success_rate - 1.0).abs() < 1e-6);
		assert!((report.throughput_stmts_per_sec - 10.0).abs() < 1e-6);
		assert_eq!(report.rounds.len(), 2);
		assert!(report.passed);
		assert!(report.failures.is_empty());
	}

	#[test]
	fn test_check_thresholds_pass() {
		let pattern = vec![(5, 512)];
		let stats: Vec<RoundStats> = (0..10).map(|_| make_round_stats(1, 5, 5, 2.0)).collect();

		let mut report =
			generate_report("test", "2025-01-01T00:00:00Z", 10.0, 10, 1, &pattern, &stats);

		check_thresholds(&mut report, Some(5.0), Some(0.99), Some(1.0));
		assert!(report.passed);
		assert!(report.failures.is_empty());
	}

	#[test]
	fn test_check_thresholds_fail_latency() {
		let pattern = vec![(5, 512)];
		let stats: Vec<RoundStats> = (0..10).map(|_| make_round_stats(1, 5, 5, 10.0)).collect();

		let mut report =
			generate_report("test", "2025-01-01T00:00:00Z", 10.0, 10, 1, &pattern, &stats);

		// Max latency is 5s but actual max is 10s.
		check_thresholds(&mut report, Some(5.0), None, None);
		assert!(!report.passed);
		assert_eq!(report.failures.len(), 1);
		assert!(report.failures[0].contains("latency"));
	}

	#[test]
	fn test_check_thresholds_fail_success_rate() {
		let pattern = vec![(5, 512)];
		// Only 3 out of 5 received.
		let stats: Vec<RoundStats> = (0..10).map(|_| make_round_stats(1, 5, 3, 2.0)).collect();

		let mut report =
			generate_report("test", "2025-01-01T00:00:00Z", 10.0, 10, 1, &pattern, &stats);

		check_thresholds(&mut report, None, Some(0.99), None);
		assert!(!report.passed);
		assert_eq!(report.failures.len(), 1);
		assert!(report.failures[0].contains("success rate"));
	}

	#[test]
	fn test_check_thresholds_fail_throughput() {
		let pattern = vec![(5, 512)];
		let stats: Vec<RoundStats> = (0..10).map(|_| make_round_stats(1, 5, 5, 2.0)).collect();

		// 50 stmts in 10s = 5 stmts/sec. Require 100.
		let mut report =
			generate_report("test", "2025-01-01T00:00:00Z", 10.0, 10, 1, &pattern, &stats);

		check_thresholds(&mut report, None, None, Some(100.0));
		assert!(!report.passed);
		assert_eq!(report.failures.len(), 1);
		assert!(report.failures[0].contains("throughput"));
	}

	#[test]
	fn test_backward_compat_custom_scenario() {
		// Custom scenario returns None, so CLI args are used unchanged.
		let params = resolve_scenario(&Scenario::Custom);
		assert!(params.is_none());
	}

	#[test]
	fn test_days_to_ymd() {
		// 1970-01-01 = day 0.
		assert_eq!(days_to_ymd(0), (1970, 1, 1));
		// 2025-01-01 = day 20089.
		assert_eq!(days_to_ymd(20089), (2025, 1, 1));
		// 2000-02-29 (leap year) = day 11016.
		assert_eq!(days_to_ymd(11016), (2000, 2, 29));
	}
}
