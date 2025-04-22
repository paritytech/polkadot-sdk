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

//! The operational pallet for the Relay Chain, designed to manage and facilitate the migration of
//! subsystems such as Governance, Staking, Balances from the Relay Chain to the Asset Hub. This
//! pallet works alongside its counterpart, `pallet_ah_migrator`, which handles migration
//! processes on the Asset Hub side.
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

pub mod accounts;
#[cfg(not(feature = "ahm-westend"))]
pub mod claims;
#[cfg(not(feature = "ahm-westend"))]
pub mod crowdloan;
pub mod indices;
pub mod multisig;
pub mod preimage;
pub mod proxy;
pub mod referenda;
pub mod staking;
pub mod types;
pub mod vesting;
pub mod weights;
pub mod weights_ah;
pub use pallet::*;
pub mod asset_rate;
#[cfg(not(feature = "ahm-westend"))]
pub mod bounties;
pub mod conviction_voting;
pub mod scheduler;
#[cfg(not(feature = "ahm-westend"))]
pub mod treasury;
pub mod xcm_config;

use crate::{
	accounts::MigratedBalances, types::MigrationFinishedData,
	xcm_config::TrustedTeleportersBeforeAndAfter,
};
use accounts::AccountsMigrator;
#[cfg(not(feature = "ahm-westend"))]
use claims::{ClaimsMigrator, ClaimsStage};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::traits::AccountIdConversion,
	storage::transactional::with_transaction_opaque_err,
	traits::{
		fungible::{Inspect, InspectFreeze, Mutate, MutateFreeze, MutateHold},
		schedule::DispatchTime,
		tokens::{Fortitude, Pay, Precision, Preservation},
		Contains, ContainsPair, Defensive, DefensiveTruncateFrom, LockableCurrency,
		ReservableCurrency, VariantCount,
	},
	weights::{Weight, WeightMeter},
};
use frame_system::{pallet_prelude::*, AccountInfo};
use indices::IndicesMigrator;
use multisig::MultisigMigrator;
use pallet_balances::AccountData;
use polkadot_parachain_primitives::primitives::Id as ParaId;
#[cfg(not(feature = "ahm-westend"))]
use polkadot_runtime_common::claims as pallet_claims;
use polkadot_runtime_common::{
	crowdloan as pallet_crowdloan, paras_registrar, slots as pallet_slots,
};
use preimage::{
	PreimageChunkMigrator, PreimageLegacyRequestStatusMigrator, PreimageRequestStatusMigrator,
};
use proxy::*;
use referenda::ReferendaStage;
use sp_core::{crypto::Ss58Codec, H256};
use sp_runtime::AccountId32;
use sp_std::prelude::*;
use staking::{
	bags_list::{BagsListMigrator, BagsListStage},
	fast_unstake::{FastUnstakeMigrator, FastUnstakeStage},
	nom_pools::{NomPoolsMigrator, NomPoolsStage},
};
use storage::TransactionOutcome;
use types::PalletMigration;
use vesting::VestingMigrator;
use weights::WeightInfo;
use weights_ah::WeightInfo as AhWeightInfo;
use xcm::prelude::*;
use xcm_builder::MintLocation;

#[cfg(feature = "ahm-polkadot")]
use runtime_parachains::hrmp;
// For westend
#[cfg(feature = "ahm-westend")]
use polkadot_runtime_parachains::hrmp;

/// The log target of this pallet.
pub const LOG_TARGET: &str = "runtime::rc-migrator";

/// Soft limit on the DMP message size.
///
/// The hard limit should be about 64KiB (TODO test) which means that we stay well below that to
/// avoid any trouble. We can raise this as final preparation for the migration once everything is
/// confirmed to work.
pub const MAX_XCM_SIZE: u32 = 50_000;

/// Out of weight Error. Can be converted to a pallet error for convenience.
pub struct OutOfWeightError;

impl<T: Config> From<OutOfWeightError> for Error<T> {
	fn from(_: OutOfWeightError) -> Self {
		Self::OutOfWeight
	}
}

pub type MigrationStageOf<T> = MigrationStage<
	<T as frame_system::Config>::AccountId,
	BlockNumberFor<T>,
	<T as pallet_bags_list::Config<pallet_bags_list::Instance1>>::Score,
	conviction_voting::alias::ClassOf<T>,
	<T as pallet_asset_rate::Config>::AssetKind,
	scheduler::SchedulerBlockNumberFor<T>,
>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum PalletEventName {
	FastUnstake,
	BagsList,
}

pub type BalanceOf<T> = <T as pallet_balances::Config>::Balance;

#[derive(Encode, Decode, Clone, Default, RuntimeDebug, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum MigrationStage<
	AccountId,
	BlockNumber,
	BagsListScore,
	VotingClass,
	AssetKind,
	SchedulerBlockNumber,
> {
	/// The migration has not yet started but will start in the future.
	#[default]
	Pending,
	/// The migration has been scheduled to start at the given block number.
	Scheduled {
		block_number: BlockNumber,
	},
	/// The migration is initializing.
	///
	/// This stage involves waiting for the notification from the Asset Hub that it is ready to
	/// receive the migration data.
	Initializing,
	/// Initializing the account migration process.
	AccountsMigrationInit,
	/// Migrating account balances.
	AccountsMigrationOngoing {
		// Last migrated account
		last_key: Option<AccountId>,
	},
	/// Note that this stage does not have any logic attached to itself. It just exists to make it
	/// easier to swap out what stage should run next for testing.
	AccountsMigrationDone,

	MultisigMigrationInit,
	MultisigMigrationOngoing {
		/// Last migrated key of the `Multisigs` double map.
		last_key: Option<(AccountId, [u8; 32])>,
	},
	MultisigMigrationDone,

	#[cfg(not(feature = "ahm-westend"))]
	ClaimsMigrationInit,
	#[cfg(not(feature = "ahm-westend"))]
	ClaimsMigrationOngoing {
		current_key: Option<ClaimsStage<AccountId>>,
	},
	ClaimsMigrationDone,

	ProxyMigrationInit,
	/// Currently migrating the proxies of the proxy pallet.
	ProxyMigrationProxies {
		last_key: Option<AccountId>,
	},
	/// Currently migrating the announcements of the proxy pallet.
	ProxyMigrationAnnouncements {
		last_key: Option<AccountId>,
	},
	ProxyMigrationDone,

	PreimageMigrationInit,
	PreimageMigrationChunksOngoing {
		// TODO type
		last_key: Option<((H256, u32), u32)>,
	},
	PreimageMigrationChunksDone,
	PreimageMigrationRequestStatusOngoing {
		next_key: Option<H256>,
	},
	PreimageMigrationRequestStatusDone,
	PreimageMigrationLegacyRequestStatusInit,
	PreimageMigrationLegacyRequestStatusOngoing {
		next_key: Option<H256>,
	},
	PreimageMigrationLegacyRequestStatusDone,
	PreimageMigrationDone,

	NomPoolsMigrationInit,
	NomPoolsMigrationOngoing {
		next_key: Option<NomPoolsStage<AccountId>>,
	},
	NomPoolsMigrationDone,

	VestingMigrationInit,
	VestingMigrationOngoing {
		next_key: Option<AccountId>,
	},
	VestingMigrationDone,

	FastUnstakeMigrationInit,
	FastUnstakeMigrationOngoing {
		next_key: Option<FastUnstakeStage<AccountId>>,
	},
	FastUnstakeMigrationDone,

	IndicesMigrationInit,
	IndicesMigrationOngoing {
		next_key: Option<()>,
	},
	IndicesMigrationDone,

	ReferendaMigrationInit,
	ReferendaMigrationOngoing {
		last_key: Option<ReferendaStage>,
	},
	ReferendaMigrationDone,

	BagsListMigrationInit,
	BagsListMigrationOngoing {
		next_key: Option<BagsListStage<AccountId, BagsListScore>>,
	},
	BagsListMigrationDone,
	SchedulerMigrationInit,
	SchedulerMigrationOngoing {
		last_key: Option<scheduler::SchedulerStage<SchedulerBlockNumber>>,
	},
	SchedulerAgendaMigrationOngoing {
		last_key: Option<BlockNumber>,
	},
	SchedulerMigrationDone,
	ConvictionVotingMigrationInit,
	ConvictionVotingMigrationOngoing {
		last_key: Option<conviction_voting::ConvictionVotingStage<AccountId, VotingClass>>,
	},
	ConvictionVotingMigrationDone,

	#[cfg(not(feature = "ahm-westend"))]
	BountiesMigrationInit,
	#[cfg(not(feature = "ahm-westend"))]
	BountiesMigrationOngoing {
		last_key: Option<bounties::BountiesStage>,
	},
	BountiesMigrationDone,

	AssetRateMigrationInit,
	AssetRateMigrationOngoing {
		last_key: Option<AssetKind>,
	},
	AssetRateMigrationDone,

	#[cfg(not(feature = "ahm-westend"))]
	CrowdloanMigrationInit,
	#[cfg(not(feature = "ahm-westend"))]
	CrowdloanMigrationOngoing {
		last_key: Option<crowdloan::CrowdloanStage>,
	},
	CrowdloanMigrationDone,

	#[cfg(not(feature = "ahm-westend"))]
	TreasuryMigrationInit,
	#[cfg(not(feature = "ahm-westend"))]
	TreasuryMigrationOngoing {
		last_key: Option<treasury::TreasuryStage>,
	},
	TreasuryMigrationDone,

	#[cfg(feature = "ahm-staking-migration")]
	StakingMigrationInit,
	#[cfg(feature = "ahm-staking-migration")]
	StakingMigrationOngoing {
		next_key: Option<staking::StakingStage<AccountId>>,
	},
	StakingMigrationDone,

	SignalMigrationFinish,
	MigrationDone,
}

impl<AccountId, BlockNumber, BagsListScore, VotingClass, AssetKind, SchedulerBlockNumber>
	MigrationStage<AccountId, BlockNumber, BagsListScore, VotingClass, AssetKind, SchedulerBlockNumber>
{
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
		!matches!(
			self,
			MigrationStage::Pending |
				MigrationStage::Scheduled { .. } |
				MigrationStage::MigrationDone
		)
	}
}

#[cfg(feature = "std")]
impl<AccountId, BlockNumber, BagsListScore, VotingClass, AssetKind, SchedulerBlockNumber>
	std::str::FromStr
	for MigrationStage<
		AccountId,
		BlockNumber,
		BagsListScore,
		VotingClass,
		AssetKind,
		SchedulerBlockNumber,
	>
{
	type Err = std::string::String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(match s {
			"skip-accounts" => MigrationStage::AccountsMigrationDone,
			#[cfg(not(feature = "ahm-westend"))]
			"crowdloan" => MigrationStage::CrowdloanMigrationInit,
			"preimage" => MigrationStage::PreimageMigrationInit,
			"referenda" => MigrationStage::ReferendaMigrationInit,
			"multisig" => MigrationStage::MultisigMigrationInit,
			"voting" => MigrationStage::ConvictionVotingMigrationInit,
			#[cfg(not(feature = "ahm-westend"))]
			"bounties" => MigrationStage::BountiesMigrationInit,
			"asset_rate" => MigrationStage::AssetRateMigrationInit,
			"indices" => MigrationStage::IndicesMigrationInit,
			#[cfg(not(feature = "ahm-westend"))]
			"treasury" => MigrationStage::TreasuryMigrationInit,
			"proxy" => MigrationStage::ProxyMigrationInit,
			"nom_pools" => MigrationStage::NomPoolsMigrationInit,
			"scheduler" => MigrationStage::SchedulerMigrationInit,
			other => return Err(format!("Unknown migration stage: {}", other)),
		})
	}
}

type AccountInfoFor<T> =
	AccountInfo<<T as frame_system::Config>::Nonce, <T as frame_system::Config>::AccountData>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// Paras Registrar Pallet
	type ParasRegistrar<T> = paras_registrar::Pallet<T>;

	/// Super config trait for all pallets that the migration depends on, providing convenient
	/// access to their items.
	#[pallet::config]
	pub trait Config:
		frame_system::Config<AccountData = AccountData<u128>, AccountId = AccountId32>
		+ pallet_balances::Config<RuntimeHoldReason = <Self as Config>::RuntimeHoldReason, Balance = u128>
		+ hrmp::Config
		+ paras_registrar::Config
		+ pallet_multisig::Config
		+ pallet_proxy::Config<BlockNumberProvider = frame_system::Pallet<Self>>
		+ pallet_preimage::Config<Hash = H256>
		+ pallet_referenda::Config<BlockNumberProvider = frame_system::Pallet<Self>, Votes = u128>
		+ pallet_nomination_pools::Config<BlockNumberProvider = frame_system::Pallet<Self>>
		+ pallet_fast_unstake::Config
		+ pallet_bags_list::Config<pallet_bags_list::Instance1>
		+ pallet_scheduler::Config<BlockNumberProvider = frame_system::Pallet<Self>>
		+ pallet_vesting::Config
		+ pallet_indices::Config
		+ pallet_conviction_voting::Config
		+ pallet_asset_rate::Config
		+ pallet_slots::Config
		+ pallet_crowdloan::Config
// 		+ pallet_staking::Config // Not on westend
+ pallet_staking::Config<RuntimeHoldReason = <Self as Config>::RuntimeHoldReason> // Only on westend
// 		+ pallet_claims::Config // Not on westend
// 		+ pallet_bounties::Config // Not on westend
// 		+ pallet_treasury::Config // Not on westend
	{
		type RuntimeHoldReason: Parameter + VariantCount;
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The origin that can perform permissioned operations like setting the migration stage.
		///
		/// This is generally root, Asset Hub and Fellows origins.
		type ManagerOrigin: EnsureOrigin<<Self as frame_system::Config>::RuntimeOrigin>;
		/// Native asset registry type.
		type Currency: Mutate<Self::AccountId, Balance = u128>
			+ MutateHold<Self::AccountId, Reason = <Self as Config>::RuntimeHoldReason>
			+ InspectFreeze<Self::AccountId, Id = Self::FreezeIdentifier>
			+ MutateFreeze<Self::AccountId>
			+ ReservableCurrency<Self::AccountId, Balance = u128>
			+ LockableCurrency<Self::AccountId, Balance = u128>;
		/// XCM checking account.
		type CheckingAccount: Get<Self::AccountId>;
		/// Send DMP message.
		type SendXcm: SendXcm;
		/// The maximum weight that this pallet can consume `on_initialize`.
		type MaxRcWeight: Get<Weight>;
		/// The maximum weight that Asset Hub can consume for processing one migration package.
		///
		/// Every data package that is sent from this pallet should not take more than this.
		type MaxAhWeight: Get<Weight>;
		/// Weight information for the functions of this pallet.
		type RcWeightInfo: WeightInfo;
		/// Weight information for the processing the packages from this pallet on the Asset Hub.
		type AhWeightInfo: AhWeightInfo;
		/// The existential deposit on the Asset Hub.
		type AhExistentialDeposit: Get<<Self as pallet_balances::Config>::Balance>;
		/// Contains calls that are allowed during the migration.
		type RcIntraMigrationCalls: Contains<<Self as frame_system::Config>::RuntimeCall>;
		/// Contains calls that are allowed after the migration finished.
		type RcPostMigrationCalls: Contains<<Self as frame_system::Config>::RuntimeCall>;
	}

	#[pallet::error]
	pub enum Error<T> {
		Unreachable,
		OutOfWeight,
		/// Failed to send XCM message to AH.
		XcmError,
		/// Failed to withdraw account from RC for migration to AH.
		FailedToWithdrawAccount,
		/// Indicates that the specified block number is in the past.
		PastBlockNumber,
		/// Balance accounting overflow.
		BalanceOverflow,
		/// Balance accounting underflow.
		BalanceUnderflow,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A stage transition has occurred.
		StageTransition {
			/// The old stage before the transition.
			old: MigrationStageOf<T>,
			/// The new stage after the transition.
			new: MigrationStageOf<T>,
		},
	}

	/// The Relay Chain migration state.
	#[pallet::storage]
	pub type RcMigrationStage<T: Config> = StorageValue<_, MigrationStageOf<T>, ValueQuery>;

	/// Helper storage item to obtain and store the known accounts that should be kept partially or
	/// fully on Relay Chain.
	#[pallet::storage]
	pub type RcAccounts<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, accounts::AccountState<T::Balance>, OptionQuery>;

	/// Helper storage item to store the total balance that should be kept on Relay Chain.
	#[pallet::storage]
	pub type RcMigratedBalance<T: Config> =
		StorageValue<_, MigratedBalances<T::Balance>, ValueQuery>;

	/// The total number of XCM data messages sent to the Asset Hub and the number of XCM messages
	/// the Asset Hub has confirmed as processed.
	///
	/// The difference between these two numbers are the messages that are "in-flight". We aim to
	/// keep this number low to not accidentally overload the asset hub.
	#[pallet::storage]
	pub type DmpDataMessageCounts<T: Config> = StorageValue<_, (u32, u32), ValueQuery>;

	/// Alias for `Paras` from `paras_registrar`.
	///
	/// The fields of the type stored in the original storage item are private, so we define the
	/// storage alias to get an access to them.
	#[frame_support::storage_alias(pallet_name)]
	pub type Paras<T: Config> = StorageMap<
		ParasRegistrar<T>,
		Twox64Concat,
		ParaId,
		types::ParaInfo<
			<T as frame_system::Config>::AccountId,
			<T as pallet_balances::Config>::Balance,
		>,
	>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the migration stage.
		///
		/// This call is intended for emergency use only and is guarded by the
		/// [`Config::ManagerOrigin`].
		#[pallet::call_index(0)]
		#[pallet::weight({0})] // TODO: weight
		pub fn force_set_stage(
			origin: OriginFor<T>,
			stage: Box<MigrationStageOf<T>>,
		) -> DispatchResult {
			<T as Config>::ManagerOrigin::ensure_origin(origin)?;
			Self::transition(*stage);
			Ok(())
		}

		/// Schedule the migration to start at a given moment.
		#[pallet::call_index(1)]
		#[pallet::weight({0})] // TODO: weight
		pub fn schedule_migration(
			origin: OriginFor<T>,
			start_moment: DispatchTime<BlockNumberFor<T>>,
		) -> DispatchResult {
			<T as Config>::ManagerOrigin::ensure_origin(origin)?;
			let now = frame_system::Pallet::<T>::block_number();
			let block_number = start_moment.evaluate(now);
			ensure!(block_number > now, Error::<T>::PastBlockNumber);
			Self::transition(MigrationStage::Scheduled { block_number });
			Ok(())
		}

		/// Start the data migration.
		///
		/// This is typically called by the Asset Hub to indicate it's readiness to receive the
		/// migration data.
		#[pallet::call_index(2)]
		#[pallet::weight({0})] // TODO: weight
		pub fn start_data_migration(origin: OriginFor<T>) -> DispatchResult {
			<T as Config>::ManagerOrigin::ensure_origin(origin)?;
			Self::transition(MigrationStage::AccountsMigrationInit);
			Ok(())
		}

		/// Update the total number of XCM messages processed by the Asset Hub.
		#[pallet::call_index(3)]
		#[pallet::weight({0})] // TODO: weight
		pub fn update_ah_msg_processed_count(origin: OriginFor<T>, count: u32) -> DispatchResult {
			<T as Config>::ManagerOrigin::ensure_origin(origin)?;
			Self::update_msg_processed_count(count);
			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
		where
		crate::BalanceOf<T>:
			From<<<T as polkadot_runtime_common::slots::Config>::Currency as frame_support::traits::Currency<sp_runtime::AccountId32>>::Balance>,
		crate::BalanceOf<T>:
			From<<<<T as polkadot_runtime_common::crowdloan::Config>::Auctioneer as polkadot_runtime_common::traits::Auctioneer<<<<T as frame_system::Config>::Block as sp_runtime::traits::Block>::Header as sp_runtime::traits::Header>::Number>>::Currency as frame_support::traits::Currency<sp_runtime::AccountId32>>::Balance>,
	{
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			let mut weight_counter = WeightMeter::with_limit(T::MaxRcWeight::get());
			let stage = RcMigrationStage::<T>::get();
			weight_counter.consume(T::DbWeight::get().reads(1));

			if Self::has_excess_unconfirmed_dmp(&stage) {
				log::info!(
					target: LOG_TARGET,
					"Excess unconfirmed XCM messages, skipping the data extraction for this block."
				);
				return weight_counter.consumed();
			}

			match stage {
				MigrationStage::Pending => {
					return weight_counter.consumed();
				},
				MigrationStage::Scheduled { block_number } =>
					if now >= block_number {
						match Self::send_xcm(types::AhMigratorCall::<T>::StartMigration, T::AhWeightInfo::start_migration()) {
							Ok(_) => {
								Self::transition(MigrationStage::Initializing);
							},
							Err(_) => {
								defensive!(
									"Failed to send StartMigration message to AH, \
									retry with the next block"
								);
							},
						}
					},
				MigrationStage::Initializing => {
					// waiting AH to send a message and to start sending the data
					return weight_counter.consumed();
				},
				MigrationStage::AccountsMigrationInit => {
					// TODO: weights
					let _ = AccountsMigrator::<T>::obtain_rc_accounts();
					RcMigratedBalance::<T>::mutate(|tracker| {
						// initialize `kept` balance as total issuance, we'll substract from it as
						// we migrate accounts
						tracker.kept = <T as Config>::Currency::total_issuance();
					});

					Self::transition(MigrationStage::AccountsMigrationOngoing { last_key: None });
				},
				MigrationStage::AccountsMigrationOngoing { last_key } => {
					let res =
						with_transaction_opaque_err::<Option<T::AccountId>, Error<T>, _>(|| {
							match AccountsMigrator::<T>::migrate_many(last_key, &mut weight_counter)
							{
								Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
								Err(e) => TransactionOutcome::Rollback(Err(e)),
							}
						})
						.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							// accounts migration is completed
							Self::transition(MigrationStage::AccountsMigrationDone);
						},
						Ok(Some(last_key)) => {
							// accounts migration continues with the next block
							Self::transition(MigrationStage::AccountsMigrationOngoing {
								last_key: Some(last_key),
							});
						},
						Err(err) => {
							defensive!("Error while migrating accounts: {:?}", err);
							// stage unchanged, retry.
						},
					}
				},
				MigrationStage::AccountsMigrationDone => {
					AccountsMigrator::<T>::finish_balances_migration();
					// Note: swap this out for faster testing to skip some migrations
					Self::transition(MigrationStage::MultisigMigrationInit);
				},
				MigrationStage::MultisigMigrationInit => {
					Self::transition(MigrationStage::MultisigMigrationOngoing { last_key: None });
				},
				MigrationStage::MultisigMigrationOngoing { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match MultisigMigrator::<T, T::AhWeightInfo, T::MaxAhWeight>::migrate_many(
							last_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							// multisig migration is completed
							Self::transition(MigrationStage::MultisigMigrationDone);
						},
						Ok(Some(last_key)) => {
							// multisig migration continues with the next block
							Self::transition(MigrationStage::MultisigMigrationOngoing {
								last_key: Some(last_key),
							});
						},
						e => {
							defensive!("Error while migrating multisigs: {:?}", e);
						},
					}
				},
				MigrationStage::MultisigMigrationDone => {
					#[cfg(not(feature = "ahm-westend"))]
					Self::transition(MigrationStage::ClaimsMigrationInit);
					#[cfg(feature = "ahm-westend")]
					Self::transition(MigrationStage::ClaimsMigrationDone);
				},
				#[cfg(not(feature = "ahm-westend"))]
				MigrationStage::ClaimsMigrationInit => {
					Self::transition(MigrationStage::ClaimsMigrationOngoing { current_key: None });
				},
				#[cfg(not(feature = "ahm-westend"))]
				MigrationStage::ClaimsMigrationOngoing { current_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match ClaimsMigrator::<T>::migrate_many(current_key, &mut weight_counter) {
							Ok(current_key) => TransactionOutcome::Commit(Ok(current_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::ClaimsMigrationDone);
						},
						Ok(Some(current_key)) => {
							Self::transition(MigrationStage::ClaimsMigrationOngoing {
								current_key: Some(current_key),
							});
						},
						e => {
							defensive!("Error while migrating claims: {:?}", e);
						},
					}
				},
				MigrationStage::ClaimsMigrationDone => {
					Self::transition(MigrationStage::ProxyMigrationInit);
				},
				MigrationStage::ProxyMigrationInit => {
					Self::transition(MigrationStage::ProxyMigrationProxies { last_key: None });
				},
				MigrationStage::ProxyMigrationProxies { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match ProxyProxiesMigrator::<T>::migrate_many(last_key, &mut weight_counter)
						{
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::ProxyMigrationAnnouncements {
								last_key: None,
							});
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::ProxyMigrationProxies {
								last_key: Some(last_key),
							});
						},
						e => {
							defensive!("Error while migrating proxies: {:?}", e);
						},
					}
				},
				MigrationStage::ProxyMigrationAnnouncements { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match ProxyAnnouncementMigrator::<T>::migrate_many(
							last_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::ProxyMigrationDone);
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::ProxyMigrationAnnouncements {
								last_key: Some(last_key),
							});
						},
						e => {
							defensive!("Error while migrating proxy announcements: {:?}", e);
						},
					}
				},
				MigrationStage::ProxyMigrationDone => {
					Self::transition(MigrationStage::PreimageMigrationInit);
				},
				MigrationStage::PreimageMigrationInit => {
					Self::transition(MigrationStage::PreimageMigrationChunksOngoing {
						last_key: None,
					});
				},
				MigrationStage::PreimageMigrationChunksOngoing { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match PreimageChunkMigrator::<T>::migrate_many(
							last_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::PreimageMigrationChunksDone);
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::PreimageMigrationChunksOngoing {
								last_key: Some(last_key),
							});
						},
						e => {
							defensive!("Error while migrating preimages: {:?}", e);
						},
					}
				},
				MigrationStage::PreimageMigrationChunksDone => {
					Self::transition(MigrationStage::PreimageMigrationRequestStatusOngoing {
						next_key: None,
					});
				},
				MigrationStage::PreimageMigrationRequestStatusOngoing { next_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match PreimageRequestStatusMigrator::<T>::migrate_many(
							next_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::PreimageMigrationRequestStatusDone);
						},
						Ok(Some(next_key)) => {
							Self::transition(
								MigrationStage::PreimageMigrationRequestStatusOngoing {
									next_key: Some(next_key),
								},
							);
						},
						e => {
							defensive!("Error while migrating preimage request status: {:?}", e);
						},
					}
				},
				MigrationStage::PreimageMigrationRequestStatusDone => {
					Self::transition(MigrationStage::PreimageMigrationLegacyRequestStatusInit);
				},
				MigrationStage::PreimageMigrationLegacyRequestStatusInit => {
					Self::transition(MigrationStage::PreimageMigrationLegacyRequestStatusOngoing {
						next_key: None,
					});
				},
				MigrationStage::PreimageMigrationLegacyRequestStatusOngoing { next_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match PreimageLegacyRequestStatusMigrator::<T>::migrate_many(
							next_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(
								MigrationStage::PreimageMigrationLegacyRequestStatusDone,
							);
						},
						Ok(Some(next_key)) => {
							Self::transition(
								MigrationStage::PreimageMigrationLegacyRequestStatusOngoing {
									next_key: Some(next_key),
								},
							);
						},
						e => {
							defensive!(
								"Error while migrating legacy preimage request status: {:?}",
								e
							);
						},
					}
				},
				MigrationStage::PreimageMigrationLegacyRequestStatusDone => {
					Self::transition(MigrationStage::PreimageMigrationDone);
				},
				MigrationStage::PreimageMigrationDone => {
					Self::transition(MigrationStage::NomPoolsMigrationInit);
				},
				MigrationStage::NomPoolsMigrationInit => {
					Self::transition(MigrationStage::NomPoolsMigrationOngoing { next_key: None });
				},
				MigrationStage::NomPoolsMigrationOngoing { next_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match NomPoolsMigrator::<T>::migrate_many(next_key, &mut weight_counter) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::NomPoolsMigrationDone);
						},
						Ok(Some(next_key)) => {
							Self::transition(MigrationStage::NomPoolsMigrationOngoing {
								next_key: Some(next_key),
							});
						},
						e => {
							defensive!("Error while migrating nom pools: {:?}", e);
						},
					}
				},
				MigrationStage::NomPoolsMigrationDone => {
					Self::transition(MigrationStage::VestingMigrationInit);
				},

				MigrationStage::VestingMigrationInit => {
					Self::transition(MigrationStage::VestingMigrationOngoing { next_key: None });
				},
				MigrationStage::VestingMigrationOngoing { next_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match VestingMigrator::<T>::migrate_many(next_key, &mut weight_counter) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::VestingMigrationDone);
						},
						Ok(Some(next_key)) => {
							Self::transition(MigrationStage::VestingMigrationOngoing {
								next_key: Some(next_key),
							});
						},
						e => {
							defensive!("Error while migrating vesting: {:?}", e);
						},
					}
				},
				MigrationStage::VestingMigrationDone => {
					Self::transition(MigrationStage::FastUnstakeMigrationInit);
				},
				MigrationStage::FastUnstakeMigrationInit => {
					Self::transition(MigrationStage::FastUnstakeMigrationOngoing {
						next_key: None,
					});
				},
				MigrationStage::FastUnstakeMigrationOngoing { next_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match FastUnstakeMigrator::<T>::migrate_many(next_key, &mut weight_counter)
						{
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::FastUnstakeMigrationDone);
						},
						Ok(Some(next_key)) => {
							Self::transition(MigrationStage::FastUnstakeMigrationOngoing {
								next_key: Some(next_key),
							});
						},
						e => {
							defensive!("Error while migrating fast unstake: {:?}", e);
						},
					}
				},
				MigrationStage::FastUnstakeMigrationDone => {
					Self::transition(MigrationStage::IndicesMigrationInit);
				},
				MigrationStage::IndicesMigrationInit => {
					Self::transition(MigrationStage::IndicesMigrationOngoing {
						next_key: Some(Default::default()),
					});
				},
				MigrationStage::IndicesMigrationOngoing { next_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match IndicesMigrator::<T>::migrate_many(next_key, &mut weight_counter) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::IndicesMigrationDone);
						},
						Ok(Some(next_key)) => {
							Self::transition(MigrationStage::IndicesMigrationOngoing {
								next_key: Some(next_key),
							});
						},
						e => {
							defensive!("Error while migrating indices: {:?}", e);
						},
					}
				},
				MigrationStage::IndicesMigrationDone => {
					Self::transition(MigrationStage::ReferendaMigrationInit);
				},
				MigrationStage::ReferendaMigrationInit => {
					Self::transition(MigrationStage::ReferendaMigrationOngoing {
						last_key: Some(Default::default()),
					});
				},
				MigrationStage::ReferendaMigrationOngoing { last_key } => {
					let res =
						with_transaction_opaque_err::<Option<ReferendaStage>, Error<T>, _>(|| {
							match referenda::ReferendaMigrator::<T>::migrate_many(
								last_key,
								&mut weight_counter,
							) {
								Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
								Err(e) => TransactionOutcome::Rollback(Err(e)),
							}
						})
						.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::ReferendaMigrationDone);
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::ReferendaMigrationOngoing {
								last_key: Some(last_key),
							});
						},
						Err(err) => {
							defensive!("Error while migrating referenda: {:?}", err);
						},
					}
				},
				MigrationStage::ReferendaMigrationDone => {
					Self::transition(MigrationStage::BagsListMigrationInit);
				},
				MigrationStage::BagsListMigrationInit => {
					Self::transition(MigrationStage::BagsListMigrationOngoing { next_key: None });
				},
				MigrationStage::BagsListMigrationOngoing { next_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match BagsListMigrator::<T>::migrate_many(next_key, &mut weight_counter) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::BagsListMigrationDone);
						},
						Ok(Some(next_key)) => {
							Self::transition(MigrationStage::BagsListMigrationOngoing {
								next_key: Some(next_key),
							});
						},
						e => {
							defensive!("Error while migrating bags list: {:?}", e);
						},
					}
				},
				MigrationStage::BagsListMigrationDone => {
					Self::transition(MigrationStage::SchedulerMigrationInit);
				},
				MigrationStage::SchedulerMigrationInit => {
					Self::transition(MigrationStage::SchedulerMigrationOngoing { last_key: None });
				},
				MigrationStage::SchedulerMigrationOngoing { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match scheduler::SchedulerMigrator::<T>::migrate_many(
							last_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::SchedulerAgendaMigrationOngoing { last_key: None });
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::SchedulerMigrationOngoing {
								last_key: Some(last_key),
							});
						},
						Err(err) => {
							defensive!("Error while migrating scheduler: {:?}", err);
						},
					}
				},
				MigrationStage::SchedulerAgendaMigrationOngoing { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match scheduler::SchedulerAgendaMigrator::<T>::migrate_many(
							last_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::SchedulerMigrationDone);
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::SchedulerAgendaMigrationOngoing {
								last_key: Some(last_key),
							});
						},
						Err(err) => {
							defensive!("Error while migrating scheduler: {:?}", err);
						},
					}
				},
				MigrationStage::SchedulerMigrationDone => {
					Self::transition(MigrationStage::ConvictionVotingMigrationInit);
				},
				MigrationStage::ConvictionVotingMigrationInit => {
					Self::transition(MigrationStage::ConvictionVotingMigrationOngoing {
						last_key: None,
					});
				},
				MigrationStage::ConvictionVotingMigrationOngoing { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match conviction_voting::ConvictionVotingMigrator::<T>::migrate_many(
							last_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::ConvictionVotingMigrationDone);
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::ConvictionVotingMigrationOngoing {
								last_key: Some(last_key),
							});
						},
						Err(err) => {
							defensive!("Error while migrating conviction voting: {:?}", err);
						},
					}
				},
				MigrationStage::ConvictionVotingMigrationDone => {
					#[cfg(feature = "ahm-westend")]
					Self::transition(MigrationStage::BountiesMigrationDone);
					#[cfg(not(feature = "ahm-westend"))]
					Self::transition(MigrationStage::BountiesMigrationInit);
				},
				#[cfg(not(feature = "ahm-westend"))]
				MigrationStage::BountiesMigrationInit => {
					Self::transition(MigrationStage::BountiesMigrationOngoing { last_key: None });
				},
				#[cfg(not(feature = "ahm-westend"))]
				MigrationStage::BountiesMigrationOngoing { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match bounties::BountiesMigrator::<T>::migrate_many(
							last_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::BountiesMigrationDone);
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::BountiesMigrationOngoing {
								last_key: Some(last_key),
							});
						},
						e => {
							defensive!("Error while migrating bounties: {:?}", e);
						},
					}
				},
				MigrationStage::BountiesMigrationDone => {
					Self::transition(MigrationStage::AssetRateMigrationInit);
				},
				MigrationStage::AssetRateMigrationInit => {
					Self::transition(MigrationStage::AssetRateMigrationOngoing { last_key: None });
				},
				MigrationStage::AssetRateMigrationOngoing { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match asset_rate::AssetRateMigrator::<T>::migrate_many(
							last_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::AssetRateMigrationDone);
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::AssetRateMigrationOngoing {
								last_key: Some(last_key),
							});
						},
						Err(err) => {
							defensive!("Error while migrating asset rates: {:?}", err);
						},
					}
				},
				MigrationStage::AssetRateMigrationDone => {
					#[cfg(not(feature = "ahm-westend"))]
					Self::transition(MigrationStage::CrowdloanMigrationInit);
					#[cfg(feature = "ahm-westend")]
					Self::transition(MigrationStage::CrowdloanMigrationDone);
				},
				#[cfg(not(feature = "ahm-westend"))]
				MigrationStage::CrowdloanMigrationInit => {
					Self::transition(MigrationStage::CrowdloanMigrationOngoing { last_key: None });
				},
				#[cfg(not(feature = "ahm-westend"))]
				MigrationStage::CrowdloanMigrationOngoing { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match crowdloan::CrowdloanMigrator::<T>::migrate_many(
							last_key,
							&mut weight_counter,
						) {
						Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::CrowdloanMigrationDone);
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::CrowdloanMigrationOngoing {
								last_key: Some(last_key),
							});
						},
						e => {
							defensive!("Error while migrating crowdloan: {:?}", e);
						},
					}
				},
				MigrationStage::CrowdloanMigrationDone => {
					#[cfg(not(feature = "ahm-westend"))]
					Self::transition(MigrationStage::TreasuryMigrationInit);
					#[cfg(feature = "ahm-westend")]
					Self::transition(MigrationStage::TreasuryMigrationDone);
				},
				#[cfg(not(feature = "ahm-westend"))]
				MigrationStage::TreasuryMigrationInit => {
					Self::transition(MigrationStage::TreasuryMigrationOngoing { last_key: None });
				},
				#[cfg(not(feature = "ahm-westend"))]
				MigrationStage::TreasuryMigrationOngoing { last_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match treasury::TreasuryMigrator::<T>::migrate_many(
							last_key,
							&mut weight_counter,
						) {
							Ok(last_key) => TransactionOutcome::Commit(Ok(last_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");	

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::TreasuryMigrationDone);
						},
						Ok(Some(last_key)) => {
							Self::transition(MigrationStage::TreasuryMigrationOngoing {
								last_key: Some(last_key),
							});
						},
						e => {
							defensive!("Error while migrating treasury: {:?}", e);
						},
					}
				},
				MigrationStage::TreasuryMigrationDone => {
					#[cfg(feature = "ahm-staking-migration")]
					Self::transition(MigrationStage::StakingMigrationInit);
					#[cfg(not(feature = "ahm-staking-migration"))]
					Self::transition(MigrationStage::StakingMigrationDone);
				},
				#[cfg(feature = "ahm-staking-migration")]
				MigrationStage::StakingMigrationInit => {
					Self::transition(MigrationStage::StakingMigrationOngoing { next_key: None });
				},
				#[cfg(feature = "ahm-staking-migration")]
				MigrationStage::StakingMigrationOngoing { next_key } => {
					let res = with_transaction_opaque_err::<Option<_>, Error<T>, _>(|| {
						match staking::StakingMigrator::<T>::migrate_many(
							next_key,
							&mut weight_counter,
						) {
							Ok(next_key) => TransactionOutcome::Commit(Ok(next_key)),
							Err(e) => TransactionOutcome::Rollback(Err(e)),
						}
					})
					.expect("Always returning Ok; qed");

					match res {
						Ok(None) => {
							Self::transition(MigrationStage::StakingMigrationDone);
						},
						Ok(Some(next_key)) => {
							Self::transition(MigrationStage::StakingMigrationOngoing { next_key: Some(next_key) });
						},
						e => {
							defensive!("Error while migrating staking: {:?}", e);
						},
					}
				},
				MigrationStage::StakingMigrationDone => {
					Self::transition(MigrationStage::SignalMigrationFinish);
				},
				MigrationStage::SignalMigrationFinish => {
					// TODO: weight
					let tracker = RcMigratedBalance::<T>::get();
					let data = MigrationFinishedData {
						rc_balance_kept: tracker.kept,
					};
					let call = types::AhMigratorCall::<T>::FinishMigration { data };
					match Self::send_xcm(call, T::AhWeightInfo::finish_migration()) {
						Ok(_) => {
							Self::transition(MigrationStage::MigrationDone);
						},
						Err(_) => {
							defensive!(
								"Failed to send FinishMigration message to AH, \
								retry with the next block"
							);
						},
					}
					Self::transition(MigrationStage::MigrationDone);
				},
				MigrationStage::MigrationDone => (),
			};

			weight_counter.consumed()
		}
	}

	impl<T: Config> Pallet<T> {
		/// Returns `true` if the migration is ongoing and the Asset Hub has not confirmed
		/// processing the same number of XCM messages as we have sent to it.
		fn has_excess_unconfirmed_dmp(current: &MigrationStageOf<T>) -> bool {
			if !current.is_ongoing() {
				return false;
			}
			let (sent, processed) = DmpDataMessageCounts::<T>::get();
			if sent > processed {
				log::info!(
					target: LOG_TARGET,
					"Excess unconfirmed XCM messages: sent = {}, processed = {}",
					sent,
					processed
				);
				// TODO: make it possible to reset the counts with an extrinsic.
				return true;
			}
			false
		}

		/// Increases the number of XCM messages sent to the Asset Hub.
		fn increase_msg_sent_count(count: u32) {
			let (sent, processed) = DmpDataMessageCounts::<T>::get();
			let new_sent = sent + count;
			DmpDataMessageCounts::<T>::put((new_sent, processed));
			log::debug!(
				target: LOG_TARGET,
				"Increased XCM message sent count by {}; sent: {}, processed: {}",
				count,
				new_sent,
				processed
			);
		}

		/// Updates the number of XCM messages processed by the Asset Hub.
		fn update_msg_processed_count(new_processed: u32) {
			let (sent, processed) = DmpDataMessageCounts::<T>::get();
			if processed > new_processed {
				defensive!(
					"Processed XCM message count is less than the new processed count: {}",
					(processed, new_processed),
				);
				return;
			}
			DmpDataMessageCounts::<T>::put((sent, new_processed));
			log::info!(
				target: LOG_TARGET,
				"Updated XCM message processed count to {}; sent: {}",
				new_processed,
				sent,
			);
		}

		/// Execute a stage transition and log it.
		fn transition(new: MigrationStageOf<T>) {
			let old = RcMigrationStage::<T>::get();
			RcMigrationStage::<T>::put(&new);
			log::info!(target: LOG_TARGET, "[Block {:?}] Stage transition: {:?} -> {:?}", frame_system::Pallet::<T>::block_number(), &old, &new);
			Self::deposit_event(Event::StageTransition { old, new });
		}

		/// Split up the items into chunks of `MAX_XCM_SIZE` and send them as separate XCM
		/// transacts.
		///
		/// ### Parameters:
		/// - items - data items to batch and send with the `create_call`
		/// - create_call - function to create the call from the items
		/// - weight_at_most - function to calculate the weight limit on AH for the call with `n`
		///   elements from `items`
		///
		/// Will modify storage in the error path.
		/// This is done to avoid exceeding the XCM message size limit.
		pub fn send_chunked_xcm<E: Encode>(
			mut items: Vec<E>,
			create_call: impl Fn(Vec<E>) -> types::AhMigratorCall<T>,
			weight_at_most: impl Fn(u32) -> Weight,
		) -> Result<u32, Error<T>> {
			log::info!(target: LOG_TARGET, "Batching {} items to send via XCM", items.len());
			defensive_assert!(items.len() > 0, "Sending XCM with empty items");
			items.reverse();
			let mut batch_count = 0;

			while !items.is_empty() {
				let mut remaining_size: u32 = MAX_XCM_SIZE;
				let mut batch = Vec::new();

				while !items.is_empty() {
					// Taking from the back as optimization is fine since we reversed
					let item = items.last().unwrap(); // FAIL-CI no unwrap
					let msg_size = item.encoded_size() as u32;
					if msg_size > remaining_size {
						break;
					}
					remaining_size -= msg_size;

					batch.push(items.pop().unwrap()); // FAIL-CI no unwrap
				}

				let batch_len = batch.len() as u32;
				log::info!(target: LOG_TARGET, "Sending XCM batch of {} items", batch_len);
				let call = types::AssetHubPalletConfig::<T>::AhmController(create_call(batch));

				let message = Xcm(vec![
					Instruction::UnpaidExecution {
						weight_limit: WeightLimit::Unlimited,
						check_origin: None,
					},
					Instruction::Transact {
						origin_kind: OriginKind::Superuser,
						// The `require_weight_at_most` parameter is used by the XCM executor to
						// verify if the available weight is sufficient to process this call. If
						// sufficient, the executor will execute the call and use the actual weight
						// from the dispatchable result to adjust the meter limit. The weight meter
						// limit on the Asset Hub is [Config::MaxAhWeight], which applies not only
						// to process the calls passed with XCM messages but also to some base work
						// required to process an XCM message.
						// Additionally the call will not be executed if `require_weight_at_most` is
						// lower than the actual weight of the call.
						// TODO: we can remove ths with XCMv5
						#[cfg(feature = "ahm-polkadot")]
						require_weight_at_most: weight_at_most(batch_len),
						#[cfg(feature = "ahm-westend")]
						fallback_max_weight: Some(weight_at_most(batch_len)),
						call: call.encode().into(),
					},
				]);

				if let Err(err) = send_xcm::<T::SendXcm>(
					Location::new(0, [Junction::Parachain(1000)]),
					message.clone(),
				) {
					log::error!(target: LOG_TARGET, "Error while sending XCM message: {:?}", err);
					return Err(Error::XcmError);
				} else {
					batch_count += 1;
				}
			}

			log::info!(target: LOG_TARGET, "Sent {} XCM batch/es", batch_count);
			Ok(batch_count)
		}

		/// Send a single XCM message.
		///
		/// ### Parameters:
		/// - call - the call to send
		/// - weight_at_most - the weight limit for the call on AH
		pub fn send_xcm(
			call: types::AhMigratorCall<T>,
			weight_at_most: Weight,
		) -> Result<(), Error<T>> {
			log::info!(target: LOG_TARGET, "Sending XCM message");

			let call = types::AssetHubPalletConfig::<T>::AhmController(call);

			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				Instruction::Transact {
					origin_kind: OriginKind::Superuser,
					// The `require_weight_at_most` parameter is used by the XCM executor to verify
					// if the available weight is sufficient to process this call. If sufficient,
					// the executor will execute the call and use the actual weight from the
					// dispatchable result to adjust the meter limit. The weight meter limit on the
					// Asset Hub is [Config::MaxAhWeight], which applies not only to process the
					// calls passed with XCM messages but also to some base work required to process
					// an XCM message.
					// Additionally the call will not be executed if `require_weight_at_most` is
					// lower than the actual weight of the call.
					// TODO: we can remove ths with XCMv5
					#[cfg(feature = "ahm-polkadot")]
					require_weight_at_most: weight_at_most,
					#[cfg(feature = "ahm-westend")]
					fallback_max_weight: Some(weight_at_most),
					call: call.encode().into(),
				},
			]);

			if let Err(err) = send_xcm::<T::SendXcm>(
				Location::new(0, [Junction::Parachain(1000)]),
				message.clone(),
			) {
				log::error!(target: LOG_TARGET, "Error while sending XCM message: {:?}", err);
				return Err(Error::XcmError);
			};

			Ok(())
		}

		/// Decorates the `send_chunked_xcm` function by calling the `increase_msg_sent_count`
		/// function with the number of XCM messages sent.
		///
		/// Check the `send_chunked_xcm` function for the documentation.
		pub fn send_chunked_xcm_and_track<E: Encode>(
			items: Vec<E>,
			create_call: impl Fn(Vec<E>) -> types::AhMigratorCall<T>,
			weight_at_most: impl Fn(u32) -> Weight,
		) -> Result<u32, Error<T>> {
			match Self::send_chunked_xcm(items, create_call, weight_at_most) {
				Ok(count) => {
					Self::increase_msg_sent_count(count);
					Ok(count)
				},
				Err(e) => Err(e),
			}
		}

		/// Decorates the `send_xcm` function by calling the `increase_msg_sent_count` function
		/// with the number of XCM messages sent.
		///
		/// Check the `send_xcm` function for the documentation.
		pub fn send_xcm_and_track(
			call: types::AhMigratorCall<T>,
			weight_at_most: Weight,
		) -> Result<u32, Error<T>> {
			match Self::send_xcm(call, weight_at_most) {
				Ok(_) => {
					Self::increase_msg_sent_count(1);
					Ok(1)
				},
				Err(e) => Err(e),
			}
		}

		pub fn teleport_tracking() -> Option<(T::AccountId, MintLocation)> {
			let stage = RcMigrationStage::<T>::get();
			if stage.is_finished() || stage.is_ongoing() {
				None
			} else {
				Some((T::CheckingAccount::get(), MintLocation::Local))
			}
		}
	}

	impl<T: Config> types::MigrationStatus for Pallet<T> {
		fn is_ongoing() -> bool {
			RcMigrationStage::<T>::get().is_ongoing()
		}
		fn is_finished() -> bool {
			RcMigrationStage::<T>::get().is_finished()
		}
	}
}

/// Returns the weight for a single item in a batch.
///
/// If the next item in the batch is the first one, it includes the base weight of the
/// `weight_of`, otherwise, it does not.
pub fn item_weight_of(weight_of: impl Fn(u32) -> Weight, batch_len: u32) -> Weight {
	if batch_len == 0 {
		weight_of(1)
	} else {
		weight_of(1).saturating_sub(weight_of(0))
	}
}

impl<T: Config> Contains<<T as frame_system::Config>::RuntimeCall> for Pallet<T> {
	fn contains(call: &<T as frame_system::Config>::RuntimeCall) -> bool {
		let stage = RcMigrationStage::<T>::get();

		// We have to return whether the call is allowed:
		const ALLOWED: bool = true;
		const FORBIDDEN: bool = false;

		// Once the migration is finished, forbid calls not in the `RcPostMigrationCalls` set.
		if stage.is_finished() && !T::RcPostMigrationCalls::contains(call) {
			return FORBIDDEN;
		}

		// If the migration is ongoing, forbid calls not in the `RcIntraMigrationCalls` set.
		if stage.is_ongoing() && !T::RcIntraMigrationCalls::contains(call) {
			return FORBIDDEN;
		}

		// Otherwise, allow the call.
		// This also implicitly allows _any_ call if the migration has not yet started.
		ALLOWED
	}
}

// To be used for `IsTeleport` filter. Disallows teleports during the migration.
impl<T: Config> ContainsPair<Asset, Location> for Pallet<T> {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		let stage = RcMigrationStage::<T>::get();
		log::trace!(target: "xcm::IsTeleport::contains", "migration stage: {:?}", stage);
		let result = if stage.is_ongoing() {
			// during migration, no teleports (in or out) allowed
			false
		} else {
			// before and after migration use normal filter
			TrustedTeleportersBeforeAndAfter::contains(asset, origin)
		};
		log::trace!(
			target: "xcm::IsTeleport::contains",
			"asset: {:?} origin {:?} result {:?}",
			asset, origin, result
		);
		result
	}
}
