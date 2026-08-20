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

//! Time-Scheduler pallet benchmarking.

use alloc::vec;
use frame_benchmarking::v2::*;
use frame_support::{
	ensure,
	traits::{schedule::Priority, BoundedInline},
	weights::WeightMeter,
};
use frame_system::{EventRecord, RawOrigin};

use sp_io::hashing::blake2_256;

use crate::{
	migrations::import_from_block_scheduler::{
		self as imp, BlockFor, ImportFromBlockScheduler, MigrationAddressMap, MigrationState,
		OldScheduledOf,
	},
	*,
};
use frame_support::migrations::SteppedMigration;

type SystemCall<T> = frame_system::Call<T>;

/// Block time used by migration benchmarks (placeholder; real value comes from
/// the runtime in production).
struct BenchBlockTime;
impl Get<u64> for BenchBlockTime {
	fn get() -> u64 {
		6_000
	}
}

/// Construct a fake old block-based scheduled task for migration benchmarks.
fn make_old_scheduled<T: Config>() -> OldScheduledOf<T> {
	let call = make_call::<T>(None);
	Scheduled {
		maybe_id: None,
		priority: 0,
		call,
		maybe_periodic: None,
		origin: make_origin::<T>(false),
		_phantom: Default::default(),
	}
}

/// Insert `n` tasks into the old block-scheduler `Agenda` storage at `block`.
/// Clears any leftover state from earlier benchmark iterations so each call
/// starts from a clean slate.
fn setup_old_agenda<T: Config>(block: BlockFor<T>, n: u32) {
	let _ = imp::old::Agenda::<T>::clear(u32::MAX, None);
	let _ = crate::Agenda::<T>::clear(u32::MAX, None);
	let _ = MigrationAddressMap::<T>::clear(u32::MAX, None);
	let tasks: alloc::vec::Vec<Option<OldScheduledOf<T>>> =
		(0..n).map(|_| Some(make_old_scheduled::<T>())).collect();
	imp::old::Agenda::<T>::insert(block, tasks);
}

const SEED: u32 = 0;
const BUCKET: u32 = 2;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

/// Add `n` named periodic items to the schedule for a given bucket.
///
/// Converts the bucket to a timestamp for scheduling, but verifies
/// the agenda by bucket index. `name_offset` shifts the naming indices
/// to avoid collisions when filling multiple buckets.
fn fill_schedule<T: Config>(
	bucket: BucketFor<T>,
	n: u32,
) -> Result<(), &'static str> {
	fill_schedule_offset::<T>(bucket, n, 0)
}

fn fill_schedule_offset<T: Config>(
	bucket: BucketFor<T>,
	n: u32,
	name_offset: u32,
) -> Result<(), &'static str> {
	let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();
	let when: TimeFor<T> = bucket * bucket_resolution;
	let t = DispatchTime::At(when);
	let origin: <T as Config>::PalletsOrigin = frame_system::RawOrigin::Root.into();
	for i in 0..n {
		let call = make_call::<T>(None);
		let duration: TimeFor<T> = bucket_resolution * (i + 100).into();
		let period = Some((duration, 100));
		let name = u32_to_name(name_offset + i);
		Pallet::<T>::do_schedule_named(name, t, period, 0, origin.clone(), call)?;
	}
	ensure!(Agenda::<T>::get(bucket).len() == n as usize, "didn't fill schedule");
	Ok(())
}

fn u32_to_name(i: u32) -> TaskName {
	i.using_encoded(blake2_256)
}

fn make_task<T: Config>(
	periodic: bool,
	named: bool,
	signed: bool,
	maybe_lookup_len: Option<u32>,
	priority: Priority,
) -> ScheduledOf<T> {
	let call = make_call::<T>(maybe_lookup_len);
	let maybe_periodic = match periodic {
		true => Some((100u32.into(), 100)),
		false => None,
	};
	let maybe_id = match named {
		true => Some(u32_to_name(0)),
		false => None,
	};
	let origin = make_origin::<T>(signed);
	Scheduled { maybe_id, priority, call, maybe_periodic, origin, _phantom: PhantomData }
}

fn bounded<T: Config>(len: u32) -> Option<BoundedCallOf<T>> {
	let call =
		<<T as Config>::RuntimeCall>::from(SystemCall::remark { remark: vec![0; len as usize] });
	T::Preimages::bound(call).ok()
}

fn make_call<T: Config>(maybe_lookup_len: Option<u32>) -> BoundedCallOf<T> {
	let bound = BoundedInline::bound() as u32;
	let mut len = match maybe_lookup_len {
		Some(len) => len.min(T::Preimages::MAX_LENGTH as u32 - 2).max(bound) - 3,
		None => bound.saturating_sub(4),
	};

	loop {
		let c = match bounded::<T>(len) {
			Some(x) => x,
			None => {
				len -= 1;
				continue
			},
		};
		if c.lookup_needed() == maybe_lookup_len.is_some() {
			break c
		}
		if maybe_lookup_len.is_some() {
			len += 1;
		} else {
			if len > 0 {
				len -= 1;
			} else {
				break c
			}
		}
	}
}

fn make_origin<T: Config>(signed: bool) -> <T as Config>::PalletsOrigin {
	match signed {
		true => frame_system::RawOrigin::Signed(account("origin", 0, SEED)).into(),
		false => frame_system::RawOrigin::Root.into(),
	}
}

#[benchmarks(where BlockFor<T>: Into<u64> + From<u32>, BucketFor<T>: From<u64>)]
mod benchmarks {
	use super::*;

	// `service_agendas` when no work is done.
	#[benchmark]
	fn service_agendas_base() {
		let bucket: BucketFor<T> = BUCKET.into();
		let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();
		let now: TimeFor<T> = bucket * bucket_resolution;
		IncompleteSince::<T>::put(bucket - One::one());

		#[block]
		{
			Pallet::<T>::service_agendas(&mut WeightMeter::new(), now, 0);
		}

		assert_eq!(IncompleteSince::<T>::get(), Some(bucket - One::one()));
	}

	// `service_agenda` when no work is done.
	#[benchmark]
	fn service_agenda_base(
		s: Linear<0, { T::MaxScheduledPerBucket::get() }>,
	) -> Result<(), BenchmarkError> {
		let bucket: BucketFor<T> = BUCKET.into();
		fill_schedule::<T>(bucket, s)?;
		assert_eq!(Agenda::<T>::get(bucket).len() as u32, s);

		#[block]
		{
			Pallet::<T>::service_agenda(&mut WeightMeter::new(), true, bucket, 0);
		}

		assert_eq!(Agenda::<T>::get(bucket).len() as u32, s);

		Ok(())
	}

	// `service_task` when the task is a non-periodic, non-named, non-fetched call which is not
	// dispatched (e.g. due to being overweight).
	#[benchmark]
	fn service_task_base() {
		let bucket: BucketFor<T> = BUCKET.into();
		let task = make_task::<T>(false, false, false, None, 0);
		// prevent any tasks from actually being executed as we only want the surrounding weight.
		let mut counter = WeightMeter::with_limit(Weight::zero());
		#[block]
		{
			let _ = Pallet::<T>::service_task(&mut counter, bucket, 0, true, task);
		}
	}

	// `service_task` when the task is a non-periodic, non-named, fetched call (with a known
	// preimage length) and which is not dispatched (e.g. due to being overweight).
	#[benchmark(pov_mode = MaxEncodedLen {
		// Use measured PoV size for the Preimages since we pass in a length witness.
		Preimage::PreimageFor: Measured
	})]
	fn service_task_fetched(
		s: Linear<{ BoundedInline::bound() as u32 }, { T::Preimages::MAX_LENGTH as u32 }>,
	) {
		let bucket: BucketFor<T> = BUCKET.into();
		let task = make_task::<T>(false, false, false, Some(s), 0);
		// prevent any tasks from actually being executed as we only want the surrounding weight.
		let mut counter = WeightMeter::with_limit(Weight::zero());

		#[block]
		{
			let _ = Pallet::<T>::service_task(&mut counter, bucket, 0, true, task);
		}
	}

	// `service_task` when the task is a non-periodic, named, non-fetched call which is not
	// dispatched (e.g. due to being overweight).
	#[benchmark]
	fn service_task_named() {
		let bucket: BucketFor<T> = BUCKET.into();
		let task = make_task::<T>(false, true, false, None, 0);
		// prevent any tasks from actually being executed as we only want the surrounding weight.
		let mut counter = WeightMeter::with_limit(Weight::zero());

		#[block]
		{
			let _ = Pallet::<T>::service_task(&mut counter, bucket, 0, true, task);
		}
	}

	// `service_task` when the task is a periodic, non-named, non-fetched call which is not
	// dispatched (e.g. due to being overweight).
	#[benchmark]
	fn service_task_periodic() {
		let bucket: BucketFor<T> = BUCKET.into();
		let task = make_task::<T>(true, false, false, None, 0);
		// prevent any tasks from actually being executed as we only want the surrounding weight.
		let mut counter = WeightMeter::with_limit(Weight::zero());

		#[block]
		{
			let _ = Pallet::<T>::service_task(&mut counter, bucket, 0, true, task);
		}
	}

	// `execute_dispatch` when the origin is `Signed`, not counting the dispatchable's weight.
	#[benchmark]
	fn execute_dispatch_signed() -> Result<(), BenchmarkError> {
		let mut counter = WeightMeter::new();
		let origin = make_origin::<T>(true);
		let call = T::Preimages::realize(&make_call::<T>(None))?.0;
		let result;

		#[block]
		{
			result = Pallet::<T>::execute_dispatch(&mut counter, origin, call);
		}

		assert!(result.is_ok());

		Ok(())
	}

	// `execute_dispatch` when the origin is not `Signed`, not counting the dispatchable's weight.
	#[benchmark]
	fn execute_dispatch_unsigned() -> Result<(), BenchmarkError> {
		let mut counter = WeightMeter::new();
		let origin = make_origin::<T>(false);
		let call = T::Preimages::realize(&make_call::<T>(None))?.0;
		let result;

		#[block]
		{
			result = Pallet::<T>::execute_dispatch(&mut counter, origin, call);
		}

		assert!(result.is_ok());

		Ok(())
	}

	#[benchmark]
	fn schedule(
		s: Linear<0, { T::MaxScheduledPerBucket::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		let bucket: BucketFor<T> = BUCKET.into();
		let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();
		let when: TimeFor<T> = bucket * bucket_resolution;
		let periodic = Some((bucket_resolution, 100));
		let priority = 0;
		// Essentially a no-op call.
		let call = Box::new(SystemCall::set_storage { items: vec![] }.into());

		fill_schedule::<T>(bucket, s)?;

		#[extrinsic_call]
		_(RawOrigin::Root, when, periodic, priority, call);

		ensure!(Agenda::<T>::get(bucket).len() == s as usize + 1, "didn't add to schedule");

		Ok(())
	}

	#[benchmark]
	fn cancel(s: Linear<1, { T::MaxScheduledPerBucket::get() }>) -> Result<(), BenchmarkError> {
		let bucket: BucketFor<T> = BUCKET.into();

		fill_schedule::<T>(bucket, s)?;
		assert_eq!(Agenda::<T>::get(bucket).len(), s as usize);

		#[extrinsic_call]
		_(RawOrigin::Root, (bucket, 0u32));

		ensure!(
			s == 1 || Lookup::<T>::get(u32_to_name(0)).is_none(),
			"didn't remove from lookup if more than 1 task scheduled"
		);
		// Removed schedule is NONE
		ensure!(
			s == 1 || Agenda::<T>::get(bucket)[0].is_none(),
			"didn't remove from schedule if more than 1 task scheduled"
		);
		ensure!(
			s > 1 || Agenda::<T>::get(bucket).len() == 0,
			"remove from schedule if only 1 task scheduled"
		);

		Ok(())
	}

	#[benchmark]
	fn schedule_named(
		s: Linear<0, { T::MaxScheduledPerBucket::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		let id = u32_to_name(s);
		let bucket: BucketFor<T> = BUCKET.into();
		let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();
		let when: TimeFor<T> = bucket * bucket_resolution;
		let periodic = Some((bucket_resolution, 100));
		let priority = 0;
		// Essentially a no-op call.
		let call = Box::new(SystemCall::set_storage { items: vec![] }.into());

		fill_schedule::<T>(bucket, s)?;

		#[extrinsic_call]
		_(RawOrigin::Root, id, when, periodic, priority, call);

		ensure!(Agenda::<T>::get(bucket).len() == s as usize + 1, "didn't add to schedule");

		Ok(())
	}

	#[benchmark]
	fn cancel_named(
		s: Linear<1, { T::MaxScheduledPerBucket::get() }>,
	) -> Result<(), BenchmarkError> {
		let bucket: BucketFor<T> = BUCKET.into();

		fill_schedule::<T>(bucket, s)?;

		#[extrinsic_call]
		_(RawOrigin::Root, u32_to_name(0));

		ensure!(
			s == 1 || Lookup::<T>::get(u32_to_name(0)).is_none(),
			"didn't remove from lookup if more than 1 task scheduled"
		);
		// Removed schedule is NONE
		ensure!(
			s == 1 || Agenda::<T>::get(bucket)[0].is_none(),
			"didn't remove from schedule if more than 1 task scheduled"
		);
		ensure!(
			s > 1 || Agenda::<T>::get(bucket).len() == 0,
			"remove from schedule if only 1 task scheduled"
		);

		Ok(())
	}

	#[benchmark]
	fn schedule_retry_periodic(
		s: Linear<1, { T::MaxScheduledPerBucket::get() }>,
	) -> Result<(), BenchmarkError> {
		let bucket: BucketFor<T> = BUCKET.into();

		fill_schedule::<T>(bucket, s)?;
		let name = u32_to_name(s - 1);
		let address = Lookup::<T>::get(name).unwrap();
		let retry_config = RetryConfig {
			total_retries: 10,
			remaining: 10,
			strategy: RetryStrategy::Periodic(1_u32.into()),
		};
		Retries::<T>::insert(address, retry_config.clone());
		let (bucket, index) = address;
		let task = Agenda::<T>::get(bucket)[index as usize].clone().unwrap();
		let mut weight_counter = WeightMeter::with_limit(T::MaximumWeight::get());

		#[block]
		{
			Pallet::<T>::schedule_retry(
				&mut weight_counter,
				bucket,
				index,
				&task,
				retry_config,
			);
		}

		let next_bucket = bucket + One::one();
		assert_eq!(
			Retries::<T>::get((next_bucket, 0)),
			Some(RetryConfig {
				total_retries: 10,
				remaining: 9,
				strategy: RetryStrategy::Periodic(1_u32.into()),
			})
		);

		Ok(())
	}

	#[benchmark]
	fn schedule_retry_same_bucket(
		s: Linear<1, { T::MaxScheduledPerBucket::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		let bucket: BucketFor<T> = BUCKET.into();

		// Fill the current bucket to capacity so same-bucket placement fails.
		fill_schedule::<T>(bucket, T::MaxScheduledPerBucket::get())?;
		let name = u32_to_name(T::MaxScheduledPerBucket::get() - 1);
		let address = Lookup::<T>::get(name).unwrap();
		let retry_config = RetryConfig {
			total_retries: 10,
			remaining: 10,
			strategy: RetryStrategy::SameBucket,
		};
		Retries::<T>::insert(address, retry_config.clone());
		let (bucket, index) = address;
		let task = Agenda::<T>::get(bucket)[index as usize].clone().unwrap();

		// Fill the fallback bucket (bucket + 1) with `s` items so place_task
		// must scan through them. One slot remains for the retry task.
		let next_bucket = bucket + One::one();
		fill_schedule_offset::<T>(next_bucket, s, T::MaxScheduledPerBucket::get())?;

		let mut weight_counter = WeightMeter::with_limit(T::MaximumWeight::get());

		#[block]
		{
			Pallet::<T>::schedule_retry(
				&mut weight_counter,
				bucket,
				index,
				&task,
				retry_config,
			);
		}

		// Same bucket was full, so retry goes to next_bucket (bucket + 1).
		// It's appended after the `s` items already there.
		assert!(Retries::<T>::get((next_bucket, s)).is_some());

		Ok(())
	}

	#[benchmark]
	fn schedule_retry_exponential_backoff(
		s: Linear<1, { T::MaxScheduledPerBucket::get() }>,
	) -> Result<(), BenchmarkError> {
		let bucket: BucketFor<T> = BUCKET.into();

		fill_schedule::<T>(bucket, s)?;
		let name = u32_to_name(s - 1);
		let address = Lookup::<T>::get(name).unwrap();
		let retry_config = RetryConfig {
			total_retries: 10,
			remaining: 10,
			strategy: RetryStrategy::ExponentialBackoff,
		};
		Retries::<T>::insert(address, retry_config.clone());
		let (bucket, index) = address;
		let task = Agenda::<T>::get(bucket)[index as usize].clone().unwrap();
		let mut weight_counter = WeightMeter::with_limit(T::MaximumWeight::get());

		#[block]
		{
			Pallet::<T>::schedule_retry(
				&mut weight_counter,
				bucket,
				index,
				&task,
				retry_config,
			);
		}

		// First attempt (attempt=0): target = bucket + 2^0 = bucket + 1
		let target_bucket = bucket + One::one();
		assert_eq!(
			Retries::<T>::get((target_bucket, 0)),
			Some(RetryConfig {
				total_retries: 10,
				remaining: 9,
				strategy: RetryStrategy::ExponentialBackoff,
			})
		);

		Ok(())
	}

	#[benchmark]
	fn set_retry() -> Result<(), BenchmarkError> {
		let s = T::MaxScheduledPerBucket::get();
		let bucket: BucketFor<T> = BUCKET.into();

		fill_schedule::<T>(bucket, s)?;
		let name = u32_to_name(s - 1);
		let address = Lookup::<T>::get(name).unwrap();
		let (bucket, index) = address;
		let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();
		let strategy = RetryStrategy::Periodic(bucket_resolution);

		#[extrinsic_call]
		_(RawOrigin::Root, (bucket, index), 255u8, strategy);

		assert_eq!(
			Retries::<T>::get((bucket, index)),
			Some(RetryConfig {
				total_retries: 255u8,
				remaining: 255u8,
				strategy: RetryStrategy::Periodic(One::one()),
			})
		);
		assert_last_event::<T>(
			Event::RetrySet {
				task: address,
				id: None,
				retries: 255u8,
				strategy: RetryStrategy::Periodic(bucket_resolution),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn set_retry_named() -> Result<(), BenchmarkError> {
		let s = T::MaxScheduledPerBucket::get();
		let bucket: BucketFor<T> = BUCKET.into();

		fill_schedule::<T>(bucket, s)?;
		let name = u32_to_name(s - 1);
		let address = Lookup::<T>::get(name).unwrap();
		let (bucket, index) = address;
		let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();
		let strategy = RetryStrategy::Periodic(bucket_resolution);

		#[extrinsic_call]
		_(RawOrigin::Root, name, 255u8, strategy);

		assert_eq!(
			Retries::<T>::get((bucket, index)),
			Some(RetryConfig {
				total_retries: 255u8,
				remaining: 255u8,
				strategy: RetryStrategy::Periodic(One::one()),
			})
		);
		assert_last_event::<T>(
			Event::RetrySet {
				task: address,
				id: Some(name),
				retries: 255u8,
				strategy: RetryStrategy::Periodic(bucket_resolution),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn cancel_retry() -> Result<(), BenchmarkError> {
		let s = T::MaxScheduledPerBucket::get();
		let bucket: BucketFor<T> = BUCKET.into();

		fill_schedule::<T>(bucket, s)?;
		let name = u32_to_name(s - 1);
		let address = Lookup::<T>::get(name).unwrap();
		let (bucket, index) = address;
		let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();
		assert!(
			Pallet::<T>::set_retry(
				RawOrigin::Root.into(),
				(bucket, index),
				10,
				RetryStrategy::Periodic(bucket_resolution),
			)
			.is_ok()
		);

		#[extrinsic_call]
		_(RawOrigin::Root, (bucket, index));

		assert!(!Retries::<T>::contains_key((bucket, index)));
		assert_last_event::<T>(Event::RetryCancelled { task: address, id: None }.into());

		Ok(())
	}

	#[benchmark]
	fn cancel_retry_named() -> Result<(), BenchmarkError> {
		let s = T::MaxScheduledPerBucket::get();
		let bucket: BucketFor<T> = BUCKET.into();

		fill_schedule::<T>(bucket, s)?;
		let name = u32_to_name(s - 1);
		let address = Lookup::<T>::get(name).unwrap();
		let (bucket, index) = address;
		let bucket_resolution: TimeFor<T> = T::BucketResolution::get().into();
		assert!(
			Pallet::<T>::set_retry_named(
				RawOrigin::Root.into(),
				name,
				10,
				RetryStrategy::Periodic(bucket_resolution),
			)
			.is_ok()
		);

		#[extrinsic_call]
		_(RawOrigin::Root, name);

		assert!(!Retries::<T>::contains_key((bucket, index)));
		assert_last_event::<T>(Event::RetryCancelled { task: address, id: Some(name) }.into());

		Ok(())
	}

	#[benchmark]
	fn migration_incomplete_since() -> Result<(), BenchmarkError> {
		let block: BlockFor<T> = 5u32.into();
		imp::old::IncompleteSince::<T>::put(block);

		#[block]
		{
			let mut meter =
				WeightMeter::with_limit(<() as imp::WeightInfo>::migration_incomplete_since());
			ImportFromBlockScheduler::<T, BenchBlockTime, ()>::step(
				Some(MigrationState::IncompleteSince),
				&mut meter,
			)
			.map_err(|_| BenchmarkError::Stop("step failed"))?;
		}

		assert!(IncompleteSince::<T>::get().is_some());
		Ok(())
	}

	#[benchmark]
	fn migration_agenda(
		s: Linear<1, { T::MaxScheduledPerBucket::get() }>,
	) -> Result<(), BenchmarkError> {
		let block: BlockFor<T> = 0u32.into();
		setup_old_agenda::<T>(block, s);

		#[block]
		{
			let bucket_resolution: u64 = T::BucketResolution::get().into();
			let max = T::MaxScheduledPerBucket::get();
			let (b, tasks) = imp::next_old_agenda::<T>(None)
				.ok_or(BenchmarkError::Stop("no old agenda"))?;
			imp::migrate_one_agenda::<T>(
				b,
				tasks,
				BenchBlockTime::get(),
				bucket_resolution,
				max,
			);
		}

		assert_eq!(MigrationAddressMap::<T>::iter().count() as u32, s);
		Ok(())
	}

	#[benchmark]
	fn migration_lookup() -> Result<(), BenchmarkError> {
		let block: BlockFor<T> = 0u32.into();
		let bucket: BucketFor<T> = 0u64.into();
		let name = u32_to_name(0);
		imp::old::Lookup::<T>::insert(name, (block, 0u32));
		MigrationAddressMap::<T>::insert((block, 0u32), (bucket, 0u32));

		#[block]
		{
			let (n, (b, i)) = imp::next_old_lookup::<T>(None)
				.ok_or(BenchmarkError::Stop("no old lookup"))?;
			imp::migrate_one_lookup::<T>(n, b, i);
		}

		assert!(Lookup::<T>::get(name).is_some());
		Ok(())
	}

	#[benchmark]
	fn migration_retries() -> Result<(), BenchmarkError> {
		let block: BlockFor<T> = 0u32.into();
		let bucket: BucketFor<T> = 0u64.into();
		let period: BlockFor<T> = 100u32.into();
		imp::old::Retries::<T>::insert(
			(block, 0u32),
			imp::old::RetryConfig { total_retries: 3, remaining: 3, period },
		);
		MigrationAddressMap::<T>::insert((block, 0u32), (bucket, 0u32));

		#[block]
		{
			let bucket_resolution_u64: u64 = T::BucketResolution::get().into();
			let ((b, i), cfg) = imp::next_old_retry::<T>(None)
				.ok_or(BenchmarkError::Stop("no old retry"))?;
			imp::migrate_one_retry::<T>(
				b,
				i,
				cfg,
				BenchBlockTime::get(),
				bucket_resolution_u64,
			);
		}

		assert!(Retries::<T>::get((bucket, 0u32)).is_some());
		Ok(())
	}

	#[benchmark]
	fn migration_cleanup() -> Result<(), BenchmarkError> {
		let block: BlockFor<T> = 0u32.into();
		let bucket: BucketFor<T> = 0u64.into();
		MigrationAddressMap::<T>::insert((block, 0u32), (bucket, 0u32));

		#[block]
		{
			let addr = imp::next_address_map_entry::<T>(None)
				.ok_or(BenchmarkError::Stop("no address map entry"))?;
			MigrationAddressMap::<T>::remove(addr);
		}

		assert_eq!(MigrationAddressMap::<T>::iter().count(), 0);
		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		mock::new_test_ext(),
		mock::Test
	}
}
