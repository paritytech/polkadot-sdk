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
//! Controller traits defined in this module are high-level traits that will rely on other traits
//! from `xcm-executor` to perform their tasks.

use frame_support::{
	dispatch::{DispatchErrorWithPostInfo, WithPostDispatchInfo},
	pallet_prelude::DispatchError,
	parameter_types, BoundedVec,
};
use sp_std::boxed::Box;
use xcm::prelude::*;
pub use xcm_executor::traits::QueryHandler;

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
	/// Weight for [`ExecuteController::execute_blob`]
	fn execute_blob() -> Weight;
}

parameter_types! {
	pub const MaxXcmEncodedSize: u32 = xcm::MAX_XCM_ENCODED_SIZE;
}

/// Execute an XCM locally, for a given origin.
///
/// An implementation of that trait will handle the low-level details of the execution, such as:
/// - Validating and Converting the origin to a Location.
/// - Handling versioning.
/// - Calling  the internal executor, which implements [`ExecuteXcm`].
pub trait ExecuteController<Origin, RuntimeCall> {
	/// Weight information for ExecuteController functions.
	type WeightInfo: ExecuteControllerWeightInfo;

	/// Attempt to execute an XCM locally, returns Ok with the weight consumed if the execution
	/// complete successfully, Err otherwise.
	///
	/// # Parameters
	///
	/// - `origin`: the origin of the call.
	/// - `msg`: the encoded XCM to be executed, should be decodable as a [`VersionedXcm`]
	/// - `max_weight`: the maximum weight that can be consumed by the execution.
	fn execute_blob(
		origin: Origin,
		message: BoundedVec<u8, MaxXcmEncodedSize>,
		max_weight: Weight,
	) -> Result<Weight, DispatchErrorWithPostInfo>;
}

/// Weight functions needed for [`SendController`].
pub trait SendControllerWeightInfo {
	/// Weight for [`SendController::send_blob`]
	fn send_blob() -> Weight;
}

/// Send an XCM from a given origin.
///
/// An implementation of that trait will handle the low-level details of dispatching an XCM, such
/// as:
/// - Validating and Converting the origin to an interior location.
/// - Handling versioning.
/// - Calling the internal router, which implements [`SendXcm`].
pub trait SendController<Origin> {
	/// Weight information for SendController functions.
	type WeightInfo: SendControllerWeightInfo;

	/// Send an XCM to be executed by a remote location.
	///
	/// # Parameters
	///
	/// - `origin`: the origin of the call.
	/// - `dest`: the destination of the message.
	/// - `msg`: the encoded XCM to be sent, should be decodable as a [`VersionedXcm`]
	fn send_blob(
		origin: Origin,
		dest: Box<VersionedLocation>,
		message: BoundedVec<u8, MaxXcmEncodedSize>,
	) -> Result<XcmHash, DispatchError>;
}

/// Weight functions needed for [`QueryController`].
pub trait QueryControllerWeightInfo {
	/// Weight for [`QueryController::query`]
	fn query() -> Weight;

	/// Weight for [`QueryHandler::take_response`]
	fn take_response() -> Weight;
}

/// Query a remote location, from a given origin.
///
/// An implementation of that trait will handle the low-level details of querying a remote location,
/// such as:
/// - Validating and Converting the origin to an interior location.
/// - Handling versioning.
/// - Calling the [`QueryHandler`] to register the query.
pub trait QueryController<Origin, Timeout>: QueryHandler {
	/// Weight information for QueryController functions.
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
		match_querier: VersionedLocation,
	) -> Result<QueryId, DispatchError>;
}

impl<Origin, RuntimeCall> ExecuteController<Origin, RuntimeCall> for () {
	type WeightInfo = ();
	fn execute_blob(
		_origin: Origin,
		_message: BoundedVec<u8, MaxXcmEncodedSize>,
		_max_weight: Weight,
	) -> Result<Weight, DispatchErrorWithPostInfo> {
		Err(DispatchError::Other("ExecuteController::execute_blob not implemented")
			.with_weight(Weight::zero()))
	}
}

impl ExecuteControllerWeightInfo for () {
	fn execute_blob() -> Weight {
		Weight::zero()
	}
}

impl<Origin> SendController<Origin> for () {
	type WeightInfo = ();
	fn send_blob(
		_origin: Origin,
		_dest: Box<VersionedLocation>,
		_message: BoundedVec<u8, MaxXcmEncodedSize>,
	) -> Result<XcmHash, DispatchError> {
		Ok(Default::default())
	}
}

impl SendControllerWeightInfo for () {
	fn send_blob() -> Weight {
		Weight::zero()
	}
}

impl QueryControllerWeightInfo for () {
	fn query() -> Weight {
		Weight::zero()
	}
	fn take_response() -> Weight {
		Weight::zero()
	}
}

impl<Origin, Timeout> QueryController<Origin, Timeout> for () {
	type WeightInfo = ();

	fn query(
		_origin: Origin,
		_timeout: Timeout,
		_match_querier: VersionedLocation,
	) -> Result<QueryId, DispatchError> {
		Ok(Default::default())
	}
}
