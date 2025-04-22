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
use call::BoundedCallOf;
use frame_support::traits::{schedule::v3::Anon, DefensiveTruncateFrom};
use pallet_referenda::{
	BalanceOf, DecidingCount, ReferendumCount, ReferendumInfoFor, ReferendumStatus,
	ScheduleAddressOf, TallyOf, TrackIdOf, TrackQueue,
};

/// ReferendumInfoOf for RC.
///
/// The `RuntimeOrigin` is a type argument that needs to be mapped to AH `RuntimeOrigin`.
/// Inline `proposal`s and the ones stored by `Preimage` pallet should also be mapped to get the
/// final local `pallet_referenda::ReferendumInfoFor::<T, ()>`.
///
/// Reflects: `pallet_referenda::ReferendumInfoOf::<T, ()>`.
pub type RcReferendumInfoOf<T, I = ()> = ReferendumInfo<
	TrackIdOf<T, I>,
	<T as Config>::RcPalletsOrigin,
	BlockNumberFor<T>,
	BoundedCallOf<T>,
	BalanceOf<T, I>,
	TallyOf<T, I>,
	<T as frame_system::Config>::AccountId,
	ScheduleAddressOf<T, I>,
>;

/// RcReferendumStatusOf for RC.
///
/// Reflects: `pallet_referenda::ReferendumStatusOf::<T, ()>`.
pub type RcReferendumStatusOf<T, I> = ReferendumStatus<
	TrackIdOf<T, I>,
	<T as Config>::RcPalletsOrigin,
	BlockNumberFor<T>,
	BoundedCallOf<T>,
	BalanceOf<T, I>,
	TallyOf<T, I>,
	<T as frame_system::Config>::AccountId,
	ScheduleAddressOf<T, I>,
>;

/// Asset Hub ReferendumInfoOf.
pub type ReferendumInfoOf<T, I> = ReferendumInfo<
	TrackIdOf<T, I>,
	<<T as frame_system::Config>::RuntimeOrigin as OriginTrait>::PalletsOrigin,
	BlockNumberFor<T>,
	BoundedCallOf<T>,
	BalanceOf<T, I>,
	TallyOf<T, I>,
	<T as frame_system::Config>::AccountId,
	ScheduleAddressOf<T, I>,
>;

/// ReferendumStatusOf for Asset Hub.
pub type ReferendumStatusOf<T, I> = ReferendumStatus<
	TrackIdOf<T, I>,
	<<T as frame_system::Config>::RuntimeOrigin as OriginTrait>::PalletsOrigin,
	BlockNumberFor<T>,
	BoundedCallOf<T>,
	BalanceOf<T, I>,
	TallyOf<T, I>,
	<T as frame_system::Config>::AccountId,
	ScheduleAddressOf<T, I>,
>;

impl<T: Config> Pallet<T> {
	pub fn do_receive_referendums(
		referendums: Vec<(u32, RcReferendumInfoOf<T, ()>)>,
	) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Integrating {} referendums", referendums.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::ReferendaReferendums,
			count: referendums.len() as u32,
		});
		let (mut count_good, mut count_bad) = (0, 0);

		for (id, referendum) in referendums {
			match Self::do_receive_referendum(id, referendum) {
				Ok(()) => count_good += 1,
				Err(_) => count_bad += 1,
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::ReferendaReferendums,
			count_good,
			count_bad,
		});
		log::info!(target: LOG_TARGET, "Processed {} referendums", count_good);

		Ok(())
	}

	pub fn do_receive_referendum(
		id: u32,
		referendum: RcReferendumInfoOf<T, ()>,
	) -> Result<(), Error<T>> {
		log::debug!(target: LOG_TARGET, "Integrating referendum id: {}, info: {:?}", id, referendum);

		let referendum: ReferendumInfoOf<T, ()> = match referendum {
			ReferendumInfo::Ongoing(status) => {
				let cancel_referendum = |id, status: RcReferendumStatusOf<T, ()>| {
					if let Some((_, last_alarm)) = status.alarm {
						// TODO: scheduler migrated first?
						let _ = T::Scheduler::cancel(last_alarm);
					}
					// TODO: use referenda block provider
					let now = frame_system::Pallet::<T>::block_number();
					ReferendumInfoFor::<T, ()>::insert(
						id,
						ReferendumInfo::Cancelled(
							now,
							Some(status.submission_deposit),
							status.decision_deposit,
						),
					);
					log::error!(target: LOG_TARGET, "!!! Referendum {} cancelled", id);
				};

				let origin = match T::RcToAhPalletsOrigin::try_convert(status.origin.clone()) {
					Ok(origin) => origin,
					Err(_) => {
						defensive!(
							"Failed to convert RC origin to AH origin for referendum {}",
							id
						);
						cancel_referendum(id, status);
						return Ok(());
					},
				};

				let proposal = if let Ok(proposal) = Self::map_rc_ah_call(&status.proposal) {
					proposal
				} else {
					log::error!(target: LOG_TARGET, "Failed to convert RC call to AH call for referendum {}", id);
					cancel_referendum(id, status);
					return Ok(());
				};

				let status = ReferendumStatusOf::<T, ()> {
					track: status.track,
					origin,
					proposal,
					enactment: status.enactment,
					submitted: status.submitted,
					submission_deposit: status.submission_deposit,
					decision_deposit: status.decision_deposit,
					deciding: status.deciding,
					tally: status.tally,
					in_queue: status.in_queue,
					alarm: status.alarm,
				};

				ReferendumInfo::Ongoing(status)
			},
			ReferendumInfo::Approved(a, b, c) => ReferendumInfo::Approved(a, b, c),
			ReferendumInfo::Rejected(a, b, c) => ReferendumInfo::Rejected(a, b, c),
			ReferendumInfo::Cancelled(a, b, c) => ReferendumInfo::Cancelled(a, b, c),
			ReferendumInfo::TimedOut(a, b, c) => ReferendumInfo::TimedOut(a, b, c),
			ReferendumInfo::Killed(a) => ReferendumInfo::Killed(a),
		};

		alias::ReferendumInfoFor::<T>::insert(id, referendum);

		log::debug!(target: LOG_TARGET, "Referendum {} integrated", id);

		Ok(())
	}

	pub fn do_receive_referenda_metadata(
		metadata: Vec<(u32, <T as frame_system::Config>::Hash)>,
	) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Integrating {} metadata", metadata.len());
		let count = metadata.len() as u32;
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::ReferendaMetadata,
			count,
		});

		for (id, hash) in metadata {
			log::debug!(target: LOG_TARGET, "Integrating referendum {} metadata", id);
			pallet_referenda::MetadataOf::<T, ()>::insert(id, hash);
			log::debug!(target: LOG_TARGET, "Referendum {} integrated", id);
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::ReferendaMetadata,
			count_good: count,
			count_bad: 0,
		});
		log::info!(target: LOG_TARGET, "Processed {} metadata", count);

		Ok(())
	}

	pub fn do_receive_referenda_values(
		referendum_count: u32,
		deciding_count: Vec<(TrackIdOf<T, ()>, u32)>,
		track_queue: Vec<(TrackIdOf<T, ()>, Vec<(u32, u128)>)>,
	) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Integrating referenda pallet values");
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::ReferendaValues,
			count: 1,
		});

		ReferendumCount::<T, ()>::put(referendum_count);
		deciding_count.iter().for_each(|(track_id, count)| {
			DecidingCount::<T, ()>::insert(track_id, count);
		});
		track_queue.into_iter().for_each(|(track_id, queue)| {
			let queue = BoundedVec::<_, T::MaxQueued>::defensive_truncate_from(queue);
			TrackQueue::<T, ()>::insert(track_id, queue);
		});

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::ReferendaValues,
			count_good: 1,
			count_bad: 0,
		});
		log::info!(target: LOG_TARGET, "Referenda pallet values integrated");
		Ok(())
	}
}

pub mod alias {
	use super::*;
	use pallet_referenda::ReferendumIndex;

	/// Information concerning any given referendum.
	/// FROM: https://github.com/paritytech/polkadot-sdk/blob/f373af0d1c1e296c1b07486dd74710b40089250e/substrate/frame/referenda/src/lib.rs#L249
	#[frame_support::storage_alias(pallet_name)]
	pub type ReferendumInfoFor<T: pallet_referenda::Config<()>> = StorageMap<
		pallet_referenda::Pallet<T, ()>,
		Blake2_128Concat,
		ReferendumIndex,
		ReferendumInfoOf<T, ()>,
	>;
}
// TODO: shift referendums' time block by the time of the migration
// TODO: schedule `one_fewer_deciding` for referendums canceled during migration
