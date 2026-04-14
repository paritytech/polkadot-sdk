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

//! Primitives for the Dynamic Allocation Pool (DAP).
//!
//! Shared between `pallet-dap` and `pallet-dap-satellite` to ensure
//! both pallets agree on the DAP buffer account derivation and the
//! interface for dispatching transfers to the central DAP.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::PalletId;

/// The [`PalletId`] used to represent the central DAP pallet.
pub const DAP_PALLET_ID: PalletId = PalletId(*b"dap/buff");

/// The [`PalletId`] used to represent the DAP satellite pallet on satellite chains.
pub const DAP_SATELLITE_PALLET_ID: PalletId = PalletId(*b"dap/satl");

/// Sub-account identifier used to derive the DAP staging account.
pub const DAP_STAGING_ACCOUNT_ID: &[u8] = b"staging";

/// Trait for dispatching the transfer to the central DAP.
///
/// Implementations are expected to perform the (withdrawal, send) steps atomically:
/// on failure, any withdrawn funds must be restored.
pub trait SendToDap<AccountId, Balance> {
	/// Transfer `amount` from `source` to the central DAP.
	///
	/// Returns `Ok(())` on success, `Err(())` otherwise.
	/// Implementations are responsible for logging the failure reason internally.
	fn send_native(source: AccountId, amount: Balance) -> Result<(), ()>;
}
