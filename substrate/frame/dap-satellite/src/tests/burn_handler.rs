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

//! BurnHandler tests for the DAP Satellite pallet.

use crate::mock::*;
use frame_support::{
	assert_ok,
	traits::{fungible::Inspect, tokens::BurnHandler},
};
use frame_system::RawOrigin;

type DapSatellitePallet = crate::Pallet<Test>;

#[test]
fn on_burned_credits_satellite_and_accumulates() {
	new_test_ext().execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: satellite has ED (funded at genesis).
		assert_eq!(Balances::free_balance(satellite), ed);
		let initial_active = <Balances as Inspect<_>>::active_issuance();

		// When: multiple burns occur (including zero amount which is a no-op).
		<DapSatellitePallet as BurnHandler<_>>::on_burned(0);
		<DapSatellitePallet as BurnHandler<_>>::on_burned(100);
		<DapSatellitePallet as BurnHandler<_>>::on_burned(200);
		<DapSatellitePallet as BurnHandler<_>>::on_burned(300);

		// Then: satellite has ED + 600 (zero amount ignored, others accumulated).
		assert_eq!(Balances::free_balance(satellite), ed + 600);

		// And: active issuance decreased by 600 (funds deactivated).
		assert_eq!(<Balances as Inspect<_>>::active_issuance(), initial_active - 600);
	});
}

#[test]
fn on_burned_handles_overflow_gracefully() {
	new_test_ext().execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();

		// Given: satellite is set to near-max balance via force_set_balance.
		let near_max = u64::MAX - 100;
		assert_ok!(Balances::force_set_balance(RawOrigin::Root.into(), satellite, near_max));
		assert_eq!(Balances::free_balance(satellite), near_max);

		// When: burn would cause overflow (near_max + 200 > u64::MAX).
		// This should NOT panic due to defensive handling with Precision::BestEffort.
		<DapSatellitePallet as BurnHandler<_>>::on_burned(200);

		// Then: satellite balance should be capped at max balance.
		let final_balance = Balances::free_balance(satellite);
		assert!(final_balance == u64::MAX, "Final balance should be equal to max balance");
	});
}
