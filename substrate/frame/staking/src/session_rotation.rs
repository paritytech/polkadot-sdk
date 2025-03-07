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
//! First 5 sessions are idle. Nothing much happens in these sessions.
//!
//! **Actions**
//! - Increment the session index in `CurrentPlannedSession`.
//!
//! ## Planning Session
//! We kick this off the planning session in the 6th planning session.
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
//! ## Era Rotation Session
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
//! **Scenarios**
//! - Happy Path: Triggered in the 7th session.
//! - Delay in exporting validator set: Triggered in a session later than 7th.
//! - Forcing Era: May triggered in a session earlier than 7th.
//!

use crate::{
	log, ActiveEra, Config, CurrentEra, CurrentPlannedSession, EraIndex, ErasStartSessionIndex,
	Pallet,
};
use frame_election_provider_support::ElectionProvider;
use frame_support::{pallet_prelude::*, traits::Defensive};
use sp_staking::SessionIndex;

#[derive(
	Encode, Decode, DecodeWithMemTracking, Debug, Clone, PartialEq, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
/// Manages session rotation logic.
///
/// This struct handles the following operations:
/// - `plan_new_session()`: Ends session `n`, activates session `n+1`, and plans session `n+2`.
/// - `plan_new_era()`: Plan the next era, which is targeted to activate at the end of the next
/// session.
/// - `activate_new_era()`: Finalizes the previous era and activates the planned era.
pub struct Rotator<T: Config> {
	/// The session that is ending.
	pub end_session_index: SessionIndex,
	_phantom_data: PhantomData<T>,
}

impl<T: Config> Rotator<T> {
	pub(crate) fn from(end_session_index: SessionIndex) -> Self {
		Rotator { end_session_index, _phantom_data: Default::default() }
	}

	/// Returns the session that should be activated.
	pub(crate) fn activating_session(&self) -> SessionIndex {
		self.end_session_index + 1
	}

	/// Returns the session that should be planned.
	pub(crate) fn planning_session(&self) -> SessionIndex {
		self.activating_session() + 1
	}

	/// Returns the planned session progress relative to the first planned session of the era.
	pub(crate) fn planned_session_progress(&self) -> SessionIndex {
		let era_start_session =
			ErasStartSessionIndex::<T>::get(&self.planning_session()).unwrap_or(0);
		self.planning_session() - era_start_session
	}

	/// Returns the session index at which we should start planning for the new era
	pub(crate) fn next_planning_era(&self) -> SessionIndex {
		let election_offset = T::ElectionOffset::get().max(1).min(T::SessionsPerEra::get());
		T::SessionsPerEra::get().saturating_sub(election_offset)
	}

	/// Plans the next session that will begin after the starting session.
	///
	/// This means:
	/// - The current session `n` is ending.
	/// - The next session `n+1` is activating.
	/// - The session after that, `n+2`, is now planned.
	pub(crate) fn plan_new_session(&self) {
		CurrentPlannedSession::<T>::put(self.planning_session());
	}

	/// Activates a new era with the given `new_era_start` timestamp.
	///
	/// This process includes:
	/// - Finalizing the current active era by computing staking payouts.
	/// - Rolling over to the next era to maintain synchronization in the staking system.
	pub(crate) fn activate_new_era(&self, new_era_start: u64) {
		debug_assert!(CurrentEra::<T>::get().unwrap() == ActiveEra::<T>::get().unwrap().index + 1);
		if let Some(current_active_era) = ActiveEra::<T>::get() {
			let previous_era_start = current_active_era.start.defensive_unwrap_or(new_era_start);
			let era_duration = new_era_start.saturating_sub(previous_era_start);
			Pallet::<T>::compute_era_payout(current_active_era, era_duration);
			Pallet::<T>::start_era(self.activating_session(), new_era_start);
		} else {
			defensive!("Active era must always be available.");
		}
	}

	/// Plans a new era by kicking off the election process.
	///
	/// The newly planned era is targeted to activate in the next session.
	pub(crate) fn plan_new_era(&self) {
		// todo: send this as id for the validator set.
		let new_planned_era = CurrentEra::<T>::mutate(|s| {
			*s = Some(s.map(|s| s + 1).unwrap_or(0));
			s.unwrap()
		});

		// this seems a good time for elections.
		log!(info, "sending election start signal");
		let _ = T::ElectionProvider::start();

		Pallet::<T>::clear_election_metadata();
		// discard the ancient era info.
		if let Some(old_era) = new_planned_era.checked_sub(T::HistoryDepth::get() + 1) {
			log!(trace, "Removing era information for {:?}", old_era);
			Pallet::<T>::clear_era_information(old_era);
		}

		log!(debug, "done planning new era: {:?}", new_planned_era);
	}
}
