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

//! Pallet to handle XCM messages.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod migration;

use codec::{Decode, Encode, EncodeLike, MaxEncodedLen};
use frame_support::{
	dispatch::{DispatchErrorWithPostInfo, GetDispatchInfo, WithPostDispatchInfo},
	pallet_prelude::*,
	traits::{
		Contains, ContainsPair, Currency, Defensive, EnsureOrigin, Get, LockableCurrency,
		OriginTrait, WithdrawReasons,
	},
	PalletId,
};
use frame_system::pallet_prelude::{BlockNumberFor, *};
pub use pallet::*;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AccountIdConversion, BadOrigin, BlakeTwo256, BlockNumberProvider, Dispatchable, Hash,
		Saturating, Zero,
	},
	RuntimeDebug,
};
use sp_std::{boxed::Box, marker::PhantomData, prelude::*, result::Result, vec};
use xcm::{latest::QueryResponseInfo, prelude::*};
use xcm_builder::{
	ExecuteController, ExecuteControllerWeightInfo, MaxXcmEncodedSize, QueryController,
	QueryControllerWeightInfo, SendController, SendControllerWeightInfo,
};
use xcm_executor::{
	traits::{
		AssetTransferError, CheckSuspension, ClaimAssets, ConvertLocation, ConvertOrigin,
		DropAssets, MatchesFungible, OnResponse, Properties, QueryHandler, QueryResponseStatus,
		TransactAsset, TransferType, VersionChangeNotifier, WeightBounds, XcmAssetTransfers,
	},
	AssetsInHolding,
};
use xcm_fee_payment_runtime_api::Error as FeePaymentError;

#[cfg(any(feature = "try-runtime", test))]
use sp_runtime::TryRuntimeError;

pub trait WeightInfo {
	fn send() -> Weight;
	fn teleport_assets() -> Weight;
	fn reserve_transfer_assets() -> Weight;
	fn transfer_assets() -> Weight;
	fn execute() -> Weight;
	fn force_xcm_version() -> Weight;
	fn force_default_xcm_version() -> Weight;
	fn force_subscribe_version_notify() -> Weight;
	fn force_unsubscribe_version_notify() -> Weight;
	fn force_suspension() -> Weight;
	fn migrate_supported_version() -> Weight;
	fn migrate_version_notifiers() -> Weight;
	fn already_notified_target() -> Weight;
	fn notify_current_targets() -> Weight;
	fn notify_target_migration_fail() -> Weight;
	fn migrate_version_notify_targets() -> Weight;
	fn migrate_and_notify_old_targets() -> Weight;
	fn new_query() -> Weight;
	fn take_response() -> Weight;
	fn claim_assets() -> Weight;
	fn execute_blob() -> Weight;
	fn send_blob() -> Weight;
}

/// fallback implementation
pub struct TestWeightInfo;
impl WeightInfo for TestWeightInfo {
	fn send() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn teleport_assets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn reserve_transfer_assets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn transfer_assets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn execute() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn force_xcm_version() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn force_default_xcm_version() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn force_subscribe_version_notify() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn force_unsubscribe_version_notify() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn force_suspension() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn migrate_supported_version() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn migrate_version_notifiers() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn already_notified_target() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn notify_current_targets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn notify_target_migration_fail() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn migrate_version_notify_targets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn migrate_and_notify_old_targets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn new_query() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn take_response() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn claim_assets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn execute_blob() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn send_blob() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		dispatch::{GetDispatchInfo, PostDispatchInfo},
		parameter_types,
	};
	use frame_system::Config as SysConfig;
	use sp_core::H256;
	use sp_runtime::traits::Dispatchable;
	use xcm_executor::traits::{MatchesFungible, WeightBounds};

	parameter_types! {
		/// An implementation of `Get<u32>` which just returns the latest XCM version which we can
		/// support.
		pub const CurrentXcmVersion: u32 = XCM_VERSION;
	}

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	pub type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::config]
	/// The module configuration trait.
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// A lockable currency.
		// TODO: We should really use a trait which can handle multiple currencies.
		type Currency: LockableCurrency<Self::AccountId, Moment = BlockNumberFor<Self>>;

		/// The `Asset` matcher for `Currency`.
		type CurrencyMatcher: MatchesFungible<BalanceOf<Self>>;

		/// Required origin for sending XCM messages. If successful, it resolves to `Location`
		/// which exists as an interior location within this chain's XCM context.
		type SendXcmOrigin: EnsureOrigin<<Self as SysConfig>::RuntimeOrigin, Success = Location>;

		/// The type used to actually dispatch an XCM to its destination.
		type XcmRouter: SendXcm;

		/// Required origin for executing XCM messages, including the teleport functionality. If
		/// successful, then it resolves to `Location` which exists as an interior location
		/// within this chain's XCM context.
		type ExecuteXcmOrigin: EnsureOrigin<<Self as SysConfig>::RuntimeOrigin, Success = Location>;

		/// Our XCM filter which messages to be executed using `XcmExecutor` must pass.
		type XcmExecuteFilter: Contains<(Location, Xcm<<Self as Config>::RuntimeCall>)>;

		/// Something to execute an XCM message.
		type XcmExecutor: ExecuteXcm<<Self as Config>::RuntimeCall> + XcmAssetTransfers;

		/// Our XCM filter which messages to be teleported using the dedicated extrinsic must pass.
		type XcmTeleportFilter: Contains<(Location, Vec<Asset>)>;

		/// Our XCM filter which messages to be reserve-transferred using the dedicated extrinsic
		/// must pass.
		type XcmReserveTransferFilter: Contains<(Location, Vec<Asset>)>;

		/// Means of measuring the weight consumed by an XCM message locally.
		type Weigher: WeightBounds<<Self as Config>::RuntimeCall>;

		/// This chain's Universal Location.
		type UniversalLocation: Get<InteriorLocation>;

		/// The runtime `Origin` type.
		type RuntimeOrigin: From<Origin> + From<<Self as SysConfig>::RuntimeOrigin>;

		/// The runtime `Call` type.
		type RuntimeCall: Parameter
			+ GetDispatchInfo
			+ Dispatchable<
				RuntimeOrigin = <Self as Config>::RuntimeOrigin,
				PostInfo = PostDispatchInfo,
			>;

		const VERSION_DISCOVERY_QUEUE_SIZE: u32;

		/// The latest supported version that we advertise. Generally just set it to
		/// `pallet_xcm::CurrentXcmVersion`.
		type AdvertisedXcmVersion: Get<XcmVersion>;

		/// The origin that is allowed to call privileged operations on the XCM pallet
		type AdminOrigin: EnsureOrigin<<Self as SysConfig>::RuntimeOrigin>;

		/// The assets which we consider a given origin is trusted if they claim to have placed a
		/// lock.
		type TrustedLockers: ContainsPair<Location, Asset>;

		/// How to get an `AccountId` value from a `Location`, useful for handling asset locks.
		type SovereignAccountOf: ConvertLocation<Self::AccountId>;

		/// The maximum number of local XCM locks that a single account may have.
		type MaxLockers: Get<u32>;

		/// The maximum number of consumers a single remote lock may have.
		type MaxRemoteLockConsumers: Get<u32>;

		/// The ID type for local consumers of remote locks.
		type RemoteLockConsumerIdentifier: Parameter + Member + MaxEncodedLen + Ord + Copy;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	impl<T: Config> ExecuteControllerWeightInfo for Pallet<T> {
		fn execute_blob() -> Weight {
			T::WeightInfo::execute_blob()
		}
	}

	impl<T: Config> ExecuteController<OriginFor<T>, <T as Config>::RuntimeCall> for Pallet<T> {
		type WeightInfo = Self;
		fn execute_blob(
			origin: OriginFor<T>,
			encoded_message: BoundedVec<u8, MaxXcmEncodedSize>,
			max_weight: Weight,
		) -> Result<Weight, DispatchErrorWithPostInfo> {
			let origin_location = T::ExecuteXcmOrigin::ensure_origin(origin)?;
			let message =
				VersionedXcm::<<T as Config>::RuntimeCall>::decode(&mut &encoded_message[..])
					.map_err(|error| {
						log::error!(target: "xcm::execute_blob", "Unable to decode XCM, error: {:?}", error);
						Error::<T>::UnableToDecode
					})?;
			Self::execute_base(origin_location, Box::new(message), max_weight)
		}
	}

	impl<T: Config> SendControllerWeightInfo for Pallet<T> {
		fn send_blob() -> Weight {
			T::WeightInfo::send_blob()
		}
	}

	impl<T: Config> SendController<OriginFor<T>> for Pallet<T> {
		type WeightInfo = Self;
		fn send_blob(
			origin: OriginFor<T>,
			dest: Box<VersionedLocation>,
			encoded_message: BoundedVec<u8, MaxXcmEncodedSize>,
		) -> Result<XcmHash, DispatchError> {
			let origin_location = T::SendXcmOrigin::ensure_origin(origin)?;
			let message =
				VersionedXcm::<()>::decode(&mut &encoded_message[..]).map_err(|error| {
					log::error!(target: "xcm::send_blob", "Unable to decode XCM, error: {:?}", error);
					Error::<T>::UnableToDecode
				})?;
			Self::send_base(origin_location, dest, Box::new(message))
		}
	}

	impl<T: Config> QueryControllerWeightInfo for Pallet<T> {
		fn query() -> Weight {
			T::WeightInfo::new_query()
		}
		fn take_response() -> Weight {
			T::WeightInfo::take_response()
		}
	}

	impl<T: Config> QueryController<OriginFor<T>, BlockNumberFor<T>> for Pallet<T> {
		type WeightInfo = Self;

		fn query(
			origin: OriginFor<T>,
			timeout: BlockNumberFor<T>,
			match_querier: VersionedLocation,
		) -> Result<QueryId, DispatchError> {
			let responder = <T as Config>::ExecuteXcmOrigin::ensure_origin(origin)?;
			let query_id = <Self as QueryHandler>::new_query(
				responder,
				timeout,
				Location::try_from(match_querier)
					.map_err(|_| Into::<DispatchError>::into(Error::<T>::BadVersion))?,
			);

			Ok(query_id)
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Execution of an XCM message was attempted.
		Attempted { outcome: xcm::latest::Outcome },
		/// A XCM message was sent.
		Sent { origin: Location, destination: Location, message: Xcm<()>, message_id: XcmHash },
		/// Query response received which does not match a registered query. This may be because a
		/// matching query was never registered, it may be because it is a duplicate response, or
		/// because the query timed out.
		UnexpectedResponse { origin: Location, query_id: QueryId },
		/// Query response has been received and is ready for taking with `take_response`. There is
		/// no registered notification call.
		ResponseReady { query_id: QueryId, response: Response },
		/// Query response has been received and query is removed. The registered notification has
		/// been dispatched and executed successfully.
		Notified { query_id: QueryId, pallet_index: u8, call_index: u8 },
		/// Query response has been received and query is removed. The registered notification
		/// could not be dispatched because the dispatch weight is greater than the maximum weight
		/// originally budgeted by this runtime for the query result.
		NotifyOverweight {
			query_id: QueryId,
			pallet_index: u8,
			call_index: u8,
			actual_weight: Weight,
			max_budgeted_weight: Weight,
		},
		/// Query response has been received and query is removed. There was a general error with
		/// dispatching the notification call.
		NotifyDispatchError { query_id: QueryId, pallet_index: u8, call_index: u8 },
		/// Query response has been received and query is removed. The dispatch was unable to be
		/// decoded into a `Call`; this might be due to dispatch function having a signature which
		/// is not `(origin, QueryId, Response)`.
		NotifyDecodeFailed { query_id: QueryId, pallet_index: u8, call_index: u8 },
		/// Expected query response has been received but the origin location of the response does
		/// not match that expected. The query remains registered for a later, valid, response to
		/// be received and acted upon.
		InvalidResponder {
			origin: Location,
			query_id: QueryId,
			expected_location: Option<Location>,
		},
		/// Expected query response has been received but the expected origin location placed in
		/// storage by this runtime previously cannot be decoded. The query remains registered.
		///
		/// This is unexpected (since a location placed in storage in a previously executing
		/// runtime should be readable prior to query timeout) and dangerous since the possibly
		/// valid response will be dropped. Manual governance intervention is probably going to be
		/// needed.
		InvalidResponderVersion { origin: Location, query_id: QueryId },
		/// Received query response has been read and removed.
		ResponseTaken { query_id: QueryId },
		/// Some assets have been placed in an asset trap.
		AssetsTrapped { hash: H256, origin: Location, assets: VersionedAssets },
		/// An XCM version change notification message has been attempted to be sent.
		///
		/// The cost of sending it (borne by the chain) is included.
		VersionChangeNotified {
			destination: Location,
			result: XcmVersion,
			cost: Assets,
			message_id: XcmHash,
		},
		/// The supported version of a location has been changed. This might be through an
		/// automatic notification or a manual intervention.
		SupportedVersionChanged { location: Location, version: XcmVersion },
		/// A given location which had a version change subscription was dropped owing to an error
		/// sending the notification to it.
		NotifyTargetSendFail { location: Location, query_id: QueryId, error: XcmError },
		/// A given location which had a version change subscription was dropped owing to an error
		/// migrating the location to our new XCM format.
		NotifyTargetMigrationFail { location: VersionedLocation, query_id: QueryId },
		/// Expected query response has been received but the expected querier location placed in
		/// storage by this runtime previously cannot be decoded. The query remains registered.
		///
		/// This is unexpected (since a location placed in storage in a previously executing
		/// runtime should be readable prior to query timeout) and dangerous since the possibly
		/// valid response will be dropped. Manual governance intervention is probably going to be
		/// needed.
		InvalidQuerierVersion { origin: Location, query_id: QueryId },
		/// Expected query response has been received but the querier location of the response does
		/// not match the expected. The query remains registered for a later, valid, response to
		/// be received and acted upon.
		InvalidQuerier {
			origin: Location,
			query_id: QueryId,
			expected_querier: Location,
			maybe_actual_querier: Option<Location>,
		},
		/// A remote has requested XCM version change notification from us and we have honored it.
		/// A version information message is sent to them and its cost is included.
		VersionNotifyStarted { destination: Location, cost: Assets, message_id: XcmHash },
		/// We have requested that a remote chain send us XCM version change notifications.
		VersionNotifyRequested { destination: Location, cost: Assets, message_id: XcmHash },
		/// We have requested that a remote chain stops sending us XCM version change
		/// notifications.
		VersionNotifyUnrequested { destination: Location, cost: Assets, message_id: XcmHash },
		/// Fees were paid from a location for an operation (often for using `SendXcm`).
		FeesPaid { paying: Location, fees: Assets },
		/// Some assets have been claimed from an asset trap
		AssetsClaimed { hash: H256, origin: Location, assets: VersionedAssets },
		/// A XCM version migration finished.
		VersionMigrationFinished { version: XcmVersion },
	}

	#[pallet::origin]
	#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub enum Origin {
		/// It comes from somewhere in the XCM space wanting to transact.
		Xcm(Location),
		/// It comes as an expected response from an XCM location.
		Response(Location),
	}
	impl From<Location> for Origin {
		fn from(location: Location) -> Origin {
			Origin::Xcm(location)
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The desired destination was unreachable, generally because there is a no way of routing
		/// to it.
		Unreachable,
		/// There was some other issue (i.e. not to do with routing) in sending the message.
		/// Perhaps a lack of space for buffering the message.
		SendFailure,
		/// The message execution fails the filter.
		Filtered,
		/// The message's weight could not be determined.
		UnweighableMessage,
		/// The destination `Location` provided cannot be inverted.
		DestinationNotInvertible,
		/// The assets to be sent are empty.
		Empty,
		/// Could not re-anchor the assets to declare the fees for the destination chain.
		CannotReanchor,
		/// Too many assets have been attempted for transfer.
		TooManyAssets,
		/// Origin is invalid for sending.
		InvalidOrigin,
		/// The version of the `Versioned` value used is not able to be interpreted.
		BadVersion,
		/// The given location could not be used (e.g. because it cannot be expressed in the
		/// desired version of XCM).
		BadLocation,
		/// The referenced subscription could not be found.
		NoSubscription,
		/// The location is invalid since it already has a subscription from us.
		AlreadySubscribed,
		/// Could not check-out the assets for teleportation to the destination chain.
		CannotCheckOutTeleport,
		/// The owner does not own (all) of the asset that they wish to do the operation on.
		LowBalance,
		/// The asset owner has too many locks on the asset.
		TooManyLocks,
		/// The given account is not an identifiable sovereign account for any location.
		AccountNotSovereign,
		/// The operation required fees to be paid which the initiator could not meet.
		FeesNotMet,
		/// A remote lock with the corresponding data could not be found.
		LockNotFound,
		/// The unlock operation cannot succeed because there are still consumers of the lock.
		InUse,
		/// Invalid non-concrete asset.
		InvalidAssetNotConcrete,
		/// Invalid asset, reserve chain could not be determined for it.
		InvalidAssetUnknownReserve,
		/// Invalid asset, do not support remote asset reserves with different fees reserves.
		InvalidAssetUnsupportedReserve,
		/// Too many assets with different reserve locations have been attempted for transfer.
		TooManyReserves,
		/// Local XCM execution incomplete.
		LocalExecutionIncomplete,
		/// Could not decode XCM.
		UnableToDecode,
		/// XCM encoded length is too large.
		/// Returned when an XCM encoded length is larger than `MaxXcmEncodedSize`.
		XcmTooLarge,
	}

	impl<T: Config> From<SendError> for Error<T> {
		fn from(e: SendError) -> Self {
			match e {
				SendError::Fees => Error::<T>::FeesNotMet,
				SendError::NotApplicable => Error::<T>::Unreachable,
				_ => Error::<T>::SendFailure,
			}
		}
	}

	impl<T: Config> From<AssetTransferError> for Error<T> {
		fn from(e: AssetTransferError) -> Self {
			match e {
				AssetTransferError::NotConcrete => Error::<T>::InvalidAssetNotConcrete,
				AssetTransferError::UnknownReserve => Error::<T>::InvalidAssetUnknownReserve,
			}
		}
	}

	/// The status of a query.
	#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
	pub enum QueryStatus<BlockNumber> {
		/// The query was sent but no response has yet been received.
		Pending {
			/// The `QueryResponse` XCM must have this origin to be considered a reply for this
			/// query.
			responder: VersionedLocation,
			/// The `QueryResponse` XCM must have this value as the `querier` field to be
			/// considered a reply for this query. If `None` then the querier is ignored.
			maybe_match_querier: Option<VersionedLocation>,
			maybe_notify: Option<(u8, u8)>,
			timeout: BlockNumber,
		},
		/// The query is for an ongoing version notification subscription.
		VersionNotifier { origin: VersionedLocation, is_active: bool },
		/// A response has been received.
		Ready { response: VersionedResponse, at: BlockNumber },
	}

	#[derive(Copy, Clone)]
	pub(crate) struct LatestVersionedLocation<'a>(pub(crate) &'a Location);
	impl<'a> EncodeLike<VersionedLocation> for LatestVersionedLocation<'a> {}
	impl<'a> Encode for LatestVersionedLocation<'a> {
		fn encode(&self) -> Vec<u8> {
			let mut r = VersionedLocation::from(Location::default()).encode();
			r.truncate(1);
			self.0.using_encoded(|d| r.extend_from_slice(d));
			r
		}
	}

	#[derive(Clone, Encode, Decode, Eq, PartialEq, Ord, PartialOrd, TypeInfo)]
	pub enum VersionMigrationStage {
		MigrateSupportedVersion,
		MigrateVersionNotifiers,
		NotifyCurrentTargets(Option<Vec<u8>>),
		MigrateAndNotifyOldTargets,
	}

	impl Default for VersionMigrationStage {
		fn default() -> Self {
			Self::MigrateSupportedVersion
		}
	}

	/// The latest available query index.
	#[pallet::storage]
	pub(super) type QueryCounter<T: Config> = StorageValue<_, QueryId, ValueQuery>;

	/// The ongoing queries.
	#[pallet::storage]
	#[pallet::getter(fn query)]
	pub(super) type Queries<T: Config> =
		StorageMap<_, Blake2_128Concat, QueryId, QueryStatus<BlockNumberFor<T>>, OptionQuery>;

	/// The existing asset traps.
	///
	/// Key is the blake2 256 hash of (origin, versioned `Assets`) pair. Value is the number of
	/// times this pair has been trapped (usually just 1 if it exists at all).
	#[pallet::storage]
	#[pallet::getter(fn asset_trap)]
	pub(super) type AssetTraps<T: Config> = StorageMap<_, Identity, H256, u32, ValueQuery>;

	/// Default version to encode XCM when latest version of destination is unknown. If `None`,
	/// then the destinations whose XCM version is unknown are considered unreachable.
	#[pallet::storage]
	#[pallet::whitelist_storage]
	pub(super) type SafeXcmVersion<T: Config> = StorageValue<_, XcmVersion, OptionQuery>;

	/// The Latest versions that we know various locations support.
	#[pallet::storage]
	pub(super) type SupportedVersion<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		XcmVersion,
		Blake2_128Concat,
		VersionedLocation,
		XcmVersion,
		OptionQuery,
	>;

	/// All locations that we have requested version notifications from.
	#[pallet::storage]
	pub(super) type VersionNotifiers<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		XcmVersion,
		Blake2_128Concat,
		VersionedLocation,
		QueryId,
		OptionQuery,
	>;

	/// The target locations that are subscribed to our version changes, as well as the most recent
	/// of our versions we informed them of.
	#[pallet::storage]
	pub(super) type VersionNotifyTargets<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		XcmVersion,
		Blake2_128Concat,
		VersionedLocation,
		(QueryId, Weight, XcmVersion),
		OptionQuery,
	>;

	pub struct VersionDiscoveryQueueSize<T>(PhantomData<T>);
	impl<T: Config> Get<u32> for VersionDiscoveryQueueSize<T> {
		fn get() -> u32 {
			T::VERSION_DISCOVERY_QUEUE_SIZE
		}
	}

	/// Destinations whose latest XCM version we would like to know. Duplicates not allowed, and
	/// the `u32` counter is the number of times that a send to the destination has been attempted,
	/// which is used as a prioritization.
	#[pallet::storage]
	#[pallet::whitelist_storage]
	pub(super) type VersionDiscoveryQueue<T: Config> = StorageValue<
		_,
		BoundedVec<(VersionedLocation, u32), VersionDiscoveryQueueSize<T>>,
		ValueQuery,
	>;

	/// The current migration's stage, if any.
	#[pallet::storage]
	pub(super) type CurrentMigration<T: Config> =
		StorageValue<_, VersionMigrationStage, OptionQuery>;

	#[derive(Clone, Encode, Decode, Eq, PartialEq, Ord, PartialOrd, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(MaxConsumers))]
	pub struct RemoteLockedFungibleRecord<ConsumerIdentifier, MaxConsumers: Get<u32>> {
		/// Total amount of the asset held by the remote lock.
		pub amount: u128,
		/// The owner of the locked asset.
		pub owner: VersionedLocation,
		/// The location which holds the original lock.
		pub locker: VersionedLocation,
		/// Local consumers of the remote lock with a consumer identifier and the amount
		/// of fungible asset every consumer holds.
		/// Every consumer can hold up to total amount of the remote lock.
		pub consumers: BoundedVec<(ConsumerIdentifier, u128), MaxConsumers>,
	}

	impl<LockId, MaxConsumers: Get<u32>> RemoteLockedFungibleRecord<LockId, MaxConsumers> {
		/// Amount of the remote lock in use by consumers.
		/// Returns `None` if the remote lock has no consumers.
		pub fn amount_held(&self) -> Option<u128> {
			self.consumers.iter().max_by(|x, y| x.1.cmp(&y.1)).map(|max| max.1)
		}
	}

	/// Fungible assets which we know are locked on a remote chain.
	#[pallet::storage]
	pub(super) type RemoteLockedFungibles<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Twox64Concat, XcmVersion>,
			NMapKey<Blake2_128Concat, T::AccountId>,
			NMapKey<Blake2_128Concat, VersionedAssetId>,
		),
		RemoteLockedFungibleRecord<T::RemoteLockConsumerIdentifier, T::MaxRemoteLockConsumers>,
		OptionQuery,
	>;

	/// Fungible assets which we know are locked on this chain.
	#[pallet::storage]
	pub(super) type LockedFungibles<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		BoundedVec<(BalanceOf<T>, VersionedLocation), T::MaxLockers>,
		OptionQuery,
	>;

	/// Global suspension state of the XCM executor.
	#[pallet::storage]
	pub(super) type XcmExecutionSuspended<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		#[serde(skip)]
		pub _config: sp_std::marker::PhantomData<T>,
		/// The default version to encode outgoing XCM messages with.
		pub safe_xcm_version: Option<XcmVersion>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { safe_xcm_version: Some(XCM_VERSION), _config: Default::default() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			SafeXcmVersion::<T>::set(self.safe_xcm_version);
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			let mut weight_used = Weight::zero();
			if let Some(migration) = CurrentMigration::<T>::get() {
				// Consume 10% of block at most
				let max_weight = T::BlockWeights::get().max_block / 10;
				let (w, maybe_migration) = Self::check_xcm_version_change(migration, max_weight);
				if maybe_migration.is_none() {
					Self::deposit_event(Event::VersionMigrationFinished { version: XCM_VERSION });
				}
				CurrentMigration::<T>::set(maybe_migration);
				weight_used.saturating_accrue(w);
			}

			// Here we aim to get one successful version negotiation request sent per block, ordered
			// by the destinations being most sent to.
			let mut q = VersionDiscoveryQueue::<T>::take().into_inner();
			// TODO: correct weights.
			weight_used.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			q.sort_by_key(|i| i.1);
			while let Some((versioned_dest, _)) = q.pop() {
				if let Ok(dest) = Location::try_from(versioned_dest) {
					if Self::request_version_notify(dest).is_ok() {
						// TODO: correct weights.
						weight_used.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
						break
					}
				}
			}
			// Should never fail since we only removed items. But better safe than panicking as it's
			// way better to drop the queue than panic on initialize.
			if let Ok(q) = BoundedVec::try_from(q) {
				VersionDiscoveryQueue::<T>::put(q);
			}
			weight_used
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), TryRuntimeError> {
			Self::do_try_state()
		}
	}

	pub mod migrations {
		use super::*;
		use frame_support::traits::{PalletInfoAccess, StorageVersion};

		#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
		enum QueryStatusV0<BlockNumber> {
			Pending {
				responder: VersionedLocation,
				maybe_notify: Option<(u8, u8)>,
				timeout: BlockNumber,
			},
			VersionNotifier {
				origin: VersionedLocation,
				is_active: bool,
			},
			Ready {
				response: VersionedResponse,
				at: BlockNumber,
			},
		}
		impl<B> From<QueryStatusV0<B>> for QueryStatus<B> {
			fn from(old: QueryStatusV0<B>) -> Self {
				use QueryStatusV0::*;
				match old {
					Pending { responder, maybe_notify, timeout } => QueryStatus::Pending {
						responder,
						maybe_notify,
						timeout,
						maybe_match_querier: Some(Location::here().into()),
					},
					VersionNotifier { origin, is_active } =>
						QueryStatus::VersionNotifier { origin, is_active },
					Ready { response, at } => QueryStatus::Ready { response, at },
				}
			}
		}

		pub fn migrate_to_v1<T: Config, P: GetStorageVersion + PalletInfoAccess>(
		) -> frame_support::weights::Weight {
			let on_chain_storage_version = <P as GetStorageVersion>::on_chain_storage_version();
			log::info!(
				target: "runtime::xcm",
				"Running migration storage v1 for xcm with storage version {:?}",
				on_chain_storage_version,
			);

			if on_chain_storage_version < 1 {
				let mut count = 0;
				Queries::<T>::translate::<QueryStatusV0<BlockNumberFor<T>>, _>(|_key, value| {
					count += 1;
					Some(value.into())
				});
				StorageVersion::new(1).put::<P>();
				log::info!(
					target: "runtime::xcm",
					"Running migration storage v1 for xcm with storage version {:?} was complete",
					on_chain_storage_version,
				);
				// calculate and return migration weights
				T::DbWeight::get().reads_writes(count as u64 + 1, count as u64 + 1)
			} else {
				log::warn!(
					target: "runtime::xcm",
					"Attempted to apply migration to v1 but failed because storage version is {:?}",
					on_chain_storage_version,
				);
				T::DbWeight::get().reads(1)
			}
		}
	}

	impl<T: Config> Pallet<T> {
		/// Underlying logic for both [`execute_blob`] and [`execute`].
		fn execute_base(
			origin_location: Location,
			message: Box<VersionedXcm<<T as Config>::RuntimeCall>>,
			max_weight: Weight,
		) -> Result<Weight, DispatchErrorWithPostInfo> {
			log::trace!(target: "xcm::pallet_xcm::execute", "message {:?}, max_weight {:?}", message, max_weight);
			let outcome = (|| {
				let mut hash = message.using_encoded(sp_io::hashing::blake2_256);
				let message = (*message).try_into().map_err(|()| Error::<T>::BadVersion)?;
				let value = (origin_location, message);
				ensure!(T::XcmExecuteFilter::contains(&value), Error::<T>::Filtered);
				let (origin_location, message) = value;
				Ok(T::XcmExecutor::prepare_and_execute(
					origin_location,
					message,
					&mut hash,
					max_weight,
					max_weight,
				))
			})()
			.map_err(|e: DispatchError| e.with_weight(T::WeightInfo::execute()))?;

			Self::deposit_event(Event::Attempted { outcome: outcome.clone() });
			let weight_used = outcome.weight_used();
			outcome.ensure_complete().map_err(|error| {
				log::error!(target: "xcm::pallet_xcm::execute", "XCM execution failed with error {:?}", error);
				Error::<T>::LocalExecutionIncomplete
					.with_weight(weight_used.saturating_add(T::WeightInfo::execute()))
			})?;
			Ok(weight_used)
		}

		/// Underlying logic for both [`send_blob`] and [`send`].
		fn send_base(
			origin_location: Location,
			dest: Box<VersionedLocation>,
			message: Box<VersionedXcm<()>>,
		) -> Result<XcmHash, DispatchError> {
			let interior: Junctions =
				origin_location.clone().try_into().map_err(|_| Error::<T>::InvalidOrigin)?;
			let dest = Location::try_from(*dest).map_err(|()| Error::<T>::BadVersion)?;
			let message: Xcm<()> = (*message).try_into().map_err(|()| Error::<T>::BadVersion)?;

			let message_id = Self::send_xcm(interior, dest.clone(), message.clone())
				.map_err(Error::<T>::from)?;
			let e = Event::Sent { origin: origin_location, destination: dest, message, message_id };
			Self::deposit_event(e);
			Ok(message_id)
		}
	}

	#[pallet::call(weight(<T as Config>::WeightInfo))]
	impl<T: Config> Pallet<T> {
		/// WARNING: DEPRECATED. `send` will be removed after June 2024. Use `send_blob` instead.
		#[allow(deprecated)]
		#[deprecated(note = "`send` will be removed after June 2024. Use `send_blob` instead.")]
		#[pallet::call_index(0)]
		pub fn send(
			origin: OriginFor<T>,
			dest: Box<VersionedLocation>,
			message: Box<VersionedXcm<()>>,
		) -> DispatchResult {
			let origin_location = T::SendXcmOrigin::ensure_origin(origin)?;
			Self::send_base(origin_location, dest, message)?;
			Ok(())
		}

		/// Teleport some assets from the local chain to some destination chain.
		///
		/// **This function is deprecated: Use `limited_teleport_assets` instead.**
		///
		/// Fee payment on the destination side is made from the asset in the `assets` vector of
		/// index `fee_asset_item`. The weight limit for fees is not provided and thus is unlimited,
		/// with all fees taken as needed from the asset.
		///
		/// - `origin`: Must be capable of withdrawing the `assets` and executing XCM.
		/// - `dest`: Destination context for the assets. Will typically be `[Parent,
		///   Parachain(..)]` to send from parachain to parachain, or `[Parachain(..)]` to send from
		///   relay to parachain.
		/// - `beneficiary`: A beneficiary location for the assets in the context of `dest`. Will
		///   generally be an `AccountId32` value.
		/// - `assets`: The assets to be withdrawn. This should include the assets used to pay the
		///   fee on the `dest` chain.
		/// - `fee_asset_item`: The index into `assets` of the item which should be used to pay
		///   fees.
		#[pallet::call_index(1)]
		#[allow(deprecated)]
		#[deprecated(
			note = "This extrinsic uses `WeightLimit::Unlimited`, please migrate to `limited_teleport_assets` or `transfer_assets`"
		)]
		pub fn teleport_assets(
			origin: OriginFor<T>,
			dest: Box<VersionedLocation>,
			beneficiary: Box<VersionedLocation>,
			assets: Box<VersionedAssets>,
			fee_asset_item: u32,
		) -> DispatchResult {
			Self::do_teleport_assets(origin, dest, beneficiary, assets, fee_asset_item, Unlimited)
		}

		/// Transfer some assets from the local chain to the destination chain through their local,
		/// destination or remote reserve.
		///
		/// `assets` must have same reserve location and may not be teleportable to `dest`.
		///  - `assets` have local reserve: transfer assets to sovereign account of destination
		///    chain and forward a notification XCM to `dest` to mint and deposit reserve-based
		///    assets to `beneficiary`.
		///  - `assets` have destination reserve: burn local assets and forward a notification to
		///    `dest` chain to withdraw the reserve assets from this chain's sovereign account and
		///    deposit them to `beneficiary`.
		///  - `assets` have remote reserve: burn local assets, forward XCM to reserve chain to move
		///    reserves from this chain's SA to `dest` chain's SA, and forward another XCM to `dest`
		///    to mint and deposit reserve-based assets to `beneficiary`.
		///
		/// **This function is deprecated: Use `limited_reserve_transfer_assets` instead.**
		///
		/// Fee payment on the destination side is made from the asset in the `assets` vector of
		/// index `fee_asset_item`. The weight limit for fees is not provided and thus is unlimited,
		/// with all fees taken as needed from the asset.
		///
		/// - `origin`: Must be capable of withdrawing the `assets` and executing XCM.
		/// - `dest`: Destination context for the assets. Will typically be `[Parent,
		///   Parachain(..)]` to send from parachain to parachain, or `[Parachain(..)]` to send from
		///   relay to parachain.
		/// - `beneficiary`: A beneficiary location for the assets in the context of `dest`. Will
		///   generally be an `AccountId32` value.
		/// - `assets`: The assets to be withdrawn. This should include the assets used to pay the
		///   fee on the `dest` (and possibly reserve) chains.
		/// - `fee_asset_item`: The index into `assets` of the item which should be used to pay
		///   fees.
		#[pallet::call_index(2)]
		#[allow(deprecated)]
		#[deprecated(
			note = "This extrinsic uses `WeightLimit::Unlimited`, please migrate to `limited_reserve_transfer_assets` or `transfer_assets`"
		)]
		pub fn reserve_transfer_assets(
			origin: OriginFor<T>,
			dest: Box<VersionedLocation>,
			beneficiary: Box<VersionedLocation>,
			assets: Box<VersionedAssets>,
			fee_asset_item: u32,
		) -> DispatchResult {
			Self::do_reserve_transfer_assets(
				origin,
				dest,
				beneficiary,
				assets,
				fee_asset_item,
				Unlimited,
			)
		}

		/// Execute an XCM message from a local, signed, origin.
		///
		/// An event is deposited indicating whether `msg` could be executed completely or only
		/// partially.
		///
		/// No more than `max_weight` will be used in its attempted execution. If this is less than
		/// the maximum amount of weight that the message could take to be executed, then no
		/// execution attempt will be made.
		///
		/// WARNING: DEPRECATED. `execute` will be removed after June 2024. Use `execute_blob`
		/// instead.
		#[allow(deprecated)]
		#[deprecated(
			note = "`execute` will be removed after June 2024. Use `execute_blob` instead."
		)]
		#[pallet::call_index(3)]
		#[pallet::weight(max_weight.saturating_add(T::WeightInfo::execute()))]
		pub fn execute(
			origin: OriginFor<T>,
			message: Box<VersionedXcm<<T as Config>::RuntimeCall>>,
			max_weight: Weight,
		) -> DispatchResultWithPostInfo {
			let origin_location = T::ExecuteXcmOrigin::ensure_origin(origin)?;
			let weight_used = Self::execute_base(origin_location, message, max_weight)?;
			Ok(Some(weight_used.saturating_add(T::WeightInfo::execute())).into())
		}

		/// Extoll that a particular destination can be communicated with through a particular
		/// version of XCM.
		///
		/// - `origin`: Must be an origin specified by AdminOrigin.
		/// - `location`: The destination that is being described.
		/// - `xcm_version`: The latest version of XCM that `location` supports.
		#[pallet::call_index(4)]
		pub fn force_xcm_version(
			origin: OriginFor<T>,
			location: Box<Location>,
			version: XcmVersion,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			let location = *location;
			SupportedVersion::<T>::insert(XCM_VERSION, LatestVersionedLocation(&location), version);
			Self::deposit_event(Event::SupportedVersionChanged { location, version });
			Ok(())
		}

		/// Set a safe XCM version (the version that XCM should be encoded with if the most recent
		/// version a destination can accept is unknown).
		///
		/// - `origin`: Must be an origin specified by AdminOrigin.
		/// - `maybe_xcm_version`: The default XCM encoding version, or `None` to disable.
		#[pallet::call_index(5)]
		pub fn force_default_xcm_version(
			origin: OriginFor<T>,
			maybe_xcm_version: Option<XcmVersion>,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			SafeXcmVersion::<T>::set(maybe_xcm_version);
			Ok(())
		}

		/// Ask a location to notify us regarding their XCM version and any changes to it.
		///
		/// - `origin`: Must be an origin specified by AdminOrigin.
		/// - `location`: The location to which we should subscribe for XCM version notifications.
		#[pallet::call_index(6)]
		pub fn force_subscribe_version_notify(
			origin: OriginFor<T>,
			location: Box<VersionedLocation>,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			let location: Location =
				(*location).try_into().map_err(|()| Error::<T>::BadLocation)?;
			Self::request_version_notify(location).map_err(|e| {
				match e {
					XcmError::InvalidLocation => Error::<T>::AlreadySubscribed,
					_ => Error::<T>::InvalidOrigin,
				}
				.into()
			})
		}

		/// Require that a particular destination should no longer notify us regarding any XCM
		/// version changes.
		///
		/// - `origin`: Must be an origin specified by AdminOrigin.
		/// - `location`: The location to which we are currently subscribed for XCM version
		///   notifications which we no longer desire.
		#[pallet::call_index(7)]
		pub fn force_unsubscribe_version_notify(
			origin: OriginFor<T>,
			location: Box<VersionedLocation>,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			let location: Location =
				(*location).try_into().map_err(|()| Error::<T>::BadLocation)?;
			Self::unrequest_version_notify(location).map_err(|e| {
				match e {
					XcmError::InvalidLocation => Error::<T>::NoSubscription,
					_ => Error::<T>::InvalidOrigin,
				}
				.into()
			})
		}

		/// Transfer some assets from the local chain to the destination chain through their local,
		/// destination or remote reserve.
		///
		/// `assets` must have same reserve location and may not be teleportable to `dest`.
		///  - `assets` have local reserve: transfer assets to sovereign account of destination
		///    chain and forward a notification XCM to `dest` to mint and deposit reserve-based
		///    assets to `beneficiary`.
		///  - `assets` have destination reserve: burn local assets and forward a notification to
		///    `dest` chain to withdraw the reserve assets from this chain's sovereign account and
		///    deposit them to `beneficiary`.
		///  - `assets` have remote reserve: burn local assets, forward XCM to reserve chain to move
		///    reserves from this chain's SA to `dest` chain's SA, and forward another XCM to `dest`
		///    to mint and deposit reserve-based assets to `beneficiary`.
		///
		/// Fee payment on the destination side is made from the asset in the `assets` vector of
		/// index `fee_asset_item`, up to enough to pay for `weight_limit` of weight. If more weight
		/// is needed than `weight_limit`, then the operation will fail and the sent assets may be
		/// at risk.
		///
		/// - `origin`: Must be capable of withdrawing the `assets` and executing XCM.
		/// - `dest`: Destination context for the assets. Will typically be `[Parent,
		///   Parachain(..)]` to send from parachain to parachain, or `[Parachain(..)]` to send from
		///   relay to parachain.
		/// - `beneficiary`: A beneficiary location for the assets in the context of `dest`. Will
		///   generally be an `AccountId32` value.
		/// - `assets`: The assets to be withdrawn. This should include the assets used to pay the
		///   fee on the `dest` (and possibly reserve) chains.
		/// - `fee_asset_item`: The index into `assets` of the item which should be used to pay
		///   fees.
		/// - `weight_limit`: The remote-side weight limit, if any, for the XCM fee purchase.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::reserve_transfer_assets())]
		pub fn limited_reserve_transfer_assets(
			origin: OriginFor<T>,
			dest: Box<VersionedLocation>,
			beneficiary: Box<VersionedLocation>,
			assets: Box<VersionedAssets>,
			fee_asset_item: u32,
			weight_limit: WeightLimit,
		) -> DispatchResult {
			Self::do_reserve_transfer_assets(
				origin,
				dest,
				beneficiary,
				assets,
				fee_asset_item,
				weight_limit,
			)
		}

		/// Teleport some assets from the local chain to some destination chain.
		///
		/// Fee payment on the destination side is made from the asset in the `assets` vector of
		/// index `fee_asset_item`, up to enough to pay for `weight_limit` of weight. If more weight
		/// is needed than `weight_limit`, then the operation will fail and the sent assets may be
		/// at risk.
		///
		/// - `origin`: Must be capable of withdrawing the `assets` and executing XCM.
		/// - `dest`: Destination context for the assets. Will typically be `[Parent,
		///   Parachain(..)]` to send from parachain to parachain, or `[Parachain(..)]` to send from
		///   relay to parachain.
		/// - `beneficiary`: A beneficiary location for the assets in the context of `dest`. Will
		///   generally be an `AccountId32` value.
		/// - `assets`: The assets to be withdrawn. This should include the assets used to pay the
		///   fee on the `dest` chain.
		/// - `fee_asset_item`: The index into `assets` of the item which should be used to pay
		///   fees.
		/// - `weight_limit`: The remote-side weight limit, if any, for the XCM fee purchase.
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::teleport_assets())]
		pub fn limited_teleport_assets(
			origin: OriginFor<T>,
			dest: Box<VersionedLocation>,
			beneficiary: Box<VersionedLocation>,
			assets: Box<VersionedAssets>,
			fee_asset_item: u32,
			weight_limit: WeightLimit,
		) -> DispatchResult {
			Self::do_teleport_assets(
				origin,
				dest,
				beneficiary,
				assets,
				fee_asset_item,
				weight_limit,
			)
		}

		/// Set or unset the global suspension state of the XCM executor.
		///
		/// - `origin`: Must be an origin specified by AdminOrigin.
		/// - `suspended`: `true` to suspend, `false` to resume.
		#[pallet::call_index(10)]
		pub fn force_suspension(origin: OriginFor<T>, suspended: bool) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			XcmExecutionSuspended::<T>::set(suspended);
			Ok(())
		}

		/// Transfer some assets from the local chain to the destination chain through their local,
		/// destination or remote reserve, or through teleports.
		///
		/// Fee payment on the destination side is made from the asset in the `assets` vector of
		/// index `fee_asset_item` (hence referred to as `fees`), up to enough to pay for
		/// `weight_limit` of weight. If more weight is needed than `weight_limit`, then the
		/// operation will fail and the sent assets may be at risk.
		///
		/// `assets` (excluding `fees`) must have same reserve location or otherwise be teleportable
		/// to `dest`, no limitations imposed on `fees`.
		///  - for local reserve: transfer assets to sovereign account of destination chain and
		///    forward a notification XCM to `dest` to mint and deposit reserve-based assets to
		///    `beneficiary`.
		///  - for destination reserve: burn local assets and forward a notification to `dest` chain
		///    to withdraw the reserve assets from this chain's sovereign account and deposit them
		///    to `beneficiary`.
		///  - for remote reserve: burn local assets, forward XCM to reserve chain to move reserves
		///    from this chain's SA to `dest` chain's SA, and forward another XCM to `dest` to mint
		///    and deposit reserve-based assets to `beneficiary`.
		///  - for teleports: burn local assets and forward XCM to `dest` chain to mint/teleport
		///    assets and deposit them to `beneficiary`.
		///
		/// - `origin`: Must be capable of withdrawing the `assets` and executing XCM.
		/// - `dest`: Destination context for the assets. Will typically be `X2(Parent,
		///   Parachain(..))` to send from parachain to parachain, or `X1(Parachain(..))` to send
		///   from relay to parachain.
		/// - `beneficiary`: A beneficiary location for the assets in the context of `dest`. Will
		///   generally be an `AccountId32` value.
		/// - `assets`: The assets to be withdrawn. This should include the assets used to pay the
		///   fee on the `dest` (and possibly reserve) chains.
		/// - `fee_asset_item`: The index into `assets` of the item which should be used to pay
		///   fees.
		/// - `weight_limit`: The remote-side weight limit, if any, for the XCM fee purchase.
		#[pallet::call_index(11)]
		pub fn transfer_assets(
			origin: OriginFor<T>,
			dest: Box<VersionedLocation>,
			beneficiary: Box<VersionedLocation>,
			assets: Box<VersionedAssets>,
			fee_asset_item: u32,
			weight_limit: WeightLimit,
		) -> DispatchResult {
			let origin = T::ExecuteXcmOrigin::ensure_origin(origin)?;
			let dest = (*dest).try_into().map_err(|()| Error::<T>::BadVersion)?;
			let beneficiary: Location =
				(*beneficiary).try_into().map_err(|()| Error::<T>::BadVersion)?;
			let assets: Assets = (*assets).try_into().map_err(|()| Error::<T>::BadVersion)?;
			log::debug!(
				target: "xcm::pallet_xcm::transfer_assets",
				"origin {:?}, dest {:?}, beneficiary {:?}, assets {:?}, fee-idx {:?}, weight_limit {:?}",
				origin, dest, beneficiary, assets, fee_asset_item, weight_limit,
			);

			ensure!(assets.len() <= MAX_ASSETS_FOR_TRANSFER, Error::<T>::TooManyAssets);
			let mut assets = assets.into_inner();
			let fee_asset_item = fee_asset_item as usize;
			let fees = assets.get(fee_asset_item as usize).ok_or(Error::<T>::Empty)?.clone();
			// Find transfer types for fee and non-fee assets.
			let (fees_transfer_type, assets_transfer_type) =
				Self::find_fee_and_assets_transfer_types(&assets, fee_asset_item, &dest)?;

			// local and remote XCM programs to potentially handle fees separately
			let fees = if fees_transfer_type == assets_transfer_type {
				// no need for custom fees instructions, fees are batched with assets
				FeesHandling::Batched { fees }
			} else {
				// Disallow _remote reserves_ unless assets & fees have same remote reserve (covered
				// by branch above). The reason for this is that we'd need to send XCMs to separate
				// chains with no guarantee of delivery order on final destination; therefore we
				// cannot guarantee to have fees in place on final destination chain to pay for
				// assets transfer.
				ensure!(
					!matches!(assets_transfer_type, TransferType::RemoteReserve(_)),
					Error::<T>::InvalidAssetUnsupportedReserve
				);
				let weight_limit = weight_limit.clone();
				// remove `fees` from `assets` and build separate fees transfer instructions to be
				// added to assets transfers XCM programs
				let fees = assets.remove(fee_asset_item);
				let (local_xcm, remote_xcm) = match fees_transfer_type {
					TransferType::LocalReserve => Self::local_reserve_fees_instructions(
						origin.clone(),
						dest.clone(),
						fees,
						weight_limit,
					)?,
					TransferType::DestinationReserve =>
						Self::destination_reserve_fees_instructions(
							origin.clone(),
							dest.clone(),
							fees,
							weight_limit,
						)?,
					TransferType::Teleport => Self::teleport_fees_instructions(
						origin.clone(),
						dest.clone(),
						fees,
						weight_limit,
					)?,
					TransferType::RemoteReserve(_) =>
						return Err(Error::<T>::InvalidAssetUnsupportedReserve.into()),
				};
				FeesHandling::Separate { local_xcm, remote_xcm }
			};

			Self::build_and_execute_xcm_transfer_type(
				origin,
				dest,
				beneficiary,
				assets,
				assets_transfer_type,
				fees,
				weight_limit,
			)
		}

		/// Claims assets trapped on this pallet because of leftover assets during XCM execution.
		///
		/// - `origin`: Anyone can call this extrinsic.
		/// - `assets`: The exact assets that were trapped. Use the version to specify what version
		/// was the latest when they were trapped.
		/// - `beneficiary`: The location/account where the claimed assets will be deposited.
		#[pallet::call_index(12)]
		pub fn claim_assets(
			origin: OriginFor<T>,
			assets: Box<VersionedAssets>,
			beneficiary: Box<VersionedLocation>,
		) -> DispatchResult {
			let origin_location = T::ExecuteXcmOrigin::ensure_origin(origin)?;
			log::debug!(target: "xcm::pallet_xcm::claim_assets", "origin: {:?}, assets: {:?}, beneficiary: {:?}", origin_location, assets, beneficiary);
			// Extract version from `assets`.
			let assets_version = assets.identify_version();
			let assets: Assets = (*assets).try_into().map_err(|()| Error::<T>::BadVersion)?;
			let number_of_assets = assets.len() as u32;
			let beneficiary: Location =
				(*beneficiary).try_into().map_err(|()| Error::<T>::BadVersion)?;
			let ticket: Location = GeneralIndex(assets_version as u128).into();
			let mut message = Xcm(vec![
				ClaimAsset { assets, ticket },
				DepositAsset { assets: AllCounted(number_of_assets).into(), beneficiary },
			]);
			let weight =
				T::Weigher::weight(&mut message).map_err(|()| Error::<T>::UnweighableMessage)?;
			let mut hash = message.using_encoded(sp_io::hashing::blake2_256);
			let outcome = T::XcmExecutor::prepare_and_execute(
				origin_location,
				message,
				&mut hash,
				weight,
				weight,
			);
			outcome.ensure_complete().map_err(|error| {
				log::error!(target: "xcm::pallet_xcm::claim_assets", "XCM execution failed with error: {:?}", error);
				Error::<T>::LocalExecutionIncomplete
			})?;
			Ok(())
		}

		/// Execute an XCM from a local, signed, origin.
		///
		/// An event is deposited indicating whether the message could be executed completely
		/// or only partially.
		///
		/// No more than `max_weight` will be used in its attempted execution. If this is less than
		/// the maximum amount of weight that the message could take to be executed, then no
		/// execution attempt will be made.
		///
		/// The message is passed in encoded. It needs to be decodable as a [`VersionedXcm`].
		#[pallet::call_index(13)]
		#[pallet::weight(max_weight.saturating_add(T::WeightInfo::execute_blob()))]
		pub fn execute_blob(
			origin: OriginFor<T>,
			encoded_message: BoundedVec<u8, MaxXcmEncodedSize>,
			max_weight: Weight,
		) -> DispatchResultWithPostInfo {
			let weight_used = <Self as ExecuteController<_, _>>::execute_blob(
				origin,
				encoded_message,
				max_weight,
			)?;
			Ok(Some(weight_used.saturating_add(T::WeightInfo::execute_blob())).into())
		}

		/// Send an XCM from a local, signed, origin.
		///
		/// The destination, `dest`, will receive this message with a `DescendOrigin` instruction
		/// that makes the origin of the message be the origin on this system.
		///
		/// The message is passed in encoded. It needs to be decodable as a [`VersionedXcm`].
		#[pallet::call_index(14)]
		pub fn send_blob(
			origin: OriginFor<T>,
			dest: Box<VersionedLocation>,
			encoded_message: BoundedVec<u8, MaxXcmEncodedSize>,
		) -> DispatchResult {
			<Self as SendController<_>>::send_blob(origin, dest, encoded_message)?;
			Ok(())
		}
	}
}

/// The maximum number of distinct assets allowed to be transferred in a single helper extrinsic.
const MAX_ASSETS_FOR_TRANSFER: usize = 2;

/// Specify how assets used for fees are handled during asset transfers.
#[derive(Clone, PartialEq)]
enum FeesHandling<T: Config> {
	/// `fees` asset can be batch-transferred with rest of assets using same XCM instructions.
	Batched { fees: Asset },
	/// fees cannot be batched, they are handled separately using XCM programs here.
	Separate { local_xcm: Xcm<<T as Config>::RuntimeCall>, remote_xcm: Xcm<()> },
}

impl<T: Config> sp_std::fmt::Debug for FeesHandling<T> {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		match self {
			Self::Batched { fees } => write!(f, "FeesHandling::Batched({:?})", fees),
			Self::Separate { local_xcm, remote_xcm } => write!(
				f,
				"FeesHandling::Separate(local: {:?}, remote: {:?})",
				local_xcm, remote_xcm
			),
		}
	}
}

impl<T: Config> QueryHandler for Pallet<T> {
	type BlockNumber = BlockNumberFor<T>;
	type Error = XcmError;
	type UniversalLocation = T::UniversalLocation;

	/// Attempt to create a new query ID and register it as a query that is yet to respond.
	fn new_query(
		responder: impl Into<Location>,
		timeout: BlockNumberFor<T>,
		match_querier: impl Into<Location>,
	) -> QueryId {
		Self::do_new_query(responder, None, timeout, match_querier)
	}

	/// To check the status of the query, use `fn query()` passing the resultant `QueryId`
	/// value.
	fn report_outcome(
		message: &mut Xcm<()>,
		responder: impl Into<Location>,
		timeout: Self::BlockNumber,
	) -> Result<QueryId, Self::Error> {
		let responder = responder.into();
		let destination = Self::UniversalLocation::get()
			.invert_target(&responder)
			.map_err(|()| XcmError::LocationNotInvertible)?;
		let query_id = Self::new_query(responder, timeout, Here);
		let response_info = QueryResponseInfo { destination, query_id, max_weight: Weight::zero() };
		let report_error = Xcm(vec![ReportError(response_info)]);
		message.0.insert(0, SetAppendix(report_error));
		Ok(query_id)
	}

	/// Removes response when ready and emits [Event::ResponseTaken] event.
	fn take_response(query_id: QueryId) -> QueryResponseStatus<Self::BlockNumber> {
		match Queries::<T>::get(query_id) {
			Some(QueryStatus::Ready { response, at }) => match response.try_into() {
				Ok(response) => {
					Queries::<T>::remove(query_id);
					Self::deposit_event(Event::ResponseTaken { query_id });
					QueryResponseStatus::Ready { response, at }
				},
				Err(_) => QueryResponseStatus::UnexpectedVersion,
			},
			Some(QueryStatus::Pending { timeout, .. }) => QueryResponseStatus::Pending { timeout },
			Some(_) => QueryResponseStatus::UnexpectedVersion,
			None => QueryResponseStatus::NotFound,
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn expect_response(id: QueryId, response: Response) {
		let response = response.into();
		Queries::<T>::insert(
			id,
			QueryStatus::Ready { response, at: frame_system::Pallet::<T>::block_number() },
		);
	}
}

impl<T: Config> Pallet<T> {
	/// Find `TransferType`s for `assets` and fee identified through `fee_asset_item`, when
	/// transferring to `dest`.
	///
	/// Validate `assets` to all have same `TransferType`.
	fn find_fee_and_assets_transfer_types(
		assets: &[Asset],
		fee_asset_item: usize,
		dest: &Location,
	) -> Result<(TransferType, TransferType), Error<T>> {
		let mut fees_transfer_type = None;
		let mut assets_transfer_type = None;
		for (idx, asset) in assets.iter().enumerate() {
			if let Fungible(x) = asset.fun {
				// If fungible asset, ensure non-zero amount.
				ensure!(!x.is_zero(), Error::<T>::Empty);
			}
			let transfer_type =
				T::XcmExecutor::determine_for(&asset, dest).map_err(Error::<T>::from)?;
			if idx == fee_asset_item {
				fees_transfer_type = Some(transfer_type);
			} else {
				if let Some(existing) = assets_transfer_type.as_ref() {
					// Ensure transfer for multiple assets uses same transfer type (only fee may
					// have different transfer type/path)
					ensure!(existing == &transfer_type, Error::<T>::TooManyReserves);
				} else {
					// asset reserve identified
					assets_transfer_type = Some(transfer_type);
				}
			}
		}
		// single asset also marked as fee item
		if assets.len() == 1 {
			assets_transfer_type = fees_transfer_type.clone()
		}
		Ok((
			fees_transfer_type.ok_or(Error::<T>::Empty)?,
			assets_transfer_type.ok_or(Error::<T>::Empty)?,
		))
	}

	fn do_reserve_transfer_assets(
		origin: OriginFor<T>,
		dest: Box<VersionedLocation>,
		beneficiary: Box<VersionedLocation>,
		assets: Box<VersionedAssets>,
		fee_asset_item: u32,
		weight_limit: WeightLimit,
	) -> DispatchResult {
		let origin_location = T::ExecuteXcmOrigin::ensure_origin(origin)?;
		let dest = (*dest).try_into().map_err(|()| Error::<T>::BadVersion)?;
		let beneficiary: Location =
			(*beneficiary).try_into().map_err(|()| Error::<T>::BadVersion)?;
		let assets: Assets = (*assets).try_into().map_err(|()| Error::<T>::BadVersion)?;
		log::debug!(
			target: "xcm::pallet_xcm::do_reserve_transfer_assets",
			"origin {:?}, dest {:?}, beneficiary {:?}, assets {:?}, fee-idx {:?}",
			origin_location, dest, beneficiary, assets, fee_asset_item,
		);

		ensure!(assets.len() <= MAX_ASSETS_FOR_TRANSFER, Error::<T>::TooManyAssets);
		let value = (origin_location, assets.into_inner());
		ensure!(T::XcmReserveTransferFilter::contains(&value), Error::<T>::Filtered);
		let (origin, assets) = value;

		let fee_asset_item = fee_asset_item as usize;
		let fees = assets.get(fee_asset_item as usize).ok_or(Error::<T>::Empty)?.clone();

		// Find transfer types for fee and non-fee assets.
		let (fees_transfer_type, assets_transfer_type) =
			Self::find_fee_and_assets_transfer_types(&assets, fee_asset_item, &dest)?;
		// Ensure assets (and fees according to check below) are not teleportable to `dest`.
		ensure!(assets_transfer_type != TransferType::Teleport, Error::<T>::Filtered);
		// Ensure all assets (including fees) have same reserve location.
		ensure!(assets_transfer_type == fees_transfer_type, Error::<T>::TooManyReserves);

		Self::build_and_execute_xcm_transfer_type(
			origin,
			dest,
			beneficiary,
			assets,
			assets_transfer_type,
			FeesHandling::Batched { fees },
			weight_limit,
		)
	}

	fn do_teleport_assets(
		origin: OriginFor<T>,
		dest: Box<VersionedLocation>,
		beneficiary: Box<VersionedLocation>,
		assets: Box<VersionedAssets>,
		fee_asset_item: u32,
		weight_limit: WeightLimit,
	) -> DispatchResult {
		let origin_location = T::ExecuteXcmOrigin::ensure_origin(origin)?;
		let dest = (*dest).try_into().map_err(|()| Error::<T>::BadVersion)?;
		let beneficiary: Location =
			(*beneficiary).try_into().map_err(|()| Error::<T>::BadVersion)?;
		let assets: Assets = (*assets).try_into().map_err(|()| Error::<T>::BadVersion)?;
		log::debug!(
			target: "xcm::pallet_xcm::do_teleport_assets",
			"origin {:?}, dest {:?}, beneficiary {:?}, assets {:?}, fee-idx {:?}, weight_limit {:?}",
			origin_location, dest, beneficiary, assets, fee_asset_item, weight_limit,
		);

		ensure!(assets.len() <= MAX_ASSETS_FOR_TRANSFER, Error::<T>::TooManyAssets);
		let value = (origin_location, assets.into_inner());
		ensure!(T::XcmTeleportFilter::contains(&value), Error::<T>::Filtered);
		let (origin_location, assets) = value;
		for asset in assets.iter() {
			let transfer_type =
				T::XcmExecutor::determine_for(asset, &dest).map_err(Error::<T>::from)?;
			ensure!(transfer_type == TransferType::Teleport, Error::<T>::Filtered);
		}
		let fees = assets.get(fee_asset_item as usize).ok_or(Error::<T>::Empty)?.clone();

		Self::build_and_execute_xcm_transfer_type(
			origin_location,
			dest,
			beneficiary,
			assets,
			TransferType::Teleport,
			FeesHandling::Batched { fees },
			weight_limit,
		)
	}

	fn build_and_execute_xcm_transfer_type(
		origin: Location,
		dest: Location,
		beneficiary: Location,
		assets: Vec<Asset>,
		transfer_type: TransferType,
		fees: FeesHandling<T>,
		weight_limit: WeightLimit,
	) -> DispatchResult {
		log::debug!(
			target: "xcm::pallet_xcm::build_and_execute_xcm_transfer_type",
			"origin {:?}, dest {:?}, beneficiary {:?}, assets {:?}, transfer_type {:?}, \
			fees_handling {:?}, weight_limit: {:?}",
			origin, dest, beneficiary, assets, transfer_type, fees, weight_limit,
		);
		let (mut local_xcm, remote_xcm) = match transfer_type {
			TransferType::LocalReserve => {
				let (local, remote) = Self::local_reserve_transfer_programs(
					origin.clone(),
					dest.clone(),
					beneficiary,
					assets,
					fees,
					weight_limit,
				)?;
				(local, Some(remote))
			},
			TransferType::DestinationReserve => {
				let (local, remote) = Self::destination_reserve_transfer_programs(
					origin.clone(),
					dest.clone(),
					beneficiary,
					assets,
					fees,
					weight_limit,
				)?;
				(local, Some(remote))
			},
			TransferType::RemoteReserve(reserve) => {
				let fees = match fees {
					FeesHandling::Batched { fees } => fees,
					_ => return Err(Error::<T>::InvalidAssetUnsupportedReserve.into()),
				};
				let local = Self::remote_reserve_transfer_program(
					origin.clone(),
					reserve,
					dest.clone(),
					beneficiary,
					assets,
					fees,
					weight_limit,
				)?;
				(local, None)
			},
			TransferType::Teleport => {
				let (local, remote) = Self::teleport_assets_program(
					origin.clone(),
					dest.clone(),
					beneficiary,
					assets,
					fees,
					weight_limit,
				)?;
				(local, Some(remote))
			},
		};
		let weight =
			T::Weigher::weight(&mut local_xcm).map_err(|()| Error::<T>::UnweighableMessage)?;
		let mut hash = local_xcm.using_encoded(sp_io::hashing::blake2_256);
		let outcome = T::XcmExecutor::prepare_and_execute(
			origin.clone(),
			local_xcm,
			&mut hash,
			weight,
			weight,
		);
		Self::deposit_event(Event::Attempted { outcome: outcome.clone() });
		outcome.ensure_complete().map_err(|error| {
			log::error!(
				target: "xcm::pallet_xcm::build_and_execute_xcm_transfer_type",
				"XCM execution failed with error {:?}", error
			);
			Error::<T>::LocalExecutionIncomplete
		})?;

		if let Some(remote_xcm) = remote_xcm {
			let (ticket, price) = validate_send::<T::XcmRouter>(dest.clone(), remote_xcm.clone())
				.map_err(Error::<T>::from)?;
			if origin != Here.into_location() {
				Self::charge_fees(origin.clone(), price).map_err(|error| {
					log::error!(
						target: "xcm::pallet_xcm::build_and_execute_xcm_transfer_type",
						"Unable to charge fee with error {:?}", error
					);
					Error::<T>::FeesNotMet
				})?;
			}
			let message_id = T::XcmRouter::deliver(ticket).map_err(Error::<T>::from)?;

			let e = Event::Sent { origin, destination: dest, message: remote_xcm, message_id };
			Self::deposit_event(e);
		}
		Ok(())
	}

	fn add_fees_to_xcm(
		dest: Location,
		fees: FeesHandling<T>,
		weight_limit: WeightLimit,
		local: &mut Xcm<<T as Config>::RuntimeCall>,
		remote: &mut Xcm<()>,
	) -> Result<(), Error<T>> {
		match fees {
			FeesHandling::Batched { fees } => {
				let context = T::UniversalLocation::get();
				// no custom fees instructions, they are batched together with `assets` transfer;
				// BuyExecution happens after receiving all `assets`
				let reanchored_fees =
					fees.reanchored(&dest, &context).map_err(|_| Error::<T>::CannotReanchor)?;
				// buy execution using `fees` batched together with above `reanchored_assets`
				remote.inner_mut().push(BuyExecution { fees: reanchored_fees, weight_limit });
			},
			FeesHandling::Separate { local_xcm: mut local_fees, remote_xcm: mut remote_fees } => {
				// fees are handled by separate XCM instructions, prepend fees instructions (for
				// remote XCM they have to be prepended instead of appended to pass barriers).
				sp_std::mem::swap(local, &mut local_fees);
				sp_std::mem::swap(remote, &mut remote_fees);
				// these are now swapped so fees actually go first
				local.inner_mut().append(&mut local_fees.into_inner());
				remote.inner_mut().append(&mut remote_fees.into_inner());
			},
		}
		Ok(())
	}

	fn local_reserve_fees_instructions(
		origin: Location,
		dest: Location,
		fees: Asset,
		weight_limit: WeightLimit,
	) -> Result<(Xcm<<T as Config>::RuntimeCall>, Xcm<()>), Error<T>> {
		let value = (origin, vec![fees.clone()]);
		ensure!(T::XcmReserveTransferFilter::contains(&value), Error::<T>::Filtered);

		let context = T::UniversalLocation::get();
		let reanchored_fees = fees
			.clone()
			.reanchored(&dest, &context)
			.map_err(|_| Error::<T>::CannotReanchor)?;

		let local_execute_xcm = Xcm(vec![
			// move `fees` to `dest`s local sovereign account
			TransferAsset { assets: fees.into(), beneficiary: dest },
		]);
		let xcm_on_dest = Xcm(vec![
			// let (dest) chain know `fees` are in its SA on reserve
			ReserveAssetDeposited(reanchored_fees.clone().into()),
			// buy exec using `fees` in holding deposited in above instruction
			BuyExecution { fees: reanchored_fees, weight_limit },
		]);
		Ok((local_execute_xcm, xcm_on_dest))
	}

	fn local_reserve_transfer_programs(
		origin: Location,
		dest: Location,
		beneficiary: Location,
		assets: Vec<Asset>,
		fees: FeesHandling<T>,
		weight_limit: WeightLimit,
	) -> Result<(Xcm<<T as Config>::RuntimeCall>, Xcm<()>), Error<T>> {
		let value = (origin, assets);
		ensure!(T::XcmReserveTransferFilter::contains(&value), Error::<T>::Filtered);
		let (_, assets) = value;

		// max assets is `assets` (+ potentially separately handled fee)
		let max_assets =
			assets.len() as u32 + if matches!(&fees, FeesHandling::Batched { .. }) { 0 } else { 1 };
		let assets: Assets = assets.into();
		let context = T::UniversalLocation::get();
		let mut reanchored_assets = assets.clone();
		reanchored_assets
			.reanchor(&dest, &context)
			.map_err(|_| Error::<T>::CannotReanchor)?;

		// XCM instructions to be executed on local chain
		let mut local_execute_xcm = Xcm(vec![
			// locally move `assets` to `dest`s local sovereign account
			TransferAsset { assets, beneficiary: dest.clone() },
		]);
		// XCM instructions to be executed on destination chain
		let mut xcm_on_dest = Xcm(vec![
			// let (dest) chain know assets are in its SA on reserve
			ReserveAssetDeposited(reanchored_assets),
			// following instructions are not exec'ed on behalf of origin chain anymore
			ClearOrigin,
		]);
		// handle fees
		Self::add_fees_to_xcm(dest, fees, weight_limit, &mut local_execute_xcm, &mut xcm_on_dest)?;
		// deposit all remaining assets in holding to `beneficiary` location
		xcm_on_dest
			.inner_mut()
			.push(DepositAsset { assets: Wild(AllCounted(max_assets)), beneficiary });

		Ok((local_execute_xcm, xcm_on_dest))
	}

	fn destination_reserve_fees_instructions(
		origin: Location,
		dest: Location,
		fees: Asset,
		weight_limit: WeightLimit,
	) -> Result<(Xcm<<T as Config>::RuntimeCall>, Xcm<()>), Error<T>> {
		let value = (origin, vec![fees.clone()]);
		ensure!(T::XcmReserveTransferFilter::contains(&value), Error::<T>::Filtered);

		let context = T::UniversalLocation::get();
		let reanchored_fees = fees
			.clone()
			.reanchored(&dest, &context)
			.map_err(|_| Error::<T>::CannotReanchor)?;
		let fees: Assets = fees.into();

		let local_execute_xcm = Xcm(vec![
			// withdraw reserve-based fees (derivatives)
			WithdrawAsset(fees.clone()),
			// burn derivatives
			BurnAsset(fees),
		]);
		let xcm_on_dest = Xcm(vec![
			// withdraw `fees` from origin chain's sovereign account
			WithdrawAsset(reanchored_fees.clone().into()),
			// buy exec using `fees` in holding withdrawn in above instruction
			BuyExecution { fees: reanchored_fees, weight_limit },
		]);
		Ok((local_execute_xcm, xcm_on_dest))
	}

	fn destination_reserve_transfer_programs(
		origin: Location,
		dest: Location,
		beneficiary: Location,
		assets: Vec<Asset>,
		fees: FeesHandling<T>,
		weight_limit: WeightLimit,
	) -> Result<(Xcm<<T as Config>::RuntimeCall>, Xcm<()>), Error<T>> {
		let value = (origin, assets);
		ensure!(T::XcmReserveTransferFilter::contains(&value), Error::<T>::Filtered);
		let (_, assets) = value;

		// max assets is `assets` (+ potentially separately handled fee)
		let max_assets =
			assets.len() as u32 + if matches!(&fees, FeesHandling::Batched { .. }) { 0 } else { 1 };
		let assets: Assets = assets.into();
		let context = T::UniversalLocation::get();
		let mut reanchored_assets = assets.clone();
		reanchored_assets
			.reanchor(&dest, &context)
			.map_err(|_| Error::<T>::CannotReanchor)?;

		// XCM instructions to be executed on local chain
		let mut local_execute_xcm = Xcm(vec![
			// withdraw reserve-based assets
			WithdrawAsset(assets.clone()),
			// burn reserve-based assets
			BurnAsset(assets),
		]);
		// XCM instructions to be executed on destination chain
		let mut xcm_on_dest = Xcm(vec![
			// withdraw `assets` from origin chain's sovereign account
			WithdrawAsset(reanchored_assets),
			// following instructions are not exec'ed on behalf of origin chain anymore
			ClearOrigin,
		]);
		// handle fees
		Self::add_fees_to_xcm(dest, fees, weight_limit, &mut local_execute_xcm, &mut xcm_on_dest)?;
		// deposit all remaining assets in holding to `beneficiary` location
		xcm_on_dest
			.inner_mut()
			.push(DepositAsset { assets: Wild(AllCounted(max_assets)), beneficiary });

		Ok((local_execute_xcm, xcm_on_dest))
	}

	// function assumes fees and assets have the same remote reserve
	fn remote_reserve_transfer_program(
		origin: Location,
		reserve: Location,
		dest: Location,
		beneficiary: Location,
		assets: Vec<Asset>,
		fees: Asset,
		weight_limit: WeightLimit,
	) -> Result<Xcm<<T as Config>::RuntimeCall>, Error<T>> {
		let value = (origin, assets);
		ensure!(T::XcmReserveTransferFilter::contains(&value), Error::<T>::Filtered);
		let (_, assets) = value;

		let max_assets = assets.len() as u32;
		let context = T::UniversalLocation::get();
		// we spend up to half of fees for execution on reserve and other half for execution on
		// destination
		let (fees_half_1, fees_half_2) = Self::halve_fees(fees)?;
		// identifies fee item as seen by `reserve` - to be used at reserve chain
		let reserve_fees = fees_half_1
			.reanchored(&reserve, &context)
			.map_err(|_| Error::<T>::CannotReanchor)?;
		// identifies fee item as seen by `dest` - to be used at destination chain
		let dest_fees = fees_half_2
			.reanchored(&dest, &context)
			.map_err(|_| Error::<T>::CannotReanchor)?;
		// identifies `dest` as seen by `reserve`
		let dest = dest.reanchored(&reserve, &context).map_err(|_| Error::<T>::CannotReanchor)?;
		// xcm to be executed at dest
		let xcm_on_dest = Xcm(vec![
			BuyExecution { fees: dest_fees, weight_limit: weight_limit.clone() },
			DepositAsset { assets: Wild(AllCounted(max_assets)), beneficiary },
		]);
		// xcm to be executed on reserve
		let xcm_on_reserve = Xcm(vec![
			BuyExecution { fees: reserve_fees, weight_limit },
			DepositReserveAsset { assets: Wild(AllCounted(max_assets)), dest, xcm: xcm_on_dest },
		]);
		Ok(Xcm(vec![
			WithdrawAsset(assets.into()),
			SetFeesMode { jit_withdraw: true },
			InitiateReserveWithdraw {
				assets: Wild(AllCounted(max_assets)),
				reserve,
				xcm: xcm_on_reserve,
			},
		]))
	}

	fn teleport_fees_instructions(
		origin: Location,
		dest: Location,
		fees: Asset,
		weight_limit: WeightLimit,
	) -> Result<(Xcm<<T as Config>::RuntimeCall>, Xcm<()>), Error<T>> {
		let value = (origin, vec![fees.clone()]);
		ensure!(T::XcmTeleportFilter::contains(&value), Error::<T>::Filtered);

		let context = T::UniversalLocation::get();
		let reanchored_fees = fees
			.clone()
			.reanchored(&dest, &context)
			.map_err(|_| Error::<T>::CannotReanchor)?;

		// XcmContext irrelevant in teleports checks
		let dummy_context =
			XcmContext { origin: None, message_id: Default::default(), topic: None };
		// We should check that the asset can actually be teleported out (for this to
		// be in error, there would need to be an accounting violation by ourselves,
		// so it's unlikely, but we don't want to allow that kind of bug to leak into
		// a trusted chain.
		<T::XcmExecutor as XcmAssetTransfers>::AssetTransactor::can_check_out(
			&dest,
			&fees,
			&dummy_context,
		)
		.map_err(|_| Error::<T>::CannotCheckOutTeleport)?;
		// safe to do this here, we're in a transactional call that will be reverted on any
		// errors down the line
		<T::XcmExecutor as XcmAssetTransfers>::AssetTransactor::check_out(
			&dest,
			&fees,
			&dummy_context,
		);

		let fees: Assets = fees.into();
		let local_execute_xcm = Xcm(vec![
			// withdraw fees
			WithdrawAsset(fees.clone()),
			// burn fees
			BurnAsset(fees),
		]);
		let xcm_on_dest = Xcm(vec![
			// (dest) chain receive teleported assets burned on origin chain
			ReceiveTeleportedAsset(reanchored_fees.clone().into()),
			// buy exec using `fees` in holding received in above instruction
			BuyExecution { fees: reanchored_fees, weight_limit },
		]);
		Ok((local_execute_xcm, xcm_on_dest))
	}

	fn teleport_assets_program(
		origin: Location,
		dest: Location,
		beneficiary: Location,
		assets: Vec<Asset>,
		fees: FeesHandling<T>,
		weight_limit: WeightLimit,
	) -> Result<(Xcm<<T as Config>::RuntimeCall>, Xcm<()>), Error<T>> {
		let value = (origin, assets);
		ensure!(T::XcmTeleportFilter::contains(&value), Error::<T>::Filtered);
		let (_, assets) = value;

		// max assets is `assets` (+ potentially separately handled fee)
		let max_assets =
			assets.len() as u32 + if matches!(&fees, FeesHandling::Batched { .. }) { 0 } else { 1 };
		let context = T::UniversalLocation::get();
		let assets: Assets = assets.into();
		let mut reanchored_assets = assets.clone();
		reanchored_assets
			.reanchor(&dest, &context)
			.map_err(|_| Error::<T>::CannotReanchor)?;

		// XcmContext irrelevant in teleports checks
		let dummy_context =
			XcmContext { origin: None, message_id: Default::default(), topic: None };
		for asset in assets.inner() {
			// We should check that the asset can actually be teleported out (for this to
			// be in error, there would need to be an accounting violation by ourselves,
			// so it's unlikely, but we don't want to allow that kind of bug to leak into
			// a trusted chain.
			<T::XcmExecutor as XcmAssetTransfers>::AssetTransactor::can_check_out(
				&dest,
				asset,
				&dummy_context,
			)
			.map_err(|_| Error::<T>::CannotCheckOutTeleport)?;
		}
		for asset in assets.inner() {
			// safe to do this here, we're in a transactional call that will be reverted on any
			// errors down the line
			<T::XcmExecutor as XcmAssetTransfers>::AssetTransactor::check_out(
				&dest,
				asset,
				&dummy_context,
			);
		}

		// XCM instructions to be executed on local chain
		let mut local_execute_xcm = Xcm(vec![
			// withdraw assets to be teleported
			WithdrawAsset(assets.clone()),
			// burn assets on local chain
			BurnAsset(assets),
		]);
		// XCM instructions to be executed on destination chain
		let mut xcm_on_dest = Xcm(vec![
			// teleport `assets` in from origin chain
			ReceiveTeleportedAsset(reanchored_assets),
			// following instructions are not exec'ed on behalf of origin chain anymore
			ClearOrigin,
		]);
		// handle fees
		Self::add_fees_to_xcm(dest, fees, weight_limit, &mut local_execute_xcm, &mut xcm_on_dest)?;
		// deposit all remaining assets in holding to `beneficiary` location
		xcm_on_dest
			.inner_mut()
			.push(DepositAsset { assets: Wild(AllCounted(max_assets)), beneficiary });

		Ok((local_execute_xcm, xcm_on_dest))
	}

	/// Halve `fees` fungible amount.
	pub(crate) fn halve_fees(fees: Asset) -> Result<(Asset, Asset), Error<T>> {
		match fees.fun {
			Fungible(amount) => {
				let fee1 = amount.saturating_div(2);
				let fee2 = amount.saturating_sub(fee1);
				ensure!(fee1 > 0, Error::<T>::FeesNotMet);
				ensure!(fee2 > 0, Error::<T>::FeesNotMet);
				Ok((Asset::from((fees.id.clone(), fee1)), Asset::from((fees.id.clone(), fee2))))
			},
			NonFungible(_) => Err(Error::<T>::FeesNotMet),
		}
	}

	/// Will always make progress, and will do its best not to use much more than `weight_cutoff`
	/// in doing so.
	pub(crate) fn check_xcm_version_change(
		mut stage: VersionMigrationStage,
		weight_cutoff: Weight,
	) -> (Weight, Option<VersionMigrationStage>) {
		let mut weight_used = Weight::zero();

		let sv_migrate_weight = T::WeightInfo::migrate_supported_version();
		let vn_migrate_weight = T::WeightInfo::migrate_version_notifiers();
		let vnt_already_notified_weight = T::WeightInfo::already_notified_target();
		let vnt_notify_weight = T::WeightInfo::notify_current_targets();
		let vnt_migrate_weight = T::WeightInfo::migrate_version_notify_targets();
		let vnt_migrate_fail_weight = T::WeightInfo::notify_target_migration_fail();
		let vnt_notify_migrate_weight = T::WeightInfo::migrate_and_notify_old_targets();

		use VersionMigrationStage::*;

		if stage == MigrateSupportedVersion {
			// We assume that supported XCM version only ever increases, so just cycle through lower
			// XCM versioned from the current.
			for v in 0..XCM_VERSION {
				for (old_key, value) in SupportedVersion::<T>::drain_prefix(v) {
					if let Ok(new_key) = old_key.into_latest() {
						SupportedVersion::<T>::insert(XCM_VERSION, new_key, value);
					}
					weight_used.saturating_accrue(sv_migrate_weight);
					if weight_used.any_gte(weight_cutoff) {
						return (weight_used, Some(stage))
					}
				}
			}
			stage = MigrateVersionNotifiers;
		}
		if stage == MigrateVersionNotifiers {
			for v in 0..XCM_VERSION {
				for (old_key, value) in VersionNotifiers::<T>::drain_prefix(v) {
					if let Ok(new_key) = old_key.into_latest() {
						VersionNotifiers::<T>::insert(XCM_VERSION, new_key, value);
					}
					weight_used.saturating_accrue(vn_migrate_weight);
					if weight_used.any_gte(weight_cutoff) {
						return (weight_used, Some(stage))
					}
				}
			}
			stage = NotifyCurrentTargets(None);
		}

		let xcm_version = T::AdvertisedXcmVersion::get();

		if let NotifyCurrentTargets(maybe_last_raw_key) = stage {
			let mut iter = match maybe_last_raw_key {
				Some(k) => VersionNotifyTargets::<T>::iter_prefix_from(XCM_VERSION, k),
				None => VersionNotifyTargets::<T>::iter_prefix(XCM_VERSION),
			};
			while let Some((key, value)) = iter.next() {
				let (query_id, max_weight, target_xcm_version) = value;
				let new_key: Location = match key.clone().try_into() {
					Ok(k) if target_xcm_version != xcm_version => k,
					_ => {
						// We don't early return here since we need to be certain that we
						// make some progress.
						weight_used.saturating_accrue(vnt_already_notified_weight);
						continue
					},
				};
				let response = Response::Version(xcm_version);
				let message =
					Xcm(vec![QueryResponse { query_id, response, max_weight, querier: None }]);
				let event = match send_xcm::<T::XcmRouter>(new_key.clone(), message) {
					Ok((message_id, cost)) => {
						let value = (query_id, max_weight, xcm_version);
						VersionNotifyTargets::<T>::insert(XCM_VERSION, key, value);
						Event::VersionChangeNotified {
							destination: new_key,
							result: xcm_version,
							cost,
							message_id,
						}
					},
					Err(e) => {
						VersionNotifyTargets::<T>::remove(XCM_VERSION, key);
						Event::NotifyTargetSendFail { location: new_key, query_id, error: e.into() }
					},
				};
				Self::deposit_event(event);
				weight_used.saturating_accrue(vnt_notify_weight);
				if weight_used.any_gte(weight_cutoff) {
					let last = Some(iter.last_raw_key().into());
					return (weight_used, Some(NotifyCurrentTargets(last)))
				}
			}
			stage = MigrateAndNotifyOldTargets;
		}
		if stage == MigrateAndNotifyOldTargets {
			for v in 0..XCM_VERSION {
				for (old_key, value) in VersionNotifyTargets::<T>::drain_prefix(v) {
					let (query_id, max_weight, target_xcm_version) = value;
					let new_key = match Location::try_from(old_key.clone()) {
						Ok(k) => k,
						Err(()) => {
							Self::deposit_event(Event::NotifyTargetMigrationFail {
								location: old_key,
								query_id: value.0,
							});
							weight_used.saturating_accrue(vnt_migrate_fail_weight);
							if weight_used.any_gte(weight_cutoff) {
								return (weight_used, Some(stage))
							}
							continue
						},
					};

					let versioned_key = LatestVersionedLocation(&new_key);
					if target_xcm_version == xcm_version {
						VersionNotifyTargets::<T>::insert(XCM_VERSION, versioned_key, value);
						weight_used.saturating_accrue(vnt_migrate_weight);
					} else {
						// Need to notify target.
						let response = Response::Version(xcm_version);
						let message = Xcm(vec![QueryResponse {
							query_id,
							response,
							max_weight,
							querier: None,
						}]);
						let event = match send_xcm::<T::XcmRouter>(new_key.clone(), message) {
							Ok((message_id, cost)) => {
								VersionNotifyTargets::<T>::insert(
									XCM_VERSION,
									versioned_key,
									(query_id, max_weight, xcm_version),
								);
								Event::VersionChangeNotified {
									destination: new_key,
									result: xcm_version,
									cost,
									message_id,
								}
							},
							Err(e) => Event::NotifyTargetSendFail {
								location: new_key,
								query_id,
								error: e.into(),
							},
						};
						Self::deposit_event(event);
						weight_used.saturating_accrue(vnt_notify_migrate_weight);
					}
					if weight_used.any_gte(weight_cutoff) {
						return (weight_used, Some(stage))
					}
				}
			}
		}
		(weight_used, None)
	}

	/// Request that `dest` informs us of its version.
	pub fn request_version_notify(dest: impl Into<Location>) -> XcmResult {
		let dest = dest.into();
		let versioned_dest = VersionedLocation::from(dest.clone());
		let already = VersionNotifiers::<T>::contains_key(XCM_VERSION, &versioned_dest);
		ensure!(!already, XcmError::InvalidLocation);
		let query_id = QueryCounter::<T>::mutate(|q| {
			let r = *q;
			q.saturating_inc();
			r
		});
		// TODO #3735: Correct weight.
		let instruction = SubscribeVersion { query_id, max_response_weight: Weight::zero() };
		let (message_id, cost) = send_xcm::<T::XcmRouter>(dest.clone(), Xcm(vec![instruction]))?;
		Self::deposit_event(Event::VersionNotifyRequested { destination: dest, cost, message_id });
		VersionNotifiers::<T>::insert(XCM_VERSION, &versioned_dest, query_id);
		let query_status =
			QueryStatus::VersionNotifier { origin: versioned_dest, is_active: false };
		Queries::<T>::insert(query_id, query_status);
		Ok(())
	}

	/// Request that `dest` ceases informing us of its version.
	pub fn unrequest_version_notify(dest: impl Into<Location>) -> XcmResult {
		let dest = dest.into();
		let versioned_dest = LatestVersionedLocation(&dest);
		let query_id = VersionNotifiers::<T>::take(XCM_VERSION, versioned_dest)
			.ok_or(XcmError::InvalidLocation)?;
		let (message_id, cost) =
			send_xcm::<T::XcmRouter>(dest.clone(), Xcm(vec![UnsubscribeVersion]))?;
		Self::deposit_event(Event::VersionNotifyUnrequested {
			destination: dest,
			cost,
			message_id,
		});
		Queries::<T>::remove(query_id);
		Ok(())
	}

	/// Relay an XCM `message` from a given `interior` location in this context to a given `dest`
	/// location. The `fee_payer` is charged for the delivery unless `None` in which case fees
	/// are not charged (and instead borne by the chain).
	pub fn send_xcm(
		interior: impl Into<Junctions>,
		dest: impl Into<Location>,
		mut message: Xcm<()>,
	) -> Result<XcmHash, SendError> {
		let interior = interior.into();
		let dest = dest.into();
		let maybe_fee_payer = if interior != Junctions::Here {
			message.0.insert(0, DescendOrigin(interior.clone()));
			Some(interior.into())
		} else {
			None
		};
		log::debug!(target: "xcm::send_xcm", "dest: {:?}, message: {:?}", &dest, &message);
		let (ticket, price) = validate_send::<T::XcmRouter>(dest, message)?;
		if let Some(fee_payer) = maybe_fee_payer {
			Self::charge_fees(fee_payer, price).map_err(|_| SendError::Fees)?;
		}
		T::XcmRouter::deliver(ticket)
	}

	pub fn check_account() -> T::AccountId {
		const ID: PalletId = PalletId(*b"py/xcmch");
		AccountIdConversion::<T::AccountId>::into_account_truncating(&ID)
	}

	pub fn query_xcm_weight(message: VersionedXcm<()>) -> Result<Weight, FeePaymentError> {
		let message =
			Xcm::<()>::try_from(message).map_err(|_| FeePaymentError::VersionedConversionFailed)?;

		T::Weigher::weight(&mut message.into()).map_err(|()| {
			log::error!(target: "xcm::pallet_xcm::query_xcm_weight", "Error when querying XCM weight");
			FeePaymentError::WeightNotComputable
		})
	}

	pub fn query_delivery_fees(
		destination: VersionedLocation,
		message: VersionedXcm<()>,
	) -> Result<VersionedAssets, FeePaymentError> {
		let result_version = destination.identify_version().max(message.identify_version());

		let destination =
			destination.try_into().map_err(|_| FeePaymentError::VersionedConversionFailed)?;

		let message = message.try_into().map_err(|_| FeePaymentError::VersionedConversionFailed)?;

		let (_, fees) = validate_send::<T::XcmRouter>(destination, message).map_err(|error| {
			log::error!(target: "xcm::pallet_xcm::query_delivery_fees", "Error when querying delivery fees: {:?}", error);
			FeePaymentError::Unroutable
		})?;

		VersionedAssets::from(fees)
			.into_version(result_version)
			.map_err(|_| FeePaymentError::VersionedConversionFailed)
	}

	/// Create a new expectation of a query response with the querier being here.
	fn do_new_query(
		responder: impl Into<Location>,
		maybe_notify: Option<(u8, u8)>,
		timeout: BlockNumberFor<T>,
		match_querier: impl Into<Location>,
	) -> u64 {
		QueryCounter::<T>::mutate(|q| {
			let r = *q;
			q.saturating_inc();
			Queries::<T>::insert(
				r,
				QueryStatus::Pending {
					responder: responder.into().into(),
					maybe_match_querier: Some(match_querier.into().into()),
					maybe_notify,
					timeout,
				},
			);
			r
		})
	}

	/// Consume `message` and return another which is equivalent to it except that it reports
	/// back the outcome and dispatches `notify` on this chain.
	///
	/// - `message`: The message whose outcome should be reported.
	/// - `responder`: The origin from which a response should be expected.
	/// - `notify`: A dispatchable function which will be called once the outcome of `message` is
	///   known. It may be a dispatchable in any pallet of the local chain, but other than the usual
	///   origin, it must accept exactly two arguments: `query_id: QueryId` and `outcome: Response`,
	///   and in that order. It should expect that the origin is `Origin::Response` and will contain
	///   the responder's location.
	/// - `timeout`: The block number after which it is permissible for `notify` not to be called
	///   even if a response is received.
	///
	/// `report_outcome_notify` may return an error if the `responder` is not invertible.
	///
	/// It is assumed that the querier of the response will be `Here`.
	///
	/// NOTE: `notify` gets called as part of handling an incoming message, so it should be
	/// lightweight. Its weight is estimated during this function and stored ready for
	/// weighing `ReportOutcome` on the way back. If it turns out to be heavier once it returns
	/// then reporting the outcome will fail. Futhermore if the estimate is too high, then it
	/// may be put in the overweight queue and need to be manually executed.
	pub fn report_outcome_notify(
		message: &mut Xcm<()>,
		responder: impl Into<Location>,
		notify: impl Into<<T as Config>::RuntimeCall>,
		timeout: BlockNumberFor<T>,
	) -> Result<(), XcmError> {
		let responder = responder.into();
		let destination = T::UniversalLocation::get()
			.invert_target(&responder)
			.map_err(|()| XcmError::LocationNotInvertible)?;
		let notify: <T as Config>::RuntimeCall = notify.into();
		let max_weight = notify.get_dispatch_info().weight;
		let query_id = Self::new_notify_query(responder, notify, timeout, Here);
		let response_info = QueryResponseInfo { destination, query_id, max_weight };
		let report_error = Xcm(vec![ReportError(response_info)]);
		message.0.insert(0, SetAppendix(report_error));
		Ok(())
	}

	/// Attempt to create a new query ID and register it as a query that is yet to respond, and
	/// which will call a dispatchable when a response happens.
	pub fn new_notify_query(
		responder: impl Into<Location>,
		notify: impl Into<<T as Config>::RuntimeCall>,
		timeout: BlockNumberFor<T>,
		match_querier: impl Into<Location>,
	) -> u64 {
		let notify = notify.into().using_encoded(|mut bytes| Decode::decode(&mut bytes)).expect(
			"decode input is output of Call encode; Call guaranteed to have two enums; qed",
		);
		Self::do_new_query(responder, Some(notify), timeout, match_querier)
	}

	/// Note that a particular destination to whom we would like to send a message is unknown
	/// and queue it for version discovery.
	fn note_unknown_version(dest: &Location) {
		log::trace!(
			target: "xcm::pallet_xcm::note_unknown_version",
			"XCM version is unknown for destination: {:?}",
			dest,
		);
		let versioned_dest = VersionedLocation::from(dest.clone());
		VersionDiscoveryQueue::<T>::mutate(|q| {
			if let Some(index) = q.iter().position(|i| &i.0 == &versioned_dest) {
				// exists - just bump the count.
				q[index].1.saturating_inc();
			} else {
				let _ = q.try_push((versioned_dest, 1));
			}
		});
	}

	/// Withdraw given `assets` from the given `location` and pay as XCM fees.
	///
	/// Fails if:
	/// - the `assets` are not known on this chain;
	/// - the `assets` cannot be withdrawn with that location as the Origin.
	fn charge_fees(location: Location, assets: Assets) -> DispatchResult {
		T::XcmExecutor::charge_fees(location.clone(), assets.clone())
			.map_err(|_| Error::<T>::FeesNotMet)?;
		Self::deposit_event(Event::FeesPaid { paying: location, fees: assets });
		Ok(())
	}

	/// Ensure the correctness of the state of this pallet.
	///
	/// This should be valid before and after each state transition of this pallet.
	///
	/// ## Invariants
	///
	/// All entries stored in the `SupportedVersion` / `VersionNotifiers` / `VersionNotifyTargets`
	/// need to be migrated to the `XCM_VERSION`. If they are not, then `CurrentMigration` has to be
	/// set.
	#[cfg(any(feature = "try-runtime", test))]
	pub fn do_try_state() -> Result<(), TryRuntimeError> {
		// if migration has been already scheduled, everything is ok and data will be eventually
		// migrated
		if CurrentMigration::<T>::exists() {
			return Ok(())
		}

		// if migration has NOT been scheduled yet, we need to check all operational data
		for v in 0..XCM_VERSION {
			ensure!(
				SupportedVersion::<T>::iter_prefix(v).next().is_none(),
				TryRuntimeError::Other(
					"`SupportedVersion` data should be migrated to the `XCM_VERSION`!`"
				)
			);
			ensure!(
				VersionNotifiers::<T>::iter_prefix(v).next().is_none(),
				TryRuntimeError::Other(
					"`VersionNotifiers` data should be migrated to the `XCM_VERSION`!`"
				)
			);
			ensure!(
				VersionNotifyTargets::<T>::iter_prefix(v).next().is_none(),
				TryRuntimeError::Other(
					"`VersionNotifyTargets` data should be migrated to the `XCM_VERSION`!`"
				)
			);
		}

		Ok(())
	}
}

pub struct LockTicket<T: Config> {
	sovereign_account: T::AccountId,
	amount: BalanceOf<T>,
	unlocker: Location,
	item_index: Option<usize>,
}

impl<T: Config> xcm_executor::traits::Enact for LockTicket<T> {
	fn enact(self) -> Result<(), xcm_executor::traits::LockError> {
		use xcm_executor::traits::LockError::UnexpectedState;
		let mut locks = LockedFungibles::<T>::get(&self.sovereign_account).unwrap_or_default();
		match self.item_index {
			Some(index) => {
				ensure!(locks.len() > index, UnexpectedState);
				ensure!(locks[index].1.try_as::<_>() == Ok(&self.unlocker), UnexpectedState);
				locks[index].0 = locks[index].0.max(self.amount);
			},
			None => {
				locks
					.try_push((self.amount, self.unlocker.into()))
					.map_err(|(_balance, _location)| UnexpectedState)?;
			},
		}
		LockedFungibles::<T>::insert(&self.sovereign_account, locks);
		T::Currency::extend_lock(
			*b"py/xcmlk",
			&self.sovereign_account,
			self.amount,
			WithdrawReasons::all(),
		);
		Ok(())
	}
}

pub struct UnlockTicket<T: Config> {
	sovereign_account: T::AccountId,
	amount: BalanceOf<T>,
	unlocker: Location,
}

impl<T: Config> xcm_executor::traits::Enact for UnlockTicket<T> {
	fn enact(self) -> Result<(), xcm_executor::traits::LockError> {
		use xcm_executor::traits::LockError::UnexpectedState;
		let mut locks =
			LockedFungibles::<T>::get(&self.sovereign_account).ok_or(UnexpectedState)?;
		let mut maybe_remove_index = None;
		let mut locked = BalanceOf::<T>::zero();
		let mut found = false;
		// We could just as well do with with an into_iter, filter_map and collect, however this way
		// avoids making an allocation.
		for (i, x) in locks.iter_mut().enumerate() {
			if x.1.try_as::<_>().defensive() == Ok(&self.unlocker) {
				x.0 = x.0.saturating_sub(self.amount);
				if x.0.is_zero() {
					maybe_remove_index = Some(i);
				}
				found = true;
			}
			locked = locked.max(x.0);
		}
		ensure!(found, UnexpectedState);
		if let Some(remove_index) = maybe_remove_index {
			locks.swap_remove(remove_index);
		}
		LockedFungibles::<T>::insert(&self.sovereign_account, locks);
		let reasons = WithdrawReasons::all();
		T::Currency::set_lock(*b"py/xcmlk", &self.sovereign_account, locked, reasons);
		Ok(())
	}
}

pub struct ReduceTicket<T: Config> {
	key: (u32, T::AccountId, VersionedAssetId),
	amount: u128,
	locker: VersionedLocation,
	owner: VersionedLocation,
}

impl<T: Config> xcm_executor::traits::Enact for ReduceTicket<T> {
	fn enact(self) -> Result<(), xcm_executor::traits::LockError> {
		use xcm_executor::traits::LockError::UnexpectedState;
		let mut record = RemoteLockedFungibles::<T>::get(&self.key).ok_or(UnexpectedState)?;
		ensure!(self.locker == record.locker && self.owner == record.owner, UnexpectedState);
		let new_amount = record.amount.checked_sub(self.amount).ok_or(UnexpectedState)?;
		ensure!(record.amount_held().map_or(true, |h| new_amount >= h), UnexpectedState);
		if new_amount == 0 {
			RemoteLockedFungibles::<T>::remove(&self.key);
		} else {
			record.amount = new_amount;
			RemoteLockedFungibles::<T>::insert(&self.key, &record);
		}
		Ok(())
	}
}

impl<T: Config> xcm_executor::traits::AssetLock for Pallet<T> {
	type LockTicket = LockTicket<T>;
	type UnlockTicket = UnlockTicket<T>;
	type ReduceTicket = ReduceTicket<T>;

	fn prepare_lock(
		unlocker: Location,
		asset: Asset,
		owner: Location,
	) -> Result<LockTicket<T>, xcm_executor::traits::LockError> {
		use xcm_executor::traits::LockError::*;
		let sovereign_account = T::SovereignAccountOf::convert_location(&owner).ok_or(BadOwner)?;
		let amount = T::CurrencyMatcher::matches_fungible(&asset).ok_or(UnknownAsset)?;
		ensure!(T::Currency::free_balance(&sovereign_account) >= amount, AssetNotOwned);
		let locks = LockedFungibles::<T>::get(&sovereign_account).unwrap_or_default();
		let item_index = locks.iter().position(|x| x.1.try_as::<_>() == Ok(&unlocker));
		ensure!(item_index.is_some() || locks.len() < T::MaxLockers::get() as usize, NoResources);
		Ok(LockTicket { sovereign_account, amount, unlocker, item_index })
	}

	fn prepare_unlock(
		unlocker: Location,
		asset: Asset,
		owner: Location,
	) -> Result<UnlockTicket<T>, xcm_executor::traits::LockError> {
		use xcm_executor::traits::LockError::*;
		let sovereign_account = T::SovereignAccountOf::convert_location(&owner).ok_or(BadOwner)?;
		let amount = T::CurrencyMatcher::matches_fungible(&asset).ok_or(UnknownAsset)?;
		ensure!(T::Currency::free_balance(&sovereign_account) >= amount, AssetNotOwned);
		let locks = LockedFungibles::<T>::get(&sovereign_account).unwrap_or_default();
		let item_index =
			locks.iter().position(|x| x.1.try_as::<_>() == Ok(&unlocker)).ok_or(NotLocked)?;
		ensure!(locks[item_index].0 >= amount, NotLocked);
		Ok(UnlockTicket { sovereign_account, amount, unlocker })
	}

	fn note_unlockable(
		locker: Location,
		asset: Asset,
		mut owner: Location,
	) -> Result<(), xcm_executor::traits::LockError> {
		use xcm_executor::traits::LockError::*;
		ensure!(T::TrustedLockers::contains(&locker, &asset), NotTrusted);
		let amount = match asset.fun {
			Fungible(a) => a,
			NonFungible(_) => return Err(Unimplemented),
		};
		owner.remove_network_id();
		let account = T::SovereignAccountOf::convert_location(&owner).ok_or(BadOwner)?;
		let locker = locker.into();
		let owner = owner.into();
		let id: VersionedAssetId = asset.id.into();
		let key = (XCM_VERSION, account, id);
		let mut record =
			RemoteLockedFungibleRecord { amount, owner, locker, consumers: BoundedVec::default() };
		if let Some(old) = RemoteLockedFungibles::<T>::get(&key) {
			// Make sure that the new record wouldn't clobber any old data.
			ensure!(old.locker == record.locker && old.owner == record.owner, WouldClobber);
			record.consumers = old.consumers;
			record.amount = record.amount.max(old.amount);
		}
		RemoteLockedFungibles::<T>::insert(&key, record);
		Ok(())
	}

	fn prepare_reduce_unlockable(
		locker: Location,
		asset: Asset,
		mut owner: Location,
	) -> Result<Self::ReduceTicket, xcm_executor::traits::LockError> {
		use xcm_executor::traits::LockError::*;
		let amount = match asset.fun {
			Fungible(a) => a,
			NonFungible(_) => return Err(Unimplemented),
		};
		owner.remove_network_id();
		let sovereign_account = T::SovereignAccountOf::convert_location(&owner).ok_or(BadOwner)?;
		let locker = locker.into();
		let owner = owner.into();
		let id: VersionedAssetId = asset.id.into();
		let key = (XCM_VERSION, sovereign_account, id);

		let record = RemoteLockedFungibles::<T>::get(&key).ok_or(NotLocked)?;
		// Make sure that the record contains what we expect and there's enough to unlock.
		ensure!(locker == record.locker && owner == record.owner, WouldClobber);
		ensure!(record.amount >= amount, NotEnoughLocked);
		ensure!(
			record.amount_held().map_or(true, |h| record.amount.saturating_sub(amount) >= h),
			InUse
		);
		Ok(ReduceTicket { key, amount, locker, owner })
	}
}

impl<T: Config> WrapVersion for Pallet<T> {
	fn wrap_version<RuntimeCall>(
		dest: &Location,
		xcm: impl Into<VersionedXcm<RuntimeCall>>,
	) -> Result<VersionedXcm<RuntimeCall>, ()> {
		Self::get_version_for(dest)
			.or_else(|| {
				Self::note_unknown_version(dest);
				SafeXcmVersion::<T>::get()
			})
			.ok_or_else(|| {
				log::trace!(
					target: "xcm::pallet_xcm::wrap_version",
					"Could not determine a version to wrap XCM for destination: {:?}",
					dest,
				);
				()
			})
			.and_then(|v| xcm.into().into_version(v.min(XCM_VERSION)))
	}
}

impl<T: Config> GetVersion for Pallet<T> {
	fn get_version_for(dest: &Location) -> Option<XcmVersion> {
		SupportedVersion::<T>::get(XCM_VERSION, LatestVersionedLocation(dest))
	}
}

impl<T: Config> VersionChangeNotifier for Pallet<T> {
	/// Start notifying `location` should the XCM version of this chain change.
	///
	/// When it does, this type should ensure a `QueryResponse` message is sent with the given
	/// `query_id` & `max_weight` and with a `response` of `Response::Version`. This should happen
	/// until/unless `stop` is called with the correct `query_id`.
	///
	/// If the `location` has an ongoing notification and when this function is called, then an
	/// error should be returned.
	fn start(
		dest: &Location,
		query_id: QueryId,
		max_weight: Weight,
		_context: &XcmContext,
	) -> XcmResult {
		let versioned_dest = LatestVersionedLocation(dest);
		let already = VersionNotifyTargets::<T>::contains_key(XCM_VERSION, versioned_dest);
		ensure!(!already, XcmError::InvalidLocation);

		let xcm_version = T::AdvertisedXcmVersion::get();
		let response = Response::Version(xcm_version);
		let instruction = QueryResponse { query_id, response, max_weight, querier: None };
		let (message_id, cost) = send_xcm::<T::XcmRouter>(dest.clone(), Xcm(vec![instruction]))?;
		Self::deposit_event(Event::<T>::VersionNotifyStarted {
			destination: dest.clone(),
			cost,
			message_id,
		});

		let value = (query_id, max_weight, xcm_version);
		VersionNotifyTargets::<T>::insert(XCM_VERSION, versioned_dest, value);
		Ok(())
	}

	/// Stop notifying `location` should the XCM change. This is a no-op if there was never a
	/// subscription.
	fn stop(dest: &Location, _context: &XcmContext) -> XcmResult {
		VersionNotifyTargets::<T>::remove(XCM_VERSION, LatestVersionedLocation(dest));
		Ok(())
	}

	/// Return true if a location is subscribed to XCM version changes.
	fn is_subscribed(dest: &Location) -> bool {
		let versioned_dest = LatestVersionedLocation(dest);
		VersionNotifyTargets::<T>::contains_key(XCM_VERSION, versioned_dest)
	}
}

impl<T: Config> DropAssets for Pallet<T> {
	fn drop_assets(origin: &Location, assets: AssetsInHolding, _context: &XcmContext) -> Weight {
		if assets.is_empty() {
			return Weight::zero()
		}
		let versioned = VersionedAssets::from(Assets::from(assets));
		let hash = BlakeTwo256::hash_of(&(&origin, &versioned));
		AssetTraps::<T>::mutate(hash, |n| *n += 1);
		Self::deposit_event(Event::AssetsTrapped {
			hash,
			origin: origin.clone(),
			assets: versioned,
		});
		// TODO #3735: Put the real weight in there.
		Weight::zero()
	}
}

impl<T: Config> ClaimAssets for Pallet<T> {
	fn claim_assets(
		origin: &Location,
		ticket: &Location,
		assets: &Assets,
		_context: &XcmContext,
	) -> bool {
		let mut versioned = VersionedAssets::from(assets.clone());
		match ticket.unpack() {
			(0, [GeneralIndex(i)]) =>
				versioned = match versioned.into_version(*i as u32) {
					Ok(v) => v,
					Err(()) => return false,
				},
			(0, []) => (),
			_ => return false,
		};
		let hash = BlakeTwo256::hash_of(&(origin.clone(), versioned.clone()));
		match AssetTraps::<T>::get(hash) {
			0 => return false,
			1 => AssetTraps::<T>::remove(hash),
			n => AssetTraps::<T>::insert(hash, n - 1),
		}
		Self::deposit_event(Event::AssetsClaimed {
			hash,
			origin: origin.clone(),
			assets: versioned,
		});
		return true
	}
}

impl<T: Config> OnResponse for Pallet<T> {
	fn expecting_response(
		origin: &Location,
		query_id: QueryId,
		querier: Option<&Location>,
	) -> bool {
		match Queries::<T>::get(query_id) {
			Some(QueryStatus::Pending { responder, maybe_match_querier, .. }) =>
				Location::try_from(responder).map_or(false, |r| origin == &r) &&
					maybe_match_querier.map_or(true, |match_querier| {
						Location::try_from(match_querier).map_or(false, |match_querier| {
							querier.map_or(false, |q| q == &match_querier)
						})
					}),
			Some(QueryStatus::VersionNotifier { origin: r, .. }) =>
				Location::try_from(r).map_or(false, |r| origin == &r),
			_ => false,
		}
	}

	fn on_response(
		origin: &Location,
		query_id: QueryId,
		querier: Option<&Location>,
		response: Response,
		max_weight: Weight,
		_context: &XcmContext,
	) -> Weight {
		let origin = origin.clone();
		match (response, Queries::<T>::get(query_id)) {
			(
				Response::Version(v),
				Some(QueryStatus::VersionNotifier { origin: expected_origin, is_active }),
			) => {
				let origin: Location = match expected_origin.try_into() {
					Ok(o) if o == origin => o,
					Ok(o) => {
						Self::deposit_event(Event::InvalidResponder {
							origin: origin.clone(),
							query_id,
							expected_location: Some(o),
						});
						return Weight::zero()
					},
					_ => {
						Self::deposit_event(Event::InvalidResponder {
							origin: origin.clone(),
							query_id,
							expected_location: None,
						});
						// TODO #3735: Correct weight for this.
						return Weight::zero()
					},
				};
				// TODO #3735: Check max_weight is correct.
				if !is_active {
					Queries::<T>::insert(
						query_id,
						QueryStatus::VersionNotifier {
							origin: origin.clone().into(),
							is_active: true,
						},
					);
				}
				// We're being notified of a version change.
				SupportedVersion::<T>::insert(XCM_VERSION, LatestVersionedLocation(&origin), v);
				Self::deposit_event(Event::SupportedVersionChanged {
					location: origin,
					version: v,
				});
				Weight::zero()
			},
			(
				response,
				Some(QueryStatus::Pending { responder, maybe_notify, maybe_match_querier, .. }),
			) => {
				if let Some(match_querier) = maybe_match_querier {
					let match_querier = match Location::try_from(match_querier) {
						Ok(mq) => mq,
						Err(_) => {
							Self::deposit_event(Event::InvalidQuerierVersion {
								origin: origin.clone(),
								query_id,
							});
							return Weight::zero()
						},
					};
					if querier.map_or(true, |q| q != &match_querier) {
						Self::deposit_event(Event::InvalidQuerier {
							origin: origin.clone(),
							query_id,
							expected_querier: match_querier,
							maybe_actual_querier: querier.cloned(),
						});
						return Weight::zero()
					}
				}
				let responder = match Location::try_from(responder) {
					Ok(r) => r,
					Err(_) => {
						Self::deposit_event(Event::InvalidResponderVersion {
							origin: origin.clone(),
							query_id,
						});
						return Weight::zero()
					},
				};
				if origin != responder {
					Self::deposit_event(Event::InvalidResponder {
						origin: origin.clone(),
						query_id,
						expected_location: Some(responder),
					});
					return Weight::zero()
				}
				return match maybe_notify {
					Some((pallet_index, call_index)) => {
						// This is a bit horrible, but we happen to know that the `Call` will
						// be built by `(pallet_index: u8, call_index: u8, QueryId, Response)`.
						// So we just encode that and then re-encode to a real Call.
						let bare = (pallet_index, call_index, query_id, response);
						if let Ok(call) = bare.using_encoded(|mut bytes| {
							<T as Config>::RuntimeCall::decode(&mut bytes)
						}) {
							Queries::<T>::remove(query_id);
							let weight = call.get_dispatch_info().weight;
							if weight.any_gt(max_weight) {
								let e = Event::NotifyOverweight {
									query_id,
									pallet_index,
									call_index,
									actual_weight: weight,
									max_budgeted_weight: max_weight,
								};
								Self::deposit_event(e);
								return Weight::zero()
							}
							let dispatch_origin = Origin::Response(origin.clone()).into();
							match call.dispatch(dispatch_origin) {
								Ok(post_info) => {
									let e = Event::Notified { query_id, pallet_index, call_index };
									Self::deposit_event(e);
									post_info.actual_weight
								},
								Err(error_and_info) => {
									let e = Event::NotifyDispatchError {
										query_id,
										pallet_index,
										call_index,
									};
									Self::deposit_event(e);
									// Not much to do with the result as it is. It's up to the
									// parachain to ensure that the message makes sense.
									error_and_info.post_info.actual_weight
								},
							}
							.unwrap_or(weight)
						} else {
							let e =
								Event::NotifyDecodeFailed { query_id, pallet_index, call_index };
							Self::deposit_event(e);
							Weight::zero()
						}
					},
					None => {
						let e = Event::ResponseReady { query_id, response: response.clone() };
						Self::deposit_event(e);
						let at = frame_system::Pallet::<T>::current_block_number();
						let response = response.into();
						Queries::<T>::insert(query_id, QueryStatus::Ready { response, at });
						Weight::zero()
					},
				}
			},
			_ => {
				let e = Event::UnexpectedResponse { origin: origin.clone(), query_id };
				Self::deposit_event(e);
				Weight::zero()
			},
		}
	}
}

impl<T: Config> CheckSuspension for Pallet<T> {
	fn is_suspended<Call>(
		_origin: &Location,
		_instructions: &mut [Instruction<Call>],
		_max_weight: Weight,
		_properties: &mut Properties,
	) -> bool {
		XcmExecutionSuspended::<T>::get()
	}
}

/// Ensure that the origin `o` represents an XCM (`Transact`) origin.
///
/// Returns `Ok` with the location of the XCM sender or an `Err` otherwise.
pub fn ensure_xcm<OuterOrigin>(o: OuterOrigin) -> Result<Location, BadOrigin>
where
	OuterOrigin: Into<Result<Origin, OuterOrigin>>,
{
	match o.into() {
		Ok(Origin::Xcm(location)) => Ok(location),
		_ => Err(BadOrigin),
	}
}

/// Ensure that the origin `o` represents an XCM response origin.
///
/// Returns `Ok` with the location of the responder or an `Err` otherwise.
pub fn ensure_response<OuterOrigin>(o: OuterOrigin) -> Result<Location, BadOrigin>
where
	OuterOrigin: Into<Result<Origin, OuterOrigin>>,
{
	match o.into() {
		Ok(Origin::Response(location)) => Ok(location),
		_ => Err(BadOrigin),
	}
}

/// Filter for `Location` to find those which represent a strict majority approval of an
/// identified plurality.
///
/// May reasonably be used with `EnsureXcm`.
pub struct IsMajorityOfBody<Prefix, Body>(PhantomData<(Prefix, Body)>);
impl<Prefix: Get<Location>, Body: Get<BodyId>> Contains<Location>
	for IsMajorityOfBody<Prefix, Body>
{
	fn contains(l: &Location) -> bool {
		let maybe_suffix = l.match_and_split(&Prefix::get());
		matches!(maybe_suffix, Some(Plurality { id, part }) if id == &Body::get() && part.is_majority())
	}
}

/// Filter for `Location` to find those which represent a voice of an identified plurality.
///
/// May reasonably be used with `EnsureXcm`.
pub struct IsVoiceOfBody<Prefix, Body>(PhantomData<(Prefix, Body)>);
impl<Prefix: Get<Location>, Body: Get<BodyId>> Contains<Location> for IsVoiceOfBody<Prefix, Body> {
	fn contains(l: &Location) -> bool {
		let maybe_suffix = l.match_and_split(&Prefix::get());
		matches!(maybe_suffix, Some(Plurality { id, part }) if id == &Body::get() && part == &BodyPart::Voice)
	}
}

/// `EnsureOrigin` implementation succeeding with a `Location` value to recognize and filter
/// the `Origin::Xcm` item.
pub struct EnsureXcm<F, L = Location>(PhantomData<(F, L)>);
impl<
		O: OriginTrait + From<Origin>,
		F: Contains<L>,
		L: TryFrom<Location> + TryInto<Location> + Clone,
	> EnsureOrigin<O> for EnsureXcm<F, L>
where
	O::PalletsOrigin: From<Origin> + TryInto<Origin, Error = O::PalletsOrigin>,
{
	type Success = L;

	fn try_origin(outer: O) -> Result<Self::Success, O> {
		outer.try_with_caller(|caller| {
			caller.try_into().and_then(|o| match o {
				Origin::Xcm(ref location)
					if F::contains(&location.clone().try_into().map_err(|_| o.clone().into())?) =>
					Ok(location.clone().try_into().map_err(|_| o.clone().into())?),
				Origin::Xcm(location) => Err(Origin::Xcm(location).into()),
				o => Err(o.into()),
			})
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<O, ()> {
		Ok(O::from(Origin::Xcm(Here.into())))
	}
}

/// `EnsureOrigin` implementation succeeding with a `Location` value to recognize and filter
/// the `Origin::Response` item.
pub struct EnsureResponse<F>(PhantomData<F>);
impl<O: OriginTrait + From<Origin>, F: Contains<Location>> EnsureOrigin<O> for EnsureResponse<F>
where
	O::PalletsOrigin: From<Origin> + TryInto<Origin, Error = O::PalletsOrigin>,
{
	type Success = Location;

	fn try_origin(outer: O) -> Result<Self::Success, O> {
		outer.try_with_caller(|caller| {
			caller.try_into().and_then(|o| match o {
				Origin::Response(responder) => Ok(responder),
				o => Err(o.into()),
			})
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<O, ()> {
		Ok(O::from(Origin::Response(Here.into())))
	}
}

/// A simple passthrough where we reuse the `Location`-typed XCM origin as the inner value of
/// this crate's `Origin::Xcm` value.
pub struct XcmPassthrough<RuntimeOrigin>(PhantomData<RuntimeOrigin>);
impl<RuntimeOrigin: From<crate::Origin>> ConvertOrigin<RuntimeOrigin>
	for XcmPassthrough<RuntimeOrigin>
{
	fn convert_origin(
		origin: impl Into<Location>,
		kind: OriginKind,
	) -> Result<RuntimeOrigin, Location> {
		let origin = origin.into();
		match kind {
			OriginKind::Xcm => Ok(crate::Origin::Xcm(origin).into()),
			_ => Err(origin),
		}
	}
}
