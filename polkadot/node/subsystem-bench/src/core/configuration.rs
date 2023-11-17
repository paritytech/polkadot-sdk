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

use crate::availability::AvailabilityRecoveryConfiguration;

use super::*;
pub use crate::cli::TestObjective;
use rand::{distributions::Uniform, prelude::Distribution, thread_rng};
use serde::{Deserialize, Serialize};

pub fn random_pov_size(min_pov_size: usize, max_pov_size: usize) -> usize {
	random_uniform_sample(min_pov_size, max_pov_size)
}

fn random_uniform_sample<T: Into<usize> + From<usize>>(min_value: T, max_value: T) -> T {
	Uniform::from(min_value.into()..=max_value.into())
		.sample(&mut thread_rng())
		.into()
}

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
	/// The test objective
	pub objective: TestObjective,
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

fn generate_pov_sizes(count: usize, min_kib: usize, max_kib: usize) -> Vec<usize> {
	(0..count).map(|_| random_pov_size(min_kib * 1024, max_kib * 1024)).collect()
}

#[derive(Serialize, Deserialize)]
pub struct TestSequence {
	#[serde(rename(serialize = "TestConfiguration", deserialize = "TestConfiguration"))]
	test_configurations: Vec<TestConfiguration>,
}

impl TestSequence {
	pub fn to_vec(mut self) -> Vec<TestConfiguration> {
		self.test_configurations
			.into_iter()
			.map(|mut config| {
				config.pov_sizes =
					generate_pov_sizes(config.n_cores, config.min_pov_size, config.max_pov_size);
				config
			})
			.collect()
	}
}

impl TestSequence {
	pub fn new_from_file(path: &Path) -> std::io::Result<TestSequence> {
		let string = String::from_utf8(std::fs::read(&path)?).expect("File is valid UTF8");
		Ok(serde_yaml::from_str(&string).expect("File is valid test sequence YA"))
	}
}

impl TestConfiguration {
	pub fn write_to_disk(&self) {
		// Serialize a slice of configurations
		let yaml = serde_yaml::to_string(&TestSequence { test_configurations: vec![self.clone()] })
			.unwrap();
		std::fs::write("last_test.yaml", yaml).unwrap();
	}

	pub fn pov_sizes(&self) -> &[usize] {
		&self.pov_sizes
	}
	/// An unconstrained standard configuration matching Polkadot/Kusama
	pub fn ideal_network(
		objective: TestObjective,
		num_blocks: usize,
		n_validators: usize,
		n_cores: usize,
		min_pov_size: usize,
		max_pov_size: usize,
	) -> TestConfiguration {
		Self {
			objective,
			n_cores,
			n_validators,
			pov_sizes: generate_pov_sizes(n_cores, min_pov_size, max_pov_size),
			bandwidth: 50 * 1024 * 1024,
			peer_bandwidth: 50 * 1024 * 1024,
			// No latency
			latency: None,
			error: 0,
			num_blocks,
			min_pov_size,
			max_pov_size,
		}
	}

	pub fn healthy_network(
		objective: TestObjective,
		num_blocks: usize,
		n_validators: usize,
		n_cores: usize,
		min_pov_size: usize,
		max_pov_size: usize,
	) -> TestConfiguration {
		Self {
			objective,
			n_cores,
			n_validators,
			pov_sizes: generate_pov_sizes(n_cores, min_pov_size, max_pov_size),
			bandwidth: 50 * 1024 * 1024,
			peer_bandwidth: 50 * 1024 * 1024,
			latency: Some(PeerLatency {
				min_latency: Duration::from_millis(1),
				max_latency: Duration::from_millis(100),
			}),
			error: 3,
			num_blocks,
			min_pov_size,
			max_pov_size,
		}
	}

	pub fn degraded_network(
		objective: TestObjective,
		num_blocks: usize,
		n_validators: usize,
		n_cores: usize,
		min_pov_size: usize,
		max_pov_size: usize,
	) -> TestConfiguration {
		Self {
			objective,
			n_cores,
			n_validators,
			pov_sizes: generate_pov_sizes(n_cores, min_pov_size, max_pov_size),
			bandwidth: 50 * 1024 * 1024,
			peer_bandwidth: 50 * 1024 * 1024,
			latency: Some(PeerLatency {
				min_latency: Duration::from_millis(10),
				max_latency: Duration::from_millis(500),
			}),
			error: 33,
			num_blocks,
			min_pov_size,
			max_pov_size,
		}
	}
}
