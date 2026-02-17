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
	/// This weight combines the cost of processing one additional transaction with
	/// the cost of its payload data. Uses results from two separate benchmarks:
	/// - `on_finalize_per_transaction`: measures transaction count scaling
	/// - `on_finalize_per_transaction_data`: measures payload size scaling
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

/// Implementation of `OnFinalizeBlockParts` that derives high-level weights from `WeightInfo`
/// benchmarks.
///
/// This implementation solves the linear dependency problem by using separate benchmarks
/// for transaction count and payload size, then combining them mathematically.
///
/// **Weight Formula:**
/// ```text
/// Total weight = fixed_part + Σ(per_tx_part(payload_i)) + Σ(per_event_part(data_len_j))
/// where:
///   per_tx_part(payload) = tx_marginal_cost + (payload × byte_marginal_cost)
/// ```
///
/// **Benchmark Sources:**
/// - Fixed cost: `on_finalize_per_transaction(0)`
/// - Per-transaction cost: `on_finalize_per_transaction(1) - on_finalize_per_transaction(0)`
/// - Per-byte cost: `on_finalize_per_transaction_data(1) - on_finalize_per_transaction_data(0)`
///
/// Uses differential calculation to isolate marginal costs from benchmark measurements.
impl<W: WeightInfo> OnFinalizeBlockParts for W {
	fn on_finalize_block_fixed() -> Weight {
		// Fixed cost: baseline finalization cost with zero transactions
		// Uses the transaction count benchmark at n=0 to capture setup overhead
		W::on_finalize_per_transaction(0)
	}

	fn on_finalize_block_per_tx(payload_size: u32) -> Weight {
		// Calculate marginal cost of adding one transaction with given payload size
		// Combines results from two linearly independent benchmarks:

		// 1. Transaction count cost: marginal cost of one additional transaction
		let per_tx_cost =
			W::on_finalize_per_transaction(1).saturating_sub(W::on_finalize_per_transaction(0));

		// 2. Payload size cost: marginal cost per byte of transaction data
		let payload_cost = if payload_size > 0 {
			let per_byte_cost = W::on_finalize_per_transaction_data(1)
				.saturating_sub(W::on_finalize_per_transaction_data(0));
			per_byte_cost.saturating_mul(payload_size as u64)
		} else {
			Weight::zero()
		};

		per_tx_cost.saturating_add(payload_cost)
	}

	fn on_finalize_block_per_event(data_len: u32) -> Weight {
		// Calculate marginal cost of adding one event with given data length
		// Combines results from two linearly independent benchmarks:

		// 1. Event count cost: marginal cost of one additional event
		let per_event_cost =
			W::on_finalize_per_event(1).saturating_sub(W::on_finalize_per_event(0));

		// 2. Event data cost: marginal cost per byte of event data
		let data_cost = if data_len > 0 {
			let per_byte_cost = W::on_finalize_per_event_data(data_len)
				.saturating_sub(W::on_finalize_per_event_data(0));
			per_byte_cost
		} else {
			Weight::zero()
		};

		per_event_cost.saturating_add(data_cost)
	}
}
