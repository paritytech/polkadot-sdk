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
#![doc = docify::embed!("src/tests.rs", basic_scheduling_works)]
//!
//! 2. Scheduling a preimage hash of a runtime call at a specific time.
#![doc = docify::embed!("src/tests.rs", scheduling_with_preimages_works)]

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
//!
//! ## Bucket Resolution
//!
//! Tasks are organized into time buckets for efficient storage and processing. The bucket size
//! is configured via [`Config::BucketResolution`] (typically 60 seconds / 1 minute).
//!
//! **Important:** All timestamps are rounded down to the nearest bucket boundary:
//! - A task scheduled at timestamp `125_000ms` with a 60-second bucket resolution will be
//!   placed in bucket 2 (`125_000 / 60_000 = 2`), which covers `120_000ms` to `179_999ms`.
//! - Periodic task durations are also converted to bucket counts. A period of `180_000ms`
//!   (3 minutes) becomes a period of 3 buckets.
//!
//! This means:
//! - Tasks cannot be scheduled with finer granularity than the bucket resolution.
//! - Periodic durations must be at least one bucket (`>= BucketResolution`).
//! - Retry durations follow the same rules. If `try_same_bucket_first` is enabled,
//!   retries first attempt the current bucket before advancing by the retry duration.

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

use alloc::{boxed::Box, collections::BTreeSet, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::{borrow::Borrow, cmp::Ordering, marker::PhantomData};
use frame_support::{
	dispatch::{DispatchResult, GetDispatchInfo, Parameter, RawOrigin},
	ensure,
	pallet_prelude::BoundedVec,
	traits::{
		schedule,
		time_schedule::{self, DispatchTime},
		Bounded, CallerTrait, EnsureOrigin, Get, IsType, OriginTrait, PrivilegeCmp, QueryPreimage,
		StorageVersion, StorePreimage, Time,
	},
	weights::{Weight, WeightMeter},
};
use frame_system::{self as system};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{BadOrigin, Dispatchable, One, Saturating},
	DispatchError, RuntimeDebug,
};

pub use pallet::*;
pub use weights::WeightInfo;

/// Absolute timestamp in milliseconds from the timestamp provider.
pub type TimeFor<T> = <<T as Config>::TimestampProvider as Time>::Moment;
/// Bucket index (`timestamp / BucketResolution`).
pub type BucketFor<T> = TimeFor<T>;
/// Task location: `(bucket_index, position_in_bucket)`.
pub type TaskAddress<Bucket> = (Bucket, u32);
/// Bounded call type for scheduled tasks.
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
	/// Number of buckets to advance between retry attempts.
	/// See [`Period`] for details on the bucket-based retry semantics.
	pub period: Period,
	/// If true, always try the same bucket first before advancing by `period`.
	/// This allows retrying within the same bucket on first failure, then
	/// using `period` for subsequent retries if the same bucket is full.
	pub try_same_bucket_first: bool,
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
pub struct Scheduled<Name, Call, Interval, PalletsOrigin, AccountId> {
	/// The unique identity for this task, if there is one.
	pub maybe_id: Option<Name>,
	/// This task's priority.
	pub priority: schedule::Priority,
	/// The call to be dispatched.
	pub call: Call,
	/// If the call is periodic, then this points to the information concerning that.
	pub maybe_periodic: Option<time_schedule::Period<Interval>>,
	/// The origin with which to dispatch the call.
	pub origin: PalletsOrigin,
	#[doc(hidden)]
	pub _phantom: PhantomData<AccountId>,
}

impl<Name, Call, Interval, PalletsOrigin, AccountId>
	Scheduled<Name, Call, Interval, PalletsOrigin, AccountId>
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
pub type ScheduledOf<T> = Scheduled<
	TaskName,
	BoundedCallOf<T>,
	BucketFor<T>,
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

		/// The resolution of scheduling buckets in milliseconds.
		///
		/// Tasks are grouped into time buckets based on this resolution. For example:
		/// - 60_000 (1 minute): Tasks are grouped by minute
		/// - 3_600_000 (1 hour): Tasks are grouped by hour
		/// - 10_000 (10 seconds): Tasks are grouped by 10-second intervals
		///
		/// This value should be greater than or equal to the expected block time to ensure
		/// meaningful batching of tasks. Smaller values provide finer scheduling granularity
		/// but create more storage entries.
		#[pallet::constant]
		type BucketResolution: Get<u32>;

		/// The maximum number of scheduled calls in the queue for a single time bucket.
		///
		/// NOTE:
		/// + Dependent pallets' benchmarks might require a higher limit for the setting. Set a
		/// higher limit under `runtime-benchmarks` feature.
		#[pallet::constant]
		type MaxScheduledPerBucket: Get<u32>;

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

	/// Time bucket at which the agenda began incomplete execution.
	#[pallet::storage]
	pub type IncompleteSince<T: Config> = StorageValue<_, BucketFor<T>>;

	/// Items to be executed, indexed by time bucket.
	///
	/// Each bucket contains a BoundedVec of optional scheduled tasks. Tasks are stored
	/// as `Option<ScheduledOf<T>>` to allow O(1) cancellation by setting to `None`.
	/// The index in the vec is the task's index for addressing purposes.
	#[pallet::storage]
	pub type Agenda<T: Config> = StorageMap<
		_,
		Twox64Concat,
		BucketFor<T>,
		BoundedVec<Option<ScheduledOf<T>>, T::MaxScheduledPerBucket>,
		ValueQuery,
	>;

	/// Retry configurations for tasks, indexed by task address (bucket, index).
	#[pallet::storage]
	pub type Retries<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		TaskAddress<BucketFor<T>>,
		RetryConfig<BucketFor<T>>,
		OptionQuery,
	>;

	/// Lookup from a name to the bucket and index of the task.
	#[pallet::storage]
	pub type Lookup<T: Config> = StorageMap<_, Twox64Concat, TaskName, TaskAddress<BucketFor<T>>>;

	/// Events type.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Scheduled some task.
		Scheduled { when: BucketFor<T>, index: u32 },
		/// Canceled some task.
		Canceled { when: BucketFor<T>, index: u32 },
		/// Dispatched some task.
		Dispatched { task: TaskAddress<BucketFor<T>>, id: Option<TaskName>, result: DispatchResult },
		/// Set a retry configuration for some task.
		///
		/// `period` is the number of buckets to advance between retries (not a time duration).
		/// The actual retry delay is `period * BucketResolution`.
		/// If `try_same_bucket_first` is true, retries will first attempt the same bucket.
		RetrySet {
			task: TaskAddress<BucketFor<T>>,
			id: Option<TaskName>,
			period: BucketFor<T>,
			retries: u8,
			try_same_bucket_first: bool,
		},
		/// Cancel a retry configuration for some task.
		RetryCancelled { task: TaskAddress<BucketFor<T>>, id: Option<TaskName> },
		/// The call for the provided hash was not found so the task has been aborted.
		CallUnavailable { task: TaskAddress<BucketFor<T>>, id: Option<TaskName> },
		/// The given task was unable to be renewed since the agenda is full at that bucket.
		PeriodicFailed { task: TaskAddress<BucketFor<T>>, id: Option<TaskName> },
		/// The given task was unable to be retried since the agenda is full at that bucket or there
		/// was not enough weight to reschedule it.
		RetryFailed { task: TaskAddress<BucketFor<T>>, id: Option<TaskName> },
		/// The given task can never be executed since it is overweight.
		PermanentlyOverweight { task: TaskAddress<BucketFor<T>>, id: Option<TaskName> },
		/// Agenda is incomplete from `when`.
		AgendaIncomplete { when: BucketFor<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Failed to schedule a call
		FailedToSchedule,
		/// Cannot find the scheduled call.
		NotFound,
		/// Given target timestamp is in the past.
		TargetTimestampInPast,
		/// Reschedule failed because it does not change scheduled bucket.
		RescheduleNoChange,
		/// Attempt to use a non-named function on a named task.
		Named,
		/// Duration must be at least BucketResolution.
		DurationTooSmall,
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
					+ T::WeightInfo::service_agenda_base(T::MaxScheduledPerBucket::get())
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
		#[pallet::weight(<T as Config>::WeightInfo::schedule(T::MaxScheduledPerBucket::get()))]
		pub fn schedule(
			origin: OriginFor<T>,
			when: TimeFor<T>,
			maybe_periodic: Option<time_schedule::Period<TimeFor<T>>>,
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
		#[pallet::weight(<T as Config>::WeightInfo::cancel(T::MaxScheduledPerBucket::get()))]
		pub fn cancel(origin: OriginFor<T>, when: TimeFor<T>, index: u32) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_cancel(Some(origin.caller().clone()), (when, index))?;
			Ok(())
		}

		/// Schedule a named task at a specific timestamp (in milliseconds since Unix epoch).
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_named(T::MaxScheduledPerBucket::get()))]
		pub fn schedule_named(
			origin: OriginFor<T>,
			id: TaskName,
			when: TimeFor<T>,
			maybe_periodic: Option<time_schedule::Period<TimeFor<T>>>,
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
		#[pallet::weight(<T as Config>::WeightInfo::cancel_named(T::MaxScheduledPerBucket::get()))]
		pub fn cancel_named(origin: OriginFor<T>, id: TaskName) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			Self::do_cancel_named(Some(origin.caller().clone()), id)?;
			Ok(())
		}

		/// Anonymously schedule a task after a delay (in milliseconds).
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule(T::MaxScheduledPerBucket::get()))]
		pub fn schedule_after(
			origin: OriginFor<T>,
			after: TimeFor<T>,
			maybe_periodic: Option<time_schedule::Period<TimeFor<T>>>,
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
		#[pallet::weight(<T as Config>::WeightInfo::schedule_named(T::MaxScheduledPerBucket::get()))]
		pub fn schedule_named_after(
			origin: OriginFor<T>,
			id: TaskName,
			after: TimeFor<T>,
			maybe_periodic: Option<time_schedule::Period<TimeFor<T>>>,
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

		/// Set a retry configuration for a task. On failure, retries up to `retries` times
		/// with `duration` ms between attempts. Duration must be >= `BucketResolution`.
		///
		/// If `try_same_bucket_first` is true, retries first attempt the current bucket before
		/// falling back to `duration` buckets ahead.
		///
		/// Tasks which need to be scheduled for a retry are still subject to weight metering and
		/// agenda space, same as a regular task. If a periodic task fails, it will be scheduled
		/// normally while the task is retrying.
		///
		/// Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic
		/// clones of the original task. Their retry configuration will be derived from the
		/// original task's configuration, but will have a lower value for `remaining` than the
		/// original `total_retries`.
		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::set_retry())]
		pub fn set_retry(
			origin: OriginFor<T>,
			task: TaskAddress<BucketFor<T>>,
			retries: u8,
			duration: TimeFor<T>,
			try_same_bucket_first: bool,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();
			ensure!(duration >= bucket_resolution, Error::<T>::DurationTooSmall);
			let period = duration / bucket_resolution;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			let (bucket, index) = task;
			let agenda = Agenda::<T>::get(bucket);
			let scheduled = agenda
				.get(index as usize)
				.and_then(Option::as_ref)
				.ok_or(Error::<T>::NotFound)?;
			Self::ensure_privilege(origin.caller(), &scheduled.origin)?;
			Retries::<T>::insert(
				task,
				RetryConfig {
					total_retries: retries,
					remaining: retries,
					period,
					try_same_bucket_first,
				},
			);
			Self::deposit_event(Event::RetrySet {
				task,
				id: None,
				period,
				retries,
				try_same_bucket_first,
			});
			Ok(())
		}

		/// Set a retry configuration for a named task. On failure, retries up to `retries` times
		/// with `duration` ms between attempts. Duration must be >= `BucketResolution`.
		///
		/// If `try_same_bucket_first` is true, retries first attempt the current bucket before
		/// falling back to `duration` buckets ahead.
		///
		/// Tasks which need to be scheduled for a retry are still subject to weight metering and
		/// agenda space, same as a regular task. If a periodic task fails, it will be scheduled
		/// normally while the task is retrying.
		///
		/// Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic
		/// clones of the original task. Their retry configuration will be derived from the
		/// original task's configuration, but will have a lower value for `remaining` than the
		/// original `total_retries`.
		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::set_retry_named())]
		pub fn set_retry_named(
			origin: OriginFor<T>,
			id: TaskName,
			retries: u8,
			duration: TimeFor<T>,
			try_same_bucket_first: bool,
		) -> DispatchResult {
			T::ScheduleOrigin::ensure_origin(origin.clone())?;
			let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();
			ensure!(duration >= bucket_resolution, Error::<T>::DurationTooSmall);
			let period = duration / bucket_resolution;
			let origin = <T as Config>::RuntimeOrigin::from(origin);
			let (bucket, index) = Lookup::<T>::get(&id).ok_or(Error::<T>::NotFound)?;
			let agenda = Agenda::<T>::get(bucket);
			let scheduled = agenda
				.get(index as usize)
				.and_then(Option::as_ref)
				.ok_or(Error::<T>::NotFound)?;
			Self::ensure_privilege(origin.caller(), &scheduled.origin)?;
			Retries::<T>::insert(
				(bucket, index),
				RetryConfig {
					total_retries: retries,
					remaining: retries,
					period,
					try_same_bucket_first,
				},
			);
			Self::deposit_event(Event::RetrySet {
				task: (bucket, index),
				id: Some(id),
				period,
				retries,
				try_same_bucket_first,
			});
			Ok(())
		}

		/// Removes the retry configuration of a task.
		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_retry())]
		pub fn cancel_retry(
			origin: OriginFor<T>,
			task: TaskAddress<BucketFor<T>>,
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
	/// Convert a timestamp in milliseconds to a bucket key (timestamp / BucketResolution).
	fn timestamp_to_bucket(timestamp: TimeFor<T>) -> BucketFor<T> {
		timestamp / T::BucketResolution::get().into()
	}

	fn resolve_time(when: DispatchTime<TimeFor<T>>) -> Result<TimeFor<T>, DispatchError> {
		let now = T::TimestampProvider::now();
		let when = when.evaluate(now);

		// Allow scheduling at the current timestamp or in the future.
		// Tasks in the current bucket will be processed in this or a subsequent block.
		if when < now {
			return Err(Error::<T>::TargetTimestampInPast.into());
		}

		Ok(when)
	}

	/// Place a task in the agenda and update lookup if named.
	fn place_task(
		when: TimeFor<T>,
		what: ScheduledOf<T>,
	) -> Result<TaskAddress<BucketFor<T>>, (DispatchError, ScheduledOf<T>)> {
		let maybe_name = what.maybe_id;
		let bucket = Self::timestamp_to_bucket(when);
		let index = Self::push_to_agenda(bucket, what)?;
		let address = (bucket, index);
		if let Some(name) = maybe_name {
			Lookup::<T>::insert(name, address)
		}
		Self::deposit_event(Event::Scheduled { when: bucket, index });
		Ok(address)
	}

	/// Push a task to the agenda for a given bucket.
	fn push_to_agenda(
		bucket: BucketFor<T>,
		what: ScheduledOf<T>,
	) -> Result<u32, (DispatchError, ScheduledOf<T>)> {
		let mut agenda = Agenda::<T>::get(bucket);
		let index = if (agenda.len() as u32) < T::MaxScheduledPerBucket::get() {
			// will always succeed due to the above check.
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
		Agenda::<T>::insert(bucket, agenda);
		Ok(index)
	}

	/// Remove trailing `None` items of an agenda at `bucket`. If all items are `None` remove
	/// the agenda record entirely.
	fn cleanup_agenda(bucket: BucketFor<T>) {
		let mut agenda = Agenda::<T>::get(bucket);
		match agenda.iter().rposition(|i| i.is_some()) {
			Some(i) if agenda.len() > i + 1 => {
				agenda.truncate(i + 1);
				Agenda::<T>::insert(bucket, agenda);
			},
			Some(_) => {},
			None => {
				Agenda::<T>::remove(bucket);
			},
		}
	}

	/// Schedule a task. Periodic durations are converted to bucket counts.
	fn do_schedule(
		when: DispatchTime<TimeFor<T>>,
		maybe_periodic: Option<time_schedule::Period<TimeFor<T>>>,
		priority: schedule::Priority,
		origin: T::PalletsOrigin,
		call: BoundedCallOf<T>,
	) -> Result<TaskAddress<BucketFor<T>>, DispatchError> {
		let when = Self::resolve_time(when)?;

		let lookup_hash = call.lookup_hash();
		let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();

		// Convert duration-based period to bucket-based period.
		// Duration must be >= BucketResolution to ensure at least one bucket between executions.
		let maybe_periodic = maybe_periodic
			.filter(|p| p.1 > 1)
			.map(|(duration, count)| -> Result<_, DispatchError> {
				ensure!(duration >= bucket_resolution, Error::<T>::DurationTooSmall);
				// Convert duration to bucket count (rounded down).
				// Safe because we checked duration >= bucket_resolution above, so period >= 1.
				let period = duration / bucket_resolution;
				// Remove one from the number of repetitions since we will schedule one now.
				Ok((period, count - 1))
			})
			.transpose()?;

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
		(bucket, index): TaskAddress<BucketFor<T>>,
	) -> Result<(), DispatchError> {
		let scheduled = Agenda::<T>::try_mutate(bucket, |agenda| {
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
			Retries::<T>::remove((bucket, index));
			Self::cleanup_agenda(bucket);
			Self::deposit_event(Event::Canceled { when: bucket, index });
			Ok(())
		} else {
			Err(Error::<T>::NotFound.into())
		}
	}

	/// Reschedule a task by address.
	fn do_reschedule(
		(bucket, index): TaskAddress<BucketFor<T>>,
		new_time: DispatchTime<TimeFor<T>>,
	) -> Result<TaskAddress<BucketFor<T>>, DispatchError> {
		let new_time = Self::resolve_time(new_time)?;
		let new_bucket = Self::timestamp_to_bucket(new_time);

		if new_bucket == bucket {
			return Err(Error::<T>::RescheduleNoChange.into());
		}

		let task = Agenda::<T>::try_mutate(bucket, |agenda| {
			let task = agenda.get_mut(index as usize).ok_or(Error::<T>::NotFound)?;
			ensure!(!matches!(task, Some(Scheduled { maybe_id: Some(_), .. })), Error::<T>::Named);
			task.take().ok_or(Error::<T>::NotFound)
		})?;

		Self::cleanup_agenda(bucket);
		Self::deposit_event(Event::Canceled { when: bucket, index });

		Self::place_task(new_time, task).map_err(|x| x.0)
	}

	/// Schedule a named task. Periodic durations are converted to bucket counts.
	fn do_schedule_named(
		id: TaskName,
		when: DispatchTime<TimeFor<T>>,
		maybe_periodic: Option<time_schedule::Period<TimeFor<T>>>,
		priority: schedule::Priority,
		origin: T::PalletsOrigin,
		call: BoundedCallOf<T>,
	) -> Result<TaskAddress<BucketFor<T>>, DispatchError> {
		// ensure id is unique
		if Lookup::<T>::contains_key(&id) {
			return Err(Error::<T>::FailedToSchedule.into());
		}

		let when = Self::resolve_time(when)?;

		let lookup_hash = call.lookup_hash();
		let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();

		// Convert duration-based period to bucket-based period.
		// Duration must be >= BucketResolution to ensure at least one bucket between executions.
		let maybe_periodic = maybe_periodic
			.filter(|p| p.1 > 1)
			.map(|(duration, count)| -> Result<_, DispatchError> {
				ensure!(duration >= bucket_resolution, Error::<T>::DurationTooSmall);
				// Convert duration to bucket count (rounded down).
				// Safe because we checked duration >= bucket_resolution above, so period >= 1.
				let period = duration / bucket_resolution;
				// Remove one from the number of repetitions since we will schedule one now.
				Ok((period, count - 1))
			})
			.transpose()?;

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
			if let Some((bucket, index)) = lookup.take() {
				let i = index as usize;
				Agenda::<T>::try_mutate(bucket, |agenda| -> DispatchResult {
					if let Some(s) = agenda.get_mut(i) {
						if let (Some(ref o), Some(ref s)) = (origin, s.borrow()) {
							Self::ensure_privilege(o, &s.origin)?;
							Retries::<T>::remove((bucket, index));
							T::Preimages::drop(&s.call);
						}
						*s = None;
					}
					Ok(())
				})?;
				Self::cleanup_agenda(bucket);
				Self::deposit_event(Event::Canceled { when: bucket, index });
				Ok(())
			} else {
				Err(Error::<T>::NotFound.into())
			}
		})
	}

	/// Reschedule a named task.
	fn do_reschedule_named(
		id: TaskName,
		new_time: DispatchTime<TimeFor<T>>,
	) -> Result<TaskAddress<BucketFor<T>>, DispatchError> {
		let new_time = Self::resolve_time(new_time)?;
		let new_bucket = Self::timestamp_to_bucket(new_time);

		let lookup = Lookup::<T>::get(id);
		let (bucket, index) = lookup.ok_or(Error::<T>::NotFound)?;

		if new_bucket == bucket {
			return Err(Error::<T>::RescheduleNoChange.into());
		}

		let task = Agenda::<T>::try_mutate(bucket, |agenda| {
			let task = agenda.get_mut(index as usize).ok_or(Error::<T>::NotFound)?;
			task.take().ok_or(Error::<T>::NotFound)
		})?;
		Self::cleanup_agenda(bucket);
		Self::deposit_event(Event::Canceled { when: bucket, index });
		Self::place_task(new_time, task).map_err(|x| x.0)
	}

	/// Cancel the retry configuration for a task.
	fn do_cancel_retry(
		origin: &T::PalletsOrigin,
		(bucket, index): TaskAddress<BucketFor<T>>,
	) -> Result<(), DispatchError> {
		let agenda = Agenda::<T>::get(bucket);
		let scheduled = agenda
			.get(index as usize)
			.and_then(|x| x.as_ref())
			.ok_or(Error::<T>::NotFound)?;
		Self::ensure_privilege(origin, &scheduled.origin)?;
		Retries::<T>::remove((bucket, index));
		Ok(())
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
	/// Service agendas starting from the earliest incomplete bucket.
	///
	/// This function processes all pending task buckets from the stored starting point
	/// up to the current bucket. We track `IncompleteSince` which serves dual purposes:
	/// 1. The earliest bucket with incomplete tasks (if any tasks couldn't complete)
	/// 2. The next bucket to start processing from (to avoid skipping buckets when time jumps)
	///
	/// This ensures that tasks scheduled in intermediate buckets are not skipped when
	/// time advances across multiple buckets between blocks.
	fn service_agendas(weight: &mut WeightMeter, now: TimeFor<T>, max: u32) {
		if weight.try_consume(T::WeightInfo::service_agendas_base()).is_err() {
			return;
		}

		let now_bucket = Self::timestamp_to_bucket(now);
		// Start from the stored bucket or the current bucket (whichever is earlier).
		// This ensures we don't skip intermediate buckets when time jumps forward.
		let mut bucket = IncompleteSince::<T>::take()
			.map(|stored| stored.min(now_bucket))
			.unwrap_or(now_bucket);
		let mut earliest_incomplete: Option<BucketFor<T>> = None;
		let mut is_first = true;

		let max_items = T::MaxScheduledPerBucket::get();
		let mut count_down = max;
		let service_agenda_base_weight = T::WeightInfo::service_agenda_base(max_items);
		while count_down > 0
			&& bucket <= now_bucket
			&& weight.can_consume(service_agenda_base_weight)
		{
			if Agenda::<T>::contains_key(bucket) {
				if !Self::service_agenda(weight, is_first, bucket, u32::MAX) {
					// Track the earliest bucket with incomplete tasks
					if earliest_incomplete.is_none() {
						earliest_incomplete = Some(bucket);
					}
				}
				is_first = false;
			}
			bucket = bucket.saturating_add(One::one());
			count_down = count_down.saturating_sub(1);
		}

		// Determine what to store for next iteration
		if let Some(incomplete) = earliest_incomplete {
			// There were incomplete tasks - store the earliest incomplete bucket
			Self::deposit_event(Event::AgendaIncomplete { when: incomplete });
			IncompleteSince::<T>::put(incomplete);
		} else if bucket <= now_bucket {
			// We ran out of iterations before reaching now_bucket - store where we stopped
			Self::deposit_event(Event::AgendaIncomplete { when: bucket });
			IncompleteSince::<T>::put(bucket);
		} else {
			// All buckets processed successfully up to and including now_bucket.
			// Store the current bucket so that:
			// 1. New tasks scheduled in this bucket (in later blocks) get processed
			// 2. We don't re-process old buckets unnecessarily
			IncompleteSince::<T>::put(now_bucket);
		}
	}

	/// Service a single agenda for the given bucket.
	/// Returns `true` if the agenda was fully completed.
	fn service_agenda(
		weight: &mut WeightMeter,
		mut is_first: bool,
		bucket: BucketFor<T>,
		max: u32,
	) -> bool {
		let mut agenda = Agenda::<T>::get(bucket);
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

		// Items which we know can be executed and have postponed for execution in a later block.
		let mut postponed = (ordered.len() as u32).saturating_sub(max);
		// Items which we don't know can ever be executed.
		let mut dropped = 0;

		// Track indices where tasks were successfully executed.
		// These need to be cleared from storage after processing.
		let mut executed_indices = BTreeSet::new();

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

			// Execute the task - any retries scheduled to the same bucket will be
			// written to storage, we'll merge them after processing
			let result = Self::service_task(weight, bucket, agenda_index, is_first, task);
			match result {
				Err((Unavailable, _)) => dropped += 1,
				Err((Overweight, _)) => postponed += 1,
				Ok(()) => {
					is_first = false;
					executed_indices.insert(agenda_index as usize);
				},
			};
		}

		// Re-read storage which now contains original tasks + any new retries.
		// Clear out the successfully executed indices.
		let mut storage_agenda = Agenda::<T>::get(bucket);
		for index in executed_indices {
			if let Some(slot) = storage_agenda.get_mut(index) {
				*slot = None;
			}
		}

		if postponed > 0 || dropped > 0 || storage_agenda.iter().any(|x| x.is_some()) {
			Agenda::<T>::insert(bucket, storage_agenda);
		} else {
			Agenda::<T>::remove(bucket);
		}

		postponed == 0
	}

	/// Service a single task.
	fn service_task(
		weight: &mut WeightMeter,
		bucket: BucketFor<T>,
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
					task: (bucket, agenda_index),
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
					task: (bucket, agenda_index),
					id: task.maybe_id,
				});
				Err((Unavailable, Some(task)))
			},
			Err(()) => Err((Overweight, Some(task))),
			Ok(result) => {
				let failed = result.is_err();
				let maybe_retry_config = Retries::<T>::take((bucket, agenda_index));
				Self::deposit_event(Event::Dispatched {
					task: (bucket, agenda_index),
					id: task.maybe_id,
					result,
				});

				match maybe_retry_config {
					Some(retry_config) if failed => {
						Self::schedule_retry(weight, bucket, agenda_index, &task, retry_config);
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
					// Calculate target bucket and convert to timestamp for place_task.
					let target_bucket = bucket.saturating_add(period);
					let wake = target_bucket.saturating_mul(T::BucketResolution::get().into());
					match Self::place_task(wake, task) {
						Ok(new_address) => {
							if let Some(retry_config) = maybe_retry_config {
								Retries::<T>::insert(new_address, retry_config);
							}
						},
						Err((_, task)) => {
							T::Preimages::drop(&task.call);
							Self::deposit_event(Event::PeriodicFailed {
								task: (bucket, agenda_index),
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

	/// Schedule a retry for a task that failed.
	///
	/// If `try_same_bucket_first` is true, first attempts the same bucket, then falls back
	/// to `period` buckets ahead. Otherwise, directly schedules `period` buckets ahead.
	/// If the target bucket is full, falls back to the next bucket.
	fn schedule_retry(
		weight: &mut WeightMeter,
		bucket: BucketFor<T>,
		agenda_index: u32,
		task: &ScheduledOf<T>,
		retry_config: RetryConfig<BucketFor<T>>,
	) {
		let max_scheduled = T::MaxScheduledPerBucket::get();
		let retry_weight = if retry_config.try_same_bucket_first {
			T::WeightInfo::schedule_retry_try_same_bucket(max_scheduled)
		} else {
			T::WeightInfo::schedule_retry(max_scheduled)
		};
		if weight.try_consume(retry_weight).is_err() {
			Self::deposit_event(Event::RetryFailed {
				task: (bucket, agenda_index),
				id: task.maybe_id,
			});
			return;
		}

		let RetryConfig { total_retries, mut remaining, period, try_same_bucket_first } =
			retry_config;
		remaining = match remaining.checked_sub(1) {
			Some(n) => n,
			None => return,
		};

		let new_retry_config =
			RetryConfig { total_retries, remaining, period, try_same_bucket_first };

		// If try_same_bucket_first, attempt same bucket first
		if try_same_bucket_first {
			let same_bucket_time = bucket.saturating_mul(T::BucketResolution::get().into());
			if let Ok(address) = Self::place_task(same_bucket_time, task.as_retry()) {
				Retries::<T>::insert(address, new_retry_config);
				return;
			}
		}

		// Try target bucket (current + period)
		let target_bucket = bucket.saturating_add(period);
		let target_time = target_bucket.saturating_mul(T::BucketResolution::get().into());
		match Self::place_task(target_time, task.as_retry()) {
			Ok(address) => {
				Retries::<T>::insert(address, new_retry_config);
			},
			Err((_, retry_task)) => {
				// Target bucket is full, try the next bucket
				let next_bucket = target_bucket.saturating_add(1u32.into());
				let next_bucket_time =
					next_bucket.saturating_mul(T::BucketResolution::get().into());
				match Self::place_task(next_bucket_time, retry_task) {
					Ok(address) => {
						Retries::<T>::insert(address, new_retry_config);
					},
					Err((_, task)) => {
						// Next bucket also full, give up
						T::Preimages::drop(&task.call);
						Self::deposit_event(Event::RetryFailed {
							task: (bucket, agenda_index),
							id: task.maybe_id,
						});
					},
				}
			},
		}
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
}

use time_schedule::v1::TaskName;

impl<T: Config>
	time_schedule::v1::Anon<TimeFor<T>, <T as Config>::RuntimeCall, T::PalletsOrigin>
	for Pallet<T>
{
	type Address = TaskAddress<BucketFor<T>>;
	type Hasher = T::Hashing;

	fn schedule(
		when: DispatchTime<TimeFor<T>>,
		maybe_periodic: Option<time_schedule::Period<TimeFor<T>>>,
		priority: time_schedule::Priority,
		origin: T::PalletsOrigin,
		call: Bounded<<T as Config>::RuntimeCall, Self::Hasher>,
	) -> Result<Self::Address, DispatchError> {
		Self::do_schedule(when, maybe_periodic, priority, origin, call)
	}

	fn cancel(address: Self::Address) -> Result<(), DispatchError> {
		Self::do_cancel(None, address).map_err(map_err_to_v1_err::<T>)
	}

	fn reschedule(
		address: Self::Address,
		when: DispatchTime<TimeFor<T>>,
	) -> Result<Self::Address, DispatchError> {
		Self::do_reschedule(address, when).map_err(map_err_to_v1_err::<T>)
	}

	fn next_dispatch_time(address: Self::Address) -> Result<TimeFor<T>, DispatchError> {
		let (bucket, index) = address;
		let agenda = Agenda::<T>::get(bucket);
		let resolution: TimeFor<T> = T::BucketResolution::get().into();
		agenda
			.get(index as usize)
			.and_then(Option::as_ref)
			.map(|_| bucket * resolution)
			.ok_or(DispatchError::Unavailable)
	}
}

impl<T: Config>
	time_schedule::v1::Named<TimeFor<T>, <T as Config>::RuntimeCall, T::PalletsOrigin>
	for Pallet<T>
{
	type Address = TaskAddress<BucketFor<T>>;
	type Hasher = T::Hashing;

	fn schedule_named(
		id: TaskName,
		when: DispatchTime<TimeFor<T>>,
		maybe_periodic: Option<time_schedule::Period<TimeFor<T>>>,
		priority: time_schedule::Priority,
		origin: T::PalletsOrigin,
		call: Bounded<<T as Config>::RuntimeCall, Self::Hasher>,
	) -> Result<Self::Address, DispatchError> {
		Self::do_schedule_named(id, when, maybe_periodic, priority, origin, call)
	}

	fn cancel_named(id: TaskName) -> Result<(), DispatchError> {
		Self::do_cancel_named(None, id).map_err(map_err_to_v1_err::<T>)
	}

	fn reschedule_named(
		id: TaskName,
		when: DispatchTime<TimeFor<T>>,
	) -> Result<Self::Address, DispatchError> {
		Self::do_reschedule_named(id, when).map_err(map_err_to_v1_err::<T>)
	}

	fn next_dispatch_time(id: TaskName) -> Result<TimeFor<T>, DispatchError> {
		let (bucket, _index) = Lookup::<T>::get(&id).ok_or(DispatchError::Unavailable)?;
		let resolution: TimeFor<T> = T::BucketResolution::get().into();
		Ok(bucket * resolution)
	}
}

/// Maps a pallet error to a `time_schedule::v1` error.
fn map_err_to_v1_err<T: Config>(err: DispatchError) -> DispatchError {
	if err == DispatchError::from(Error::<T>::NotFound) {
		DispatchError::Unavailable
	} else {
		err
	}
}
