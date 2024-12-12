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

use crate::{
	imports::*,
	tests::{snowbridge::WETH, snowbridge_common::*},
};
use emulated_integration_tests_common::{impls::Decode, PenpalBTeleportableAssetLocation};
use frame_support::pallet_prelude::TypeInfo;
use rococo_westend_system_emulated_network::penpal_emulated_chain::penpal_runtime::xcm_config::LocalTeleportableToAssetHub;
use snowbridge_core::AssetMetadata;
use snowbridge_outbound_primitives::TransactInfo;
use snowbridge_router_primitives::inbound::EthereumLocationsConverterFor;
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;
use xcm::v5::AssetTransferFilter;
use xcm_executor::traits::ConvertLocation;

#[test]
fn send_weth_from_asset_hub_to_ethereum() {
	fund_on_bh();

	register_weth_on_ah();

	fund_on_ah();

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };

		let remote_fee_asset =
			Asset { id: AssetId(weth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH) };

		let reserve_asset = Asset {
			id: AssetId(weth_location()),
			fun: Fungible(TOKEN_AMOUNT - REMOTE_FEE_AMOUNT_IN_WETH),
		};

		let assets = vec![
			Asset { id: weth_location().into(), fun: Fungible(TOKEN_AMOUNT) },
			local_fee_asset.clone(),
		];

		let xcm = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination: ethereum(),
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![AssetTransferFilter::ReserveWithdraw(Definite(
					reserve_asset.clone().into(),
				))],
				remote_xcm: Xcm(vec![DepositAsset {
					assets: Wild(AllCounted(2)),
					beneficiary: beneficiary(),
				}]),
			},
		]));

		// Send the Weth back to Ethereum
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendReceiver::get()),
			bx!(xcm),
			Weight::from(EXECUTION_WEIGHT),
		)
		.unwrap();
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// Check that the Ethereum message was queue in the Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}

#[test]
fn transfer_relay_token_from_ah() {
	let ethereum_sovereign: AccountId =
		EthereumLocationsConverterFor::<[u8; 32]>::convert_location(&ethereum())
			.unwrap()
			.into();

	fund_on_bh();

	register_relay_token_on_bh();

	register_weth_on_ah();

	fund_on_ah();

	// Send token to Ethereum
	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };
		let remote_fee_asset =
			Asset { id: AssetId(weth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH) };

		let assets = vec![
			Asset {
				id: AssetId(Location::parent()),
				fun: Fungible(TOKEN_AMOUNT + LOCAL_FEE_AMOUNT_IN_DOT),
			},
			remote_fee_asset.clone(),
		];

		let xcm = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination: ethereum(),
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![AssetTransferFilter::ReserveDeposit(Definite(
					Asset { id: AssetId(Location::parent()), fun: Fungible(TOKEN_AMOUNT) }.into(),
				))],
				remote_xcm: Xcm(vec![DepositAsset {
					assets: Wild(AllCounted(2)),
					beneficiary: beneficiary(),
				}]),
			},
		]));

		// Send DOT to Ethereum
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			bx!(xcm),
			Weight::from(EXECUTION_WEIGHT),
		)
		.unwrap();

		// Check that the native asset transferred to some reserved account(sovereign of Ethereum)
		let events = AssetHubWestend::events();
		assert!(
			events.iter().any(|event| matches!(
				event,
				RuntimeEvent::Balances(pallet_balances::Event::Minted { who, amount})
					if *who == ethereum_sovereign.clone() && *amount == TOKEN_AMOUNT,
			)),
			"native token reserved to Ethereum sovereign account."
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;

		// Check that the Ethereum message was queue in the Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}

#[test]
fn send_weth_and_dot_from_asset_hub_to_ethereum() {
	fund_on_bh();

	register_relay_token_on_bh();

	register_weth_on_ah();

	fund_on_ah();

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };
		let remote_fee_asset =
			Asset { id: AssetId(weth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH) };

		let reserve_asset = Asset {
			id: AssetId(weth_location()),
			fun: Fungible(TOKEN_AMOUNT - REMOTE_FEE_AMOUNT_IN_WETH),
		};

		let weth_asset = Asset { id: weth_location().into(), fun: Fungible(TOKEN_AMOUNT) };

		let dot_asset = Asset { id: AssetId(Location::parent()), fun: Fungible(TOKEN_AMOUNT) };

		let assets = vec![weth_asset, dot_asset.clone(), local_fee_asset.clone()];

		let xcms = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination: ethereum(),
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![
					AssetTransferFilter::ReserveWithdraw(Definite(reserve_asset.clone().into())),
					AssetTransferFilter::ReserveDeposit(Definite(dot_asset.into())),
				],
				remote_xcm: Xcm(vec![DepositAsset {
					assets: Wild(All),
					beneficiary: beneficiary(),
				}]),
			},
		]));

		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendReceiver::get()),
			bx!(xcms),
			Weight::from(EXECUTION_WEIGHT),
		)
		.unwrap();
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// Check that Ethereum message was queue in the Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}

#[test]
fn transact_with_agent() {
	let weth_asset_location: Location =
		(Parent, Parent, EthereumNetwork::get(), AccountKey20 { network: None, key: WETH }).into();

	fund_on_bh();

	register_ah_user_agent_on_ethereum();

	register_weth_on_ah();

	fund_on_ah();

	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };

		let remote_fee_asset = Asset {
			id: AssetId(weth_asset_location.clone()),
			fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH),
		};
		let reserve_asset = Asset {
			id: AssetId(weth_asset_location.clone()),
			fun: Fungible(TOKEN_AMOUNT - REMOTE_FEE_AMOUNT_IN_WETH),
		};

		let assets = vec![
			Asset { id: weth_asset_location.clone().into(), fun: Fungible(TOKEN_AMOUNT) },
			local_fee_asset.clone(),
		];

		let beneficiary =
			Location::new(0, [AccountKey20 { network: None, key: AGENT_ADDRESS.into() }]);

		let transact_info = TransactInfo {
			target: Default::default(),
			data: vec![],
			gas_limit: 40000,
			// value should be less than the transfer amount, require validation on BH Exporter
			value: 4 * (TOKEN_AMOUNT - REMOTE_FEE_AMOUNT_IN_WETH) / 5,
		};

		let xcms = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset.clone() },
			InitiateTransfer {
				destination: ethereum(),
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![AssetTransferFilter::ReserveWithdraw(Definite(
					reserve_asset.clone().into(),
				))],
				remote_xcm: Xcm(vec![
					DepositAsset { assets: Wild(AllCounted(2)), beneficiary },
					Transact {
						origin_kind: OriginKind::SovereignAccount,
						fallback_max_weight: None,
						call: transact_info.encode().into(),
					},
				]),
			},
		]));

		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			bx!(xcms),
			Weight::from(EXECUTION_WEIGHT),
		)
		.unwrap();
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// Check that Ethereum message was queue in the Outbound Queue
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}

fn send_message_from_penpal_to_ethereum(sudo: bool) {
	// bh
	fund_on_bh();
	register_penpal_agent_on_ethereum();
	// ah
	register_weth_on_ah();
	register_pal_on_ah();
	register_pal_on_bh();
	fund_on_ah();
	create_pools_on_ah();
	// penpal
	set_trust_reserve_on_penpal();
	register_weth_on_penpal();
	fund_on_penpal();

	PenpalB::execute_with(|| {
		type RuntimeOrigin = <PenpalB as Chain>::RuntimeOrigin;

		let local_fee_asset_on_penpal =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };

		let remote_fee_asset_on_ah =
			Asset { id: AssetId(weth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH) };

		let remote_fee_asset_on_ethereum =
			Asset { id: AssetId(weth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH) };

		let pna =
			Asset { id: AssetId(LocalTeleportableToAssetHub::get()), fun: Fungible(TOKEN_AMOUNT) };

		let ena = Asset { id: AssetId(weth_location()), fun: Fungible(TOKEN_AMOUNT / 2) };

		let transfer_asset_reanchor_on_ah = Asset {
			id: AssetId(PenpalBTeleportableAssetLocation::get()),
			fun: Fungible(TOKEN_AMOUNT),
		};

		let assets = vec![
			local_fee_asset_on_penpal.clone(),
			remote_fee_asset_on_ah.clone(),
			remote_fee_asset_on_ethereum.clone(),
			pna.clone(),
			ena.clone(),
		];

		let transact_info =
			TransactInfo { target: Default::default(), data: vec![], gas_limit: 40000, value: 0 };

		let xcm = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset_on_penpal.clone() },
			InitiateTransfer {
				destination: asset_hub(),
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset_on_ah.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![
					AssetTransferFilter::ReserveWithdraw(Definite(
						remote_fee_asset_on_ethereum.clone().into(),
					)),
					AssetTransferFilter::ReserveWithdraw(Definite(ena.clone().into())),
					// Should use Teleport here because:
					// a. Penpal is configured to allow teleport specific asset to AH
					// b. AH is configured to trust asset teleport from sibling chain
					AssetTransferFilter::Teleport(Definite(pna.clone().into())),
				],
				remote_xcm: Xcm(vec![InitiateTransfer {
					destination: ethereum(),
					remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
						remote_fee_asset_on_ethereum.clone().into(),
					))),
					preserve_origin: true,
					assets: vec![
						// should use ReserveDeposit because Ethereum does not trust asset from
						// penpal. transfer_asset should be reachored first on AH
						AssetTransferFilter::ReserveDeposit(Definite(
							transfer_asset_reanchor_on_ah.clone().into(),
						)),
						AssetTransferFilter::ReserveWithdraw(Definite(ena.clone().into())),
					],
					remote_xcm: Xcm(vec![
						DepositAsset { assets: Wild(All), beneficiary: beneficiary() },
						Transact {
							origin_kind: OriginKind::SovereignAccount,
							fallback_max_weight: None,
							call: transact_info.encode().into(),
						},
					]),
				}]),
			},
		]));

		if sudo {
			assert_ok!(<PenpalB as PenpalBPallet>::PolkadotXcm::execute(
				RuntimeOrigin::root(),
				bx!(xcm.clone()),
				Weight::from(EXECUTION_WEIGHT),
			));
		} else {
			assert_ok!(<PenpalB as PenpalBPallet>::PolkadotXcm::execute(
				RuntimeOrigin::signed(PenpalBSender::get()),
				bx!(xcm.clone()),
				Weight::from(EXECUTION_WEIGHT),
			));
		}
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapCreditExecuted { .. }) => {},]
		);
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},]
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}

#[test]
fn send_message_from_penpal_to_ethereum_with_sudo() {
	send_message_from_penpal_to_ethereum(true)
}

#[test]
fn send_message_from_penpal_to_ethereum_with_user_origin() {
	send_message_from_penpal_to_ethereum(false)
}

#[derive(Encode, Decode, Debug, PartialEq, Clone, TypeInfo)]
pub enum ControlFrontendCall {
	#[codec(index = 1)]
	CreateAgent { fee: u128 },
	#[codec(index = 2)]
	RegisterToken { asset_id: Box<VersionedLocation>, metadata: AssetMetadata, fee: u128 },
}

#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Clone, TypeInfo)]
pub enum SnowbridgeControlFrontend {
	#[codec(index = 80)]
	Control(ControlFrontendCall),
}

#[test]
fn create_user_agent_from_penpal() {
	fund_on_bh();
	register_weth_on_ah();
	fund_on_ah();
	create_pools_on_ah();
	set_trust_reserve_on_penpal();
	register_weth_on_penpal();
	fund_on_penpal();
	let penpal_user_location = Location::new(
		1,
		[
			Parachain(PenpalB::para_id().into()),
			AccountId32 {
				network: Some(ByGenesis(WESTEND_GENESIS_HASH)),
				id: PenpalBSender::get().into(),
			},
		],
	);
	PenpalB::execute_with(|| {
		type RuntimeOrigin = <PenpalB as Chain>::RuntimeOrigin;

		let local_fee_asset_on_penpal =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };

		let remote_fee_asset_on_ah =
			Asset { id: AssetId(weth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH) };

		let remote_fee_asset_on_ethereum =
			Asset { id: AssetId(weth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_WETH) };

		let call = SnowbridgeControlFrontend::Control(ControlFrontendCall::CreateAgent {
			fee: REMOTE_FEE_AMOUNT_IN_WETH,
		});

		let assets = vec![
			local_fee_asset_on_penpal.clone(),
			remote_fee_asset_on_ah.clone(),
			remote_fee_asset_on_ethereum.clone(),
		];

		let xcm = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset_on_penpal.clone() },
			InitiateTransfer {
				destination: asset_hub(),
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset_on_ah.clone().into(),
				))),
				preserve_origin: true,
				assets: vec![AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset_on_ethereum.clone().into(),
				))],
				remote_xcm: Xcm(vec![
					DepositAsset { assets: Wild(All), beneficiary: penpal_user_location },
					Transact {
						origin_kind: OriginKind::Xcm,
						call: call.encode().into(),
						fallback_max_weight: None,
					},
					ExpectTransactStatus(MaybeErrorCode::Success),
				]),
			},
		]));

		assert_ok!(<PenpalB as PenpalBPallet>::PolkadotXcm::execute(
			RuntimeOrigin::signed(PenpalBSender::get()),
			bx!(xcm.clone()),
			Weight::from(EXECUTION_WEIGHT),
		));
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned { .. }) => {},]
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::EthereumOutboundQueueV2(snowbridge_pallet_outbound_queue_v2::Event::MessageQueued{ .. }) => {},]
		);
	});
}
