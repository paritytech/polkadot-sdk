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

use super::*;
use frame_support::{assert_err, assert_noop, assert_ok};
use mock::{
	active_era, advance_blocks, start_session, ExtBuilder, RootOffences, RuntimeOrigin, System,
	Test as T,
};
use pallet_staking::asset;

#[test]
fn create_offence_fails_given_signed_origin() {
	use sp_runtime::traits::BadOrigin;
	ExtBuilder::default().build_and_execute(|| {
		let offenders = (&[]).to_vec();
		assert_err!(
			RootOffences::create_offence(RuntimeOrigin::signed(1), offenders, None, None),
			BadOrigin
		);
	})
}

#[test]
fn create_offence_works_given_root_origin() {
	ExtBuilder::default().build_and_execute(|| {
		start_session(1);

		assert_eq!(active_era(), 0);

		assert_eq!(asset::staked::<T>(&11), 1000);

		let offenders = [(11, Perbill::from_percent(50))].to_vec();
		assert_ok!(RootOffences::create_offence(
			RuntimeOrigin::root(),
			offenders.clone(),
			None,
			None
		));

		System::assert_last_event(Event::OffenceCreated { offenders }.into());

		// offence is processed in the following block.
		advance_blocks(1);

		// the slash should be applied right away.
		assert_eq!(asset::staked::<T>(&11), 500);

		// the other validator should keep their balance, because we only created
		// an offences for the first validator.
		assert_eq!(asset::staked::<T>(&21), 1000);
	})
}

#[test]
fn create_offence_wont_slash_non_active_validators() {
	ExtBuilder::default().build_and_execute(|| {
		start_session(1);

		assert_eq!(active_era(), 0);

		// we cannot even submit an offence for this, because we cannot generate an identification
		// for them.
		let offenders = [(31, Perbill::from_percent(20)), (11, Perbill::from_percent(20))].to_vec();
		assert_noop!(
			RootOffences::create_offence(RuntimeOrigin::root(), offenders.clone(), None, None),
			"failed to call FullIdentificationOf"
		);
	})
}

#[test]
fn create_offence_wont_slash_idle() {
	ExtBuilder::default().build_and_execute(|| {
		start_session(1);

		assert_eq!(active_era(), 0);

		// 41 is idle.
		assert_eq!(asset::staked::<T>(&41), 1000);

		// we cannot even submit an offence for this, because we cannot generate an identification
		// for them.
		let offenders = [(41, Perbill::from_percent(50))].to_vec();
		assert_noop!(
			RootOffences::create_offence(RuntimeOrigin::root(), offenders.clone(), None, None),
			"failed to call FullIdentificationOf"
		);
	})
}
