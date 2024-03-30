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

//! General tests that do not fit in any other of the specific files.

#![cfg(test)]

use crate::tests::*;
use frame_support::traits::Currency;
use frame_system::IncRefStatus;

#[test]
fn endowed_event_only_upon_balance_receival() {
	ExtBuilder::default().existential_deposit(10).build_and_execute_with(|| {
		let account = 0;

		assert_eq!(System::inc_sufficients(&account), IncRefStatus::Created);
		assert_eq!(events(), vec![frame_system::Event::<Test>::NewAccount { account }.into()]);

		let free_balance = 100;
		Balances::make_free_balance_be(&account, free_balance);
		// Endowed event is being emitted:
		assert_eq!(
			events(),
			vec![
				crate::Event::<Test>::BalanceSet { who: account, free: free_balance }.into(),
				crate::Event::<Test>::Endowed { account, free_balance }.into()
			]
		);
	});
}
