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

use crate::*;
use frame_support::traits::DefensiveTruncateFrom;
use pallet_rc_migrator::scheduler::{alias::Scheduled, RcSchedulerMessage};

/// Messages sent from the RC Migrator concerning the Scheduler pallet.
pub type RcSchedulerMessageOf<T> = RcSchedulerMessage<BlockNumberFor<T>>;

/// Relay Chain `Scheduled` type.
// From https://github.com/paritytech/polkadot-sdk/blob/f373af0d1c1e296c1b07486dd74710b40089250e/substrate/frame/scheduler/src/lib.rs#L203
pub type RcScheduledOf<T> =
	Scheduled<call::BoundedCallOf<T>, BlockNumberFor<T>, <T as Config>::RcPalletsOrigin>;

impl<T: Config> Pallet<T> {
	pub fn do_receive_scheduler_messages(
		messages: Vec<RcSchedulerMessageOf<T>>,
	) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Processing {} scheduler messages", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::Scheduler,
			count: messages.len() as u32,
		});
		let (mut count_good, mut count_bad) = (0, 0);

		for message in messages {
			match Self::do_process_scheduler_message(message) {
				Ok(()) => count_good += 1,
				Err(_) => count_bad += 1,
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::Scheduler,
			count_good,
			count_bad,
		});
		log::info!(target: LOG_TARGET, "Processed {} scheduler messages", count_good);

		Ok(())
	}

	fn do_process_scheduler_message(message: RcSchedulerMessageOf<T>) -> Result<(), Error<T>> {
		log::debug!(target: LOG_TARGET, "Processing scheduler message: {:?}", message);

		match message {
			RcSchedulerMessage::IncompleteSince(block_number) => {
				pallet_scheduler::IncompleteSince::<T>::put(block_number);
			},
			RcSchedulerMessage::Retries((task_address, retry_config)) => {
				pallet_scheduler::Retries::<T>::insert(task_address, retry_config);
			},
			RcSchedulerMessage::Lookup((task_name, task_address)) => {
				pallet_rc_migrator::scheduler::alias::Lookup::<T>::insert(task_name, task_address);
			},
		}

		Ok(())
	}

	pub fn do_receive_scheduler_agenda_messages(
		messages: Vec<(BlockNumberFor<T>, Vec<Option<RcScheduledOf<T>>>)>,
	) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Processing {} scheduler agenda messages", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::SchedulerAgenda,
			count: messages.len() as u32,
		});
		let (count_good, mut count_bad) = (messages.len() as u32, 0);

		for (block_number, agenda) in messages {
			let mut ah_tasks = Vec::new();
			for task in agenda {
				let Some(task) = task else {
					continue;
				};

				let origin = match T::RcToAhPalletsOrigin::try_convert(task.origin.clone()) {
					Ok(origin) => origin,
					Err(_) => {
						// we map all existing cases and do not expect this to happen.
						defensive!("Failed to convert scheduler call origin: {:?}", task.origin);
						count_bad += 1;
						continue;
					},
				};
				let Ok(call) = Self::map_rc_ah_call(&task.call) else {
					log::error!(
						target: LOG_TARGET,
						"Failed to convert RC call to AH call for task at block number {:?}",
						block_number
					);
					count_bad += 1;
					continue;
				};

				let task = Scheduled {
					maybe_id: task.maybe_id,
					priority: task.priority,
					call,
					maybe_periodic: task.maybe_periodic,
					origin,
				};

				ah_tasks.push(Some(task));
			}

			if ah_tasks.len() > 0 {
				let ah_tasks =
					BoundedVec::<_, T::MaxScheduledPerBlock>::defensive_truncate_from(ah_tasks);
				pallet_rc_migrator::scheduler::alias::Agenda::<T>::insert(block_number, ah_tasks);
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::SchedulerAgenda,
			count_good,
			count_bad,
		});
		log::info!(target: LOG_TARGET, "Processed {} scheduler agenda messages", count_good);

		Ok(())
	}
}
