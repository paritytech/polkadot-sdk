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
	/// Returns the fixed part `a` of finalize_block weight.
	fn on_finalize_block_fixed() -> Weight;

	/// Returns the per-transaction part `b` of finalize_block weight.
	fn on_finalize_block_per_tx(payload_size: u32) -> Weight;

	/// Returns the per-event part `c` of finalize_block weight.
	fn on_finalize_block_per_event(data_len: u32) -> Weight;
}

/// Splits finalize_block weight into fixed, per-transaction, per-event components.
///
/// **Weight Formula:**
/// ```
/// Total weight = fixed_part +
///                Σ(per_tx_part(payload_i)) +
///                Σ(per_event_part(data_len_j))
/// ```
///
/// **Component Functions:**
/// - `on_finalize_block_fixed()`: Fixed overhead added in `on_finalize()`
/// - `on_finalize_block_per_tx(payload_size)`: Per-transaction weight added incrementally in each
///   `eth_call()` to enforce gas limits and reject transactions early if needed.
/// - `on_finalize_block_per_event(data_len)`: Per-event weight for processing events during
///   `on_finalize()`, added dynamically in each `deposit_event()` based on event and its data
///   length.
impl<W: WeightInfo> OnFinalizeBlockParts for W {
	fn on_finalize_block_fixed() -> Weight {
		// Fixed cost is incurred no matter what number of transactions
		W::on_finalize(0, 0)
	}

	fn on_finalize_block_per_tx(payload_size: u32) -> Weight {
		// Cost per transaction: on_finalize(1, payload_size) - fixed_cost
		W::on_finalize(1, payload_size).saturating_sub(W::on_finalize_block_fixed())
	}

	/// Calculate per-event weight including data processing costs.
	///
	/// This uses differential cost calculation to isolate the marginal cost components:
	/// per_event_base + (data_len * per_byte_cost)
	fn on_finalize_block_per_event(data_len: u32) -> Weight {
		// Base cost per event: cost difference between 1 event and 0 events (with no data)
		let per_event_base_cost = W::on_finalize_per_transaction(1, 0)
			.saturating_sub(W::on_finalize_per_transaction(0, 0));

		// Additional cost per byte of event data: cost difference for data_len bytes vs 0 bytes
		let per_byte_cost = if data_len > 0 {
			W::on_finalize_per_transaction(1, data_len)
				.saturating_sub(W::on_finalize_per_transaction(1, 0))
		} else {
			Weight::zero()
		};

		per_event_base_cost.saturating_add(per_byte_cost)
	}
}
