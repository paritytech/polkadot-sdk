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

//! Trait for grouping asset kinds into named categories (e.g. USD stablecoins).

use crate::BoundedVec;
use core::marker::PhantomData;
use sp_core::{ConstU32, Get};

/// Resolves named asset categories to their member asset kinds and inspects available balances.
///
/// A category groups asset kinds considered interchangeable for payment purposes, allowing a
/// payment of some amount to be fulfilled from any combination of the category's members.
pub trait AssetCategoryManager<AccountId> {
	/// Means of identifying one asset kind from another.
	type AssetKind;
	/// Balance type used for availability checks.
	type Balance;
	/// Maximum length of a category name in bytes.
	type NameLimit: Get<u32>;
	/// Maximum number of asset kinds a category can contain. Must be at least 1.
	type MaxAssets: Get<u32>;

	/// Asset kinds registered under `category`, at most [`Self::MaxAssets`]. Empty if the
	/// category is unknown. Implementations must bound the underlying read by this limit.
	fn assets_in_category(category: &[u8]) -> BoundedVec<Self::AssetKind, Self::MaxAssets>;

	/// Balance of `asset` held by `owner` available for spending, or `None` if it cannot be
	/// determined locally (e.g. the asset lives on another chain).
	fn available_balance(asset: Self::AssetKind, owner: &AccountId) -> Option<Self::Balance>;
}

/// An [`AssetCategoryManager`] with no categories.
///
/// For runtimes that only make specific-asset payments or cannot inspect balances locally.
pub struct NoAssetCategories<AssetKind, Balance>(PhantomData<(AssetKind, Balance)>);
impl<AccountId, AssetKind, Balance> AssetCategoryManager<AccountId>
	for NoAssetCategories<AssetKind, Balance>
{
	type AssetKind = AssetKind;
	type Balance = Balance;
	type NameLimit = ConstU32<32>;
	type MaxAssets = ConstU32<1>;

	fn assets_in_category(_category: &[u8]) -> BoundedVec<Self::AssetKind, Self::MaxAssets> {
		BoundedVec::new()
	}

	fn available_balance(_asset: Self::AssetKind, _owner: &AccountId) -> Option<Self::Balance> {
		None
	}
}
