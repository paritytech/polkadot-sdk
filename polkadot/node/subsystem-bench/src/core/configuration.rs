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
//
//! Test configuration definition and helpers.
use super::*;
use keyring::Keyring;
use std::path::Path;

pub use crate::cli::TestObjective;
use polkadot_primitives::{AuthorityDiscoveryId, ValidatorId};
use rand::thread_rng;
use rand_distr::{Distribution, Normal, Uniform};

use serde::{Deserialize, Serialize};

pub fn random_pov_size(min_pov_size: usize, max_pov_size: usize) -> usize {
	random_uniform_sample(min_pov_size, max_pov_size)
}

fn random_uniform_sample<T: Into<usize> + From<usize>>(min_value: T, max_value: T) -> T {
	Uniform::from(min_value.into()..=max_value.into())
		.sample(&mut thread_rng())
		.into()
}

/// Peer networking latency configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PeerLatency {
	/// The mean latency(milliseconds) of the peers.
	pub mean_latency_ms: usize,
	/// The standard deviation
	pub std_dev: f64,
}

// Default PoV size in KiB.
fn default_pov_size() -> usize {
	5120
}

// Default bandwidth in bytes
fn default_bandwidth() -> usize {
	52428800
}

// Default connectivity percentage
fn default_connectivity() -> usize {
	100
}

// Default backing group size
fn default_backing_group_size() -> usize {
	5
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
	/// Maximum backing group size
	#[serde(default = "default_backing_group_size")]
	pub max_validators_per_core: usize,
	/// The min PoV size
	#[serde(default = "default_pov_size")]
	pub min_pov_size: usize,
	/// The max PoV size,
	#[serde(default = "default_pov_size")]
	pub max_pov_size: usize,
	/// Randomly sampled pov_sizes
	#[serde(skip)]
	pov_sizes: Vec<usize>,
	/// The amount of bandiwdth remote validators have.
	#[serde(default = "default_bandwidth")]
	pub peer_bandwidth: usize,
	/// The amount of bandiwdth our node has.
	#[serde(default = "default_bandwidth")]
	pub bandwidth: usize,
	/// Optional peer emulation latency (round trip time) wrt node under test
	#[serde(default)]
	pub latency: Option<PeerLatency>,
	/// Connectivity ratio, the percentage of peers we are not connected to, but ar part of
	/// the topology.
	#[serde(default = "default_connectivity")]
	pub connectivity: usize,
	/// Number of blocks to run the test for
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
	pub fn into_vec(self) -> Vec<TestConfiguration> {
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
		let string = String::from_utf8(std::fs::read(path)?).expect("File is valid UTF8");
		Ok(serde_yaml::from_str(&string).expect("File is valid test sequence YA"))
	}
}

/// Helper struct for authority related state.
#[derive(Clone)]
pub struct TestAuthorities {
	pub keyring: Keyring,
	pub validator_public: Vec<ValidatorId>,
	pub validator_authority_id: Vec<AuthorityDiscoveryId>,
}

impl TestConfiguration {
	#[allow(unused)]
	pub fn write_to_disk(&self) {
		// Serialize a slice of configurations
		let yaml = serde_yaml::to_string(&TestSequence { test_configurations: vec![self.clone()] })
			.unwrap();
		std::fs::write("last_test.yaml", yaml).unwrap();
	}

	pub fn pov_sizes(&self) -> &[usize] {
		&self.pov_sizes
	}
	/// Return the number of peers connected to our node.
	pub fn connected_count(&self) -> usize {
		((self.n_validators - 1) as f64 / (100.0 / self.connectivity as f64)) as usize
	}

	/// Generates the authority keys we need for the network emulation.
	pub fn generate_authorities(&self) -> TestAuthorities {
		let keyring = Keyring::default();

		let keys = (0..self.n_validators)
			.map(|peer_index| keyring.sr25519_new(format!("Node{}", peer_index)))
			.collect::<Vec<_>>();

		// Generate `AuthorityDiscoveryId`` for each peer
		let validator_public: Vec<ValidatorId> =
			keys.iter().map(|key| (*key).into()).collect::<Vec<_>>();

		let validator_authority_id: Vec<AuthorityDiscoveryId> =
			keys.iter().map(|key| (*key).into()).collect::<Vec<_>>();

		TestAuthorities { keyring, validator_public, validator_authority_id }
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
			max_validators_per_core: 5,
			pov_sizes: generate_pov_sizes(n_cores, min_pov_size, max_pov_size),
			bandwidth: 50 * 1024 * 1024,
			peer_bandwidth: 50 * 1024 * 1024,
			// No latency
			latency: None,
			num_blocks,
			min_pov_size,
			max_pov_size,
			connectivity: 100,
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
			max_validators_per_core: 5,
			pov_sizes: generate_pov_sizes(n_cores, min_pov_size, max_pov_size),
			bandwidth: 50 * 1024 * 1024,
			peer_bandwidth: 50 * 1024 * 1024,
			latency: Some(PeerLatency { mean_latency_ms: 50, std_dev: 12.5 }),
			num_blocks,
			min_pov_size,
			max_pov_size,
			connectivity: 95,
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
			max_validators_per_core: 5,
			pov_sizes: generate_pov_sizes(n_cores, min_pov_size, max_pov_size),
			bandwidth: 50 * 1024 * 1024,
			peer_bandwidth: 50 * 1024 * 1024,
			latency: Some(PeerLatency { mean_latency_ms: 150, std_dev: 40.0 }),
			num_blocks,
			min_pov_size,
			max_pov_size,
			connectivity: 67,
		}
	}
}

/// Sample latency (in milliseconds) from a normal distribution with parameters
/// specified in `maybe_peer_latency`.
pub fn random_latency(maybe_peer_latency: Option<&PeerLatency>) -> usize {
	maybe_peer_latency
		.map(|latency_config| {
			Normal::new(latency_config.mean_latency_ms as f64, latency_config.std_dev)
				.expect("normal distribution parameters are good")
				.sample(&mut thread_rng())
		})
		.unwrap_or(0.0) as usize
}
