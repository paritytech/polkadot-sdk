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

use crate::*;
use frame::testing_prelude::*;
use std::cell::UnsafeCell;

thread_local! {
	pub static RC_STATE: UnsafeCell<TestState> = UnsafeCell::new(Default::default());
	pub static AH_STATE: UnsafeCell<TestState> = UnsafeCell::new(Default::default());
}

parameter_types! {
	// counts how many times a new offence message is sent from RC -> AH.
	pub static CounterRCAHNewOffence: u32 = 0;
	// counts how many times a new session report is sent from RC -> AH.
	pub static CounterRCAHSessionReport: u32 = 0;
	// counts how many times a validator set is sent to RC.
	pub static CounterAHRCValidatorSet: u32 = 0;
}

pub fn put_ah_state(ah: TestState) {
	AH_STATE.with(|state| unsafe {
		let ptr = state.get();
		*ptr = ah;
	})
}

pub fn in_ah(f: impl FnMut() -> ()) {
	AH_STATE.with(|state| unsafe {
		let ptr = state.get();
		(*ptr).execute_with(f)
	})
}

pub fn put_rc_state(rc: TestState) {
	RC_STATE.with(|state| unsafe {
		let ptr = state.get();
		*ptr = rc;
	})
}

pub fn in_rc(f: impl FnMut() -> ()) {
	RC_STATE.with(|state| unsafe {
		let ptr = state.get();
		(*ptr).execute_with(f)
	})
}

pub fn migrate_state() {
	// NOTE: this is not exhaustive, only migrates the state that is needed for the tests.
	shared::in_rc(|| {
		let current_era = pallet_staking::CurrentEra::<rc::Runtime>::take();
		let active_era = pallet_staking::ActiveEra::<rc::Runtime>::take().unwrap();
		shared::in_ah(|| {
			pallet_staking_async::CurrentEra::<ah::Runtime>::set(current_era);
			pallet_staking_async::ActiveEra::<ah::Runtime>::set(Some(
				pallet_staking_async::ActiveEraInfo {
					index: active_era.index,
					start: active_era.start,
				},
			));
		});

		for (era, reward_points) in pallet_staking::ErasRewardPoints::<rc::Runtime>::drain() {
			shared::in_ah(|| {
				pallet_staking_async::ErasRewardPoints::<ah::Runtime>::insert(
					era,
					pallet_staking_async::EraRewardPoints {
						total: reward_points.total,
						individual: reward_points.individual.clone(),
					},
				)
			});
		}

		// exposure
		for (era, account, overview) in pallet_staking::ErasStakersOverview::<rc::Runtime>::drain()
		{
			shared::in_ah(|| {
				pallet_staking_async::ErasStakersOverview::<ah::Runtime>::insert(
					era, account, overview,
				)
			});
		}

		for ((era, account, page), exposure_page) in
			pallet_staking::ErasStakersPaged::<rc::Runtime>::drain()
		{
			shared::in_ah(|| {
				pallet_staking_async::ErasStakersPaged::<ah::Runtime>::insert(
					(era, account, page),
					exposure_page.clone(),
				)
			});
		}

		for (era, session_index) in pallet_staking::ErasStartSessionIndex::<rc::Runtime>::drain() {
			shared::in_ah(|| {
				pallet_staking_async::ErasStartSessionIndex::<ah::Runtime>::insert(
					era,
					session_index,
				)
			});
		}
	})
}
