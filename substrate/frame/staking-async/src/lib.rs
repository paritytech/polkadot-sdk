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

//! # Staking Async Pallet
//!
//! This pallet is a fork of the original `pallet-staking`, with a number of key differences:
//!
//! * It no longer has access to a secure timestamp, previously used to calculate the duration of an
//!   era.
//! * It no longer has access to a pallet-session.
//! * It no longer has access to a pallet-authorship.
//! * It is capable of working with a multi-page `ElectionProvider``, aka.
//!   `pallet-election-provider-multi-block`.
//!
//! While `pallet-staking` was somewhat general-purpose, this pallet is absolutely NOT right from
//! the get-go: It is designed to be used ONLY in Polkadot/Kusama AssetHub system parachains.
//!
//! The workings of this pallet can be divided into a number of subsystems, as follows.
//!
//! ## User Interactions
//!
//! TODO
//!
//! ## Session and Era Rotation
//!
//! TODO
//!
//! ## Exposure Collection
//!
//! TODO
//!
//! ## Slashing Pipeline and Withdrawal Restrictions
//!
//! This pallet implements a robust slashing mechanism that ensures the integrity of the staking
//! system while preventing stakers from withdrawing funds that might still be subject to slashing.
//!
//! ### Overview of the Slashing Pipeline
//!
//! The slashing process consists of multiple phases:
//!
//! 1. **Offence Reporting**: Offences are reported from the relay chain through `on_new_offences`
//! 2. **Queuing**: Valid offences are added to the `OffenceQueue` for processing
//! 3. **Processing**: Offences are processed incrementally over multiple blocks
//! 4. **Application**: Slashes are either applied immediately or deferred based on configuration
//!
//! ### Phase 1: Offence Reporting
//!
//! Offences are reported from the relay chain (e.g., from BABE, GRANDPA, BEEFY, or parachain
//! modules) through the `on_new_offences` function:
//!
//! ```text
//! struct Offence {
//!     offender: AccountId,        // The validator being slashed
//!     reporters: Vec<AccountId>,  // Who reported the offence (may be empty)
//!     slash_fraction: Perbill,    // Percentage of stake to slash
//! }
//! ```
//!
//! **Reporting Deadlines**:
//! - With deferred slashing: Offences must be reported within `SlashDeferDuration - 1` eras
//! - With immediate slashing: Offences can be reported up to `BondingDuration` eras old
//!
//! Example: If `SlashDeferDuration = 27` and current era is 100:
//! - Oldest reportable offence: Era 74 (100 - 26)
//! - Offences from era 73 or earlier are rejected
//!
//! ### Phase 2: Queuing
//!
//! When an offence passes validation, it's added to the queue:
//!
//! 1. **Storage**: Added to `OffenceQueue`: `(EraIndex, AccountId) -> OffenceRecord`
//! 2. **Era Tracking**: Era added to `OffenceQueueEras` (sorted vector of eras with offences)
//! 3. **Duplicate Handling**: If an offence already exists for the same validator in the same era,
//!    only the higher slash fraction is kept
//!
//! ### Phase 3: Processing
//!
//! Offences are processed incrementally in `on_initialize` each block:
//!
//! ```text
//! 1. Load oldest offence from queue
//! 2. Move to `ProcessingOffence` storage
//! 3. For each exposure page (from last to first):
//!    - Calculate slash for validator's own stake
//!    - Calculate slash for each nominator (pro-rata based on exposure)
//!    - Track total slash and reward amounts
//! 4. Once all pages processed, create `UnappliedSlash`
//! ```
//!
//! **Key Features**:
//! - **Page-by-page processing**: Large validator sets don't overwhelm a single block
//! - **Pro-rata slashing**: Nominators slashed proportionally to their stake
//! - **Reward calculation**: A portion goes to reporters (if any)
//!
//! ### Phase 4: Application
//!
//! Based on `SlashDeferDuration`, slashes are either:
//!
//! **Immediate (SlashDeferDuration = 0)**:
//! - Applied right away in the same block
//! - Funds deducted from staking ledger immediately
//!
//! **Deferred (SlashDeferDuration > 0)**:
//! - Stored in `UnappliedSlashes` for future application
//! - Applied at era: `offence_era + SlashDeferDuration`
//! - Can be cancelled by governance before application
//!
//! ### Storage Items Involved
//!
//! - `OffenceQueue`: Pending offences to process
//! - `OffenceQueueEras`: Sorted list of eras with offences
//! - `ProcessingOffence`: Currently processing offence
//! - `ValidatorSlashInEra`: Tracks highest slash per validator per era
//! - `UnappliedSlashes`: Deferred slashes waiting for application
//!
//! ### Withdrawal Restrictions
//!
//! To maintain slashing guarantees, withdrawals are restricted:
//!
//! **Withdrawal Era Calculation**:
//! ```text
//! earliest_era_to_withdraw = min(
//!     active_era,
//!     last_fully_processed_offence_era + BondingDuration
//! )
//! ```
//!
//! **Example**:
//! - Active era: 100
//! - Oldest unprocessed offence: Era 70
//! - BondingDuration: 28
//! - Withdrawal allowed only for chunks with era ≤ 97 (70 - 1 + 28)
//!
//! **Withdrawal Timeline Example with an Offence**:
//! ```text
//! Era:        90    91    92    93    94    95    96    97    98    99    100   ...  117   118
//!             |     |     |     |     |     |     |     |     |     |     |          |     |
//! Unbond:     U                                                                             
//! Offence:    X                                                                             
//! Reported:               R                                                                 
//! Processed:              P (within next few blocks)                                        
//! Slash Applied:                                                                       S     
//! Withdraw:                                                                            ❌    ✓
//!
//! With BondingDuration = 28 and SlashDeferDuration = 27:
//! - User unbonds in era 90
//! - Offence occurs in era 90
//! - Reported in era 92 (typically within 2 days, but reportable until Era 116)
//! - Processed in era 92 (within next few blocks after reporting)
//! - Slash deferred for 27 eras, applied at era 117 (90 + 27)
//! - Cannot withdraw unbonded chunks until era 118 (90 + 28)
//!
//! The 28-era bonding duration ensures that any offences committed before or during
//! unbonding have time to be reported, processed, and applied before funds can be
//! withdrawn. This provides a window for governance to cancel slashes that may have
//! resulted from software bugs.
//! ```
//!
//! **Key Restrictions**:
//! 1. Cannot withdraw if previous era has unapplied slashes
//! 2. Cannot withdraw funds from eras with unprocessed offences

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(any(feature = "runtime-benchmarks", test))]
pub mod testing_utils;

#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;

pub mod asset;
pub mod election_size_tracker;
pub mod ledger;
mod pallet;
pub mod session_rotation;
pub mod slashing;
pub mod weights;

extern crate alloc;
use alloc::{vec, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode, HasCompact, MaxEncodedLen};
use frame_election_provider_support::ElectionProvider;
use frame_support::{
	traits::{
		tokens::fungible::{Credit, Debt},
		ConstU32, Contains, Get, LockIdentifier,
	},
	BoundedVec, DebugNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound,
	WeakBoundedVec,
};
use frame_system::pallet_prelude::BlockNumberFor;
use ledger::LedgerIntegrityState;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, One, StaticLookup, UniqueSaturatedInto},
	BoundedBTreeMap, Perbill, RuntimeDebug, Saturating,
};
use sp_staking::{EraIndex, ExposurePage, PagedExposureMetadata, SessionIndex};
pub use sp_staking::{Exposure, IndividualExposure, StakerStatus};
pub use weights::WeightInfo;

// public exports
pub use ledger::{StakingLedger, UnlockChunk};
pub use pallet::{pallet::*, UseNominatorsAndValidatorsMap, UseValidatorsMap};

pub(crate) const STAKING_ID: LockIdentifier = *b"staking ";
pub(crate) const LOG_TARGET: &str = "runtime::staking-async";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] 💸 ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

/// Alias for a bounded set of exposures behind a validator, parameterized by this pallet's
/// election provider.
pub type BoundedExposuresOf<T> = BoundedVec<
	(
		<T as frame_system::Config>::AccountId,
		Exposure<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
	),
	MaxWinnersPerPageOf<<T as Config>::ElectionProvider>,
>;

/// Alias for the maximum number of winners (aka. active validators), as defined in by this pallet's
/// config.
pub type MaxWinnersOf<T> = <T as Config>::MaxValidatorSet;

/// Alias for the maximum number of winners per page, as expected by the election provider.
pub type MaxWinnersPerPageOf<P> = <P as ElectionProvider>::MaxWinnersPerPage;

/// Maximum number of nominations per nominator.
pub type MaxNominationsOf<T> =
	<<T as Config>::NominationsQuota as NominationsQuota<BalanceOf<T>>>::MaxNominations;

/// Counter for the number of "reward" points earned by a given validator.
pub type RewardPoint = u32;

/// The balance type of this pallet.
pub type BalanceOf<T> = <T as Config>::CurrencyBalance;

type PositiveImbalanceOf<T> = Debt<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;
pub type NegativeImbalanceOf<T> =
	Credit<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

/// Information regarding the active era (era in used in session).
#[derive(Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen, PartialEq, Eq, Clone)]
pub struct ActiveEraInfo {
	/// Index of era.
	pub index: EraIndex,
	/// Moment of start expressed as millisecond from `$UNIX_EPOCH`.
	///
	/// Start can be none if start hasn't been set for the era yet,
	/// Start is set on the first on_finalize of the era to guarantee usage of `Time`.
	pub start: Option<u64>,
}

/// Reward points of an era. Used to split era total payout between validators.
///
/// This points will be used to reward validators and their respective nominators.
#[derive(
	PartialEqNoBound, Encode, Decode, DebugNoBound, TypeInfo, MaxEncodedLen, DefaultNoBound,
)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(T))]
pub struct EraRewardPoints<T: Config> {
	/// Total number of points. Equals the sum of reward points for each validator.
	pub total: RewardPoint,
	/// The reward points earned by a given validator.
	pub individual: BoundedBTreeMap<T::AccountId, RewardPoint, T::MaxValidatorSet>,
}

/// A destination account for payment.
#[derive(
	PartialEq,
	Eq,
	Copy,
	Clone,
	Encode,
	Decode,
	DecodeWithMemTracking,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum RewardDestination<AccountId> {
	/// Pay into the stash account, increasing the amount at stake accordingly.
	Staked,
	/// Pay into the stash account, not increasing the amount at stake.
	Stash,
	#[deprecated(
		note = "`Controller` will be removed after January 2024. Use `Account(controller)` instead."
	)]
	Controller,
	/// Pay into a specified account.
	Account(AccountId),
	/// Receive no reward.
	None,
}

/// Preference of what happens regarding validation.
#[derive(
	PartialEq,
	Eq,
	Clone,
	Encode,
	Decode,
	DecodeWithMemTracking,
	RuntimeDebug,
	TypeInfo,
	Default,
	MaxEncodedLen,
)]
pub struct ValidatorPrefs {
	/// Reward that validator takes up-front; only the rest is split between themselves and
	/// nominators.
	#[codec(compact)]
	pub commission: Perbill,
	/// Whether or not this validator is accepting more nominations. If `true`, then no nominator
	/// who is not already nominating this validator may nominate them. By default, validators
	/// are accepting nominations.
	pub blocked: bool,
}

/// Status of a paged snapshot progress.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen, Default)]
pub enum SnapshotStatus<AccountId> {
	/// Paged snapshot is in progress, the `AccountId` was the last staker iterated in the list.
	Ongoing(AccountId),
	/// All the stakers in the system have been consumed since the snapshot started.
	Consumed,
	/// Waiting for a new snapshot to be requested.
	#[default]
	Waiting,
}

/// A record of the nominations made by a specific account.
#[derive(
	PartialEqNoBound, EqNoBound, Clone, Encode, Decode, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen,
)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(T))]
pub struct Nominations<T: Config> {
	/// The targets of nomination.
	pub targets: BoundedVec<T::AccountId, MaxNominationsOf<T>>,
	/// The era the nominations were submitted.
	///
	/// Except for initial nominations which are considered submitted at era 0.
	pub submitted_in: EraIndex,
	/// Whether the nominations have been suppressed. This can happen due to slashing of the
	/// validators, or other events that might invalidate the nomination.
	///
	/// NOTE: this for future proofing and is thus far not used.
	pub suppressed: bool,
}

/// Facade struct to encapsulate `PagedExposureMetadata` and a single page of `ExposurePage`.
///
/// This is useful where we need to take into account the validator's own stake and total exposure
/// in consideration, in addition to the individual nominators backing them.
#[derive(Encode, Decode, RuntimeDebug, TypeInfo, PartialEq, Eq)]
pub struct PagedExposure<AccountId, Balance: HasCompact + codec::MaxEncodedLen> {
	exposure_metadata: PagedExposureMetadata<Balance>,
	exposure_page: ExposurePage<AccountId, Balance>,
}

impl<AccountId, Balance: HasCompact + Copy + AtLeast32BitUnsigned + codec::MaxEncodedLen>
	PagedExposure<AccountId, Balance>
{
	/// Create a new instance of `PagedExposure` from legacy clipped exposures.
	pub fn from_clipped(exposure: Exposure<AccountId, Balance>) -> Self {
		Self {
			exposure_metadata: PagedExposureMetadata {
				total: exposure.total,
				own: exposure.own,
				nominator_count: exposure.others.len() as u32,
				page_count: 1,
			},
			exposure_page: ExposurePage { page_total: exposure.total, others: exposure.others },
		}
	}

	/// Returns total exposure of this validator across pages
	pub fn total(&self) -> Balance {
		self.exposure_metadata.total
	}

	/// Returns total exposure of this validator for the current page
	pub fn page_total(&self) -> Balance {
		self.exposure_page.page_total + self.exposure_metadata.own
	}

	/// Returns validator's own stake that is exposed
	pub fn own(&self) -> Balance {
		self.exposure_metadata.own
	}

	/// Returns the portions of nominators stashes that are exposed in this page.
	pub fn others(&self) -> &Vec<IndividualExposure<AccountId, Balance>> {
		&self.exposure_page.others
	}
}

/// A pending slash record. The value of the slash has been computed but not applied yet,
/// rather deferred for several eras.
#[derive(
	Encode, Decode, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen, PartialEqNoBound, EqNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct UnappliedSlash<T: Config> {
	/// The stash ID of the offending validator.
	pub validator: T::AccountId,
	/// The validator's own slash.
	pub own: BalanceOf<T>,
	/// All other slashed stakers and amounts.
	pub others: WeakBoundedVec<(T::AccountId, BalanceOf<T>), T::MaxExposurePageSize>,
	/// Reporters of the offence; bounty payout recipients.
	pub reporter: Option<T::AccountId>,
	/// The amount of payout.
	pub payout: BalanceOf<T>,
}

/// Something that defines the maximum number of nominations per nominator based on a curve.
///
/// The method `curve` implements the nomination quota curve and should not be used directly.
/// However, `get_quota` returns the bounded maximum number of nominations based on `fn curve` and
/// the nominator's balance.
pub trait NominationsQuota<Balance> {
	/// Strict maximum number of nominations that caps the nominations curve. This value can be
	/// used as the upper bound of the number of votes per nominator.
	type MaxNominations: Get<u32>;

	/// Returns the voter's nomination quota within reasonable bounds [`min`, `max`], where `min`
	/// is 1 and `max` is `Self::MaxNominations`.
	fn get_quota(balance: Balance) -> u32 {
		Self::curve(balance).clamp(1, Self::MaxNominations::get())
	}

	/// Returns the voter's nomination quota based on its balance and a curve.
	fn curve(balance: Balance) -> u32;
}

/// A nomination quota that allows up to MAX nominations for all validators.
pub struct FixedNominationsQuota<const MAX: u32>;
impl<Balance, const MAX: u32> NominationsQuota<Balance> for FixedNominationsQuota<MAX> {
	type MaxNominations = ConstU32<MAX>;

	fn curve(_: Balance) -> u32 {
		MAX
	}
}

/// Handler for determining how much of a balance should be paid out on the current era.
pub trait EraPayout<Balance> {
	/// Determine the payout for this era.
	///
	/// Returns the amount to be paid to stakers in this era, as well as whatever else should be
	/// paid out ("the rest").
	fn era_payout(
		total_staked: Balance,
		total_issuance: Balance,
		era_duration_millis: u64,
	) -> (Balance, Balance);
}

impl<Balance: Default> EraPayout<Balance> for () {
	fn era_payout(
		_total_staked: Balance,
		_total_issuance: Balance,
		_era_duration_millis: u64,
	) -> (Balance, Balance) {
		(Default::default(), Default::default())
	}
}

/// Mode of era-forcing.
#[derive(
	Copy,
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum Forcing {
	/// Not forcing anything - just let whatever happen.
	NotForcing,
	/// Force a new era, then reset to `NotForcing` as soon as it is done.
	/// Note that this will force to trigger an election until a new era is triggered, if the
	/// election failed, the next session end will trigger a new election again, until success.
	ForceNew,
	/// Avoid a new era indefinitely.
	ForceNone,
	/// Force a new era at the end of all sessions indefinitely.
	ForceAlways,
}

impl Default for Forcing {
	fn default() -> Self {
		Forcing::NotForcing
	}
}

/// A utility struct that provides a way to check if a given account is a staker.
///
/// This struct implements the `Contains` trait, allowing it to determine whether
/// a particular account is currently staking by checking if the account exists in
/// the staking ledger.
///
/// Intended to be used in [`crate::Config::Filter`].
pub struct AllStakers<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> Contains<T::AccountId> for AllStakers<T> {
	/// Checks if the given account ID corresponds to a staker.
	///
	/// # Returns
	/// - `true` if the account has an entry in the staking ledger (indicating it is staking).
	/// - `false` otherwise.
	fn contains(account: &T::AccountId) -> bool {
		Ledger::<T>::contains_key(account)
	}
}

/// A smart type to determine the [`Config::PlanningEraOffset`], given:
///
/// * Expected relay session duration, `RS`
/// * Time taking into consideration for XCM sending, `S`
///
/// It will use the estimated election duration, the relay session duration, and add one as it knows
/// the relay chain will want to buffer validators for one session. This is needed because we use
/// this in our calculation based on the "active era".
pub struct PlanningEraOffsetOf<T, RS, S>(core::marker::PhantomData<(T, RS, S)>);
impl<T: Config, RS: Get<BlockNumberFor<T>>, S: Get<BlockNumberFor<T>>> Get<SessionIndex>
	for PlanningEraOffsetOf<T, RS, S>
{
	fn get() -> SessionIndex {
		let election_duration = <T::ElectionProvider as ElectionProvider>::duration_with_export();
		let sessions_needed = (election_duration + S::get()) / RS::get();
		// add one, because we know the RC session pallet wants to buffer for one session, and
		// another one cause we will receive activation report one session after that.
		sessions_needed
			.saturating_add(One::one())
			.saturating_add(One::one())
			.unique_saturated_into()
	}
}
