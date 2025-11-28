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
use core::{fmt::Debug, marker::PhantomData};
use frame_support::traits::Get;
use sp_runtime::traits::TryConvert;
use xcm::{latest::Error, opaque::lts::Weight, prelude::*};
use xcm_executor::traits::{FeeManager, FeeReason, QueryHandler, QueryResponseStatus};

pub use frame_support::traits::tokens::transfer::{Transfer, TransferStatus};

const LOG_TARGET: &str = "xcm::transfer_over_xcm";

/// Transfers an asset to a beneficiary on a remote chain via XCM.
///
/// The sender account on the destination chain is derived from the local account. For example:
///
/// `
/// Location::new(1, X2([Parachain(SourceParaId), from_location.interior]))
/// `
///
/// For a more specialized implementation, see [`super::pay::PayOverXcm`].
/// That variant assumes:
/// - The origin chain does **not** pay remote XCM execution fees on the destination chain.
/// - The sender account is fixed to a static `Interior` location of the origin chain.
///
/// The low-level XCM construction and configuration is handled by the generic
/// [`TransferOverXcmHelper`] type.
///
/// **NOTE**: It is assumed that the origin location has free execution on the remote chain
/// the `remote_fee` is `None.
pub struct TransferOverXcm<TransactorRefToLocation, TransferOverXcmHelper>(
	PhantomData<(TransactorRefToLocation, TransferOverXcmHelper)>,
);

impl<TransactorRefToLocation, TransferOverXcmHelper> Transfer
	for TransferOverXcm<TransactorRefToLocation, TransferOverXcmHelper>
where
	TransferOverXcmHelper: TransferOverXcmHelperT<Balance = u128, QueryId = QueryId>,
	TransactorRefToLocation: for<'a> TryConvert<&'a TransferOverXcmHelper::Beneficiary, Location>,
{
	type Balance = u128;
	type Sender = TransferOverXcmHelper::Beneficiary;
	type Beneficiary = TransferOverXcmHelper::Beneficiary;
	type AssetKind = TransferOverXcmHelper::AssetKind;
	type RemoteFeeAsset = Asset;

	type Id = TransferOverXcmHelper::QueryId;
	type Error = Error;

	fn transfer(
		from: &Self::Sender,
		to: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
		remote_fee: Option<Self::RemoteFeeAsset>,
	) -> Result<Self::Id, Self::Error> {
		let from_location = TransactorRefToLocation::try_convert(from).map_err(|error| {
			tracing::error!(target: LOG_TARGET, ?error, "Failed to convert `Sender` to location");
			Error::InvalidLocation
		})?;

		TransferOverXcmHelper::send_remote_transfer_xcm(
			from_location.clone(),
			to,
			asset_kind,
			amount,
			remote_fee,
		)
	}

	fn check_transfer(id: Self::Id) -> TransferStatus {
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

/// Helper type to make a transfer on another chain via XCM.
pub struct TransferOverXcmHelper<
	Router,
	Querier,
	XcmFeeHandler,
	Timeout,
	Transactor,
	AssetKind,
	AssetKindToLocatableAsset,
	BeneficiaryRefToLocation,
>(
	PhantomData<(
		Router,
		Querier,
		XcmFeeHandler,
		Timeout,
		Transactor,
		AssetKind,
		AssetKindToLocatableAsset,
		BeneficiaryRefToLocation,
	)>,
);

/// Helper trait to abstract away the complexities of executing a remote transfer.
pub trait TransferOverXcmHelperT {
	/// The beneficiary of a remote transfer.
	type Beneficiary: Debug + Clone;
	/// The type for the kinds of asset that are going to be paid.
	type AssetKind: Debug + Clone;
	/// The units of the currency.
	type Balance: Debug + Clone;
	/// The query id of an XCM Payment
	type QueryId: Debug + Clone;
	/// Construct and send a remote transfer xcm to the destination chain.
	fn send_remote_transfer_xcm(
		from_location: Location,
		to: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
		remote_fee: Option<Asset>,
	) -> Result<QueryId, Error>;

	fn check_transfer(id: Self::QueryId) -> TransferStatus;
	/// Ensure that a call to `send_remote_transfer_xcm` with the given parameters will be
	/// successful if done immediately after this call. Used in benchmarking code.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		beneficiary: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		balance: Self::Balance,
	);

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::QueryId);
}

impl<
		Router: SendXcm,
		Querier: QueryHandler,
		XcmFeeHandler: FeeManager,
		Timeout: Get<Querier::BlockNumber>,
		Beneficiary: Clone + Debug,
		AssetKind: Clone + Debug,
		AssetKindToLocatableAsset: TryConvert<AssetKind, LocatableAssetId>,
		BeneficiaryRefToLocation: for<'a> TryConvert<&'a Beneficiary, Location>,
	> TransferOverXcmHelperT
	for TransferOverXcmHelper<
		Router,
		Querier,
		XcmFeeHandler,
		Timeout,
		Beneficiary,
		AssetKind,
		AssetKindToLocatableAsset,
		BeneficiaryRefToLocation,
	>
{
	type Beneficiary = Beneficiary;
	type AssetKind = AssetKind;
	type Balance = u128;
	type QueryId = QueryId;

	/// Gets the XCM executing the transfer on the remote chain.
	fn send_remote_transfer_xcm(
		from_location: Location,
		to: &Beneficiary,
		asset_kind: AssetKind,
		amount: Self::Balance,
		remote_fee: Option<Asset>,
	) -> Result<QueryId, Error> {
		let locatable = Self::locatable_asset_id(asset_kind)?;
		let LocatableAssetId { asset_id, location: asset_location } = locatable;

		let origin_location_on_remote = Self::origin_location_on_remote(&asset_location)?;

		let beneficiary = BeneficiaryRefToLocation::try_convert(to).map_err(|error| {
			tracing::debug!(target: LOG_TARGET, ?error, "Failed to convert beneficiary to location");
			Error::InvalidLocation
		})?;

		let query_id = Querier::new_query(
			asset_location.clone(),
			Timeout::get(),
			from_location.interior.clone(),
		);

		let message = match remote_fee {
			None => remote_transfer_xcm_free_execution(
				from_location.clone(),
				origin_location_on_remote,
				beneficiary,
				asset_id,
				amount,
				query_id,
			)?,
			Some(fee_asset) => remote_transfer_xcm_paying_fees(
				from_location.clone(),
				origin_location_on_remote,
				beneficiary,
				asset_id,
				amount,
				fee_asset,
				query_id,
			)?,
		};

		let (ticket, delivery_fees) =
			Router::validate(&mut Some(asset_location), &mut Some(message))?;
		if !XcmFeeHandler::is_waived(Some(&from_location), FeeReason::ChargeFees) &&
			!delivery_fees.is_none()
		{
			// To support this case, we'd need to charge the caller account for the `delivery_fees`
			return Err(Error::NotHoldingFees)
		}
		Router::deliver(ticket)?;

		Ok(query_id)
	}

	fn check_transfer(id: Self::QueryId) -> TransferStatus {
		use QueryResponseStatus::*;
		match Querier::take_response(id) {
			Ready { response, .. } => match response {
				Response::ExecutionResult(None) => TransferStatus::Success,
				Response::ExecutionResult(Some(_)) => TransferStatus::Failure,
				_ => TransferStatus::Unknown,
			},
			Pending { .. } => TransferStatus::InProgress,
			NotFound | UnexpectedVersion => TransferStatus::Unknown,
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_: &Self::Beneficiary, asset_kind: Self::AssetKind, _: Self::Balance) {
		let locatable = AssetKindToLocatableAsset::try_convert(asset_kind).unwrap();
		Router::ensure_successful_delivery(Some(locatable.location));
	}
	/// Ensure that a call to `check_payment` with the given parameters will return either `Success`
	/// or `Failure`.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::QueryId) {
		Querier::expect_response(id, Response::ExecutionResult(None));
	}
}

impl<
		Router,
		Querier: QueryHandler,
		XcmFeeHandler,
		Timeout,
		Beneficiary: Clone + Debug,
		AssetKind: Clone + Debug,
		AssetKindToLocatableAsset: TryConvert<AssetKind, LocatableAssetId>,
		BeneficiaryRefToLocation: for<'a> TryConvert<&'a Beneficiary, Location>,
	>
	TransferOverXcmHelper<
		Router,
		Querier,
		XcmFeeHandler,
		Timeout,
		Beneficiary,
		AssetKind,
		AssetKindToLocatableAsset,
		BeneficiaryRefToLocation,
	>
{
	/// Returns the `from` relative to the asset's location.
	///
	/// This is the account that executes the transfer on the remote chain.
	pub fn from_relative_to_asset_location(
		from: &Beneficiary,
		asset_kind: AssetKind,
	) -> Result<Location, Error> {
		let from_location = BeneficiaryRefToLocation::try_convert(from).map_err(|error| {
			tracing::error!(target: LOG_TARGET, ?error, "Failed to convert from to location");
			Error::InvalidLocation
		})?;

		let locatable = Self::locatable_asset_id(asset_kind)?;

		let origin_location_on_remote = Self::origin_location_on_remote(&locatable.location)?;
		append_from_to_target(from_location, origin_location_on_remote)
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

	fn locatable_asset_id(asset_kind: AssetKind) -> Result<LocatableAssetId, Error> {
		AssetKindToLocatableAsset::try_convert(asset_kind).map_err(|error| {
			tracing::debug!(target: LOG_TARGET, ?error, "Failed to convert asset kind to locatable asset");
			Error::InvalidLocation
		})
	}
}

fn remote_transfer_xcm_paying_fees(
	from_location: Location,
	origin_relative_to_remote: Location,
	beneficiary: Location,
	asset_id: AssetId,
	amount: u128,
	remote_fee: Asset,
	query_id: QueryId,
) -> Result<Xcm<()>, Error> {
	// Transform `from` into Location::new(1, XX([Parachain(source), from.interior }])
	// We need this one for the refunds.
	let from_at_target =
		append_from_to_target(from_location.clone(), origin_relative_to_remote.clone())?;
	tracing::trace!(target: LOG_TARGET, ?from_at_target, "From at target");

	let xcm = Xcm(vec![
		// Transform origin into Location::new(1, X2([Parachain(SourceParaId), from.interior }])
		DescendOrigin(from_location.interior.clone()),
		WithdrawAsset(vec![remote_fee.clone()].into()),
		PayFees { asset: remote_fee },
		SetAppendix(Xcm(vec![
			ReportError(QueryResponseInfo {
				destination: origin_relative_to_remote.clone(),
				query_id,
				max_weight: Weight::MAX,
			}),
			RefundSurplus,
			DepositAsset { assets: AssetFilter::Wild(WildAsset::All), beneficiary: from_at_target },
		])),
		TransferAsset { beneficiary, assets: (asset_id, amount).into() },
	]);

	Ok(xcm)
}

fn remote_transfer_xcm_free_execution(
	from_location: Location,
	origin_relative_to_remote: Location,
	beneficiary: Location,
	asset_id: AssetId,
	amount: u128,
	query_id: QueryId,
) -> Result<Xcm<()>, Error> {
	let xcm = Xcm(vec![
		DescendOrigin(from_location.interior),
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		SetAppendix(Xcm(vec![
			SetFeesMode { jit_withdraw: true },
			ReportError(QueryResponseInfo {
				destination: origin_relative_to_remote,
				query_id,
				max_weight: Weight::zero(),
			}),
		])),
		TransferAsset {
			beneficiary,
			assets: vec![Asset { id: asset_id, fun: Fungibility::Fungible(amount) }].into(),
		},
	]);

	Ok(xcm)
}

fn append_from_to_target(from: Location, target: Location) -> Result<Location, Error> {
	let from_at_target = target.appended_with(from).map_err(|_| Error::LocationFull)?;
	Ok(from_at_target)
}
