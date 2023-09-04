use crate::{AccountIdOf, Config, Error, RawOrigin};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::DispatchResultWithPostInfo, weights::Weight};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_xcm::WeightInfo as XcmWeightInfo;
use sp_runtime::{DispatchError, DispatchResult};
use xcm::{v3::MultiLocation, VersionedMultiLocation, VersionedXcm};
use xcm_executor::traits::{QueryHandler, QueryResponseStatus};
pub type CallOf<T> = <T as frame_system::Config>::RuntimeCall;

pub trait XCM<T: Config> {
	type QueryId: Encode + Decode + MaxEncodedLen;
	type WeightInfo: WeightInfo;

	/// Execute an XCM message locally. see [`pallet_xcm::execute`]
	///
	/// - `origin`: the origin of the call.
	/// - `message`: the XCM message to be executed.
	/// - `max_weight`: the maximum weight that can be consumed by the execution.
	fn execute(
		origin: &AccountIdOf<T>,
		message: VersionedXcm<CallOf<T>>,
		max_weight: Weight,
	) -> DispatchResultWithPostInfo;

	/// Send an XCM message to be executed by a remote location. see [`pallet_xcm::send`]
	///
	/// - `origin`: the origin of the call.
	/// - `dest`: the destination of the message.
	/// - `msg`: the XCM message to be sent.
	fn send(
		origin: &AccountIdOf<T>,
		dest: VersionedMultiLocation,
		msg: VersionedXcm<()>,
	) -> DispatchResult;

	/// Query a remote location. see [`QueryHandler::new_query`]
	///
	/// - `origin`: the origin of the call, used to determine the responder.
	/// - `timeout`: the maximum block number that the query should be responded to.
	/// - `match_querier`: the querier that the query should be responded to.
	fn query(
		origin: &AccountIdOf<T>,
		timeout: BlockNumberFor<T>,
		match_querier: VersionedMultiLocation,
	) -> Result<Self::QueryId, DispatchError>;
	fn take_response(query_id: Self::QueryId) -> QueryResponseStatus<BlockNumberFor<T>>;
}

/// A no-op implementation of [`XCM`].
pub struct NoopXcmConfig;

impl<T: Config> XCM<T> for NoopXcmConfig {
	type QueryId = ();
	type WeightInfo = Self;
	fn execute(
		_origin: &AccountIdOf<T>,
		_message: VersionedXcm<CallOf<T>>,
		_max_weight: Weight,
	) -> DispatchResultWithPostInfo {
		Err(Error::<T>::XcmDisabled.into()).into()
	}
	fn send(
		_origin: &AccountIdOf<T>,
		_dest: VersionedMultiLocation,
		_msg: VersionedXcm<()>,
	) -> DispatchResult {
		Err(Error::<T>::XcmDisabled.into())
	}
	fn query(
		_origin: &AccountIdOf<T>,
		_timeout: BlockNumberFor<T>,
		_match_querier: VersionedMultiLocation,
	) -> Result<Self::QueryId, DispatchError> {
		Err(Error::<T>::XcmDisabled.into())
	}
	fn take_response(_query_id: Self::QueryId) -> QueryResponseStatus<BlockNumberFor<T>> {
		QueryResponseStatus::UnexpectedVersion
	}
}

/// Weight info trait for methods exposed by the [`XCM`] trait.
pub trait WeightInfo {
	/// Weight of the [`XCM::execute`] function.
	fn execute() -> Weight;
	/// Weight of the [`XCM::send`] function.
	fn send() -> Weight;
	/// Weight of the [`XCM::query`] function.
	fn query() -> Weight;
	/// Weight of the [`XCM::take_response`] function.
	fn take_response() -> Weight;
}

impl WeightInfo for NoopXcmConfig {
	fn execute() -> Weight {
		Weight::zero()
	}
	fn send() -> Weight {
		Weight::zero()
	}
	fn query() -> Weight {
		Weight::zero()
	}
	fn take_response() -> Weight {
		Weight::zero()
	}
}

/// TODO use feature flag to compile this only when xcm is enabled
impl<T> WeightInfo for T
where
	T: pallet_xcm::Config,
{
	fn execute() -> Weight {
		<T as pallet_xcm::Config>::WeightInfo::execute()
	}
	fn send() -> Weight {
		<T as pallet_xcm::Config>::WeightInfo::send()
	}
	fn query() -> Weight {
		<T as pallet_xcm::Config>::WeightInfo::new_query()
	}
	fn take_response() -> Weight {
		<T as pallet_xcm::Config>::WeightInfo::take_response()
	}
}

impl<T: Config> XCM<T> for T
where
	T: pallet_xcm::Config,
{
	type QueryId = <pallet_xcm::Pallet<T> as QueryHandler>::QueryId;
	type WeightInfo = T;

	fn execute(
		origin: &AccountIdOf<T>,
		message: VersionedXcm<CallOf<T>>,
		max_weight: Weight,
	) -> DispatchResultWithPostInfo {
		let origin = RawOrigin::Signed(origin.clone()).into();
		pallet_xcm::Pallet::<T>::execute(origin, Box::new(message), max_weight)
	}
	fn send(
		origin: &AccountIdOf<T>,
		dest: VersionedMultiLocation,
		msg: VersionedXcm<()>,
	) -> DispatchResult {
		let origin = RawOrigin::Signed(origin.clone()).into();
		pallet_xcm::Pallet::<T>::send(origin, Box::new(dest), Box::new(msg))
	}

	fn query(
		origin: &AccountIdOf<T>,
		timeout: BlockNumberFor<T>,
		match_querier: VersionedMultiLocation,
	) -> Result<Self::QueryId, DispatchError> {
		use frame_support::traits::EnsureOrigin;

		let origin = RawOrigin::Signed(origin.clone()).into();
		let responder = <T as pallet_xcm::Config>::ExecuteXcmOrigin::ensure_origin(origin)?;

		let query_id = <pallet_xcm::Pallet<T> as QueryHandler>::new_query(
			responder,
			timeout.into(),
			MultiLocation::try_from(match_querier)
				.map_err(|_| Into::<DispatchError>::into(pallet_xcm::Error::<T>::BadVersion))?,
		);

		Ok(query_id)
	}

	fn take_response(query_id: Self::QueryId) -> QueryResponseStatus<BlockNumberFor<T>> {
		<pallet_xcm::Pallet<T> as QueryHandler>::take_response(query_id)
	}
}

// impl
