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

use std::path::Path;

use super::*;
use serde::{Deserialize, Serialize};
/// Peer response latency configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PeerLatency {
	/// Min latency for `NetworkAction` completion.
	pub min_latency: Duration,
	/// Max latency or `NetworkAction` completion.
	pub max_latency: Duration,
}

/// The test input parameters
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestConfiguration {
	/// Configuration for the `availability-recovery` subsystem.
	pub use_fast_path: bool,
	/// Number of validators
	pub n_validators: usize,
	/// Number of cores
	pub n_cores: usize,
	/// The min PoV size
	pub min_pov_size: usize,
	/// The max PoV size,
	pub max_pov_size: usize,
	/// Randomly sampled pov_sizes
	#[serde(skip)]
	pov_sizes: Vec<usize>,
	/// The amount of bandiwdth remote validators have.
	pub peer_bandwidth: usize,
	/// The amount of bandiwdth our node has.
	pub bandwidth: usize,
	/// Optional peer emulation latency
	pub latency: Option<PeerLatency>,
	/// Error probability
	pub error: usize,
	/// Number of blocks
	/// In one block `n_cores` candidates are recovered
	pub num_blocks: usize,
}

impl Default for TestConfiguration {
	fn default() -> Self {
		Self {
			use_fast_path: false,
			n_validators: 100,
			n_cores: 10,
			pov_sizes: vec![5 * 1024 * 1024],
			bandwidth: 60 * 1024 * 1024,
			peer_bandwidth: 60 * 1024 * 1024,
			latency: None,
			error: 0,
			num_blocks: 1,
			min_pov_size: 5 * 1024 * 1024,
			max_pov_size: 5 * 1024 * 1024,
		}
	}
}

fn generate_pov_sizes(count: usize, min: usize, max: usize) -> Vec<usize> {
	(0..count).map(|_| random_pov_size(min, max)).collect()
}

#[derive(Serialize, Deserialize)]
pub struct TestSequence {
	#[serde(rename(serialize = "TestConfiguration", deserialize = "TestConfiguration"))]
	test_configurations: Vec<TestConfiguration>,
}

impl TestSequence {
	pub fn to_vec(mut self) -> Vec<TestConfiguration> {
		// Generate Pov sizes

		for config in self.test_configurations.iter_mut() {
			config.pov_sizes =
				generate_pov_sizes(config.n_cores, config.min_pov_size, config.max_pov_size);
		}

		self.test_configurations
	}
}

impl TestSequence {
	pub fn new_from_file(path: &Path) -> std::io::Result<TestSequence> {
		let string = String::from_utf8(std::fs::read(&path)?).expect("File is valid UTF8");
		Ok(toml::from_str(&string).expect("File is valid test sequence TOML"))
	}
}

impl TestConfiguration {
	pub fn write_to_disk(&self) {
		// Serialize a slice of configurations
		let toml =
			toml::to_string(&TestSequence { test_configurations: vec![self.clone()] }).unwrap();
		std::fs::write("last_test.toml", toml).unwrap();
	}

	pub fn pov_sizes(&self) -> &[usize] {
		&self.pov_sizes
	}
	/// An unconstrained standard configuration matching Polkadot/Kusama
	pub fn ideal_network(
		num_blocks: usize,
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
			peer_bandwidth: 50 * 1024 * 1024,
			// No latency
			latency: None,
			error: 0,
			num_blocks,
			..Default::default()
		}
	}

	pub fn healthy_network(
		num_blocks: usize,
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
			peer_bandwidth: 50 * 1024 * 1024,
			latency: Some(PeerLatency {
				min_latency: Duration::from_millis(1),
				max_latency: Duration::from_millis(100),
			}),
			error: 3,
			num_blocks,
			..Default::default()
		}
	}

	pub fn degraded_network(
		num_blocks: usize,
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
			peer_bandwidth: 50 * 1024 * 1024,
			latency: Some(PeerLatency {
				min_latency: Duration::from_millis(10),
				max_latency: Duration::from_millis(500),
			}),
			error: 33,
			num_blocks,
			..Default::default()
		}
	}
}
