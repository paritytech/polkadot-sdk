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

//! A tool to compare subsystem benchmark outputs looking at CI regression.

use clap::Parser;
use color_eyre::eyre;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Parser)]
#[allow(missing_docs)]
struct GuardCli {
	/// A path to a current subsystem-bench output YAML
	#[clap(long)]
	pub current: String,

	/// A path to a reference subsystem-bench output YAML
	#[clap(long)]
	pub reference: String,

	/// A threshold in percents to find a regression
	#[clap(long)]
	pub threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkDiff {
	benchmark_name: String,
	network_usage: Vec<ResourceDiff>,
	cpu_usage: Vec<ResourceDiff>,
}

impl std::fmt::Display for BenchmarkDiff {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(
			f,
			"\n{}\n\n{}\n{}\n\n{}\n{}\n",
			self.benchmark_name.purple(),
			format!("{:<32}", "Total network usage, s").blue(),
			self.network_usage
				.iter()
				.map(|v| v.to_string())
				.collect::<Vec<String>>()
				.join("\n"),
			format!("{:<32}", "Total CPU usage, s").blue(),
			self.cpu_usage.iter().map(|v| v.to_string()).collect::<Vec<String>>().join("\n")
		)
	}
}

impl BenchmarkDiff {
	fn compare_with(&mut self, reference: Option<&Self>) {
		if let Some(reference) = reference {
			let reference_network_usage: HashMap<String, ResourceDiff> = reference
				.network_usage
				.iter()
				.map(|v| (v.resource_name.clone(), v.clone()))
				.collect();
			self.network_usage
				.iter_mut()
				.for_each(|v| v.compare_with(reference_network_usage.get(&v.resource_name)));

			let reference_cpu_usage: HashMap<String, ResourceDiff> = reference
				.cpu_usage
				.iter()
				.map(|v| (v.resource_name.clone(), v.clone()))
				.collect();
			self.cpu_usage
				.iter_mut()
				.for_each(|v| v.compare_with(reference_cpu_usage.get(&v.resource_name)));
		}
	}

	fn has_regression(&self, threshold: f64) -> bool {
		self.network_usage.iter().any(|v| v.has_regression(threshold)) ||
			self.cpu_usage.iter().any(|v| v.has_regression(threshold))
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResourceDiff {
	resource_name: String,
	total: f64,
	diff: Option<f64>,
}

impl std::fmt::Display for ResourceDiff {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let diff = self.diff.unwrap_or(0f64);
		let diff_str = format!("{:+.1}%", self.diff.unwrap_or(0f64));
		write!(
			f,
			"{:<32}{:>12.3}{:>8}",
			self.resource_name.cyan(),
			self.total,
			if diff > 0f64 { diff_str.red() } else { diff_str.green() }
		)
	}
}

impl ResourceDiff {
	fn compare_with(&mut self, reference: Option<&Self>) {
		if let Some(reference) = reference {
			self.diff = Some((self.total - reference.total) / reference.total * 100f64);
		}
	}

	fn has_regression(&self, threshold: f64) -> bool {
		match self.diff {
			Some(diff) => diff > threshold,
			None => false,
		}
	}
}

fn from_file(path: &str) -> eyre::Result<Vec<BenchmarkDiff>> {
	let path = std::path::Path::new(path);
	let string = String::from_utf8(std::fs::read(path)?)?;
	Ok(serde_yaml::from_str(&string)?)
}

fn main() -> eyre::Result<()> {
	color_eyre::install()?;
	let cli: GuardCli = GuardCli::parse();

	let reference: HashMap<String, BenchmarkDiff> = from_file(&cli.reference)?
		.into_iter()
		.map(|v| (v.benchmark_name.clone(), v))
		.collect();
	let mut current = from_file(&cli.current)?;
	current.iter_mut().for_each(|diff| {
		diff.compare_with(reference.get(&diff.benchmark_name));
	});
	let has_regression = current.iter().any(|diff| diff.has_regression(cli.threshold));

	for diff in current {
		println!("{}\n\n", diff);
	}

	if has_regression {
		return Err(eyre::eyre!("Found regressions more then {}%", cli.threshold))
	}

	Ok(())
}
