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
//! Some helper methods for parsing prometheus metrics to a format that can be
//! displayed in the CLI.
//!
//! Currently histogram buckets are skipped.
use super::LOG_TARGET;
use colored::Colorize;
use prometheus::{
	proto::{MetricFamily, MetricType},
	Registry,
};
use std::fmt::Display;

#[derive(Default)]
pub struct MetricCollection(Vec<TestMetric>);

impl From<Vec<TestMetric>> for MetricCollection {
	fn from(metrics: Vec<TestMetric>) -> Self {
		MetricCollection(metrics)
	}
}

impl MetricCollection {
	pub fn get(&self, name: &str) -> Vec<&TestMetric> {
		self.all().into_iter().filter(|metric| &metric.name == name).collect()
	}

	pub fn all(&self) -> &Vec<TestMetric> {
		&self.0
	}

	/// Sums up all metrics with the given name in the collection
	pub fn sum_by(&self, name: &str) -> f64 {
		self.all()
			.into_iter()
			.filter(|metric| &metric.name == name)
			.map(|metric| metric.value)
			.sum()
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
		writeln!(f, "")?;
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

// fn encode_impl(
//     &self,
//     metric_families: &[MetricFamily],
//     writer: &mut dyn WriteUtf8,
// ) -> Result<()> { for mf in metric_families { // Fail-fast checks. check_metric_family(mf)?;

//         // Write `# HELP` header.
//         let name = mf.get_name();
//         let help = mf.get_help();
//         if !help.is_empty() {
//             writer.write_all("# HELP ")?;
//             writer.write_all(name)?;
//             writer.write_all(" ")?;
//             writer.write_all(&escape_string(help, false))?;
//             writer.write_all("\n")?;
//         }

//         // Write `# TYPE` header.
//         let metric_type = mf.get_field_type();
//         let lowercase_type = format!("{:?}", metric_type).to_lowercase();
//         writer.write_all("# TYPE ")?;
//         writer.write_all(name)?;
//         writer.write_all(" ")?;
//         writer.write_all(&lowercase_type)?;
//         writer.write_all("\n")?;

//         for m in mf.get_metric() {
//             match metric_type {
//                 MetricType::COUNTER => {
//                     write_sample(writer, name, None, m, None, m.get_counter().get_value())?;
//                 }
//                 MetricType::GAUGE => {
//                     write_sample(writer, name, None, m, None, m.get_gauge().get_value())?;
//                 }
//                 MetricType::HISTOGRAM => {
//                     let h = m.get_histogram();

//                     let mut inf_seen = false;
//                     for b in h.get_bucket() {
//                         let upper_bound = b.get_upper_bound();
//                         write_sample(
//                             writer,
//                             name,
//                             Some("_bucket"),
//                             m,
//                             Some((BUCKET_LABEL, &upper_bound.to_string())),
//                             b.get_cumulative_count() as f64,
//                         )?;
//                         if upper_bound.is_sign_positive() && upper_bound.is_infinite() {
//                             inf_seen = true;
//                         }
//                     }
//                     if !inf_seen {
//                         write_sample(
//                             writer,
//                             name,
//                             Some("_bucket"),
//                             m,
//                             Some((BUCKET_LABEL, POSITIVE_INF)),
//                             h.get_sample_count() as f64,
//                         )?;
//                     }

//                     write_sample(writer, name, Some("_sum"), m, None, h.get_sample_sum())?;

//                     write_sample(
//                         writer,
//                         name,
//                         Some("_count"),
//                         m,
//                         None,
//                         h.get_sample_count() as f64,
//                     )?;
//                 }
//                 MetricType::SUMMARY => {
//                     let s = m.get_summary();

//                     for q in s.get_quantile() {
//                         write_sample(
//                             writer,
//                             name,
//                             None,
//                             m,
//                             Some((QUANTILE, &q.get_quantile().to_string())),
//                             q.get_value(),
//                         )?;
//                     }

//                     write_sample(writer, name, Some("_sum"), m, None, s.get_sample_sum())?;

//                     write_sample(
//                         writer,
//                         name,
//                         Some("_count"),
//                         m,
//                         None,
//                         s.get_sample_count() as f64,
//                     )?;
//                 }
//                 MetricType::UNTYPED => {
//                     unimplemented!();
//                 }
//             }
//         }
//     }

//     Ok(())
// }

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
					let h_name = name.clone() + "_sum".into();
					test_metrics.push(TestMetric {
						name: h_name,
						label_names: label_names.clone(),
						label_values: label_values.clone(),
						value: h.get_sample_sum(),
					});

					let h_name = name.clone() + "_count".into();
					test_metrics.push(TestMetric {
						name: h_name,
						label_names,
						label_values,
						value: h.get_sample_sum(),
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
