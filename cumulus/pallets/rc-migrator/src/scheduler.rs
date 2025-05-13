// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;
use frame_support::traits::schedule::v3::TaskName;
pub use frame_system::pallet_prelude::BlockNumberFor as SchedulerBlockNumberFor;
use pallet_scheduler::{RetryConfig, TaskAddress};

/// Stage of the scheduler pallet migration.
#[derive(Encode, Decode, Clone, Default, RuntimeDebug, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum SchedulerStage<BlockNumber> {
	#[default]
	IncompleteSince,
	Retries(Option<TaskAddress<BlockNumber>>),
	Lookup(Option<TaskName>),
	Finished,
}

/// Message that is being sent to the AH Migrator.
#[derive(Encode, Decode, Debug, Clone, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum RcSchedulerMessage<BlockNumber> {
	IncompleteSince(BlockNumber),
	Retries((TaskAddress<BlockNumber>, RetryConfig<BlockNumber>)),
	Lookup((TaskName, TaskAddress<BlockNumber>)),
}

pub type RcSchedulerMessageOf<T> = RcSchedulerMessage<SchedulerBlockNumberFor<T>>;

pub struct SchedulerMigrator<T> {
	_phantom: PhantomData<T>,
}

impl<T: Config> PalletMigration for SchedulerMigrator<T> {
	type Key = SchedulerStage<SchedulerBlockNumberFor<T>>;
	type Error = Error<T>;
	fn migrate_many(
		last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut last_key = last_key.unwrap_or(SchedulerStage::IncompleteSince);
		let mut messages = XcmBatchAndMeter::new_from_config::<T>();

		loop {
			if weight_counter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() ||
				weight_counter.try_consume(messages.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if T::MaxAhWeight::get()
				.any_lt(T::AhWeightInfo::receive_scheduler_lookup((messages.len() + 1) as u32))
			{
				log::info!("AH weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if messages.len() > 10_000 {
				log::warn!(target: LOG_TARGET, "Weight allowed very big batch, stopping");
				break;
			}

			last_key = match last_key {
				SchedulerStage::IncompleteSince => {
					if let Some(since) = pallet_scheduler::IncompleteSince::<T>::take() {
						messages.push(RcSchedulerMessage::IncompleteSince(since));
					}
					SchedulerStage::Retries(None)
				},
				SchedulerStage::Retries(last_key) => {
					let mut iter = if let Some(last_key) = last_key {
						pallet_scheduler::Retries::<T>::iter_from_key(last_key)
					} else {
						pallet_scheduler::Retries::<T>::iter()
					};
					match iter.next() {
						Some((key, value)) => {
							pallet_scheduler::Retries::<T>::remove(&key);
							messages.push(RcSchedulerMessage::Retries((key, value)));
							SchedulerStage::Retries(Some(key))
						},
						None => SchedulerStage::Lookup(None),
					}
				},
				SchedulerStage::Lookup(last_key) => {
					let mut iter = if let Some(last_key) = last_key {
						alias::Lookup::<T>::iter_from_key(last_key)
					} else {
						alias::Lookup::<T>::iter()
					};
					match iter.next() {
						Some((key, value)) => {
							alias::Lookup::<T>::remove(&key);
							messages.push(RcSchedulerMessage::Lookup((key, value)));
							SchedulerStage::Lookup(Some(key))
						},
						None => SchedulerStage::Finished,
					}
				},
				SchedulerStage::Finished => {
					break;
				},
			};
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				messages,
				|messages| types::AhMigratorCall::<T>::ReceiveSchedulerMessages { messages },
				|len| T::AhWeightInfo::receive_scheduler_lookup(len),
			)?;
		}

		if last_key == SchedulerStage::Finished {
			Ok(None)
		} else {
			Ok(Some(last_key))
		}
	}
}

pub struct SchedulerAgendaMigrator<T: Config> {
	_phantom: PhantomData<T>,
}

impl<T: Config> PalletMigration for SchedulerAgendaMigrator<T> {
	type Key = BlockNumberFor<T>;
	type Error = Error<T>;
	fn migrate_many(
		mut last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut messages = XcmBatchAndMeter::new_from_config::<T>();
		let mut ah_weight_counter = WeightMeter::with_limit(T::MaxAhWeight::get());

		let last_key = loop {
			if weight_counter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() ||
				weight_counter.try_consume(messages.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break last_key;
				}
			}
			if messages.len() > 10_000 {
				log::warn!(target: LOG_TARGET, "Weight allowed very big batch, stopping");
				break last_key;
			}

			let maybe_agenda = if let Some(last_key) = last_key {
				alias::Agenda::<T>::iter_from_key(last_key).next()
			} else {
				alias::Agenda::<T>::iter().next()
			};

			let Some((block, agenda)) = maybe_agenda else {
				break None;
			};

			// check if AH can handle the weight of the next agenda
			for maybe_task in agenda.iter() {
				// generally there is only one task per agenda
				let Some(task) = maybe_task else {
					continue;
				};
				let preimage_len = task.call.len().defensive_unwrap_or(
					// should not happen, but we assume some sane call length.
					512,
				);
				if ah_weight_counter
					.try_consume(T::AhWeightInfo::receive_single_scheduler_agenda(preimage_len))
					.is_err()
				{
					log::info!(
						"AH weight limit reached at batch length {}, stopping",
						messages.len()
					);
					if messages.is_empty() {
						return Err(Error::OutOfWeight);
					} else {
						break;
					}
				}
			}

			last_key = Some(block);
			alias::Agenda::<T>::remove(&block);

			if agenda.len() == 0 {
				// there are many agendas with no tasks, so we skip them
				continue;
			}

			let agenda = agenda.into_inner();
			messages.push((block, agenda));
		};

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				messages,
				|messages| types::AhMigratorCall::<T>::ReceiveSchedulerAgendaMessages { messages },
				|_| Weight::from_all(1),
			)?;
		}

		Ok(last_key)
	}
}

pub mod alias {
	use super::*;
	use frame_support::traits::{
		schedule::{Period, Priority},
		Bounded, OriginTrait,
	};

	pub type BoundedCallOf<T> =
		Bounded<<T as frame_system::Config>::RuntimeCall, <T as frame_system::Config>::Hashing>;

	/// Information regarding an item to be executed in the future.
	// FROM: https://github.com/paritytech/polkadot-sdk/blob/f373af0d1c1e296c1b07486dd74710b40089250e/substrate/frame/scheduler/src/lib.rs#L148
	#[derive(Clone, RuntimeDebug, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
	#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
	pub struct Scheduled<Call, BlockNumber, PalletsOrigin> {
		/// The unique identity for this task, if there is one.
		pub maybe_id: Option<TaskName>,
		/// This task's priority.
		pub priority: Priority,
		/// The call to be dispatched.
		pub call: Call,
		/// If the call is periodic, then this points to the information concerning that.
		pub maybe_periodic: Option<Period<BlockNumber>>,
		/// The origin with which to dispatch the call.
		pub origin: PalletsOrigin,
	}

	/// Scheduled type for the Asset Hub.
	pub type ScheduledOf<T> = Scheduled<
		BoundedCallOf<T>,
		SchedulerBlockNumberFor<T>,
		<<T as frame_system::Config>::RuntimeOrigin as OriginTrait>::PalletsOrigin,
	>;

	/// Items to be executed, indexed by the block number that they should be executed on.
	// Alias of
	#[frame_support::storage_alias(pallet_name)]
	pub type Agenda<T: pallet_scheduler::Config> = StorageMap<
		pallet_scheduler::Pallet<T>,
		Twox64Concat,
		SchedulerBlockNumberFor<T>,
		BoundedVec<Option<ScheduledOf<T>>, <T as pallet_scheduler::Config>::MaxScheduledPerBlock>,
		ValueQuery,
	>;

	// From https://github.com/paritytech/polkadot-sdk/blob/f373af0d1c1e296c1b07486dd74710b40089250e/substrate/frame/scheduler/src/lib.rs#L325
	#[frame_support::storage_alias(pallet_name)]
	pub type Lookup<T: pallet_scheduler::Config> = StorageMap<
		pallet_scheduler::Pallet<T>,
		Twox64Concat,
		TaskName,
		TaskAddress<SchedulerBlockNumberFor<T>>,
	>;
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::RcMigrationCheck for SchedulerMigrator<T> {
	type RcPrePayload = Vec<u8>;

	fn pre_check() -> Self::RcPrePayload {
		let incomplete_since = pallet_scheduler::IncompleteSince::<T>::get();
		// When the Agenda state item is migrated on the AH side, it relies on pallet-preimage state
		// for the call conversion, but it also changes the preimage state during that conversion,
		// breaking any checks we try and do after. So we grab all the necessary data for call
		// conversion upfront to avoid this reliance and allow for the checks to happen smoothly.
		let agenda_and_call_encodings: Vec<_> = alias::Agenda::<T>::iter()
			.map(|(bn, tasks)| {
				(bn, tasks.clone().into_inner(), Self::get_task_call_encodings(tasks))
			})
			.collect();
		let retries: Vec<_> = pallet_scheduler::Retries::<T>::iter().collect();
		let lookup: Vec<_> = alias::Lookup::<T>::iter().collect();

		// (IncompleteSince, Agendas and their schedule's call encodings, Retries, Lookup)
		(incomplete_since, agenda_and_call_encodings, retries, lookup).encode()
	}

	fn post_check(_rc_pre_payload: Self::RcPrePayload) {
		// Assert storage 'Scheduler::IncompleteSince::rc_post::empty'
		assert!(
			pallet_scheduler::IncompleteSince::<T>::get().is_none(),
			"IncompleteSince should be None on RC after migration"
		);

		// Assert storage 'Scheduler::Agenda::rc_post::empty'
		assert!(
			alias::Agenda::<T>::iter().next().is_none(),
			"Agenda map should be empty on RC after migration"
		);

		// Assert storage 'Scheduler::Retries::rc_post::empty'
		assert!(
			pallet_scheduler::Retries::<T>::iter().next().is_none(),
			"Retries map should be empty on RC after migration"
		);

		// Assert storage 'Scheduler::Lookup::rc_post::empty'
		assert!(
			alias::Lookup::<T>::iter().next().is_none(),
			"Lookup map should be empty on RC after migration"
		);
	}
}

#[cfg(feature = "std")]
impl<T: Config> SchedulerMigrator<T> {
	// Convert all scheduled task calls to their Vec<u8> encodings, either directly or by grabbing
	// the preimage. Used for migration checks. Note: Does not return `Scheduled`, just the call
	// encodings.
	fn get_task_call_encodings(
		tasks: BoundedVec<
			Option<alias::ScheduledOf<T>>,
			<T as pallet_scheduler::Config>::MaxScheduledPerBlock,
		>,
	) -> Vec<Option<Vec<u8>>> {
		use frame_support::traits::{Bounded, QueryPreimage};

		// Convert based on Schedules existance and call type.
		tasks
			.into_inner()
			.into_iter()
			.map(|maybe_schedule| {
				maybe_schedule.and_then(|sched| match sched.call {
					// Inline. Grab inlined call.
					Bounded::Inline(bounded_call) => Some(bounded_call.into_inner()),
					// Lookup. Fetch preimage and store.
					Bounded::Lookup { hash, len } =>
						<pallet_preimage::Pallet<T> as QueryPreimage>::fetch(&hash, Some(len))
							.ok()
							.map(|preimage| preimage.into_owned()),
					// Legacy. Fetch preimage and store.
					Bounded::Legacy { hash, .. } =>
						<pallet_preimage::Pallet<T> as QueryPreimage>::fetch(&hash, None)
							.ok()
							.map(|preimage| preimage.into_owned()),
				})
			})
			.collect::<Vec<_>>()
	}
}
