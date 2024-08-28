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

use polkadot_erasure_coding::systematic_recovery_threshold;
use polkadot_primitives::{node_features, ChunkIndex, CoreIndex, NodeFeatures, ValidatorIndex};

/// Compute the per-validator availability chunk index.
/// WARNING: THIS FUNCTION IS CRITICAL TO PARACHAIN CONSENSUS.
/// Any modification to the output of the function needs to be coordinated via the runtime.
/// It's best to use minimal/no external dependencies.
pub fn availability_chunk_index(
	maybe_node_features: Option<&NodeFeatures>,
	n_validators: usize,
	core_index: CoreIndex,
	validator_index: ValidatorIndex,
) -> Result<ChunkIndex, polkadot_erasure_coding::Error> {
	if let Some(features) = maybe_node_features {
		if let Some(&true) = features
			.get(usize::from(node_features::FeatureIndex::AvailabilityChunkMapping as u8))
			.as_deref()
		{
			let systematic_threshold = systematic_recovery_threshold(n_validators)? as u32;
			let core_start_pos = core_index.0 * systematic_threshold;

			return Ok(ChunkIndex((core_start_pos + validator_index.0) % n_validators as u32))
		}
	}

	Ok(validator_index.into())
}

/// Compute the per-core availability chunk indices. Returns a Vec which maps ValidatorIndex to
/// ChunkIndex for a given availability core index
/// WARNING: THIS FUNCTION IS CRITICAL TO PARACHAIN CONSENSUS.
/// Any modification to the output of the function needs to be coordinated via the
/// runtime. It's best to use minimal/no external dependencies.
pub fn availability_chunk_indices(
	maybe_node_features: Option<&NodeFeatures>,
	n_validators: usize,
	core_index: CoreIndex,
) -> Result<Vec<ChunkIndex>, polkadot_erasure_coding::Error> {
	let identity = (0..n_validators).map(|index| ChunkIndex(index as u32));
	if let Some(features) = maybe_node_features {
		if let Some(&true) = features
			.get(usize::from(node_features::FeatureIndex::AvailabilityChunkMapping as u8))
			.as_deref()
		{
			let systematic_threshold = systematic_recovery_threshold(n_validators)? as u32;
			let core_start_pos = core_index.0 * systematic_threshold;

			return Ok(identity
				.into_iter()
				.cycle()
				.skip(core_start_pos as usize)
				.take(n_validators)
				.collect())
		}
	}

	Ok(identity.collect())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashSet;

	pub fn node_features_with_mapping_enabled() -> NodeFeatures {
		let mut node_features = NodeFeatures::new();
		node_features
			.resize(node_features::FeatureIndex::AvailabilityChunkMapping as usize + 1, false);
		node_features
			.set(node_features::FeatureIndex::AvailabilityChunkMapping as u8 as usize, true);
		node_features
	}

	pub fn node_features_with_other_bits_enabled() -> NodeFeatures {
		let mut node_features = NodeFeatures::new();
		node_features.resize(node_features::FeatureIndex::FirstUnassigned as usize + 1, true);
		node_features
			.set(node_features::FeatureIndex::AvailabilityChunkMapping as u8 as usize, false);
		node_features
	}

	#[test]
	fn test_availability_chunk_indices() {
		let n_validators = 20u32;
		let n_cores = 15u32;

		// If the mapping feature is not enabled, it should always be the identity vector.
		{
			for node_features in
				[None, Some(NodeFeatures::EMPTY), Some(node_features_with_other_bits_enabled())]
			{
				for core_index in 0..n_cores {
					let indices = availability_chunk_indices(
						node_features.as_ref(),
						n_validators as usize,
						CoreIndex(core_index),
					)
					.unwrap();

					for validator_index in 0..n_validators {
						assert_eq!(
							indices[validator_index as usize],
							availability_chunk_index(
								node_features.as_ref(),
								n_validators as usize,
								CoreIndex(core_index),
								ValidatorIndex(validator_index)
							)
							.unwrap()
						)
					}

					assert_eq!(
						indices,
						(0..n_validators).map(|i| ChunkIndex(i)).collect::<Vec<_>>()
					);
				}
			}
		}

		// Test when mapping feature is enabled.
		{
			let node_features = node_features_with_mapping_enabled();
			let mut previous_indices = None;

			for core_index in 0..n_cores {
				let indices = availability_chunk_indices(
					Some(&node_features),
					n_validators as usize,
					CoreIndex(core_index),
				)
				.unwrap();

				for validator_index in 0..n_validators {
					assert_eq!(
						indices[validator_index as usize],
						availability_chunk_index(
							Some(&node_features),
							n_validators as usize,
							CoreIndex(core_index),
							ValidatorIndex(validator_index)
						)
						.unwrap()
					)
				}

				// Check that it's not equal to the previous core's indices.
				if let Some(previous_indices) = previous_indices {
					assert_ne!(previous_indices, indices);
				}

				previous_indices = Some(indices.clone());

				// Check that it's indeed a permutation.
				assert_eq!(
					(0..n_validators).map(|i| ChunkIndex(i)).collect::<HashSet<_>>(),
					indices.into_iter().collect::<HashSet<_>>()
				);
			}
		}
	}

	#[test]
	// This is just a dummy test that checks the mapping against some hardcoded outputs, to prevent
	// accidental changes to the algorithms.
	fn prevent_changes_to_mapping() {
		let n_validators = 7;
		let node_features = node_features_with_mapping_enabled();

		assert_eq!(
			availability_chunk_indices(Some(&node_features), n_validators, CoreIndex(0))
				.unwrap()
				.into_iter()
				.map(|i| i.0)
				.collect::<Vec<u32>>(),
			vec![0, 1, 2, 3, 4, 5, 6]
		);
		assert_eq!(
			availability_chunk_indices(Some(&node_features), n_validators, CoreIndex(1))
				.unwrap()
				.into_iter()
				.map(|i| i.0)
				.collect::<Vec<u32>>(),
			vec![2, 3, 4, 5, 6, 0, 1]
		);
		assert_eq!(
			availability_chunk_indices(Some(&node_features), n_validators, CoreIndex(2))
				.unwrap()
				.into_iter()
				.map(|i| i.0)
				.collect::<Vec<u32>>(),
			vec![4, 5, 6, 0, 1, 2, 3]
		);
		assert_eq!(
			availability_chunk_indices(Some(&node_features), n_validators, CoreIndex(3))
				.unwrap()
				.into_iter()
				.map(|i| i.0)
				.collect::<Vec<u32>>(),
			vec![6, 0, 1, 2, 3, 4, 5]
		);
		assert_eq!(
			availability_chunk_indices(Some(&node_features), n_validators, CoreIndex(4))
				.unwrap()
				.into_iter()
				.map(|i| i.0)
				.collect::<Vec<u32>>(),
			vec![1, 2, 3, 4, 5, 6, 0]
		);
	}
}
