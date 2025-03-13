// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::{
	fungibles,
	tokens::{PaymentStatus, Preservation},
};
use polkadot_runtime_common::impls::VersionedLocatableAsset;
use sp_runtime::{traits::TypedGet, DispatchError, RuntimeDebug};
use xcm::latest::prelude::*;
use xcm_executor::traits::ConvertLocation;

/// Versioned locatable account type which contains both an XCM `location` and `account_id` to
/// identify an account which exists on some chain.
#[derive(
	Encode, Decode, Eq, PartialEq, Clone, RuntimeDebug, scale_info::TypeInfo, MaxEncodedLen,
)]
pub enum VersionedLocatableAccount {
	#[codec(index = 4)]
	V4 { location: xcm::v4::Location, account_id: xcm::v4::Location },
	#[codec(index = 5)]
	V5 { location: xcm::v5::Location, account_id: xcm::v5::Location },
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
		let who = Self::match_location(who).map_err(|_| DispatchError::Unavailable)?;
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

impl<A, F, C> LocalPay<F, A, C>
where
	A: TypedGet,
	F: fungibles::Mutate<A::Type> + fungibles::Create<A::Type>,
	C: ConvertLocation<A::Type>,
	A::Type: Eq + Clone,
{
	fn match_location(who: &VersionedLocatableAccount) -> Result<A::Type, ()> {
		// only applicable for the local accounts
		let account_id = match who {
			VersionedLocatableAccount::V4 { location, account_id } if location.is_here() =>
				&account_id.clone().try_into().map_err(|_| ())?,
			VersionedLocatableAccount::V5 { location, account_id } if location.is_here() =>
				account_id,
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
