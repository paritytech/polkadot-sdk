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

//! `propagation` subcommand: per-pair submit→subscribe latency benchmark.
//!
//! For every (submit_endpoint, subscribe_endpoint) pair, opens a separate ws
//! connection on each side and measures the time from `statement_submit` to
//! the matching `NewStatements` event on the subscription. The caller is
//! responsible for constructing separate [`StatementRpc`] instances per side
//! so the two sides do not share a ws connection — the production entry point
//! does this in `ops_bench.rs::main`.

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

/// Static configuration for the `propagation` subcommand.
#[derive(Clone)]
pub struct PropagationConfig {
	pub iterations: u32,
	pub message_size: usize,
	pub base_expiry_secs: u64,
	pub run_id: u64,
	pub drain_timeout_ms: u64,
	pub receive_timeout_ms: u64,
	/// If `Some`, every iteration uses this exact topic instead of a derived
	/// per-iteration one. The initial subscription dump may then legitimately
	/// contain prior statements; the empty-dump assertion is relaxed in that
	/// case (see [`run_single_iteration`]).
	pub topic_override: Option<[u8; 32]>,
}

impl PropagationConfig {
	pub fn validate(&self) -> Result<()> {
		anyhow::ensure!(self.iterations > 0, "--iterations must be > 0");
		anyhow::ensure!(self.message_size > 0, "--message-size must be > 0");
		anyhow::ensure!(self.base_expiry_secs > 0, "--base-expiry-secs must be > 0");
		anyhow::ensure!(self.drain_timeout_ms > 0, "--drain-timeout-ms must be > 0");
		anyhow::ensure!(self.receive_timeout_ms > 0, "--receive-timeout-ms must be > 0");
		Ok(())
	}
}

/// Per-pair report. `propagation_stats` is total submit→receive latency;
/// `submit_stats` is the submit-only portion measured inside the same loop.
#[derive(Debug, Clone)]
pub struct PropagationPairReport {
	pub submit_endpoint: String,
	pub subscribe_endpoint: String,
	pub propagation_stats: Option<Stats>,
	pub submit_stats: Option<Stats>,
	pub successes: u32,
	pub failures: u32,
	pub first_error: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PropagationReport {
	pub per_pair: Vec<PropagationPairReport>,
}

fn topic_filter_from(topic: [u8; 32]) -> Result<TopicFilter> {
	let topics: BoundedVec<Topic, ConstU32<128>> = vec![Topic::from(topic)]
		.try_into()
		.map_err(|_| anyhow::anyhow!("Failed to build BoundedVec for topic filter"))?;
	Ok(TopicFilter::MatchAny(topics))
}

/// Run the `propagation` benchmark across the Cartesian product of submit and
/// subscribe endpoints.
pub async fn run_propagation(
	submit_endpoints: &[(String, Arc<dyn StatementRpc>)],
	subscribe_endpoints: &[(String, Arc<dyn StatementRpc>)],
	keypair: &sr25519::Pair,
	clock: &dyn Clock,
	config: &PropagationConfig,
	tag_for_logging: &str,
) -> Result<PropagationReport> {
	config.validate()?;
	anyhow::ensure!(
		!submit_endpoints.is_empty(),
		"--submit-endpoints must list at least one endpoint",
	);
	anyhow::ensure!(
		!subscribe_endpoints.is_empty(),
		"--subscribe-endpoints must list at least one endpoint",
	);

	let mut per_pair = Vec::with_capacity(submit_endpoints.len() * subscribe_endpoints.len());
	for (pair_idx, (submit_name, submit_rpc)) in submit_endpoints.iter().enumerate() {
		for (sub_idx, (sub_name, sub_rpc)) in subscribe_endpoints.iter().enumerate() {
			let scope_idx = pair_idx * subscribe_endpoints.len() + sub_idx;
			let report = run_pair(
				submit_name,
				submit_rpc.as_ref(),
				sub_name,
				sub_rpc.as_ref(),
				keypair,
				clock,
				config,
				scope_idx as u32,
			)
			.await;
			log_pair_report(tag_for_logging, &report);
			per_pair.push(report);
		}
	}

	Ok(PropagationReport { per_pair })
}

#[allow(clippy::too_many_arguments)]
async fn run_pair(
	submit_endpoint: &str,
	submit_rpc: &dyn StatementRpc,
	subscribe_endpoint: &str,
	subscribe_rpc: &dyn StatementRpc,
	keypair: &sr25519::Pair,
	clock: &dyn Clock,
	config: &PropagationConfig,
	scope_idx: u32,
) -> PropagationPairReport {
	let mut prop_durations = Vec::with_capacity(config.iterations as usize);
	let mut submit_durations = Vec::with_capacity(config.iterations as usize);
	let mut successes = 0u32;
	let mut failures = 0u32;
	let mut first_error: Option<String> = None;

	let base_expiry = expiry_seconds_from_now(clock, config.base_expiry_secs);
	let scope = format!("prop-{scope_idx}");
	let data = vec![0u8; config.message_size];

	for i in 0..config.iterations {
		let topic = config.topic_override.unwrap_or_else(|| derive_topic(config.run_id, &scope, i));
		let channel = derive_channel(config.run_id, &scope, i);
		let expiry_ts = base_expiry.saturating_add(i);

		let timing = run_single_iteration(
			submit_rpc,
			subscribe_rpc,
			keypair,
			topic,
			channel,
			expiry_ts,
			i,
			data.clone(),
			config,
		)
		.await;

		// Record whatever timing data the iteration produced, regardless of
		// outcome: submit-only duration on submit/receive failure, full
		// propagation duration on success.
		if let Some(d) = timing.submit_dur {
			submit_durations.push(d.as_secs_f64());
		}
		if let Some(d) = timing.propagation_dur {
			prop_durations.push(d.as_secs_f64());
		}
		match timing.outcome {
			Ok(()) => successes += 1,
			Err(e) => {
				failures += 1;
				if first_error.is_none() {
					first_error = Some(e.to_string());
				}
			},
		}
	}

	PropagationPairReport {
		submit_endpoint: submit_endpoint.to_string(),
		subscribe_endpoint: subscribe_endpoint.to_string(),
		propagation_stats: calc_stats(prop_durations),
		submit_stats: calc_stats(submit_durations),
		successes,
		failures,
		first_error,
	}
}

/// Partial timing returned by [`run_single_iteration`]. Fields are populated
/// to whatever level the iteration reached before terminating, so a submit
/// failure still surfaces the submit-call duration in `submit_durations`.
struct IterationTiming {
	submit_dur: Option<Duration>,
	propagation_dur: Option<Duration>,
	outcome: Result<()>,
}

impl IterationTiming {
	fn err(e: anyhow::Error) -> Self {
		Self { submit_dur: None, propagation_dur: None, outcome: Err(e) }
	}
}

#[allow(clippy::too_many_arguments)]
async fn run_single_iteration(
	submit_rpc: &dyn StatementRpc,
	subscribe_rpc: &dyn StatementRpc,
	keypair: &sr25519::Pair,
	topic: [u8; 32],
	channel: [u8; 32],
	expiry_ts: u32,
	seq: u32,
	data: Vec<u8>,
	config: &PropagationConfig,
) -> IterationTiming {
	let filter = match topic_filter_from(topic) {
		Ok(f) => f,
		Err(e) => return IterationTiming::err(e),
	};
	let mut stream = match subscribe_rpc.subscribe_topic(filter).await {
		Ok(s) => s,
		Err(e) => return IterationTiming::err(e),
	};

	// Drain the initial batch first so it doesn't count against the propagation
	// timer. With a derived per-iteration topic the dump must be empty (run-id
	// namespacing guarantees no prior matches); with a `--topic` override the
	// dump may legitimately contain prior matching statements, so we simply
	// consume them and proceed.
	let drained = match drain_initial_batch(
		&mut stream,
		Duration::from_millis(config.drain_timeout_ms),
	)
	.await
	{
		Ok(d) => d,
		Err(e) => return IterationTiming::err(e),
	};
	if config.topic_override.is_none() && drained != 0 {
		return IterationTiming::err(anyhow::anyhow!(
			"Initial dump for unique topic was not empty (got {drained}); run-id namespacing failed",
		));
	}

	let statement = build_statement(keypair, topic, channel, expiry_ts, seq, data);

	let t_start = Instant::now();
	let submit_result = submit_rpc.submit_statement(&statement).await;
	let submit_dur = Some(t_start.elapsed());
	match submit_result {
		Ok(SubmitResult::New) | Ok(SubmitResult::Known) => {},
		Ok(other) => {
			return IterationTiming {
				submit_dur,
				propagation_dur: None,
				outcome: Err(anyhow::anyhow!("submit returned {other:?}")),
			}
		},
		Err(e) => return IterationTiming { submit_dur, propagation_dur: None, outcome: Err(e) },
	}

	let received =
		match next_statement_batch(&mut stream, Duration::from_millis(config.receive_timeout_ms))
			.await
		{
			Ok(r) => r,
			Err(e) => {
				return IterationTiming { submit_dur, propagation_dur: None, outcome: Err(e) }
			},
		};
	if received == 0 {
		return IterationTiming {
			submit_dur,
			propagation_dur: None,
			outcome: Err(anyhow::anyhow!("Subscription event delivered 0 statements")),
		};
	}
	let propagation_dur = Some(t_start.elapsed());

	IterationTiming { submit_dur, propagation_dur, outcome: Ok(()) }
}

fn log_pair_report(tag: &str, r: &PropagationPairReport) {
	let err_suffix = r
		.first_error
		.as_ref()
		.map(|e| format!(" first_error=\"{e}\""))
		.unwrap_or_default();

	let prop_part = match &r.propagation_stats {
		Some(p) => format!(
			"prop_min={:.4}s prop_avg={:.4}s prop_max={:.4}s n={}",
			p.min, p.avg, p.max, p.count,
		),
		None => "prop=none".to_string(),
	};
	let submit_part = match &r.submit_stats {
		Some(s) => format!(
			"submit_min={:.4}s submit_avg={:.4}s submit_max={:.4}s submit_n={}",
			s.min, s.avg, s.max, s.count,
		),
		None => "submit=none".to_string(),
	};
	let line = format!(
		"{tag}propagation submit_endpoint={} subscribe_endpoint={} ok={} fail={} {} {}{}",
		r.submit_endpoint,
		r.subscribe_endpoint,
		r.successes,
		r.failures,
		prop_part,
		submit_part,
		err_suffix,
	);
	if r.failures > 0 {
		warn!("{line}");
	} else {
		info!("{line}");
	}
}

/// Convenience: run with the system clock.
pub async fn run_propagation_with_system_clock(
	submit_endpoints: &[(String, Arc<dyn StatementRpc>)],
	subscribe_endpoints: &[(String, Arc<dyn StatementRpc>)],
	keypair: &sr25519::Pair,
	config: &PropagationConfig,
	tag_for_logging: &str,
) -> Result<PropagationReport> {
	run_propagation(
		submit_endpoints,
		subscribe_endpoints,
		keypair,
		&SystemClock,
		config,
		tag_for_logging,
	)
	.await
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ops::{common::FixedClock, rpc::MockRpc};
	use sp_core::Bytes;
	use sp_statement_store::StatementEvent;
	use std::time::Duration;

	fn cfg(iterations: u32) -> PropagationConfig {
		PropagationConfig {
			iterations,
			message_size: 32,
			base_expiry_secs: 600,
			run_id: 7,
			drain_timeout_ms: 500,
			receive_timeout_ms: 500,
			topic_override: None,
		}
	}

	fn empty_dump() -> StatementEvent {
		StatementEvent::NewStatements { statements: vec![], remaining: Some(0) }
	}

	fn one_statement_event() -> StatementEvent {
		StatementEvent::NewStatements { statements: vec![Bytes(vec![1, 2, 3])], remaining: None }
	}

	fn mock_pair() -> (String, Arc<dyn StatementRpc>, MockRpc) {
		let m = MockRpc::new();
		let dynm: Arc<dyn StatementRpc> = Arc::new(m.clone());
		("ep".to_string(), dynm, m)
	}

	#[tokio::test]
	async fn pair_enumeration_is_cartesian_product() {
		let (a_name, a_rpc, a_mock) = mock_pair();
		let (b_name, b_rpc, b_mock) = mock_pair();
		let (c_name, c_rpc, c_mock) = mock_pair();
		let (d_name, d_rpc, d_mock) = mock_pair();

		// Prepare 2 iterations × 2 subscribers each = 4 subscriptions per submit endpoint
		// → 4 subscriptions per subscribe-side mock instance overall.
		for mock in [&c_mock, &d_mock] {
			for _ in 0..(2 * 2) {
				mock.push_subscribe_events(vec![Ok(empty_dump()), Ok(one_statement_event())]);
			}
		}
		let _ = (&a_mock, &b_mock); // submit-side mocks just record submissions

		let submit_eps = vec![(a_name, a_rpc), (b_name, b_rpc)];
		let subscribe_eps = vec![(c_name, c_rpc), (d_name, d_rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(2_000_000);

		let report = run_propagation(&submit_eps, &subscribe_eps, &kp, &clock, &cfg(2), "")
			.await
			.unwrap();

		assert_eq!(report.per_pair.len(), 4);
		// All four pair-keys present
		let mut keys: Vec<_> = report
			.per_pair
			.iter()
			.map(|p| (p.submit_endpoint.clone(), p.subscribe_endpoint.clone()))
			.collect();
		keys.sort();
		assert_eq!(
			keys,
			vec![
				("ep".into(), "ep".into()),
				("ep".into(), "ep".into()),
				("ep".into(), "ep".into()),
				("ep".into(), "ep".into()),
			],
		);

		// Each pair did 2 iterations → 4 submits per submit-mock, 4 subscribes per sub-mock
		assert_eq!(a_mock.submit_count() + b_mock.submit_count(), 8);
		assert_eq!(c_mock.captured_filters().len() + d_mock.captured_filters().len(), 8);
	}

	#[tokio::test]
	async fn submit_side_never_subscribes_and_vice_versa() {
		let (sub_name, sub_rpc, sub_mock) = mock_pair();
		let (rec_name, rec_rpc, rec_mock) = mock_pair();
		// 3 iterations → 3 subscribes on rec_mock
		for _ in 0..3 {
			rec_mock.push_subscribe_events(vec![Ok(empty_dump()), Ok(one_statement_event())]);
		}

		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(2_000_000);
		let _ = run_propagation(
			&[(sub_name, sub_rpc)],
			&[(rec_name, rec_rpc)],
			&kp,
			&clock,
			&cfg(3),
			"",
		)
		.await
		.unwrap();

		assert_eq!(sub_mock.submit_count(), 3);
		assert_eq!(sub_mock.captured_filters().len(), 0, "submit-side must never subscribe");
		assert_eq!(rec_mock.submit_count(), 0, "subscribe-side must never submit");
		assert_eq!(rec_mock.captured_filters().len(), 3);
	}

	#[tokio::test]
	async fn drain_excludes_initial_dump_from_timer() {
		// Two events in the initial dump (remaining=Some(1), then remaining=Some(0))
		// followed by the post-submit live event. Drain must consume the first two
		// before the propagation timer starts.
		let (sub_name, sub_rpc, _sub_mock) = mock_pair();
		let (rec_name, rec_rpc, rec_mock) = mock_pair();
		rec_mock.push_subscribe_events(vec![
			Ok(StatementEvent::NewStatements {
				statements: vec![Bytes(vec![1])],
				remaining: Some(1),
			}),
			Ok(StatementEvent::NewStatements {
				statements: vec![Bytes(vec![2])],
				remaining: Some(0),
			}),
			Ok(one_statement_event()),
		]);
		// drain_initial_batch should fail because we expect an empty initial dump
		// (the bench namespaces topics by run_id so the dump must be empty).
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(2_000_000);
		let report = run_propagation(
			&[(sub_name, sub_rpc)],
			&[(rec_name, rec_rpc)],
			&kp,
			&clock,
			&cfg(1),
			"",
		)
		.await
		.unwrap();
		let r = &report.per_pair[0];
		assert_eq!(r.successes, 0);
		assert_eq!(r.failures, 1);
		assert!(r.first_error.as_ref().unwrap().contains("run-id namespacing failed"));
	}

	#[tokio::test]
	async fn happy_path_records_propagation_and_submit_stats() {
		let (sub_name, sub_rpc, _sub_mock) = mock_pair();
		let (rec_name, rec_rpc, rec_mock) = mock_pair();
		for _ in 0..3 {
			rec_mock.push_subscribe_events(vec![Ok(empty_dump()), Ok(one_statement_event())]);
		}
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(2_000_000);
		let report = run_propagation(
			&[(sub_name, sub_rpc)],
			&[(rec_name, rec_rpc)],
			&kp,
			&clock,
			&cfg(3),
			"",
		)
		.await
		.unwrap();
		let r = &report.per_pair[0];
		assert_eq!(r.successes, 3);
		assert_eq!(r.failures, 0);
		assert_eq!(r.propagation_stats.unwrap().count, 3);
		assert_eq!(r.submit_stats.unwrap().count, 3);
	}

	#[tokio::test(start_paused = true)]
	async fn receive_timeout_records_failure() {
		let (sub_name, sub_rpc, _sub_mock) = mock_pair();
		let (rec_name, rec_rpc, rec_mock) = mock_pair();
		// Initial drain succeeds (empty), but the live stream then never delivers.
		// The receive_timeout_ms should fire.
		rec_mock.push_subscribe_events_then_pending(vec![Ok(empty_dump())]);
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(2_000_000);
		let mut config = cfg(1);
		config.receive_timeout_ms = 50;
		let submit_eps = vec![(sub_name, sub_rpc)];
		let sub_eps = vec![(rec_name, rec_rpc)];
		let fut = run_propagation(&submit_eps, &sub_eps, &kp, &clock, &config, "");
		tokio::pin!(fut);
		let (report, _) = tokio::join!(fut, async {
			tokio::time::advance(Duration::from_millis(200)).await;
		});
		let report = report.unwrap();
		let r = &report.per_pair[0];
		assert_eq!(r.failures, 1);
		assert!(r.first_error.as_ref().unwrap().contains("Timed out"));
	}

	#[tokio::test(start_paused = true)]
	async fn drain_timeout_records_failure() {
		let (sub_name, sub_rpc, _sub_mock) = mock_pair();
		let (rec_name, rec_rpc, rec_mock) = mock_pair();
		// Subscription never delivers anything → drain times out.
		rec_mock.push_subscribe_pending();
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(2_000_000);
		let mut config = cfg(1);
		config.drain_timeout_ms = 50;
		let submit_eps = vec![(sub_name, sub_rpc)];
		let sub_eps = vec![(rec_name, rec_rpc)];
		let fut = run_propagation(&submit_eps, &sub_eps, &kp, &clock, &config, "");
		tokio::pin!(fut);
		let (report, _) = tokio::join!(fut, async {
			tokio::time::advance(Duration::from_millis(200)).await;
		});
		let r = &report.unwrap().per_pair[0];
		assert_eq!(r.failures, 1);
		assert!(r.first_error.as_ref().unwrap().contains("Initial drain timed out"));
	}

	#[tokio::test]
	async fn subscribe_error_is_recorded() {
		let (sub_name, sub_rpc, _sub_mock) = mock_pair();
		let (rec_name, rec_rpc, rec_mock) = mock_pair();
		rec_mock.push_subscribe_error("server denied");
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(2_000_000);
		let report = run_propagation(
			&[(sub_name, sub_rpc)],
			&[(rec_name, rec_rpc)],
			&kp,
			&clock,
			&cfg(1),
			"",
		)
		.await
		.unwrap();
		let r = &report.per_pair[0];
		assert_eq!(r.failures, 1);
		assert!(r.first_error.as_ref().unwrap().contains("server denied"));
	}

	#[tokio::test]
	async fn topic_override_tolerates_non_empty_initial_dump() {
		let (sub_name, sub_rpc, _sub_mock) = mock_pair();
		let (rec_name, rec_rpc, rec_mock) = mock_pair();
		// Subscription returns a non-empty initial dump (prior statements with
		// the same topic), then the live event matching our submission.
		rec_mock.push_subscribe_events(vec![
			Ok(StatementEvent::NewStatements {
				statements: vec![Bytes(vec![7, 8]), Bytes(vec![9])],
				remaining: Some(0),
			}),
			Ok(one_statement_event()),
		]);
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(2_000_000);
		let mut config = cfg(1);
		config.topic_override = Some([0xCDu8; 32]);
		let report = run_propagation(
			&[(sub_name, sub_rpc)],
			&[(rec_name, rec_rpc)],
			&kp,
			&clock,
			&config,
			"",
		)
		.await
		.unwrap();
		let r = &report.per_pair[0];
		assert_eq!(r.successes, 1, "non-empty initial dump must be tolerated when override is set");
		assert_eq!(r.failures, 0);
	}

	#[tokio::test]
	async fn topic_override_is_used_in_filter_and_statement() {
		let (sub_name, sub_rpc, sub_mock) = mock_pair();
		let (rec_name, rec_rpc, rec_mock) = mock_pair();
		rec_mock.push_subscribe_events(vec![Ok(empty_dump()), Ok(one_statement_event())]);
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(2_000_000);
		let mut config = cfg(1);
		let override_topic = [0xEFu8; 32];
		config.topic_override = Some(override_topic);
		run_propagation(&[(sub_name, sub_rpc)], &[(rec_name, rec_rpc)], &kp, &clock, &config, "")
			.await
			.unwrap();

		// Submit-side: the one submitted statement uses the override topic.
		let submitted = sub_mock.submitted();
		assert_eq!(submitted.len(), 1);
		assert_eq!(submitted[0].topic(0).map(|t| t.0), Some(override_topic));

		// Subscribe-side: the filter contains the override topic.
		let filters = rec_mock.captured_filters();
		assert_eq!(filters.len(), 1);
		match &filters[0] {
			TopicFilter::MatchAny(ts) => {
				assert_eq!(ts.len(), 1);
				assert_eq!(ts[0].0, override_topic);
			},
			other => panic!("expected MatchAny filter, got {other:?}"),
		}
	}

	#[tokio::test]
	async fn validate_rejects_zero_iterations() {
		assert!(cfg(0).validate().is_err());
	}

	#[tokio::test]
	async fn empty_endpoint_lists_error() {
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(2_000_000);
		let r = run_propagation(&[], &[], &kp, &clock, &cfg(1), "").await;
		assert!(r.is_err());
	}
}
