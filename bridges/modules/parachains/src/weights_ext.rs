// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Weight-related utilities.

use crate::weights::{BridgeWeight, WeightInfo};

use bp_runtime::Size;
use frame_support::weights::{RuntimeDbWeight, Weight};

/// Size of the regular parachain head.
///
/// It's not that we are expecting all parachain heads to share the same size or that we would
/// reject all heads that have larger/lesser size. It is about head size that we use in benchmarks.
/// Relayer would need to pay additional fee for extra bytes.
///
/// 384 is a bit larger (1.3 times) than the size of the randomly chosen Polkadot block.
pub const DEFAULT_PARACHAIN_HEAD_SIZE: u32 = 384;

/// Number of extra bytes (excluding size of storage value itself) of storage proof, built at
/// some generic chain.
pub const EXTRA_STORAGE_PROOF_SIZE: u32 = 1024;

/// Extended weight info.
pub trait WeightInfoExt: WeightInfo {
	/// Storage proof overhead, that is included in every storage proof.
	///
	/// The relayer would pay some extra fee for additional proof bytes, since they mean
	/// more hashing operations.
	fn expected_extra_storage_proof_size() -> u32;

	/// Weight of the parachain heads delivery extrinsic.
	fn submit_parachain_heads_weight(
		db_weight: RuntimeDbWeight,
		proof: &impl Size,
		parachains_count: u32,
	) -> Weight {
		// weight of the `submit_parachain_heads` with exactly `parachains_count` parachain
		// heads of the default size (`DEFAULT_PARACHAIN_HEAD_SIZE`)
		let base_weight = Self::submit_parachain_heads_with_n_parachains(parachains_count);

		// overhead because of extra storage proof bytes
		let expected_proof_size = parachains_count
			.saturating_mul(DEFAULT_PARACHAIN_HEAD_SIZE)
			.saturating_add(Self::expected_extra_storage_proof_size());
		let actual_proof_size = proof.size();
		let proof_size_overhead = Self::storage_proof_size_overhead(
			actual_proof_size.saturating_sub(expected_proof_size),
		);

		// potential pruning weight (refunded if hasn't happened)
		let pruning_weight =
			Self::parachain_head_pruning_weight(db_weight).saturating_mul(parachains_count as u64);

		base_weight.saturating_add(proof_size_overhead).saturating_add(pruning_weight)
	}

	/// Returns weight of single parachain head storage update.
	///
	/// This weight only includes db write operations that happens if parachain head is actually
	/// updated. All extra weights (weight of storage proof validation, additional checks, ...) is
	/// not included.
	fn parachain_head_storage_write_weight(db_weight: RuntimeDbWeight) -> Weight {
		// it's just a couple of operations - we need to write the hash (`ImportedParaHashes`) and
		// the head itself (`ImportedParaHeads`. Pruning is not included here
		db_weight.writes(2)
	}

	/// Returns weight of single parachain head pruning.
	fn parachain_head_pruning_weight(db_weight: RuntimeDbWeight) -> Weight {
		// it's just one write operation, we don't want any benchmarks for that
		db_weight.writes(1)
	}

	/// Returns weight that needs to be accounted when storage proof of given size is received.
	fn storage_proof_size_overhead(extra_proof_bytes: u32) -> Weight {
		let extra_byte_weight = (Self::submit_parachain_heads_with_16kb_proof() -
			Self::submit_parachain_heads_with_1kb_proof()) /
			(15 * 1024);
		extra_byte_weight.saturating_mul(extra_proof_bytes as u64)
	}
}

impl WeightInfoExt for () {
	fn expected_extra_storage_proof_size() -> u32 {
		EXTRA_STORAGE_PROOF_SIZE
	}
}

impl<T: frame_system::Config> WeightInfoExt for BridgeWeight<T> {
	fn expected_extra_storage_proof_size() -> u32 {
		EXTRA_STORAGE_PROOF_SIZE
	}
}
