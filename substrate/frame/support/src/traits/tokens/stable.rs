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

/// Trait exposing the PSM pallet's reserved capacity to other pallets.
///
/// Implemented by the PSM pallet, used by the Vaults pallet to account for
/// PSM-reserved debt ceiling when calculating available vault capacity.
pub trait PsmInterface {
	/// The balance type.
	type Balance;

	/// Get the amount of pUSD issuance capacity reserved by the PSM.
	fn reserved_capacity() -> Self::Balance;
}

impl PsmInterface for () {
	type Balance = u128;

	fn reserved_capacity() -> Self::Balance {
		0
	}
}
