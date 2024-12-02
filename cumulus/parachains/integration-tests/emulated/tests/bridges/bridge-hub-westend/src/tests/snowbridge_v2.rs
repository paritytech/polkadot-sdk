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
use bridge_hub_westend_runtime::{
	bridge_to_ethereum_config::{CreateAssetCall, CreateAssetDeposit},
	EthereumInboundQueueV2,
};
use codec::Encode;
use frame_support::{traits::fungibles::Mutate};
use hex_literal::hex;
use snowbridge_router_primitives::inbound::{
	v2::{Asset::NativeTokenERC20, Message},
	EthereumLocationsConverterFor,
};
use sp_core::H160;
use sp_runtime::MultiAddress;

/// Calculates the XCM prologue fee for sending an XCM to AH.
const INITIAL_FUND: u128 = 5_000_000_000_000;
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;
const WETH: [u8; 20] = hex!("fff9976782d46cc05630d1f6ebab18b2324d6b14");

pub fn weth_location() -> Location {
	Location::new(
		2,
		[
			GlobalConsensus(EthereumNetwork::get().into()),
			AccountKey20 { network: None, key: WETH.into() },
		],
	)
}

pub fn register_weth() {
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);
	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
			RuntimeOrigin::root(),
			weth_location().try_into().unwrap(),
			assethub_sovereign.clone().into(),
			true,
			1000, //ED will be used as exchange rate by default when used to PayFees with
		));

		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			weth_location().try_into().unwrap(),
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint_into(
			weth_location().try_into().unwrap(),
			&AssetHubWestendReceiver::get(),
			1000000,
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint_into(
			weth_location().try_into().unwrap(),
			&AssetHubWestendSender::get(),
			1000000,
		));
	});
}

pub(crate) fn set_up_weth_pool_with_wnd_on_ah_westend(asset: v5::Location) {
	let wnd: v5::Location = v5::Parent.into();
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let owner = AssetHubWestendSender::get();
	let bh_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);

	AssetHubWestend::fund_accounts(vec![
		(owner.clone(), 3_000_000_000_000),
	]);

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		let signed_owner = <AssetHubWestend as Chain>::RuntimeOrigin::signed(owner.clone());
		let signed_bh_sovereign = <AssetHubWestend as Chain>::RuntimeOrigin::signed(bh_sovereign.clone());

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint(
			signed_bh_sovereign.clone(),
			asset.clone().into(),
			bh_sovereign.clone().into(),
			3_500_000_000_000,
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::transfer(
			signed_bh_sovereign.clone(),
			asset.clone().into(),
			owner.clone().into(),
			3_000_000_000_000,
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
			signed_owner.clone(),
			Box::new(wnd.clone()),
			Box::new(asset.clone()),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
			signed_owner.clone(),
			Box::new(wnd),
			Box::new(asset),
			1_000_000_000_000,
			2_000_000_000_000,
			1,
			1,
			owner.into()
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {..}) => {},
			]
		);
	});
}

#[test]
fn register_token_v2() {
	// Whole register token fee is 374_851_000_000
	let relayer = BridgeHubWestendSender::get();
	let receiver = AssetHubWestendReceiver::get();
	BridgeHubWestend::fund_accounts(vec![(relayer.clone(), INITIAL_FUND)]);

	register_weth();

	set_up_weth_pool_with_wnd_on_ah_westend(weth_location());

	let chain_id = 11155111u64;
	let claimer = AccountId32 { network: None, id: receiver.clone().into() };
	let claimer_bytes = claimer.encode();
	let origin = H160::random();
	let relayer_location =
		Location::new(0, AccountId32 { network: None, id: relayer.clone().into() });

	let bridge_owner = EthereumLocationsConverterFor::<[u8; 32]>::from_chain_id(&chain_id);

	let token: H160 = hex!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").into();

	let ethereum_network_v5: NetworkId = EthereumNetwork::get().into();

	// Used to pay the asset creation deposit.
	let weth_asset_value = 9_000_000_000_000u128;
	let assets = vec![NativeTokenERC20 { token_id: WETH.into(), value: weth_asset_value }];
	let asset_deposit_weth: xcm::prelude::Asset = (weth_location(), weth_asset_value).into();

	let asset_id = Location::new(
		2,
		[GlobalConsensus(ethereum_network_v5), AccountKey20 { network: None, key: token.into() }],
	);

	let dot_asset = Location::new(1, Here);
	let dot_fee: xcm::prelude::Asset = (dot_asset, CreateAssetDeposit::get()).into();

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		let register_token_instructions = vec![
			// Exchange weth for dot to pay the asset creation deposit
			ExchangeAsset { give: asset_deposit_weth.clone().into(), want: dot_fee.clone().into(), maximal: false },
			// Deposit the dot deposit into the bridge sovereign account (where the asset creation fee
			// will be deducted from)
			DepositAsset { assets: dot_fee.into(), beneficiary: bridge_owner.into() },
			// Call to create the asset.
			Transact {
				origin_kind: OriginKind::Xcm,
				call: (
					CreateAssetCall::get(),
					asset_id,
					MultiAddress::<[u8; 32], ()>::Id(bridge_owner.into()),
					1u128,
				)
					.encode()
					.into(),
			},
			ExpectTransactStatus(MaybeErrorCode::Success),
		];
		let xcm: Xcm<()> = register_token_instructions.into();
		let versioned_message_xcm = VersionedXcm::V5(xcm);

		let message = Message {
			origin,
			fee: 1_500_000_000_000u128,
			assets,
			xcm: versioned_message_xcm.encode(),
			claimer: Some(claimer_bytes),
		};
		let xcm = EthereumInboundQueueV2::do_convert(message, relayer_location).unwrap();
		let _ = EthereumInboundQueueV2::send_xcm(xcm, AssetHubWestend::para_id().into()).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Created { .. }) => {},]
		);
	});
}

#[test]
fn register_token_xcm() {
	BridgeHubWestend::execute_with(|| {
		println!("register token mainnet: {:x?}", get_xcm_hex(1u64));
		println!("===============================",);
		println!("register token sepolia: {:x?}", get_xcm_hex(11155111u64));
	});
}

fn get_xcm_hex(chain_id: u64) -> String {
	let owner = EthereumLocationsConverterFor::<[u8; 32]>::from_chain_id(&chain_id);
	let weth_token_id: H160 = hex!("be68fc2d8249eb60bfcf0e71d5a0d2f2e292c4ed").into(); // TODO insert token id
	let token: H160 = hex!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").into(); // token id placeholder
	let weth_amount = 300_000_000_000_000u128;

	let ethereum_network_v5: NetworkId = EthereumNetwork::get().into();
	let dot_asset = Location::new(1, Here);
	let dot_fee: xcm::prelude::Asset = (dot_asset, CreateAssetDeposit::get()).into();

	println!("register token id: {:x?}", token);
	println!("weth token id: {:x?}", weth_token_id);
	println!("weth_amount: {:x?}", hex::encode(weth_amount.encode()));
	println!("dot asset: {:x?}", hex::encode(dot_fee.encode()));

	let weth_asset = Location::new(
		2,
		[
			GlobalConsensus(ethereum_network_v5),
			AccountKey20 { network: None, key: weth_token_id.into() },
		],
	);
	let weth_fee: xcm::prelude::Asset = (weth_asset, weth_amount).into(); // TODO replace Weth fee acmount

	let asset_id = Location::new(
		2,
		[GlobalConsensus(ethereum_network_v5), AccountKey20 { network: None, key: token.into() }],
	);

	let register_token_xcm = vec![
		ExchangeAsset { give: weth_fee.into(), want: dot_fee.clone().into(), maximal: false },
		PayFees { asset: dot_fee },
		Transact {
			origin_kind: OriginKind::Xcm,
			call: (
				CreateAssetCall::get(),
				asset_id,
				MultiAddress::<[u8; 32], ()>::Id(owner.into()),
				1,
			)
				.encode()
				.into(),
		},
	];
	let message_xcm: Xcm<()> = register_token_xcm.into();
	let versioned_message_xcm = VersionedXcm::V5(message_xcm);

	let xcm_bytes = versioned_message_xcm.encode();
	hex::encode(xcm_bytes)
}
