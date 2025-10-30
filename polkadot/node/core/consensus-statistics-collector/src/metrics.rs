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

use std::collections::HashMap;
use gum::CandidateHash;
use polkadot_node_subsystem::prometheus::Opts;
use polkadot_node_subsystem_util::metrics::{
	self,
	prometheus::{self, Gauge, GaugeVec, U64},
};
use polkadot_primitives::SessionIndex;
use crate::approval_voting_metrics::ApprovalsStats;

#[derive(Clone)]
pub(crate) struct MetricsInner {
	approvals_usage_per_session: prometheus::CounterVec<U64>,
	no_shows_per_session: prometheus::CounterVec<U64>,

	approvals_per_session_per_validator: prometheus::CounterVec<U64>,
	no_shows_per_session_per_validator: prometheus::CounterVec<U64>,
}


/// Candidate backing metrics.
#[derive(Default, Clone)]
pub struct Metrics(pub(crate) Option<MetricsInner>);

impl Metrics {
	pub fn record_approvals_stats(&self, session: SessionIndex, approval_stats: HashMap<CandidateHash, ApprovalsStats>) {
		self.0.as_ref().map(|metrics| {
			for stats in approval_stats.values() {
				metrics.approvals_usage_per_session.with_label_values(
					&[session.to_string().as_str()]).inc_by(stats.votes.len() as u64);

				metrics.no_shows_per_session.with_label_values(
					&[session.to_string().as_str()]).inc_by(stats.no_shows.len() as u64);

				for validator in &stats.votes {
					metrics.approvals_per_session_per_validator.with_label_values(
						&[session.to_string().as_str(), validator.0.to_string().as_str()]).inc()
				}

				for validator in &stats.no_shows {
					metrics.no_shows_per_session_per_validator.with_label_values(
						&[session.to_string().as_str(), validator.0.to_string().as_str()]).inc()
				}
			}
		});
	}
}

impl metrics::Metrics for Metrics {
	fn try_register(registry: &prometheus::Registry) -> Result<Self, prometheus::PrometheusError> {
		let metrics = MetricsInner {
			approvals_per_session_per_validator: prometheus::register(
				prometheus::CounterVec::new(
					prometheus::Opts::new(
						"polkadot_parachain_rewards_statistics_collector_approvals_per_session_per_validator",
						"Total number of useful approvals a given validator provided on a session.",
					),
						vec!["session", "validator_idx"].as_ref(),
				)?,
				registry,
			)?,
			no_shows_per_session_per_validator: prometheus::register(
				prometheus::CounterVec::new(
					prometheus::Opts::new(
						"polkadot_parachain_rewards_statistics_collector_no_shows_per_session_per_validator",
						"Total number a given validator no showed on a session.",
					),
					vec!["session", "validator_idx"].as_ref(),
				)?,
				registry,
			)?,
			approvals_usage_per_session: prometheus::register(
				prometheus::CounterVec::new(
					prometheus::Opts::new(
						"polkadot_parachain_rewards_statistics_collector_approvals_per_session",
						"Total number of useful approvals on a session.",
					),
					vec!["session"].as_ref(),
				)?,
				registry,
			)?,
			no_shows_per_session: prometheus::register(
				prometheus::CounterVec::new(
					prometheus::Opts::new(
						"polkadot_parachain_rewards_statistics_collector_no_shows_per_session",
						"Total number of no-shows on a session.",
					),
					vec!["session"].as_ref(),
				)?,
				registry,
			)?,
		};
		Ok(Metrics(Some(metrics)))
	}
}
