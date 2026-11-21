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

/// Put the staking state that the Asset Hub migration moves from the relay chain into AH.
///
/// The relay chain started era 1 in session 5, with validators 1, 2, 5 and 6.
pub fn put_migrated_staking_state() {
	// NOTE: this is not exhaustive, only the state that is needed for the tests.
	shared::in_ah(|| {
		pallet_staking_async::CurrentEra::<ah::Runtime>::set(Some(1));
		// Session 5 starts at RC block 150, and the RC timestamp is the block number * 1000.
		pallet_staking_async::ActiveEra::<ah::Runtime>::set(Some(
			pallet_staking_async::ActiveEraInfo { index: 1, start: Some(150_000) },
		));
		pallet_staking_async::BondedEras::<ah::Runtime>::set(
			vec![(0, 2), (1, 5)].try_into().unwrap(),
		);
		// 11 authored one block of era 1 before the migration started.
		pallet_staking_async::ErasRewardPoints::<ah::Runtime>::insert(
			1,
			pallet_staking_async::EraRewardPoints {
				total: 20,
				individual: [(11, 20)]
					.into_iter()
					.collect::<std::collections::BTreeMap<_, _>>()
					.try_into()
					.unwrap(),
			},
		);

		let exposed_nominators: [(ah::AccountId, Vec<(ah::AccountId, ah::Balance)>); 4] = [
			(1, vec![(107, 49), (104, 53), (100, 85), (112, 100), (102, 100)]),
			(2, vec![(110, 62), (106, 57), (108, 100), (101, 62), (100, 15)]),
			(5, vec![(110, 38), (101, 38), (104, 47), (105, 100)]),
			(6, vec![(106, 43), (107, 51), (111, 100)]),
		];
		for (validator, nominators) in exposed_nominators {
			let own = 100;
			let others = nominators
				.into_iter()
				.map(|(who, value)| sp_staking::IndividualExposure { who, value })
				.collect::<Vec<_>>();
			let total = own + others.iter().map(|other| other.value).sum::<ah::Balance>();
			let (overview, pages) = sp_staking::Exposure { total, own, others }
				.into_pages(ah::MaxExposurePageSize::get());
			pallet_staking_async::ErasStakersOverview::<ah::Runtime>::insert(
				1, validator, overview,
			);
			for (page, exposure_page) in pages.into_iter().enumerate() {
				pallet_staking_async::ErasStakersPaged::<ah::Runtime>::insert(
					(1, validator, page as sp_staking::Page),
					exposure_page,
				);
			}
		}
	})
}
