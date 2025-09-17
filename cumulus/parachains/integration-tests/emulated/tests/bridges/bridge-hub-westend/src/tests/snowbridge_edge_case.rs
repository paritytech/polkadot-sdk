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

use crate::{imports::*, tests::snowbridge_common::*};
use bridge_hub_westend_runtime::xcm_config::LocationToAccountId;
use emulated_integration_tests_common::snowbridge::{SEPOLIA_ID, WETH};
use frame_support::assert_noop;
use hex_literal::hex;
use snowbridge_beacon_primitives::{
	types::deneb, BeaconHeader, ExecutionProof, VersionedExecutionPayloadHeader,
};
use snowbridge_core::AssetMetadata;
use snowbridge_inbound_queue_primitives::{EventProof, Log, Proof};
use snowbridge_pallet_inbound_queue::{
	Error as InboundQueueError, SendError as InboundQueueSendError,
};
use snowbridge_pallet_system::Error as SystemError;
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;
use xcm_executor::traits::ConvertLocation;

// The user origin should be banned in ethereum_blob_exporter with error logs
// xcm::ethereum_blob_exporter: could not get parachain id from universal source
// 'X2([Parachain(1000), AccountId32 {...}])'
#[test]
fn user_send_message_directly_bypass_exporter_from_ah_will_fail() {
	fund_on_bh();
	register_assets_on_ah();
	fund_on_ah();
	create_pools_on_ah();

	let sov_account_for_sender = LocationToAccountId::convert_location(&Location::new(
		1,
		[
			Parachain(AssetHubWestend::para_id().into()),
			AccountId32 {
				network: Some(ByGenesis(WESTEND_GENESIS_HASH)),
				id: AssetHubWestendSender::get().into(),
			},
		],
	))
	.unwrap();
	BridgeHubWestend::fund_accounts(vec![(sov_account_for_sender, INITIAL_FUND)]);

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(1_000_000_000_000) };

		let weth_location_reanchored =
			Location::new(0, [AccountKey20 { network: None, key: WETH.into() }]);

		let weth_asset = Asset {
			id: AssetId(weth_location_reanchored.clone()),
			fun: Fungible(TOKEN_AMOUNT * 1_000_000_000),
		};

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::send(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			bx!(VersionedLocation::from(bridge_hub())),
			bx!(VersionedXcm::from(Xcm(vec![
				WithdrawAsset(local_fee_asset.clone().into()),
				BuyExecution { fees: local_fee_asset.clone(), weight_limit: Unlimited },
				ExportMessage {
					network: Ethereum { chain_id: SEPOLIA_ID },
					destination: Here,
					xcm: Xcm(vec![
						WithdrawAsset(weth_asset.clone().into()),
						DepositAsset { assets: Wild(All), beneficiary: beneficiary() },
						SetTopic([0; 32]),
					]),
				},
			]))),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent{ .. }) => {},]
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed{ success:false, .. }) => {},]
		);
	});
}

// ENA is not allowed to be registered as PNA
#[test]
fn test_register_ena_on_bh_will_fail() {
	BridgeHubWestend::execute_with(|| {
		type RuntimeOrigin = <BridgeHubWestend as Chain>::RuntimeOrigin;
		type Runtime = <BridgeHubWestend as Chain>::Runtime;

		assert_ok!(<BridgeHubWestend as BridgeHubWestendPallet>::Balances::force_set_balance(
			RuntimeOrigin::root(),
			sp_runtime::MultiAddress::Id(BridgeHubWestendSender::get()),
			INITIAL_FUND * 10,
		));

		assert_err!(
			<BridgeHubWestend as BridgeHubWestendPallet>::EthereumSystem::register_token(
				RuntimeOrigin::root(),
				Box::new(VersionedLocation::from(weth_location())),
				AssetMetadata {
					name: "weth".as_bytes().to_vec().try_into().unwrap(),
					symbol: "weth".as_bytes().to_vec().try_into().unwrap(),
					decimals: 18,
				},
			),
			SystemError::<Runtime>::LocationConversionFailed
		);
	});
}

#[test]
fn user_exploit_with_arbitrary_message_will_fail() {
	fund_on_bh();
	register_assets_on_ah();
	fund_on_ah();
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let remote_fee_asset_location: Location =
			Location::new(2, [EthereumNetwork::get().into()]).into();

		let remote_fee_asset: Asset = (remote_fee_asset_location.clone(), 1).into();

		let assets = VersionedAssets::from(vec![remote_fee_asset]);

		let exploited_weth = Asset {
			id: AssetId(Location::new(0, [AccountKey20 { network: None, key: WETH.into() }])),
			// A big amount without burning
			fun: Fungible(TOKEN_AMOUNT * 1_000_000_000),
		};

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			bx!(VersionedLocation::from(ethereum())),
			bx!(assets),
			bx!(TransferType::DestinationReserve),
			bx!(VersionedAssetId::from(remote_fee_asset_location.clone())),
			bx!(TransferType::DestinationReserve),
			// exploited_weth here is far more than the burnt, which means instructions inner
			// are user provided and untrustworthy/dangerous!
			// Currently it depends on EthereumBlobExporter on BH to check the message is legal
			// and convert to Ethereum command.
			bx!(VersionedXcm::from(Xcm(vec![
				WithdrawAsset(exploited_weth.clone().into()),
				DepositAsset { assets: Wild(All), beneficiary: beneficiary() },
				SetTopic([0; 32]),
			]))),
			Unlimited
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent{ .. }) => {},]
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed{ success:false, .. }) => {},]
		);
	});
}

#[test]
fn export_from_system_parachain_but_not_root_will_fail() {
	fund_on_bh();
	register_assets_on_ah();
	fund_on_ah();
	create_pools_on_ah();

	let sub_location = PalletInstance(100);
	let assethub_pallet_sovereign = BridgeHubWestend::sovereign_account_id_of(Location::new(
		1,
		[Parachain(AssetHubWestend::para_id().into()), sub_location],
	));
	BridgeHubWestend::fund_accounts(vec![(assethub_pallet_sovereign.clone(), INITIAL_FUND)]);

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		let local_fee_asset =
			Asset { id: AssetId(Location::parent()), fun: Fungible(1_000_000_000_000) };

		let weth_location_reanchored =
			Location::new(0, [AccountKey20 { network: None, key: WETH.into() }]);

		let weth_asset = Asset {
			id: AssetId(weth_location_reanchored.clone()),
			fun: Fungible(TOKEN_AMOUNT * 1_000_000_000),
		};

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::send(
			RuntimeOrigin::root(),
			bx!(VersionedLocation::from(bridge_hub())),
			bx!(VersionedXcm::from(Xcm(vec![
				DescendOrigin(sub_location.into()),
				WithdrawAsset(local_fee_asset.clone().into()),
				BuyExecution { fees: local_fee_asset.clone(), weight_limit: Unlimited },
				ExportMessage {
					network: Ethereum { chain_id: SEPOLIA_ID },
					destination: Here,
					xcm: Xcm(vec![
						WithdrawAsset(weth_asset.clone().into()),
						DepositAsset { assets: Wild(All), beneficiary: beneficiary() },
						SetTopic([0; 32]),
					]),
				},
			]))),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent{ .. }) => {},]
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed{ success:false, .. }) => {},]
		);
	});
}

#[test]
fn export_from_non_system_parachain_will_fail() {
	let penpal_sovereign = BridgeHubWestend::sovereign_account_id_of(Location::new(
		1,
		[Parachain(PenpalB::para_id().into())],
	));
	BridgeHubWestend::fund_accounts(vec![(penpal_sovereign.clone(), INITIAL_FUND)]);

	PenpalB::execute_with(|| {
		type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
		type RuntimeOrigin = <PenpalB as Chain>::RuntimeOrigin;

		let local_fee_asset =
			Asset { id: AssetId(Location::here()), fun: Fungible(1_000_000_000_000) };

		let weth_location_reanchored =
			Location::new(0, [AccountKey20 { network: None, key: WETH.into() }]);

		let weth_asset =
			Asset { id: AssetId(weth_location_reanchored.clone()), fun: Fungible(TOKEN_AMOUNT) };

		assert_ok!(<PenpalB as PenpalBPallet>::PolkadotXcm::send(
			RuntimeOrigin::root(),
			bx!(VersionedLocation::from(bridge_hub())),
			bx!(VersionedXcm::from(Xcm(vec![
				WithdrawAsset(local_fee_asset.clone().into()),
				BuyExecution { fees: local_fee_asset.clone(), weight_limit: Unlimited },
				ExportMessage {
					network: Ethereum { chain_id: SEPOLIA_ID },
					destination: Here,
					xcm: Xcm(vec![
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
			vec![RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed{ success:false, origin, .. }) => {
				origin: *origin == bridge_hub_common::AggregateMessageOrigin::Sibling(PenpalB::para_id()),
			},]
		);
	});
}

pub fn mock_event_log() -> Log {
	Log {
		// gateway address
		address: hex!("eda338e4dc46038493b885327842fd3e301cab39").into(),
		topics: vec![
			hex!("7153f9357c8ea496bba60bf82e67143e27b64462b49041f8e689e1b05728f84f").into(),
			// channel id
			hex!("c173fac324158e77fb5840738a1a541f633cbec8884c6a601c567d2b376a0539").into(),
			// message id
			hex!("5f7060e971b0dc81e63f0aa41831091847d97c1a4693ac450cc128c7214e65e0").into(),
		],
		// Nonce + Payload
		data: hex!("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000002e000f000000000000000087d1f7fdfee7f651fabc8bfcb6e086c278b77a7d00e40b54020000000000000000000000000000000000000000000000000000000000").into(),
	}
}

pub fn mock_execution_proof() -> ExecutionProof {
	ExecutionProof {
		header: BeaconHeader::default(),
		ancestry_proof: None,
		execution_header: VersionedExecutionPayloadHeader::Deneb(deneb::ExecutionPayloadHeader {
			parent_hash: Default::default(),
			fee_recipient: Default::default(),
			state_root: Default::default(),
			receipts_root: Default::default(),
			logs_bloom: vec![],
			prev_randao: Default::default(),
			block_number: 0,
			gas_limit: 0,
			gas_used: 0,
			timestamp: 0,
			extra_data: vec![],
			base_fee_per_gas: Default::default(),
			block_hash: Default::default(),
			transactions_root: Default::default(),
			withdrawals_root: Default::default(),
			blob_gas_used: 0,
			excess_blob_gas: 0,
		}),
		execution_branch: vec![],
	}
}

/// Tests the full cycle of eth transfers:
/// - sending a token to AssetHub
/// - returning the token to Ethereum
#[test]
fn send_eth_asset_from_ethereum_to_parachain_fail_and_storage_rollback() {
	let assethub_location = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let assethub_sovereign = BridgeHubWestend::sovereign_account_id_of(assethub_location);
	BridgeHubWestend::fund_accounts(vec![(assethub_sovereign.clone(), INITIAL_FUND)]);

	// Send the token
	BridgeHubWestend::execute_with(|| {
		let free_balance_before =
			<BridgeHubWestend as BridgeHubWestendPallet>::Balances::free_balance(
				assethub_sovereign.clone(),
			);
		assert_eq!(free_balance_before, INITIAL_FUND);
		type RuntimeOrigin = <BridgeHubWestend as Chain>::RuntimeOrigin;

		type Runtime = <BridgeHubWestend as Chain>::Runtime;

		type EthereumInboundQueue =
			<BridgeHubWestend as BridgeHubWestendPallet>::EthereumInboundQueue;

		let event = EventProof {
			event_log: mock_event_log(),
			proof: Proof {
				receipt_proof: Default::default(),
				execution_proof: mock_execution_proof(),
			},
		};

		assert_noop!(
			EthereumInboundQueue::submit_without_verification(
				RuntimeOrigin::signed(BridgeHubWestendSender::get()),
				event
			),
			InboundQueueError::<Runtime>::Send(InboundQueueSendError::Transport)
		);

		let free_balance_after =
			<BridgeHubWestend as BridgeHubWestendPallet>::Balances::free_balance(
				assethub_sovereign,
			);
		assert_eq!(free_balance_after, INITIAL_FUND);
	});
}
