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

pub struct SchedulerMigrator<T: Config> {
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
		let mut messages = Vec::new();

		loop {
			if weight_counter
				.try_consume(<T as frame_system::Config>::DbWeight::get().reads_writes(1, 1))
				.is_err()
			{
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
		let mut messages = Vec::new();
		let mut ah_weight_counter = WeightMeter::with_limit(T::MaxAhWeight::get());

		let last_key = loop {
			if weight_counter
				.try_consume(<T as frame_system::Config>::DbWeight::get().reads_writes(1, 1))
				.is_err()
			{
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
