use crate::traits::QueryHandler;
use frame_support::pallet_prelude::*;
use sp_weights::Weight;
use xcm::{v3::XcmHash, VersionedMultiLocation, VersionedXcm};

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
	) -> Result<XcmHash, DispatchError>;
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
