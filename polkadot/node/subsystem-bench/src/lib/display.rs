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

//! Display implementations and helper methods for parsing prometheus metrics
//! to a format that can be displayed in the CLI.
//!
//! Currently histogram buckets are skipped.

use crate::configuration::TestConfiguration;
use colored::Colorize;
use prometheus::{
	proto::{MetricFamily, MetricType},
	Registry,
};
use std::fmt::Display;

const LOG_TARGET: &str = "subsystem-bench::display";

#[derive(Default, Debug)]
pub struct MetricCollection(Vec<TestMetric>);

impl From<Vec<TestMetric>> for MetricCollection {
	fn from(metrics: Vec<TestMetric>) -> Self {
		MetricCollection(metrics)
	}
}

impl MetricCollection {
	pub fn all(&self) -> &Vec<TestMetric> {
		&self.0
	}

	/// Sums up all metrics with the given name in the collection
	pub fn sum_by(&self, name: &str) -> f64 {
		self.all()
			.iter()
			.filter(|metric| metric.name == name)
			.map(|metric| metric.value)
			.sum()
	}

	/// Tells if entries in bucket metric is lower than `value`
	pub fn metric_lower_than(&self, metric_name: &str, value: f64) -> bool {
		self.sum_by(metric_name) < value
	}

	pub fn subset_with_label_value(&self, label_name: &str, label_value: &str) -> MetricCollection {
		self.0
			.iter()
			.filter_map(|metric| {
				if let Some(index) = metric.label_names.iter().position(|label| label == label_name)
				{
					if Some(&String::from(label_value)) == metric.label_values.get(index) {
						Some(metric.clone())
					} else {
						None
					}
				} else {
					None
				}
			})
			.collect::<Vec<_>>()
			.into()
	}
}

impl Display for MetricCollection {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		writeln!(f)?;
		let metrics = self.all();
		for metric in metrics {
			writeln!(f, "{}", metric)?;
		}
		Ok(())
	}
}

#[derive(Debug, Clone)]
pub struct TestMetric {
	name: String,
	label_names: Vec<String>,
	label_values: Vec<String>,
	value: f64,
}

impl Display for TestMetric {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"({} = {}) [{:?}, {:?}]",
			self.name.cyan(),
			format!("{}", self.value).white(),
			self.label_names,
			self.label_values
		)
	}
}

// Returns `false` if metric should be skipped.
fn check_metric_family(mf: &MetricFamily) -> bool {
	if mf.get_metric().is_empty() {
		gum::error!(target: LOG_TARGET, "MetricFamily has no metrics: {:?}", mf);
		return false
	}
	if mf.get_name().is_empty() {
		gum::error!(target: LOG_TARGET, "MetricFamily has no name: {:?}", mf);
		return false
	}

	true
}

pub fn parse_metrics(registry: &Registry) -> MetricCollection {
	let metric_families = registry.gather();
	let mut test_metrics = Vec::new();
	for mf in metric_families {
		if !check_metric_family(&mf) {
			continue
		}

		let name: String = mf.get_name().into();
		let metric_type = mf.get_field_type();
		for m in mf.get_metric() {
			let (label_names, label_values): (Vec<String>, Vec<String>) = m
				.get_label()
				.iter()
				.map(|pair| (String::from(pair.get_name()), String::from(pair.get_value())))
				.unzip();

			match metric_type {
				MetricType::COUNTER => {
					test_metrics.push(TestMetric {
						name: name.clone(),
						label_names,
						label_values,
						value: m.get_counter().get_value(),
					});
				},
				MetricType::GAUGE => {
					test_metrics.push(TestMetric {
						name: name.clone(),
						label_names,
						label_values,
						value: m.get_gauge().get_value(),
					});
				},
				MetricType::HISTOGRAM => {
					let h = m.get_histogram();
					let h_name = name.clone() + "_sum";
					test_metrics.push(TestMetric {
						name: h_name,
						label_names: label_names.clone(),
						label_values: label_values.clone(),
						value: h.get_sample_sum(),
					});

					let h_name = name.clone() + "_count";
					test_metrics.push(TestMetric {
						name: h_name,
						label_names,
						label_values,
						value: h.get_sample_count() as f64,
					});
				},
				MetricType::SUMMARY => {
					unimplemented!();
				},
				MetricType::UNTYPED => {
					unimplemented!();
				},
			}
		}
	}
	test_metrics.into()
}

impl Display for TestConfiguration {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{}, {}, {}, {}, {}",
			format!("n_validators = {}", self.n_validators).blue(),
			format!("n_cores = {}", self.n_cores).blue(),
			format!("pov_size = {} - {}", self.min_pov_size, self.max_pov_size).bright_black(),
			format!("connectivity = {}", self.connectivity).bright_black(),
			format!("latency = {:?}", self.latency).bright_black(),
		)
	}
}
