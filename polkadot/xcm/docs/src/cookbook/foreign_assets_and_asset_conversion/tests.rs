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
use frame::{prelude::fungible::Mutate, testing_prelude::*, traits::TryConvert};
use sp_runtime::AccountId32;
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

	let simple_para_asset_location = Location::new(1, Parachain(SIMPLE_PARA_ID));

	// We ensure that Simple Para's sovereign account has funds on the Asset Para to pay for the
	// deposits needed to create the foreign asset.
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
					id: simple_para_asset_location.clone(),
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
					id: simple_para_asset_location.clone(),
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
				asset_id: simple_para_asset_location.clone(),
				creator: simple_para_sovereign.clone().into(),
				owner: simple_para_sovereign.clone().into(),
			},
		));

		// The creation of the asset required an asset deposit.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::Balances(
			pallet_balances::Event::Reserved {
				who: simple_para_sovereign.clone(),
				amount: AssetDeposit::get(),
			},
		));

		// Confirm that we have successfully set the metadata.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::ForeignAssets(
			pallet_assets::Event::MetadataSet {
				asset_id: simple_para_asset_location.clone(),
				name: "Simple Para Token".into(),
				symbol: "TOK".into(),
				decimals: 12,
				is_frozen: false,
			},
		));

		// The setting of the metadata required a deposit too.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::Balances(
			pallet_balances::Event::Reserved {
				who: simple_para_sovereign.clone().into(),
				// T::MetadataDepositBase + T::MetadataDepositPerByte * metadata_bytes
				amount: 30,
			},
		));

		// clear events that we do not want later.
		asset_para::System::reset_events();
	});

	// Step 2. Create a pool with the AssetPara's native asset and the foreign asset.
	AssetPara::execute_with(|| {
		use asset_para::assets::PoolIdToAccountId;

		// Create some liquidity of the foreign asset on the Asset Para.
		assert_ok!(asset_para::ForeignAssets::mint(
			asset_para::RuntimeOrigin::signed(simple_para_sovereign.clone().into()),
			simple_para_asset_location.clone(),
			simple_para_sovereign.clone().into(),
			3 * UNITS,
		));
		asset_para::System::reset_events();

		// Anyone can create a liquidy pool that doesn't exist yet.
		assert_ok!(asset_para::AssetConversion::create_pool(
			asset_para::RuntimeOrigin::signed(simple_para_sovereign.clone().into()),
			Box::new(Location::here()),
			Box::new(simple_para_asset_location.clone()),
		));

		// Give names to some values for better understanding later.
		let pool_id = (Location::here(), simple_para_asset_location.clone());
		let pool_account: AccountId32 = PoolIdToAccountId::try_convert(&pool_id).unwrap();
		let lp_token_id = 0;

		// Assert that we have successfully created the liquidity pool.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::AssetConversion(
			pallet_asset_conversion::Event::PoolCreated {
				creator: simple_para_sovereign.clone().into(),
				pool_id: pool_id.clone(),
				pool_account: PoolIdToAccountId::try_convert(&pool_id).unwrap(),
				lp_token: lp_token_id,
			},
		));

		// Creating the liquidity pool will also create the corresponding asset in our
		// `PoolAssets` instance.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::PoolAssets(
			pallet_assets::Event::ForceCreated {
				asset_id: lp_token_id,
				owner: pool_account.clone(),
			},
		));

		// There are also balance events regarding the deposits, but we will ignore them for
		// conciseness.

		// clear events that we do not want later.
		asset_para::System::reset_events();

		// Anybody can add liquidity to the pool.
		assert_ok!(asset_para::AssetConversion::add_liquidity(
			asset_para::RuntimeOrigin::signed(simple_para_sovereign.clone().into()),
			Box::new(Location::here()),
			Box::new(simple_para_asset_location.clone()),
			1 * UNITS,
			2 * UNITS,
			1,
			1,
			simple_para_sovereign.clone(),
		));

		// This is the first time we add liquidity. So we will create the pool account.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::System(
			frame_system::Event::NewAccount { account: pool_account.clone() },
		));

		// There are also some balance/foreign assets transfers. We will again omit them for
		// conciseness.

		asset_para::System::assert_has_event(asset_para::RuntimeEvent::AssetConversion(
			pallet_asset_conversion::Event::LiquidityAdded {
				who: simple_para_sovereign.clone(),
				mint_to: simple_para_sovereign.clone(),
				pool_id: pool_id.clone(),
				amount1_provided: 1 * UNITS,
				amount2_provided: 2 * UNITS,
				lp_token: lp_token_id,
				lp_token_minted: 14142135523,
			},
		));
	});

	// Todo: Step 3. Show how we can transfer our asset to the relay chain, and pay XCM-execution
	// fees with it.
}
