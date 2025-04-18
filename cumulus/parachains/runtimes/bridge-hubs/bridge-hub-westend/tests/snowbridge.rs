// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

#![cfg(test)]

use bp_asset_hub_westend::ASSET_HUB_WESTEND_PARACHAIN_ID;
use bp_bridge_hub_westend::BRIDGE_HUB_WESTEND_PARACHAIN_ID;
use bp_polkadot_core::Signature;
use bridge_hub_westend_runtime::{
	bridge_to_rococo_config, xcm_config::XcmConfig, AllPalletsWithoutSystem,
	BridgeRejectObsoleteHeadersAndMessages, Executive, MessageQueueServiceWeight, Runtime,
	RuntimeCall, RuntimeEvent, SessionKeys, TxExtension, UncheckedExtrinsic,
};
use codec::{Decode, Encode};
use cumulus_primitives_core::XcmError::{FailedToTransactAsset, NotHoldingFees};
use frame_support::parameter_types;
use parachains_common::{AccountId, AuraId, Balance};
use snowbridge_pallet_ethereum_client::WeightInfo;
use sp_core::H160;
use sp_runtime::{
	generic::{Era, SignedPayload},
	AccountId32,
};

parameter_types! {
		pub const DefaultBridgeHubEthereumBaseFee: Balance = 3_833_568_200_000;
}

fn collator_session_keys() -> bridge_hub_test_utils::CollatorSessionKeys<Runtime> {
	use sp_keyring::Sr25519Keyring::Alice;
	bridge_hub_test_utils::CollatorSessionKeys::new(
		AccountId::from(Alice),
		AccountId::from(Alice),
		SessionKeys { aura: AuraId::from(Alice.public()) },
	)
}

#[test]
pub fn transfer_token_to_ethereum_works() {
	snowbridge_runtime_test_common::send_transfer_token_message_success::<Runtime, XcmConfig>(
		11155111,
		collator_session_keys(),
		BRIDGE_HUB_WESTEND_PARACHAIN_ID,
		ASSET_HUB_WESTEND_PARACHAIN_ID,
		H160::random(),
		H160::random(),
		DefaultBridgeHubEthereumBaseFee::get(),
		Box::new(|runtime_event_encoded: Vec<u8>| {
			match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
				Ok(RuntimeEvent::EthereumOutboundQueue(event)) => Some(event),
				_ => None,
			}
		}),
	)
}

#[test]
pub fn unpaid_transfer_token_to_ethereum_fails_with_barrier() {
	snowbridge_runtime_test_common::send_unpaid_transfer_token_message::<Runtime, XcmConfig>(
		11155111,
		collator_session_keys(),
		BRIDGE_HUB_WESTEND_PARACHAIN_ID,
		ASSET_HUB_WESTEND_PARACHAIN_ID,
		H160::random(),
		H160::random(),
	)
}

#[test]
pub fn transfer_token_to_ethereum_fee_not_enough() {
	snowbridge_runtime_test_common::send_transfer_token_message_failure::<Runtime, XcmConfig>(
		11155111,
		collator_session_keys(),
		BRIDGE_HUB_WESTEND_PARACHAIN_ID,
		ASSET_HUB_WESTEND_PARACHAIN_ID,
		DefaultBridgeHubEthereumBaseFee::get() + 20_000_000_000,
		H160::random(),
		H160::random(),
		// fee not enough
		20_000_000_000,
		NotHoldingFees,
	)
}

#[test]
pub fn transfer_token_to_ethereum_insufficient_fund() {
	snowbridge_runtime_test_common::send_transfer_token_message_failure::<Runtime, XcmConfig>(
		11155111,
		collator_session_keys(),
		BRIDGE_HUB_WESTEND_PARACHAIN_ID,
		ASSET_HUB_WESTEND_PARACHAIN_ID,
		1_000_000_000,
		H160::random(),
		H160::random(),
		DefaultBridgeHubEthereumBaseFee::get(),
		FailedToTransactAsset("Funds are unavailable"),
	)
}

#[test]
fn max_message_queue_service_weight_is_more_than_beacon_extrinsic_weights() {
	let max_message_queue_weight = MessageQueueServiceWeight::get();
	let force_checkpoint =
		<Runtime as snowbridge_pallet_ethereum_client::Config>::WeightInfo::force_checkpoint();
	let submit_checkpoint =
		<Runtime as snowbridge_pallet_ethereum_client::Config>::WeightInfo::submit();
	max_message_queue_weight.all_gt(force_checkpoint);
	max_message_queue_weight.all_gt(submit_checkpoint);
}

#[test]
fn ethereum_client_consensus_extrinsics_work() {
	snowbridge_runtime_test_common::ethereum_extrinsic(
		collator_session_keys(),
		BRIDGE_HUB_WESTEND_PARACHAIN_ID,
		construct_and_apply_extrinsic,
	);
}

#[test]
fn ethereum_to_polkadot_message_extrinsics_work() {
	snowbridge_runtime_test_common::ethereum_to_polkadot_message_extrinsics_work(
		collator_session_keys(),
		BRIDGE_HUB_WESTEND_PARACHAIN_ID,
		construct_and_apply_extrinsic,
	);
}

/// Tests that the digest items are as expected when a Ethereum Outbound message is received.
/// If the MessageQueue pallet is configured before (i.e. the MessageQueue pallet is listed before
/// the EthereumOutboundQueue in the construct_runtime macro) the EthereumOutboundQueue, this test
/// will fail.
#[test]
pub fn ethereum_outbound_queue_processes_messages_before_message_queue_works() {
	snowbridge_runtime_test_common::ethereum_outbound_queue_processes_messages_before_message_queue_works::<
		Runtime,
		XcmConfig,
		AllPalletsWithoutSystem,
	>(
		11155111,
		collator_session_keys(),
		BRIDGE_HUB_WESTEND_PARACHAIN_ID,
		ASSET_HUB_WESTEND_PARACHAIN_ID,
		H160::random(),
		H160::random(),
		DefaultBridgeHubEthereumBaseFee::get(),
		Box::new(|runtime_event_encoded: Vec<u8>| {
			match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
				Ok(RuntimeEvent::EthereumOutboundQueue(event)) => Some(event),
				_ => None,
			}
		}),
	)
}

fn construct_extrinsic(
	sender: sp_keyring::Sr25519Keyring,
	call: RuntimeCall,
) -> UncheckedExtrinsic {
	let account_id = AccountId32::from(sender.public());
	let extra: TxExtension = (
		frame_system::CheckNonZeroSender::<Runtime>::new(),
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckTxVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckEra::<Runtime>::from(Era::immortal()),
		frame_system::CheckNonce::<Runtime>::from(
			frame_system::Pallet::<Runtime>::account(&account_id).nonce,
		),
		frame_system::CheckWeight::<Runtime>::new(),
		pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0),
		BridgeRejectObsoleteHeadersAndMessages::default(),
		(bridge_to_rococo_config::OnBridgeHubWestendRefundBridgeHubRococoMessages::default(),),
		frame_metadata_hash_extension::CheckMetadataHash::<Runtime>::new(false),
	)
		.into();
	let payload = SignedPayload::new(call.clone(), extra.clone()).unwrap();
	let signature = payload.using_encoded(|e| sender.sign(e));
	UncheckedExtrinsic::new_signed(call, account_id.into(), Signature::Sr25519(signature), extra)
}

fn construct_and_apply_extrinsic(
	origin: sp_keyring::Sr25519Keyring,
	call: RuntimeCall,
) -> sp_runtime::DispatchOutcome {
	let xt = construct_extrinsic(origin, call);
	let r = Executive::apply_extrinsic(xt);
	r.unwrap()
}
