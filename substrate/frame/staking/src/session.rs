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
/// Something that manages the rotation of eras.
pub struct Manager<T: Config> {
	/// The session that is ending.
	pub end_session_index: SessionIndex,
	_phantom_data: PhantomData<T>,
}

impl<T: Config> Manager<T> {
	pub fn from(end_session_index: SessionIndex) -> Self {
		Manager { end_session_index, _phantom_data: Default::default() }
	}

	/// Returns the session that should be started.
	pub(crate) fn starting_session(&self) -> SessionIndex {
		self.end_session_index + 1
	}

	/// Returns the session that should be planned.
	pub(crate) fn planning_session(&self) -> SessionIndex {
		self.starting_session() + 1
	}

	/// Returns the planned session progress relative to the start of the era.
	pub(crate) fn planned_session_progress(&self) -> SessionIndex {
		let era_start_session =
			ErasStartSessionIndex::<T>::get(&self.planning_session()).unwrap_or(0);
		self.planning_session() - era_start_session
	}

	/// Returns `true` if an election should be kicked off.
	pub(crate) fn should_start_election(&self) -> bool {
		let session_progress = self.planned_session_progress();
		log!(debug, "RUNTIME IMPL: session progress: {:?}", session_progress);

		let election_offset = T::ElectionOffset::get().max(1).min(T::SessionsPerEra::get());

		// start the election `election_offset` sessions before the intended time.
		session_progress == (T::SessionsPerEra::get() - election_offset)
	}

	/// Infallible. Ends the session `end_session_index` and starts the next session.
	///
	/// There are three types of sessions:
	/// 1. Idle session: We are just waiting for enough sessions to pass.
	/// 2. Election kickoff session: We are about to start an election.
	/// 3. Era Rotation session: Sessions in which an activation timestamp of validator set is
	///   present.
	pub(crate) fn end_session(&self) {}

	/// Common work that needs to be done at the end of every session.
	fn do_common_session_end_work(&self) {
		// update the current planned session.
		CurrentPlannedSession::<T>::put(self.planning_session());
	}

	/// Starts an idle session.
	fn start_idle_session(&self) {
		self.do_common_session_end_work();
	}

	/// Starts the next session that will kick off an election.
	pub(crate) fn start_election_session(&self) {
		self.do_common_session_end_work();

		// kick off the election.
		log!(info, "sending election start signal");
		let _ = T::ElectionProvider::start();
		let new_planned_era = CurrentEra::<T>::mutate(|s| {
			*s = Some(s.map(|s| s + 1).unwrap_or(0));
			s.unwrap()
		});
		ErasStartSessionIndex::<T>::insert(&new_planned_era, &self.planning_session());

		self.clean_up_old_era(new_planned_era);
		Pallet::<T>::clear_election_metadata();
	}

	fn clean_up_old_era(&self, new_planned_era: EraIndex) {
		if let Some(old_era) = new_planned_era.checked_sub(T::HistoryDepth::get() + 1) {
			log!(trace, "Removing era information for {:?}", old_era);
			Pallet::<T>::clear_era_information(old_era);
		}
	}

	/// Starts the next session that would rotate the era.
	///
	/// Receives the activation timestamp `new_era_start` of the new validator set, i.e. the era
	/// start timestamp.
	fn start_rotation_era_session(&self, new_era_start: u64) {
		self.do_common_session_end_work();

		if let Some(current_active_era) = ActiveEra::<T>::get() {
			let previous_era_start = current_active_era.start.defensive_unwrap_or(new_era_start);
			let era_duration = new_era_start.saturating_sub(previous_era_start);
			Pallet::<T>::compute_era_payout(current_active_era, era_duration);
			Pallet::<T>::start_era(self.starting_session(), new_era_start);
		} else {
			defensive!("Active era must always be available.");
		}
	}
}
