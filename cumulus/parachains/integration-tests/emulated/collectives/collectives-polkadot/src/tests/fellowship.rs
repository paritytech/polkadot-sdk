// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Integration tests concerning the Fellowship.

#[test]
#[ignore]
fn pay_salary() {
	// let asset_id: u32 = 1984;
	// let pay_from: AccountId =
	// 	<AccountId as Ss58Codec>::from_string("13w7NdvSR1Af8xsQTArDtZmVvjE8XhWNdL4yed3iFHrUNCnS")
	// 		.unwrap();
	// let pay_to = Polkadot::account_id_of(ALICE);
	// let pay_amount = 9000;

	// AssetHubPolkadot::execute_with(|| {
	// 	type AssetHubAssets = <AssetHubPolkadot as AssetHubPolkadotPallet>::Assets;

	// 	assert_ok!(<AssetHubAssets as Create<_>>::create(
	// 		asset_id,
	// 		pay_to.clone(),
	// 		true,
	// 		pay_amount / 2
	// 	));
	// 	assert_ok!(<AssetHubAssets as Mutate<_>>::mint_into(asset_id, &pay_from, pay_amount * 2));
	// });

	// Collectives::execute_with(|| {
	// 	type RuntimeEvent = <Collectives as Chain>::RuntimeEvent;

	// 	assert_ok!(FellowshipSalaryPaymaster::pay(&pay_to, (), pay_amount));
	// 	assert_expected_events!(
	// 		Collectives,
	// 		vec![
	// 			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
	// 		]
	// 	);
	// });

	// AssetHubPolkadot::execute_with(|| {
	// 	type RuntimeEvent = <AssetHubPolkadot as Chain>::RuntimeEvent;

	// 	assert_expected_events!(
	// 		AssetHubPolkadot,
	// 		vec![
	// 			RuntimeEvent::Assets(pallet_assets::Event::Transferred { asset_id: id, from, to, amount }) =>
	// { 				asset_id: id == &asset_id,
	// 				from: from == &pay_from,
	// 				to: to == &pay_to,
	// 				amount: amount == &pay_amount,
	// 			},
	// 			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Success { .. }) => {},
	// 		]
	// 	);
	// });
}
