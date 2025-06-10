// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! XCM Execution weights for invoking the backend implementation

use frame_support::weights::{constants::RocksDbWeight, Weight};

/// XCM Execution weights for invoking the backend implementation
pub trait BackendWeightInfo {
	/// Execution weight for remote xcm that dispatches `EthereumSystemCall::RegisterToken`
	/// using `Transact`.
	fn transact_register_token() -> Weight;
	fn transact_add_tip() -> Weight;
	fn do_process_message() -> Weight;
	fn commit_single() -> Weight;
	fn submit_delivery_receipt() -> Weight;
}

impl BackendWeightInfo for () {
	fn transact_register_token() -> Weight {
		Weight::from_parts(100_000_000, 10000)
	}
	fn transact_add_tip() -> Weight {
		Weight::from_parts(100_000_000, 10000)
	}
	fn do_process_message() -> Weight {
		Weight::from_parts(39_000_000, 3485)
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(4_u64))
	}
	fn commit_single() -> Weight {
		Weight::from_parts(9_000_000, 1586)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}

	fn submit_delivery_receipt() -> Weight {
		Weight::from_parts(70_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3601))
			.saturating_add(RocksDbWeight::get().reads(2))
			.saturating_add(RocksDbWeight::get().writes(2))
	}
}
