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

//! Manages all era rotation logic based on session increments.
//!
//! # Lifecycle:
//!
//! When a session ends in RC, a session report is sent to AH with the ending session index. Given
//! there are 6 sessions per Era, and we configure the PlanningEraOffset to be 1, the following
//! happens.
//!
//! ## Idle Sessions
//! In the happy path, first 3 sessions are idle. Nothing much happens in these sessions.
//!
//! **Actions**
//! - Increment the session index in `CurrentPlannedSession`.
//!
//!
//! ## Planning New Era Session
//! In the happy path, `planning new era` session is initiated when 3rd session ends and the 4th
//! starts in the active era.
//!
//! **Triggers**
//! 1. `SessionProgress == SessionsPerEra - PlanningEraOffset`
//! 2. Forcing is set to `ForceNew` or `ForceAlways`
//!
//! **Actions**
//! 1. Triggers the election process,
//! 2. Updates the CurrentEra.
//!
//! **SkipIf**
//! CurrentEra = ActiveEra + 1 // this implies planning session has already been triggered.
//!
//! **FollowUp**
//! When the election process is over, we send the new validator set, with the CurrentEra index
//! as the id of the validator set.
//!
//!
//! ## Era Rotation Session
//! In the happy path, this happens when the 5th session ends and the 6th starts in the active era.
//!
//! **Triggers**
//! When we receive an activation timestamp from RC.
//!
//! **Assertions**
//! 1. CurrentEra must be ActiveEra + 1.
//! 2. Id of the activation timestamp same as CurrentEra.
//!
//! **Actions**
//! - Finalize the currently active era.
//! - Increment ActiveEra by 1.
//! - Cleanup the old era information.
//! - Set ErasStartSessionIndex with the activating era index and starting session index.
//!
//! **Exceptional Scenarios**
//! - Delay in exporting validator set: Triggered in a session later than 7th.
//! - Forcing Era: May triggered in a session earlier than 7th.
//!
//! ## Example Flow of a happy path
//!
//! * end 0, start 1, plan 2
//! * end 1, start 2, plan 3
//! * end 2, start 3, plan 4
//! * end 3, start 4, plan 5 // `Plan new era` session. Current Era++. Trigger Election.
//! * **** Somewhere here: Election set is sent to RC, keyed with Current Era
//! * end 4, start 5, plan 6 // RC::session receives and queues this set.
//! * end 5, start 6, plan 7 // Session report contains activation timestamp with Current Era.

use crate::{
	log, ActiveEra, ActiveEraInfo, Config, CurrentEra, CurrentPlannedSession, EraIndex,
	ErasStartSessionIndex, Event, ForceEra, Forcing, Pallet,
};
use frame_election_provider_support::ElectionProvider;
use frame_support::{
	pallet_prelude::*,
	traits::{Defensive, DefensiveSaturating},
};
use sp_staking::SessionIndex;

#[derive(
	Encode, Decode, DecodeWithMemTracking, Debug, Clone, PartialEq, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]

/// Manages session rotation logic.
pub struct Rotator<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> Rotator<T> {
	/// End the session and start the next one.
	pub(crate) fn end_session(end_index: SessionIndex, activation_timestamp: Option<(u64, u32)>) {
		let Some(active_era) = ActiveEra::<T>::get() else {
			defensive!("Active era must always be available.");
			return;
		};
		let planned_era = CurrentEra::<T>::get().unwrap_or(0);
		let starting = end_index + 1;
		// the session after the starting session.
		let planning = starting + 1;

		log!(info, "Session: end {:?}, start {:?}, plan {:?}", end_index, starting, planning);
		log!(info, "Era: active {:?}, planned {:?}", active_era.index, planned_era);

		CurrentPlannedSession::<T>::mutate(|s| {
			// For the genesis session, we don't have any planned session.
			debug_assert!(*s == 0 || *s == starting, "Session must be sequential.");
			// Plan the next session.
			*s = planning
		});

		// We rotate the era if we have the activation timestamp.
		if let Some((time, id)) = activation_timestamp {
			// If the activation timestamp is provided, we are starting a new era.
			// fixme: RC is not sending correct id: debug_assert!(id == planned_era);
			Self::start_era(&active_era, starting, time);
		}

		// check if we should plan new era.
		let should_plan_era = match ForceEra::<T>::get() {
			// see if it's good time to plan a new era.
			Forcing::NotForcing => Self::is_plan_era_deadline(starting, active_era.index),
			// Force plan new era only once.
			Forcing::ForceNew => {
				ForceEra::<T>::put(Forcing::NotForcing);
				true
			},
			// always plan the new era.
			Forcing::ForceAlways => true,
			// never force.
			Forcing::ForceNone => false,
		};

		if should_plan_era {
			Self::plan_new_era(&active_era, planned_era, starting);
		}

		Pallet::<T>::deposit_event(Event::SessionRotated {
			starting_session: starting,
			active_era: ActiveEra::<T>::get().map(|a| a.index).defensive_unwrap_or(0),
			planned_era: CurrentEra::<T>::get().defensive_unwrap_or(0),
		});
	}

	fn start_era(ending_era: &ActiveEraInfo, starting_session: SessionIndex, new_era_start: u64) {
		// verify that a new era was planned
		debug_assert!(CurrentEra::<T>::get().unwrap_or(0) == ending_era.index + 1);

		let starting_era = ending_era.index + 1;

		// finalize the ending era.
		Self::end_era(&ending_era, new_era_start);

		// start the next era.
		Pallet::<T>::start_era(starting_session, new_era_start);

		// add the index to starting session so later we can compute the era duration in sessions.
		ErasStartSessionIndex::<T>::insert(starting_era, starting_session);

		// discard old era information that is no longer needed.
		Self::cleanup_old_era(starting_era);
	}

	fn end_era(ending_era: &ActiveEraInfo, new_era_start: u64) {
		let previous_era_start = ending_era.start.defensive_unwrap_or(new_era_start);
		let era_duration = new_era_start.saturating_sub(previous_era_start);
		Pallet::<T>::compute_era_payout(ending_era.clone(), era_duration);
	}

	/// Plans a new era by kicking off the election process.
	///
	/// The newly planned era is targeted to activate in the next session.
	// todo: handle `ForcingEra` scenario.
	fn plan_new_era(
		active_era: &ActiveEraInfo,
		planned_era: EraIndex,
		starting_session: SessionIndex,
	) {
		if planned_era == active_era.index + 1 {
			// era already planned, no need to plan again.
			return;
		}

		debug_assert!(planned_era == active_era.index);

		log!(debug, "Planning new era: {:?}", planned_era);
		CurrentEra::<T>::put(planned_era + 1);

		log!(info, "sending election start signal");
		let _ = T::ElectionProvider::start();
	}

	/// Returns whether we are at the session where we should plan the new era.
	fn is_plan_era_deadline(start_session: SessionIndex, active_era: EraIndex) -> bool {
		let election_offset = T::ElectionOffset::get().max(1).min(T::SessionsPerEra::get());
		// session at which we should plan the new era.
		let plan_era_session = T::SessionsPerEra::get().saturating_sub(election_offset);
		let era_start_session = ErasStartSessionIndex::<T>::get(&active_era).unwrap_or(0);

		// progress of the active era in sessions.
		let session_progress =
			start_session.saturating_add(1).defensive_saturating_sub(era_start_session);

		session_progress == plan_era_session
	}

	fn cleanup_old_era(starting_era: EraIndex) {
		Pallet::<T>::clear_election_metadata();

		// discard the ancient era info.
		if let Some(old_era) = starting_era.checked_sub(T::HistoryDepth::get() + 1) {
			log!(trace, "Removing era information for {:?}", old_era);
			Pallet::<T>::clear_era_information(old_era);
		}
	}
}
