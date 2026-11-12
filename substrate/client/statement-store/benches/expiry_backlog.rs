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

//! Compares how long an expiry backlog larger than one pass' budget (10,000 statements) takes
//! to drain under the two scheduling policies of the background task.
//!
//! Before the fix, the task runs one `enforce_limits` pass per `ENFORCE_LIMITS_PERIOD` (31 s)
//! tick, and any statements left over wait for the next tick. After the fix, a pass that reports
//! a remaining backlog makes the task run the next pass at once.
//!
//! Both policies run the same passes. The bench runs the passes back to back on two identical
//! stores and adds the 31 s gaps to the "before" time instead of sleeping through them.
//!
//! The bench creates fresh stores in a temporary directory and needs no fixture. Two environment
//! variables tune it:
//! - `EXPIRY_BACKLOG` sets the backlog size (default 50,000).
//! - `EXPIRY_SUBMIT_MARGIN_SECS` sets how many seconds ahead the backlog expires. It must cover
//!   submitting the backlog to both stores (default 20).

#[allow(dead_code, unused_imports)]
#[path = "full_store/common.rs"]
mod common;

use common::*;
use sc_statement_store::Store;
use sp_statement_store::{Hash, StatementSource, StatementStore, SubmitResult};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Same value as `ENFORCE_LIMITS_PERIOD` in `src/lib.rs`.
const ENFORCE_LIMITS_PERIOD: Duration = Duration::from_secs(31);
/// Same value as `MAX_EXPIRY_STATEMENTS_PER_ITERATION` in `src/lib.rs`.
const PASS_BUDGET: usize = 10_000;

fn unix_secs() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("system clock after epoch; qed")
		.as_secs()
}

fn env_or(name: &str, default: u64) -> u64 {
	std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn submit_backlog(store: &Store, backlog: usize, expiry_secs: u64) -> Vec<Hash> {
	let keypair = sacrifice_keypair();
	let base = fresh_id_base();
	(0..backlog as u64)
		.map(|k| {
			let statement = create_statement(base + k, &[], 64, expiry_secs << 32, &keypair);
			let hash = statement.hash();
			let result = store.submit(statement, StatementSource::Local);
			assert!(
				matches!(result, SubmitResult::New),
				"backlog statement rejected: {:?}",
				result
			);
			hash
		})
		.collect()
}

fn assert_drained(store: &Store, hashes: &[Hash]) {
	let left = hashes.iter().filter(|h| store.has_statement(h)).count();
	assert_eq!(left, 0, "{} backlog statements were not reaped", left);
}

fn main() {
	let backlog = env_or("EXPIRY_BACKLOG", 50_000) as usize;
	let margin = env_or("EXPIRY_SUBMIT_MARGIN_SECS", 20);

	let dir = tempfile::tempdir().expect("temporary directory; qed");
	let before_store = open_store(&dir.path().join("before"), TestClient::generous());
	let after_store = open_store(&dir.path().join("after"), TestClient::generous());

	let expiry_secs = unix_secs() + margin;
	let started = Instant::now();
	let before_hashes = submit_backlog(&before_store, backlog, expiry_secs);
	let after_hashes = submit_backlog(&after_store, backlog, expiry_secs);
	println!("EXPIRY_BACKLOG submit_2x{}_secs={:.3}", backlog, started.elapsed().as_secs_f64());
	assert!(
		unix_secs() < expiry_secs,
		"submitting took longer than the margin; raise EXPIRY_SUBMIT_MARGIN_SECS"
	);
	// Wait until the backlog has expired by the stores' clock.
	while unix_secs() <= expiry_secs {
		std::thread::sleep(Duration::from_millis(200));
	}

	// Before the fix, one pass runs per tick. The bench reads the return value only to tell when
	// the backlog is gone. The old scheduler ignored it.
	let mut before_passes = Vec::new();
	loop {
		let started = Instant::now();
		let more_due = before_store.enforce_limits();
		before_passes.push(started.elapsed());
		if !more_due {
			break;
		}
	}
	assert_drained(&before_store, &before_hashes);
	// The first pass runs on the tick that finds the backlog; every further pass waits a period.
	let before_drain = ENFORCE_LIMITS_PERIOD * (before_passes.len() as u32 - 1) +
		*before_passes.last().expect("at least one pass ran; qed");

	// After the fix, the next pass starts at once while a backlog remains.
	let started = Instant::now();
	let mut after_passes = 1;
	while after_store.enforce_limits() {
		after_passes += 1;
	}
	let after_drain = started.elapsed();
	assert_drained(&after_store, &after_hashes);

	let busy: Duration = before_passes.iter().sum();
	let mean_pass = busy / before_passes.len() as u32;
	let rate = |d: Duration| backlog as f64 / d.as_secs_f64();
	let before_cap = PASS_BUDGET as f64 / ENFORCE_LIMITS_PERIOD.as_secs_f64();
	let after_cap = PASS_BUDGET as f64 / mean_pass.as_secs_f64();

	println!("EXPIRY_BACKLOG backlog={}", backlog);
	println!("EXPIRY_BACKLOG mean_pass_secs={:.3}", mean_pass.as_secs_f64());
	println!(
		"EXPIRY_BACKLOG before passes={} drain_secs={:.3} rate_per_sec={:.0}",
		before_passes.len(),
		before_drain.as_secs_f64(),
		rate(before_drain)
	);
	println!(
		"EXPIRY_BACKLOG after passes={} drain_secs={:.3} rate_per_sec={:.0}",
		after_passes,
		after_drain.as_secs_f64(),
		rate(after_drain)
	);
	println!(
		"EXPIRY_BACKLOG sustained_cap_per_sec before={:.0} after={:.0}",
		before_cap, after_cap
	);
}
