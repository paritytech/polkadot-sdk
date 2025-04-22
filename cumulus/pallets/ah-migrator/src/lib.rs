// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The operational pallet for the Asset Hub, designed to manage and facilitate the migration of
//! subsystems such as Governance, Staking, Balances from the Relay Chain to the Asset Hub. This
//! pallet works alongside its counterpart, `pallet_rc_migrator`, which handles migration
//! processes on the Relay Chain side.
//!
//! This pallet is responsible for controlling the initiation, progression, and completion of the
//! migration process, including managing its various stages and transferring the necessary data.
//! The pallet directly accesses the storage of other pallets for read/write operations while
//! maintaining compatibility with their existing APIs.
//!
//! To simplify development and avoid the need to edit the original pallets, this pallet may
//! duplicate private items such as storage entries from the original pallets. This ensures that the
//! migration logic can be implemented without altering the original implementations.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod account;
pub mod asset_rate;
#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(not(feature = "ahm-westend"))]
pub mod bounties;
pub mod call;
#[cfg(not(feature = "ahm-westend"))]
pub mod claims;
pub mod conviction_voting;
#[cfg(not(feature = "ahm-westend"))]
pub mod crowdloan;
pub mod indices;
pub mod multisig;
pub mod preimage;
pub mod proxy;
pub mod referenda;
pub mod scheduler;
pub mod staking;
#[cfg(not(feature = "ahm-westend"))]
pub mod treasury;
pub mod types;
pub mod vesting;
pub mod xcm_config;

pub use pallet::*;
pub use pallet_rc_migrator::{types::ZeroWeightOr, weights_ah};
pub use weights_ah::WeightInfo;

use frame_support::{
	pallet_prelude::*,
	storage::{transactional::with_transaction_opaque_err, TransactionOutcome},
	traits::{
		fungible::{Inspect, InspectFreeze, Mutate, MutateFreeze, MutateHold, Unbalanced},
		fungibles::{Inspect as FungiblesInspect, Mutate as FungiblesMutate},
		tokens::{Fortitude, Pay, Preservation},
		Contains, Defensive, DefensiveTruncateFrom, LockableCurrency, OriginTrait, QueryPreimage,
		ReservableCurrency, StorePreimage, VariantCount, WithdrawReasons as LockWithdrawReasons,
	},
};
use frame_system::pallet_prelude::*;
use pallet_balances::{AccountData, Reasons as LockReasons};

#[cfg(not(feature = "ahm-westend"))]
use pallet_rc_migrator::bounties::RcBountiesMessageOf;
#[cfg(not(feature = "ahm-westend"))]
use pallet_rc_migrator::claims::RcClaimsMessageOf;
#[cfg(not(feature = "ahm-westend"))]
use pallet_rc_migrator::crowdloan::RcCrowdloanMessageOf;
#[cfg(not(feature = "ahm-westend"))]
use pallet_rc_migrator::treasury::RcTreasuryMessage;

use pallet_rc_migrator::{
	accounts::Account as RcAccount,
	conviction_voting::RcConvictionVotingMessageOf,
	indices::RcIndicesIndexOf,
	multisig::*,
	preimage::*,
	proxy::*,
	staking::{bags_list::RcBagsListMessage, fast_unstake::RcFastUnstakeMessage, nom_pools::*, *},
	types::MigrationFinishedData,
	vesting::RcVestingSchedule,
};
use pallet_referenda::{ReferendumInfo, TrackIdOf};
use polkadot_runtime_common::{claims as pallet_claims, impls::VersionedLocatableAsset};
use referenda::RcReferendumInfoOf;
use scheduler::RcSchedulerMessageOf;
use sp_application_crypto::Ss58Codec;
use sp_core::H256;
use sp_runtime::{
	traits::{BlockNumberProvider, Convert, TryConvert, Zero},
	AccountId32, FixedU128,
};
use sp_std::prelude::*;
use xcm::prelude::*;
use xcm_builder::MintLocation;

/// The log target of this pallet.
pub const LOG_TARGET: &str = "runtime::ah-migrator";

type RcAccountFor<T> = RcAccount<
	<T as frame_system::Config>::AccountId,
	<T as pallet_balances::Config>::Balance,
	<T as Config>::RcHoldReason,
	<T as Config>::RcFreezeReason,
>;

#[cfg(not(feature = "ahm-westend"))]
pub type RcTreasuryMessageOf<T> = RcTreasuryMessage<
	<T as frame_system::Config>::AccountId,
	pallet_treasury::BalanceOf<T, ()>,
	pallet_treasury::AssetBalanceOf<T, ()>,
	BlockNumberFor<T>,
	VersionedLocatableAsset,
	VersionedLocation,
	<<T as pallet_treasury::Config>::Paymaster as Pay>::Id,
>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum PalletEventName {
	Indices,
	FastUnstake,
	Crowdloan,
	BagsList,
	Vesting,
	Bounties,
	Treasury,
	Balances,
	Multisig,
	Claims,
	ProxyProxies,
	ProxyAnnouncements,
	PreimageChunk,
	PreimageRequestStatus,
	PreimageLegacyStatus,
	NomPools,
	ReferendaValues,
	ReferendaMetadata,
	ReferendaReferendums,
	Scheduler,
	SchedulerAgenda,
	ConvictionVoting,
	AssetRates,
	Staking,
}

/// The migration stage on the Asset Hub.
#[derive(Encode, Decode, Clone, Default, RuntimeDebug, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum MigrationStage {
	/// The migration has not started but will start in the future.
	#[default]
	Pending,
	/// Migrating data from the Relay Chain.
	DataMigrationOngoing,
	/// Migrating data from the Relay Chain is completed.
	DataMigrationDone,
	/// The migration is done.
	MigrationDone,
}

impl MigrationStage {
	/// Whether the migration is finished.
	///
	/// This is **not** the same as `!self.is_ongoing()` since it may not have started.
	pub fn is_finished(&self) -> bool {
		matches!(self, MigrationStage::MigrationDone)
	}

	/// Whether the migration is ongoing.
	///
	/// This is **not** the same as `!self.is_finished()` since it may not have started.
	pub fn is_ongoing(&self) -> bool {
		!matches!(self, MigrationStage::Pending | MigrationStage::MigrationDone)
	}
}

/// Helper struct storing certain balances before the migration.
#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct BalancesBefore<Balance: Default> {
	pub checking_account: Balance,
	pub total_issuance: Balance,
}

pub type BalanceOf<T> = <T as pallet_balances::Config>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::xcm_config::{TrustedTeleportersBeforeAfter, TrustedTeleportersDuring};
	use frame_support::traits::ContainsPair;

	/// Super config trait for all pallets that the migration depends on, providing convenient
	/// access to their items.
	#[pallet::config]
	pub trait Config:
		frame_system::Config<AccountData = AccountData<u128>, AccountId = AccountId32, Hash = H256>
		+ pallet_balances::Config<Balance = u128>
		+ pallet_multisig::Config
		+ pallet_proxy::Config<BlockNumberProvider = <Self as Config>::RcBlockNumberProvider>
		+ pallet_preimage::Config<Hash = H256>
		+ pallet_referenda::Config<BlockNumberProvider = <Self as Config>::RcBlockNumberProvider, Votes = u128>
		+ pallet_nomination_pools::Config<BlockNumberProvider = <Self as Config>::RcBlockNumberProvider>
		+ pallet_fast_unstake::Config
		+ pallet_bags_list::Config<pallet_bags_list::Instance1>
		+ pallet_scheduler::Config<BlockNumberProvider = <Self as Config>::RcBlockNumberProvider>
		+ pallet_vesting::Config
		+ pallet_indices::Config
		+ pallet_conviction_voting::Config
		+ pallet_asset_rate::Config
		+ pallet_timestamp::Config<Moment = u64> // Needed for testing
		+ pallet_ah_ops::Config
// 		+ pallet_claims::Config // Not on westend
// 		+ pallet_bounties::Config // Not on westend
// 		+ pallet_treasury::Config // Not on westend
+ pallet_staking::Config // Only on westend
	{
		type RuntimeHoldReason: Parameter + VariantCount;
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The origin that can perform permissioned operations like setting the migration stage.
		///
		/// This is generally root and Fellows origins.
		type ManagerOrigin: EnsureOrigin<<Self as frame_system::Config>::RuntimeOrigin>;
		/// Native asset registry type.
		type Currency: Mutate<Self::AccountId, Balance = u128>
			+ MutateHold<Self::AccountId, Reason = <Self as Config>::RuntimeHoldReason>
			+ InspectFreeze<Self::AccountId, Id = Self::FreezeIdentifier>
			+ MutateFreeze<Self::AccountId>
			+ Unbalanced<Self::AccountId>
			+ ReservableCurrency<Self::AccountId, Balance = u128>
			+ LockableCurrency<Self::AccountId, Balance = u128>;
		/// All supported assets registry.
		type Assets: FungiblesMutate<Self::AccountId>;
		/// XCM check account.
		/// 
		/// Note: the account ID is the same for Polkadot/Kusama Relay and Asset Hub Chains.
		type CheckingAccount: Get<Self::AccountId>;
		/// Relay Chain Hold Reasons.
		///
		/// Additionally requires the `Default` implementation for the benchmarking mocks.
		type RcHoldReason: Parameter + Default + MaxEncodedLen;
		/// Relay Chain Freeze Reasons.
		///
		/// Additionally requires the `Default` implementation for the benchmarking mocks.
		type RcFreezeReason: Parameter + Default + MaxEncodedLen;
		/// Relay Chain to Asset Hub Hold Reasons mapping.
		type RcToAhHoldReason: Convert<Self::RcHoldReason, <Self as Config>::RuntimeHoldReason>;
		/// Relay Chain to Asset Hub Freeze Reasons mapping.
		type RcToAhFreezeReason: Convert<Self::RcFreezeReason, Self::FreezeIdentifier>;
		/// The abridged Relay Chain Proxy Type.
		///
		/// Additionally requires the `Default` implementation for the benchmarking mocks.
		type RcProxyType: Parameter + Default;
		/// Convert a Relay Chain Proxy Type to a local AH one.
		type RcToProxyType: TryConvert<Self::RcProxyType, <Self as pallet_proxy::Config>::ProxyType>;
		/// Convert a Relay Chain block number delay to an Asset Hub one.
		///
		/// Note that we make a simplification here by assuming that both chains have the same block
		/// number type.
		type RcToAhDelay: Convert<BlockNumberFor<Self>, BlockNumberFor<Self>>;
		/// Access the block number of the Relay Chain.
		type RcBlockNumberProvider: BlockNumberProvider<BlockNumber = BlockNumberFor<Self>>;
		/// Some part of the Relay Chain origins used in Governance.
		///
		/// Additionally requires the `Default` implementation for the benchmarking mocks.
		type RcPalletsOrigin: Parameter + Default;
		/// Convert a Relay Chain origin to an Asset Hub one.
		type RcToAhPalletsOrigin: TryConvert<
			Self::RcPalletsOrigin,
			<<Self as frame_system::Config>::RuntimeOrigin as OriginTrait>::PalletsOrigin,
		>;
		/// Preimage registry.
		type Preimage: QueryPreimage<H = <Self as frame_system::Config>::Hashing> + StorePreimage;
		/// Convert a Relay Chain Call to a local AH one.
		type RcToAhCall: for<'a> TryConvert<&'a [u8], <Self as frame_system::Config>::RuntimeCall>;
		/// Send UMP message.
		type SendXcm: SendXcm;
		/// Weight information for extrinsics in this pallet.
		type AhWeightInfo: WeightInfo;
		/// Asset Hub Treasury accounts migrating to the new treasury account address (same account
		/// address that was used on the Relay Chain).
		///
		/// The provided asset ids should be manageable by the [`Self::Assets`] registry. The asset
		/// list should not include the native asset.
		#[cfg(not(feature = "ahm-westend"))]
		type TreasuryAccounts: Get<(Self::AccountId, Vec<<Self::Assets as FungiblesInspect<Self::AccountId>>::AssetId>)>;
		/// Convert the Relay Chain Treasury Spend (AssetKind, Beneficiary) parameters to the
		/// Asset Hub (AssetKind, Beneficiary) parameters.
		#[cfg(not(feature = "ahm-westend"))]
		type RcToAhTreasurySpend: Convert<
			(VersionedLocatableAsset, VersionedLocation),
			Result<
				(
					<Self as pallet_treasury::Config>::AssetKind,
					<Self as pallet_treasury::Config>::Beneficiary,
				),
				(),
			>,
		>;

		/// Calls that are allowed during the migration.
		type AhIntraMigrationCalls: Contains<<Self as frame_system::Config>::RuntimeCall>;

		/// Calls that are allowed after the migration finished.
		type AhPostMigrationCalls: Contains<<Self as frame_system::Config>::RuntimeCall>;
	}

	/// RC accounts that failed to migrate when were received on the Asset Hub.
	///
	/// This is unlikely to happen, since we dry run the migration, but we keep it for completeness.
	#[pallet::storage]
	pub type RcAccounts<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, RcAccountFor<T>, OptionQuery>;

	/// The Asset Hub migration state.
	#[pallet::storage]
	pub type AhMigrationStage<T: Config> = StorageValue<_, MigrationStage, ValueQuery>;

	/// The total number of XCM data messages processed from the Relay Chain and the number of XCM
	/// messages that encountered an error during processing.
	#[pallet::storage]
	pub type DmpDataMessageCounts<T: Config> = StorageValue<_, (u32, u32), ValueQuery>;

	/// Helper storage item to store the total balance / total issuance of native token at the start
	/// of the migration. Since teleports are disabled during migration, the total issuance will not
	/// change for other reason than the migration itself.
	#[pallet::storage]
	pub type AhBalancesBefore<T: Config> = StorageValue<_, BalancesBefore<T::Balance>, ValueQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// The error that should to be replaced by something meaningful.
		TODO,
		FailedToUnreserveDeposit,
		/// Failed to process an account data from RC.
		FailedToProcessAccount,
		/// Some item could not be inserted because it already exists.
		InsertConflict,
		/// Failed to convert RC type to AH type.
		FailedToConvertType,
		/// Failed to fetch preimage.
		PreimageNotFound,
		/// Failed to convert RC call to AH call.
		FailedToConvertCall,
		/// Failed to bound a call.
		FailedToBoundCall,
		/// Failed to send XCM message.
		XcmError,
		/// Failed to integrate a vesting schedule.
		FailedToIntegrateVestingSchedule,
		/// Checking account overflow or underflow.
		FailedToCalculateCheckingAccount,
		/// Vector did not fit into its compile-time bound.
		FailedToBoundVector,
		Unreachable,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A stage transition has occurred.
		StageTransition {
			/// The old stage before the transition.
			old: MigrationStage,
			/// The new stage after the transition.
			new: MigrationStage,
		},
		/// We received a batch of messages that will be integrated into a pallet.
		BatchReceived { pallet: PalletEventName, count: u32 },
		/// We processed a batch of messages for this pallet.
		BatchProcessed { pallet: PalletEventName, count_good: u32, count_bad: u32 },
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Receive accounts from the Relay Chain.
		///
		/// The accounts sent with `pallet_rc_migrator::Pallet::migrate_accounts` function.
		#[pallet::call_index(0)]
		#[pallet::weight({
			let mut total = Weight::zero();
			let weight_of = |account: &RcAccountFor<T>| if account.is_liquid() {
				T::AhWeightInfo::receive_liquid_accounts
			} else {
				// TODO: use `T::AhWeightInfo::receive_accounts` with xcm v5, where 
				// `require_weight_at_most` not required
				T::AhWeightInfo::receive_liquid_accounts
			};
			for account in accounts.iter() {
				let weight = if total.is_zero() {
					weight_of(account)(1)
				} else {
					weight_of(account)(1).saturating_sub(weight_of(account)(0))
				};
				total = total.saturating_add(weight);
			}
			total
		})]
		pub fn receive_accounts(
			origin: OriginFor<T>,
			accounts: Vec<RcAccountFor<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_accounts(accounts);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		/// Receive multisigs from the Relay Chain.
		///
		/// This will be called from an XCM `Transact` inside a DMP from the relay chain. The
		/// multisigs were prepared by
		/// `pallet_rc_migrator::multisig::MultisigMigrator::migrate_many`.
		#[pallet::call_index(1)]
		#[pallet::weight(T::AhWeightInfo::receive_multisigs(accounts.len() as u32))]
		pub fn receive_multisigs(
			origin: OriginFor<T>,
			accounts: Vec<RcMultisigOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_multisigs(accounts);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		/// Receive proxies from the Relay Chain.
		#[pallet::call_index(2)]
		#[pallet::weight(T::AhWeightInfo::receive_proxy_proxies(proxies.len() as u32))]
		pub fn receive_proxy_proxies(
			origin: OriginFor<T>,
			proxies: Vec<RcProxyOf<T, T::RcProxyType>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_proxies(proxies);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		/// Receive proxy announcements from the Relay Chain.
		#[pallet::call_index(3)]
		#[pallet::weight(T::AhWeightInfo::receive_proxy_announcements(announcements.len() as u32))]
		pub fn receive_proxy_announcements(
			origin: OriginFor<T>,
			announcements: Vec<RcProxyAnnouncementOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_proxy_announcements(announcements);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(4)]
		#[pallet::weight({1})]
		// TODO use with xcm v5
		// #[pallet::weight({
		// 	let mut total = Weight::zero();
		// 	for chunk in chunks.iter() {
		// 		total = total.saturating_add(T::AhWeightInfo::receive_preimage_chunk(chunk.
		// chunk_byte_offset / chunks::CHUNK_SIZE)); 	}
		// 	total
		// })]
		pub fn receive_preimage_chunks(
			origin: OriginFor<T>,
			chunks: Vec<RcPreimageChunk>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_preimage_chunks(chunks);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(5)]
		#[pallet::weight(T::AhWeightInfo::receive_preimage_request_status(request_status.len() as u32))]
		pub fn receive_preimage_request_status(
			origin: OriginFor<T>,
			request_status: Vec<RcPreimageRequestStatusOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_preimage_request_statuses(request_status);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(6)]
		#[pallet::weight(T::AhWeightInfo::receive_preimage_legacy_status(legacy_status.len() as u32))]
		pub fn receive_preimage_legacy_status(
			origin: OriginFor<T>,
			legacy_status: Vec<RcPreimageLegacyStatusOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_preimage_legacy_statuses(legacy_status);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::AhWeightInfo::receive_nom_pools_messages(messages.len() as u32))]
		pub fn receive_nom_pools_messages(
			origin: OriginFor<T>,
			messages: Vec<RcNomPoolsMessage<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_nom_pools_messages(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(8)]
		#[pallet::weight(T::AhWeightInfo::receive_vesting_schedules(schedules.len() as u32))]
		pub fn receive_vesting_schedules(
			origin: OriginFor<T>,
			schedules: Vec<RcVestingSchedule<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_vesting_schedules(schedules);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(9)]
		#[pallet::weight(T::AhWeightInfo::receive_fast_unstake_messages(messages.len() as u32))]
		pub fn receive_fast_unstake_messages(
			origin: OriginFor<T>,
			messages: Vec<RcFastUnstakeMessage<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_fast_unstake_messages(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		/// Receive referendum counts, deciding counts, votes for the track queue.
		#[pallet::call_index(10)]
		#[pallet::weight(T::AhWeightInfo::receive_referenda_values())]
		pub fn receive_referenda_values(
			origin: OriginFor<T>,
			referendum_count: u32,
			// track_id, count
			deciding_count: Vec<(TrackIdOf<T, ()>, u32)>,
			// referendum_id, votes
			track_queue: Vec<(TrackIdOf<T, ()>, Vec<(u32, u128)>)>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res =
				Self::do_receive_referenda_values(referendum_count, deciding_count, track_queue);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		/// Receive referendums from the Relay Chain.
		#[pallet::call_index(11)]
		#[pallet::weight(T::AhWeightInfo::receive_complete_referendums(referendums.len() as u32))]
		// TODO: use with xcm v5
		// #[pallet::weight({
		// 	let mut total = Weight::zero();
		// 	for (_, info) in referendums.iter() {
		// 		let weight = match info {
		// 			ReferendumInfo::Ongoing(status) => {
		// 				let len = status.proposal.len().defensive_unwrap_or(
		// 					// should not happen, but we pick some sane call length.
		// 					512,
		// 				);
		// 				T::AhWeightInfo::receive_single_active_referendums(len)
		// 			},
		// 			_ =>
		// 				if total.is_zero() {
		// 					T::AhWeightInfo::receive_complete_referendums(1)
		// 				} else {
		// 					T::AhWeightInfo::receive_complete_referendums(1)
		// 						.saturating_sub(T::AhWeightInfo::receive_complete_referendums(0))
		// 				},
		// 		};
		// 		total = total.saturating_add(weight);
		// 	}
		// 	total
		// })]
		pub fn receive_referendums(
			origin: OriginFor<T>,
			referendums: Vec<(u32, RcReferendumInfoOf<T, ()>)>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_referendums(referendums);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[cfg(not(feature = "ahm-westend"))]
		#[pallet::call_index(12)]
		#[pallet::weight(T::AhWeightInfo::receive_claims(messages.len() as u32))]
		pub fn receive_claims(
			origin: OriginFor<T>,
			messages: Vec<RcClaimsMessageOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_claims(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(13)]
		#[pallet::weight(T::AhWeightInfo::receive_bags_list_messages(messages.len() as u32))]
		pub fn receive_bags_list_messages(
			origin: OriginFor<T>,
			messages: Vec<RcBagsListMessage<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_bags_list_messages(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(14)]
		#[pallet::weight(T::AhWeightInfo::receive_scheduler_lookup(messages.len() as u32))]
		pub fn receive_scheduler_messages(
			origin: OriginFor<T>,
			messages: Vec<RcSchedulerMessageOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_scheduler_messages(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(15)]
		#[pallet::weight(T::AhWeightInfo::receive_indices(indices.len() as u32))]
		pub fn receive_indices(
			origin: OriginFor<T>,
			indices: Vec<RcIndicesIndexOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_indices(indices);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(16)]
		#[pallet::weight(T::AhWeightInfo::receive_conviction_voting_messages(messages.len() as u32))]
		pub fn receive_conviction_voting_messages(
			origin: OriginFor<T>,
			messages: Vec<RcConvictionVotingMessageOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_conviction_voting_messages(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[cfg(not(feature = "ahm-westend"))]
		#[pallet::call_index(17)]
		#[pallet::weight(T::AhWeightInfo::receive_bounties_messages(messages.len() as u32))]
		pub fn receive_bounties_messages(
			origin: OriginFor<T>,
			messages: Vec<RcBountiesMessageOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_bounties_messages(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(18)]
		#[pallet::weight(T::AhWeightInfo::receive_asset_rates(rates.len() as u32))]
		pub fn receive_asset_rates(
			origin: OriginFor<T>,
			rates: Vec<(<T as pallet_asset_rate::Config>::AssetKind, FixedU128)>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_asset_rates(rates);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[cfg(not(feature = "ahm-westend"))]
		#[pallet::call_index(19)]
		#[pallet::weight(T::AhWeightInfo::receive_crowdloan_messages(messages.len() as u32))]
		pub fn receive_crowdloan_messages(
			origin: OriginFor<T>,
			messages: Vec<RcCrowdloanMessageOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_crowdloan_messages(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(20)]
		#[pallet::weight(T::AhWeightInfo::receive_referenda_metadata(metadata.len() as u32))]
		pub fn receive_referenda_metadata(
			origin: OriginFor<T>,
			metadata: Vec<(u32, <T as frame_system::Config>::Hash)>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_referenda_metadata(metadata);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[cfg(not(feature = "ahm-westend"))]
		#[pallet::call_index(21)]
		#[pallet::weight(T::AhWeightInfo::receive_treasury_messages(messages.len() as u32))]
		pub fn receive_treasury_messages(
			origin: OriginFor<T>,
			messages: Vec<RcTreasuryMessageOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_treasury_messages(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[pallet::call_index(22)]
		#[pallet::weight({1})]
		// TODO: use with xcm v5
		// #[pallet::weight({
		// 	let mut total = Weight::zero();
		// 	for (_, agenda) in messages.iter() {
		// 		for maybe_task in agenda {
		// 			let Some(task) = maybe_task else {
		// 				continue;
		// 			};
		// 			let preimage_len = task.call.len().defensive_unwrap_or(
		// 				// should not happen, but we assume some sane call length.
		// 				512,
		// 			);
		// 			total =
		// total.saturating_add(T::AhWeightInfo::receive_single_scheduler_agenda(preimage_len));
		// 		}
		// 	}
		// 	total
		// })]
		pub fn receive_scheduler_agenda_messages(
			origin: OriginFor<T>,
			messages: Vec<(BlockNumberFor<T>, Vec<Option<scheduler::RcScheduledOf<T>>>)>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_scheduler_agenda_messages(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		#[cfg(feature = "ahm-staking-migration")]
		#[pallet::call_index(30)]
		#[pallet::weight({1})] // TODO: weight
		pub fn receive_staking_messages(
			origin: OriginFor<T>,
			messages: Vec<RcStakingMessageOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let res = Self::do_receive_staking_messages(messages);

			Self::increment_msg_received_count(res.is_err());

			res.map_err(Into::into)
		}

		/// Set the migration stage.
		///
		/// This call is intended for emergency use only and is guarded by the
		/// [`Config::ManagerOrigin`].
		#[pallet::call_index(100)]
		#[pallet::weight(T::AhWeightInfo::force_set_stage())]
		pub fn force_set_stage(origin: OriginFor<T>, stage: MigrationStage) -> DispatchResult {
			<T as Config>::ManagerOrigin::ensure_origin(origin)?;
			Self::transition(stage);
			Ok(())
		}

		/// Start the data migration.
		///
		/// This is typically called by the Relay Chain to start the migration on the Asset Hub and
		/// receive a handshake message indicating the Asset Hub's readiness.
		#[pallet::call_index(101)]
		#[pallet::weight(T::AhWeightInfo::start_migration())]
		pub fn start_migration(origin: OriginFor<T>) -> DispatchResult {
			<T as Config>::ManagerOrigin::ensure_origin(origin)?;
			Self::send_xcm(types::RcMigratorCall::StartDataMigration)?;

			let checking_account = T::CheckingAccount::get();
			let balances_before = BalancesBefore {
				checking_account: <T as Config>::Currency::total_balance(&checking_account),
				total_issuance: <T as Config>::Currency::total_issuance(),
			};
			log::info!(
				target: LOG_TARGET,
				"start_migration(): checking_account_balance {:?}, total_issuance {:?}",
				balances_before.checking_account, balances_before.total_issuance
			);
			AhBalancesBefore::<T>::put(balances_before);

			Self::transition(MigrationStage::DataMigrationOngoing);
			Ok(())
		}

		/// Finish the migration.
		///
		/// This is typically called by the Relay Chain to signal the migration has finished.
		#[pallet::call_index(110)]
		#[pallet::weight(T::AhWeightInfo::finish_migration())]
		pub fn finish_migration(
			origin: OriginFor<T>,
			data: MigrationFinishedData<T::Balance>,
		) -> DispatchResult {
			<T as Config>::ManagerOrigin::ensure_origin(origin)?;
			Self::finish_accounts_migration(data.rc_balance_kept)?;
			Self::transition(MigrationStage::MigrationDone);
			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			T::AhWeightInfo::on_finalize()
		}

		fn on_finalize(_: BlockNumberFor<T>) {
			let (processed, _) = DmpDataMessageCounts::<T>::get();
			if processed == 0 {
				return;
			}
			log::info!(
				target: LOG_TARGET,
				"Sending XCM message to update XCM data message processed count: {}",
				processed
			);
			let res = Self::send_xcm(types::RcMigratorCall::UpdateAhMsgProcessedCount(processed));
			defensive_assert!(
				res.is_ok(),
				"Failed to send XCM message to update XCM data message processed count"
			);
		}
	}

	impl<T: Config> Pallet<T> {
		/// Increments the number of XCM messages received from the Relay Chain.
		fn increment_msg_received_count(with_error: bool) {
			let (processed, processed_with_error) = DmpDataMessageCounts::<T>::get();
			let processed = processed + 1;
			let processed_with_error = processed_with_error + if with_error { 1 } else { 0 };
			DmpDataMessageCounts::<T>::put((processed, processed_with_error));
			log::debug!(
				target: LOG_TARGET,
				"Increment XCM message processed, total processed: {}, failed: {}",
				processed,
				processed_with_error
			);
		}

		/// Execute a stage transition and log it.
		fn transition(new: MigrationStage) {
			let old = AhMigrationStage::<T>::get();
			AhMigrationStage::<T>::put(&new);
			log::info!(
				target: LOG_TARGET,
				"[Block {:?}] Stage transition: {:?} -> {:?}",
				frame_system::Pallet::<T>::block_number(),
				&old,
				&new
			);
			Self::deposit_event(Event::StageTransition { old, new });
		}

		/// Send a single XCM message.
		pub fn send_xcm(call: types::RcMigratorCall) -> Result<(), Error<T>> {
			log::debug!(target: LOG_TARGET, "Sending XCM message");

			let call = types::RcPalletConfig::RcmController(call);

			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				Instruction::Transact {
					origin_kind: OriginKind::Xcm,
					#[cfg(feature = "stable2503")]
					fallback_max_weight: None,
					#[cfg(not(feature = "stable2503"))]
					require_weight_at_most: Weight::from_all(1), // TODO
					call: call.encode().into(),
				},
			]);

			if let Err(err) = send_xcm::<T::SendXcm>(Location::parent(), message.clone()) {
				log::error!(target: LOG_TARGET, "Error while sending XCM message: {:?}", err);
				return Err(Error::XcmError);
			};

			Ok(())
		}

		pub fn teleport_tracking() -> Option<(T::AccountId, MintLocation)> {
			let stage = AhMigrationStage::<T>::get();
			if stage.is_finished() {
				Some((T::CheckingAccount::get(), MintLocation::Local))
			} else {
				None
			}
		}
	}

	impl<T: Config> pallet_rc_migrator::types::MigrationStatus for Pallet<T> {
		fn is_ongoing() -> bool {
			AhMigrationStage::<T>::get().is_ongoing()
		}
		fn is_finished() -> bool {
			AhMigrationStage::<T>::get().is_finished()
		}
	}

	// To be used for `IsTeleport` filter. Disallows DOT teleports during the migration.
	impl<T: Config> ContainsPair<Asset, Location> for Pallet<T> {
		fn contains(asset: &Asset, origin: &Location) -> bool {
			let stage = AhMigrationStage::<T>::get();
			log::trace!(target: "xcm::IsTeleport::contains", "migration stage: {:?}", stage);
			let result = if stage.is_ongoing() {
				TrustedTeleportersDuring::contains(asset, origin)
			} else {
				// before and after migration use normal filter
				TrustedTeleportersBeforeAfter::contains(asset, origin)
			};
			log::trace!(
				target: "xcm::IsTeleport::contains",
				"asset: {:?} origin {:?} result {:?}",
				asset, origin, result
			);
			result
		}
	}
}

impl<T: Config> Contains<<T as frame_system::Config>::RuntimeCall> for Pallet<T> {
	fn contains(call: &<T as frame_system::Config>::RuntimeCall) -> bool {
		let stage = AhMigrationStage::<T>::get();

		// We have to return whether the call is allowed:
		const ALLOWED: bool = true;
		const FORBIDDEN: bool = false;

		// Once the migration is finished, forbid calls not in the `RcPostMigrationCalls` set.
		if stage.is_finished() && !T::AhPostMigrationCalls::contains(call) {
			return FORBIDDEN;
		}

		// If the migration is ongoing, forbid calls not in the `RcIntraMigrationCalls` set.
		if stage.is_ongoing() && !T::AhIntraMigrationCalls::contains(call) {
			return FORBIDDEN;
		}

		// Otherwise, allow the call.
		// This also implicitly allows _any_ call if the migration has not yet started.
		ALLOWED
	}
}
