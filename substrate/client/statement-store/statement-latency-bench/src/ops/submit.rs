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

//! `submit` subcommand: per-node `statement_submit` duration benchmark.

use crate::ops::{
	common::{
		build_statement, calc_stats, derive_channel, derive_topic, expiry_seconds_from_now, Clock,
		Stats, SystemClock,
	},
	rpc::StatementRpc,
};
use anyhow::Result;
use log::{info, warn};
use sp_core::sr25519;
use sp_statement_store::SubmitResult;
use std::{sync::Arc, time::Instant};

/// Static configuration for the `submit` subcommand.
#[derive(Clone)]
pub struct SubmitConfig {
	/// Number of timing samples to take. Each sample is one parallel batch of
	/// `iteration_batch` statements; total submissions per endpoint =
	/// `iterations * iteration_batch`.
	pub iterations: u32,
	/// How many statements to submit in parallel per timing sample. `1`
	/// reproduces the sequential, one-submit-per-sample behaviour.
	pub iteration_batch: u32,
	pub message_size: usize,
	pub base_expiry_secs: u64,
	pub run_id: u64,
	/// If `Some`, every iteration uses this exact topic instead of a derived one.
	pub topic_override: Option<[u8; 32]>,
}

impl SubmitConfig {
	pub fn validate(&self) -> Result<()> {
		anyhow::ensure!(self.iterations > 0, "--iterations must be > 0");
		anyhow::ensure!(self.iteration_batch > 0, "--iteration-batch must be > 0");
		anyhow::ensure!(self.message_size > 0, "--message-size must be > 0");
		anyhow::ensure!(self.base_expiry_secs > 0, "--base-expiry-secs must be > 0");
		Ok(())
	}
}

/// Result of running the submit benchmark on a single endpoint.
#[derive(Debug, Clone)]
pub struct SubmitEndpointReport {
	pub endpoint: String,
	pub stats: Option<Stats>,
	pub successes: u32,
	pub failures: u32,
	pub first_error: Option<String>,
	/// Submissions per timing sample (matches [`SubmitConfig::iteration_batch`]).
	pub iteration_batch: u32,
}

/// Top-level result for the `submit` subcommand.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SubmitReport {
	pub per_endpoint: Vec<SubmitEndpointReport>,
}

/// Run the `submit` benchmark across the provided endpoints, sequentially.
///
/// `tag_for_logging` is included in log lines so this can be reused by the
/// `loop` subcommand to mark which iteration produced the stats.
pub async fn run_submit(
	endpoints: &[(String, Arc<dyn StatementRpc>)],
	keypair: &sr25519::Pair,
	clock: &dyn Clock,
	config: &SubmitConfig,
	tag_for_logging: &str,
) -> Result<SubmitReport> {
	config.validate()?;

	let mut per_endpoint = Vec::with_capacity(endpoints.len());
	for (endpoint, rpc) in endpoints {
		let report = run_submit_on_endpoint(endpoint, rpc.as_ref(), keypair, clock, config).await;
		log_endpoint_report(tag_for_logging, &report);
		per_endpoint.push(report);
	}

	Ok(SubmitReport { per_endpoint })
}

async fn run_submit_on_endpoint(
	endpoint: &str,
	rpc: &dyn StatementRpc,
	keypair: &sr25519::Pair,
	clock: &dyn Clock,
	config: &SubmitConfig,
) -> SubmitEndpointReport {
	let mut durations_secs = Vec::with_capacity(config.iterations as usize);
	let mut successes = 0u32;
	let mut failures = 0u32;
	let mut first_error: Option<String> = None;

	let base_expiry = expiry_seconds_from_now(clock, config.base_expiry_secs);
	let scope = format!("submit-{endpoint}");
	let data = vec![0u8; config.message_size];

	let batch_size = config.iteration_batch as usize;
	for batch_idx in 0..config.iterations {
		// Build all statements for this batch up-front so per-statement
		// construction cost is excluded from the batch timer.
		let mut statements = Vec::with_capacity(batch_size);
		for sub_idx in 0..config.iteration_batch {
			let global_idx = batch_idx * config.iteration_batch + sub_idx;
			let topic = config
				.topic_override
				.unwrap_or_else(|| derive_topic(config.run_id, &scope, global_idx));
			let channel = derive_channel(config.run_id, &scope, global_idx);
			let expiry_ts = base_expiry.saturating_add(global_idx);
			statements.push(build_statement(
				keypair,
				topic,
				channel,
				expiry_ts,
				global_idx,
				data.clone(),
			));
		}

		// Kick off all `batch_size` submits in parallel on the same ws
		// connection and wait for every one to come back. The wall-clock
		// from kick-off to all-completed is recorded as one sample, so
		// `n` in the stats == number of iterations (not submissions).
		// Per-attempt duration is also captured for both successes and
		// failures via `started.elapsed()` on the join.
		let started = Instant::now();
		let results: Vec<_> =
			futures::future::join_all(statements.iter().map(|s| rpc.submit_statement(s))).await;
		durations_secs.push(started.elapsed().as_secs_f64());

		for result in results {
			match result {
				Ok(SubmitResult::New) | Ok(SubmitResult::Known) => {
					successes += 1;
				},
				Ok(other) => {
					failures += 1;
					if first_error.is_none() {
						first_error = Some(format!("submit returned {other:?}"));
					}
				},
				Err(e) => {
					failures += 1;
					if first_error.is_none() {
						first_error = Some(e.to_string());
					}
				},
			}
		}
	}

	SubmitEndpointReport {
		endpoint: endpoint.to_string(),
		stats: calc_stats(durations_secs),
		successes,
		failures,
		first_error,
		iteration_batch: config.iteration_batch,
	}
}

fn log_endpoint_report(tag: &str, r: &SubmitEndpointReport) {
	let err_suffix = r
		.first_error
		.as_ref()
		.map(|e| format!(" first_error=\"{e}\""))
		.unwrap_or_default();
	let batch_suffix =
		if r.iteration_batch > 1 { format!(" batch={}", r.iteration_batch) } else { String::new() };
	match &r.stats {
		Some(s) => {
			let line = format!(
				"{tag}submit endpoint={} ok={} fail={} min={:.4}s avg={:.4}s max={:.4}s n={}{}{}",
				r.endpoint,
				r.successes,
				r.failures,
				s.min,
				s.avg,
				s.max,
				s.count,
				batch_suffix,
				err_suffix,
			);
			if r.failures > 0 {
				warn!("{line}");
			} else {
				info!("{line}");
			}
		},
		None => warn!(
			"{tag}submit endpoint={} ok={} fail={} no samples{}{}",
			r.endpoint, r.successes, r.failures, batch_suffix, err_suffix,
		),
	}
}

/// Convenience: run with the system clock.
pub async fn run_submit_with_system_clock(
	endpoints: &[(String, Arc<dyn StatementRpc>)],
	keypair: &sr25519::Pair,
	config: &SubmitConfig,
	tag_for_logging: &str,
) -> Result<SubmitReport> {
	run_submit(endpoints, keypair, &SystemClock, config, tag_for_logging).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ops::{common::FixedClock, rpc::MockRpc};
	use sp_statement_store::SubmitResult;
	use std::time::Duration;

	fn config(iterations: u32) -> SubmitConfig {
		SubmitConfig {
			iterations,
			iteration_batch: 1,
			message_size: 32,
			base_expiry_secs: 600,
			run_id: 123,
			topic_override: None,
		}
	}

	fn make_endpoint(name: &str) -> (String, Arc<dyn StatementRpc>, MockRpc) {
		let mock = MockRpc::new();
		let dyn_arc: Arc<dyn StatementRpc> = Arc::new(mock.clone());
		(name.to_string(), dyn_arc, mock)
	}

	#[tokio::test]
	async fn happy_path_records_iterations_per_endpoint() {
		let (a_name, a_rpc, a_mock) = make_endpoint("a");
		let (b_name, b_rpc, b_mock) = make_endpoint("b");
		let endpoints = vec![(a_name, a_rpc), (b_name, b_rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(1_000_000);

		let report = run_submit(&endpoints, &kp, &clock, &config(5), "").await.expect("run_submit");

		assert_eq!(report.per_endpoint.len(), 2);
		for r in &report.per_endpoint {
			assert_eq!(r.successes, 5);
			assert_eq!(r.failures, 0);
			let s = r.stats.expect("stats present");
			assert_eq!(s.count, 5);
		}
		assert_eq!(a_mock.submit_count(), 5);
		assert_eq!(b_mock.submit_count(), 5);
	}

	#[tokio::test]
	async fn channels_are_distinct_across_iterations() {
		let (name, rpc, mock) = make_endpoint("a");
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(1_000_000);

		run_submit(&endpoints, &kp, &clock, &config(10), "").await.unwrap();

		let statements = mock.submitted();
		let mut channels: Vec<_> = statements.iter().filter_map(|s| s.channel()).collect();
		channels.sort();
		channels.dedup();
		assert_eq!(channels.len(), 10, "all channels must be unique");
	}

	#[tokio::test]
	async fn expiry_strictly_increases_across_iterations() {
		let (name, rpc, mock) = make_endpoint("a");
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(1_000_000);

		run_submit(&endpoints, &kp, &clock, &config(5), "").await.unwrap();
		let statements = mock.submitted();
		let expiries: Vec<u64> = statements.iter().map(|s| s.expiry()).collect();
		for w in expiries.windows(2) {
			assert!(w[0] < w[1], "expiry must strictly increase: {:?}", expiries);
		}
	}

	#[tokio::test(start_paused = true)]
	async fn injected_submit_delay_is_visible_in_stats() {
		let (name, rpc, mock) = make_endpoint("a");
		mock.set_submit_delay(Duration::from_millis(50));
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(1_000_000);

		// With paused time, run_submit drives the simulated sleep through advance().
		let cfg = config(2);
		let fut = run_submit(&endpoints, &kp, &clock, &cfg, "");
		tokio::pin!(fut);
		let (report, _) = tokio::join!(fut, async {
			// Two iterations × 50ms = 100ms of virtual sleep.
			tokio::time::advance(Duration::from_millis(200)).await;
		});
		let report = report.unwrap();
		let stats = report.per_endpoint[0].stats.expect("stats");
		assert_eq!(stats.count, 2);
	}

	#[tokio::test]
	async fn submit_error_is_recorded_as_failure() {
		let (name, rpc, mock) = make_endpoint("a");
		mock.push_submit_result(Err("rpc died".into()));
		mock.push_submit_result(Ok(SubmitResult::New));
		mock.push_submit_result(Ok(SubmitResult::New));
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(1_000_000);

		let report = run_submit(&endpoints, &kp, &clock, &config(3), "").await.unwrap();
		let r = &report.per_endpoint[0];
		assert_eq!(r.failures, 1);
		assert_eq!(r.successes, 2);
		assert!(r.first_error.as_ref().unwrap().contains("rpc died"));
	}

	#[tokio::test]
	async fn submit_invalid_result_counts_as_failure() {
		let (name, rpc, mock) = make_endpoint("a");
		mock.set_default_submit_result(SubmitResult::Invalid(
			sp_statement_store::InvalidReason::NoProof,
		));
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(1_000_000);

		let report = run_submit(&endpoints, &kp, &clock, &config(2), "").await.unwrap();
		let r = &report.per_endpoint[0];
		assert_eq!(r.successes, 0);
		assert_eq!(r.failures, 2);
		// Durations are recorded for every attempt, including failures.
		let stats = r.stats.expect("stats present even when all attempts fail");
		assert_eq!(stats.count, 2);
	}

	#[tokio::test]
	async fn topic_override_is_used_for_every_iteration() {
		let (name, rpc, mock) = make_endpoint("a");
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(1_000_000);
		let override_topic = [0xABu8; 32];
		let mut cfg = config(4);
		cfg.topic_override = Some(override_topic);

		run_submit(&endpoints, &kp, &clock, &cfg, "").await.unwrap();

		let statements = mock.submitted();
		assert_eq!(statements.len(), 4);
		for s in &statements {
			assert_eq!(
				s.topic(0).map(|t| t.0),
				Some(override_topic),
				"every submitted statement must carry the override topic",
			);
		}
		// Channels still distinct so statements coexist in the store.
		let mut channels: Vec<_> = statements.iter().filter_map(|s| s.channel()).collect();
		channels.sort();
		channels.dedup();
		assert_eq!(channels.len(), 4);
	}

	#[tokio::test]
	async fn validate_rejects_zero_iterations() {
		let cfg = SubmitConfig {
			iterations: 0,
			iteration_batch: 1,
			message_size: 32,
			base_expiry_secs: 60,
			run_id: 1,
			topic_override: None,
		};
		assert!(cfg.validate().is_err());
	}

	#[tokio::test]
	async fn validate_rejects_zero_message_size() {
		let cfg = SubmitConfig {
			iterations: 1,
			iteration_batch: 1,
			message_size: 0,
			base_expiry_secs: 60,
			run_id: 1,
			topic_override: None,
		};
		assert!(cfg.validate().is_err());
	}

	#[tokio::test]
	async fn validate_rejects_zero_iteration_batch() {
		let cfg = SubmitConfig {
			iterations: 1,
			iteration_batch: 0,
			message_size: 32,
			base_expiry_secs: 60,
			run_id: 1,
			topic_override: None,
		};
		assert!(cfg.validate().is_err());
	}

	#[tokio::test]
	async fn iteration_batch_runs_n_submits_per_sample() {
		let (name, rpc, mock) = make_endpoint("a");
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(1_000_000);
		let cfg = SubmitConfig {
			iterations: 3,
			iteration_batch: 4,
			message_size: 32,
			base_expiry_secs: 600,
			run_id: 123,
			topic_override: None,
		};

		let report = run_submit(&endpoints, &kp, &clock, &cfg, "").await.unwrap();
		let r = &report.per_endpoint[0];
		assert_eq!(r.successes, 12, "3 batches * 4 submits each = 12 total submissions");
		assert_eq!(r.failures, 0);
		assert_eq!(r.stats.unwrap().count, 3, "one timing sample per batch (3 batches)");
		assert_eq!(r.iteration_batch, 4);
		assert_eq!(mock.submit_count(), 12);
	}

	#[tokio::test]
	async fn iteration_batch_uses_distinct_channels_across_batches() {
		let (name, rpc, mock) = make_endpoint("a");
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(1_000_000);
		let cfg = SubmitConfig {
			iterations: 2,
			iteration_batch: 3,
			message_size: 32,
			base_expiry_secs: 600,
			run_id: 123,
			topic_override: None,
		};

		run_submit(&endpoints, &kp, &clock, &cfg, "").await.unwrap();
		let statements = mock.submitted();
		assert_eq!(statements.len(), 6);
		let mut channels: Vec<_> = statements.iter().filter_map(|s| s.channel()).collect();
		channels.sort();
		channels.dedup();
		assert_eq!(channels.len(), 6, "every batched submit gets its own channel");
	}
}
