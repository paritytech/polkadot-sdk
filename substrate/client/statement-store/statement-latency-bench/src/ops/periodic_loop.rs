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

//! `loop` subcommand: run `submit`, `propagation`, `subscribe` periodically.
//!
//! The loop body is decoupled from the OS signal so it can be unit-tested:
//! callers pass a `cancel` future that resolves when the loop should exit.
//! The production entry point passes `tokio::signal::ctrl_c().map(...)`.

use crate::ops::{
	common::{Clock, SystemClock},
	propagation::{self, PropagationConfig, PropagationReport},
	rpc::StatementRpc,
	submit::{self, SubmitConfig, SubmitReport},
	subscribe::{self, SubscribeConfig, SubscribeReport},
};
use anyhow::Result;
use log::info;
use sp_core::sr25519;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::time::Instant;

/// Static configuration for the `loop` subcommand.
pub struct LoopConfig {
	pub interval_secs: u64,
	pub max_iterations: Option<u32>,
	pub max_duration_secs: Option<u64>,
	pub submit_config: SubmitConfig,
	pub propagation_config: PropagationConfig,
	pub subscribe_config: SubscribeConfig,
}

impl LoopConfig {
	pub fn validate(&self) -> Result<()> {
		anyhow::ensure!(self.interval_secs > 0, "--interval-secs must be > 0");
		self.submit_config.validate()?;
		self.propagation_config.validate()?;
		self.subscribe_config.validate()?;
		Ok(())
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopStopReason {
	MaxIterations,
	MaxDuration,
	Cancelled,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct LoopReport {
	pub iterations_completed: u32,
	pub last_submit: Option<SubmitReport>,
	pub last_propagation: Option<PropagationReport>,
	pub last_subscribe: Option<SubscribeReport>,
	pub stopped_reason: LoopStopReason,
}

/// Run the loop until `max_iterations`, `max_duration_secs` or `cancel`
/// triggers. Sleeps `max(0, interval_secs - body_duration)` between iterations.
pub async fn run_loop<C: Future<Output = ()> + Send>(
	endpoints: &[(String, Arc<dyn StatementRpc>)],
	keypair: &sr25519::Pair,
	clock: &dyn Clock,
	config: LoopConfig,
	cancel: C,
) -> Result<LoopReport> {
	config.validate()?;
	anyhow::ensure!(!endpoints.is_empty(), "--rpc-endpoints must list at least one endpoint");

	let start = Instant::now();
	let mut iterations_completed = 0u32;
	let mut last_submit: Option<SubmitReport> = None;
	let mut last_propagation: Option<PropagationReport> = None;
	let mut last_subscribe: Option<SubscribeReport> = None;
	let stopped_reason;

	tokio::pin!(cancel);

	loop {
		if let Some(max) = config.max_iterations {
			if iterations_completed >= max {
				stopped_reason = LoopStopReason::MaxIterations;
				break;
			}
		}
		if let Some(max) = config.max_duration_secs {
			if start.elapsed() >= Duration::from_secs(max) {
				stopped_reason = LoopStopReason::MaxDuration;
				break;
			}
		}

		let iter_idx = iterations_completed + 1;
		let tag = format!("[loop #{iter_idx}] ");
		info!("{tag}starting iteration");
		let iter_start = Instant::now();

		// Each cycle gets a fresh run_id so the per-iteration topics derived
		// inside `submit`/`propagation`/`subscribe` don't collide with the
		// statements left over from the previous cycle. Without this, the
		// propagation subcommand's empty-initial-dump assertion fires from
		// cycle #2 onwards.
		let iter_run_id = rand::random::<u64>();
		let iter_submit_config =
			SubmitConfig { run_id: iter_run_id, ..config.submit_config.clone() };
		let iter_propagation_config =
			PropagationConfig { run_id: iter_run_id, ..config.propagation_config.clone() };
		let iter_subscribe_config =
			SubscribeConfig { run_id: iter_run_id, ..config.subscribe_config.clone() };

		let work = async {
			let s =
				submit::run_submit(endpoints, keypair, clock, &iter_submit_config, &tag).await?;
			let p = propagation::run_propagation(
				endpoints,
				endpoints,
				keypair,
				clock,
				&iter_propagation_config,
				&tag,
			)
			.await?;
			let r =
				subscribe::run_subscribe(endpoints, keypair, clock, &iter_subscribe_config, &tag)
					.await?;
			Ok::<_, anyhow::Error>((s, p, r))
		};

		// If cancellation arrives during the iteration body, drop the body and exit.
		tokio::select! {
			biased;
			_ = &mut cancel => {
				stopped_reason = LoopStopReason::Cancelled;
				break;
			}
			result = work => {
				match result {
					Ok((s, p, r)) => {
						last_submit = Some(s);
						last_propagation = Some(p);
						last_subscribe = Some(r);
					}
					Err(e) => {
						log::warn!("{tag}iteration failed: {e}");
					}
				}
			}
		}

		iterations_completed += 1;

		// After the body, check stop conditions again before sleeping. This
		// avoids unnecessary sleeps when we've already hit a stop boundary.
		if let Some(max) = config.max_iterations {
			if iterations_completed >= max {
				stopped_reason = LoopStopReason::MaxIterations;
				break;
			}
		}
		if let Some(max) = config.max_duration_secs {
			if start.elapsed() >= Duration::from_secs(max) {
				stopped_reason = LoopStopReason::MaxDuration;
				break;
			}
		}

		let elapsed = iter_start.elapsed();
		let interval = Duration::from_secs(config.interval_secs);
		if elapsed < interval {
			let sleep_for = interval - elapsed;
			tokio::select! {
				biased;
				_ = &mut cancel => {
					stopped_reason = LoopStopReason::Cancelled;
					break;
				}
				_ = tokio::time::sleep(sleep_for) => {}
			}
		}
	}

	Ok(LoopReport {
		iterations_completed,
		last_submit,
		last_propagation,
		last_subscribe,
		stopped_reason,
	})
}

/// Production-flavour wrapper: cancels on Ctrl-C, uses the system clock.
pub async fn run_loop_with_ctrl_c(
	endpoints: &[(String, Arc<dyn StatementRpc>)],
	keypair: &sr25519::Pair,
	config: LoopConfig,
) -> Result<LoopReport> {
	let cancel =
		async {
			if let Err(e) = tokio::signal::ctrl_c().await {
				log::warn!("Failed to install Ctrl-C handler: {e}; loop will run until other stop conditions");
				// Park forever so the select! arm never fires by accident.
				std::future::pending::<()>().await;
			}
		};
	run_loop(endpoints, keypair, &SystemClock, config, cancel).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ops::{
		common::FixedClock, propagation::PropagationConfig, rpc::MockRpc, submit::SubmitConfig,
		subscribe::SubscribeConfig,
	};
	use sp_core::Bytes;
	use sp_statement_store::StatementEvent;
	use std::time::Duration;

	fn empty_dump() -> StatementEvent {
		StatementEvent::NewStatements { statements: vec![], remaining: Some(0) }
	}
	fn one_event() -> StatementEvent {
		StatementEvent::NewStatements { statements: vec![Bytes(vec![1])], remaining: None }
	}
	fn one_init_dump() -> StatementEvent {
		StatementEvent::NewStatements { statements: vec![Bytes(vec![1])], remaining: Some(0) }
	}

	fn loop_config(
		max_iterations: Option<u32>,
		max_duration_secs: Option<u64>,
		interval_secs: u64,
	) -> LoopConfig {
		LoopConfig {
			interval_secs,
			max_iterations,
			max_duration_secs,
			submit_config: SubmitConfig {
				iterations: 1,
				iteration_batch: 1,
				message_size: 16,
				base_expiry_secs: 60,
				run_id: 1,
				topic_override: None,
			},
			propagation_config: PropagationConfig {
				iterations: 1,
				message_size: 16,
				base_expiry_secs: 60,
				run_id: 1,
				drain_timeout_ms: 100,
				receive_timeout_ms: 100,
				topic_override: None,
			},
			subscribe_config: SubscribeConfig {
				reads_per_node: 1,
				message_size: 16,
				base_expiry_secs: 60,
				run_id: 1,
				settle_ms: 0,
				drain_timeout_ms: 100,
				topic_override: None,
			},
		}
	}

	/// Configure the mock so a single iteration's submit + propagation +
	/// subscribe scenarios all succeed.
	fn arm_mock_for_one_iteration(m: &MockRpc) {
		// `submit` subcommand: just records submits, default result is Ok(New).
		// `propagation` subcommand (1 iter, 1 endpoint × 1 endpoint = 1 pair):
		// One subscription with [empty initial dump, one live event].
		m.push_subscribe_events(vec![Ok(empty_dump()), Ok(one_event())]);
		// `subscribe` subcommand (1 read per endpoint): one subscription with the
		// seed statement in the initial dump.
		m.push_subscribe_events(vec![Ok(one_init_dump())]);
	}

	fn make_endpoint() -> (String, Arc<dyn StatementRpc>, MockRpc) {
		let m = MockRpc::new();
		let dynm: Arc<dyn StatementRpc> = Arc::new(m.clone());
		("ep".to_string(), dynm, m)
	}

	#[tokio::test(start_paused = true)]
	async fn honors_max_iterations() {
		let (name, rpc, mock) = make_endpoint();
		for _ in 0..3 {
			arm_mock_for_one_iteration(&mock);
		}
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(4_000_000);
		let config = loop_config(Some(3), None, 5);
		let never = std::future::pending::<()>();

		let fut = run_loop(&endpoints, &kp, &clock, config, never);
		tokio::pin!(fut);
		let (report, _) = tokio::join!(fut, async {
			tokio::time::advance(Duration::from_secs(20)).await;
		});
		let report = report.unwrap();
		assert_eq!(report.iterations_completed, 3);
		assert_eq!(report.stopped_reason, LoopStopReason::MaxIterations);
	}

	#[tokio::test(start_paused = true)]
	async fn honors_max_duration() {
		let (name, rpc, mock) = make_endpoint();
		// Plenty of mock state so we don't run out before the duration cap fires.
		for _ in 0..20 {
			arm_mock_for_one_iteration(&mock);
		}
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(4_000_000);
		// 10-second budget, 3-second interval. The exact iteration count
		// depends on how virtual-time advance and the loop's await points
		// interleave; we assert structural correctness instead: the loop must
		// terminate with `MaxDuration` after at least one iteration.
		let config = loop_config(None, Some(10), 3);
		let never = std::future::pending::<()>();

		let fut = run_loop(&endpoints, &kp, &clock, config, never);
		tokio::pin!(fut);
		let (report, _) = tokio::join!(fut, async {
			tokio::time::advance(Duration::from_secs(60)).await;
		});
		let report = report.unwrap();
		assert_eq!(report.stopped_reason, LoopStopReason::MaxDuration);
		assert!(
			report.iterations_completed >= 1,
			"expected at least one iteration, got {}",
			report.iterations_completed,
		);
	}

	#[tokio::test(start_paused = true)]
	async fn cancel_terminates_loop_mid_sleep() {
		let (name, rpc, mock) = make_endpoint();
		arm_mock_for_one_iteration(&mock);
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(4_000_000);
		// Large interval so the loop is sleeping when we cancel.
		let config = loop_config(Some(100), None, 60);
		let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
		let cancel = async move {
			let _ = cancel_rx.await;
		};

		let fut = run_loop(&endpoints, &kp, &clock, config, cancel);
		tokio::pin!(fut);
		// Advance enough so iteration 1 has completed and we are sleeping.
		let (report, _) = tokio::join!(fut, async {
			tokio::time::advance(Duration::from_secs(1)).await;
			let _ = cancel_tx.send(());
		});
		let report = report.unwrap();
		assert_eq!(report.iterations_completed, 1);
		assert_eq!(report.stopped_reason, LoopStopReason::Cancelled);
	}

	#[tokio::test(start_paused = true)]
	async fn body_longer_than_interval_skips_sleep() {
		// Make submit take 5s of virtual time per iteration, with 2s interval.
		// Loop should still progress (no negative sleep) and complete 2
		// iterations under max_iterations=2.
		let (name, rpc, mock) = make_endpoint();
		mock.set_submit_delay(Duration::from_secs(5));
		arm_mock_for_one_iteration(&mock);
		arm_mock_for_one_iteration(&mock);
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(4_000_000);
		let config = loop_config(Some(2), None, 2);
		let never = std::future::pending::<()>();

		let fut = run_loop(&endpoints, &kp, &clock, config, never);
		tokio::pin!(fut);
		let (report, _) = tokio::join!(fut, async {
			tokio::time::advance(Duration::from_secs(30)).await;
		});
		let report = report.unwrap();
		assert_eq!(report.iterations_completed, 2);
	}

	#[tokio::test(start_paused = true)]
	async fn reports_last_iteration_results() {
		let (name, rpc, mock) = make_endpoint();
		arm_mock_for_one_iteration(&mock);
		arm_mock_for_one_iteration(&mock);
		let endpoints = vec![(name, rpc)];
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(4_000_000);
		let config = loop_config(Some(2), None, 1);
		let never = std::future::pending::<()>();

		let fut = run_loop(&endpoints, &kp, &clock, config, never);
		tokio::pin!(fut);
		let (report, _) = tokio::join!(fut, async {
			tokio::time::advance(Duration::from_secs(10)).await;
		});
		let report = report.unwrap();
		assert!(report.last_submit.is_some());
		assert!(report.last_propagation.is_some());
		assert!(report.last_subscribe.is_some());
	}

	#[tokio::test]
	async fn empty_endpoints_errors() {
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(4_000_000);
		let never = std::future::pending::<()>();
		let r = run_loop(&[], &kp, &clock, loop_config(Some(1), None, 1), never).await;
		assert!(r.is_err());
	}

	#[tokio::test]
	async fn validate_rejects_zero_interval() {
		let kp = sc_statement_store::test_utils::get_keypair(0);
		let clock = FixedClock(4_000_000);
		let never = std::future::pending::<()>();
		let r = run_loop(
			&[("ep".to_string(), Arc::new(MockRpc::new()) as Arc<dyn StatementRpc>)],
			&kp,
			&clock,
			loop_config(Some(1), None, 0),
			never,
		)
		.await;
		assert!(r.is_err());
	}
}
