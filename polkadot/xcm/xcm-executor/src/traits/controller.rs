use crate::traits::QueryHandler;
use frame_support::pallet_prelude::*;
use sp_weights::Weight;
use xcm::{VersionedMultiLocation, VersionedXcm};

pub trait ControllerWeightInfo {
	fn execute() -> Weight;
	fn send() -> Weight;
}

impl ControllerWeightInfo for () {
	fn execute() -> Weight {
		Weight::zero()
	}
	fn send() -> Weight {
		Weight::zero()
	}
}

pub trait Controller<Origin, RuntimeCall> {
	type WeightInfo: ControllerWeightInfo;

	fn execute(
		origin: Origin,
		message: Box<VersionedXcm<RuntimeCall>>,
		max_weight: Weight,
	) -> DispatchResultWithPostInfo;

	fn send(
		origin: Origin,
		dest: Box<VersionedMultiLocation>,
		message: Box<VersionedXcm<()>>,
	) -> DispatchResult;
}

pub trait QueryControllerWeightInfo {
	fn query() -> Weight;
}

impl QueryControllerWeightInfo for () {
	fn query() -> Weight {
		Weight::zero()
	}
}

pub trait QueryController<Origin, Timeout>: QueryHandler {
	type WeightInfo: QueryControllerWeightInfo;
	fn query(
		origin: Origin,
		timeout: Timeout,
		match_querier: VersionedMultiLocation,
	) -> Result<Self::QueryId, DispatchError>;
}
