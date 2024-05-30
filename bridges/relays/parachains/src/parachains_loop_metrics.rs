// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use bp_polkadot_core::parachains::ParaId;
use relay_utils::{
	metrics::{metric_name, register, Gauge, Metric, PrometheusError, Registry, U64},
	UniqueSaturatedInto,
};

/// Parachains sync metrics.
#[derive(Clone)]
pub struct ParachainsLoopMetrics {
	/// Best parachains header numbers at the source.
	best_source_block_numbers: Gauge<U64>,
	/// Best parachains header numbers at the target.
	best_target_block_numbers: Gauge<U64>,
}

impl ParachainsLoopMetrics {
	/// Create and register parachains loop metrics.
	pub fn new(prefix: Option<&str>) -> Result<Self, PrometheusError> {
		Ok(ParachainsLoopMetrics {
			best_source_block_numbers: Gauge::new(
				metric_name(prefix, "best_parachain_block_number_at_source"),
				"Best parachain block numbers at the source relay chain".to_string(),
			)?,
			best_target_block_numbers: Gauge::new(
				metric_name(prefix, "best_parachain_block_number_at_target"),
				"Best parachain block numbers at the target chain".to_string(),
			)?,
		})
	}

	/// Update best block number at source.
	pub fn update_best_parachain_block_at_source<Number: UniqueSaturatedInto<u64>>(
		&self,
		parachain: ParaId,
		block_number: Number,
	) {
		let block_number = block_number.unique_saturated_into();
		log::trace!(
			target: "bridge-metrics",
			"Updated value of metric 'best_parachain_block_number_at_source[{:?}]': {:?}",
			parachain,
			block_number,
		);
		self.best_source_block_numbers.set(block_number);
	}

	/// Update best block number at target.
	pub fn update_best_parachain_block_at_target<Number: UniqueSaturatedInto<u64>>(
		&self,
		parachain: ParaId,
		block_number: Number,
	) {
		let block_number = block_number.unique_saturated_into();
		log::trace!(
			target: "bridge-metrics",
			"Updated value of metric 'best_parachain_block_number_at_target[{:?}]': {:?}",
			parachain,
			block_number,
		);
		self.best_target_block_numbers.set(block_number);
	}
}

impl Metric for ParachainsLoopMetrics {
	fn register(&self, registry: &Registry) -> Result<(), PrometheusError> {
		register(self.best_source_block_numbers.clone(), registry)?;
		register(self.best_target_block_numbers.clone(), registry)?;
		Ok(())
	}
}
