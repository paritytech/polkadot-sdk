// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Deterministic smoke runner for the PoV estimation fuzzer.
//!
//! Environment variables for `random_smoke`:
//! - `POV_FUZZ_ITERS`: number of scripts (default 300).
//! - `POV_FUZZ_SEED`: base seed, decimal or `0x` hex (default fixed). Script `i` uses seed `base +
//!   i`, so `POV_FUZZ_SEED=<failing seed> POV_FUZZ_ITERS=1` replays one failure.
//! - `POV_FUZZ_KEEP_GOING=1`: do not stop at the first failure, print a summary at the end.

use cumulus_proof_size_hostfunction_fuzz::{
	op_count, pinned_scripts, script_from_seed, shrink, try_run_script, Failure, Mode, Report,
};
use std::{
	collections::BTreeMap,
	panic::{self, AssertUnwindSafe},
};

fn env_u64(name: &str, default: u64) -> u64 {
	std::env::var(name)
		.ok()
		.and_then(|v| match v.strip_prefix("0x") {
			Some(hex) => u64::from_str_radix(hex, 16).ok(),
			None => v.parse().ok(),
		})
		.unwrap_or(default)
}

#[derive(Default)]
struct Stats {
	runs: usize,
	executive: usize,
	blocks: usize,
	estimates: usize,
	over_sum: i64,
	over_max: i64,
	over_min: i64,
	over_nonzero: usize,
	e_last_sum: u64,
	truth_sum: u64,
}

impl Stats {
	fn add(&mut self, mode: Mode, report: &Report) {
		self.runs += 1;
		if mode == Mode::Executive {
			self.executive += 1;
		}
		for block in &report.blocks {
			let over = block.overestimation();
			if self.blocks == 0 {
				self.over_max = over;
				self.over_min = over;
			}
			self.blocks += 1;
			self.estimates += block.estimates;
			self.over_sum += over;
			self.over_max = self.over_max.max(over);
			self.over_min = self.over_min.min(over);
			self.over_nonzero += (over != 0) as usize;
			self.e_last_sum += block.e_last;
			self.truth_sum += block.truth;
		}
	}

	fn print(&self) {
		let blocks = self.blocks.max(1);
		println!(
			"stats: {} passing scripts ({} executive, {} free), {} blocks, {} estimations",
			self.runs,
			self.executive,
			self.runs - self.executive,
			self.blocks,
			self.estimates,
		);
		println!(
			"overestimation e_last - truth per block: mean {:.1} B, max {} B, min {} B, \
			 non-zero in {}/{} blocks; mean e_last {} B, mean truth {} B",
			self.over_sum as f64 / blocks as f64,
			self.over_max,
			self.over_min,
			self.over_nonzero,
			self.blocks,
			self.e_last_sum / blocks as u64,
			self.truth_sum / blocks as u64,
		);
	}
}

#[test]
fn pinned_reproductions() {
	let mut failed = Vec::new();
	for (name, script) in pinned_scripts() {
		match panic::catch_unwind(AssertUnwindSafe(|| try_run_script(&script))) {
			Ok(Ok(report)) => println!("pinned {name}: ok {:?}", report.blocks),
			Ok(Err(failure)) => {
				println!("pinned {name}: FAILED: {failure}");
				failed.push(name);
			},
			Err(_) => {
				println!("pinned {name}: FAILED: panicked");
				failed.push(name);
			},
		}
	}
	assert!(failed.is_empty(), "{} pinned scripts failed: {failed:?}", failed.len());
}

#[test]
fn random_smoke() {
	let iters = env_u64("POV_FUZZ_ITERS", 300);
	let base = env_u64("POV_FUZZ_SEED", 0x5eed_0f_10215);
	let keep_going = std::env::var("POV_FUZZ_KEEP_GOING").is_ok_and(|v| v == "1");

	let mut stats = Stats::default();
	let mut failures: Vec<(u64, Failure)> = Vec::new();
	for i in 0..iters {
		let seed = base.wrapping_add(i);
		let script = script_from_seed(seed);
		match try_run_script(&script) {
			Ok(report) => stats.add(script.config.mode, &report),
			Err(failure) => {
				println!("seed {seed:#x} (iteration {i}): {failure}");
				if !keep_going {
					report_shrunk(seed, script, true);
					stats.print();
					panic!("seed {seed:#x}: {failure}");
				}
				failures.push((seed, failure));
			},
		}
	}
	stats.print();

	if !failures.is_empty() {
		let mut by_kind: BTreeMap<String, Vec<u64>> = BTreeMap::new();
		for (seed, failure) in &failures {
			by_kind.entry(format!("{:?}", failure.kind)).or_default().push(*seed);
		}
		println!("{} of {iters} scripts failed:", failures.len());
		for (kind, seeds) in &by_kind {
			let seeds: Vec<_> = seeds.iter().map(|s| format!("{s:#x}")).collect();
			println!("  {kind}: {} ({})", seeds.len(), seeds.join(", "));
		}
		// Shrink the first failure of every kind.
		for kind in by_kind.keys() {
			let (seed, _) = failures
				.iter()
				.find(|(_, f)| &format!("{:?}", f.kind) == kind)
				.expect("kind was collected from failures; qed");
			report_shrunk(*seed, script_from_seed(*seed), false);
		}
		panic!("{} of {iters} scripts failed", failures.len());
	}
}

/// Shrinks and prints the script, pretty or on one line.
fn report_shrunk(seed: u64, script: cumulus_proof_size_hostfunction_fuzz::Script, pretty: bool) {
	let ops = op_count(&script);
	let shrunk = shrink(script);
	let failure = try_run_script(&shrunk).err();
	let failure = failure.as_ref().map_or_else(|| "no longer fails?".into(), |f| f.to_string());
	let shrunk_ops = op_count(&shrunk);
	if pretty {
		println!("shrunk seed {seed:#x} from {ops} to {shrunk_ops} ops: {failure}\n{shrunk:#?}");
	} else {
		println!("shrunk seed {seed:#x} from {ops} to {shrunk_ops} ops: {failure}\n{shrunk:?}");
	}
}
