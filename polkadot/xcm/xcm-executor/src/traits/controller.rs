use frame_support::pallet_prelude::*;
use sp_weights::Weight;
use xcm::{v3::XcmHash, VersionedMultiLocation, VersionedXcm};
use crate::traits::QueryResponseStatus;

/// Umbrella trait for all Controller traits.
pub trait Controller<Origin, RuntimeCall, Timeout>: ExecuteController<Origin, RuntimeCall> + SendController<Origin> + QueryController<Origin, Timeout> {}
impl <T, Origin, RuntimeCall, Timeout> Controller<Origin, RuntimeCall, Timeout> for T
where T: ExecuteController<Origin, RuntimeCall> + SendController<Origin> + QueryController<Origin, Timeout>
{}

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

/// Weight functions needed for [`QueryController`].
pub trait QueryControllerWeightInfo {
	/// Weight for [`QueryController::query`]
	fn query() -> Weight;

	/// Weight for [`QueryController::take_response`]
	fn take_response() -> Weight;
}

/// Query a remote location, from a given origin.
pub trait QueryController<Origin, Timeout> {
	type QueryId: Encode + Decode + MaxEncodedLen;
	type BlockNumber: Encode + Decode + MaxEncodedLen;
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

	/// Take an XCM response for the specified query.
	///
	/// - `query_id`: the query id returned by [`Self::query`].
	fn take_response(query_id: Self::QueryId) -> QueryResponseStatus<Self::BlockNumber>;
}

impl<Origin, Timeout> QueryController<Origin, Timeout> for () {
	type QueryId = u64;
	type BlockNumber = u64;
	type WeightInfo = ();
	fn query(
		_origin: Origin,
		_timeout: Timeout,
		_match_querier: VersionedMultiLocation,
	) -> Result<Self::QueryId, DispatchError> {
		Ok(Default::default())
	}
	fn take_response(_query_id: Self::QueryId) -> QueryResponseStatus<Self::BlockNumber> {
		QueryResponseStatus::NotFound
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
