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

//! Integration tests concerning the Ambassador Program.

use crate::*;
use collectives_polkadot_runtime::ambassador::AmbassadorSalaryPaymaster;
use frame_support::traits::{fungible::Mutate, tokens::Pay};
use sp_core::crypto::Ss58Codec;
use xcm_emulator::TestExt;

#[test]
fn pay_salary() {
	let pay_from: AccountId =
		<AccountId as Ss58Codec>::from_string("5DS1Gaf6R9eFAV8QyeZP9P89kTkJMurxv3y3J3TTMu8p8VCX")
			.unwrap();
	let pay_to = Polkadot::account_id_of(ALICE);
	let pay_amount = 90000000000;

	AssetHubPolkadot::execute_with(|| {
		type AssetHubBalances = <AssetHubPolkadot as AssetHubPolkadotPallet>::Balances;

		assert_ok!(<AssetHubBalances as Mutate<_>>::mint_into(&pay_from, pay_amount * 2));
	});

	Collectives::execute_with(|| {
		type RuntimeEvent = <Collectives as Chain>::RuntimeEvent;

		assert_ok!(AmbassadorSalaryPaymaster::pay(&pay_to, (), pay_amount));
		assert_expected_events!(
			Collectives,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
			]
		);
	});

	AssetHubPolkadot::execute_with(|| {
		type RuntimeEvent = <AssetHubPolkadot as Chain>::RuntimeEvent;

		assert_expected_events!(
			AssetHubPolkadot,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Transfer { from, to, amount }) => {
					from: from == &pay_from,
					to: to == &pay_to,
					amount: amount == &pay_amount,
				},
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Success { .. }) => {},
			]
		);
	});
}
