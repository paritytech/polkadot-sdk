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

//! Genesis tests for the DAP Satellite pallet.

use crate::mock::*;
use frame_support::sp_runtime::traits::AccountIdConversion;

type DapSatellitePallet = crate::Pallet<Test>;

#[test]
fn satellite_account_is_derived_from_pallet_id() {
	new_test_ext().execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();
		let expected: u64 = DapSatellitePalletId::get().into_account_truncating();
		assert_eq!(satellite, expected);
	});
}

#[test]
fn genesis_creates_satellite_account_with_ed() {
	new_test_ext().execute_with(|| {
		let satellite = DapSatellitePallet::satellite_account();
		// Satellite account should exist after genesis and be funded with ED
		assert!(System::account_exists(&satellite));
		// ED is 1 in TestDefaultConfig
		assert_eq!(Balances::free_balance(satellite), 1);
	});
}
