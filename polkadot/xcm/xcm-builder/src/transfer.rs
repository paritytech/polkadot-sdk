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

//! `TransferOverXcm` struct for paying through XCM and getting the status back.

use crate::LocatableAssetId;
use alloc::vec;
use core::marker::PhantomData;
use frame_support::traits::{
	tokens::{transfer::PaysRemoteFee, PaymentStatus},
	Get,
};
use sp_runtime::traits::TryConvert;
use xcm::{latest::Error, opaque::lts::Weight, prelude::*};
use xcm_executor::traits::{FeeManager, FeeReason, QueryHandler, QueryResponseStatus};

pub use frame_support::traits::tokens::transfer::Transfer;

const LOG_TARGET: &str = "xcm::transfer_remote";

/// Abstraction to get a default remote xcm execution fee.
///
/// This might come from some pallet's storage value that is frequently
/// updated with the result of a dry-run execution to make sure that the
/// fee is sensible.
pub trait GetDefaultRemoteFee {
	fn get_default_remote_fee() -> Asset;
}

/// Transfer an asset on a remote chain (in practice this should be only asset hub).
///
/// It is similar to the `PayOverXcm` struct from the polkadot-sdk with the difference
/// that the source account executing the transaction is function parameter.
///
/// The account transferring funds remotely will be for example:
///  * `Location::new(1, XX([Parachain(SourceParaId), from_location.interior ])`
#[allow(clippy::type_complexity)]
pub struct TransferOverXcm<
	Router,
	Querier,
	XcmFeeHandler,
	Timeout,
	Transactors,
	AssetKind,
	AssetKindToLocatableAsset,
	TransactorRefToLocation,
	RemoteFee,
>(
	PhantomData<(
		Router,
		Querier,
		XcmFeeHandler,
		Timeout,
		Transactors,
		AssetKind,
		AssetKindToLocatableAsset,
		TransactorRefToLocation,
		RemoteFee,
	)>,
);
impl<
		Router: SendXcm,
		Querier: QueryHandler,
		XcmFeeHandler: FeeManager,
		Timeout: Get<Querier::BlockNumber>,
		Transactor: Clone + core::fmt::Debug,
		AssetKind: Clone + core::fmt::Debug,
		AssetKindToLocatableAsset: TryConvert<AssetKind, LocatableAssetId>,
		TransactorRefToLocation: for<'a> TryConvert<&'a Transactor, Location>,
		DefaultRemoteFee: GetDefaultRemoteFee,
	> Transfer
	for TransferOverXcm<
		XcmFeeHandler,
		Router,
		Querier,
		Timeout,
		Transactor,
		AssetKind,
		AssetKindToLocatableAsset,
		TransactorRefToLocation,
		DefaultRemoteFee,
	>
{
	type Balance = u128;
	type Sender = Transactor;
	type Beneficiary = Transactor;
	type AssetKind = AssetKind;
	type RemoteFeeAsset = Asset;
	type Id = QueryId;
	type Error = Error;

	fn transfer(
		from: &Self::Sender,
		to: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
		remote_fee: PaysRemoteFee<Self::RemoteFeeAsset>,
	) -> Result<Self::Id, Self::Error> {
		let from_location = TransactorRefToLocation::try_convert(from).map_err(|error| {
			tracing::error!(target: LOG_TARGET, ?error, "Failed to convert from to location");
			Error::InvalidLocation
		})?;

		let (message, asset_location, query_id) = Self::get_remote_transfer_xcm(
			from_location.clone(),
			to,
			asset_kind,
			amount,
			remote_fee,
		)?;

		let (ticket, delivery_fees) =
			Router::validate(&mut Some(asset_location), &mut Some(message))?;
		Router::deliver(ticket)?;

		if !XcmFeeHandler::is_waived(Some(&from_location), FeeReason::ChargeFees) {
			XcmFeeHandler::handle_fee(delivery_fees, None, FeeReason::ChargeFees)
		}

		Ok(query_id)
	}

	fn check_payment(id: Self::Id) -> PaymentStatus {
		use QueryResponseStatus::*;
		match Querier::take_response(id) {
			Ready { response, .. } => match response {
				Response::ExecutionResult(None) => PaymentStatus::Success,
				Response::ExecutionResult(Some(_)) => PaymentStatus::Failure,
				_ => PaymentStatus::Unknown,
			},
			Pending { .. } => PaymentStatus::InProgress,
			NotFound | UnexpectedVersion => PaymentStatus::Unknown,
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		_: &Self::Sender,
		_: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		_: Self::Balance,
	) {
		let locatable = AssetKindToLocatableAsset::try_convert(asset_kind).unwrap();
		Router::ensure_successful_delivery(Some(locatable.location));
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::Id) {
		Querier::expect_response(id, Response::ExecutionResult(None));
	}
}

impl<
		Router: SendXcm,
		Querier: QueryHandler,
		HandleXcmFee: FeeManager,
		Timeout: Get<Querier::BlockNumber>,
		Transactor: Clone + core::fmt::Debug,
		AssetKind: Clone + core::fmt::Debug,
		AssetKindToLocatableAsset: TryConvert<AssetKind, LocatableAssetId>,
		TransactorRefToLocation: for<'a> TryConvert<&'a Transactor, Location>,
		DefaultRemoteFee: GetDefaultRemoteFee,
	>
	TransferOverXcm<
		HandleXcmFee,
		Router,
		Querier,
		Timeout,
		Transactor,
		AssetKind,
		AssetKindToLocatableAsset,
		TransactorRefToLocation,
		DefaultRemoteFee,
	>
{
	/// Gets the XCM executing the transfer on the remote chain.
	pub fn get_remote_transfer_xcm(
		from_location: Location,
		to: &<Self as Transfer>::Beneficiary,
		asset_kind: <Self as Transfer>::AssetKind,
		amount: <Self as Transfer>::Balance,
		remote_fee: PaysRemoteFee<<Self as Transfer>::RemoteFeeAsset>,
	) -> Result<(Xcm<()>, Location, QueryId), Error> {
		let locatable = Self::locatable_asset_id(asset_kind)?;
		let LocatableAssetId { asset_id, location: asset_location } = locatable;

		let origin_location_on_remote = Self::origin_location_on_remote(&asset_location)?;

		let beneficiary = TransactorRefToLocation::try_convert(to).map_err(|error| {
			tracing::debug!(target: "xcm::pay", ?error, "Failed to convert beneficiary to location");
			Error::InvalidLocation
		})?;

		let query_id = Querier::new_query(
			asset_location.clone(),
			Timeout::get(),
			from_location.interior.clone(),
		);

		let message = match remote_fee {
			PaysRemoteFee::No => unimplemented!(),
			PaysRemoteFee::Yes { fee_asset } => remote_transfer_xcm_paying_fees(
				from_location,
				origin_location_on_remote,
				beneficiary,
				asset_id,
				amount,
				fee_asset.unwrap_or_else(DefaultRemoteFee::get_default_remote_fee),
				query_id,
			)?,
		};

		Ok((message, asset_location, query_id))
	}

	/// Returns the `from` relative to the asset's location.
	///
	/// This is the account that executes the transfer on the remote chain.
	pub fn from_on_remote(
		from: &<Self as Transfer>::Sender,
		asset_kind: <Self as Transfer>::AssetKind,
	) -> Result<Location, Error> {
		let from_location = TransactorRefToLocation::try_convert(from).map_err(|error| {
			tracing::error!(target: LOG_TARGET, ?error, "Failed to convert from to location");
			Error::InvalidLocation
		})?;

		let locatable = Self::locatable_asset_id(asset_kind)?;

		let origin_location_on_remote = Self::origin_location_on_remote(&locatable.location)?;
		origin_location_on_remote
			.appended_with(from_location)
			.map_err(|_| Error::LocationFull)
	}

	fn origin_location_on_remote(asset_location: &Location) -> Result<Location, Error> {
		let origin_on_remote =
			Querier::UniversalLocation::get().invert_target(asset_location).map_err(|()| {
				tracing::debug!(target: LOG_TARGET, "Failed to invert asset location");
				Error::LocationNotInvertible
			})?;
		tracing::trace!(target: LOG_TARGET, ?origin_on_remote, "Origin on destination");
		Ok(origin_on_remote)
	}

	fn locatable_asset_id(
		asset_kind: <Self as Transfer>::AssetKind,
	) -> Result<LocatableAssetId, Error> {
		AssetKindToLocatableAsset::try_convert(asset_kind).map_err(|error| {
			tracing::debug!(target: LOG_TARGET, ?error, "Failed to convert asset kind to locatable asset");
			Error::InvalidLocation
		})
	}
}

pub fn remote_transfer_xcm_paying_fees(
	from_location: Location,
	destination: Location,
	beneficiary: Location,
	asset_id: AssetId,
	amount: u128,
	remote_fee: Asset,
	query_id: QueryId,
) -> Result<Xcm<()>, Error> {
	// Transform `from` into Location::new(1, XX([Parachain(source), from.interior }])
	// We need this one for the refunds.
	let from_at_target = append_from_to_target(from_location.clone(), destination.clone())?;
	tracing::trace!(target: LOG_TARGET, ?from_at_target, "From at target");

	let xcm = Xcm(vec![
		// Transform origin into Location::new(1, X2([Parachain(SourceParaId), from.interior }])
		DescendOrigin(from_location.interior.clone()),
		WithdrawAsset(vec![remote_fee.clone()].into()),
		PayFees { asset: remote_fee },
		SetAppendix(Xcm(vec![
			ReportError(QueryResponseInfo {
				destination: destination.clone(),
				query_id,
				max_weight: Weight::zero(),
			}),
			RefundSurplus,
			DepositAsset { assets: AssetFilter::Wild(WildAsset::All), beneficiary: from_at_target },
		])),
		TransferAsset { beneficiary, assets: (asset_id, amount).into() },
	]);

	Ok(xcm)
}

fn append_from_to_target(from: Location, target: Location) -> Result<Location, Error> {
	let from_at_target = target.appended_with(from).map_err(|_| Error::LocationFull)?;
	Ok(from_at_target)
}
