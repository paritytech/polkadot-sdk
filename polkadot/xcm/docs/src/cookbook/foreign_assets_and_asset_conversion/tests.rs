// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::{
	asset_para,
	network::{AssetPara, MockNet, SimplePara, ASSET_PARA_ID, SIMPLE_PARA_ID, UNITS},
	simple_para,
};
use frame::{prelude::fungible::Mutate, testing_prelude::*};
use test_log::test;
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;
use xcm_simulator::TestExt;

#[docify::export]
#[test]
fn registering_foreign_assets_work() {
	// We restart the mock network.
	MockNet::reset();

	let simple_para_sovereign = asset_para::xcm_config::LocationToAccountId::convert_location(
		&Location::new(1, Parachain(SIMPLE_PARA_ID)),
	)
	.expect("Can convert");

	AssetPara::execute_with(|| {
		assert_ok!(asset_para::Balances::mint_into(&simple_para_sovereign, 10 * UNITS));
		assert_eq!(asset_para::Balances::free_balance(&simple_para_sovereign), 10 * UNITS);

		// clear events that we do not want later.
		asset_para::System::reset_events();
	});

	// Step 1. Create the asset on the target chain and set its metadata.

	SimplePara::execute_with(|| {
		let xcm = Xcm(vec![
			// We have free execution on the target chain, but usually we need
			// a Withdraw and a PayFees execution here.
			Transact {
				origin_kind: OriginKind::Xcm,
				fallback_max_weight: None,
				call: asset_para::RuntimeCall::ForeignAssets(pallet_assets::Call::create {
					id: Location::new(1, Parachain(SIMPLE_PARA_ID)),
					admin: simple_para_sovereign.clone().into(),
					min_balance: 1_000_000_000,
				})
				.encode()
				.into(),
			},
			Transact {
				origin_kind: OriginKind::SovereignAccount,
				fallback_max_weight: None,
				call: asset_para::RuntimeCall::ForeignAssets(pallet_assets::Call::set_metadata {
					id: Location::new(1, Parachain(SIMPLE_PARA_ID)),
					name: "Simple Para Token".into(),
					symbol: "TOK".into(),
					decimals: 12,
				})
				.encode()
				.into(),
			},
		]);

		assert_ok!(simple_para::XcmPallet::send(
			simple_para::RuntimeOrigin::root(),
			Box::new(Location::new(1, Parachain(ASSET_PARA_ID)).into()),
			Box::new(VersionedXcm::V5(xcm)),
		));
	});

	AssetPara::execute_with(|| {
		use asset_para::assets::AssetDeposit;

		// Confirm that we have successfully created the asset.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::ForeignAssets(
			pallet_assets::Event::Created {
				asset_id: Location::new(1, Parachain(SIMPLE_PARA_ID)),
				creator: simple_para_sovereign.clone().into(),
				owner: simple_para_sovereign.clone().into(),
			},
		));

		// The creation of the asset required an asset deposit
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::Balances(
			pallet_balances::Event::Reserved {
				who: simple_para_sovereign.clone(),
				amount: AssetDeposit::get(),
			},
		));

		// Confirm that we have successfully set the metadata.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::ForeignAssets(
			pallet_assets::Event::MetadataSet {
				asset_id: Location::new(1, Parachain(SIMPLE_PARA_ID)),
				name: "Simple Para Token".into(),
				symbol: "TOK".into(),
				decimals: 12,
				is_frozen: false,
			},
		));

		// The setting of the metadata required a deposit too.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::Balances(
			pallet_balances::Event::Reserved {
				who: simple_para_sovereign.into(),
				// T::MetadataDepositBase + metadata_bytes * T::MetadataDepositPerByte
				amount: 30,
			},
		));
	});

	// Todo: Step 2. Create a pool with the AssetPara's native asset and the foreign asset that
	// we just registered.

	// Todo: Step 3. Show how we can transfer our asset to the relay chain, and pay XCM-execution
	// fees with it.
}
