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
use frame_support::traits::{fungible::Inspect, tokens::BurnHandler};

type DapSatellitePallet = crate::Pallet<Test>;

#[test]
fn on_burned_credits_satellite_and_accumulates() {
	new_test_ext().execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: satellite has ED (funded at genesis).
		assert_eq!(Balances::free_balance(satellite), ed);

		// When: multiple burns occur (including zero amount which is a no-op).
		<DapSatellitePallet as BurnHandler<_, _>>::on_burned(&1u64, 0);
		<DapSatellitePallet as BurnHandler<_, _>>::on_burned(&1u64, 100);
		<DapSatellitePallet as BurnHandler<_, _>>::on_burned(&2u64, 200);
		<DapSatellitePallet as BurnHandler<_, _>>::on_burned(&3u64, 300);

		// Then: satellite has ED + 600 (zero amount ignored, others accumulated).
		assert_eq!(Balances::free_balance(satellite), ed + 600);
	});
}
