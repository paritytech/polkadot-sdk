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

use frame_support::traits::{
	tokens::{Pay, PaymentStatus},
	Get,
};
use sp_std::{marker::PhantomData, vec};
use xcm::{opaque::lts::Weight, prelude::*};
use xcm_executor::traits::{QueryHandler, QueryResponseStatus};

/// Implementation of the `frame_support::traits::tokens::Pay` trait, to allow
/// for XCM-based payments of a given `Balance` of some asset ID existing on some chain under
/// ownership of some `Interior` location of the local chain to a particular `Beneficiary`. The
/// `AssetKind` value is not itself bounded (to avoid the issue of needing to wrap some preexisting
/// datatype), however a converter type `AssetKindToLocatableAsset` must be provided in order to
/// translate it into a `LocatableAsset`, which comprises both an XCM `MultiLocation` describing
/// the XCM endpoint on which the asset to be paid resides and an XCM `AssetId` to identify the
/// specific asset at that endpoint.
///
/// `PayOverXcm::pay` is asynchronous, and returns a `QueryId` which can then be used in
/// `check_payment` to check the status of the XCM transaction.
///
/// Only payment in fungible assets is handled.
///
/// The last junction of the beneficiary location will be used as the account and
/// the earlier junctions as the destination.
pub struct PayOverXcm<Interior, Router, Querier, Timeout>(
	PhantomData<(Interior, Router, Querier, Timeout)>,
);
impl<
		Interior: Get<InteriorMultiLocation>,
		Router: SendXcm,
		Querier: QueryHandler,
		Timeout: Get<Querier::BlockNumber>,
	> Pay
	for PayOverXcm<Interior, Router, Querier, Timeout>
{
	type Beneficiary = MultiLocation;
	type AssetKind = AssetId;
	type Balance = u128;
	type Id = Querier::QueryId;
	type Error = xcm::latest::Error;

	fn pay(
		who: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		let (destination, beneficiary) = who.split_last_interior();
		let beneficiary: MultiLocation = beneficiary
			.ok_or(Self::Error::InvalidLocation)?
			.into();
		let return_destination = Querier::UniversalLocation::get()
			.invert_target(&destination)
			.map_err(|()| Self::Error::LocationNotInvertible)?;

		let query_id = Querier::new_query(destination, Timeout::get(), Interior::get());

		let message = Xcm(vec![
			DescendOrigin(Interior::get()),
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			SetAppendix(Xcm(vec![
				SetFeesMode { jit_withdraw: true },
				ReportError(QueryResponseInfo {
					destination: return_destination,
					query_id,
					max_weight: Weight::zero(),
				}),
			])),
			TransferAsset {
				beneficiary,
				assets: (asset_kind, amount).into(),
			},
		]);

		let (ticket, _) = Router::validate(&mut Some(destination), &mut Some(message))?;
		Router::deliver(ticket)?;
		Ok(query_id.into())
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
	fn ensure_successful(_: &Self::Beneficiary, _: Self::AssetKind, _: Self::Balance) {
		// We cannot generally guarantee this will go through successfully since we don't have any
		// control over the XCM transport layers. We just assume that the benchmark environment
		// will be sending it somewhere sensible.
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::Id) {
		Querier::expect_response(id, Response::ExecutionResult(None));
	}
}
