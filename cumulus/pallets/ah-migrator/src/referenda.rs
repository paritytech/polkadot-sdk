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
use pallet_rc_migrator::referenda::ReferendaMigrator;
use pallet_referenda::{
	BalanceOf, DecidingCount, MetadataOf, ReferendumCount, ReferendumIndex, ReferendumInfo,
	ReferendumInfoFor, ReferendumStatus, ScheduleAddressOf, TallyOf, TrackIdOf, TrackQueue,
	VotesOf,
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
pub type AhReferendumInfoOf<T, I> = ReferendumInfo<
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
pub type AhReferendumStatusOf<T, I> = ReferendumStatus<
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

		let referendum: AhReferendumInfoOf<T, ()> = match referendum {
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

				let status = AhReferendumStatusOf::<T, ()> {
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
			MetadataOf::<T, ()>::insert(id, hash);
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
		AhReferendumInfoOf<T, ()>,
	>;
}
// TODO: shift referendums' time block by the time of the migration
// TODO: schedule `one_fewer_deciding` for referendums canceled during migration

// (ReferendumCount, DecidingCount, TrackQueue, MetadataOf, ReferendumInfoFor)
#[derive(Decode)]
pub struct RcPrePayload<T: Config> {
	pub referendum_count: ReferendumIndex,
	pub deciding_count: Vec<(TrackIdOf<T, ()>, u32)>,
	pub track_queue: Vec<(TrackIdOf<T, ()>, Vec<(ReferendumIndex, VotesOf<T, ()>)>)>,
	pub metadata: Vec<(ReferendumIndex, <T as frame_system::Config>::Hash)>,
	pub referenda: Vec<(ReferendumIndex, RcReferendumInfoOf<T, ()>)>,
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for ReferendaMigrator<T> {
	type RcPrePayload = Vec<u8>;
	type AhPrePayload = ();

	fn pre_check(_rc_pre_payload: Self::RcPrePayload) -> Self::AhPrePayload {
		// Assert storage 'Referenda::ReferendumCount::ah_pre::empty'
		assert_eq!(
			ReferendumCount::<T, ()>::get(),
			0,
			"Referendum count should be 0 on AH before the migration"
		);

		// Assert storage 'Referenda::DecidingCount::ah_pre::empty'
		assert!(
			DecidingCount::<T, ()>::iter().next().is_none(),
			"Deciding count map should be empty on AH before the migration"
		);

		// Assert storage 'Referenda::TrackQueue::ah_pre::empty'
		assert!(
			TrackQueue::<T, ()>::iter().next().is_none(),
			"Track queue map should be empty on AH before the migration"
		);

		// Assert storage 'Referenda::MetadataOf::ah_pre::empty'
		assert!(
			MetadataOf::<T, ()>::iter().next().is_none(),
			"MetadataOf map should be empty on AH before the migration"
		);

		// Assert storage 'Referenda::ReferendumInfoFor::ah_pre::empty'
		assert!(
			alias::ReferendumInfoFor::<T>::iter().next().is_none(),
			"Referendum info for map should be empty on AH before the migration"
		);

		()
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _ah_pre_payload: Self::AhPrePayload) {
		let rc_payload = RcPrePayload::<T>::decode(&mut &rc_pre_payload[..])
			.expect("Failed to decode RcPrePayload bytes");

		// Assert storage 'Referenda::ReferendumCount::ah_post::correct'
		// Assert storage 'Referenda::ReferendumCount::ah_post::consistent'
		assert_eq!(
			ReferendumCount::<T, ()>::get(),
			rc_payload.referendum_count,
			"ReferendumCount on AH post migration should match the pre migration RC value"
		);

		// Assert storage 'Referenda::DecidingCount::ah_post::length'
		assert_eq!(
			DecidingCount::<T, ()>::iter_keys().count() as u32,
			rc_payload.deciding_count.len() as u32,
			"DecidingCount length on AH post migration should match the pre migration RC length"
		);

		// Assert storage 'Referenda::DecidingCount::ah_post::correct'
		// Assert storage 'Referenda::DecidingCount::ah_post::consistent'
		assert_eq!(
			DecidingCount::<T, ()>::iter().collect::<Vec<_>>(),
			rc_payload.deciding_count,
			"DecidingCount on AH post migration should match the pre migration RC value"
		);

		// Assert storage 'Referenda::TrackQueue::ah_post::length'
		assert_eq!(
			TrackQueue::<T, ()>::iter_keys().count() as u32,
			rc_payload.track_queue.len() as u32,
			"TrackQueue length on AH post migration should match the pre migration RC length"
		);

		// Assert storage 'Referenda::TrackQueue::ah_post::correct'
		// Assert storage 'Referenda::TrackQueue::ah_post::consistent'
		assert_eq!(
			TrackQueue::<T, ()>::iter()
				.map(|(track_id, queue)| (track_id, queue.into_inner()))
				.collect::<Vec<_>>(),
			rc_payload.track_queue,
			"TrackQueue on AH post migration should match the pre migration RC value"
		);

		// Assert storage 'Referenda::MetadataOf::ah_post::length'
		assert_eq!(
			MetadataOf::<T, ()>::iter_keys().count() as u32,
			rc_payload.metadata.len() as u32,
			"MetadataOf length on AH post migration should match the pre migration RC length"
		);

		// Assert storage 'Referenda::MetadataOf::ah_post::correct'
		// Assert storage 'Referenda::MetadataOf::ah_post::consistent'
		assert_eq!(
			MetadataOf::<T, ()>::iter().collect::<Vec<_>>(),
			rc_payload.metadata,
			"MetadataOf on AH post migration should match the pre migration RC value"
		);

		// --- ReferendumInfoOf checks, some special reconstruction logic required ---

		// Function to convert a single RC ReferendumInfo to its expected AH form.
		// A whittled version of `do_receive_referendum` used for the actual migration above ^.
		fn convert_rc_to_ah_referendum<T: Config>(
			rc_info: RcReferendumInfoOf<T, ()>,
		) -> AhReferendumInfoOf<T, ()> {
			match rc_info {
				ReferendumInfo::Ongoing(rc_status) => {
					// --- Mimic do_receive_referendum logic ---
					let ah_origin =
						match T::RcToAhPalletsOrigin::try_convert(rc_status.origin.clone()) {
							Ok(origin) => origin,
							Err(_) => {
								// Origin conversion failed, return cancelled.
								let now = frame_system::Pallet::<T>::block_number();
								return AhReferendumInfoOf::<T, ()>::Cancelled(
									now,
									Some(rc_status.submission_deposit),
									rc_status.decision_deposit,
								);
							},
						};

					let ah_proposal = match crate::Pallet::<T>::map_rc_ah_call(&rc_status.proposal)
					{
						Ok(proposal) => proposal,
						Err(_) => {
							// Call conversion failed, return cancelled.
							let now = frame_system::Pallet::<T>::block_number();
							return AhReferendumInfoOf::<T, ()>::Cancelled(
								now,
								Some(rc_status.submission_deposit),
								rc_status.decision_deposit,
							);
						},
					};

					// Construct the AH status using converted parts
					let ah_status = AhReferendumStatusOf::<T, ()> {
						track: rc_status.track,
						origin: ah_origin,     // Use converted origin
						proposal: ah_proposal, // Use converted proposal
						enactment: rc_status.enactment,
						submitted: rc_status.submitted,
						submission_deposit: rc_status.submission_deposit,
						decision_deposit: rc_status.decision_deposit,
						deciding: rc_status.deciding,
						tally: rc_status.tally,
						in_queue: rc_status.in_queue,
						alarm: rc_status.alarm,
					};
					AhReferendumInfoOf::<T, ()>::Ongoing(ah_status)
				},
				ReferendumInfo::Approved(a, b, c) => AhReferendumInfoOf::<T, ()>::Approved(a, b, c),
				ReferendumInfo::Rejected(a, b, c) => AhReferendumInfoOf::<T, ()>::Rejected(a, b, c),
				ReferendumInfo::Cancelled(a, b, c) =>
					AhReferendumInfoOf::<T, ()>::Cancelled(a, b, c),
				ReferendumInfo::TimedOut(a, b, c) => AhReferendumInfoOf::<T, ()>::TimedOut(a, b, c),
				ReferendumInfo::Killed(a) => AhReferendumInfoOf::<T, ()>::Killed(a),
			}
		}

		// Check if referendums are equal, ignoring the `Moment` field when comparing
		// `ReferendumInfo::Cancelled`s as block numbers are different during the actual
		// migration and the reconstruction here.
		fn referendums_equal<T: Config>(
			ref1: &AhReferendumInfoOf<T, ()>,
			ref2: &AhReferendumInfoOf<T, ()>,
		) -> bool {
			match (ref1, ref2) {
				// Special case: Cancelled vs Cancelled.
				(
					AhReferendumInfoOf::<T, ()>::Cancelled(_moment1, sd1, dd1),
					AhReferendumInfoOf::<T, ()>::Cancelled(_moment2, sd2, dd2),
				) => {
					// Compare only the deposits
					sd1 == sd2 && dd1 == dd2
				},

				// Other enum variants.
				(ref1_variant, ref2_variant)
					if core::mem::discriminant(ref1_variant) ==
						core::mem::discriminant(ref2_variant) =>
					ref1 == ref2,

				// Variants are different (e.g., Ongoing vs Approved), they are not equal.
				_ => false,
			}
		}

		// Convert referenda from RcPrePayload to expected values.
		let mut expected_ah_referenda: Vec<_> = rc_payload
			.referenda
			.iter()
			.map(|(ref_index, referenda)| {
				(*ref_index, convert_rc_to_ah_referendum::<T>(referenda.clone()))
			})
			.collect();
		// Grab values on AH.
		let mut current_ah_referenda = alias::ReferendumInfoFor::<T>::iter().collect::<Vec<_>>();

		// Assert storage 'Referenda::ReferendumInfoFor::ah_post::length'
		assert_eq!(
			current_ah_referenda.len() as u32,
			expected_ah_referenda.len() as u32,
			"ReferendumInfoFor length on AH post migration should match the RC length post conversion"
		);

		// Ensure no ordering issues between original and reconstruction.
		current_ah_referenda.sort_by_key(|(index, _)| *index);
		expected_ah_referenda.sort_by_key(|(index, _)| *index);

		// Assert storage 'Referenda::ReferendumInfoFor::ah_post::correct'
		for i in 0..current_ah_referenda.len() {
			assert!(
				referendums_equal::<T>(
					&current_ah_referenda[i].1,
					&expected_ah_referenda[i].1
				),
				"ReferendumInfoFor on AH post migration should match the RC value post conversion, mismatch for ref {}", current_ah_referenda[i].0
			);
		}
	}
}
