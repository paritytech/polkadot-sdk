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

use super::*;

/// Peer response latency configuration.
#[derive(Clone, Debug)]
pub struct PeerLatency {
	/// Min latency for `NetworkAction` completion.
	pub min_latency: Duration,
	/// Max latency or `NetworkAction` completion.
	pub max_latency: Duration,
}

/// The test input parameters
#[derive(Clone, Debug)]
pub struct TestConfiguration {
	/// Configuration for the `availability-recovery` subsystem.
	pub use_fast_path: bool,
	/// Number of validators
	pub n_validators: usize,
	/// Number of cores
	pub n_cores: usize,
	/// The PoV size
	pub pov_sizes: Vec<usize>,
	/// This parameter is used to determine how many recoveries we batch in parallel
	/// to simulate tranche0 recoveries.
	pub max_parallel_recoveries: usize,
	/// The amount of bandiwdht remote validators have.
	pub bandwidth: usize,
	/// Optional peer emulation latency
	pub latency: Option<PeerLatency>,
	/// Error probability
	pub error: usize,
}

impl Default for TestConfiguration {
	fn default() -> Self {
		Self {
			use_fast_path: false,
			n_validators: 10,
			n_cores: 10,
			pov_sizes: vec![5 * 1024 * 1024],
			max_parallel_recoveries: 6,
			bandwidth: 60 * 1024 * 1024,
			latency: None,
			error: 0,
		}
	}
}

impl TestConfiguration {
	/// An unconstrained standard configuration matching Polkadot/Kusama
	pub fn unconstrained_300_validators_60_cores(pov_sizes: Vec<usize>) -> TestConfiguration {
		Self {
			use_fast_path: false,
			n_validators: 300,
			n_cores: 100,
			pov_sizes,
			max_parallel_recoveries: 100,
			// HW specs node bandwidth
			bandwidth: 60 * 1024 * 1024,
			// No latency
			latency: None,
			error: 0,
		}
	}
	pub fn unconstrained_1000_validators_60_cores(pov_sizes: Vec<usize>) -> TestConfiguration {
		Self {
			use_fast_path: false,
			n_validators: 300,
			n_cores: 60,
			pov_sizes,
			max_parallel_recoveries: 30,
			// HW specs node bandwidth
			bandwidth: 60 * 1024 * 1024,
			// No latency
			latency: None,
			error: 0,
		}
	}

	/// Polkadot/Kusama configuration with typical latency constraints.
	pub fn healthy_network_300_validators_60_cores(pov_sizes: Vec<usize>) -> TestConfiguration {
		Self {
			use_fast_path: true,
			n_validators: 300,
			n_cores: 60,
			pov_sizes,
			max_parallel_recoveries: 6,
			// HW specs node bandwidth
			bandwidth: 60 * 1024 * 1024,
			latency: Some(PeerLatency {
				min_latency: Duration::from_millis(1),
				max_latency: Duration::from_millis(50),
			}),
			error: 5,
		}
	}

	/// Polkadot/Kusama configuration with degraded due to latencies.
	/// TODO: implement errors.
	pub fn degraded_network_300_validators_60_cores(pov_sizes: Vec<usize>) -> TestConfiguration {
		Self {
			use_fast_path: false,
			n_validators: 300,
			n_cores: 100,
			pov_sizes,
			max_parallel_recoveries: 20,
			// HW specs node bandwidth
			bandwidth: 60 * 1024 * 1024,
			// A range of latencies to expect in a degraded network
			latency: Some(PeerLatency {
				min_latency: Duration::from_millis(1),
				max_latency: Duration::from_millis(500),
			}),
			error: 30,
		}
	}
}
