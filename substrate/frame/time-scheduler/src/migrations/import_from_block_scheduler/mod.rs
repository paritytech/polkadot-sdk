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

//! Stepped migration that imports storage from the block-based `pallet-scheduler`
//! (prefix `"Scheduler"`) into the time-based `pallet-time-scheduler`.
//!
//! # Sizing `MaxScheduledPerBucket`
//!
//! Multiple old blocks map to the same bucket. When merging, the new bucket must
//! be large enough to hold all merged tasks, otherwise excess tasks are **dropped**.
//! Pick `MaxScheduledPerBucket` such that:
//!
//! ```text
//! MaxScheduledPerBucket >= old_max_per_block * (bucket_resolution_ms / block_time_ms)
//! ```
//!
//! For example, with `old_max_per_block = 50`, `bucket_resolution = 60_000ms`, and
//! `block_time = 6000ms`, set `MaxScheduledPerBucket >= 500`.

pub mod weights;

use crate::*;
use alloc::vec::Vec;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	weights::WeightMeter,
};
use frame_system::pallet_prelude::BlockNumberFor;

pub use weights::WeightInfo;

/// The log target.
const TARGET: &str = "runtime::time_scheduler::migration";

/// Migration identifier (must be exactly 21 bytes).
pub const MIGRATION_ID: &[u8; 21] = b"pallet-time-scheduler";

/// Block number of the old block-based scheduler. Its agendas, retries and lookups
/// are all keyed by (or contain) this type.
pub type BlockFor<T> = BlockNumberFor<T>;

/// A scheduled task as stored by the old block-based scheduler.
///
/// Identical to [`ScheduledOf`] except the periodic interval is a block count
/// (`BlockFor<T>`) rather than a bucket count (`BucketFor<T>`).
pub type OldScheduledOf<T> = Scheduled<
	TaskName,
	BoundedCallOf<T>,
	BlockFor<T>,
	<T as Config>::PalletsOrigin,
	<T as frame_system::Config>::AccountId,
>;

/// Old block-based scheduler storage, mirrored here so we can decode it without
/// depending on `pallet-scheduler` directly.
///
/// These mirror `pallet_scheduler`'s storage layout (v4). Block numbers are keyed
/// as `BlockFor<T>` — the exact type the old scheduler used — so the decoding is
/// correct regardless of how `BlockNumber` and `Moment` relate.
pub mod old {
	use super::*;

	/// The block-based `RetryConfig` has 3 fields (no `strategy`). `period` is a
	/// block count.
	#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub struct RetryConfig<Period> {
		pub total_retries: u8,
		pub remaining: u8,
		pub period: Period,
	}

	#[frame_support::storage_alias]
	pub type IncompleteSince<T: Config> = StorageValue<Scheduler, BlockFor<T>>;

	#[frame_support::storage_alias]
	pub type Agenda<T: Config> = StorageMap<
		Scheduler,
		Twox64Concat,
		BlockFor<T>,
		Vec<Option<OldScheduledOf<T>>>,
		ValueQuery,
	>;

	#[frame_support::storage_alias]
	pub type Retries<T: Config> = StorageMap<
		Scheduler,
		Blake2_128Concat,
		TaskAddress<BlockFor<T>>,
		RetryConfig<BlockFor<T>>,
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub type Lookup<T: Config> =
		StorageMap<Scheduler, Twox64Concat, TaskName, TaskAddress<BlockFor<T>>>;
}

/// Temporary storage built during the `Agenda` phase, mapping each old
/// `(block, index)` to its new `(bucket, new_index)`. Used by the `Lookup`
/// and `Retries` phases, then cleared at the end.
#[frame_support::storage_alias]
pub(crate) type MigrationAddressMap<T: Config> = StorageMap<
	Pallet<T>,
	Blake2_128Concat,
	TaskAddress<BlockFor<T>>,
	TaskAddress<BucketFor<T>>,
	OptionQuery,
>;

/// Convert a block number to a bucket index.
///
/// `bucket = block * block_time_ms / bucket_resolution`. Returns 0 if
/// `bucket_resolution` is 0 (which is rejected by `integrity_test`, but guarded
/// here defensively).
pub fn block_to_bucket(block: u64, block_time_ms: u64, bucket_resolution: u64) -> u64 {
	block.saturating_mul(block_time_ms).checked_div(bucket_resolution).unwrap_or(0)
}

/// Convert a period in blocks to a period in buckets (minimum 1). Defaults to 1
/// if `bucket_resolution` is 0.
pub fn block_period_to_bucket_period(
	block_period: u64,
	block_time_ms: u64,
	bucket_resolution: u64,
) -> u64 {
	block_period
		.saturating_mul(block_time_ms)
		.checked_div(bucket_resolution)
		.unwrap_or(1)
		.max(1)
}

/// Progressive states of the migration. The migration starts at the first
/// variant and ends with `Cleanup` returning `None`.
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
#[scale_info(skip_type_params(T))]
pub enum MigrationState<T: Config> {
	/// Migrate `IncompleteSince` (one-shot).
	IncompleteSince,
	/// Migrate one old `Agenda` entry per call. Resumes after `last_block`.
	Agenda { last_block: Option<BlockFor<T>> },
	/// Migrate one old `Lookup` entry per call. Resumes after `last_name`.
	Lookup { last_name: Option<TaskName> },
	/// Migrate one old `Retries` entry per call. Resumes after `last_addr`.
	Retries { last_addr: Option<TaskAddress<BlockFor<T>>> },
	/// Delete one `MigrationAddressMap` entry per call. Resumes after `last_addr`.
	Cleanup { last_addr: Option<TaskAddress<BlockFor<T>>> },
}

/// Per-phase weight estimate used by the meter to decide whether to attempt
/// another iteration in this block.
fn required_weight<T: Config, W: WeightInfo>(state: &MigrationState<T>) -> Weight {
	match state {
		MigrationState::IncompleteSince => W::migration_incomplete_since(),
		MigrationState::Agenda { .. } => W::migration_agenda(T::MaxScheduledPerBucket::get()),
		MigrationState::Lookup { .. } => W::migration_lookup(),
		MigrationState::Retries { .. } => W::migration_retries(),
		MigrationState::Cleanup { .. } => W::migration_cleanup(),
	}
}

/// Stepped migration that imports storage from `pallet-scheduler` into
/// `pallet-time-scheduler`.
///
/// Generic parameters:
/// - `T`: the time-scheduler `Config`.
/// - `BlockTimeMs`: provides the chain's expected block time in milliseconds (e.g.
///   a `Get<u64>` returning `6000` for 6-second blocks).
/// - `W`: weights for each phase of the migration.
pub struct ImportFromBlockScheduler<T, BlockTimeMs, W>(PhantomData<(T, BlockTimeMs, W)>);

impl<T, BlockTimeMs, W> SteppedMigration for ImportFromBlockScheduler<T, BlockTimeMs, W>
where
	T: Config,
	BlockTimeMs: Get<u64>,
	W: WeightInfo,
	BlockFor<T>: Into<u64>,
	BucketFor<T>: From<u64>,
{
	type Cursor = MigrationState<T>;
	type Identifier = MigrationId<21>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *MIGRATION_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let block_time_ms = BlockTimeMs::get();
		let bucket_resolution: u64 = T::BucketResolution::get().into();
		let max_per_bucket = T::MaxScheduledPerBucket::get();

		// First-ever call: start at the `IncompleteSince` phase.
		let mut cursor = cursor.unwrap_or(MigrationState::IncompleteSince);

		// Refuse to start if we can't afford even the next step.
		let initial_required = required_weight::<T, W>(&cursor);
		if meter.remaining().any_lt(initial_required) {
			return Err(SteppedMigrationError::InsufficientWeight {
				required: initial_required,
			});
		}

		loop {
			let required = required_weight::<T, W>(&cursor);
			if meter.try_consume(required).is_err() {
				break;
			}

			cursor = match cursor {
				MigrationState::IncompleteSince => {
					if let Some(block) = old::IncompleteSince::<T>::take() {
						let bucket: BucketFor<T> = block_to_bucket(
							block.into(),
							block_time_ms,
							bucket_resolution,
						)
						.into();
						crate::IncompleteSince::<T>::put(bucket);
						log::info!(
							target: TARGET,
							"Migrated IncompleteSince: block {:?} -> bucket {:?}",
							block,
							bucket,
						);
					}
					MigrationState::Agenda { last_block: None }
				},
				MigrationState::Agenda { last_block } => {
					match next_old_agenda::<T>(last_block) {
						Some((block, tasks)) => {
							migrate_one_agenda::<T>(
								block,
								tasks,
								block_time_ms,
								bucket_resolution,
								max_per_bucket,
							);
							MigrationState::Agenda { last_block: Some(block) }
						},
						None => MigrationState::Lookup { last_name: None },
					}
				},
				MigrationState::Lookup { last_name } => {
					match next_old_lookup::<T>(last_name) {
						Some((name, (block, index))) => {
							migrate_one_lookup::<T>(name, block, index);
							MigrationState::Lookup { last_name: Some(name) }
						},
						None => MigrationState::Retries { last_addr: None },
					}
				},
				MigrationState::Retries { last_addr } => {
					match next_old_retry::<T>(last_addr) {
						Some(((block, index), old_config)) => {
							migrate_one_retry::<T>(
								block,
								index,
								old_config,
								block_time_ms,
								bucket_resolution,
							);
							MigrationState::Retries { last_addr: Some((block, index)) }
						},
						None => MigrationState::Cleanup { last_addr: None },
					}
				},
				MigrationState::Cleanup { last_addr } => {
					match next_address_map_entry::<T>(last_addr) {
						Some(addr) => {
							MigrationAddressMap::<T>::remove(addr);
							MigrationState::Cleanup { last_addr: Some(addr) }
						},
						None => {
							log::info!(target: TARGET, "Migration complete");
							return Ok(None);
						},
					}
				},
			};
		}

		Ok(Some(cursor))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let incomplete_since = old::IncompleteSince::<T>::get().is_some();
		let mut agenda_count: u32 = 0;
		let mut total_tasks: u32 = 0;
		for (_block, tasks) in old::Agenda::<T>::iter() {
			agenda_count = agenda_count.saturating_add(1);
			total_tasks = total_tasks.saturating_add(tasks.len() as u32);
		}
		let lookup_count = old::Lookup::<T>::iter().count() as u32;
		let retries_count = old::Retries::<T>::iter().count() as u32;

		log::info!(
			target: TARGET,
			"pre_upgrade: incomplete_since={}, agendas={}, tasks={}, lookups={}, retries={}",
			incomplete_since,
			agenda_count,
			total_tasks,
			lookup_count,
			retries_count,
		);

		Ok((incomplete_since, agenda_count, total_tasks, lookup_count, retries_count).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (prev_incomplete, prev_agendas, prev_tasks, prev_lookups, prev_retries) =
			<(bool, u32, u32, u32, u32)>::decode(&mut &prev[..])
				.map_err(|_| "failed to decode pre_upgrade state")?;

		let new_incomplete = crate::IncompleteSince::<T>::get().is_some();
		ensure!(
			prev_incomplete == new_incomplete,
			"IncompleteSince presence mismatch after migration",
		);

		let mut new_agenda_count: u32 = 0;
		let mut new_task_count: u32 = 0;
		for (_bucket, tasks) in crate::Agenda::<T>::iter() {
			new_agenda_count = new_agenda_count.saturating_add(1);
			new_task_count = new_task_count.saturating_add(tasks.len() as u32);
		}

		// Multiple old blocks may merge into a single bucket, so new agenda count
		// must be <= old. Tasks must match exactly (we drop loudly if they don't fit,
		// which would surface as a mismatch here).
		ensure!(
			new_agenda_count <= prev_agendas,
			"new agenda count exceeds old (impossible after merge)",
		);
		ensure!(
			new_task_count == prev_tasks,
			"task count mismatch after migration (some tasks may have been dropped)",
		);

		let new_lookup_count = crate::Lookup::<T>::iter().count() as u32;
		ensure!(
			new_lookup_count == prev_lookups,
			"lookup count mismatch after migration",
		);

		let new_retries_count = crate::Retries::<T>::iter().count() as u32;
		ensure!(
			new_retries_count == prev_retries,
			"retries count mismatch after migration",
		);

		ensure!(
			MigrationAddressMap::<T>::iter().next().is_none(),
			"temporary address map was not fully cleaned up",
		);

		Ok(())
	}
}

pub(crate) fn next_old_agenda<T: Config>(
	last_block: Option<BlockFor<T>>,
) -> Option<(BlockFor<T>, Vec<Option<OldScheduledOf<T>>>)> {
	let mut iter = match last_block {
		Some(b) => old::Agenda::<T>::iter_from(old::Agenda::<T>::hashed_key_for(b)),
		None => old::Agenda::<T>::iter(),
	};
	iter.next()
}

pub(crate) fn next_old_lookup<T: Config>(
	last_name: Option<TaskName>,
) -> Option<(TaskName, TaskAddress<BlockFor<T>>)> {
	let mut iter = match last_name {
		Some(n) => old::Lookup::<T>::iter_from(old::Lookup::<T>::hashed_key_for(n)),
		None => old::Lookup::<T>::iter(),
	};
	iter.next()
}

pub(crate) fn next_old_retry<T: Config>(
	last_addr: Option<TaskAddress<BlockFor<T>>>,
) -> Option<(TaskAddress<BlockFor<T>>, old::RetryConfig<BlockFor<T>>)> {
	let mut iter = match last_addr {
		Some(a) => old::Retries::<T>::iter_from(old::Retries::<T>::hashed_key_for(a)),
		None => old::Retries::<T>::iter(),
	};
	iter.next()
}

pub(crate) fn next_address_map_entry<T: Config>(
	last_addr: Option<TaskAddress<BlockFor<T>>>,
) -> Option<TaskAddress<BlockFor<T>>> {
	let mut iter = match last_addr {
		Some(a) => MigrationAddressMap::<T>::iter_from(
			MigrationAddressMap::<T>::hashed_key_for(a),
		),
		None => MigrationAddressMap::<T>::iter(),
	};
	iter.next().map(|(k, _)| k)
}

/// Convert an old block-based scheduled task into a time-based one, translating
/// its periodic interval from blocks to buckets.
fn convert_task<T>(
	task: OldScheduledOf<T>,
	block_time_ms: u64,
	bucket_resolution: u64,
) -> ScheduledOf<T>
where
	T: Config,
	BlockFor<T>: Into<u64>,
	BucketFor<T>: From<u64>,
{
	let maybe_periodic = task.maybe_periodic.map(|(period, count)| {
		let new_period: BucketFor<T> = block_period_to_bucket_period(
			period.into(),
			block_time_ms,
			bucket_resolution,
		)
		.into();
		(new_period, count)
	});
	Scheduled {
		maybe_id: task.maybe_id,
		priority: task.priority,
		call: task.call,
		maybe_periodic,
		origin: task.origin,
		_phantom: PhantomData,
	}
}

pub(crate) fn migrate_one_agenda<T>(
	block: BlockFor<T>,
	tasks: Vec<Option<OldScheduledOf<T>>>,
	block_time_ms: u64,
	bucket_resolution: u64,
	max_per_bucket: u32,
) where
	T: Config,
	BlockFor<T>: Into<u64>,
	BucketFor<T>: From<u64>,
{
	let bucket_u64 = block_to_bucket(block.into(), block_time_ms, bucket_resolution);
	let bucket: BucketFor<T> = bucket_u64.into();

	let mut existing = crate::Agenda::<T>::get(bucket);
	let starting_offset = existing.len() as u32;
	let max = max_per_bucket as usize;

	let mut dropped: u32 = 0;
	for (offset, maybe_task) in tasks.into_iter().enumerate() {
		if existing.len() >= max {
			dropped = dropped.saturating_add(1);
			continue;
		}
		let new_index = starting_offset.saturating_add(offset as u32);
		// Record mapping for this task position regardless of whether the slot is
		// occupied or `None`, so Lookup/Retries can resolve any old (block, index).
		MigrationAddressMap::<T>::insert((block, offset as u32), (bucket, new_index));
		let converted =
			maybe_task.map(|task| convert_task::<T>(task, block_time_ms, bucket_resolution));
		existing
			.try_push(converted)
			.expect("checked existing.len() < max above; qed");
	}

	if dropped > 0 {
		log::warn!(
			target: TARGET,
			"Bucket {:?}: dropped {} tasks from block {:?} (MaxScheduledPerBucket={}); \
			 consider increasing MaxScheduledPerBucket",
			bucket,
			dropped,
			block,
			max_per_bucket,
		);
	}

	crate::Agenda::<T>::insert(bucket, existing);
}

pub(crate) fn migrate_one_lookup<T>(name: TaskName, block: BlockFor<T>, index: u32)
where
	T: Config,
{
	if let Some(new_addr) = MigrationAddressMap::<T>::get((block, index)) {
		crate::Lookup::<T>::insert(name, new_addr);
	} else {
		log::warn!(
			target: TARGET,
			"Lookup entry for ({:?}, {}) has no address mapping (task dropped during merge)",
			block,
			index,
		);
	}
}

pub(crate) fn migrate_one_retry<T>(
	block: BlockFor<T>,
	index: u32,
	old_config: old::RetryConfig<BlockFor<T>>,
	block_time_ms: u64,
	bucket_resolution: u64,
) where
	T: Config,
	BlockFor<T>: Into<u64>,
	BucketFor<T>: From<u64>,
{
	let Some(new_addr) = MigrationAddressMap::<T>::get((block, index)) else {
		log::warn!(
			target: TARGET,
			"Retry entry for ({:?}, {}) has no address mapping (task dropped during merge)",
			block,
			index,
		);
		return;
	};
	let new_period: BucketFor<T> = block_period_to_bucket_period(
		old_config.period.into(),
		block_time_ms,
		bucket_resolution,
	)
	.into();
	let new_config = RetryConfig {
		total_retries: old_config.total_retries,
		remaining: old_config.remaining,
		strategy: RetryStrategy::Periodic(new_period),
	};
	crate::Retries::<T>::insert(new_addr, new_config);
}
