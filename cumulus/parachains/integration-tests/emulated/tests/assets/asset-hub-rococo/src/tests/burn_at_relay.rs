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
use emulated_integration_tests_common::accounts;

/// Create asset and then remove privileges; this results in the burning of the deposit, which must
/// burn at the Relay Chain.
#[test]
fn create_asset_remove_privileges_burn_deposit_at_relay() {
	let owner = AssetHubRococo::account_id_of(accounts::CHARLIE);
	let asset_id = 3939;

	// We need first to teleport some balance so that it can be withdrawn later on the Relay
	// Chain. Otherwise the asset is not withdrawable due to how genesis is set up.
	Rococo::execute_with(|| {
		type XcmPallet = <Rococo as RococoPallet>::XcmPallet;

		assert_ok!(XcmPallet::limited_teleport_assets(
			<Rococo as Chain>::RuntimeOrigin::signed(Rococo::account_id_of(accounts::CHARLIE)),
			bx!(Parachain(AssetHubRococo::para_id().into()).into()),
			bx!((*AsRef::<[u8; 32]>::as_ref(&owner)).into()),
			bx!(Assets::from((Here, 1_000_000_000_000u128)).into()),
			0,
			WeightLimit::Unlimited,
		));
	});

	let original_total_issuance =
		Rococo::execute_with(|| <Rococo as RococoPallet>::Balances::total_issuance());

	let deposit = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::Assets;
		type Balances = <AssetHubRococo as AssetHubRococoPallet>::Balances;

		let original_reserved_balance = Balances::reserved_balance(owner.clone());

		assert_ok!(Assets::create(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(owner.clone()),
			asset_id.into(),
			owner.clone().into(),
			1,
		));
		assert_ok!(Assets::set_metadata(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(owner.clone()),
			asset_id.into(),
			vec![],
			vec![],
			0,
		));

		let new_reserved_balance = Balances::reserved_balance(owner.clone());
		let original_total_issuance = Balances::total_issuance();

		let deposit = new_reserved_balance - original_reserved_balance;
		assert!(deposit != 0); // This ensures we are testing something.

		assert_ok!(Assets::revoke_all_privileges(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(owner.clone()),
			asset_id.into(),
		));

		assert_eq!(deposit, original_total_issuance - Balances::total_issuance());

		deposit
	});

	let new_total_issuance =
		Rococo::execute_with(|| <Rococo as RococoPallet>::Balances::total_issuance());

	assert_eq!(deposit, original_total_issuance - new_total_issuance);
}
