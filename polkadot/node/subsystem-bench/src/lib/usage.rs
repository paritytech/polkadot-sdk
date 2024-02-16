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

//! Test usage implementation

use colored::Colorize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkUsage {
	pub benchmark_name: String,
	pub network_usage: Vec<ResourceUsage>,
	pub cpu_usage: Vec<ResourceUsage>,
}

impl std::fmt::Display for BenchmarkUsage {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(
			f,
			"\n{}\n\n{}\n{}\n\n{}\n{}\n",
			self.benchmark_name.purple(),
			format!("{:<32}{:>12}{:>12}", "Network usage, KiB", "total", "per block").blue(),
			self.network_usage
				.iter()
				.map(|v| v.to_string())
				.collect::<Vec<String>>()
				.join("\n"),
			format!("{:<32}{:>12}{:>12}", "CPU usage, seconds", "total", "per block").blue(),
			self.cpu_usage.iter().map(|v| v.to_string()).collect::<Vec<String>>().join("\n")
		)
	}
}

impl BenchmarkUsage {
	pub fn check_network_usage(&self, checks: &[ResourceUsageCheck]) -> Vec<String> {
		check_usage(&self.benchmark_name, &self.network_usage, checks)
	}

	pub fn check_cpu_usage(&self, checks: &[ResourceUsageCheck]) -> Vec<String> {
		check_usage(&self.benchmark_name, &self.cpu_usage, checks)
	}
}

fn check_usage(
	benchmark_name: &str,
	usage: &[ResourceUsage],
	checks: &[ResourceUsageCheck],
) -> Vec<String> {
	checks
		.iter()
		.filter_map(|check| {
			check_resource_usage(usage, check)
				.map(|message| format!("{}: {}", benchmark_name, message))
		})
		.collect()
}

fn check_resource_usage(
	usage: &[ResourceUsage],
	(resource_name, min, max): &ResourceUsageCheck,
) -> Option<String> {
	if let Some(usage) = usage.iter().find(|v| v.resource_name == *resource_name) {
		if usage.per_block >= *min && usage.per_block < *max {
			None
		} else {
			Some(format!(
				"The resource `{}` is expected to be in the range of {}..{}, but the value is {}",
				resource_name, min, max, usage.per_block
			))
		}
	} else {
		Some(format!("The resource `{}` is not found", resource_name))
	}
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceUsage {
	pub resource_name: String,
	pub total: f64,
	pub per_block: f64,
}

impl std::fmt::Display for ResourceUsage {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "{:<32}{:>12.3}{:>12.3}", self.resource_name.cyan(), self.total, self.per_block)
	}
}

type ResourceUsageCheck<'a> = (&'a str, f64, f64);
