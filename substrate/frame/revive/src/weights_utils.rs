// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::WeightInfo;
use frame_support::weights::Weight;

pub trait OnFinalizeBlockParts {
	/// Returns the fixed base cost of finalize_block operations.
	///
	/// This represents the constant overhead incurred during `on_finalize()` regardless 
	/// of transaction count or event data. Includes setup costs, storage reads/writes,
	/// and other fixed operations.
	fn on_finalize_block_fixed() -> Weight;

	/// Returns the per-transaction weight cost for finalize_block operations.
	///
	/// This weight is applied incrementally during each `eth_call()` to:
	/// - Enforce gas limits before transaction execution
	/// - Reject transactions early if block capacity would be exceeded  
	/// - Account for transaction-specific processing costs (RLP encoding, etc.)
	///
	/// # Parameters
	/// - `payload_size`: Size of the transaction payload in bytes
	fn on_finalize_block_per_tx(payload_size: u32) -> Weight;

	/// Returns the per-event weight cost for finalize_block operations.
	///
	/// This weight is applied dynamically during each `deposit_event()` to account for:
	/// - Event processing overhead (bloom filter updates, log conversion)
	/// - Data-dependent costs (RLP encoding scales with event data size)
	/// - Storage operations for event persistence
	///
	/// Uses differential cost calculation: `per_event_base + (data_len * per_byte_cost)`
	///
	/// # Parameters  
	/// - `data_len`: Total bytes of event data (includes topics and data field)
	fn on_finalize_block_per_event(data_len: u32) -> Weight;
}

/// Implementation of `OnFinalizeBlockParts` that derives high-level weights from `WeightInfo` benchmarks.
///
/// **Weight Formula:**
/// ```text
/// Total weight = fixed_part + Σ(per_tx_part(payload_i)) + Σ(per_event_part(data_len_j))
/// ```
///
/// Uses differential calculation to isolate marginal costs from benchmark measurements.
impl<W: WeightInfo> OnFinalizeBlockParts for W {
	fn on_finalize_block_fixed() -> Weight {
		// Fixed cost is incurred no matter what number of transactions
		W::on_finalize(0, 0)
	}

	fn on_finalize_block_per_tx(payload_size: u32) -> Weight {
		// Cost per transaction: on_finalize(1, payload_size) - fixed_cost
		W::on_finalize(1, payload_size).saturating_sub(W::on_finalize_block_fixed())
	}

	fn on_finalize_block_per_event(data_len: u32) -> Weight {
		// Base cost per event: cost difference between 1 event and 0 events
		let per_event_base_cost = W::on_finalize_per_event(1)
			.saturating_sub(W::on_finalize_per_event(0));

		// Additional cost per byte of event data
		let per_byte_cost = if data_len > 0 {
			W::on_finalize_per_event_data(data_len)
				.saturating_sub(W::on_finalize_per_event_data(0))
		} else {
			Weight::zero()
		};

		per_event_base_cost.saturating_add(per_byte_cost)
	}
}
