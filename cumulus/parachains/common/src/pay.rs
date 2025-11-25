// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::traits::{
	fungibles,
	tokens::{PayWithSource, PaymentStatus, Preservation},
};
use polkadot_runtime_common::impls::VersionedLocatableAsset;
use sp_runtime::{traits::TypedGet, DispatchError, RuntimeDebug};
use xcm::latest::prelude::*;
use xcm_executor::traits::ConvertLocation;

/// Versioned locatable account type which contains both an XCM `location` and `account_id` to
/// identify an account which exists on some chain.
#[derive(
	Encode,
	Decode,
	Eq,
	PartialEq,
	Clone,
	RuntimeDebug,
	scale_info::TypeInfo,
	MaxEncodedLen,
	DecodeWithMemTracking,
)]
pub enum VersionedLocatableAccount {
	#[codec(index = 4)]
	V4 { location: xcm::v4::Location, account_id: xcm::v4::Location },
	#[codec(index = 5)]
	V5 { location: xcm::v5::Location, account_id: xcm::v5::Location },
}

// Implement Convert trait for use with pallet_multi_asset_bounties
/// Converter from `AccountId32` to `VersionedLocatableAccount` for use with
/// `pallet_multi_asset_bounties`.
///
/// # Example
///
///,ignore
/// type FundingSource = PalletIdAsFundingSource<
///     TreasuryPalletId,
///     Runtime,
///     AccountIdToVersionedLocatableAccount
/// >;
/// ///
/// # Warning
/// This conversion fills in default values (location = "here", network = None) which may
/// be incorrect if the account is from another chain or network.
pub struct AccountIdToVersionedLocatableAccount;

impl sp_runtime::traits::Convert<sp_runtime::AccountId32, VersionedLocatableAccount>
	for AccountIdToVersionedLocatableAccount
{
	/// Convert a local account ID into a `VersionedLocatableAccount`.
	///
	/// This assumes the account is on the local chain (`Location::here()`) and has no network
	/// specification (`network: None`). Only use this when you're certain the account is local.
	///
	/// # Warning
	/// This conversion fills in default values (location = "here", network = None) which may
	/// be incorrect if the account is from another chain or network.
	fn convert(account_id: sp_runtime::AccountId32) -> VersionedLocatableAccount {
		VersionedLocatableAccount::V5 {
			location: Location::here(),
			account_id: Location::new(
				0,
				[xcm::v5::Junction::AccountId32 { network: None, id: account_id.into() }],
			),
		}
	}
}

/// Pay on the local chain with `fungibles` implementation if the beneficiary and the asset are both
/// local.
pub struct LocalPay<F, A, C>(core::marker::PhantomData<(F, A, C)>);
impl<A, F, C> frame_support::traits::tokens::Pay for LocalPay<F, A, C>
where
	A: TypedGet,
	F: fungibles::Mutate<A::Type, AssetId = xcm::v5::Location> + fungibles::Create<A::Type>,
	C: ConvertLocation<A::Type>,
	A::Type: Eq + Clone,
{
	type Balance = F::Balance;
	type Beneficiary = VersionedLocatableAccount;
	type AssetKind = VersionedLocatableAsset;
	type Id = QueryId;
	type Error = DispatchError;
	fn pay(
		who: &Self::Beneficiary,
		asset: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		let who = Self::match_location::<A::Type>(who).map_err(|_| DispatchError::Unavailable)?;
		let asset = Self::match_asset(&asset).map_err(|_| DispatchError::Unavailable)?;
		<F as fungibles::Mutate<_>>::transfer(
			asset,
			&A::get(),
			&who,
			amount,
			Preservation::Expendable,
		)?;
		// We use `QueryId::MAX` as a constant identifier for these payments since they are always
		// processed immediately and successfully on the local chain. The `QueryId` type is used to
		// maintain compatibility with XCM payment implementations.
		Ok(Self::Id::MAX)
	}
	fn check_payment(_: Self::Id) -> PaymentStatus {
		PaymentStatus::Success
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_: &Self::Beneficiary, asset: Self::AssetKind, amount: Self::Balance) {
		let asset = Self::match_asset(&asset).expect("invalid asset");
		<F as fungibles::Create<_>>::create(asset.clone(), A::get(), true, amount).unwrap();
		<F as fungibles::Mutate<_>>::mint_into(asset, &A::get(), amount).unwrap();
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(_: Self::Id) {}
}

impl<A, F, C> LocalPay<F, A, C> {
	fn match_location<T>(who: &VersionedLocatableAccount) -> Result<T, ()>
	where
		T: Eq + Clone,
		C: ConvertLocation<T>,
	{
		// only applicable for the local accounts
		let account_id = match who {
			VersionedLocatableAccount::V4 { location, account_id } if location.is_here() =>
				&account_id.clone().try_into().map_err(|_| ())?,
			VersionedLocatableAccount::V5 { location, account_id } if location.is_here() => account_id,
			_ => return Err(()),
		};
		C::convert_location(account_id).ok_or(())
	}

	fn match_asset(asset: &VersionedLocatableAsset) -> Result<xcm::v5::Location, ()> {
		match asset {
			VersionedLocatableAsset::V4 { location, asset_id } if location.is_here() =>
				asset_id.clone().try_into().map(|a: xcm::v5::AssetId| a.0).map_err(|_| ()),
			VersionedLocatableAsset::V5 { location, asset_id } if location.is_here() =>
				Ok(asset_id.clone().0),
			_ => Err(()),
		}
	}
}

// Implement PayWithSource for LocalPay
impl<A, F, C> PayWithSource for LocalPay<F, A, C>
where
	A: Eq + Clone,
	F: fungibles::Mutate<A, AssetId = xcm::v5::Location> + fungibles::Create<A>,
	C: ConvertLocation<A>,
{
	type Balance = F::Balance;
	type Source = VersionedLocatableAccount;
	type Beneficiary = VersionedLocatableAccount;
	type AssetKind = VersionedLocatableAsset;
	type Id = QueryId;
	type Error = DispatchError;
	fn pay(
		source: &Self::Source,
		who: &Self::Beneficiary,
		asset: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		let source =
			Self::match_location::<A>(source).map_err(|_| DispatchError::Unavailable)?;
		let who = Self::match_location::<A>(who).map_err(|_| DispatchError::Unavailable)?;
		let asset = Self::match_asset(&asset).map_err(|_| DispatchError::Unavailable)?;
		<F as fungibles::Mutate<_>>::transfer(
			asset,
			&source,
			&who,
			amount,
			Preservation::Expendable,
		)?;
		// We use `QueryId::MAX` as a constant identifier for these payments since they are always
		// processed immediately and successfully on the local chain. The `QueryId` type is used to
		// maintain compatibility with XCM payment implementations.
		Ok(Self::Id::MAX)
	}
	fn check_payment(_: Self::Id) -> PaymentStatus {
		PaymentStatus::Success
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		source: &Self::Source,
		_: &Self::Beneficiary,
		asset: Self::AssetKind,
		amount: Self::Balance,
	) {
		use sp_runtime::traits::Zero;

		let source = Self::match_location::<A>(source).expect("invalid source");
		let asset = Self::match_asset(&asset).expect("invalid asset");
		if F::total_issuance(asset.clone()).is_zero() {
			<F as fungibles::Create<_>>::create(asset.clone(), source.clone(), true, 1u32.into())
				.unwrap();
		}
		<F as fungibles::Mutate<_>>::mint_into(asset, &source, amount).unwrap();
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(_: Self::Id) {}
}

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarks {
	use super::*;
	use core::marker::PhantomData;
	use frame_support::traits::Get;
	use pallet_multi_asset_bounties::ArgumentsFactory as MultiAssetBountiesArgumentsFactory;
	use pallet_treasury::ArgumentsFactory as TreasuryArgumentsFactory;
	use sp_core::ConstU8;

	/// Provides factory methods for the `AssetKind` and the `Beneficiary` that are applicable for
	/// the payout made by [`LocalPay`].
	///
	/// ### Parameters:
	/// - `PalletId`: The ID of the assets registry pallet.
	pub struct LocalPayArguments<PalletId = ConstU8<0>>(PhantomData<PalletId>);
	impl<PalletId: Get<u8>>
		TreasuryArgumentsFactory<VersionedLocatableAsset, VersionedLocatableAccount>
		for LocalPayArguments<PalletId>
	{
		fn create_asset_kind(seed: u32) -> VersionedLocatableAsset {
			VersionedLocatableAsset::V5 {
				location: Location::new(0, []),
				asset_id: Location::new(
					0,
					[PalletInstance(PalletId::get()), GeneralIndex(seed.into())],
				)
				.into(),
			}
		}
		fn create_beneficiary(seed: [u8; 32]) -> VersionedLocatableAccount {
			VersionedLocatableAccount::V5 {
				location: Location::new(0, []),
				account_id: Location::new(0, [AccountId32 { network: None, id: seed }]),
			}
		}
	}

	/// Provides factory methods for the `AssetKind`, `Source`, and `Beneficiary` that are
	/// applicable for the payout made by [`LocalPay`] when used with `PayWithSource`.
	///
	/// ### Parameters:
	/// - `PalletId`: The ID of the assets registry pallet.
	pub struct LocalPayWithSourceArguments<PalletId = ConstU8<0>>(PhantomData<PalletId>);
	impl<PalletId: Get<u8>, Balance>
		MultiAssetBountiesArgumentsFactory<
			VersionedLocatableAsset,
			VersionedLocatableAccount,
			Balance,
		> for LocalPayWithSourceArguments<PalletId>
	{
		fn create_asset_kind(seed: u32) -> VersionedLocatableAsset {
			VersionedLocatableAsset::V5 {
				location: Location::new(0, []),
				asset_id: Location::new(
					0,
					[PalletInstance(PalletId::get()), GeneralIndex(seed.into())],
				)
				.into(),
			}
		}
		fn create_beneficiary(seed: [u8; 32]) -> VersionedLocatableAccount {
			VersionedLocatableAccount::V5 {
				location: Location::new(0, []),
				account_id: Location::new(0, [AccountId32 { network: None, id: seed }]),
			}
		}
	}
}
