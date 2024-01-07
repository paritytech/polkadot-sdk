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

use polkadot_node_primitives::BabeRandomness;
use polkadot_primitives::{
	vstaging::{node_features, NodeFeatures},
	BlockNumber, ChunkIndex, Id as ParaId, SessionIndex, ValidatorIndex,
};
use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use schnellru::{ByLength, LruMap};

/// Object used for holding and computing assigned chunk indices for validators.
pub struct ChunkIndexCacheRegistry(
	LruMap<(BlockNumber, SessionIndex), (Vec<ChunkIndex>, Option<NodeFeatures>)>,
);

impl ChunkIndexCacheRegistry {
	/// Initialize with the cache capacity.
	pub fn new(capacity: u32) -> Self {
		Self(LruMap::new(ByLength::new(capacity)))
	}

	/// Return the per-validator chunk index if present in the cache.
	pub fn query_cache_for_validator(
		&mut self,
		block_number: BlockNumber,
		session_index: SessionIndex,
		para_id: ParaId,
		validator_index: ValidatorIndex,
	) -> Option<ChunkIndex> {
		if let Some((shuffle, maybe_node_features)) = self.0.get(&(block_number, session_index)) {
			Some(Self::chunk_index_for_validator(
				maybe_node_features.as_ref(),
				shuffle,
				para_id,
				validator_index,
			))
		} else {
			None
		}
	}

	/// Return the per-para chunk index vector if present in the cache.
	pub fn query_cache_for_para(
		&mut self,
		block_number: BlockNumber,
		session_index: SessionIndex,
		para_id: ParaId,
	) -> Option<Vec<ChunkIndex>> {
		if let Some((shuffle, maybe_node_features)) = self.0.get(&(block_number, session_index)) {
			let core_start_index =
				Self::para_start_index(maybe_node_features.as_ref(), shuffle.len(), para_id);

			let chunk_indices = shuffle
				.clone()
				.into_iter()
				.cycle()
				.skip(core_start_index)
				.take(shuffle.len())
				.collect();

			Some(chunk_indices)
		} else {
			None
		}
	}

	/// Return and populate the cache with the per-validator chunk index.
	/// Should only be called if `query_cache_for_validator` returns `None`.
	pub fn populate_for_validator(
		&mut self,
		maybe_node_features: Option<NodeFeatures>,
		babe_randomness: BabeRandomness,
		n_validators: usize,
		block_number: BlockNumber,
		session_index: SessionIndex,
		para_id: ParaId,
		validator_index: ValidatorIndex,
	) -> ChunkIndex {
		let shuffle = Self::get_shuffle(
			maybe_node_features.as_ref(),
			block_number,
			babe_randomness,
			n_validators,
		);
		self.0.insert((block_number, session_index), (shuffle, maybe_node_features));

		self.query_cache_for_validator(block_number, session_index, para_id, validator_index)
			.expect("We just inserted the entry.")
	}

	/// Return and populate the cache with the per-para chunk index vector.
	/// Should only be called if `query_cache_for_para` returns `None`.
	pub fn populate_for_para(
		&mut self,
		maybe_node_features: Option<NodeFeatures>,
		babe_randomness: BabeRandomness,
		n_validators: usize,
		block_number: BlockNumber,
		session_index: SessionIndex,
		para_id: ParaId,
	) -> Vec<ChunkIndex> {
		let shuffle = Self::get_shuffle(
			maybe_node_features.as_ref(),
			block_number,
			babe_randomness,
			n_validators,
		);
		self.0.insert((block_number, session_index), (shuffle, maybe_node_features));

		self.query_cache_for_para(block_number, session_index, para_id)
			.expect("We just inserted the entry.")
	}

	fn get_shuffle(
		maybe_node_features: Option<&NodeFeatures>,
		block_number: BlockNumber,
		mut babe_randomness: BabeRandomness,
		n_validators: usize,
	) -> Vec<ChunkIndex> {
		let mut indices: Vec<_> = (0..n_validators)
			.map(|i| ChunkIndex(u32::try_from(i).expect("validator count should not exceed u32")))
			.collect();

		if let Some(features) = maybe_node_features {
			if let Some(&true) = features
				.get(usize::from(node_features::FeatureIndex::AvailabilityChunkShuffling as u8))
				.as_deref()
			{
				let block_number_bytes = block_number.to_be_bytes();
				for i in 0..32 {
					babe_randomness[i] ^= block_number_bytes[i % block_number_bytes.len()];
				}

				let mut rng: ChaCha8Rng = SeedableRng::from_seed(babe_randomness);

				indices.shuffle(&mut rng);
			}
		}

		indices
	}

	/// Return the availability chunk start index for this para.
	fn para_start_index(
		maybe_node_features: Option<&NodeFeatures>,
		n_validators: usize,
		para_id: ParaId,
	) -> usize {
		if let Some(features) = maybe_node_features {
			if let Some(&true) = features
				.get(usize::from(node_features::FeatureIndex::AvailabilityChunkShuffling as u8))
				.as_deref()
			{
				let mut rng: ChaCha8Rng =
					SeedableRng::from_seed(
						u32::from(para_id).to_be_bytes().repeat(8).try_into().expect(
							"vector of 32 bytes is safe to cast to array of 32 bytes. qed.",
						),
					);
				return rng.gen_range(0..n_validators)
			}
		}

		0
	}

	fn chunk_index_for_validator(
		maybe_node_features: Option<&NodeFeatures>,
		shuffle: &Vec<ChunkIndex>,
		para_id: ParaId,
		validator_index: ValidatorIndex,
	) -> ChunkIndex {
		let core_start_index = Self::para_start_index(maybe_node_features, shuffle.len(), para_id);

		let chunk_index = shuffle[(core_start_index +
			usize::try_from(validator_index.0)
				.expect("usize is at least u32 bytes on all modern targets.")) %
			shuffle.len()];
		chunk_index
	}
}

/// Compute the per-validator availability chunk index.
/// It's preferred to use the `ChunkIndexCacheRegistry` if you also need a cache.
pub fn availability_chunk_index(
	maybe_node_features: Option<&NodeFeatures>,
	babe_randomness: BabeRandomness,
	n_validators: usize,
	block_number: BlockNumber,
	para_id: ParaId,
	validator_index: ValidatorIndex,
) -> ChunkIndex {
	let shuffle = ChunkIndexCacheRegistry::get_shuffle(
		maybe_node_features,
		block_number,
		babe_randomness,
		n_validators,
	);

	ChunkIndexCacheRegistry::chunk_index_for_validator(
		maybe_node_features,
		&shuffle,
		para_id,
		validator_index,
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashSet;

	pub fn node_features_with_shuffling() -> NodeFeatures {
		let mut node_features = NodeFeatures::new();
		node_features
			.resize(node_features::FeatureIndex::AvailabilityChunkShuffling as usize + 1, false);
		node_features
			.set(node_features::FeatureIndex::AvailabilityChunkShuffling as u8 as usize, true);
		node_features
	}

	#[test]
	fn test_availability_chunk_indices() {
		let block_number = 89;
		let n_validators = 11u32;
		let babe_randomness = [12u8; 32];
		let session_index = 0;
		let n_paras = 5u32;

		// Test the `_for_validator` methods
		{
			let para_id = 2.into();
			let mut index_registry = ChunkIndexCacheRegistry::new(2);

			for validator in 0..n_validators {
				assert!(index_registry
					.query_cache_for_validator(
						block_number,
						session_index,
						para_id,
						validator.into()
					)
					.is_none());
			}

			for validator in 0..n_validators {
				// Check that if the node feature is not set, we'll always return the validator
				// index.
				let chunk_index = index_registry.populate_for_validator(
					None,
					babe_randomness,
					n_validators as usize,
					block_number,
					session_index,
					para_id,
					validator.into(),
				);
				assert_eq!(
					index_registry
						.query_cache_for_validator(
							block_number,
							session_index,
							para_id,
							validator.into()
						)
						.unwrap(),
					chunk_index
				);
				assert_eq!(chunk_index.0, validator);
				assert_eq!(
					chunk_index,
					availability_chunk_index(
						None,
						babe_randomness,
						n_validators as usize,
						block_number,
						para_id,
						validator.into(),
					)
				);

				// Check for when the node feature is set.
				let chunk_index = index_registry.populate_for_validator(
					Some(node_features_with_shuffling()),
					babe_randomness,
					n_validators as usize,
					block_number,
					session_index,
					para_id,
					validator.into(),
				);
				assert_eq!(
					index_registry
						.query_cache_for_validator(
							block_number,
							session_index,
							para_id,
							validator.into()
						)
						.unwrap(),
					chunk_index
				);
				assert_ne!(chunk_index.0, validator);
				assert_eq!(
					chunk_index,
					availability_chunk_index(
						Some(&node_features_with_shuffling()),
						babe_randomness,
						n_validators as usize,
						block_number,
						para_id,
						validator.into(),
					)
				);
			}
		}

		// Test the `_for_para` methods
		{
			let mut index_registry = ChunkIndexCacheRegistry::new(2);

			for para in 0..n_paras {
				assert!(index_registry
					.query_cache_for_para(block_number, session_index, para.into())
					.is_none());
			}

			for para in 0..n_paras {
				// Check that if the node feature is not set, we'll always return the identity
				// vector.
				let chunk_indices = index_registry.populate_for_para(
					None,
					babe_randomness,
					n_validators as usize,
					block_number,
					session_index,
					para.into(),
				);
				assert_eq!(
					index_registry
						.query_cache_for_para(block_number, session_index, para.into())
						.unwrap(),
					chunk_indices
				);
				assert_eq!(
					chunk_indices,
					(0..n_validators).map(|i| ChunkIndex(i)).collect::<Vec<_>>()
				);

				for validator in 0..n_validators {
					assert_eq!(
						availability_chunk_index(
							None,
							babe_randomness,
							n_validators as usize,
							block_number,
							para.into(),
							validator.into(),
						),
						chunk_indices[validator as usize]
					);
				}

				// Check for when the node feature is set.
				let chunk_indices = index_registry.populate_for_para(
					Some(node_features_with_shuffling()),
					babe_randomness,
					n_validators as usize,
					block_number,
					session_index,
					para.into(),
				);
				assert_eq!(
					index_registry
						.query_cache_for_para(block_number, session_index, para.into())
						.unwrap(),
					chunk_indices
				);
				assert_eq!(chunk_indices.len(), n_validators as usize);
				assert_ne!(
					chunk_indices,
					(0..n_validators).map(|i| ChunkIndex(i)).collect::<Vec<_>>()
				);
				assert_eq!(
					chunk_indices.iter().collect::<HashSet<_>>().len(),
					n_validators as usize
				);

				for validator in 0..n_validators {
					assert_eq!(
						availability_chunk_index(
							Some(&node_features_with_shuffling()),
							babe_randomness,
							n_validators as usize,
							block_number,
							para.into(),
							validator.into(),
						),
						chunk_indices[validator as usize]
					);
				}
			}
		}
	}
}
