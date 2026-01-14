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
//! __NOTE:__ Instead of using the filter contained in the origin to call `fn schedule`,
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
		time_schedule::{self, DispatchTime},
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
/// The location of a scheduled task (minute, index).
pub type TaskAddress<Moment> = (Moment, u32);

/// The moment type used by a config's timestamp provider.
pub type MomentFor<T> = <<T as Config>::TimestampProvider as Time>::Moment;

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

/// Scheduled task type alias.
/// Uses `MomentFor<T>` for time period (milliseconds) instead of block number.
pub type ScheduledOf<T> = Scheduled<
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

		/// The maximum number of scheduled calls in the queue for a single minute.
		///
		/// NOTE:
		/// + Dependent pallets' benchmarks might require a higher limit for the setting. Set a
		/// higher limit under `runtime-benchmarks` feature.
		#[pallet::constant]
		type MaxScheduledPerMinute: Get<u32>;

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

	/// Minute at which the agenda began incomplete execution.
	#[pallet::storage]
	pub type IncompleteSince<T: Config> = StorageValue<_, MomentFor<T>>;

	/// Items to be executed, indexed by minute (timestamp in milliseconds / 60_000).
	///
	/// The key is the minute number since Unix epoch. Tasks scheduled for a specific
	/// time will be stored in the minute bucket they belong to.
	#[pallet::storage]
	pub type Agenda<T: Config> = StorageMap<
		_,
		Twox64Concat,
		MomentFor<T>,
		BoundedVec<Option<ScheduledOf<T>>, T::MaxScheduledPerMinute>,
		ValueQuery,
	>;

	/// Retry configurations for tasks, indexed by task address (minute, index).
	#[pallet::storage]
	pub type Retries<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		TaskAddress<MomentFor<T>>,
		RetryConfig<MomentFor<T>>,
		OptionQuery,
	>;

	/// Lookup from a name to the minute and index of the task.
	#[pallet::storage]
	pub type Lookup<T: Config> =
		StorageMap<_, Twox64Concat, TaskName, TaskAddress<MomentFor<T>>>;

	/// Events type.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Scheduled some task.
		Scheduled { when: MomentFor<T>, index: u32 },
		/// Canceled some task.
		Canceled { when: MomentFor<T>, index: u32 },
		/// Dispatched some task.
		Dispatched {
			task: TaskAddress<MomentFor<T>>,
			id: Option<TaskName>,
			result: DispatchResult,
		},
		/// Set a retry configuration for some task.
		RetrySet {
			task: TaskAddress<MomentFor<T>>,
			id: Option<TaskName>,
			period: MomentFor<T>,
			retries: u8,
		},
		/// Cancel a retry configuration for some task.
		RetryCancelled { task: TaskAddress<MomentFor<T>>, id: Option<TaskName> },
		/// The call for the provided hash was not found so the task has been aborted.
		CallUnavailable { task: TaskAddress<MomentFor<T>>, id: Option<TaskName> },
		/// The given task was unable to be renewed since the agenda is full at that minute.
		PeriodicFailed { task: TaskAddress<MomentFor<T>>, id: Option<TaskName> },
		/// The given task was unable to be retried since the agenda is full at that minute or there
		/// was not enough weight to reschedule it.
		RetryFailed { task: TaskAddress<MomentFor<T>>, id: Option<TaskName> },
		/// The given task can never be executed since it is overweight.
		PermanentlyOverweight { task: TaskAddress<MomentFor<T>>, id: Option<TaskName> },
		/// Agenda is incomplete from `when`.
		AgendaIncomplete { when: MomentFor<T> },
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
		/// Execute the scheduled calls
		fn on_initialize(_now: SystemBlockNumberFor<T>) -> Weight {
			let now = T::TimestampProvider::now();
			let mut weight_counter = frame_system::Pallet::<T>::remaining_block_weight()
				.limit_to(T::MaximumWeight::get());

			// Service agendas
			// Note: This reads the timestamp from the previous block (1-block delay)
			Self::service_agendas(&mut weight_counter, now, u32::MAX);

			weight_counter.consumed()
		}

		#[cfg(feature = "std")]
		fn integrity_test() {
			/// Calculate the maximum weight that a lookup of a given size can take.
			fn lookup_weight<T: Config>(s: usize) -> Weight {
				T::WeightInfo::service_agendas_base()
					+ T::WeightInfo::service_agenda_base(T::MaxScheduledPerMinute::get())
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
		#[pallet::weight(<T as Config>::WeightInfo::schedule(T::MaxScheduledPerMinute::get()))]
		pub fn schedule(
			origin: OriginFor<T>,
			when: MomentFor<T>,
			maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
			priority: schedule::Priority,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_schedule(
				DispatchTime::At(when),
				maybe_periodic,
				priority,
				origin.caller().clone(),
				T::Preimages::bound(*call)?,
			)?;
			Ok(())
		}

		/// Cancel an anonymously scheduled task.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel(T::MaxScheduledPerMinute::get()))]
		pub fn cancel(
			origin: OriginFor<T>,
			when: MomentFor<T>,
			index: u32,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_cancel(Some(origin.caller().clone()), (when, index))?;
			Ok(())
		}

		/// Schedule a named task at a specific timestamp (in milliseconds since Unix epoch).
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_named(T::MaxScheduledPerMinute::get()))]
		pub fn schedule_named(
			origin: OriginFor<T>,
			id: TaskName,
			when: MomentFor<T>,
			maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
			priority: schedule::Priority,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_schedule_named(
				id,
				DispatchTime::At(when),
				maybe_periodic,
				priority,
				origin.caller().clone(),
				T::Preimages::bound(*call)?,
			)?;
			Ok(())
		}

		/// Cancel a named scheduled task.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_named(T::MaxScheduledPerMinute::get()))]
		pub fn cancel_named(origin: OriginFor<T>, id: TaskName) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_cancel_named(Some(origin.caller().clone()), id)?;
			Ok(())
		}

		/// Anonymously schedule a task after a delay (in milliseconds).
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule(T::MaxScheduledPerMinute::get()))]
		pub fn schedule_after(
			origin: OriginFor<T>,
			after: MomentFor<T>,
			maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
			priority: schedule::Priority,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_schedule(
				DispatchTime::After(after),
				maybe_periodic,
				priority,
				origin.caller().clone(),
				T::Preimages::bound(*call)?,
			)?;
			Ok(())
		}

		/// Schedule a named task after a delay (in milliseconds).
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_named(T::MaxScheduledPerMinute::get()))]
		pub fn schedule_named_after(
			origin: OriginFor<T>,
			id: TaskName,
			after: MomentFor<T>,
			maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
			priority: schedule::Priority,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_schedule_named(
				id,
				DispatchTime::After(after),
				maybe_periodic,
				priority,
				origin.caller().clone(),
				T::Preimages::bound(*call)?,
			)?;
			Ok(())
		}

		/// Set a retry configuration for a task so that, in case its scheduled run
		/// fails, it will be retried after `period` milliseconds, for a total amount of `retries`
		/// retries or until it succeeds.
		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::set_retry())]
		pub fn set_retry(
			origin: OriginFor<T>,
			task: TaskAddress<MomentFor<T>>,
			retries: u8,
			period: MomentFor<T>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			let (when, index) = task;
			let agenda = Agenda::<T>::get(when);
			let scheduled = agenda
				.get(index as usize)
				.and_then(Option::as_ref)
				.ok_or(Error::<T>::NotFound)?;
			Self::ensure_privilege(origin.caller(), &scheduled.origin)?;
			Retries::<T>::insert(
				task,
				RetryConfig { total_retries: retries, remaining: retries, period },
			);
			Self::deposit_event(Event::RetrySet { task, id: None, period, retries });
			Ok(())
		}

		/// Set a retry configuration for a named task so that, in case its scheduled
		/// run fails, it will be retried after `period` milliseconds, for a total amount of
		/// `retries` retries or until it succeeds.
		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::set_retry_named())]
		pub fn set_retry_named(
			origin: OriginFor<T>,
			id: TaskName,
			retries: u8,
			period: MomentFor<T>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			let (when, agenda_index) = Lookup::<T>::get(&id).ok_or(Error::<T>::NotFound)?;
			let agenda = Agenda::<T>::get(when);
			let scheduled = agenda
				.get(agenda_index as usize)
				.and_then(Option::as_ref)
				.ok_or(Error::<T>::NotFound)?;
			Self::ensure_privilege(origin.caller(), &scheduled.origin)?;
			Retries::<T>::insert(
				(when, agenda_index),
				RetryConfig { total_retries: retries, remaining: retries, period },
			);
			Self::deposit_event(Event::RetrySet {
				task: (when, agenda_index),
				id: Some(id),
				period,
				retries,
			});
			Ok(())
		}

		/// Removes the retry configuration of a task.
		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_retry())]
		pub fn cancel_retry(
			origin: OriginFor<T>,
			task: TaskAddress<MomentFor<T>>,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_cancel_retry(origin.caller(), task)?;
			Self::deposit_event(Event::RetryCancelled { task, id: None });
			Ok(())
		}

		/// Cancel the retry configuration of a named task.
		#[pallet::call_index(9)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_retry_named())]
		pub fn cancel_retry_named(origin: OriginFor<T>, id: TaskName) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			let task = Lookup::<T>::get(&id).ok_or(Error::<T>::NotFound)?;
			Self::do_cancel_retry(origin.caller(), task)?;
			Self::deposit_event(Event::RetryCancelled { task, id: Some(id) });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Convert a timestamp in milliseconds to a minute key (timestamp / 60_000).
	fn timestamp_to_minute(timestamp: MomentFor<T>) -> MomentFor<T> {
		timestamp / 60_000u32.into()
	}

	/// Place a task in the agenda and update lookup if named.
	fn place_task(
		when: MomentFor<T>,
		what: ScheduledOf<T>,
	) -> Result<TaskAddress<MomentFor<T>>, (DispatchError, ScheduledOf<T>)> {
		let maybe_name = what.maybe_id;
		let minute = Self::timestamp_to_minute(when);
		let index = Self::push_to_agenda(minute, what)?;
		let address = (minute, index);
		if let Some(name) = maybe_name {
			Lookup::<T>::insert(name, address);
		}
		Self::deposit_event(Event::Scheduled { when: minute, index });
		Ok(address)
	}

	/// Push a task to the agenda for a given minute.
	fn push_to_agenda(
		when: MomentFor<T>,
		what: ScheduledOf<T>,
	) -> Result<u32, (DispatchError, ScheduledOf<T>)> {
		let mut agenda = Agenda::<T>::get(when);
		let index = if (agenda.len() as u32) < T::MaxScheduledPerMinute::get() {
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
		Agenda::<T>::insert(when, agenda);
		Ok(index)
	}

	/// Remove trailing `None` items of an agenda at `when`. If all items are `None` remove
	/// the agenda record entirely.
	fn cleanup_agenda(when: MomentFor<T>) {
		let mut agenda = Agenda::<T>::get(when);
		match agenda.iter().rposition(|i| i.is_some()) {
			Some(i) if agenda.len() > i + 1 => {
				agenda.truncate(i + 1);
				Agenda::<T>::insert(when, agenda);
			},
			Some(_) => {},
			None => {
				Agenda::<T>::remove(when);
			},
		}
	}

	/// Schedule a task.
	fn do_schedule(
		when: DispatchTime<MomentFor<T>>,
		maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
		priority: schedule::Priority,
		origin: T::PalletsOrigin,
		call: BoundedCallOf<T>,
	) -> Result<TaskAddress<MomentFor<T>>, DispatchError> {
		let now = T::TimestampProvider::now();
		let when = when.evaluate(now);

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

		let res = Self::place_task(when, task).map_err(|x| x.0)?;

		if let Some(hash) = lookup_hash {
			// Request the call to be made available.
			T::Preimages::request(&hash);
		}

		Ok(res)
	}

	/// Cancel a task by address.
	fn do_cancel(
		origin: Option<T::PalletsOrigin>,
		(when, index): TaskAddress<MomentFor<T>>,
	) -> Result<(), DispatchError> {
		let scheduled = Agenda::<T>::try_mutate(when, |agenda| {
			agenda.get_mut(index as usize).map_or(
				Ok(None),
				|s| -> Result<Option<ScheduledOf<T>>, DispatchError> {
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
				Lookup::<T>::remove(id);
			}
			Self::cleanup_agenda(when);
			Self::deposit_event(Event::Canceled { when, index });
			Ok(())
		} else {
			Err(Error::<T>::NotFound.into())
		}
	}

	/// Reschedule a task by address.
	fn do_reschedule(
		(when, index): TaskAddress<MomentFor<T>>,
		new_time: DispatchTime<MomentFor<T>>,
	) -> Result<TaskAddress<MomentFor<T>>, DispatchError> {
		let now = T::TimestampProvider::now();
		let new_time = new_time.evaluate(now);

		if new_time <= now {
			return Err(Error::<T>::TargetTimestampInPast.into());
		}

		let task = Agenda::<T>::try_mutate(
			when,
			|agenda| -> Result<ScheduledOf<T>, DispatchError> {
				let task = agenda
					.get_mut(index as usize)
					.ok_or(Error::<T>::NotFound)?
					.take()
					.ok_or(Error::<T>::NotFound)?;
				Ok(task)
			},
		)?;

		Self::cleanup_agenda(when);
		Self::deposit_event(Event::Canceled { when, index });

		Self::place_task(new_time, task).map_err(|x| x.0)
	}

	/// Schedule a named task.
	fn do_schedule_named(
		id: TaskName,
		when: DispatchTime<MomentFor<T>>,
		maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
		priority: schedule::Priority,
		origin: T::PalletsOrigin,
		call: BoundedCallOf<T>,
	) -> Result<TaskAddress<MomentFor<T>>, DispatchError> {
		// Ensure id is unique
		if Lookup::<T>::contains_key(&id) {
			return Err(Error::<T>::FailedToSchedule.into());
		}

		let now = T::TimestampProvider::now();
		let when = when.evaluate(now);

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

		let res = Self::place_task(when, task).map_err(|x| x.0)?;

		if let Some(hash) = lookup_hash {
			T::Preimages::request(&hash);
		}

		Ok(res)
	}

	/// Cancel a named task.
	fn do_cancel_named(origin: Option<T::PalletsOrigin>, id: TaskName) -> DispatchResult {
		Lookup::<T>::try_mutate_exists(id, |lookup| -> DispatchResult {
			if let Some((when, index)) = lookup.take() {
				let i = index as usize;
				Agenda::<T>::try_mutate(when, |agenda| -> DispatchResult {
					if let Some(s) = agenda.get_mut(i) {
						if let (Some(ref o), Some(ref s)) = (origin, s.borrow()) {
							Self::ensure_privilege(o, &s.origin)?;
							T::Preimages::drop(&s.call);
						}
						*s = None;
					}
					Ok(())
				})?;
				Self::cleanup_agenda(when);
				Self::deposit_event(Event::Canceled { when, index });
				Ok(())
			} else {
				Err(Error::<T>::NotFound.into())
			}
		})
	}

	/// Reschedule a named task.
	fn do_reschedule_named(
		id: TaskName,
		when: DispatchTime<MomentFor<T>>,
	) -> Result<TaskAddress<MomentFor<T>>, DispatchError> {
		let (minute, index) = Lookup::<T>::get(&id).ok_or(Error::<T>::NotFound)?;
		Self::do_reschedule((minute, index), when)
	}

	/// Cancel the retry configuration for a task.
	fn do_cancel_retry(
		origin: &T::PalletsOrigin,
		(when, index): TaskAddress<MomentFor<T>>,
	) -> Result<(), DispatchError> {
		let agenda = Agenda::<T>::get(when);
		let scheduled = agenda
			.get(index as usize)
			.and_then(Option::as_ref)
			.ok_or(Error::<T>::NotFound)?;
		Self::ensure_privilege(origin, &scheduled.origin)?;
		Retries::<T>::remove((when, index));
		Ok(())
	}

	/// Schedule a retry for a task that failed.
	fn schedule_retry(
		weight: &mut WeightMeter,
		now: MomentFor<T>,
		when: MomentFor<T>,
		agenda_index: u32,
		task: &ScheduledOf<T>,
		retry_config: RetryConfig<MomentFor<T>>,
	) {
		if weight
			.try_consume(T::WeightInfo::schedule_retry(T::MaxScheduledPerMinute::get()))
			.is_err()
		{
			Self::deposit_event(Event::RetryFailed {
				task: (when, agenda_index),
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
		match Self::place_task(wake, task.as_retry()) {
			Ok(address) => {
				// Reinsert the retry config to the new address of the task after it was placed.
				Retries::<T>::insert(address, RetryConfig { total_retries, remaining, period });
			},
			Err((_, task)) => {
				T::Preimages::drop(&task.call);
				Self::deposit_event(Event::RetryFailed {
					task: (when, agenda_index),
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

	/// Service agendas starting from the earliest incomplete minute.
	fn service_agendas(weight: &mut WeightMeter, now: MomentFor<T>, max: u32) {
		if weight.try_consume(T::WeightInfo::service_agendas_base()).is_err() {
			return;
		}

		let now_minute = Self::timestamp_to_minute(now);
		let mut incomplete_since = now_minute + One::one();
		let mut minute = IncompleteSince::<T>::take().unwrap_or(now_minute);
		let mut is_first = true;

		let max_items = T::MaxScheduledPerMinute::get();
		let mut count_down = max;
		let service_agenda_base_weight = T::WeightInfo::service_agenda_base(max_items);
		while count_down > 0
			&& minute <= now_minute
			&& weight.can_consume(service_agenda_base_weight)
		{
			if !Self::service_agenda(weight, is_first, now, minute, u32::MAX) {
				incomplete_since = incomplete_since.min(minute);
			}
			is_first = false;
			minute = minute.saturating_add(One::one());
			count_down = count_down.saturating_sub(1);
		}
		incomplete_since = incomplete_since.min(minute);
		if incomplete_since <= now_minute {
			Self::deposit_event(Event::AgendaIncomplete { when: incomplete_since });
			IncompleteSince::<T>::put(incomplete_since);
		} else {
			// Start from the next minute on the next iteration
			IncompleteSince::<T>::put(now_minute + One::one());
		}
	}

	/// Service a single agenda for the given minute.
	/// Returns `true` if the agenda was fully completed.
	fn service_agenda(
		weight: &mut WeightMeter,
		mut is_first: bool,
		now: MomentFor<T>,
		minute: MomentFor<T>,
		max: u32,
	) -> bool {
		let mut agenda = Agenda::<T>::get(minute);
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
				Self::service_task(weight, now, minute, agenda_index, is_first, task);
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
			Agenda::<T>::insert(minute, agenda);
		} else {
			Agenda::<T>::remove(minute);
		}

		postponed == 0
	}

	/// Service a single task.
	fn service_task(
		weight: &mut WeightMeter,
		now: MomentFor<T>,
		minute: MomentFor<T>,
		agenda_index: u32,
		is_first: bool,
		mut task: ScheduledOf<T>,
	) -> Result<(), (ServiceTaskError, Option<ScheduledOf<T>>)> {
		if let Some(ref id) = task.maybe_id {
			Lookup::<T>::remove(id);
		}

		let (call, lookup_len) = match T::Preimages::peek(&task.call) {
			Ok(c) => c,
			Err(_) => {
				Self::deposit_event(Event::CallUnavailable {
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
				Self::deposit_event(Event::PermanentlyOverweight {
					task: (minute, agenda_index),
					id: task.maybe_id,
				});
				Err((Unavailable, Some(task)))
			},
			Err(()) => Err((Overweight, Some(task))),
			Ok(result) => {
				let failed = result.is_err();
				let maybe_retry_config = Retries::<T>::take((minute, agenda_index));
				Self::deposit_event(Event::Dispatched {
					task: (minute, agenda_index),
					id: task.maybe_id,
					result,
				});

				match maybe_retry_config {
					Some(retry_config) if failed => {
						Self::schedule_retry(
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
					match Self::place_task(wake, task) {
						Ok(new_address) => {
							if let Some(retry_config) = maybe_retry_config {
								Retries::<T>::insert(new_address, retry_config);
							}
						},
						Err((_, task)) => {
							T::Preimages::drop(&task.call);
							Self::deposit_event(Event::PeriodicFailed {
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

impl<T: Config> time_schedule::v1::Anon<MomentFor<T>, <T as Config>::RuntimeCall, T::PalletsOrigin>
	for Pallet<T>
{
	type Address = TaskAddress<MomentFor<T>>;
	type Hasher = <T as frame_system::Config>::Hashing;

	fn schedule(
		when: DispatchTime<MomentFor<T>>,
		maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
		priority: time_schedule::Priority,
		origin: T::PalletsOrigin,
		call: Bounded<<T as Config>::RuntimeCall, Self::Hasher>,
	) -> Result<Self::Address, DispatchError> {
		Self::do_schedule(when, maybe_periodic, priority, origin, call)
	}

	fn cancel(address: Self::Address) -> Result<(), DispatchError> {
		Self::do_cancel(None, address)
	}

	fn reschedule(
		address: Self::Address,
		when: DispatchTime<MomentFor<T>>,
	) -> Result<Self::Address, DispatchError> {
		Self::do_reschedule(address, when)
	}

	fn next_dispatch_time(address: Self::Address) -> Result<MomentFor<T>, DispatchError> {
		let (minute, index) = address;
		let agenda = Agenda::<T>::get(minute);
		agenda
			.get(index as usize)
			.and_then(Option::as_ref)
			.map(|_| minute * 60_000u32.into())
			.ok_or(Error::<T>::NotFound.into())
	}
}

impl<T: Config> time_schedule::v1::Named<MomentFor<T>, <T as Config>::RuntimeCall, T::PalletsOrigin>
	for Pallet<T>
{
	type Address = TaskAddress<MomentFor<T>>;
	type Hasher = <T as frame_system::Config>::Hashing;

	fn schedule_named(
		id: TaskName,
		when: DispatchTime<MomentFor<T>>,
		maybe_periodic: Option<time_schedule::Period<MomentFor<T>>>,
		priority: time_schedule::Priority,
		origin: T::PalletsOrigin,
		call: Bounded<<T as Config>::RuntimeCall, Self::Hasher>,
	) -> Result<Self::Address, DispatchError> {
		Self::do_schedule_named(id, when, maybe_periodic, priority, origin, call)
	}

	fn cancel_named(id: TaskName) -> Result<(), DispatchError> {
		Self::do_cancel_named(None, id)
	}

	fn reschedule_named(
		id: TaskName,
		when: DispatchTime<MomentFor<T>>,
	) -> Result<Self::Address, DispatchError> {
		Self::do_reschedule_named(id, when)
	}

	fn next_dispatch_time(id: TaskName) -> Result<MomentFor<T>, DispatchError> {
		let (minute, _index) = Lookup::<T>::get(&id).ok_or(Error::<T>::NotFound)?;
		Ok(minute * 60_000u32.into())
	}
}
