// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! XCM Execution weights for invoking the backend implementation

use frame_support::weights::Weight;

/// XCM Execution weights for invoking the backend implementation
pub trait BackendWeightInfo {
	/// Execution weight for remote xcm that dispatches `EthereumSystemCall::RegisterToken`
	/// using `Transact`.
	fn transact_register_token() -> Weight;
	fn transact_add_tip() -> Weight;
}

impl BackendWeightInfo for () {
	fn transact_register_token() -> Weight {
		Weight::from_parts(100_000_000, 10000)
	}
	fn transact_add_tip() -> Weight {
		Weight::from_parts(100_000_000, 10000)
	}
}
