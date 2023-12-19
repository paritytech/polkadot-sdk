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
use scale_info::TypeInfo;
use sp_std::{cmp::Ordering, marker::PhantomData};

/// Pool ID.
///
/// The pool's `AccountId` is derived from this type. Any changes to the type may necessitate a
/// migration.
pub(super) type PoolIdOf<T> = (<T as Config>::MultiAssetId, <T as Config>::MultiAssetId);

/// Represents a swap path with associated asset amounts indicating how much of the asset needs to
/// be deposited to get the following asset's amount withdrawn (this is inclusive of fees).
///
/// Example:
/// Given path [(asset1, amount_in), (asset2, amount_out2), (asset3, amount_out3)], can be resolved:
/// 1. `asset(asset1, amount_in)` take from `user` and move to the pool(asset1, asset2);
/// 2. `asset(asset2, amount_out2)` transfer from pool(asset1, asset2) to pool(asset2, asset3);
/// 3. `asset(asset3, amount_out3)` move from pool(asset2, asset3) to `user`.
pub(super) type BalancePath<T> = Vec<(<T as Config>::MultiAssetId, <T as Config>::Balance)>;

/// Stores the lp_token asset id a particular pool has been assigned.
#[derive(Decode, Encode, Default, PartialEq, Eq, MaxEncodedLen, TypeInfo)]
pub struct PoolInfo<PoolAssetId> {
	/// Liquidity pool asset
	pub lp_token: PoolAssetId,
}

/// A trait that converts between a MultiAssetId and either the native currency or an AssetId.
pub trait MultiAssetIdConverter<MultiAssetId, AssetId> {
	/// Returns the MultiAssetId representing the native currency of the chain.
	fn get_native() -> MultiAssetId;

	/// Returns true if the given MultiAssetId is the native currency.
	fn is_native(asset: &MultiAssetId) -> bool;

	/// If it's not native, returns the AssetId for the given MultiAssetId.
	fn try_convert(asset: &MultiAssetId) -> MultiAssetIdConversionResult<MultiAssetId, AssetId>;
}

/// Result of `MultiAssetIdConverter::try_convert`.
#[cfg_attr(feature = "std", derive(PartialEq, Debug))]
pub enum MultiAssetIdConversionResult<MultiAssetId, AssetId> {
	/// Input asset is successfully converted. Means that converted asset is supported.
	Converted(AssetId),
	/// Means that input asset is the chain's native asset, if it has one, so no conversion (see
	/// `MultiAssetIdConverter::get_native`).
	Native,
	/// Means input asset is not supported for pool.
	Unsupported(MultiAssetId),
}

/// Benchmark Helper
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<AssetId, MultiAssetId> {
	/// Returns an `AssetId` from a given integer.
	fn asset_id(asset_id: u32) -> AssetId;

	/// Returns a `MultiAssetId` from a given integer.
	fn multiasset_id(asset_id: u32) -> MultiAssetId;
}

#[cfg(feature = "runtime-benchmarks")]
impl<AssetId, MultiAssetId> BenchmarkHelper<AssetId, MultiAssetId> for ()
where
	AssetId: From<u32>,
	MultiAssetId: From<u32>,
{
	fn asset_id(asset_id: u32) -> AssetId {
		asset_id.into()
	}

	fn multiasset_id(asset_id: u32) -> MultiAssetId {
		asset_id.into()
	}
}

/// An implementation of MultiAssetId that can be either Native or an asset.
#[derive(Decode, Encode, Default, MaxEncodedLen, TypeInfo, Clone, Copy, Debug)]
pub enum NativeOrAssetId<AssetId>
where
	AssetId: Ord,
{
	/// Native asset. For example, on the Polkadot Asset Hub this would be DOT.
	#[default]
	Native,
	/// A non-native asset id.
	Asset(AssetId),
}

impl<AssetId: Ord> From<AssetId> for NativeOrAssetId<AssetId> {
	fn from(asset: AssetId) -> Self {
		Self::Asset(asset)
	}
}

impl<AssetId: Ord> Ord for NativeOrAssetId<AssetId> {
	fn cmp(&self, other: &Self) -> Ordering {
		match (self, other) {
			(Self::Native, Self::Native) => Ordering::Equal,
			(Self::Native, Self::Asset(_)) => Ordering::Less,
			(Self::Asset(_), Self::Native) => Ordering::Greater,
			(Self::Asset(id1), Self::Asset(id2)) => <AssetId as Ord>::cmp(id1, id2),
		}
	}
}
impl<AssetId: Ord> PartialOrd for NativeOrAssetId<AssetId> {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(<Self as Ord>::cmp(self, other))
	}
}
impl<AssetId: Ord> PartialEq for NativeOrAssetId<AssetId> {
	fn eq(&self, other: &Self) -> bool {
		self.cmp(other) == Ordering::Equal
	}
}
impl<AssetId: Ord> Eq for NativeOrAssetId<AssetId> {}

/// Converts between a MultiAssetId and an AssetId (or the native currency).
pub struct NativeOrAssetIdConverter<AssetId> {
	_phantom: PhantomData<AssetId>,
}

impl<AssetId: Ord + Clone> MultiAssetIdConverter<NativeOrAssetId<AssetId>, AssetId>
	for NativeOrAssetIdConverter<AssetId>
{
	fn get_native() -> NativeOrAssetId<AssetId> {
		NativeOrAssetId::Native
	}

	fn is_native(asset: &NativeOrAssetId<AssetId>) -> bool {
		*asset == Self::get_native()
	}

	fn try_convert(
		asset: &NativeOrAssetId<AssetId>,
	) -> MultiAssetIdConversionResult<NativeOrAssetId<AssetId>, AssetId> {
		match asset {
			NativeOrAssetId::Asset(asset) => MultiAssetIdConversionResult::Converted(asset.clone()),
			NativeOrAssetId::Native => MultiAssetIdConversionResult::Native,
		}
	}
}

/// Credit of [Config::Currency].
///
/// Implies a negative imbalance in the system that can be placed into an account or alter the total
/// supply.
pub type NativeCredit<T> =
	CreditFungible<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

/// Credit (aka negative imbalance) of [Config::Assets].
///
/// Implies a negative imbalance in the system that can be placed into an account or alter the total
/// supply.
pub type AssetCredit<T> =
	CreditFungibles<<T as frame_system::Config>::AccountId, <T as Config>::Assets>;

/// Credit that can be either [`NativeCredit`] or [`AssetCredit`].
///
/// Implies a negative imbalance in the system that can be placed into an account or alter the total
/// supply.
#[derive(RuntimeDebug, Eq, PartialEq)]
pub enum Credit<T: Config> {
	/// Native credit.
	Native(NativeCredit<T>),
	/// Asset credit.
	Asset(AssetCredit<T>),
}

impl<T: Config> From<NativeCredit<T>> for Credit<T> {
	fn from(value: NativeCredit<T>) -> Self {
		Credit::Native(value)
	}
}

impl<T: Config> From<AssetCredit<T>> for Credit<T> {
	fn from(value: AssetCredit<T>) -> Self {
		Credit::Asset(value)
	}
}

impl<T: Config> TryInto<NativeCredit<T>> for Credit<T> {
	type Error = ();
	fn try_into(self) -> Result<NativeCredit<T>, ()> {
		match self {
			Credit::Native(c) => Ok(c),
			_ => Err(()),
		}
	}
}

impl<T: Config> TryInto<AssetCredit<T>> for Credit<T> {
	type Error = ();
	fn try_into(self) -> Result<AssetCredit<T>, ()> {
		match self {
			Credit::Asset(c) => Ok(c),
			_ => Err(()),
		}
	}
}

impl<T: Config> Credit<T> {
	/// Create zero native credit.
	pub fn native_zero() -> Self {
		NativeCredit::<T>::zero().into()
	}

	/// Amount of `self`.
	pub fn peek(&self) -> T::Balance {
		match self {
			Credit::Native(c) => c.peek(),
			Credit::Asset(c) => c.peek(),
		}
	}

	/// Asset class of `self`.
	pub fn asset(&self) -> T::MultiAssetId {
		match self {
			Credit::Native(_) => T::MultiAssetIdConverter::get_native(),
			Credit::Asset(c) => c.asset().into(),
		}
	}

	/// Consume `self` and return two independent instances; the first is guaranteed to be at most
	/// `amount` and the second will be the remainder.
	pub fn split(self, amount: T::Balance) -> (Self, Self) {
		match self {
			Credit::Native(c) => {
				let (left, right) = c.split(amount);
				(left.into(), right.into())
			},
			Credit::Asset(c) => {
				let (left, right) = c.split(amount);
				(left.into(), right.into())
			},
		}
	}
}
