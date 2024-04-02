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

use frame_support::traits::fungibles::Inspect;
use super::*;

pub type AssetIdOf<T, I> = <pallet_assets::Pallet<T, I> as Inspect<AccountIdOf<T>>>::AssetId;
pub type AssetBalanceOf<T, I> = <pallet_assets::Pallet<T, I> as Inspect<AccountIdOf<T>>>::Balance;
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// An identifier and balance.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
pub struct IdAmount<Id, Balance> {
	/// An identifier for this item.
	pub id: Id,
	/// Some amount for this item.
	pub amount: Balance,
}
