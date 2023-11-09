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
#[derive(Clone, Debug, Default)]
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
	/// The amount of bandiwdth remote validators have.
	pub bandwidth: usize,
	/// Optional peer emulation latency
	pub latency: Option<PeerLatency>,
	/// Error probability
	pub error: usize,
	/// Number of loops
	/// In one loop `n_cores` candidates are recovered
	pub num_loops: usize,
}

impl Default for TestConfiguration {
	fn default() -> Self {
		Self {
			use_fast_path: false,
			n_validators: 10,
			n_cores: 10,
			pov_sizes: vec![5 * 1024 * 1024],
			bandwidth: 60 * 1024 * 1024,
			latency: None,
			error: 0,
			num_loops: 1,
		}
	}
}

impl TestConfiguration {
	/// An unconstrained standard configuration matching Polkadot/Kusama
	pub fn ideal_network(
		num_loops: usize,
		use_fast_path: bool,
		n_validators: usize,
		n_cores: usize,
		pov_sizes: Vec<usize>,
	) -> TestConfiguration {
		Self {
			use_fast_path,
			n_cores,
			n_validators,
			pov_sizes,
			// HW specs node bandwidth
			bandwidth: 50 * 1024 * 1024,
			// No latency
			latency: None,
			error: 0,
			num_loops,
		}
	}

	pub fn healthy_network(
		num_loops: usize,
		use_fast_path: bool,
		n_validators: usize,
		n_cores: usize,
		pov_sizes: Vec<usize>,
	) -> TestConfiguration {
		Self {
			use_fast_path,
			n_cores,
			n_validators,
			pov_sizes,
			bandwidth: 50 * 1024 * 1024,
			latency: Some(PeerLatency {
				min_latency: Duration::from_millis(1),
				max_latency: Duration::from_millis(100),
			}),
			error: 3,
			num_loops,
		}
	}

	pub fn degraded_network(
		num_loops: usize,
		use_fast_path: bool,
		n_validators: usize,
		n_cores: usize,
		pov_sizes: Vec<usize>,
	) -> TestConfiguration {
		Self {
			use_fast_path,
			n_cores,
			n_validators,
			pov_sizes,
			bandwidth: 50 * 1024 * 1024,
			latency: Some(PeerLatency {
				min_latency: Duration::from_millis(10),
				max_latency: Duration::from_millis(500),
			}),
			error: 33,
			num_loops,
		}
	}
}
