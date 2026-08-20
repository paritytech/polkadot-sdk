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

//! # TimeScheduler test environment.

use super::*;

use crate as scheduler;
use frame_support::{
	derive_impl, ord_parameter_types, parameter_types,
	traits::{ConstU32, Contains, EitherOfDiverse, EqualPrivilegeOnly},
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use sp_runtime::{BuildStorage, Perbill};
use sp_weights::constants::WEIGHT_REF_TIME_PER_SECOND;

// Logger module to track execution.
#[frame_support::pallet]
pub mod logger {
	use super::{OriginCaller, OriginTrait};
	use frame_support::{pallet_prelude::*, parameter_types};
	use frame_system::pallet_prelude::*;

	parameter_types! {
		static Log: Vec<(OriginCaller, u32)> = Vec::new();
		// Time-based threshold (start_ms, end_ms) for timed_log
		static TimeThreshold: Option<(u64, u64)> = None;
	}
	pub fn log() -> Vec<(OriginCaller, u32)> {
		Log::get().clone()
	}

	pub fn clear_log() {
		Log::take();
	}

	pub fn set_time_threshold(start: u64, end: u64) {
		TimeThreshold::set(Some((start, end)));
	}

	pub fn clear_time_threshold() {
		TimeThreshold::set(None);
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type Threshold<T: Config> = StorageValue<_, (BlockNumberFor<T>, BlockNumberFor<T>)>;

	#[pallet::error]
	pub enum Error<T> {
		/// Under the threshold.
		TooEarly,
		/// Over the threshold.
		TooLate,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_timestamp::Config<Moment = u64> {
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Logged(u32, Weight),
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		<T as frame_system::Config>::RuntimeOrigin: OriginTrait<PalletsOrigin = OriginCaller>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(*weight)]
		pub fn log(origin: OriginFor<T>, i: u32, weight: Weight) -> DispatchResult {
			Self::deposit_event(Event::Logged(i, weight));
			Log::mutate(|log| {
				log.push((origin.caller().clone(), i));
			});
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(*weight)]
		pub fn log_without_filter(origin: OriginFor<T>, i: u32, weight: Weight) -> DispatchResult {
			Self::deposit_event(Event::Logged(i, weight));
			Log::mutate(|log| {
				log.push((origin.caller().clone(), i));
			});
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(*weight)]
		pub fn timed_log(origin: OriginFor<T>, i: u32, weight: Weight) -> DispatchResult {
			// Use timestamp-based threshold for time-based scheduler
			if let Some((start, end)) = TimeThreshold::get() {
				let now = pallet_timestamp::Pallet::<T>::get();
				ensure!(now >= start, Error::<T>::TooEarly);
				ensure!(now <= end, Error::<T>::TooLate);
			}
			Self::deposit_event(Event::Logged(i, weight));
			Log::mutate(|log| {
				log.push((origin.caller().clone(), i));
			});
			Ok(())
		}
	}
}

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Logger: logger,
		TimeScheduler: scheduler,
		Preimage: pallet_preimage,
		Timestamp: pallet_timestamp,
	}
);

// TimeScheduler must dispatch with root and no filter, this tests base filter is indeed not used.
pub struct BaseFilter;
impl Contains<RuntimeCall> for BaseFilter {
	fn contains(call: &RuntimeCall) -> bool {
		!matches!(call, RuntimeCall::Logger(LoggerCall::log { .. }))
	}
}

parameter_types! {
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(
			Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND * 2, u64::MAX),
		);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
	type BaseCallFilter = BaseFilter;
	type Block = Block;
	type BlockWeights = BlockWeights;
}
impl logger::Config for Test {
	type RuntimeEvent = RuntimeEvent;
}
ord_parameter_types! {
	pub const One: u64 = 1;
}

impl pallet_preimage::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = ();
	type ManagerOrigin = EnsureRoot<u64>;
	type Consideration = ();
}

parameter_types! {
	pub const MinimumPeriod: u64 = 5;
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

pub struct TestWeightInfo;
impl WeightInfo for TestWeightInfo {
	fn service_agendas_base() -> Weight {
		Weight::from_parts(0b0000_0001, 0)
	}
	fn service_agenda_base(i: u32) -> Weight {
		Weight::from_parts((i << 8) as u64 + 0b0000_0010, 0)
	}
	fn service_task_base() -> Weight {
		Weight::from_parts(0b0000_0100, 0)
	}
	fn service_task_periodic() -> Weight {
		Weight::from_parts(0b0000_1100, 0)
	}
	fn service_task_named() -> Weight {
		Weight::from_parts(0b0001_0100, 0)
	}
	fn service_task_fetched(s: u32) -> Weight {
		Weight::from_parts((s << 8) as u64 + 0b0010_0100, 0)
	}
	fn execute_dispatch_signed() -> Weight {
		Weight::from_parts(0b0100_0000, 0)
	}
	fn execute_dispatch_unsigned() -> Weight {
		Weight::from_parts(0b1000_0000, 0)
	}
	fn schedule(_s: u32) -> Weight {
		Weight::from_parts(50, 0)
	}
	fn cancel(_s: u32) -> Weight {
		Weight::from_parts(50, 0)
	}
	fn schedule_named(_s: u32) -> Weight {
		Weight::from_parts(50, 0)
	}
	fn cancel_named(_s: u32) -> Weight {
		Weight::from_parts(50, 0)
	}
	fn schedule_retry_periodic(_s: u32) -> Weight {
		Weight::from_parts(100000, 0)
	}
	fn schedule_retry_same_bucket(_s: u32) -> Weight {
		Weight::from_parts(200000, 0)
	}
	fn schedule_retry_exponential_backoff(_s: u32) -> Weight {
		Weight::from_parts(100000, 0)
	}
	fn set_retry() -> Weight {
		Weight::from_parts(50, 0)
	}
	fn set_retry_named() -> Weight {
		Weight::from_parts(50, 0)
	}
	fn cancel_retry() -> Weight {
		Weight::from_parts(50, 0)
	}
	fn cancel_retry_named() -> Weight {
		Weight::from_parts(50, 0)
	}
}
parameter_types! {
	pub storage MaximumSchedulerWeight: Weight = Perbill::from_percent(80) *
		BlockWeights::get().max_block;
	/// Bucket resolution in milliseconds - default 60 seconds (1 minute)
	/// Can be overridden per-test using BucketResolution::set()
	pub storage BucketResolution: u32 = 60_000;
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaximumSchedulerWeight;
	type ScheduleOrigin = EitherOfDiverse<EnsureRoot<u64>, EnsureSignedBy<One, u64>>;
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type BucketResolution = BucketResolution;
	type MaxScheduledPerBucket = ConstU32<100>;
	type WeightInfo = TestWeightInfo;
	type Preimages = Preimage;
	type TimestampProvider = Timestamp;
}

pub type LoggerCall = logger::Call<Test>;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| {
		// Set a default initial timestamp so tests don't need to set it manually.
		// Tests that need a different starting time can override with Timestamp::set_timestamp().
		Timestamp::set_timestamp(60_000);
	});
	ext
}

pub fn root() -> OriginCaller {
	system::RawOrigin::Root.into()
}

// Advance timestamp to the given time and run on_initialize for the next block.
// This simulates production behavior where:
// 1. Block N's on_initialize runs (sees block N-1's timestamp)
// 2. Block N's timestamp inherent is applied
// 3. Block N's on_finalize runs (sees block N's timestamp)
// 4. Block N+1's on_initialize runs (sees block N's timestamp)
pub fn run_to_time(time_ms: u64) {
	use frame_support::traits::{OnFinalize, OnInitialize};

	let current_block = System::block_number();

	// Set timestamp for current block (simulating inherent applied after on_initialize)
	Timestamp::set_timestamp(time_ms);

	// Finalize current block (sees current timestamp)
	<AllPalletsWithSystem as OnFinalize<u64>>::on_finalize(current_block);

	// Move to next block
	let next_block = current_block + 1;
	System::set_block_number(next_block);
	System::reset_events();

	// Initialize next block - all pallets see the timestamp we just set
	<AllPalletsWithSystem as OnInitialize<u64>>::on_initialize(next_block);
}
