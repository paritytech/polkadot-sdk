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

use crate::imports::*;
use collectives_westend_runtime::{
	fellowship::FellowshipSalaryPaymaster, secretary::SecretarySalaryPaymaster,
};
use frame_support::{
	assert_ok,
	traits::{fungibles::Mutate, tokens::Pay},
};
use xcm_executor::traits::ConvertLocation;

const FELLOWSHIP_SALARY_PALLET_ID: u8 = 64;
const SECRETARY_SALARY_PALLET_ID: u8 = 91;

#[test]
fn pay_salary_technical_fellowship() {
	let asset_id: u32 = 1984;
	let fellowship_salary = (
		Parent,
		Parachain(CollectivesWestend::para_id().into()),
		PalletInstance(FELLOWSHIP_SALARY_PALLET_ID),
	);
	let pay_from =
		AssetHubLocationToAccountId::convert_location(&fellowship_salary.into()).unwrap();
	let pay_to = Westend::account_id_of(ALICE);
	let pay_amount = 9_000_000_000;

	AssetHubWestend::execute_with(|| {
		type AssetHubAssets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		assert_ok!(<AssetHubAssets as Mutate<_>>::mint_into(asset_id, &pay_from, pay_amount * 2));
	});

	CollectivesWestend::execute_with(|| {
		type RuntimeEvent = <CollectivesWestend as Chain>::RuntimeEvent;

		assert_ok!(FellowshipSalaryPaymaster::pay(&pay_to, (), pay_amount));
		assert_expected_events!(
			CollectivesWestend,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
			RuntimeEvent::Assets(pallet_assets::Event::Transferred { .. }) => {},
			RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
				]
		);
	});
}

#[test]
fn pay_salary_secretary() {
	const USDT_ID: u32 = 1984;
	let secretary_salary = (
		Parent,
		Parachain(CollectivesWestend::para_id().into()),
		PalletInstance(SECRETARY_SALARY_PALLET_ID),
	);
	let pay_from = AssetHubLocationToAccountId::convert_location(&secretary_salary.into()).unwrap();
	let pay_to = Westend::account_id_of(ALICE);
	let pay_amount = 9_000_000_000;

	AssetHubWestend::execute_with(|| {
		type AssetHubAssets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		// USDT registered in genesis, now mint some into the payer's account
		assert_ok!(<AssetHubAssets as Mutate<_>>::mint_into(USDT_ID, &pay_from, pay_amount * 2));
	});

	CollectivesWestend::execute_with(|| {
		type RuntimeEvent = <CollectivesWestend as Chain>::RuntimeEvent;

		assert_ok!(SecretarySalaryPaymaster::pay(&pay_to, (), pay_amount));
		assert_expected_events!(
			CollectivesWestend,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::Assets(pallet_assets::Event::Transferred { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
			]
		);
	});
}
