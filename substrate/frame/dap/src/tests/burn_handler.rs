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

//! BurnHandler tests for the DAP pallet.

use crate::mock::*;
use frame_support::traits::{fungible::Inspect, tokens::BurnHandler};

type DapPallet = crate::Pallet<Test>;

#[test]
fn on_burned_credits_buffer_and_accumulates() {
	new_test_ext().execute_with(|| {
		let buffer = DapPallet::buffer_account();
		let ed = <Balances as Inspect<_>>::minimum_balance();

		// Given: buffer has ED (funded at genesis).
		assert_eq!(Balances::free_balance(buffer), ed);

		// When: multiple burns occur (including zero amount which is a no-op).
		<DapPallet as BurnHandler<_, _>>::on_burned(&1u64, 0);
		<DapPallet as BurnHandler<_, _>>::on_burned(&1u64, 100);
		<DapPallet as BurnHandler<_, _>>::on_burned(&2u64, 200);
		<DapPallet as BurnHandler<_, _>>::on_burned(&3u64, 300);

		// Then: buffer has ED + 600 (zero amount ignored, others accumulated).
		assert_eq!(Balances::free_balance(buffer), ed + 600);
	});
}
