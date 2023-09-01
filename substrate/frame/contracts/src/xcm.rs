use crate::{AccountIdOf, Config, Error, RawOrigin};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::DispatchResultWithPostInfo, weights::Weight};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::{DispatchError, DispatchResult};
use xcm::{v3::MultiLocation, VersionedMultiLocation, VersionedXcm};
use xcm_executor::traits::{QueryHandler, QueryResponseStatus};
pub type CallOf<T> = <T as frame_system::Config>::RuntimeCall;

pub trait XCM<T: Config> {
	type QueryId: Encode + Decode + MaxEncodedLen;
	type WeightInfo: WeightInfo;

	fn execute(
		origin: &AccountIdOf<T>,
		message: VersionedXcm<CallOf<T>>,
		max_weight: Weight,
	) -> DispatchResultWithPostInfo;
	fn send(
		origin: &AccountIdOf<T>,
		dest: VersionedMultiLocation,
		msg: VersionedXcm<()>,
	) -> DispatchResult;
	fn query(
		origin: &AccountIdOf<T>,
		timeout: BlockNumberFor<T>,
		match_querier: VersionedMultiLocation,
	) -> Result<Self::QueryId, DispatchError>;
	fn take_response(query_id: Self::QueryId) -> QueryResponseStatus<BlockNumberFor<T>>;
}

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

pub trait WeightInfo {
	fn execute() -> Weight;
	fn send() -> Weight;
	fn query() -> Weight;
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

use pallet_xcm::WeightInfo as XcmWeightInfo;

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
