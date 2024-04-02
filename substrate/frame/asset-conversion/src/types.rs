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

use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use scale_info::TypeInfo;

/// Represents a swap path with associated asset amounts indicating how much of the asset needs to
/// be deposited to get the following asset's amount withdrawn (this is inclusive of fees).
///
/// Example:
/// Given path [(asset1, amount_in), (asset2, amount_out2), (asset3, amount_out3)], can be resolved:
/// 1. `asset(asset1, amount_in)` take from `user` and move to the pool(asset1, asset2);
/// 2. `asset(asset2, amount_out2)` transfer from pool(asset1, asset2) to pool(asset2, asset3);
/// 3. `asset(asset3, amount_out3)` move from pool(asset2, asset3) to `user`.
pub(super) type BalancePath<T> = Vec<(<T as Config>::AssetKind, <T as Config>::Balance)>;

/// Credit of [Config::Assets].
pub type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Assets>;

/// Stores the lp_token asset id a particular pool has been assigned.
#[derive(Decode, Encode, Default, PartialEq, Eq, MaxEncodedLen, TypeInfo)]
pub struct PoolInfo<PoolAssetId> {
	/// Liquidity pool asset
	pub lp_token: PoolAssetId,
}

/// Provides means to resolve the `PoolId` and `AccountId` from a pair of assets.
///
/// Resulting `PoolId` remains consistent whether the asset pair is presented as (asset1, asset2)
/// or (asset2, asset1). The derived `AccountId` may serve as an address for liquidity provider
/// tokens.
pub trait PoolLocator<AccountId, AssetKind, PoolId> {
	/// Retrieves the account address associated with a valid `PoolId`.
	fn address(id: &PoolId) -> Result<AccountId, ()>;
	/// Identifies the `PoolId` for a given pair of assets.
	///
	/// Returns an error if the asset pair isn't supported.
	fn pool_id(asset1: &AssetKind, asset2: &AssetKind) -> Result<PoolId, ()>;
	/// Retrieves the account address associated with a given asset pair.
	///
	/// Returns an error if the asset pair isn't supported.
	fn pool_address(asset1: &AssetKind, asset2: &AssetKind) -> Result<AccountId, ()> {
		if let Ok(id) = Self::pool_id(asset1, asset2) {
			Self::address(&id)
		} else {
			Err(())
		}
	}
}

/// Pool locator that mandates the inclusion of the specified `FirstAsset` in every asset pair.
///
/// The `PoolId` is represented as a tuple of `AssetKind`s with `FirstAsset` always positioned as
/// the first element.
pub struct WithFirstAsset<FirstAsset, AccountId, AssetKind>(
	PhantomData<(FirstAsset, AccountId, AssetKind)>,
);
impl<FirstAsset, AccountId, AssetKind> PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>
	for WithFirstAsset<FirstAsset, AccountId, AssetKind>
where
	AssetKind: Eq + Clone + Encode,
	AccountId: Decode,
	FirstAsset: Get<AssetKind>,
{
	fn pool_id(asset1: &AssetKind, asset2: &AssetKind) -> Result<(AssetKind, AssetKind), ()> {
		let first = FirstAsset::get();
		match true {
			_ if asset1 == asset2 => Err(()),
			_ if first == *asset1 => Ok((first, asset2.clone())),
			_ if first == *asset2 => Ok((first, asset1.clone())),
			_ => Err(()),
		}
	}
	fn address(id: &(AssetKind, AssetKind)) -> Result<AccountId, ()> {
		let encoded = sp_io::hashing::blake2_256(&Encode::encode(id)[..]);
		Decode::decode(&mut TrailingZeroInput::new(encoded.as_ref())).map_err(|_| ())
	}
}

/// Pool locator where the `PoolId` is a tuple of `AssetKind`s arranged in ascending order.
pub struct Ascending<AccountId, AssetKind>(PhantomData<(AccountId, AssetKind)>);
impl<AccountId, AssetKind> PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>
	for Ascending<AccountId, AssetKind>
where
	AssetKind: Ord + Clone + Encode,
	AccountId: Decode,
{
	fn pool_id(asset1: &AssetKind, asset2: &AssetKind) -> Result<(AssetKind, AssetKind), ()> {
		match true {
			_ if asset1 > asset2 => Ok((asset2.clone(), asset1.clone())),
			_ if asset1 < asset2 => Ok((asset1.clone(), asset2.clone())),
			_ => Err(()),
		}
	}
	fn address(id: &(AssetKind, AssetKind)) -> Result<AccountId, ()> {
		let encoded = sp_io::hashing::blake2_256(&Encode::encode(id)[..]);
		Decode::decode(&mut TrailingZeroInput::new(encoded.as_ref())).map_err(|_| ())
	}
}

/// Pool locator that chains the `First` and `Second` implementations of [`PoolLocator`].
///
/// If the `First` implementation fails, it falls back to the `Second`.
pub struct Chain<First, Second>(PhantomData<(First, Second)>);
impl<First, Second, AccountId, AssetKind> PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>
	for Chain<First, Second>
where
	First: PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>,
	Second: PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>,
{
	fn pool_id(asset1: &AssetKind, asset2: &AssetKind) -> Result<(AssetKind, AssetKind), ()> {
		First::pool_id(asset1, asset2).or(Second::pool_id(asset1, asset2))
	}
	fn address(id: &(AssetKind, AssetKind)) -> Result<AccountId, ()> {
		First::address(id).or(Second::address(id))
	}
}
