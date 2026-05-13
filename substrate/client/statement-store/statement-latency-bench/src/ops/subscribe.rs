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

//! `subscribe` subcommand: per-node retrieval latency benchmark.
//!
//! For each endpoint, submits one statement, then opens `reads_per_node`
//! subscriptions filtered to that statement's topic. The latency reported is
//! the time from subscription-open to the first non-empty initial-dump batch.
//!
//! This is the closest read-equivalent the public RPC offers — `statement_get`
//! is intentionally not exposed (see `substrate/client/rpc-api/src/statement/mod.rs`).

use crate::ops::{
	common::{
		build_statement, calc_stats, derive_channel, derive_topic, drain_initial_batch,
		expiry_seconds_from_now, next_statement_batch, Clock, Stats, SystemClock,
	},
	rpc::StatementRpc,
};
use anyhow::Result;
use log::{info, warn};
use sp_core::{bounded_vec::BoundedVec, sr25519, ConstU32};
use sp_statement_store::{SubmitResult, Topic, TopicFilter};
use std::{
	sync::Arc,
	time::{Duration, Instant},
};

/// Static configuration for the `subscribe` subcommand.
#[derive(Clone)]
pub struct SubscribeConfig {
	pub reads_per_node: u32,
	pub message_size: usize,
	pub base_expiry_secs: u64,
	pub run_id: u64,
	pub settle_ms: u64,
	pub drain_timeout_ms: u64,
	/// If `Some`, the seed statement uses this topic and the read filter is
	/// scoped to it. Useful for measuring retrieval of statements that already
	/// exist in the store under a known topic.
	pub topic_override: Option<[u8; 32]>,
}

impl SubscribeConfig {
	pub fn validate(&self) -> Result<()> {
		anyhow::ensure!(self.reads_per_node > 0, "--reads-per-node must be > 0");
		anyhow::ensure!(self.message_size > 0, "--message-size must be > 0");
		anyhow::ensure!(self.base_expiry_secs > 0, "--base-expiry-secs must be > 0");
		anyhow::ensure!(self.drain_timeout_ms > 0, "--drain-timeout-ms must be > 0");
		Ok(())
	}
}

/// Outcome of the optional seed step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedStatus {
	/// No `--topic` was set, so we submitted a fresh seed statement under a
	/// derived (per-run-id) topic.
	Submitted,
	/// `--topic` was set, so we deliberately did NOT submit a seed. Reads
	/// will succeed if a matching statement is already in the store (or
	/// arrives live during the read window), and time out otherwise.
	NotSeeded,
	/// The seed submit was attempted but failed; reads were not run.
	Failed,
}

#[derive(Debug, Clone)]
pub struct SubscribeEndpointReport {
	pub endpoint: String,
	pub stats: Option<Stats>,
	pub successes: u32,
	pub failures: u32,
	pub first_error: Option<String>,
	pub seed_status: SeedStatus,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SubscribeReport {
	pub per_endpoint: Vec<SubscribeEndpointReport>,
}

fn topic_filter_from(topic: [u8; 32]) -> Result<TopicFilter> {
	let topics: BoundedVec<Topic, ConstU32<128>> = vec![Topic::from(topic)]
		.try_into()
		.map_err(|_| anyhow::anyhow!("Failed to build BoundedVec for topic filter"))?;
	Ok(TopicFilter::MatchAny(topics))
}

pub async fn run_subscribe(
	endpoints: &[(String, Arc<dyn StatementRpc>)],
	keypair: &sr25519::Pair,
	clock: &dyn Clock,
	config: &SubscribeConfig,
	tag_for_logging: &str,
) -> Result<SubscribeReport> {
	config.validate()?;

	let mut per_endpoint = Vec::with_capacity(endpoints.len());
	for (idx, (endpoint, rpc)) in endpoints.iter().enumerate() {
		let report =
			run_subscribe_on_endpoint(endpoint, rpc.as_ref(), keypair, clock, config, idx as u32)
				.await;
		log_endpoint_report(tag_for_logging, &report);
		per_endpoint.push(report);
	}

	Ok(SubscribeReport { per_endpoint })
}

async fn run_subscribe_on_endpoint(
	endpoint: &str,
	rpc: &dyn StatementRpc,
	keypair: &sr25519::Pair,
	clock: &dyn Clock,
	config: &SubscribeConfig,
	endpoint_idx: u32,
) -> SubscribeEndpointReport {
	let mut durations_secs = Vec::with_capacity(config.reads_per_node as usize);
	let mut successes = 0u32;
	let mut failures = 0u32;
	let mut first_error: Option<String> = None;

	let scope = format!("subscribe-{endpoint_idx}");
	let topic = config.topic_override.unwrap_or_else(|| derive_topic(config.run_id, &scope, 0));
	let channel = derive_channel(config.run_id, &scope, 0);
	let expiry_ts = expiry_seconds_from_now(clock, config.base_expiry_secs);

	// Step 1: conditionally seed. We only ever seed when no `--topic` was
	// provided; with a pinned topic the user wants to read whatever is (or
	// isn't) already on the node, so we deliberately leave the store
	// untouched and let the read step time out if there's no match.
	let seed_status = match ensure_seed(rpc, keypair, topic, channel, expiry_ts, config).await {
		Ok(status) => {
			match status {
				SeedStatus::Submitted => info!(
					"endpoint={endpoint} seeded with new statement on topic 0x{}",
					hex_short(&topic),
				),
				SeedStatus::NotSeeded => info!(
					"endpoint={endpoint} --topic 0x{} set; not seeding (reads time out if no matching statement is present)",
					hex_short(&topic),
				),
				SeedStatus::Failed => {},
			}
			status
		},
		Err(e) => {
			return SubscribeEndpointReport {
				endpoint: endpoint.to_string(),
				stats: None,
				successes: 0,
				failures: 1,
				first_error: Some(e.to_string()),
				seed_status: SeedStatus::Failed,
			}
		},
	};

	// Step 2: optionally wait for the store to settle, but only if we just
	// submitted — if the statement was already present, no settling is needed.
	if matches!(seed_status, SeedStatus::Submitted) && config.settle_ms > 0 {
		tokio::time::sleep(Duration::from_millis(config.settle_ms)).await;
	}

	// Step 3: read the statement back N times. Durations are recorded for
	// every attempt so the stats reflect failure latencies (e.g. drain
	// timeouts) as well as successes.
	for _ in 0..config.reads_per_node {
		let (outcome, elapsed) = run_single_read(rpc, topic, config).await;
		durations_secs.push(elapsed.as_secs_f64());
		match outcome {
			Ok(()) => successes += 1,
			Err(e) => {
				failures += 1;
				if first_error.is_none() {
					first_error = Some(e.to_string());
				}
			},
		}
	}

	SubscribeEndpointReport {
		endpoint: endpoint.to_string(),
		stats: calc_stats(durations_secs),
		successes,
		failures,
		first_error,
		seed_status,
	}
}

/// If the user pinned a topic, do nothing (the read step will succeed only
/// if a matching statement is already in the store or arrives live within
/// the drain timeout). Otherwise submit one seed statement under the
/// derived per-run topic.
async fn ensure_seed(
	rpc: &dyn StatementRpc,
	keypair: &sr25519::Pair,
	topic: [u8; 32],
	channel: [u8; 32],
	expiry_ts: u32,
	config: &SubscribeConfig,
) -> Result<SeedStatus> {
	if config.topic_override.is_some() {
		return Ok(SeedStatus::NotSeeded);
	}

	let statement =
		build_statement(keypair, topic, channel, expiry_ts, 0, vec![0u8; config.message_size]);
	match rpc.submit_statement(&statement).await {
		Ok(SubmitResult::New) | Ok(SubmitResult::Known) => Ok(SeedStatus::Submitted),
		Ok(other) => Err(anyhow::anyhow!("seed submit returned {other:?}")),
		Err(e) => Err(anyhow::anyhow!("seed submit failed: {e}")),
	}
}

fn hex_short(bytes: &[u8; 32]) -> String {
	let mut s = String::with_capacity(8);
	for b in &bytes[..4] {
		s.push_str(&format!("{b:02x}"));
	}
	s.push_str("..");
	s
}

async fn run_single_read(
	rpc: &dyn StatementRpc,
	topic: [u8; 32],
	config: &SubscribeConfig,
) -> (Result<()>, Duration) {
	let filter = match topic_filter_from(topic) {
		Ok(f) => f,
		Err(e) => return (Err(e), Duration::ZERO),
	};
	let t_start = Instant::now();
	let outcome: Result<()> = async {
		let mut stream = rpc.subscribe_topic(filter).await?;
		let drain_to = Duration::from_millis(config.drain_timeout_ms);
		let drained = drain_initial_batch(&mut stream, drain_to).await?;
		if drained > 0 {
			return Ok(());
		}
		// Initial dump was empty. Without `--topic` the seed should have been
		// there, so this is an immediate failure. With `--topic` we deliberately
		// did not seed, so wait the same drain window for a live event before
		// giving up — that's the "just timeout" behaviour the CLI promises.
		if config.topic_override.is_some() {
			let n = next_statement_batch(&mut stream, drain_to).await?;
			if n == 0 {
				anyhow::bail!("Subscription event delivered 0 statements");
			}
			Ok(())
		} else {
			anyhow::bail!("Initial dump returned 0 statements; expected the seed statement")
		}
	}
	.await;
	(outcome, t_start.elapsed())
}

fn log_endpoint_report(tag: &str, r: &SubscribeEndpointReport) {
	let err_suffix = r
		.first_error
		.as_ref()
		.map(|e| format!(" first_error=\"{e}\""))
		.unwrap_or_default();
	match &r.stats {
		Some(s) => {
			let line = format!(
				"{tag}subscribe endpoint={} ok={} fail={} min={:.4}s avg={:.4}s max={:.4}s n={} seed={:?}{}",
				r.endpoint,
				r.successes,
				r.failures,
				s.min,
				s.avg,
				s.max,
				s.count,
				r.seed_status,
				err_suffix,
			);
			if r.failures > 0 {
				warn!("{line}");
			} else {
				info!("{line}");
			}
		},
		None => warn!(
			"{tag}subscribe endpoint={} ok={} fail={} no samples (seed={:?}{})",
			r.endpoint, r.successes, r.failures, r.seed_status, err_suffix,
		),
	}
}

/// Convenience: run with the system clock.
pub async fn run_subscribe_with_system_clock(
	endpoints: &[(String, Arc<dyn StatementRpc>)],
	keypair: &sr25519::Pair,
	config: &SubscribeConfig,
	tag_for_logging: &str,
) -> Result<SubscribeReport> {
	run_subscribe(endpoints, keypair, &SystemClock, config, tag_for_logging).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ops::{common::FixedClock, rpc::MockRpc};
	use sp_core::Bytes;
	use sp_statement_store::StatementEvent;

	fn cfg(reads: u32) -> SubscribeConfig {
		SubscribeConfig {
			reads_per_node: reads,
			message_size: 32,
			base_expiry_secs: 600,
			run_id: 11,
			settle_ms: 0,
			drain_timeout_ms: 500,
			topic_override: None,
		}
	}

	fn one_statement_initial_dump() -> StatementEvent {
		StatementEvent::NewStatements { statements: vec![Bytes(vec![9, 9, 9])], remaining: Some(0) }
	}

	fn empty_initial_dump() -> StatementEvent {
		StatementEvent::NewStatements { statements: vec![], remaining: Some(0) }
	}

	fn make_mock() -> (String, Arc<dyn StatementRpc>, MockRpc) {
		let m = MockRpc::new();
		let dynm: Arc<dyn StatementRpc> = Arc::new(m.clone());
		("ep".to_string(), dynm, m)
	}

	#[tokio::test]
	async fn happy_path_one_seed_then_n_reads() {
		let (name, rpc, mock) = make_mock();
		// 4 reads → 4 subscriptions, each returning the seed statement in the
		// initial dump.
		for _ in 0..4 {
			mock.push_subscribe_events(vec![Ok(one_statement_initial_dump())]);
		}
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		let report = run_subscribe(&[(name, rpc)], &kp, &clock, &cfg(4), "").await.unwrap();
		let r = &report.per_endpoint[0];
		assert_eq!(r.seed_status, SeedStatus::Submitted);
		assert_eq!(r.successes, 4);
		assert_eq!(r.failures, 0);
		let stats = r.stats.expect("stats");
		assert_eq!(stats.count, 4);
		assert_eq!(mock.submit_count(), 1, "exactly one seed submission");
		assert_eq!(
			mock.captured_filters().len(),
			4,
			"derived topic skips the probe; one subscribe per read",
		);
	}

	#[tokio::test]
	async fn seed_submit_failure_skips_reads() {
		let (name, rpc, mock) = make_mock();
		mock.push_submit_result(Err("offline".into()));
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		let report = run_subscribe(&[(name, rpc.clone())], &kp, &clock, &cfg(3), "").await.unwrap();
		let r = &report.per_endpoint[0];
		assert_eq!(r.seed_status, SeedStatus::Failed);
		assert_eq!(r.successes, 0);
		assert_eq!(r.failures, 1);
		assert!(r.first_error.as_ref().unwrap().contains("offline"));
		assert_eq!(mock.captured_filters().len(), 0, "no reads attempted when seed fails");
	}

	#[tokio::test]
	async fn empty_initial_dump_is_recorded_as_failure() {
		let (name, rpc, mock) = make_mock();
		mock.push_subscribe_events(vec![Ok(empty_initial_dump())]);
		mock.push_subscribe_events(vec![Ok(one_statement_initial_dump())]);
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		let report = run_subscribe(&[(name, rpc)], &kp, &clock, &cfg(2), "").await.unwrap();
		let r = &report.per_endpoint[0];
		assert_eq!(r.successes, 1);
		assert_eq!(r.failures, 1);
		assert!(r.first_error.as_ref().unwrap().contains("Initial dump returned 0"));
	}

	#[tokio::test]
	async fn subscribe_error_during_read_is_recorded() {
		let (name, rpc, mock) = make_mock();
		mock.push_subscribe_error("no such filter");
		mock.push_subscribe_events(vec![Ok(one_statement_initial_dump())]);
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		let report = run_subscribe(&[(name, rpc)], &kp, &clock, &cfg(2), "").await.unwrap();
		let r = &report.per_endpoint[0];
		assert_eq!(r.successes, 1);
		assert_eq!(r.failures, 1);
	}

	#[tokio::test(start_paused = true)]
	async fn drain_timeout_is_recorded_as_failure() {
		let (name, rpc, mock) = make_mock();
		mock.push_subscribe_pending();
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		let mut config = cfg(1);
		config.drain_timeout_ms = 50;
		let endpoints = vec![(name, rpc)];
		let fut = run_subscribe(&endpoints, &kp, &clock, &config, "");
		tokio::pin!(fut);
		let (report, _) = tokio::join!(fut, async {
			tokio::time::advance(Duration::from_millis(200)).await;
		});
		let r = &report.unwrap().per_endpoint[0];
		assert_eq!(r.failures, 1);
		assert!(r.first_error.as_ref().unwrap().contains("Initial drain timed out"));
	}

	#[tokio::test]
	async fn topic_is_consistent_across_reads() {
		let (name, rpc, mock) = make_mock();
		for _ in 0..3 {
			mock.push_subscribe_events(vec![Ok(one_statement_initial_dump())]);
		}
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		run_subscribe(&[(name, rpc)], &kp, &clock, &cfg(3), "").await.unwrap();

		// All three filters should match the same topic (the seed statement's).
		let filters = mock.captured_filters();
		assert_eq!(filters.len(), 3);
		let topics: Vec<_> = filters
			.iter()
			.map(|f| match f {
				TopicFilter::MatchAny(ts) => ts.to_vec(),
				_ => panic!("expected MatchAny filter"),
			})
			.collect();
		assert!(topics.windows(2).all(|w| w[0] == w[1]), "topic must be the same across reads");
	}

	#[tokio::test]
	async fn multiple_endpoints_run_in_sequence() {
		let (a_name, a_rpc, a_mock) = make_mock();
		let (b_name, b_rpc, b_mock) = make_mock();
		for _ in 0..2 {
			a_mock.push_subscribe_events(vec![Ok(one_statement_initial_dump())]);
			b_mock.push_subscribe_events(vec![Ok(one_statement_initial_dump())]);
		}
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		let report = run_subscribe(&[(a_name, a_rpc), (b_name, b_rpc)], &kp, &clock, &cfg(2), "")
			.await
			.unwrap();
		assert_eq!(report.per_endpoint.len(), 2);
		assert_eq!(a_mock.submit_count(), 1);
		assert_eq!(b_mock.submit_count(), 1);
	}

	fn live_statement_event() -> StatementEvent {
		StatementEvent::NewStatements { statements: vec![Bytes(vec![1, 2, 3])], remaining: None }
	}

	#[tokio::test]
	async fn topic_override_never_seeds() {
		// With `--topic`, the bench must never submit anything: it only reads
		// what's already there (or what arrives during the drain window).
		let (name, rpc, mock) = make_mock();
		// Both reads happen to find a pre-existing statement in the initial dump.
		for _ in 0..2 {
			mock.push_subscribe_events(vec![Ok(one_statement_initial_dump())]);
		}
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		let mut config = cfg(2);
		let override_topic = [0x55u8; 32];
		config.topic_override = Some(override_topic);
		let report = run_subscribe(&[(name, rpc)], &kp, &clock, &config, "").await.unwrap();
		let r = &report.per_endpoint[0];
		assert_eq!(r.seed_status, SeedStatus::NotSeeded);
		assert_eq!(r.successes, 2);
		assert_eq!(r.failures, 0);
		assert_eq!(mock.submit_count(), 0, "must NOT submit when --topic is set");
		assert_eq!(mock.captured_filters().len(), 2, "no probe; just reads_per_node subscribes");

		// Every read filters on the override topic.
		for f in mock.captured_filters() {
			match f {
				TopicFilter::MatchAny(ts) => {
					assert_eq!(ts.len(), 1);
					assert_eq!(ts[0].0, override_topic);
				},
				other => panic!("expected MatchAny filter, got {other:?}"),
			}
		}
	}

	#[tokio::test(start_paused = true)]
	async fn topic_override_no_match_times_out() {
		// Empty initial dump + nothing live → read should time out, NOT
		// fabricate a seed and NOT instant-fail.
		let (name, rpc, mock) = make_mock();
		mock.push_subscribe_events_then_pending(vec![Ok(empty_initial_dump())]);
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		let mut config = cfg(1);
		config.drain_timeout_ms = 50;
		config.topic_override = Some([0xAAu8; 32]);
		let endpoints = vec![(name, rpc)];
		let fut = run_subscribe(&endpoints, &kp, &clock, &config, "");
		tokio::pin!(fut);
		let (report, _) = tokio::join!(fut, async {
			// Drain returns Some(0) immediately, then we wait drain_timeout_ms
			// for a live event; advance past that budget.
			tokio::time::advance(Duration::from_millis(200)).await;
		});
		let r = &report.unwrap().per_endpoint[0];
		assert_eq!(r.seed_status, SeedStatus::NotSeeded);
		assert_eq!(r.successes, 0);
		assert_eq!(r.failures, 1);
		assert!(
			r.first_error.as_ref().unwrap().contains("Timed out waiting"),
			"first_error: {:?}",
			r.first_error,
		);
		assert_eq!(mock.submit_count(), 0, "must NEVER seed when --topic is set");
	}

	#[tokio::test]
	async fn topic_override_live_arrival_succeeds() {
		// Initial dump empty, then a matching statement arrives live: success.
		let (name, rpc, mock) = make_mock();
		mock.push_subscribe_events(vec![Ok(empty_initial_dump()), Ok(live_statement_event())]);
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		let mut config = cfg(1);
		config.topic_override = Some([0xBBu8; 32]);
		let report = run_subscribe(&[(name, rpc)], &kp, &clock, &config, "").await.unwrap();
		let r = &report.per_endpoint[0];
		assert_eq!(r.seed_status, SeedStatus::NotSeeded);
		assert_eq!(r.successes, 1);
		assert_eq!(r.failures, 0);
		assert_eq!(mock.submit_count(), 0);
	}

	#[tokio::test]
	async fn topic_override_subscribe_error_recorded() {
		let (name, rpc, mock) = make_mock();
		mock.push_subscribe_error("server denied");
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		let mut config = cfg(1);
		config.topic_override = Some([0xCCu8; 32]);
		let report = run_subscribe(&[(name, rpc)], &kp, &clock, &config, "").await.unwrap();
		let r = &report.per_endpoint[0];
		// Seed step succeeded (NotSeeded) — the failure is on the read.
		assert_eq!(r.seed_status, SeedStatus::NotSeeded);
		assert_eq!(r.successes, 0);
		assert_eq!(r.failures, 1);
		assert!(r.first_error.as_ref().unwrap().contains("server denied"));
		assert_eq!(mock.submit_count(), 0);
	}

	#[tokio::test]
	async fn derived_topic_always_seeds_no_probe() {
		// Without `--topic`, the derived per-run-id topic is unique, so we
		// can safely always seed without probing. Mock should see 1 submit
		// and exactly `reads_per_node` subscribes (no extra probe call).
		let (name, rpc, mock) = make_mock();
		for _ in 0..3 {
			mock.push_subscribe_events(vec![Ok(one_statement_initial_dump())]);
		}
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(3_000_000);
		run_subscribe(&[(name, rpc)], &kp, &clock, &cfg(3), "").await.unwrap();
		assert_eq!(
			mock.captured_filters().len(),
			3,
			"no probe → exactly reads_per_node subscribes"
		);
		assert_eq!(mock.submit_count(), 1, "always seed when topic is derived");
	}

	#[tokio::test]
	async fn validate_rejects_zero_reads() {
		assert!(cfg(0).validate().is_err());
	}
}
