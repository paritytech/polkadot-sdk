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

//! A set of traits that define how a pallet interface with XCM.

use crate::{
	traits::{QueryHandler, QueryResponseStatus},
	InteriorMultiLocation,
	Junctions::Here,
	MultiLocation, Xcm,
};
use frame_support::{pallet_prelude::*, parameter_types};
use sp_weights::Weight;
use xcm::{v3::XcmHash, VersionedMultiLocation, VersionedXcm};

/// Umbrella trait for all Controller traits.
pub trait Controller<Origin, RuntimeCall, Timeout>:
	ExecuteController<Origin, RuntimeCall> + SendController<Origin> + QueryController<Origin, Timeout>
{
}

impl<T, Origin, RuntimeCall, Timeout> Controller<Origin, RuntimeCall, Timeout> for T where
	T: ExecuteController<Origin, RuntimeCall>
		+ SendController<Origin>
		+ QueryController<Origin, Timeout>
{
}

/// Weight functions needed for [`ExecuteController`].
pub trait ExecuteControllerWeightInfo {
	/// Weight for [`ExecuteController::execute`]
	fn execute() -> Weight;
}

/// Execute an XCM locally, for a given origin.
pub trait ExecuteController<Origin, RuntimeCall> {
	type WeightInfo: ExecuteControllerWeightInfo;

	/// Execute an XCM locally.
	///
	/// # Parameters
	///
	/// - `origin`: the origin of the call.
	/// - `message`: the XCM program to be executed.
	/// - `max_weight`: the maximum weight that can be consumed by the execution.
	fn execute(
		origin: Origin,
		message: Box<VersionedXcm<RuntimeCall>>,
		max_weight: Weight,
	) -> DispatchResultWithPostInfo;
}

/// Weight functions needed for [`SendController`].
pub trait SendControllerWeightInfo {
	/// Weight for [`SendController::send`]
	fn send() -> Weight;
}

/// Send an XCM from a given origin.
pub trait SendController<Origin> {
	type WeightInfo: SendControllerWeightInfo;

	/// Send an XCM to be executed by a remote location.
	///
	/// # Parameters
	///
	/// - `origin`: the origin of the call.
	/// - `dest`: the destination of the message.
	/// - `msg`: the XCM to be sent.
	fn send(
		origin: Origin,
		dest: Box<VersionedMultiLocation>,
		message: Box<VersionedXcm<()>>,
	) -> Result<XcmHash, DispatchError>;
}

/// Weight functions needed for [`QueryController`].
pub trait QueryControllerWeightInfo {
	/// Weight for [`QueryController::query`]
	fn query() -> Weight;

	/// Weight for [`QueryController::take_response`]
	fn take_response() -> Weight;
}

/// Query a remote location, from a given origin.
pub trait QueryController<Origin, Timeout>: QueryHandler {
	type WeightInfo: QueryControllerWeightInfo;

	/// Query a remote location.
	///
	/// # Parameters
	///
	/// - `origin`: the origin of the call, used to determine the responder.
	/// - `timeout`: the maximum block number that the query should be responded to.
	/// - `match_querier`: the querier that the query should be responded to.
	fn query(
		origin: Origin,
		timeout: Timeout,
		match_querier: VersionedMultiLocation,
	) -> Result<Self::QueryId, DispatchError>;
}

impl<Origin, RuntimeCall> ExecuteController<Origin, RuntimeCall> for () {
	type WeightInfo = ();
	fn execute(
		_origin: Origin,
		_message: Box<VersionedXcm<RuntimeCall>>,
		_max_weight: Weight,
	) -> DispatchResultWithPostInfo {
		Ok(().into())
	}
}

impl ExecuteControllerWeightInfo for () {
	fn execute() -> Weight {
		Weight::zero()
	}
}

impl<Origin> SendController<Origin> for () {
	type WeightInfo = ();
	fn send(
		_origin: Origin,
		_dest: Box<VersionedMultiLocation>,
		_message: Box<VersionedXcm<()>>,
	) -> Result<XcmHash, DispatchError> {
		Ok(Default::default())
	}
}

impl SendControllerWeightInfo for () {
	fn send() -> Weight {
		Weight::zero()
	}
}

impl<Origin, Timeout> QueryController<Origin, Timeout> for () {
	type WeightInfo = ();

	fn query(
		_origin: Origin,
		_timeout: Timeout,
		_match_querier: VersionedMultiLocation,
	) -> Result<Self::QueryId, DispatchError> {
		Ok(Default::default())
	}
}

parameter_types! {
	pub UniversalLocation: InteriorMultiLocation = Here;
}

impl QueryHandler for () {
	type BlockNumber = u64;
	type Error = ();
	type QueryId = u64;
	type UniversalLocation = UniversalLocation;

	fn take_response(_query_id: Self::QueryId) -> QueryResponseStatus<Self::BlockNumber> {
		QueryResponseStatus::NotFound
	}
	fn new_query(
		_responder: impl Into<MultiLocation>,
		_timeout: Self::BlockNumber,
		_match_querier: impl Into<MultiLocation>,
	) -> Self::QueryId {
		0u64
	}

	fn report_outcome(
		_message: &mut Xcm<()>,
		_responder: impl Into<MultiLocation>,
		_timeout: Self::BlockNumber,
	) -> Result<Self::QueryId, Self::Error> {
		Err(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn expect_response(_id: Self::QueryId, _response: Response) {}
}

impl QueryControllerWeightInfo for () {
	fn query() -> Weight {
		Weight::zero()
	}
	fn take_response() -> Weight {
		Weight::zero()
	}
}
