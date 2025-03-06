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

use crate::{log, Config, CurrentPlannedSession, ErasStartSessionIndex};
use frame_support::{__private::sp_std, pallet_prelude::*};
use sp_staking::SessionIndex;

#[derive(
	Encode, Decode, DecodeWithMemTracking, Debug, Clone, PartialEq, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
/// Something that manages the rotation of eras.
pub struct EraRotator<T: Config> {
	/// The session that is ending.
	pub end_session_index: SessionIndex,
	/// If none, it means no new validator set was activated as a part of this session.
	///
	/// If `Some((timestamp, id))`, it means that the new validator set was activated at the given
	/// timestamp, and the id of the validator set is `id`.
	pub activation_timestamp: Option<(u64, u32)>,
	_phantom_data: PhantomData<T>,
}

impl<T: Config> EraRotator<T> {
	pub fn from(report: pallet_staking_rc_client::SessionReport<T::AccountId>) -> Self {
		EraRotator {
			end_session_index: report.end_index,
			activation_timestamp: report.activation_timestamp,
			_phantom_data: Default::default(),
		}
	}

	/// Returns the session that should be started.
	fn starting_session(&self) -> SessionIndex {
		self.end_session_index + 1
	}

	/// Returns the session that should be planned.
	fn planning_session(&self) -> SessionIndex {
		self.starting_session() + 1
	}

	/// Returns the planned session progress relative to the start of the era.
	fn planned_session_progress(&self) -> SessionIndex {
		let era_start_session =
			ErasStartSessionIndex::<T>::get(&self.planning_session()).unwrap_or(0);
		self.planning_session() - era_start_session
	}

	/// Returns `true` if an election should be kicked off.
	fn should_start_election(&self) -> bool {
		let session_progress = self.planned_session_progress();
		log!(debug, "RUNTIME IMPL: session progress: {:?}", session_progress);

		let election_offset = T::ElectionOffset::get().max(1).min(T::SessionsPerEra::get());

		// start the election `election_offset` sessions before the intended time.
		session_progress >= (T::SessionsPerEra::get() - election_offset)
	}

	/// Infallible. Ends the session `end_session_index` and starts the next session.
	///
	/// There are three types of sessions:
	/// 1. Idle session: We are just waiting for enough sessions to pass.
	/// 2. Election kickoff session: We are about to start an election.
	/// 3. Era Rotation session: Sessions in which an activation timestamp of validator set is
	///   present.
	fn end_session(&self) {
		self.do_common_session_end_work();

		if self.activation_timestamp.is_some() {
			// rotate era session.
			self.rotate_era();
			return;
		}

		let session_progress = self.planned_session_progress();
		let election_offset = T::ElectionOffset::get().max(1).min(T::SessionsPerEra::get());
		let election_start_session = T::SessionsPerEra::get() - election_offset;

		if session_progress < election_start_session {
			self.start_idle_session()
		} else {
			self.start_election_session()
		}
	}

	/// Common work that needs to be done at the end of every session.
	fn do_common_session_end_work(&self) {
		// update the current planned session.
		CurrentPlannedSession::<T>::put(self.planning_session());
	}

	/// Starts an idle session.
	fn start_idle_session(&self) {}

	/// Starts the next session that will kick off an election.
	fn start_election_session(&self) {}

	/// Starts the next session that would rotate the era.
	fn rotate_era(&self) {}
}
