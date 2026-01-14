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

//! > Made with *Substrate*, for *Polkadot*.
//!
//! [![github]](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/time-scheduler) -
//! [![polkadot]](https://polkadot.com)
//!
//! [polkadot]: https://img.shields.io/badge/polkadot-E6007A?style=for-the-badge&logo=polkadot&logoColor=white
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//!
//! # Time Scheduler Pallet
//!
//! A Pallet for scheduling runtime calls based on timestamps.
//!
//! ## Overview
//!
//! This Pallet exposes capabilities for scheduling runtime calls to occur at a specified timestamp
//! (in milliseconds since Unix epoch) or after a specified duration. These scheduled runtime calls
//! may be named or anonymous and may be canceled.
//!
//! __NOTE:__ Instead of using the filter contained in the origin to call `fn schedule_at_time`,
//! scheduled runtime calls will be dispatched with the default filter for the origin: namely
//! `frame_system::Config::BaseCallFilter` for all origin types (except root which will get no
//! filter).
//!
//! If a call is scheduled using proxy or whatever mechanism which adds filter, then those filter
//! will not be used when dispatching the schedule runtime call.
//!
//! ### Examples
//!
//! 1. Scheduling a runtime call at a specific time.
#![doc = docify::embed!("src/tests.rs", basic_time_scheduling_works)]

//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Warning
//!
//! This Pallet executes all scheduled runtime calls in the [`on_initialize`] hook. Do not execute
//! any runtime calls which should not be considered mandatory.
//!
//! Please be aware that any scheduled runtime calls executed in a future block may __fail__ or may
//! result in __undefined behavior__ since the runtime could have upgraded between the time of
//! scheduling and execution. For example, the runtime upgrade could have:
//!
//! * Modified the implementation of the runtime call (runtime specification upgrade).
//!     * Could lead to undefined behavior.
//! * Removed or changed the ordering/index of the runtime call.
//!     * Could fail due to the runtime call index not being part of the `Call`.
//!     * Could lead to undefined behavior, such as executing another runtime call with the same
//!       index.
//!
//! [`on_initialize`]: frame_support::traits::Hooks::on_initialize

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod migration;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::{borrow::Borrow, cmp::Ordering, marker::PhantomData};
use frame_support::{
	dispatch::{DispatchResult, GetDispatchInfo, Parameter, RawOrigin},
	traits::{
		schedule,
		time_schedule,
		Bounded, CallerTrait, EnsureOrigin, Get, IsType, OriginTrait,
		PrivilegeCmp, QueryPreimage, StorageVersion, StorePreimage, Time,
	},
	weights::{Weight, WeightMeter},
};
use frame_system::{self as system};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{BadOrigin, Dispatchable, One, Saturating, Zero},
	DispatchError, RuntimeDebug,
};

pub use pallet::*;
pub use weights::WeightInfo;

/// Just a simple index for naming period tasks.
pub type PeriodicIndex = u32;
/// The location of a time-scheduled task (minute, index).
pub type TimeTaskAddress<Moment> = (Moment, u32);

/// The moment type used by a config's timestamp provider.
pub type MomentFor<T> = <<T as Config>::TimestampProvider as Time>::Moment;

/// Alias for TimeTaskAddress using the config's moment type.
pub type TimeTaskAddressFor<T> = TimeTaskAddress<MomentFor<T>>;

pub type BoundedCallOf<T> =
	Bounded<<T as Config>::RuntimeCall, <T as frame_system::Config>::Hashing>;

/// The configuration of the retry mechanism for a given task along with its current state.
#[derive(
	Clone,
	Copy,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct RetryConfig<Period> {
	/// Initial amount of retries allowed.
	pub total_retries: u8,
	/// Amount of retries left.
	pub remaining: u8,
	/// Period of time between retry attempts.
	pub period: Period,
}

/// Information regarding an item to be executed in the future.
#[derive(
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	DecodeWithMemTracking,
)]
pub struct Scheduled<Name, Call, Period, PalletsOrigin, AccountId> {
	/// The unique identity for this task, if there is one.
	pub maybe_id: Option<Name>,
	/// This task's priority.
	pub priority: schedule::Priority,
	/// The call to be dispatched.
	pub call: Call,
	/// If the call is periodic, then this points to the information concerning that.
	pub maybe_periodic: Option<time_schedule::Period<Period>>,
	/// The origin with which to dispatch the call.
	pub origin: PalletsOrigin,
	#[doc(hidden)]
	pub _phantom: PhantomData<AccountId>,
}

impl<Name, Call, Period, PalletsOrigin, AccountId>
	Scheduled<Name, Call, Period, PalletsOrigin, AccountId>
where
	Call: Clone,
	PalletsOrigin: Clone,
{
	/// Create a new task to be used for retry attempts of the original one. The cloned task will
	/// have the same `priority`, `call` and `origin`, but will always be non-periodic and unnamed.
	pub fn as_retry(&self) -> Self {
		Self {
			maybe_id: None,
			priority: self.priority,
			call: self.call.clone(),
			maybe_periodic: None,
			origin: self.origin.clone(),
			_phantom: Default::default(),
		}
	}
}

/// Scheduled task for time-based scheduling.
/// Uses `MomentFor<T>` for time period (milliseconds) instead of block number.
pub type ScheduledTimeOf<T> = Scheduled<
	TaskName,
	BoundedCallOf<T>,
	MomentFor<T>,
	<T as Config>::PalletsOrigin,
	<T as frame_system::Config>::AccountId,
>;

pub(crate) trait MarginalWeightInfo: WeightInfo {
	fn service_task(maybe_lookup_len: Option<usize>, named: bool, periodic: bool) -> Weight {
		let base = Self::service_task_base();
		let mut total = match maybe_lookup_len {
			None => base,
			Some(l) => Self::service_task_fetched(l as u32),
		};
		if named {
			total.saturating_accrue(Self::service_task_named().saturating_sub(base));
		}
		if periodic {
			total.saturating_accrue(Self::service_task_periodic().saturating_sub(base));
		}
		total
	}
}
impl<T: WeightInfo> MarginalWeightInfo for T {}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{dispatch::PostDispatchInfo, pallet_prelude::*};
	use frame_system::pallet_prelude::{BlockNumberFor as SystemBlockNumberFor, OriginFor};

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(4);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// `system::Config` should always be included in our implied traits.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The aggregated origin which the dispatch will take.
		type RuntimeOrigin: OriginTrait<PalletsOrigin = Self::PalletsOrigin>
			+ From<Self::PalletsOrigin>
			+ IsType<<Self as system::Config>::RuntimeOrigin>;

		/// The caller origin, overarching type of all pallets origins.
		type PalletsOrigin: From<system::RawOrigin<Self::AccountId>>
			+ CallerTrait<Self::AccountId>
			+ MaxEncodedLen;

		/// The aggregated call type.
		type RuntimeCall: Parameter
			+ Dispatchable<
				RuntimeOrigin = <Self as Config>::RuntimeOrigin,
				PostInfo = PostDispatchInfo,
			> + GetDispatchInfo
			+ From<system::Call<Self>>;

		/// The maximum weight that may be scheduled per block for any dispatchables.
		#[pallet::constant]
		type MaximumWeight: Get<Weight>;

		/// Required origin to schedule or cancel calls.
		type ScheduleOrigin: EnsureOrigin<<Self as system::Config>::RuntimeOrigin>;

		/// Compare the privileges of origins.
		///
		/// This will be used when canceling a task, to ensure that the origin that tries
		/// to cancel has greater or equal privileges as the origin that created the scheduled task.
		///
		/// For simplicity the [`EqualPrivilegeOnly`](frame_support::traits::EqualPrivilegeOnly) can
		/// be used. This will only check if two given origins are equal.
		type OriginPrivilegeCmp: PrivilegeCmp<Self::PalletsOrigin>;

		/// The maximum number of time-scheduled calls in the queue for a single minute.
		///
		/// This is used for the time-based scheduler where tasks are indexed by
		/// minute (timestamp / 60_000).
		#[pallet::constant]
		type MaxTimeScheduledPerMinute: Get<u32>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The preimage provider with which we look up call hashes to get the call.
		type Preimages: QueryPreimage<H = Self::Hashing> + StorePreimage;

		/// Provider for the current timestamp in milliseconds.
		///
		/// Used for time-based scheduling. Typically configured as `pallet_timestamp::Pallet<Self>`.
		///
		/// NOTE: The timestamp is read during `on_initialize`, so it will return the timestamp
		/// from the previous block. This means there is a 1-block delay for time-based scheduling.
		type TimestampProvider: Time;
	}

	/// Items to be executed, indexed by minute (timestamp in milliseconds / 60_000).
	///
	/// The key is the minute number since Unix epoch. Tasks scheduled for a specific
	/// time will be stored in the minute bucket they belong to.
	#[pallet::storage]
	pub type TimeAgenda<T: Config> = StorageMap<
		_,
		Twox64Concat,
		MomentFor<T>,
		BoundedVec<Option<ScheduledTimeOf<T>>, T::MaxTimeScheduledPerMinute>,
		ValueQuery,
	>;

	/// Minute at which the time agenda began incomplete execution.
	#[pallet::storage]
	pub type TimeIncompleteSince<T: Config> = StorageValue<_, MomentFor<T>>;

	/// Lookup from a name to the minute and index of the time-scheduled task.
	#[pallet::storage]
	pub type TimeLookup<T: Config> =
		StorageMap<_, Twox64Concat, TaskName, TimeTaskAddressFor<T>>;

	/// Retry configurations for time-based tasks, indexed by task address (minute, index).
	#[pallet::storage]
	pub type TimeRetries<T: Config> =
		StorageMap<_, Blake2_128Concat, TimeTaskAddressFor<T>, RetryConfig<MomentFor<T>>, OptionQuery>;

	/// Events type.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Scheduled some time-based task.
		TimeScheduled { when: MomentFor<T>, index: u32 },
		/// Canceled some time-based task.
		TimeCanceled { when: MomentFor<T>, index: u32 },
		/// Dispatched some time-based task.
		TimeDispatched {
			task: TimeTaskAddressFor<T>,
			id: Option<TaskName>,
			result: DispatchResult,
		},
		/// The call for the provided hash was not found so the time-based task has been aborted.
		TimeCallUnavailable { task: TimeTaskAddressFor<T>, id: Option<TaskName> },
		/// The given time-based task was unable to be renewed since the agenda is full.
		TimePeriodicFailed { task: TimeTaskAddressFor<T>, id: Option<TaskName> },
		/// The given time-based task can never be executed since it is overweight.
		TimePermanentlyOverweight { task: TimeTaskAddressFor<T>, id: Option<TaskName> },
		/// Time agenda is incomplete from `when` (minute).
		TimeAgendaIncomplete { when: MomentFor<T> },
		/// Set a retry configuration for some time-based task.
		TimeRetrySet {
			task: TimeTaskAddressFor<T>,
			id: Option<TaskName>,
			period: MomentFor<T>,
			retries: u8,
		},
		/// Cancel a retry configuration for some time-based task.
		TimeRetryCancelled { task: TimeTaskAddressFor<T>, id: Option<TaskName> },
		/// The given time-based task was unable to be retried since the agenda is full or there
		/// was not enough weight to reschedule it.
		TimeRetryFailed { task: TimeTaskAddressFor<T>, id: Option<TaskName> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Failed to schedule a call
		FailedToSchedule,
		/// Cannot find the scheduled call.
		NotFound,
		/// Given target timestamp is in the past.
		TargetTimestampInPast,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<SystemBlockNumberFor<T>> for Pallet<T> {
		/// Execute the scheduled calls (time-based)
		fn on_initialize(_now: SystemBlockNumberFor<T>) -> Weight {
			let mut weight_counter = frame_system::Pallet::<T>::remaining_block_weight()
				.limit_to(T::MaximumWeight::get());

			// Service time-based agendas
			// Note: This reads the timestamp from the previous block (1-block delay)
			let now = T::TimestampProvider::now();
			Self::service_time_agendas(&mut weight_counter, now, u32::MAX);

			weight_counter.consumed()
		}

		#[cfg(feature = "std")]
		fn integrity_test() {
			/// Calculate the maximum weight that a lookup of a given size can take.
			fn lookup_weight<T: Config>(s: usize) -> Weight {
				T::WeightInfo::service_agendas_base()
					+ T::WeightInfo::service_agenda_base(T::MaxTimeScheduledPerMinute::get())
					+ T::WeightInfo::service_task(Some(s), true, true)
			}

			let limit = sp_runtime::Perbill::from_percent(90) * T::MaximumWeight::get();

			let small_lookup = lookup_weight::<T>(128);
			assert!(small_lookup.all_lte(limit), "Must be possible to submit a small lookup");

			let medium_lookup = lookup_weight::<T>(1024);
			assert!(medium_lookup.all_lte(limit), "Must be possible to submit a medium lookup");

			let large_lookup = lookup_weight::<T>(1024 * 1024);
			assert!(large_lookup.all_lte(limit), "Must be possible to submit a large lookup");
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Schedule a task at a specific timestamp (in milliseconds since Unix epoch).
		///
		/// The task will be dispatched during the first block where the timestamp
		/// is greater than or equal to `when`. Note that there is typically a 1-block
		/// delay since the timestamp is read from the previous block.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule(T::MaxTimeScheduledPerMinute::get()))]
		pub fn schedule_at_time(
			origin: OriginFor<T>,
			when: MomentFor<T>,
			maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
			priority: schedule::Priority,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_schedule_at_time(
				when,
				maybe_periodic,
				priority,
				origin.caller().clone(),
				T::Preimages::bound(*call)?,
			)?;
			Ok(())
		}

		/// Schedule a named task at a specific timestamp (in milliseconds since Unix epoch).
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_named(T::MaxTimeScheduledPerMinute::get()))]
		pub fn schedule_named_at_time(
			origin: OriginFor<T>,
			id: TaskName,
			when: MomentFor<T>,
			maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
			priority: schedule::Priority,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_schedule_named_at_time(
				id,
				when,
				maybe_periodic,
				priority,
				origin.caller().clone(),
				T::Preimages::bound(*call)?,
			)?;
			Ok(())
		}

		/// Cancel an anonymously scheduled time-based task.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel(T::MaxTimeScheduledPerMinute::get()))]
		pub fn cancel_time_task(
			origin: OriginFor<T>,
			minute: MomentFor<T>,
			index: u32,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_cancel_time_task(Some(origin.caller().clone()), (minute, index))?;
			Ok(())
		}

		/// Cancel a named time-based task.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_named(T::MaxTimeScheduledPerMinute::get()))]
		pub fn cancel_time_named(origin: OriginFor<T>, id: TaskName) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_cancel_time_named(Some(origin.caller().clone()), id)?;
			Ok(())
		}

		/// Anonymously schedule a task after a delay (in milliseconds).
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule(T::MaxTimeScheduledPerMinute::get()))]
		pub fn schedule_after_time(
			origin: OriginFor<T>,
			after: MomentFor<T>,
			maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
			priority: schedule::Priority,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			let now = T::TimestampProvider::now();
			let when = now.saturating_add(after);
			Self::do_schedule_at_time(
				when,
				maybe_periodic,
				priority,
				origin.caller().clone(),
				T::Preimages::bound(*call)?,
			)?;
			Ok(())
		}

		/// Schedule a named task after a delay (in milliseconds).
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_named(T::MaxTimeScheduledPerMinute::get()))]
		pub fn schedule_named_after_time(
			origin: OriginFor<T>,
			id: TaskName,
			after: MomentFor<T>,
			maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
			priority: schedule::Priority,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			let now = T::TimestampProvider::now();
			let when = now.saturating_add(after);
			Self::do_schedule_named_at_time(
				id,
				when,
				maybe_periodic,
				priority,
				origin.caller().clone(),
				T::Preimages::bound(*call)?,
			)?;
			Ok(())
		}

		/// Set a retry configuration for a time-based task so that, in case its scheduled run
		/// fails, it will be retried after `period` milliseconds, for a total amount of `retries`
		/// retries or until it succeeds.
		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::set_retry())]
		pub fn set_time_retry(
			origin: OriginFor<T>,
			task: TimeTaskAddressFor<T>,
			retries: u8,
			period: MomentFor<T>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			let (minute, index) = task;
			let agenda = TimeAgenda::<T>::get(minute);
			let scheduled = agenda
				.get(index as usize)
				.and_then(Option::as_ref)
				.ok_or(Error::<T>::NotFound)?;
			Self::ensure_privilege(origin.caller(), &scheduled.origin)?;
			TimeRetries::<T>::insert(
				task,
				RetryConfig { total_retries: retries, remaining: retries, period },
			);
			Self::deposit_event(Event::TimeRetrySet { task, id: None, period, retries });
			Ok(())
		}

		/// Set a retry configuration for a named time-based task so that, in case its scheduled
		/// run fails, it will be retried after `period` milliseconds, for a total amount of
		/// `retries` retries or until it succeeds.
		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::set_retry_named())]
		pub fn set_time_retry_named(
			origin: OriginFor<T>,
			id: TaskName,
			retries: u8,
			period: MomentFor<T>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			let (minute, agenda_index) = TimeLookup::<T>::get(&id).ok_or(Error::<T>::NotFound)?;
			let agenda = TimeAgenda::<T>::get(minute);
			let scheduled = agenda
				.get(agenda_index as usize)
				.and_then(Option::as_ref)
				.ok_or(Error::<T>::NotFound)?;
			Self::ensure_privilege(origin.caller(), &scheduled.origin)?;
			TimeRetries::<T>::insert(
				(minute, agenda_index),
				RetryConfig { total_retries: retries, remaining: retries, period },
			);
			Self::deposit_event(Event::TimeRetrySet {
				task: (minute, agenda_index),
				id: Some(id),
				period,
				retries,
			});
			Ok(())
		}

		/// Removes the retry configuration of a time-based task.
		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_retry())]
		pub fn cancel_time_retry(
			origin: OriginFor<T>,
			task: TimeTaskAddressFor<T>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_cancel_time_retry(origin.caller(), task)?;
			Self::deposit_event(Event::TimeRetryCancelled { task, id: None });
			Ok(())
		}

		/// Cancel the retry configuration of a named time-based task.
		#[pallet::call_index(9)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_retry_named())]
		pub fn cancel_time_retry_named(origin: OriginFor<T>, id: TaskName) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			let task = TimeLookup::<T>::get(&id).ok_or(Error::<T>::NotFound)?;
			Self::do_cancel_time_retry(origin.caller(), task)?;
			Self::deposit_event(Event::TimeRetryCancelled { task, id: Some(id) });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	// ==================== Time-based scheduling functions ====================

	/// Convert a timestamp in milliseconds to a minute key (timestamp / 60_000).
	fn timestamp_to_minute(timestamp: MomentFor<T>) -> MomentFor<T> {
		timestamp / 60_000u32.into()
	}

	/// Schedule a task at a specific timestamp.
	fn do_schedule_at_time(
		when: MomentFor<T>,
		maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
		priority: schedule::Priority,
		origin: T::PalletsOrigin,
		call: BoundedCallOf<T>,
	) -> Result<TimeTaskAddressFor<T>, DispatchError> {
		// Get current timestamp from the timestamp provider
		let now = T::TimestampProvider::now();

		// Ensure the target time is in the future
		if when <= now {
			return Err(Error::<T>::TargetTimestampInPast.into());
		}

		let lookup_hash = call.lookup_hash();

		// Sanitize maybe_periodic: period must be > 0 and count must be > 1
		let maybe_periodic = maybe_periodic
			.filter(|p| p.1 > 1 && p.0 > Zero::zero())
			// Remove one from the number of repetitions since we will schedule one now.
			.map(|(p, c)| (p, c - 1));

		let task = Scheduled {
			maybe_id: None,
			priority,
			call,
			maybe_periodic,
			origin,
			_phantom: PhantomData,
		};

		let res = Self::place_time_task(when, task).map_err(|x| x.0)?;

		if let Some(hash) = lookup_hash {
			// Request the call to be made available.
			T::Preimages::request(&hash);
		}

		Ok(res)
	}

	/// Schedule a named task at a specific timestamp.
	fn do_schedule_named_at_time(
		id: TaskName,
		when: MomentFor<T>,
		maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
		priority: schedule::Priority,
		origin: T::PalletsOrigin,
		call: BoundedCallOf<T>,
	) -> Result<TimeTaskAddressFor<T>, DispatchError> {
		// Ensure id is unique
		if TimeLookup::<T>::contains_key(&id) {
			return Err(Error::<T>::FailedToSchedule.into());
		}

		let now = T::TimestampProvider::now();

		if when <= now {
			return Err(Error::<T>::TargetTimestampInPast.into());
		}

		let lookup_hash = call.lookup_hash();

		// Sanitize maybe_periodic
		let maybe_periodic = maybe_periodic
			.filter(|p| p.1 > 1 && p.0 > Zero::zero())
			.map(|(p, c)| (p, c - 1));

		let task = Scheduled {
			maybe_id: Some(id),
			priority,
			call,
			maybe_periodic,
			origin,
			_phantom: PhantomData,
		};

		let res = Self::place_time_task(when, task).map_err(|x| x.0)?;

		if let Some(hash) = lookup_hash {
			T::Preimages::request(&hash);
		}

		Ok(res)
	}

	/// Place a time-based task in the agenda and update lookup if named.
	fn place_time_task(
		when: MomentFor<T>,
		what: ScheduledTimeOf<T>,
	) -> Result<TimeTaskAddressFor<T>, (DispatchError, ScheduledTimeOf<T>)> {
		let maybe_name = what.maybe_id;
		let minute = Self::timestamp_to_minute(when);
		let index = Self::push_to_time_agenda(minute, what)?;
		let address = (minute, index);
		if let Some(name) = maybe_name {
			TimeLookup::<T>::insert(name, address);
		}
		Self::deposit_event(Event::TimeScheduled { when: minute, index });
		Ok(address)
	}

	/// Push a task to the time agenda for a given minute.
	fn push_to_time_agenda(
		minute: MomentFor<T>,
		what: ScheduledTimeOf<T>,
	) -> Result<u32, (DispatchError, ScheduledTimeOf<T>)> {
		let mut agenda = TimeAgenda::<T>::get(minute);
		let index = if (agenda.len() as u32) < T::MaxTimeScheduledPerMinute::get() {
			// Will always succeed due to the above check.
			let _ = agenda.try_push(Some(what));
			agenda.len() as u32 - 1
		} else {
			if let Some(hole_index) = agenda.iter().position(|i| i.is_none()) {
				agenda[hole_index] = Some(what);
				hole_index as u32
			} else {
				return Err((DispatchError::Exhausted, what));
			}
		};
		TimeAgenda::<T>::insert(minute, agenda);
		Ok(index)
	}

	/// Remove trailing `None` items of a time agenda at `minute`. If all items are `None` remove
	/// the agenda record entirely.
	fn cleanup_time_agenda(minute: MomentFor<T>) {
		let mut agenda = TimeAgenda::<T>::get(minute);
		match agenda.iter().rposition(|i| i.is_some()) {
			Some(i) if agenda.len() > i + 1 => {
				agenda.truncate(i + 1);
				TimeAgenda::<T>::insert(minute, agenda);
			},
			Some(_) => {},
			None => {
				TimeAgenda::<T>::remove(minute);
			},
		}
	}

	/// Cancel a time-based task by address.
	fn do_cancel_time_task(
		origin: Option<T::PalletsOrigin>,
		(minute, index): TimeTaskAddressFor<T>,
	) -> Result<(), DispatchError> {
		let scheduled = TimeAgenda::<T>::try_mutate(minute, |agenda| {
			agenda.get_mut(index as usize).map_or(
				Ok(None),
				|s| -> Result<Option<ScheduledTimeOf<T>>, DispatchError> {
					if let (Some(ref o), Some(ref s)) = (origin, s.borrow()) {
						Self::ensure_privilege(o, &s.origin)?;
					};
					Ok(s.take())
				},
			)
		})?;
		if let Some(s) = scheduled {
			T::Preimages::drop(&s.call);
			if let Some(id) = s.maybe_id {
				TimeLookup::<T>::remove(id);
			}
			Self::cleanup_time_agenda(minute);
			Self::deposit_event(Event::TimeCanceled { when: minute, index });
			Ok(())
		} else {
			Err(Error::<T>::NotFound.into())
		}
	}

	/// Cancel a named time-based task.
	fn do_cancel_time_named(origin: Option<T::PalletsOrigin>, id: TaskName) -> DispatchResult {
		TimeLookup::<T>::try_mutate_exists(id, |lookup| -> DispatchResult {
			if let Some((minute, index)) = lookup.take() {
				let i = index as usize;
				TimeAgenda::<T>::try_mutate(minute, |agenda| -> DispatchResult {
					if let Some(s) = agenda.get_mut(i) {
						if let (Some(ref o), Some(ref s)) = (origin, s.borrow()) {
							Self::ensure_privilege(o, &s.origin)?;
							T::Preimages::drop(&s.call);
						}
						*s = None;
					}
					Ok(())
				})?;
				Self::cleanup_time_agenda(minute);
				Self::deposit_event(Event::TimeCanceled { when: minute, index });
				Ok(())
			} else {
				Err(Error::<T>::NotFound.into())
			}
		})
	}

	/// Cancel the retry configuration for a time-based task.
	fn do_cancel_time_retry(
		origin: &T::PalletsOrigin,
		(minute, index): TimeTaskAddressFor<T>,
	) -> Result<(), DispatchError> {
		let agenda = TimeAgenda::<T>::get(minute);
		let scheduled = agenda
			.get(index as usize)
			.and_then(Option::as_ref)
			.ok_or(Error::<T>::NotFound)?;
		Self::ensure_privilege(origin, &scheduled.origin)?;
		TimeRetries::<T>::remove((minute, index));
		Ok(())
	}

	/// Schedule a retry for a time-based task that failed.
	fn schedule_time_retry(
		weight: &mut WeightMeter,
		now: MomentFor<T>,
		minute: MomentFor<T>,
		agenda_index: u32,
		task: &ScheduledTimeOf<T>,
		retry_config: RetryConfig<MomentFor<T>>,
	) {
		if weight
			.try_consume(T::WeightInfo::schedule_retry(T::MaxTimeScheduledPerMinute::get()))
			.is_err()
		{
			Self::deposit_event(Event::TimeRetryFailed {
				task: (minute, agenda_index),
				id: task.maybe_id,
			});
			return;
		}

		let RetryConfig { total_retries, mut remaining, period } = retry_config;
		remaining = match remaining.checked_sub(1) {
			Some(n) => n,
			None => return,
		};
		let wake = now.saturating_add(period);
		match Self::place_time_task(wake, task.as_retry()) {
			Ok(address) => {
				// Reinsert the retry config to the new address of the task after it was placed.
				TimeRetries::<T>::insert(address, RetryConfig { total_retries, remaining, period });
			},
			Err((_, task)) => {
				T::Preimages::drop(&task.call);
				Self::deposit_event(Event::TimeRetryFailed {
					task: (minute, agenda_index),
					id: task.maybe_id,
				});
			},
		}
	}
}

enum ServiceTaskError {
	/// Could not be executed due to missing preimage.
	Unavailable,
	/// Could not be executed due to weight limitations.
	Overweight,
}
use ServiceTaskError::*;

impl<T: Config> Pallet<T> {
	/// Make a dispatch to the given `call` from the given `origin`, ensuring that the `weight`
	/// counter does not exceed its limit and that it is counted accurately (e.g. accounted using
	/// post info if available).
	///
	/// NOTE: Only the weight for this function will be counted (origin lookup, dispatch and the
	/// call itself).
	///
	/// Returns an error if the call is overweight.
	fn execute_dispatch(
		weight: &mut WeightMeter,
		origin: T::PalletsOrigin,
		call: <T as Config>::RuntimeCall,
	) -> Result<DispatchResult, ()> {
		let base_weight = match origin.as_system_ref() {
			Some(&RawOrigin::Signed(_)) => T::WeightInfo::execute_dispatch_signed(),
			_ => T::WeightInfo::execute_dispatch_unsigned(),
		};
		let call_weight = call.get_dispatch_info().call_weight;
		// We only allow a scheduled call if it cannot push the weight past the limit.
		let max_weight = base_weight.saturating_add(call_weight);

		if !weight.can_consume(max_weight) {
			return Err(());
		}

		let dispatch_origin = origin.into();
		let (maybe_actual_call_weight, result) = match call.dispatch(dispatch_origin) {
			Ok(post_info) => (post_info.actual_weight, Ok(())),
			Err(error_and_info) => {
				(error_and_info.post_info.actual_weight, Err(error_and_info.error))
			},
		};
		let call_weight = maybe_actual_call_weight.unwrap_or(call_weight);
		let _ = weight.try_consume(base_weight);
		let _ = weight.try_consume(call_weight);
		Ok(result)
	}

	/// Ensure that `left` has at least the same level of privilege or higher than `right`.
	///
	/// Returns an error if `left` has a lower level of privilege or the two cannot be compared.
	fn ensure_privilege(
		left: &<T as Config>::PalletsOrigin,
		right: &<T as Config>::PalletsOrigin,
	) -> Result<(), DispatchError> {
		if matches!(T::OriginPrivilegeCmp::cmp_privilege(left, right), Some(Ordering::Less) | None)
		{
			return Err(BadOrigin.into());
		}
		Ok(())
	}

	// ==================== Time-based dispatch service functions ====================

	/// Service time-based agendas starting from the earliest incomplete minute.
	fn service_time_agendas(weight: &mut WeightMeter, now: MomentFor<T>, max: u32) {
		if weight.try_consume(T::WeightInfo::service_agendas_base()).is_err() {
			return;
		}

		let now_minute = Self::timestamp_to_minute(now);
		let mut incomplete_since = now_minute + One::one();
		let mut minute = TimeIncompleteSince::<T>::take().unwrap_or(now_minute);
		let mut is_first = true;

		let max_items = T::MaxTimeScheduledPerMinute::get();
		let mut count_down = max;
		let service_agenda_base_weight = T::WeightInfo::service_agenda_base(max_items);
		while count_down > 0
			&& minute <= now_minute
			&& weight.can_consume(service_agenda_base_weight)
		{
			if !Self::service_time_agenda(weight, is_first, now, minute, u32::MAX) {
				incomplete_since = incomplete_since.min(minute);
			}
			is_first = false;
			minute = minute.saturating_add(One::one());
			count_down = count_down.saturating_sub(1);
		}
		incomplete_since = incomplete_since.min(minute);
		if incomplete_since <= now_minute {
			Self::deposit_event(Event::TimeAgendaIncomplete { when: incomplete_since });
			TimeIncompleteSince::<T>::put(incomplete_since);
		} else {
			// Start from the next minute on the next iteration
			TimeIncompleteSince::<T>::put(now_minute + One::one());
		}
	}

	/// Service a single time agenda for the given minute.
	/// Returns `true` if the agenda was fully completed.
	fn service_time_agenda(
		weight: &mut WeightMeter,
		mut is_first: bool,
		now: MomentFor<T>,
		minute: MomentFor<T>,
		max: u32,
	) -> bool {
		let mut agenda = TimeAgenda::<T>::get(minute);
		let mut ordered = agenda
			.iter()
			.enumerate()
			.filter_map(|(index, maybe_item)| {
				maybe_item.as_ref().map(|item| (index as u32, item.priority))
			})
			.collect::<Vec<_>>();
		ordered.sort_by_key(|k| k.1);
		let within_limit = weight
			.try_consume(T::WeightInfo::service_agenda_base(ordered.len() as u32))
			.is_ok();
		debug_assert!(within_limit, "weight limit should have been checked in advance");

		let mut postponed = (ordered.len() as u32).saturating_sub(max);
		let mut dropped = 0;

		for (agenda_index, _) in ordered.into_iter().take(max as usize) {
			let Some(task) = agenda[agenda_index as usize].take() else { continue };
			let base_weight = T::WeightInfo::service_task(
				task.call.lookup_len().map(|x| x as usize),
				task.maybe_id.is_some(),
				task.maybe_periodic.is_some(),
			);
			if !weight.can_consume(base_weight) {
				postponed += 1;
				agenda[agenda_index as usize] = Some(task);
				break;
			}
			let result =
				Self::service_time_task(weight, now, minute, agenda_index, is_first, task);
			agenda[agenda_index as usize] = match result {
				Err((Unavailable, slot)) => {
					dropped += 1;
					slot
				},
				Err((Overweight, slot)) => {
					postponed += 1;
					slot
				},
				Ok(()) => {
					is_first = false;
					None
				},
			};
		}
		if postponed > 0 || dropped > 0 {
			TimeAgenda::<T>::insert(minute, agenda);
		} else {
			TimeAgenda::<T>::remove(minute);
		}

		postponed == 0
	}

	/// Service a single time-based task.
	fn service_time_task(
		weight: &mut WeightMeter,
		now: MomentFor<T>,
		minute: MomentFor<T>,
		agenda_index: u32,
		is_first: bool,
		mut task: ScheduledTimeOf<T>,
	) -> Result<(), (ServiceTaskError, Option<ScheduledTimeOf<T>>)> {
		if let Some(ref id) = task.maybe_id {
			TimeLookup::<T>::remove(id);
		}

		let (call, lookup_len) = match T::Preimages::peek(&task.call) {
			Ok(c) => c,
			Err(_) => {
				Self::deposit_event(Event::TimeCallUnavailable {
					task: (minute, agenda_index),
					id: task.maybe_id,
				});
				T::Preimages::drop(&task.call);
				let _ = weight.try_consume(T::WeightInfo::service_task(
					task.call.lookup_len().map(|x| x as usize),
					task.maybe_id.is_some(),
					task.maybe_periodic.is_some(),
				));
				return Err((Unavailable, Some(task)));
			},
		};

		let _ = weight.try_consume(T::WeightInfo::service_task(
			lookup_len.map(|x| x as usize),
			task.maybe_id.is_some(),
			task.maybe_periodic.is_some(),
		));

		match Self::execute_dispatch(weight, task.origin.clone(), call) {
			Err(()) if is_first => {
				T::Preimages::drop(&task.call);
				Self::deposit_event(Event::TimePermanentlyOverweight {
					task: (minute, agenda_index),
					id: task.maybe_id,
				});
				Err((Unavailable, Some(task)))
			},
			Err(()) => Err((Overweight, Some(task))),
			Ok(result) => {
				let failed = result.is_err();
				let maybe_retry_config = TimeRetries::<T>::take((minute, agenda_index));
				Self::deposit_event(Event::TimeDispatched {
					task: (minute, agenda_index),
					id: task.maybe_id,
					result,
				});

				match maybe_retry_config {
					Some(retry_config) if failed => {
						Self::schedule_time_retry(
							weight,
							now,
							minute,
							agenda_index,
							&task,
							retry_config,
						);
					},
					_ => {},
				}

				// Handle periodic rescheduling
				if let &Some((period, count)) = &task.maybe_periodic {
					if count > 1 {
						task.maybe_periodic = Some((period, count - 1));
					} else {
						task.maybe_periodic = None;
					}
					let wake = now.saturating_add(period);
					match Self::place_time_task(wake, task) {
						Ok(new_address) => {
							if let Some(retry_config) = maybe_retry_config {
								TimeRetries::<T>::insert(new_address, retry_config);
							}
						},
						Err((_, task)) => {
							T::Preimages::drop(&task.call);
							Self::deposit_event(Event::TimePeriodicFailed {
								task: (minute, agenda_index),
								id: task.maybe_id,
							});
						},
					}
				} else {
					T::Preimages::drop(&task.call);
				}
				Ok(())
			},
		}
	}
}

use time_schedule::v1::TaskName;
