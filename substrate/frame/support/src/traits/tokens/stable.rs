// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Traits for stablecoin inter-pallet communication.

/// Trait exposing PSM-reserved issuance capacity to other pallets, scoped to a specific
/// internal asset.
///
/// Implemented by the PSM pallet. Consumers (e.g. the Vaults pallet) query the issuance
/// reserved by the PSM for a given stablecoin so they can size their own available
/// capacity accordingly.
pub trait PsmInterface {
	/// Asset identifier type used by the underlying fungibles backend.
	type AssetId;
	/// The balance type.
	type Balance;

	/// Issuance reserved by the PSM for `asset`. Zero if no PSM is registered for it.
	fn reserved_capacity(asset: Self::AssetId) -> Self::Balance;
}

impl PsmInterface for () {
	type AssetId = ();
	type Balance = u128;

	fn reserved_capacity(_asset: Self::AssetId) -> Self::Balance {
		0
	}
}
