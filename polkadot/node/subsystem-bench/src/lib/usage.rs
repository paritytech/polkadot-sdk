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
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
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
				.sorted()
				.collect::<Vec<String>>()
				.join("\n"),
			format!("{:<32}{:>12}{:>12}", "CPU usage, seconds", "total", "per block").blue(),
			self.cpu_usage
				.iter()
				.map(|v| v.to_string())
				.sorted()
				.collect::<Vec<String>>()
				.join("\n")
		)
	}
}

impl BenchmarkUsage {
	pub fn average(usages: &[Self]) -> Self {
		let all_network_usages: Vec<&ResourceUsage> =
			usages.iter().flat_map(|v| &v.network_usage).collect();
		let all_cpu_usage: Vec<&ResourceUsage> = usages.iter().flat_map(|v| &v.cpu_usage).collect();

		Self {
			benchmark_name: usages.first().map(|v| v.benchmark_name.clone()).unwrap_or_default(),
			network_usage: ResourceUsage::average_by_resource_name(&all_network_usages),
			cpu_usage: ResourceUsage::average_by_resource_name(&all_cpu_usage),
		}
	}

	pub fn check_network_usage(&self, checks: &[ResourceUsageCheck]) -> Vec<String> {
		check_usage(&self.benchmark_name, &self.network_usage, checks)
	}

	pub fn check_cpu_usage(&self, checks: &[ResourceUsageCheck]) -> Vec<String> {
		check_usage(&self.benchmark_name, &self.cpu_usage, checks)
	}

	pub fn cpu_usage_diff(&self, other: &Self, resource_name: &str) -> Option<f64> {
		let self_res = self.cpu_usage.iter().find(|v| v.resource_name == resource_name);
		let other_res = other.cpu_usage.iter().find(|v| v.resource_name == resource_name);

		match (self_res, other_res) {
			(Some(self_res), Some(other_res)) => Some(self_res.diff(other_res)),
			_ => None,
		}
	}

	// Prepares a json string for a graph representation
	// See: https://github.com/benchmark-action/github-action-benchmark?tab=readme-ov-file#examples
	pub fn to_chart_json(&self) -> color_eyre::eyre::Result<String> {
		let chart = self
			.network_usage
			.iter()
			.map(|v| ChartItem {
				name: v.resource_name.clone(),
				unit: "KiB".to_string(),
				value: v.per_block,
			})
			.chain(self.cpu_usage.iter().map(|v| ChartItem {
				name: v.resource_name.clone(),
				unit: "seconds".to_string(),
				value: v.per_block,
			}))
			.collect::<Vec<_>>();

		Ok(serde_json::to_string(&chart)?)
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
	(resource_name, base, precision): &ResourceUsageCheck,
) -> Option<String> {
	if let Some(usage) = usage.iter().find(|v| v.resource_name == *resource_name) {
		let diff = (base - usage.per_block).abs() / base;
		if diff < *precision {
			None
		} else {
			Some(format!(
				"The resource `{}` is expected to be equal to {} with a precision {}, but the current value is {} ({})",
				resource_name, base, precision, usage.per_block, diff
			))
		}
	} else {
		Some(format!("The resource `{}` is not found", resource_name))
	}
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResourceUsage {
	pub resource_name: String,
	pub total: f64,
	pub per_block: f64,
}

impl std::fmt::Display for ResourceUsage {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "{:<32}{:>12.4}{:>12.4}", self.resource_name.cyan(), self.total, self.per_block)
	}
}

impl ResourceUsage {
	fn average_by_resource_name(usages: &[&Self]) -> Vec<Self> {
		let mut by_name: HashMap<String, Vec<&Self>> = Default::default();
		for usage in usages {
			by_name.entry(usage.resource_name.clone()).or_default().push(usage);
		}
		let mut average = vec![];
		for (resource_name, values) in by_name {
			let total = values.iter().map(|v| v.total).sum::<f64>() / values.len() as f64;
			let per_block = values.iter().map(|v| v.per_block).sum::<f64>() / values.len() as f64;
			average.push(Self { resource_name, total, per_block });
		}
		average
	}

	fn diff(&self, other: &Self) -> f64 {
		(self.per_block - other.per_block).abs() / self.per_block
	}
}

type ResourceUsageCheck<'a> = (&'a str, f64, f64);

#[derive(Debug, Serialize)]
pub struct ChartItem {
	pub name: String,
	pub unit: String,
	pub value: f64,
}
