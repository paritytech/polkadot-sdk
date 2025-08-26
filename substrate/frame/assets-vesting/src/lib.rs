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

//! # Assets Vesting Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ## Overview
//!
//! A simple pallet providing a means of placing a linear curve on an assets account's frozen
//! balance. This pallet ensures that there is a frozen amount in place preventing the balance to
//! drop below the *unvested* amount.
//!
//! As the vested amount increases over time, the unvested amount reduces. However, freezes remain
//! in place and an explicit action is needed on behalf of the user to ensure that the frozen
//! amount is equivalent to the amount remaining to be vested. This is done through a dispatchable
//! function, either `vest` (in typical case where the sender is calling on their own behalf) or
//! `vest_other` in case the sender is calling on another account's behalf.
//!
//! ## Interface
//!
//! This pallet implements the [`VestedInspect`], [`VestedMutate`] and [`VestedTransfer`] traits.
//!
//! ### Dispatchable Functions
//!
//! - `vest` - Update the lock, reducing it in line with the amount "vested" so far.
//! - `vest_other` - Update the lock of another account, reducing it in line with the amount
//!   "vested" so far.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

mod types;
mod weights;

use frame::{
	deps::{codec::Decode, sp_runtime::ModuleError},
	prelude::*,
	traits::{
		fungibles::{Inspect, Mutate, MutateFreeze, VestedInspect, VestedMutate, VestedTransfer},
		tokens::Preservation,
	},
};
use scale_info::prelude::vec::Vec;

pub use pallet::*;
pub use types::*;
pub use weights::*;

#[cfg(feature = "runtime-benchmarks")]
use frame::traits::fungibles::Create;

#[frame::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config<I: 'static = ()>:
		frame_system::Config<RuntimeEvent: From<Event<Self, I>>>
	where
		AssetIdOf<Self, I>: MaybeSerializeDeserialize,
		AssetFreezeReasonOf<Self, I>: From<FreezeReason<I>>,
	{
		/// An Origin that can control the `force` calls.
		type ForceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Type represents interactions between assets
		#[cfg(not(feature = "runtime-benchmarks"))]
		type Assets: Mutate<AccountIdOf<Self>>;

		/// Type represents interactions between assets
		#[cfg(feature = "runtime-benchmarks")]
		type Assets: Mutate<AccountIdOf<Self>> + Create<AccountIdOf<Self>>;

		/// Type allows handling fungibles' freezes.
		type Freezer: MutateFreeze<
			AccountIdOf<Self>,
			AssetId = AssetIdOf<Self, I>,
			Balance = BalanceOf<Self, I>,
		>;

		/// Convert the block number into a balance.
		type BlockNumberToBalance: Convert<BlockNumberFor<Self>, BalanceOf<Self, I>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The minimum amount transferred to call `vested_transfer`.
		#[pallet::constant]
		type MinVestedTransfer: Get<BalanceOf<Self, I>>;

		/// Provider for the block number.
		type BlockNumberProvider: BlockNumberProvider<BlockNumber = BlockNumberFor<Self>>;

		/// Maximum number of vesting schedules an account may have at a given moment.
		const MAX_VESTING_SCHEDULES: u32;

		/// The benchmarking helper
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper<Self, I>;
	}

	#[pallet::extra_constants]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		#[pallet::constant_name(MaxVestingSchedules)]
		fn max_vesting_schedules() -> u32 {
			T::MAX_VESTING_SCHEDULES
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	pub type GenesisVestingSchedule<T, I> =
		(AssetIdOf<T, I>, AccountIdOf<T>, BlockNumberFor<T>, BlockNumberFor<T>, BalanceOf<T, I>);

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub vesting: Vec<GenesisVestingSchedule<T, I>>,
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			// Generate initial vesting configuration
			// * asset - The id of the asset class the vesting is related to.
			// * who - Account which we are generating vesting configuration for
			// * begin - Block when the account will start to vest
			// * length - Number of blocks from `begin` until fully vested
			// * liquid - Number of units which can be spent before vesting begins
			for &(ref asset, ref who, begin, length, liquid) in self.vesting.iter() {
				let balance = T::Assets::total_balance(asset.clone(), who);
				assert!(!balance.is_zero(), "Assets must be init'd before vesting");

				// Total genesis `balance` minus `liquid` equals assets frozen for vesting
				let frozen = balance.saturating_sub(liquid);
				let length_as_balance = T::BlockNumberToBalance::convert(length);
				let per_block = (frozen / length_as_balance.max(One::one())).max(One::one());

				Pallet::<T, I>::add_vesting_schedule(asset.clone(), who, frozen, per_block, begin)
					.map_err(|err| {
						let DispatchError::Module(ModuleError { message: Some(message), .. }) =
							err.into()
						else {
							panic!("Failure to add vesting at genesis");
						};
						let msg = match message {
							"InvalidScheduleParams" => "Invalid VestingInfo params at genesis.",
							"AtMaxVestingSchedules" => "Too many vesting schedules at genesis.",
							msg => panic!("Failure to add vesting at genesis: {msg}"),
						};

						panic!("{msg}");
					})
					.unwrap();
			}
		}
	}

	/// Information regarding the vesting of a given account.
	#[pallet::storage]
	pub type Vesting<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		AssetIdOf<T, I>,
		Blake2_128Concat,
		T::AccountId,
		BoundedVec<VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>, MaxVestingSchedulesGet<T, I>>,
	>;

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn integrity_test() {
			assert!(T::MAX_VESTING_SCHEDULES > 0, "`MaxVestingSchedules` must ge greater than 0");
		}
	}

	/// A reason for the pallet assets vesting placing a freeze on funds.
	#[pallet::composite_enum]
	pub enum FreezeReason<I: 'static = ()> {
		// An account is vesting some funds.
		Vesting,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// The amount vested has been updated. This could indicate a change in funds available.
		/// The balance given is the amount which is left unvested (and thus frozen).
		VestingUpdated { asset: AssetIdOf<T, I>, account: T::AccountId, unvested: BalanceOf<T, I> },
		/// An \[asset account\] has become fully vested.
		VestingCompleted { asset: AssetIdOf<T, I>, account: T::AccountId },
	}

	/// Error for the vesting pallet.
	#[pallet::error]
	pub enum Error<T, I = ()> {
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
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Unlock any vested funds of the sender account.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must have funds still
		/// frozen under this pallet.
		///
		/// - `asset`: Id of the asset class of the asset account for which the vesting applies.
		///
		/// Emits either `VestingCompleted` or `VestingUpdated`.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::vest_locked(T::MAX_VESTING_SCHEDULES)
				.max(T::WeightInfo::vest_unlocked(T::MAX_VESTING_SCHEDULES))
		)]
		pub fn vest(origin: OriginFor<T>, asset: AssetIdOf<T, I>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_vest(asset, who)
		}

		/// Unlock any vested funds of a `target` account.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// - `asset`: Id of the asset class of the asset account for which the vesting applies.
		/// - `target`: The account whose vested funds should be unlocked. Must have funds still
		/// frozen under this pallet.
		///
		/// Emits either `VestingCompleted` or `VestingUpdated`.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::vest_other_locked(T::MAX_VESTING_SCHEDULES)
				.max(T::WeightInfo::vest_other_unlocked(T::MAX_VESTING_SCHEDULES))
			)]
		pub fn vest_other(
			origin: OriginFor<T>,
			asset: AssetIdOf<T, I>,
			target: AccountIdLookupOf<T>,
		) -> DispatchResult {
			ensure_signed(origin)?;
			let who = T::Lookup::lookup(target)?;
			Self::do_vest(asset, who)
		}

		/// Create a vested transfer.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// - `asset`: Id of the asset class of the asset account for which the vesting applies.
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
		#[pallet::weight(T::WeightInfo::vested_transfer(T::MAX_VESTING_SCHEDULES))]
		pub fn vested_transfer(
			origin: OriginFor<T>,
			asset: AssetIdOf<T, I>,
			target: AccountIdLookupOf<T>,
			schedule: VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>,
		) -> DispatchResult {
			let transactor = ensure_signed(origin)?;
			let target = T::Lookup::lookup(target)?;
			Self::do_vested_transfer(
				asset,
				&transactor,
				&target,
				schedule,
				Preservation::Expendable,
			)
		}

		/// Force a vested transfer.
		///
		/// The dispatch origin for this call must be `ForceOrigin`.
		///
		/// - `asset`: Id of the asset class of the asset account for which the vesting applies.
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
		#[pallet::weight(T::WeightInfo::force_vested_transfer(T::MAX_VESTING_SCHEDULES))]
		pub fn force_vested_transfer(
			origin: OriginFor<T>,
			asset: AssetIdOf<T, I>,
			source: AccountIdLookupOf<T>,
			target: AccountIdLookupOf<T>,
			schedule: VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>,
		) -> DispatchResult {
			T::ForceOrigin::ensure_origin(origin)?;
			let target = T::Lookup::lookup(target)?;
			let source = T::Lookup::lookup(source)?;
			Self::do_vested_transfer(asset, &source, &target, schedule, Preservation::Expendable)
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
		/// NOTE: The outcome of this call is disadvantageous to the caller, since it ends up
		/// extending the vesting period of the merged schedules. Ideally, this should be only used
		/// when the caller has too many vesting schedules and cannot add one more.
		///
		/// Merged schedule attributes:
		/// - `starting_block`: `MAX(schedule1.starting_block, scheduled2.starting_block,
		///   current_block)`.
		/// - `ending_block`: `MAX(schedule1.ending_block, schedule2.ending_block)`.
		/// - `locked`: `schedule1.locked_at(current_block) + schedule2.locked_at(current_block)`.
		/// The dispatch origin for this call must be _Signed_.
		///
		/// - `asset`: Id of the asset class of the asset account for which the vesting applies.
		/// - `schedule1_index`: index of the first schedule to merge.
		/// - `schedule2_index`: index of the second schedule to merge.
		#[pallet::call_index(4)]
		#[pallet::weight(
				T::WeightInfo::not_unlocking_merge_schedules(T::MAX_VESTING_SCHEDULES)
					.max(T::WeightInfo::unlocking_merge_schedules(T::MAX_VESTING_SCHEDULES))
			)]
		pub fn merge_schedules(
			origin: OriginFor<T>,
			asset: AssetIdOf<T, I>,
			schedule1_index: u32,
			schedule2_index: u32,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			if schedule1_index == schedule2_index {
				return Ok(())
			};
			let schedule1_index = schedule1_index as usize;
			let schedule2_index = schedule2_index as usize;

			let schedules = Vesting::<T, I>::get(&asset, &who).ok_or(Error::<T, I>::NotVesting)?;
			let merge_action =
				VestingAction::Merge { index1: schedule1_index, index2: schedule2_index };

			let (schedules, locked_now) = Self::exec_action(schedules.to_vec(), merge_action)?;

			Self::write_vesting(asset.clone(), &who, schedules)?;
			Self::write_lock(asset, &who, locked_now)?;

			Ok(())
		}

		/// Force remove a vesting schedule
		///
		/// The dispatch origin for this call must be of `ForceOrigin`.
		///
		/// - `asset`: Id of the asset class of the asset account for which the vesting applies.
		/// - `target`: An account that has a vesting schedule
		/// - `schedule_index`: The vesting schedule index that should be removed
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::force_remove_vesting_schedule(T::MAX_VESTING_SCHEDULES))]
		pub fn force_remove_vesting_schedule(
			origin: OriginFor<T>,
			asset: AssetIdOf<T, I>,
			target: <T::Lookup as StaticLookup>::Source,
			schedule_index: u32,
		) -> DispatchResultWithPostInfo {
			T::ForceOrigin::ensure_origin(origin)?;
			let who = T::Lookup::lookup(target)?;

			let schedules_count =
				Vesting::<T, I>::decode_len(asset.clone(), &who).unwrap_or_default();
			ensure!(schedule_index < schedules_count as u32, Error::<T, I>::InvalidScheduleParams);

			Self::remove_vesting_schedule(asset, &who, schedule_index)?;

			Ok(Some(T::WeightInfo::force_remove_vesting_schedule(schedules_count as u32)).into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Public function for accessing vesting storage
	pub fn vesting(
		asset: AssetIdOf<T, I>,
		account: T::AccountId,
	) -> Option<
		BoundedVec<VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>, MaxVestingSchedulesGet<T, I>>,
	> {
		Vesting::<T, I>::get(asset, account)
	}

	// Create a new `VestingInfo`, based off of two other `VestingInfo`s.
	// NOTE: We assume both schedules have had funds unlocked up through the current block.
	fn merge_vesting_info(
		now: BlockNumberFor<T>,
		schedule1: VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>,
		schedule2: VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>,
	) -> Option<VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>> {
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

		let frozen = schedule1
			.locked_at::<T::BlockNumberToBalance>(now)
			.saturating_add(schedule2.locked_at::<T::BlockNumberToBalance>(now));
		// This shouldn't happen because we know at least one ending block is greater than now,
		// thus at least a schedule a some locked balance.
		debug_assert!(
			!frozen.is_zero(),
			"merge_vesting_info validation checks failed to catch a locked of 0"
		);

		let ending_block = schedule1_ending_block.max(schedule2_ending_block);
		let starting_block = now.max(schedule1.starting_block()).max(schedule2.starting_block());

		let per_block = {
			let duration = ending_block
				.saturating_sub(T::BlockNumberToBalance::convert(starting_block))
				.max(One::one());
			(frozen / duration).max(One::one())
		};

		let schedule = VestingInfo::new(frozen, per_block, starting_block);
		debug_assert!(schedule.is_valid(), "merge_vesting_info schedule validation check failed");

		Some(schedule)
	}

	// Execute a vested transfer from `source` to `target` with the given `schedule`.
	fn do_vested_transfer(
		asset: AssetIdOf<T, I>,
		source: &T::AccountId,
		target: &T::AccountId,
		schedule: VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>,
		preservation: Preservation,
	) -> DispatchResult {
		// Validate user inputs.
		ensure!(schedule.locked() >= T::MinVestedTransfer::get(), Error::<T, I>::AmountLow);
		if !schedule.is_valid() {
			return Err(Error::<T, I>::InvalidScheduleParams.into())
		};

		// Check we can add to this account prior to any storage writes.
		Self::can_add_vesting_schedule(
			asset.clone(),
			target,
			schedule.locked(),
			schedule.per_block(),
			schedule.starting_block(),
		)?;

		T::Assets::transfer(asset.clone(), source, target, schedule.locked(), preservation)?;

		// We can't let this fail because the currency transfer has already happened.
		// Must be successful as it has been checked before.
		// Better to return error on failure anyway.
		let res = Self::add_vesting_schedule(
			asset,
			target,
			schedule.locked(),
			schedule.per_block(),
			schedule.starting_block(),
		);
		debug_assert!(res.is_ok(), "Failed to add a schedule when we had to succeed.");

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
		schedules: Vec<VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>>,
		action: VestingAction,
	) -> (Vec<VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>>, BalanceOf<T, I>) {
		let now = T::BlockNumberProvider::current_block_number();

		let mut total_locked_now: BalanceOf<T, I> = Zero::zero();
		let filtered_schedules = action
			.pick_schedules::<T, I>(schedules)
			.filter(|schedule| {
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
	fn write_lock(
		asset: AssetIdOf<T, I>,
		who: &T::AccountId,
		total_locked_now: BalanceOf<T, I>,
	) -> DispatchResult {
		T::Freezer::set_freeze(
			asset.clone(),
			&FreezeReason::<I>::Vesting.into(),
			&who,
			total_locked_now,
		)?;

		if total_locked_now.is_zero() {
			Self::deposit_event(Event::<T, I>::VestingCompleted { asset, account: who.clone() });
		} else {
			Self::deposit_event(Event::<T, I>::VestingUpdated {
				asset,
				account: who.clone(),
				unvested: total_locked_now,
			});
		}

		Ok(())
	}

	/// Write an accounts updated vesting schedules to storage.
	fn write_vesting(
		asset: AssetIdOf<T, I>,
		who: &T::AccountId,
		schedules: Vec<VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>>,
	) -> Result<(), DispatchError> {
		let schedules: BoundedVec<
			VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>,
			MaxVestingSchedulesGet<T, I>,
		> = schedules.try_into().map_err(|_| Error::<T, I>::AtMaxVestingSchedules)?;

		if schedules.len() == 0 {
			Vesting::<T, I>::remove(asset, &who);
		} else {
			Vesting::<T, I>::insert(asset, who, schedules)
		}

		Ok(())
	}

	/// Unlock any vested funds of `who`.
	fn do_vest(asset: AssetIdOf<T, I>, who: T::AccountId) -> DispatchResult {
		let schedules = Vesting::<T, I>::get(&asset, &who).ok_or(Error::<T, I>::NotVesting)?;

		let (schedules, locked_now) =
			Self::exec_action(schedules.to_vec(), VestingAction::Passive)?;

		Self::write_vesting(asset.clone(), &who, schedules)?;
		Self::write_lock(asset, &who, locked_now)?;

		Ok(())
	}

	/// Execute a `VestingAction` against the given `schedules`. Returns the updated schedules
	/// and locked amount.
	fn exec_action(
		schedules: Vec<VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>>,
		action: VestingAction,
	) -> Result<
		(Vec<VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>>, BalanceOf<T, I>),
		DispatchError,
	> {
		let (schedules, locked_now) = match action {
			VestingAction::Merge { index1: idx1, index2: idx2 } => {
				// The schedule index is based off of the schedule ordering prior to filtering out
				// any schedules that may be ending at this block.
				let schedule1 =
					*schedules.get(idx1).ok_or(Error::<T, I>::ScheduleIndexOutOfBounds)?;
				let schedule2 =
					*schedules.get(idx2).ok_or(Error::<T, I>::ScheduleIndexOutOfBounds)?;

				// The length of `schedules` decreases by 2 here since we filter out 2 schedules.
				// Thus we know below that we can push the new merged schedule without error
				// (assuming initial state was valid).
				let (mut schedules, mut locked_now) =
					Self::report_schedule_updates(schedules.to_vec(), action);

				let now = T::BlockNumberProvider::current_block_number();
				if let Some(new_schedule) = Self::merge_vesting_info(now, schedule1, schedule2) {
					// Merging created a new schedule so we:
					// 1) need to add it to the accounts vesting schedule collection,
					schedules.push(new_schedule);
					// (we use `locked_at` in case this is a schedule that started in the past)
					let new_schedule_locked =
						new_schedule.locked_at::<T::BlockNumberToBalance>(now);
					// and 2) update the locked amount to reflect the schedule we just added.
					locked_now = locked_now.saturating_add(new_schedule_locked);
				} // In the None case there was no new schedule to account for.

				(schedules, locked_now)
			},
			_ => Self::report_schedule_updates(schedules.to_vec(), action),
		};

		debug_assert!(
			locked_now > Zero::zero() && schedules.len() > 0 ||
				locked_now == Zero::zero() && schedules.len() == 0
		);

		Ok((schedules, locked_now))
	}
}

impl<T: Config<I>, I: 'static> VestedInspect<T::AccountId> for Pallet<T, I>
where
	AssetIdOf<T, I>: MaybeSerializeDeserialize,
	BalanceOf<T, I>: MaybeSerializeDeserialize + Debug,
{
	type Moment = BlockNumberFor<T>;
	type AssetId = AssetIdOf<T, I>;
	type Balance = BalanceOf<T, I>;

	fn vesting_balance(asset: AssetIdOf<T, I>, who: &T::AccountId) -> Option<BalanceOf<T, I>> {
		Vesting::<T, I>::get(&asset, who).map(|v| {
			let now = T::BlockNumberProvider::current_block_number();
			let total_locked_now = v.iter().fold(Zero::zero(), |total, schedule| {
				schedule.locked_at::<T::BlockNumberToBalance>(now).saturating_add(total)
			});
			total_locked_now
		})
	}

	fn can_add_vesting_schedule(
		asset: AssetIdOf<T, I>,
		who: &T::AccountId,
		locked: BalanceOf<T, I>,
		per_block: BalanceOf<T, I>,
		starting_block: BlockNumberFor<T>,
	) -> DispatchResult {
		// Check for `per_block` or `locked` of 0.
		if !VestingInfo::new(locked, per_block, starting_block).is_valid() {
			return Err(Error::<T, I>::InvalidScheduleParams.into())
		}

		ensure!(
			(Vesting::<T, I>::decode_len(asset, who).unwrap_or_default() as u32) <
				T::MAX_VESTING_SCHEDULES,
			Error::<T, I>::AtMaxVestingSchedules
		);

		Ok(())
	}
}

impl<T: Config<I>, I: 'static> VestedMutate<T::AccountId> for Pallet<T, I> {
	fn add_vesting_schedule(
		asset: AssetIdOf<T, I>,
		who: &T::AccountId,
		locked: BalanceOf<T, I>,
		per_block: BalanceOf<T, I>,
		starting_block: BlockNumberFor<T>,
	) -> DispatchResult {
		if locked.is_zero() {
			return Ok(())
		}

		let vesting_schedule = VestingInfo::new(locked, per_block, starting_block);
		// Check for `per_block` or `locked` of 0.
		if !vesting_schedule.is_valid() {
			return Err(Error::<T, I>::InvalidScheduleParams.into())
		};

		let mut schedules = Vesting::<T, I>::get(&asset, who).unwrap_or_default();

		// NOTE: we must push the new schedule so that `exec_action`
		// will give the correct new locked amount.
		ensure!(schedules.try_push(vesting_schedule).is_ok(), Error::<T, I>::AtMaxVestingSchedules);

		let (schedules, locked_now) =
			Self::exec_action(schedules.to_vec(), VestingAction::Passive)?;

		Self::write_vesting(asset.clone(), who, schedules)?;
		Self::write_lock(asset, who, locked_now)?;

		Ok(())
	}

	fn remove_vesting_schedule(
		asset: AssetIdOf<T, I>,
		who: &T::AccountId,
		schedule_index: u32,
	) -> DispatchResult {
		let schedules = Vesting::<T, I>::get(&asset, who).ok_or(Error::<T, I>::NotVesting)?;
		let remove_action = VestingAction::Remove { index: schedule_index as usize };

		let (schedules, locked_now) = Self::exec_action(schedules.to_vec(), remove_action)?;

		Self::write_vesting(asset.clone(), who, schedules)?;
		Self::write_lock(asset, who, locked_now)?;
		Ok(())
	}
}

impl<T: Config<I>, I: 'static> VestedTransfer<T::AccountId> for Pallet<T, I>
where
	AssetIdOf<T, I>: MaybeSerializeDeserialize,
	BalanceOf<T, I>: MaybeSerializeDeserialize + Debug,
{
	fn vested_transfer(
		asset: AssetIdOf<T, I>,
		source: &T::AccountId,
		target: &T::AccountId,
		locked: BalanceOf<T, I>,
		per_block: BalanceOf<T, I>,
		starting_block: BlockNumberFor<T>,
	) -> DispatchResult {
		use storage::with_storage_layer;
		let schedule = VestingInfo::new(locked, per_block, starting_block);
		with_storage_layer(|| {
			Self::do_vested_transfer(asset, source, target, schedule, Preservation::Expendable)
		})
	}
}
