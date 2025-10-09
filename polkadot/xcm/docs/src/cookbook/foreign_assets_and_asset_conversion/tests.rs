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
	asset_para::xcm_config::ThisNetwork,
	network::{
		AssetPara, MockNet, SimplePara, ALICE, ASSET_PARA_ID, FOREIGN_UNITS, SIMPLE_PARA_ID, UNITS,
	},
	simple_para,
};
use asset_para::assets::PoolIdToAccountId;
use frame::{prelude::fungible::Mutate, testing_prelude::*, traits::TryConvert};
use sp_runtime::AccountId32;
use sp_tracing;
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;
use xcm_simulator::TestExt;

#[docify::export]
#[test]
fn registering_foreign_assets_work() {
	// This will print extensive logs if the test fails - very helpful for XCM debugging.
	sp_tracing::init_for_tests();

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
		assert_ok!(asset_para::Balances::mint_into(&simple_para_sovereign, 100 * UNITS));
		assert_eq!(asset_para::Balances::free_balance(&simple_para_sovereign), 100 * UNITS);

		// clear events that we do not want later.
		asset_para::System::reset_events();
	});

	// Fee asset to pay for remote XCM execution fees.
	// The actual sensible amount would be estimated by using the dry-run api, but here we just make
	// sure that we withdraw enough, as the surplus can be refunded.
	let fee_asset: Asset = (Here, Fungible(UNITS / 2)).into();

	// ------------- Step 1. Create the asset on the target chain and set its metadata.

	SimplePara::execute_with(|| {
		let xcm = Xcm(vec![
			// In general parachains do not have free execution. So we have to withdraw some funds
			// into the holding register to pay for our execution fees on the target chain.
			WithdrawAsset(fee_asset.clone().into()),
			PayFees { asset: fee_asset },
			SetAppendix(Xcm(vec![
				RefundSurplus,
				DepositAsset {
					assets: AssetFilter::Wild(WildAsset::All),
					beneficiary: simple_para_sovereign.clone().into(),
				},
			])),
			// The following instructions create and configure the foreign asset.
			Transact {
				origin_kind: OriginKind::Xcm,
				fallback_max_weight: None,
				call: asset_para::RuntimeCall::ForeignAssets(pallet_assets::Call::create {
					id: simple_para_asset_location.clone(),
					admin: simple_para_sovereign.clone(),
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
					decimals: 10,
				})
				.encode()
				.into(),
			},
		]);

		// Send the XCM...
		assert_ok!(simple_para::XcmPallet::send(
			simple_para::RuntimeOrigin::root(),
			Box::new(Location::new(1, Parachain(ASSET_PARA_ID)).into()),
			Box::new(VersionedXcm::V5(xcm)),
		));
	});

	// Let's check if the token was successfully created on the asset para.
	AssetPara::execute_with(|| {
		use asset_para::assets::AssetDeposit;

		// Confirm that we have successfully created the asset.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::ForeignAssets(
			pallet_assets::Event::Created {
				asset_id: simple_para_asset_location.clone(),
				creator: simple_para_sovereign.clone(),
				owner: simple_para_sovereign.clone(),
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
				decimals: 10,
				is_frozen: false,
			},
		));

		// The setting of the metadata required a deposit too.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::Balances(
			pallet_balances::Event::Reserved {
				who: simple_para_sovereign.clone(),
				// T::MetadataDepositBase + T::MetadataDepositPerByte * metadata_bytes
				amount: 30,
			},
		));

		// clear events that we do not want later.
		asset_para::System::reset_events();
	});

	// ------------- Step 2. Create a pool with the AssetPara's native asset and the foreign asset.

	// Give names to some values for better understanding later.
	let pool_id = (Location::here(), simple_para_asset_location.clone());
	let pool_account: AccountId32 = PoolIdToAccountId::try_convert(&pool_id).unwrap();
	let lp_token_id = 0;

	AssetPara::execute_with(|| {
		// Create some liquidity of the foreign asset on the Asset Para.
		assert_ok!(asset_para::ForeignAssets::mint(
			asset_para::RuntimeOrigin::signed(simple_para_sovereign.clone()),
			simple_para_asset_location.clone(),
			simple_para_sovereign.clone(),
			100 * FOREIGN_UNITS,
		));
		asset_para::System::reset_events();

		// Anyone can create a liquidy pool that doesn't exist yet.
		assert_ok!(asset_para::AssetConversion::create_pool(
			asset_para::RuntimeOrigin::signed(simple_para_sovereign.clone()),
			Box::new(Location::here()),
			Box::new(simple_para_asset_location.clone()),
		));

		// Assert that we have successfully created the liquidity pool.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::AssetConversion(
			pallet_asset_conversion::Event::PoolCreated {
				creator: simple_para_sovereign.clone(),
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
			asset_para::RuntimeOrigin::signed(simple_para_sovereign.clone()),
			Box::new(Location::here()),
			Box::new(simple_para_asset_location.clone()),
			10 * UNITS,
			20 * FOREIGN_UNITS,
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
				amount1_provided: 10 * UNITS,
				amount2_provided: 20 * FOREIGN_UNITS,
				lp_token: lp_token_id,
				lp_token_minted: 141421356137,
			},
		));

		// clear events that we do not want later.
		asset_para::System::reset_events();
	});

	// ------------- Step 3. Teleport our native asset to Asset Para and back.

	let alice_location = Location {
		parents: 0,
		interior: Junction::AccountId32 { network: Some(ThisNetwork::get()), id: ALICE.into() }
			.into(),
	};

	SimplePara::execute_with(|| {
		assert_ok!(simple_para::XcmPallet::limited_teleport_assets(
			simple_para::RuntimeOrigin::signed(ALICE),
			// Destination chain
			Box::new(Location::new(1, Parachain(ASSET_PARA_ID)).into()),
			// Beneficiary
			Box::new(alice_location.clone().into()),
			// Assets to be teleported
			Box::new(vec![(Location::here(), Fungible(2 * FOREIGN_UNITS)).into()].into()),
			// Fee asset index
			0,
			WeightLimit::Unlimited,
		));

		simple_para::System::reset_events();
	});

	// Confirm that we have received the foreign asset and that we could pay the XCM execution
	// fees with our foreign asset.
	AssetPara::execute_with(|| {
		// We configured our `SwapFirstAssetTrader` to always charge 1 native Balance, but we need
		// to pay 3 Simple Para tokens to get 1 native token.
		let fee_to_be_paid = 3;

		// The 3 Simple Para tokens that will be used to buy on native token are deposited to the
		// pool account.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::ForeignAssets(
			pallet_assets::Event::Deposited {
				amount: fee_to_be_paid,
				asset_id: simple_para_asset_location.clone(),
				who: pool_account.clone(),
			},
		));

		// We see that paid 3 Simple Para token to get one Asset Para token.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::AssetConversion(
			pallet_asset_conversion::Event::SwapCreditExecuted {
				// We need to pay 3 Simple Para tokens to get 1 native token
				amount_in: fee_to_be_paid,
				amount_out: 1,
				path: vec![
					(simple_para_asset_location.clone(), fee_to_be_paid),
					(Location::here(), 1),
				],
			},
		));

		// Alice receives the remaining amount after deducting the fees.
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::ForeignAssets(
			pallet_assets::Event::Issued {
				asset_id: simple_para_asset_location.clone(),
				owner: ALICE,
				amount: 2 * FOREIGN_UNITS - fee_to_be_paid,
			},
		));

		// clear events that we do not want later.
		asset_para::System::reset_events();

		// Teleport some tokens back to the Simple Para.
		assert_ok!(asset_para::XcmPallet::limited_teleport_assets(
			asset_para::RuntimeOrigin::signed(ALICE),
			// Destination chain
			Box::new(Location::new(1, Parachain(SIMPLE_PARA_ID)).into()),
			// Beneficiary
			Box::new(alice_location.into()),
			// Assets to be teleported (note the difference to above).
			Box::new(
				vec![(Location::new(1, Parachain(SIMPLE_PARA_ID)), Fungible(FOREIGN_UNITS)).into()]
					.into()
			),
			// Fee asset index
			0,
			WeightLimit::Unlimited,
		));
	});

	// Confirm that the tokens made it back to simple para.
	SimplePara::execute_with(|| {
		// In this example, Simple Para does not pay for execution fees. Hence, Alice receives
		// the full amount.
		simple_para::System::assert_has_event(simple_para::RuntimeEvent::Balances(
			pallet_balances::Event::Minted { who: ALICE, amount: FOREIGN_UNITS },
		));
	});
}
