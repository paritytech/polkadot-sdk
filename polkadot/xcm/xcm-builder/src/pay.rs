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
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! `PayOverXcm` struct for paying through XCM and getting the status back.

use crate::{transfer::TransferOverXcmHelperT, TransferOverXcmHelper};
use core::marker::PhantomData;
use frame_support::traits::{
	tokens::{Pay, PaymentStatus},
	Get,
};
use sp_runtime::traits::TryConvert;
use xcm::prelude::*;
use xcm_executor::traits::WaiveDeliveryFees;

/// Implementation of the `frame_support::traits::tokens::Pay` trait, to allow
/// for XCM-based payments of a given `Balance` of some asset ID existing on some chain under
/// ownership of some `Interior` location of the local chain to a particular `Beneficiary`. The
/// `AssetKind` value is not itself bounded (to avoid the issue of needing to wrap some preexisting
/// datatype), however a converter type `AssetKindToLocatableAsset` must be provided in order to
/// translate it into a `LocatableAsset`, which comprises both an XCM `Location` describing
/// the XCM endpoint on which the asset to be paid resides and an XCM `AssetId` to identify the
/// specific asset at that endpoint.
///
/// This relies on the XCM `TransferAsset` instruction. A trait `BeneficiaryRefToLocation` must be
/// provided in order to convert the `Beneficiary` reference into a location usable by
/// `TransferAsset`.
///
/// `PayOverXcm::pay` is asynchronous, and returns a `QueryId` which can then be used in
/// `check_payment` to check the status of the XCM transaction.
///
/// See also `PayAccountId32OverXcm` which is similar to this except that `BeneficiaryRefToLocation`
/// need not be supplied and `Beneficiary` must implement `Into<[u8; 32]>`.
///
/// The implementation of this type assumes:
///
/// - The sending account on the remote chain is fixed (derived from the `Interior` location),
///   rather than being fully configurable.
/// - The remote chain waives the XCM execution fee (`PaysRemoteFee::No`).
///
/// See also [super::transfer::TransferOverXcm] for a more generic implementation with a flexible
/// sender account on the remote chain, and not making the assumption that the remote XCM execution
/// fee is waived.
pub type PayOverXcm<
	Interior,
	Router,
	Querier,
	Timeout,
	Beneficiary,
	AssetKind,
	AssetKindToLocatableAsset,
	BeneficiaryRefToLocation,
> = PayOverXcmWithHelper<
	Interior,
	TransferOverXcmHelper<
		Router,
		Querier,
		WaiveDeliveryFees,
		Timeout,
		Beneficiary,
		AssetKind,
		AssetKindToLocatableAsset,
		BeneficiaryRefToLocation,
	>,
>;

/// Simpler than [`PayOverXcm`] the low-level XCM configuration is extracted to the
/// `TransferOverXcmHelper` type.
pub struct PayOverXcmWithHelper<Interior, TransferOverXcmHelper>(
	PhantomData<(Interior, TransferOverXcmHelper)>,
);
impl<Interior, TransferOverXcmHelper> Pay for PayOverXcmWithHelper<Interior, TransferOverXcmHelper>
where
	Interior: Get<InteriorLocation>,
	TransferOverXcmHelper: TransferOverXcmHelperT<Balance = u128, QueryId = QueryId>,
{
	type Balance = u128;
	type Beneficiary = TransferOverXcmHelper::Beneficiary;
	type AssetKind = TransferOverXcmHelper::AssetKind;
	type Id = TransferOverXcmHelper::QueryId;
	type Error = xcm::latest::Error;

	fn pay(
		who: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		TransferOverXcmHelper::send_remote_transfer_xcm(
			Interior::get().into(),
			who,
			asset_kind,
			amount,
			None,
		)
	}

	fn check_payment(id: Self::Id) -> PaymentStatus {
		TransferOverXcmHelper::check_transfer(id)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		beneficiary: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		balance: Self::Balance,
	) {
		TransferOverXcmHelper::ensure_successful(beneficiary, asset_kind, balance);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::Id) {
		TransferOverXcmHelper::ensure_concluded(id);
	}
}

/// Specialization of the [`PayOverXcm`] trait to allow `[u8; 32]`-based `AccountId` values to be
/// paid on a remote chain.
///
/// Implementation of the [`frame_support::traits::tokens::Pay`] trait, to allow
/// for XCM payments of a given `Balance` of `AssetKind` existing on a `DestinationChain` under
/// ownership of some `Interior` location of the local chain to a particular `Beneficiary`.
///
/// This relies on the XCM `TransferAsset` instruction. `Beneficiary` must implement
/// `Into<[u8; 32]>` (as 32-byte `AccountId`s generally do), and the actual XCM beneficiary will be
/// the location consisting of a single `AccountId32` junction with an appropriate account and no
/// specific network.
///
/// `PayOverXcm::pay` is asynchronous, and returns a `QueryId` which can then be used in
/// `check_payment` to check the status of the XCM transaction.
pub type PayAccountId32OnChainOverXcm<
	DestinationChain,
	Interior,
	Router,
	Querier,
	Timeout,
	Beneficiary,
	AssetKind,
> = PayOverXcm<
	Interior,
	Router,
	Querier,
	Timeout,
	Beneficiary,
	AssetKind,
	crate::AliasesIntoAccountId32<(), Beneficiary>,
	FixedLocation<DestinationChain>,
>;

/// Simple struct which contains both an XCM `location` and `asset_id` to identify an asset which
/// exists on some chain.
pub struct LocatableAssetId {
	/// The asset's ID.
	pub asset_id: AssetId,
	/// The (relative) location in which the asset ID is meaningful.
	pub location: Location,
}

/// Adapter `struct` which implements a conversion from any `AssetKind` into a [`LocatableAssetId`]
/// value using a fixed `Location` for the `location` field.
pub struct FixedLocation<FixedLocationValue>(core::marker::PhantomData<FixedLocationValue>);
impl<FixedLocationValue: Get<Location>, AssetKind: Into<AssetId>>
	TryConvert<AssetKind, LocatableAssetId> for FixedLocation<FixedLocationValue>
{
	fn try_convert(value: AssetKind) -> Result<LocatableAssetId, AssetKind> {
		Ok(LocatableAssetId { asset_id: value.into(), location: FixedLocationValue::get() })
	}
}
