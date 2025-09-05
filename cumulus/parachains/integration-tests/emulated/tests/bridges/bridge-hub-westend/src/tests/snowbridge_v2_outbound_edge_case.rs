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
	tests::{
		snowbridge_common::*,
		snowbridge_v2_outbound::{EthereumSystemFrontend, EthereumSystemFrontendCall},
		usdt_at_ah_westend,
	},
};
use emulated_integration_tests_common::snowbridge::{SEPOLIA_ID, WETH};
use frame_support::assert_noop;
use snowbridge_core::AssetMetadata;
use sp_runtime::DispatchError::BadOrigin;
use xcm::v5::AssetTransferFilter;

#[test]
fn register_penpal_a_asset_from_penpal_b_will_fail() {
	fund_on_bh();
	register_assets_on_ah();
	fund_on_ah();
	create_pools_on_ah();
	set_trust_reserve_on_penpal();
	register_assets_on_penpal();
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
	let asset_location_on_penpal = PenpalLocalTeleportableToAssetHub::get();
	let penpal_a_asset_at_asset_hub =
		Location::new(1, [Junction::Parachain(PenpalA::para_id().into())])
			.appended_with(asset_location_on_penpal)
			.unwrap();
	PenpalB::execute_with(|| {
		type RuntimeOrigin = <PenpalB as Chain>::RuntimeOrigin;

		let local_fee_asset_on_penpal =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };

		let remote_fee_asset_on_ah =
			Asset { id: AssetId(ethereum()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_ETHER) };

		let remote_fee_asset_on_ethereum =
			Asset { id: AssetId(ethereum()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_ETHER) };

		let call = EthereumSystemFrontend::EthereumSystemFrontend(
			EthereumSystemFrontendCall::RegisterToken {
				asset_id: Box::new(VersionedLocation::from(penpal_a_asset_at_asset_hub)),
				metadata: Default::default(),
				fee_asset: remote_fee_asset_on_ethereum.clone(),
			},
		);

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
				assets: BoundedVec::truncate_from(vec![AssetTransferFilter::ReserveWithdraw(
					Definite(remote_fee_asset_on_ethereum.clone().into()),
				)]),
				remote_xcm: Xcm(vec![
					DepositAsset { assets: Wild(All), beneficiary: penpal_user_location },
					Transact {
						origin_kind: OriginKind::Xcm,
						call: call.encode().into(),
						fallback_max_weight: None,
					},
				]),
			},
		]));

		assert_ok!(<PenpalB as PenpalBPallet>::PolkadotXcm::execute(
			RuntimeOrigin::root(),
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

	// No events should be emitted on the bridge hub
	BridgeHubWestend::execute_with(|| {
		assert_expected_events!(BridgeHubWestend, vec![]);
	});
}

#[test]
fn export_from_non_system_parachain_will_fail() {
	let penpal_location = Location::new(1, [Parachain(PenpalB::para_id().into())]);
	let penpal_sovereign = BridgeHubWestend::sovereign_account_id_of(penpal_location.clone());
	BridgeHubWestend::fund_accounts(vec![(penpal_sovereign.clone(), INITIAL_FUND)]);

	PenpalB::execute_with(|| {
		type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
		type RuntimeOrigin = <PenpalB as Chain>::RuntimeOrigin;

		let relay_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(1_000_000_000_000) };

		let weth_location_reanchored =
			Location::new(0, [AccountKey20 { network: None, key: WETH.into() }]);

		let weth_asset =
			Asset { id: AssetId(weth_location_reanchored.clone()), fun: Fungible(TOKEN_AMOUNT) };

		assert_ok!(<PenpalB as PenpalBPallet>::PolkadotXcm::send(
			RuntimeOrigin::root(),
			bx!(VersionedLocation::from(bridge_hub())),
			bx!(VersionedXcm::from(Xcm(vec![
				WithdrawAsset(relay_fee_asset.clone().into()),
				BuyExecution { fees: relay_fee_asset.clone(), weight_limit: Unlimited },
				ExportMessage {
					network: Ethereum { chain_id: SEPOLIA_ID },
					destination: Here,
					xcm: Xcm(vec![
						AliasOrigin(penpal_location),
						WithdrawAsset(weth_asset.clone().into()),
						DepositAsset { assets: Wild(All), beneficiary: beneficiary() },
						SetTopic([0; 32]),
					]),
				},
			]))),
		));

		assert_expected_events!(
			PenpalB,
			vec![RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent{ .. }) => {},]
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed{ success: false, .. }) => {},]
		);
	});
}

#[test]
pub fn register_usdt_not_from_owner_on_asset_hub_will_fail() {
	fund_on_bh();
	register_assets_on_ah();
	fund_on_ah();
	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let fees_asset =
			Asset { id: AssetId(ethereum()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_ETHER) };

		assert_noop!(
			<AssetHubWestend as AssetHubWestendPallet>::SnowbridgeSystemFrontend::register_token(
				// The owner is Alice, while AssetHubWestendReceiver is Bob, so it should fail
				RuntimeOrigin::signed(AssetHubWestendReceiver::get()),
				bx!(VersionedLocation::from(usdt_at_ah_westend())),
				AssetMetadata {
					name: "usdt".as_bytes().to_vec().try_into().unwrap(),
					symbol: "usdt".as_bytes().to_vec().try_into().unwrap(),
					decimals: 6,
				},
				fees_asset
			),
			BadOrigin
		);
	});
}

#[test]
pub fn register_relay_token_from_asset_hub_user_origin_will_fail() {
	fund_on_bh();
	register_assets_on_ah();
	fund_on_ah();
	AssetHubWestend::execute_with(|| {
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let fees_asset =
			Asset { id: AssetId(ethereum()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_ETHER) };

		assert_noop!(
			<AssetHubWestend as AssetHubWestendPallet>::SnowbridgeSystemFrontend::register_token(
				RuntimeOrigin::signed(AssetHubWestendSender::get()),
				bx!(VersionedLocation::from(Location { parents: 1, interior: [].into() })),
				AssetMetadata {
					name: "wnd".as_bytes().to_vec().try_into().unwrap(),
					symbol: "wnd".as_bytes().to_vec().try_into().unwrap(),
					decimals: 12,
				},
				fees_asset
			),
			BadOrigin
		);
	});
}

pub const INSUFFICIENT_REMOTE_FEE_AMOUNT: u128 = 1_000_000_000;

// Test that the asset trapping and claim flow work correctly.
#[test]
fn transfer_from_penpal_to_ethereum_trapped_on_ah_and_then_claim_can_work() {
	create_pools_on_ah();
	register_pal_on_ah();
	register_pal_on_bh();
	fund_on_ah();
	// penpal
	set_trust_reserve_on_penpal();
	register_assets_on_penpal();
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

	// Since fee is insufficient, asset got trapped on AH
	PenpalB::execute_with(|| {
		type RuntimeOrigin = <PenpalB as Chain>::RuntimeOrigin;

		let local_fee_asset_on_penpal =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };

		let remote_fee_asset_on_ah =
			Asset { id: AssetId(ethereum()), fun: Fungible(INSUFFICIENT_REMOTE_FEE_AMOUNT) };

		let remote_fee_asset_on_ethereum =
			Asset { id: AssetId(ethereum()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_ETHER) };

		let ena = Asset { id: AssetId(weth_location()), fun: Fungible(TOKEN_AMOUNT) };

		let assets = vec![
			local_fee_asset_on_penpal.clone(),
			remote_fee_asset_on_ah.clone(),
			remote_fee_asset_on_ethereum.clone(),
			ena.clone(),
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
				assets: BoundedVec::truncate_from(vec![
					AssetTransferFilter::ReserveWithdraw(Definite(
						remote_fee_asset_on_ethereum.clone().into(),
					)),
					AssetTransferFilter::ReserveWithdraw(Definite(ena.clone().into())),
				]),
				remote_xcm: Xcm(vec![
					SetAppendix(Xcm(vec![SetHints {
						hints: vec![AssetClaimer { location: penpal_user_location }]
							.try_into()
							.unwrap(),
					}])),
					InitiateTransfer {
						destination: ethereum(),
						remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
							remote_fee_asset_on_ethereum.clone().into(),
						))),
						preserve_origin: true,
						assets: BoundedVec::truncate_from(vec![
							AssetTransferFilter::ReserveWithdraw(Definite(ena.clone().into())),
						]),
						remote_xcm: Xcm(vec![DepositAsset {
							assets: Wild(All),
							beneficiary: beneficiary(),
						}]),
					},
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
			vec![RuntimeEvent::PolkadotXcm(pallet_xcm::Event::ProcessXcmError { .. }) => {},]
		);
	});

	// Claim the trapped asset and deposit on AH.
	PenpalB::execute_with(|| {
		type RuntimeOrigin = <PenpalB as Chain>::RuntimeOrigin;

		let local_fee_asset_on_penpal =
			Asset { id: AssetId(Location::parent()), fun: Fungible(LOCAL_FEE_AMOUNT_IN_DOT) };

		let remote_fee_asset_on_ah =
			Asset { id: AssetId(ethereum()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_ETHER) };

		let assets = vec![local_fee_asset_on_penpal.clone(), remote_fee_asset_on_ah.clone()];

		let xcm = VersionedXcm::from(Xcm(vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: local_fee_asset_on_penpal.clone() },
			InitiateTransfer {
				destination: asset_hub(),
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(Definite(
					remote_fee_asset_on_ah.clone().into(),
				))),
				preserve_origin: true,
				assets: BoundedVec::truncate_from(vec![]),
				remote_xcm: Xcm(vec![
					ClaimAsset {
						assets: vec![
							Asset { id: AssetId(ethereum()), fun: Fungible(600422871584) },
							Asset { id: AssetId(weth_location()), fun: Fungible(TOKEN_AMOUNT) },
						]
						.into(),
						ticket: GeneralIndex(5).into(),
					},
					RefundSurplus,
					DepositAsset {
						assets: Wild(All),
						beneficiary: AssetHubWestendReceiver::get().into(),
					},
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
			vec![RuntimeEvent::PolkadotXcm(pallet_xcm::Event::AssetsClaimed { .. }) => {},]
		);
	});
}

// A malicious user attempted to exploit the bridge by manually adding an AliasOrigin in the
// remoteXcm, successfully routing to the V2 path, but ultimately failing at the BH Exporter.
#[test]
pub fn exploit_v2_route_with_legacy_v1_transfer_will_fail() {
	create_pools_on_ah();
	fund_on_ah();

	let remote_fee_asset =
		Asset { id: AssetId(eth_location()), fun: Fungible(REMOTE_FEE_AMOUNT_IN_ETHER) };

	let reserve_asset = Asset { id: AssetId(eth_location()), fun: Fungible(TOKEN_AMOUNT) };

	let assets = vec![reserve_asset.clone(), remote_fee_asset.clone()];

	let custom_xcm_on_dest = Xcm::<()>(vec![
		AliasOrigin(Location::parent()),
		DepositAsset { assets: Wild(AllCounted(2)), beneficiary: beneficiary() },
	]);

	assert_ok!(AssetHubWestend::execute_with(|| {
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			bx!(eth_location().into()),
			bx!(assets.into()),
			bx!(TransferType::DestinationReserve),
			bx!(AssetId(eth_location()).into()),
			bx!(TransferType::DestinationReserve),
			bx!(VersionedXcm::from(custom_xcm_on_dest)),
			Unlimited,
		)
	}));

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		// Check that the process failed in MessageQueue
		assert_expected_events!(
			BridgeHubWestend,
			vec![
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed{ success: false, .. }) => {},
			]
		);
	})
}
