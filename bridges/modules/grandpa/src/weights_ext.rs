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

use frame_support::weights::Weight;

/// Extended weight info.
pub trait WeightInfoExt: WeightInfo {
	// Our configuration assumes that the runtime has special signed extensions used to:
	//
	// 1) boost priority of `submit_finality_proof` transactions;
	//
	// 2) slash relayer if he submits an invalid transaction.
	//
	// We read and update storage values of other pallets (`pallet-bridge-relayers` and
	// balances/assets pallet). So we need to add this weight to the weight of our call.
	// Hence two following methods.

	/// Extra weight that is added to the `submit_finality_proof` call weight by signed extensions
	/// that are declared at runtime level.
	fn submit_finality_proof_overhead_from_runtime() -> Weight;

	// Functions that are directly mapped to extrinsics weights.

	/// Weight of message delivery extrinsic.
	fn submit_finality_proof_weight(precommits_len: u32, votes_ancestries_len: u32) -> Weight {
		let base_weight = Self::submit_finality_proof(precommits_len, votes_ancestries_len);
		base_weight.saturating_add(Self::submit_finality_proof_overhead_from_runtime())
	}
}

impl<T: frame_system::Config> WeightInfoExt for BridgeWeight<T> {
	fn submit_finality_proof_overhead_from_runtime() -> Weight {
		Weight::zero()
	}
}

impl WeightInfoExt for () {
	fn submit_finality_proof_overhead_from_runtime() -> Weight {
		Weight::zero()
	}
}
