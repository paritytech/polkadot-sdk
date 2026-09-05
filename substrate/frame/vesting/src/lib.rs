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

//! # Vesting Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ## Overview
//!
//! A simple pallet providing a means of placing a linear curve on an account's locked balance. This
//! pallet ensures that there is a lock in place preventing the balance to drop below the *unvested*
//! amount for any reason other than the ones specified in `UnvestedFundsAllowedWithdrawReasons`
//! configuration value.
//!
//! As the amount vested increases over time, the amount unvested reduces. However, locks remain in
//! place and explicit action is needed on behalf of the user to ensure that the amount locked is
//! equivalent to the amount remaining to be vested. This is done through a dispatchable function,
//! either `vest` (in typical case where the sender is calling on their own behalf) or `vest_other`
//! in case the sender is calling on another account's behalf.
//!
//! ## Interface
//!
//! This pallet implements the `VestingSchedule` trait.
//!
//! ### Dispatchable Functions
//!
//! - `vest` - Update the lock, reducing it in line with the amount "vested" so far.
//! - `vest_other` - Update the lock of another account, reducing it in line with the amount
//!   "vested" so far.

#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
mod vesting_info;

pub mod migrations;
pub mod weights;

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::{fmt::Debug, marker::PhantomData};
pub use frame_support::traits::tokens::{VestedPayoutError, VestingKind};
use frame_support::{
	dispatch::DispatchResult,
	ensure,
	storage::{bounded_vec::BoundedVec, with_storage_layer},
	traits::{
		tokens::VestedPayout, Currency, ExistenceRequirement, Get, LockIdentifier,
		LockableCurrency, VestedTransfer, VestingSchedule, WithdrawReasons,
	},
	weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AtLeast32BitUnsigned, BlockNumberProvider, Bounded, CheckedMul, Convert,
		MaybeSerializeDeserialize, One, Saturating, StaticLookup, Zero,
	},
	DispatchError,
};

pub use pallet::*;
pub use vesting_info::*;
pub use weights::WeightInfo;

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
type MaxLocksOf<T> =
	<<T as Config>::Currency as LockableCurrency<<T as frame_system::Config>::AccountId>>::MaxLocks;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

const VESTING_ID: LockIdentifier = *b"vesting ";

// A value placed in storage that represents the current version of the Vesting storage.
// This value is used by `on_runtime_upgrade` to determine whether we run storage migration logic.
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, Debug, MaxEncodedLen, TypeInfo)]
pub enum Releases {
	V0,
	V1,
	V2,
}

impl Default for Releases {
	fn default() -> Self {
		Releases::V0
	}
}

/// Actions to take against a user's `Vesting` storage entry.
#[derive(Clone, Copy)]
enum VestingAction {
	/// Do not actively remove any schedules.
	Passive,
	/// Remove the schedule specified by the index.
	Remove { index: usize },
	/// Remove the two schedules, specified by index, so they can be merged.
	Merge { index1: usize, index2: usize },
}

impl VestingAction {
	/// Whether or not the filter says the schedule index should be removed.
	fn should_remove(&self, index: usize) -> bool {
		match self {
			Self::Passive => false,
			Self::Remove { index: index1 } => *index1 == index,
			Self::Merge { index1, index2 } => *index1 == index || *index2 == index,
		}
	}

	/// Pick the schedules that this action dictates should continue vesting undisturbed.
	fn pick_schedules<T: Config>(
		&self,
		schedules: Vec<(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind)>,
	) -> impl Iterator<Item = (VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind)> + '_ {
		schedules.into_iter().enumerate().filter_map(move |(index, entry)| {
			if self.should_remove(index) {
				None
			} else {
				Some(entry)
			}
		})
	}
}

// Wrapper for `T::MAX_VESTING_SCHEDULES` to satisfy `trait Get`.
pub struct MaxVestingSchedulesGet<T>(PhantomData<T>);
impl<T: Config> Get<u32> for MaxVestingSchedulesGet<T> {
	fn get() -> u32 {
		T::MAX_VESTING_SCHEDULES
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency trait.
		type Currency: LockableCurrency<Self::AccountId>;

		/// Convert the block number into a balance.
		type BlockNumberToBalance: Convert<BlockNumberFor<Self>, BalanceOf<Self>>;

		/// The minimum amount transferred to call `vested_transfer`.
		#[pallet::constant]
		type MinVestedTransfer: Get<BalanceOf<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// Reasons that determine under which conditions the balance may drop below
		/// the unvested amount.
		type UnvestedFundsAllowedWithdrawReasons: Get<WithdrawReasons>;

		/// Query the current block number.
		///
		/// Must return monotonically increasing values when called from consecutive blocks.
		/// Can be configured to return either:
		/// - the local block number of the runtime via `frame_system::Pallet`
		/// - a remote block number, eg from the relay chain through `RelaychainDataProvider`
		/// - an arbitrary value through a custom implementation of the trait
		///
		/// There is currently no migration provided to "hot-swap" block number providers and it may
		/// result in undefined behavior when doing so. Parachains are therefore best off setting
		/// this to their local block number provider if they have the pallet already deployed.
		///
		/// Suggested values:
		/// - Solo- and Relay-chains: `frame_system::Pallet`
		/// - Parachains that may produce blocks sparingly or only when needed (on-demand):
		///   - already have the pallet deployed: `frame_system::Pallet`
		///   - are freshly deploying this pallet: `RelaychainDataProvider`
		/// - Parachains with a reliably block production rate (PLO or bulk-coretime):
		///   - already have the pallet deployed: `frame_system::Pallet`
		///   - are freshly deploying this pallet: no strong recommendation. Both local and remote
		///     providers can be used. Relay provider can be a bit better in cases where the
		///     parachain is lagging its block production to avoid clock skew.
		type BlockNumberProvider: BlockNumberProvider<BlockNumber = BlockNumberFor<Self>>;

		/// Maximum number of vesting schedules an account may have at a given moment.
		///
		/// This is the total storage cap. Trusted programmatic callers (via the [`VestedPayout`]
		/// and [`VestedTransfer`] traits, and the root-only `force_vested_transfer` extrinsic)
		/// may fill schedules up to this limit.
		const MAX_VESTING_SCHEDULES: u32;

		/// Maximum number of vesting schedules an account may hold from the permissionless
		/// `vested_transfer` extrinsic. Must not exceed `MAX_VESTING_SCHEDULES`.
		/// Defaults to half of `MAX_VESTING_SCHEDULES`, reserving the other half for
		/// trusted system callers. If `MAX_VESTING_SCHEDULES` is 1, there will be no public
		/// schedule capacity, the single slot being reserved for system schedules.
		const MAX_PUBLIC_VESTING_SCHEDULES: u32 = Self::MAX_VESTING_SCHEDULES / 2;

		/// Returns the slot cap for a given [`VestingKind`].
		fn slot_cap(kind: VestingKind) -> u32 {
			match kind {
				VestingKind::Public => Self::MAX_PUBLIC_VESTING_SCHEDULES,
				VestingKind::System => {
					Self::MAX_VESTING_SCHEDULES.saturating_sub(Self::MAX_PUBLIC_VESTING_SCHEDULES)
				},
			}
		}
	}

	#[pallet::extra_constants]
	impl<T: Config> Pallet<T> {
		#[pallet::constant_name(MaxVestingSchedules)]
		fn max_vesting_schedules() -> u32 {
			T::MAX_VESTING_SCHEDULES
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(T::MAX_VESTING_SCHEDULES > 0, "`MaxVestingSchedules` must be greater than 0");

			assert!(
				T::MAX_PUBLIC_VESTING_SCHEDULES <= T::MAX_VESTING_SCHEDULES,
				"MAX_PUBLIC_VESTING_SCHEDULES must not exceed MAX_VESTING_SCHEDULES"
			);
		}
	}

	/// Information regarding the vesting of a given account.
	///
	/// Each entry is a `(VestingInfo, VestingKind)` tuple where `Public` indicates
	/// a schedule from the permissionless `vested_transfer` extrinsic and `System`
	/// indicates a schedule from a trusted caller (staking payouts, root/force calls).
	#[pallet::storage]
	pub type Vesting<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		BoundedVec<
			(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind),
			MaxVestingSchedulesGet<T>,
		>,
	>;

	/// Storage version of the pallet.
	///
	/// New networks start with latest version, as determined by the genesis build.
	#[pallet::storage]
	pub type StorageVersion<T: Config> = StorageValue<_, Releases, ValueQuery>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub vesting: Vec<(T::AccountId, BlockNumberFor<T>, BlockNumberFor<T>, BalanceOf<T>)>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			use sp_runtime::traits::Saturating;

			// Genesis uses the latest storage version.
			StorageVersion::<T>::put(Releases::V2);

			// Generate initial vesting configuration
			// * who - Account which we are generating vesting configuration for
			// * begin - Block when the account will start to vest
			// * length - Number of blocks from `begin` until fully vested
			// * liquid - Number of units which can be spent before vesting begins
			for &(ref who, begin, length, liquid) in self.vesting.iter() {
				let balance = T::Currency::free_balance(who);
				assert!(!balance.is_zero(), "Currencies must be init'd before vesting");
				// Total genesis `balance` minus `liquid` equals funds locked for vesting
				let locked = balance.saturating_sub(liquid);
				let length_as_balance = T::BlockNumberToBalance::convert(length);
				let per_block = locked / length_as_balance.max(sp_runtime::traits::One::one());
				let vesting_info = VestingInfo::new(locked, per_block, begin);
				if !vesting_info.is_valid() {
					panic!("Invalid VestingInfo params at genesis")
				};

				// Tag genesis schedules as Public — they originate from the initial chain config.
				Vesting::<T>::try_append(who, (vesting_info, VestingKind::Public))
					.expect("Too many vesting schedules at genesis.");
			}

			// Lock once per account, after every schedule is stored. `set_lock` replaces a
			// lock of the same ID, so locking inside the loop above would leave an account
			// with several entries holding only the last entry's amount.
			let reasons = WithdrawReasons::except(T::UnvestedFundsAllowedWithdrawReasons::get());
			for (who, schedules) in Vesting::<T>::iter() {
				let locked = schedules
					.iter()
					.map(|s| s.locked())
					.fold(BalanceOf::<T>::zero(), |a, b| a.saturating_add(b));
				T::Currency::set_lock(VESTING_ID, &who, locked, reasons);
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A vesting schedule has been created.
		VestingCreated { account: T::AccountId, schedule_index: u32 },
		/// The amount vested has been updated. This could indicate a change in funds available.
		/// The balance given is the amount which is left unvested (and thus locked).
		VestingUpdated { account: T::AccountId, unvested: BalanceOf<T> },
		/// An \[account\] has become fully vested.
		VestingCompleted { account: T::AccountId },
	}

	/// Error for the vesting pallet.
	#[pallet::error]
	pub enum Error<T> {
		/// The account given is not vesting.
		NotVesting,
		/// The account already has `MaxVestingSchedules` count of schedules and thus
		/// cannot add another one. Consider merging existing schedules in order to add another.
		AtMaxVestingSchedules,
		/// Amount being transferred is too low to create a vesting schedule.
		AmountLow,
		/// An index was out of bounds of the vesting schedules.
		ScheduleIndexOutOfBounds,
		/// Failed to create a new schedule because some parameter was invalid.
		InvalidScheduleParams,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Unlock any vested funds of the sender account.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must have funds still
		/// locked under this pallet.
		///
		/// Emits either `VestingCompleted` or `VestingUpdated`.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::vest_locked(MaxLocksOf::<T>::get(), T::MAX_VESTING_SCHEDULES)
			.max(T::WeightInfo::vest_unlocked(MaxLocksOf::<T>::get(), T::MAX_VESTING_SCHEDULES))
		)]
		pub fn vest(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_vest(who)
		}

		/// Unlock any vested funds of a `target` account.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// - `target`: The account whose vested funds should be unlocked. Must have funds still
		/// locked under this pallet.
		///
		/// Emits either `VestingCompleted` or `VestingUpdated`.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::vest_other_locked(MaxLocksOf::<T>::get(), T::MAX_VESTING_SCHEDULES)
			.max(T::WeightInfo::vest_other_unlocked(MaxLocksOf::<T>::get(), T::MAX_VESTING_SCHEDULES))
		)]
		pub fn vest_other(origin: OriginFor<T>, target: AccountIdLookupOf<T>) -> DispatchResult {
			ensure_signed(origin)?;
			let who = T::Lookup::lookup(target)?;
			Self::do_vest(who)
		}

		/// Create a vested transfer.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// - `target`: The account receiving the vested funds.
		/// - `schedule`: The vesting schedule attached to the transfer.
		///
		/// Emits `VestingCreated`.
		///
		/// NOTE: This will unlock all schedules through the current block.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(2)]
		#[pallet::weight(
			T::WeightInfo::vested_transfer(MaxLocksOf::<T>::get(), T::slot_cap(VestingKind::Public))
		)]
		pub fn vested_transfer(
			origin: OriginFor<T>,
			target: AccountIdLookupOf<T>,
			schedule: VestingInfo<BalanceOf<T>, BlockNumberFor<T>>,
		) -> DispatchResult {
			let transactor = ensure_signed(origin)?;
			let target = T::Lookup::lookup(target)?;
			Self::do_vested_transfer(&transactor, &target, schedule, VestingKind::Public)
		}

		/// Force a vested transfer.
		///
		/// The dispatch origin for this call must be _Root_.
		///
		/// - `source`: The account whose funds should be transferred.
		/// - `target`: The account that should be transferred the vested funds.
		/// - `schedule`: The vesting schedule attached to the transfer.
		///
		/// Emits `VestingCreated`.
		///
		/// NOTE: This will unlock all schedules through the current block.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(3)]
		#[pallet::weight(
			T::WeightInfo::force_vested_transfer(MaxLocksOf::<T>::get(), T::MAX_VESTING_SCHEDULES)
		)]
		pub fn force_vested_transfer(
			origin: OriginFor<T>,
			source: AccountIdLookupOf<T>,
			target: AccountIdLookupOf<T>,
			schedule: VestingInfo<BalanceOf<T>, BlockNumberFor<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;
			let target = T::Lookup::lookup(target)?;
			let source = T::Lookup::lookup(source)?;
			Self::do_vested_transfer(&source, &target, schedule, VestingKind::System)
		}

		/// Merge two vesting schedules together, creating a new vesting schedule that unlocks over
		/// the highest possible start and end blocks. If both schedules have already started the
		/// current block will be used as the schedule start; with the caveat that if one schedule
		/// is finished by the current block, the other will be treated as the new merged schedule,
		/// unmodified.
		///
		/// NOTE: If `schedule1_index == schedule2_index` this is a no-op.
		/// NOTE: This will unlock all schedules through the current block prior to merging.
		/// NOTE: If both schedules have ended by the current block, no new schedule will be created
		/// and both will be removed.
		///
		/// Merged schedule attributes:
		/// - `starting_block`: `MAX(schedule1.starting_block, scheduled2.starting_block,
		///   current_block)`.
		/// - `ending_block`: `MAX(schedule1.ending_block, schedule2.ending_block)`.
		/// - `locked`: `schedule1.locked_at(current_block) + schedule2.locked_at(current_block)`.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// - `schedule1_index`: index of the first schedule to merge.
		/// - `schedule2_index`: index of the second schedule to merge.
		#[pallet::call_index(4)]
		#[pallet::weight(
			T::WeightInfo::not_unlocking_merge_schedules(MaxLocksOf::<T>::get(), T::MAX_VESTING_SCHEDULES)
			.max(T::WeightInfo::unlocking_merge_schedules(MaxLocksOf::<T>::get(), T::MAX_VESTING_SCHEDULES))
		)]
		pub fn merge_schedules(
			origin: OriginFor<T>,
			schedule1_index: u32,
			schedule2_index: u32,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			if schedule1_index == schedule2_index {
				return Ok(());
			};
			let schedule1_index = schedule1_index as usize;
			let schedule2_index = schedule2_index as usize;

			let schedules = Vesting::<T>::get(&who).ok_or(Error::<T>::NotVesting)?;
			let merge_action =
				VestingAction::Merge { index1: schedule1_index, index2: schedule2_index };

			let (schedules, locked_now) = Self::exec_action(schedules.into_inner(), merge_action)?;

			Self::write_vesting(&who, schedules)?;
			Self::write_lock(&who, locked_now);

			Ok(())
		}

		/// Force remove a vesting schedule
		///
		/// The dispatch origin for this call must be _Root_.
		///
		/// - `target`: An account that has a vesting schedule
		/// - `schedule_index`: The vesting schedule index that should be removed
		#[pallet::call_index(5)]
		#[pallet::weight(
			T::WeightInfo::force_remove_vesting_schedule(MaxLocksOf::<T>::get(), T::MAX_VESTING_SCHEDULES)
		)]
		pub fn force_remove_vesting_schedule(
			origin: OriginFor<T>,
			target: <T::Lookup as StaticLookup>::Source,
			schedule_index: u32,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let who = T::Lookup::lookup(target)?;

			let schedules_count = Vesting::<T>::decode_len(&who).unwrap_or_default();
			ensure!(schedule_index < schedules_count as u32, Error::<T>::InvalidScheduleParams);

			Self::remove_vesting_schedule(&who, schedule_index)?;

			Ok(Some(T::WeightInfo::force_remove_vesting_schedule(
				MaxLocksOf::<T>::get(),
				schedules_count as u32,
			))
			.into())
		}
	}
}

impl<T: Config> Pallet<T> {
	// Public function for accessing vesting storage
	pub fn vesting(
		account: T::AccountId,
	) -> Option<
		BoundedVec<
			(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind),
			MaxVestingSchedulesGet<T>,
		>,
	> {
		Vesting::<T>::get(account)
	}

	// Create a new `VestingInfo`, based off of two other `VestingInfo`s.
	// NOTE: We assume both schedules have had funds unlocked up through the current block.
	fn merge_vesting_info(
		now: BlockNumberFor<T>,
		schedule1: VestingInfo<BalanceOf<T>, BlockNumberFor<T>>,
		schedule2: VestingInfo<BalanceOf<T>, BlockNumberFor<T>>,
	) -> Option<VestingInfo<BalanceOf<T>, BlockNumberFor<T>>> {
		let schedule1_ending_block = schedule1.ending_block_as_balance::<T::BlockNumberToBalance>();
		let schedule2_ending_block = schedule2.ending_block_as_balance::<T::BlockNumberToBalance>();
		let now_as_balance = T::BlockNumberToBalance::convert(now);

		// Check if one or both schedules have ended.
		match (schedule1_ending_block <= now_as_balance, schedule2_ending_block <= now_as_balance) {
			// If both schedules have ended, we don't merge and exit early.
			(true, true) => return None,
			// If one schedule has ended, we treat the one that has not ended as the new
			// merged schedule.
			(true, false) => return Some(schedule2),
			(false, true) => return Some(schedule1),
			// If neither schedule has ended don't exit early.
			_ => {},
		}

		let locked = schedule1
			.locked_at::<T::BlockNumberToBalance>(now)
			.saturating_add(schedule2.locked_at::<T::BlockNumberToBalance>(now));
		// This shouldn't happen because we know at least one ending block is greater than now,
		// thus at least a schedule a some locked balance.
		debug_assert!(
			!locked.is_zero(),
			"merge_vesting_info validation checks failed to catch a locked of 0"
		);

		let ending_block = schedule1_ending_block.max(schedule2_ending_block);
		let starting_block = now.max(schedule1.starting_block()).max(schedule2.starting_block());

		let per_block = {
			let duration = ending_block
				.saturating_sub(T::BlockNumberToBalance::convert(starting_block))
				.max(One::one());
			(locked / duration).max(One::one())
		};

		let schedule = VestingInfo::new(locked, per_block, starting_block);
		debug_assert!(schedule.is_valid(), "merge_vesting_info schedule validation check failed");

		Some(schedule)
	}

	// Merge `incoming` into `existing`, anchoring the result on `existing.starting_block()`.
	// The two schedules may have different starting blocks; `existing`'s start is always
	// preserved as the output's starting block.
	//
	// The merged schedule ends at `max(existing_end, incoming_end)` so neither payout is
	// compressed into a shorter window than intended.
	//
	// `incoming.locked()` (the full original amount) is used, not `locked_at(now)`, so that
	// the complete payout is locked regardless of how far into the epoch it arrives.
	//
	// `per_block` is sized to unlock the combined locked-at-now amount over the remaining
	// window. `locked` is back-calculated so that `locked_at(now) == target_locked_now` exactly.
	fn merge_vesting_info_preserving_start(
		now: BlockNumberFor<T>,
		existing: VestingInfo<BalanceOf<T>, BlockNumberFor<T>>,
		incoming: VestingInfo<BalanceOf<T>, BlockNumberFor<T>>,
	) -> VestingInfo<BalanceOf<T>, BlockNumberFor<T>> {
		// Lock the full incoming amount, irrespective of when the existing schedule started.
		let target_locked_now = existing
			.locked_at::<T::BlockNumberToBalance>(now)
			.saturating_add(incoming.locked());

		// The merged schedule should end at the later of the two ending blocks.
		let ending_block = existing
			.ending_block_as_balance::<T::BlockNumberToBalance>()
			.max(incoming.ending_block_as_balance::<T::BlockNumberToBalance>());

		let now_as_balance = T::BlockNumberToBalance::convert(now);
		let elapsed = now_as_balance
			.saturating_sub(T::BlockNumberToBalance::convert(existing.starting_block()));
		let remaining = ending_block.saturating_sub(now_as_balance).max(One::one());

		// Ceiling division: per_block = ceil(target_locked_now / remaining).
		let per_block = (target_locked_now.saturating_add(remaining).saturating_sub(One::one()) /
			remaining)
			.max(One::one());

		// Back-calculate "locked" so that "locked_at(now)" exactly matches "target_locked_now".
		// `per_block * elapsed` can overflow Balance when an ancient target is selected (large
		// `elapsed`). `VestingInfo::locked_at` uses `checked_mul` with a zero fallback on
		// overflow, so the stored `locked` would diverge and the schedule would appear
		// fully-vested. Fall back to anchoring at `now` (`elapsed = 0`) to keep the invariant.
		let (locked, starting_block) = if let Some(notional) = per_block.checked_mul(&elapsed) {
			(target_locked_now.saturating_add(notional), existing.starting_block())
		} else {
			(target_locked_now, now)
		};

		VestingInfo::new(locked, per_block, starting_block)
	}

	// Validates a schedule before creating it via a transfer.
	// `MinVestedTransfer` is enforced for all kinds except `System`, which is trusted.
	// Any future kind must explicitly be added to the exception list to bypass this guard.
	fn validate_new_transfer_schedule(
		schedule: &VestingInfo<BalanceOf<T>, BlockNumberFor<T>>,
		kind: VestingKind,
	) -> DispatchResult {
		if !matches!(kind, VestingKind::System) {
			ensure!(schedule.locked() >= T::MinVestedTransfer::get(), Error::<T>::AmountLow);
		}
		ensure!(schedule.is_valid(), Error::<T>::InvalidScheduleParams);
		Ok(())
	}

	// Execute a vested transfer from `source` to `target` with the given `schedule`.
	fn do_vested_transfer(
		source: &T::AccountId,
		target: &T::AccountId,
		schedule: VestingInfo<BalanceOf<T>, BlockNumberFor<T>>,
		kind: VestingKind,
	) -> DispatchResult {
		Self::validate_new_transfer_schedule(&schedule, kind)?;

		// The currency transfer and vesting schedule setup must run atomically.
		with_storage_layer(|| {
			T::Currency::transfer(
				source,
				target,
				schedule.locked(),
				ExistenceRequirement::AllowDeath,
			)?;

			Self::add_vesting_schedule_with_kind(
				target,
				schedule.locked(),
				schedule.per_block(),
				schedule.starting_block(),
				kind,
			)
		})
	}

	// Internal kind-aware schedule insertion with cap enforcement.
	fn add_vesting_schedule_with_kind(
		who: &T::AccountId,
		locked: BalanceOf<T>,
		per_block: BalanceOf<T>,
		starting_block: BlockNumberFor<T>,
		kind: VestingKind,
	) -> DispatchResult {
		if locked.is_zero() {
			return Ok(());
		}

		let vesting_schedule = VestingInfo::new(locked, per_block, starting_block);
		if !vesting_schedule.is_valid() {
			return Err(Error::<T>::InvalidScheduleParams.into());
		};

		let mut schedules = Vesting::<T>::get(who).unwrap_or_default();

		// Single point of cap enforcement for every insert path.
		ensure!(
			schedules.iter().filter(|(_, k)| *k == kind).count() < T::slot_cap(kind) as usize,
			Error::<T>::AtMaxVestingSchedules,
		);

		ensure!(
			schedules.try_push((vesting_schedule, kind)).is_ok(),
			Error::<T>::AtMaxVestingSchedules
		);

		debug_assert!(schedules.len() > 0, "schedules cannot be empty after insertion");
		let schedule_index = schedules.len() - 1;
		Self::deposit_event(Event::<T>::VestingCreated {
			account: who.clone(),
			schedule_index: schedule_index as u32,
		});

		let (schedules, locked_now) =
			Self::exec_action(schedules.into_inner(), VestingAction::Passive)?;

		Self::write_vesting(who, schedules)?;
		Self::write_lock(who, locked_now);

		Ok(())
	}

	/// Iterate through the schedules to track the current locked amount and
	/// filter out completed and specified schedules.
	///
	/// Returns a tuple that consists of:
	/// - Vec of vesting schedules, where completed schedules and those specified
	/// 	by filter are removed. (Note the vec is not checked for respecting
	/// 	bounded length.)
	/// - The amount locked at the current block number based on the given schedules.
	///
	/// NOTE: the amount locked does not include any schedules that are filtered out via `action`.
	fn report_schedule_updates(
		schedules: Vec<(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind)>,
		action: VestingAction,
	) -> (Vec<(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind)>, BalanceOf<T>) {
		let now = T::BlockNumberProvider::current_block_number();

		let mut total_locked_now: BalanceOf<T> = Zero::zero();
		let filtered_schedules = action
			.pick_schedules::<T>(schedules)
			.filter(|(schedule, _kind)| {
				let locked_now = schedule.locked_at::<T::BlockNumberToBalance>(now);
				let keep = !locked_now.is_zero();
				if keep {
					total_locked_now = total_locked_now.saturating_add(locked_now);
				}
				keep
			})
			.collect::<Vec<_>>();

		(filtered_schedules, total_locked_now)
	}

	/// Write an accounts updated vesting lock to storage.
	fn write_lock(who: &T::AccountId, total_locked_now: BalanceOf<T>) {
		if total_locked_now.is_zero() {
			T::Currency::remove_lock(VESTING_ID, who);
			Self::deposit_event(Event::<T>::VestingCompleted { account: who.clone() });
		} else {
			let reasons = WithdrawReasons::except(T::UnvestedFundsAllowedWithdrawReasons::get());
			T::Currency::set_lock(VESTING_ID, who, total_locked_now, reasons);
			Self::deposit_event(Event::<T>::VestingUpdated {
				account: who.clone(),
				unvested: total_locked_now,
			});
		};
	}

	/// Write an accounts updated vesting schedules to storage.
	fn write_vesting(
		who: &T::AccountId,
		schedules: Vec<(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind)>,
	) -> Result<(), DispatchError> {
		let schedules: BoundedVec<
			(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind),
			MaxVestingSchedulesGet<T>,
		> = schedules.try_into().map_err(|_| Error::<T>::AtMaxVestingSchedules)?;

		if schedules.len() == 0 {
			Vesting::<T>::remove(&who);
		} else {
			Vesting::<T>::insert(who, schedules)
		}

		Ok(())
	}

	/// Unlock any vested funds of `who`.
	fn do_vest(who: T::AccountId) -> DispatchResult {
		let schedules = Vesting::<T>::get(&who).ok_or(Error::<T>::NotVesting)?;

		let (schedules, locked_now) =
			Self::exec_action(schedules.into_inner(), VestingAction::Passive)?;

		Self::write_vesting(&who, schedules)?;
		Self::write_lock(&who, locked_now);

		Ok(())
	}

	/// Execute a `VestingAction` against the given `schedules`. Returns the updated schedules
	/// and locked amount.
	fn exec_action(
		schedules: Vec<(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind)>,
		action: VestingAction,
	) -> Result<
		(Vec<(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind)>, BalanceOf<T>),
		DispatchError,
	> {
		let (schedules, locked_now) = match action {
			VestingAction::Merge { index1: idx1, index2: idx2 } => {
				// The schedule index is based off of the schedule ordering prior to filtering out
				// any schedules that may be ending at this block.
				let (schedule1, kind1) =
					schedules.get(idx1).copied().ok_or(Error::<T>::ScheduleIndexOutOfBounds)?;
				let (schedule2, _kind2) =
					schedules.get(idx2).copied().ok_or(Error::<T>::ScheduleIndexOutOfBounds)?;

				// The length of `schedules` decreases by 2 here since we filter out 2 schedules.
				// Thus we know below that we can push the new merged schedule without error
				// (assuming initial state was valid).
				let (mut schedules, mut locked_now) =
					Self::report_schedule_updates(schedules, action);

				let now = T::BlockNumberProvider::current_block_number();
				if let Some(new_schedule) = Self::merge_vesting_info(now, schedule1, schedule2) {
					// Merging created a new schedule so we:
					// 1) need to add it to the accounts vesting schedule collection,
					// keeping the kind of the first schedule (arbitrary but consistent).
					schedules.push((new_schedule, kind1));
					// (we use `locked_at` in case this is a schedule that started in the past)
					let new_schedule_locked =
						new_schedule.locked_at::<T::BlockNumberToBalance>(now);
					// and 2) update the locked amount to reflect the schedule we just added.
					locked_now = locked_now.saturating_add(new_schedule_locked);
				} // In the None case there was no new schedule to account for.

				(schedules, locked_now)
			},
			_ => Self::report_schedule_updates(schedules, action),
		};

		debug_assert!(
			locked_now > Zero::zero() && schedules.len() > 0 ||
				locked_now == Zero::zero() && schedules.len() == 0
		);

		Ok((schedules, locked_now))
	}

	fn has_capacity_for_kind_inner(
		schedules: &[(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind)],
		kind: VestingKind,
	) -> bool {
		schedules.iter().filter(|(_, k)| *k == kind).count() < T::slot_cap(kind) as usize
	}

	fn has_capacity_for_kind(who: &T::AccountId, kind: VestingKind) -> bool {
		let schedules = Vesting::<T>::get(who).unwrap_or_default();
		Self::has_capacity_for_kind_inner(&schedules, kind)
	}
}

impl<T: Config> VestedPayout<T::AccountId, BalanceOf<T>> for Pallet<T>
where
	BalanceOf<T>: MaybeSerializeDeserialize + Debug,
{
	type BlockNumber = BlockNumberFor<T>;

	fn vested_transfer(
		source: &T::AccountId,
		dest: &T::AccountId,
		amount: BalanceOf<T>,
		duration: BlockNumberFor<T>,
		start_at: Option<BlockNumberFor<T>>,
	) -> DispatchResult {
		if amount.is_zero() {
			return Ok(());
		}

		if duration.is_zero() {
			// Zero duration means liquid transfer with no vesting schedule.
			T::Currency::transfer(source, dest, amount, ExistenceRequirement::AllowDeath)
		} else {
			let starting_block =
				start_at.unwrap_or_else(|| T::BlockNumberProvider::current_block_number());
			let duration_as_balance = T::BlockNumberToBalance::convert(duration);
			// Round up so that vesting completes within `duration` blocks, not longer.
			let per_block =
				((amount.saturating_add(duration_as_balance).saturating_sub(One::one())) /
					duration_as_balance)
					.max(One::one());
			let schedule = VestingInfo::new(amount, per_block, starting_block);

			Self::do_vested_transfer(source, dest, schedule, VestingKind::Public)
		}
	}

	fn add_to_vesting(
		source: &T::AccountId,
		dest: &T::AccountId,
		amount: BalanceOf<T>,
		duration: BlockNumberFor<T>,
		start_at: BlockNumberFor<T>,
		kind: VestingKind,
	) -> Result<(), VestedPayoutError> {
		if amount.is_zero() || duration.is_zero() {
			return Ok(());
		}

		let now = T::BlockNumberProvider::current_block_number();
		let duration_as_balance = T::BlockNumberToBalance::convert(duration);
		let per_block = (amount.saturating_add(duration_as_balance).saturating_sub(One::one()) /
			duration_as_balance)
			.max(One::one());
		let incoming = VestingInfo::new(amount, per_block, start_at);
		let schedules = Vesting::<T>::get(dest).unwrap_or_default();

		if let Some(idx) = schedules
			.iter()
			.position(|(vi, k)| vi.starting_block() == start_at && *k == kind)
		{
			// Merging bypasses MinVestedTransfer and all operations are performed atomically.
			with_storage_layer(|| {
				T::Currency::transfer(source, dest, amount, ExistenceRequirement::AllowDeath)?;
				let mut schedules = schedules.into_inner();
				let merged_vi =
					Self::merge_vesting_info_preserving_start(now, schedules[idx].0, incoming);
				schedules[idx] = (merged_vi, kind);
				let (schedules, locked_now) = Self::exec_action(schedules, VestingAction::Passive)?;
				Self::write_vesting(dest, schedules)?;
				Self::write_lock(dest, locked_now);
				Ok(())
			})
			.map_err(VestedPayoutError::Other)
		} else {
			if !Self::has_capacity_for_kind_inner(&schedules, kind) {
				return Err(VestedPayoutError::NoCapacity);
			}
			Self::do_vested_transfer(source, dest, incoming, kind).map_err(|e| {
				if e == Error::<T>::AtMaxVestingSchedules.into() {
					VestedPayoutError::NoCapacity
				} else {
					VestedPayoutError::Other(e)
				}
			})
		}
	}

	fn merge_amount_into_closest_schedule(
		source: &T::AccountId,
		dest: &T::AccountId,
		amount: BalanceOf<T>,
		duration: BlockNumberFor<T>,
		start_at: BlockNumberFor<T>,
		kind: VestingKind,
	) -> DispatchResult {
		if amount.is_zero() || duration.is_zero() {
			return Ok(());
		}
		let duration_as_balance = T::BlockNumberToBalance::convert(duration);
		let incoming_per_block =
			(amount.saturating_add(duration_as_balance).saturating_sub(One::one()) /
				duration_as_balance)
				.max(One::one());
		let incoming = VestingInfo::new(amount, incoming_per_block, start_at);

		let now = T::BlockNumberProvider::current_block_number();
		let incoming_end_block = incoming.ending_block_as_balance::<T::BlockNumberToBalance>();

		with_storage_layer(|| {
			let schedules = Vesting::<T>::get(dest).unwrap_or_default();

			// Find the schedule of `kind` with ending_block closest to incoming.ending_block.
			let (idx, target_vi) = schedules
				.iter()
				.enumerate()
				.filter(|(_, (_, k))| *k == kind)
				.min_by_key(|(_, (vi, _))| {
					let end_block = vi.ending_block_as_balance::<T::BlockNumberToBalance>();

					// Absolute value: | end_block - incoming_end_block |
					end_block.max(incoming_end_block) - end_block.min(incoming_end_block)
				})
				.map(|(i, (vi, _))| (i, *vi))
				.ok_or(Error::<T>::NotVesting)?;

			// Merge using the same algorithm as the normal same-era path: target's starting_block
			// is the anchor, and the window extends to max(target_end, incoming_end).
			let merged_vi = Self::merge_vesting_info_preserving_start(now, target_vi, incoming);

			T::Currency::transfer(
				source,
				dest,
				incoming.locked(),
				ExistenceRequirement::AllowDeath,
			)?;

			let mut scheds = schedules.into_inner();
			scheds[idx] = (merged_vi, kind);
			let (scheds, locked_now) = Self::exec_action(scheds, VestingAction::Passive)?;
			Self::write_vesting(dest, scheds)?;
			Self::write_lock(dest, locked_now);
			Ok(())
		})
	}
}

impl<T: Config> VestingSchedule<T::AccountId> for Pallet<T>
where
	BalanceOf<T>: MaybeSerializeDeserialize + Debug,
{
	type Currency = T::Currency;
	type Moment = BlockNumberFor<T>;

	/// Get the amount that is currently being vested and cannot be transferred out of this account.
	fn vesting_balance(who: &T::AccountId) -> Option<BalanceOf<T>> {
		if let Some(v) = Vesting::<T>::get(who) {
			let now = T::BlockNumberProvider::current_block_number();
			let total_locked_now = v.iter().fold(Zero::zero(), |total, (schedule, _kind)| {
				schedule.locked_at::<T::BlockNumberToBalance>(now).saturating_add(total)
			});
			Some(T::Currency::free_balance(who).min(total_locked_now))
		} else {
			None
		}
	}

	/// Adds a vesting schedule to a given account, tagged as [`VestingKind::Public`].
	///
	/// If the account has `MaxVestingSchedules`, an Error is returned and nothing
	/// is updated.
	///
	/// On success, a linearly reducing amount of funds will be locked. In order to realise any
	/// reduction of the lock over time as it diminishes, the account owner must use `vest` or
	/// `vest_other`.
	///
	/// It is a no-op if the amount to be vested is zero.
	///
	/// NOTE: This doesn't alter the free balance of the account.
	fn add_vesting_schedule(
		who: &T::AccountId,
		locked: BalanceOf<T>,
		per_block: BalanceOf<T>,
		starting_block: BlockNumberFor<T>,
	) -> DispatchResult {
		// External callers don't carry kind info; treat as Public (the natural default).
		Self::add_vesting_schedule_with_kind(
			who,
			locked,
			per_block,
			starting_block,
			VestingKind::Public,
		)
	}

	/// Ensure we can call `add_vesting_schedule` without error. This should always
	/// be called prior to `add_vesting_schedule`.
	fn can_add_vesting_schedule(
		who: &T::AccountId,
		locked: BalanceOf<T>,
		per_block: BalanceOf<T>,
		starting_block: BlockNumberFor<T>,
	) -> DispatchResult {
		// Check for `per_block` or `locked` of 0.
		if !VestingInfo::new(locked, per_block, starting_block).is_valid() {
			return Err(Error::<T>::InvalidScheduleParams.into());
		}

		// `add_vesting_schedule` tags Public, so the predicate must apply the Public quota
		// to honour the can-add → add contract.
		ensure!(
			Self::has_capacity_for_kind(who, VestingKind::Public),
			Error::<T>::AtMaxVestingSchedules,
		);

		Ok(())
	}

	/// Remove a vesting schedule for a given account.
	fn remove_vesting_schedule(who: &T::AccountId, schedule_index: u32) -> DispatchResult {
		let schedules = Vesting::<T>::get(who).ok_or(Error::<T>::NotVesting)?;
		let remove_action = VestingAction::Remove { index: schedule_index as usize };

		let (schedules, locked_now) = Self::exec_action(schedules.into_inner(), remove_action)?;

		Self::write_vesting(who, schedules)?;
		Self::write_lock(who, locked_now);
		Ok(())
	}
}

/// An implementation that allows the Vesting Pallet to handle a vested transfer
/// on behalf of another Pallet.
impl<T: Config> VestedTransfer<T::AccountId> for Pallet<T>
where
	BalanceOf<T>: MaybeSerializeDeserialize + Debug,
{
	type Currency = T::Currency;
	type Moment = BlockNumberFor<T>;

	fn vested_transfer(
		source: &T::AccountId,
		target: &T::AccountId,
		locked: BalanceOf<T>,
		per_block: BalanceOf<T>,
		starting_block: BlockNumberFor<T>,
	) -> DispatchResult {
		use frame_support::storage::{with_transaction, TransactionOutcome};
		let schedule = VestingInfo::new(locked, per_block, starting_block);
		with_transaction(|| -> TransactionOutcome<DispatchResult> {
			// VestedTransfer callers are trusted (they hold Currency); treat as Public.
			let result = Self::do_vested_transfer(source, target, schedule, VestingKind::Public);

			match &result {
				Ok(()) => TransactionOutcome::Commit(result),
				_ => TransactionOutcome::Rollback(result),
			}
		})
	}
}
