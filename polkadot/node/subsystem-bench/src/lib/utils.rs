// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Test utils

use crate::usage::BenchmarkUsage;
use std::io::{stdout, Write};

pub struct WarmUpOptions<'a> {
	/// The maximum number of runs considered for marming up.
	pub warm_up: usize,
	/// The number of runs considered for benchmarking.
	pub bench: usize,
	/// The difference in CPU usage between runs considered as normal
	pub precision: f64,
	/// The subsystems whose CPU usage is checked during warm-up cycles
	pub subsystems: &'a [&'a str],
}

impl<'a> WarmUpOptions<'a> {
	pub fn new(subsystems: &'a [&'a str]) -> Self {
		Self { warm_up: 100, bench: 3, precision: 0.02, subsystems }
	}
}

pub fn warm_up_and_benchmark(
	options: WarmUpOptions,
	run: impl Fn() -> BenchmarkUsage,
) -> Result<BenchmarkUsage, String> {
	println!("Warming up...");
	let mut usages = Vec::with_capacity(options.bench);

	for n in 1..=options.warm_up {
		let curr = run();
		if let Some(prev) = usages.last() {
			let diffs = options
				.subsystems
				.iter()
				.map(|&v| {
					curr.cpu_usage_diff(prev, v)
						.ok_or(format!("{} not found in benchmark {:?}", v, prev))
				})
				.collect::<Result<Vec<f64>, String>>()?;
			if !diffs.iter().all(|&v| v < options.precision) {
				usages.clear();
			}
		}
		usages.push(curr);
		print!("\r{}%", n * 100 / options.warm_up);
		if usages.len() == options.bench {
			println!("\rTook {} runs to warm up", n.saturating_sub(options.bench));
			break;
		}
		stdout().flush().unwrap();
	}

	if usages.len() != options.bench {
		println!("Didn't warm up after {} runs", options.warm_up);
		return Err("Can't warm up".to_string())
	}

	Ok(BenchmarkUsage::average(&usages))
}
