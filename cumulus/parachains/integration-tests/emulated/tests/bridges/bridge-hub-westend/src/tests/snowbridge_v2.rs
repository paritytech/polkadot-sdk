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
use frame_support::weights::WeightToFee;
use hex_literal::hex;
use snowbridge_router_primitives::inbound::{
	v2::{Asset::NativeTokenERC20, Message},
	EthereumLocationsConverterFor,
};
use sp_core::H160;
use sp_runtime::MultiAddress;
use testnet_parachains_constants::westend::fee::WeightToFee as WeightCalculator;

/// Calculates the XCM prologue fee for sending an XCM to AH.
const INITIAL_FUND: u128 = 5_000_000_000_000;
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;
#[test]
fn xcm_prologue_fee() {
	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id().into(), INITIAL_FUND);

	let asset_hub_sovereign = BridgeHubWestend::sovereign_account_id_of(Location::new(
		1,
		[Parachain(AssetHubWestend::para_id().into())],
	));

	let relayer = BridgeHubWestendSender::get();
	let receiver = AssetHubWestendReceiver::get();
	BridgeHubWestend::fund_accounts(vec![(relayer.clone(), INITIAL_FUND)]);

	let mut token_ids = Vec::new();
	for _ in 0..8 {
		token_ids.push(H160::random());
	}

	let ethereum_network_v5: NetworkId = EthereumNetwork::get().into();

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		for token_id in token_ids.iter() {
			let token_id = *token_id;

			let asset_location = Location::new(
				2,
				[
					GlobalConsensus(ethereum_network_v5),
					AccountKey20 { network: None, key: token_id.into() },
				],
			);

			assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
				RuntimeOrigin::root(),
				asset_location.clone(),
				asset_hub_sovereign.clone().into(),
				false,
				1,
			));

			assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
				asset_location.clone().try_into().unwrap(),
			));
		}
	});

	let native_tokens: Vec<snowbridge_router_primitives::inbound::v2::Asset> = token_ids
		.iter()
		.map(|token_id| NativeTokenERC20 { token_id: *token_id, value: 3_000_000_000 })
		.collect();

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;

		let claimer = AccountId32 { network: None, id: receiver.clone().into() };
		let claimer_bytes = claimer.encode();
		let origin = H160::random();
		let relayer_location =
			Location::new(0, AccountId32 { network: None, id: relayer.clone().into() });

		let message_xcm_instructions =
			vec![DepositAsset { assets: Wild(AllCounted(8)), beneficiary: receiver.into() }];
		let message_xcm: Xcm<()> = message_xcm_instructions.into();
		let versioned_message_xcm = VersionedXcm::V5(message_xcm);

		let message = Message {
			origin,
			assets: native_tokens,
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

	let execution_fee = WeightCalculator::weight_to_fee(&Weight::from_parts(1410450000, 33826));
	let buffered_fee = execution_fee * 2;
	println!("buffered execution fee for prologue for 8 assets: {}", buffered_fee);
}

#[test]
fn register_token_xcm() {
	BridgeHubWestend::execute_with(|| {
		let token: H160 = hex!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").into(); // token id placeholder
		println!("token: {:x?}", token);

		let ethereum_network_v5: NetworkId = EthereumNetwork::get().into();
		let dot_asset = Location::new(1, Here);
		let dot_fee: xcm::prelude::Asset = (dot_asset, CreateAssetDeposit::get()).into();

		let asset_id = Location::new(
			2,
			[
				GlobalConsensus(ethereum_network_v5),
				AccountKey20 { network: None, key: token.into() },
			],
		);

		println!(
			"register token mainnet: {:x?}",
			get_xcm_hex(1u64, asset_id.clone(), dot_fee.clone())
		);
		println!("register token sepolia: {:x?}", get_xcm_hex(11155111u64, asset_id, dot_fee));
	});
}

fn get_xcm_hex(chain_id: u64, asset_id: Location, dot_fee: xcm::prelude::Asset) -> String {
	let owner = EthereumLocationsConverterFor::<[u8; 32]>::from_chain_id(&chain_id);

	let register_token_xcm = vec![
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
