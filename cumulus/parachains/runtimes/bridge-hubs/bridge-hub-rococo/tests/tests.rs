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

#![cfg(test)]

use bp_polkadot_core::Signature;
use bridge_hub_rococo_runtime::{
	bridge_common_config, bridge_hub_rococo_config, bridge_hub_wococo_config,
	xcm_config::{RelayNetwork, TokenLocation, XcmConfig},
	AllPalletsWithoutSystem, BridgeRejectObsoleteHeadersAndMessages, Executive, ExistentialDeposit,
	ParachainSystem, PolkadotXcm, Runtime, RuntimeCall, RuntimeEvent, SessionKeys, SignedExtra,
	UncheckedExtrinsic,
};
use codec::{Decode, Encode};
use frame_support::parameter_types;
use frame_system::pallet_prelude::HeaderFor;
use parachains_common::{rococo::fee::WeightToFee, AccountId, AuraId, Balance};
use sp_keyring::AccountKeyring::Alice;
use sp_runtime::{
	generic::{Era, SignedPayload},
	AccountId32,
};
use xcm::latest::prelude::*;

// Para id of sibling chain used in tests.
pub const SIBLING_PARACHAIN_ID: u32 = 1000;

parameter_types! {
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
}

fn construct_extrinsic(
	sender: sp_keyring::AccountKeyring,
	call: RuntimeCall,
) -> UncheckedExtrinsic {
	let extra: SignedExtra = (
		frame_system::CheckNonZeroSender::<Runtime>::new(),
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckTxVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckEra::<Runtime>::from(Era::immortal()),
		frame_system::CheckNonce::<Runtime>::from(0),
		frame_system::CheckWeight::<Runtime>::new(),
		pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0),
		BridgeRejectObsoleteHeadersAndMessages::default(),
		(
			bridge_hub_wococo_config::BridgeRefundBridgeHubRococoMessages::default(),
			bridge_hub_rococo_config::BridgeRefundBridgeHubWococoMessages::default(),
		),
	);
	let payload = SignedPayload::new(call.clone(), extra.clone()).unwrap();
	let signature = payload.using_encoded(|e| sender.sign(e));
	UncheckedExtrinsic::new_signed(
		call,
		AccountId32::from(sender.public()).into(),
		Signature::Sr25519(signature.clone()),
		extra,
	)
}

fn construct_and_apply_extrinsic(
	relayer_at_target: sp_keyring::AccountKeyring,
	batch: pallet_utility::Call<Runtime>,
) -> sp_runtime::DispatchOutcome {
	let batch_call = RuntimeCall::Utility(batch);
	let xt = construct_extrinsic(relayer_at_target, batch_call);
	let r = Executive::apply_extrinsic(xt);
	r.unwrap()
}

fn executive_init_block(header: &HeaderFor<Runtime>) {
	Executive::initialize_block(header)
}

fn collator_session_keys() -> bridge_hub_test_utils::CollatorSessionKeys<Runtime> {
	bridge_hub_test_utils::CollatorSessionKeys::new(
		AccountId::from(Alice),
		AccountId::from(Alice),
		SessionKeys { aura: AuraId::from(Alice.public()) },
	)
}

mod bridge_hub_rococo_tests {
	use super::*;
	use bridge_common_config::{
		BridgeGrandpaWococoInstance, BridgeParachainWococoInstance, DeliveryRewardInBalance,
		RequiredStakeForStakeAndSlash,
	};
	use bridge_hub_rococo_config::{
		WithBridgeHubWococoMessageBridge, WithBridgeHubWococoMessagesInstance,
		DEFAULT_XCM_LANE_TO_BRIDGE_HUB_WOCOCO,
	};

	bridge_hub_test_utils::test_cases::include_teleports_for_native_asset_works!(
		Runtime,
		AllPalletsWithoutSystem,
		XcmConfig,
		CheckingAccount,
		WeightToFee,
		ParachainSystem,
		collator_session_keys(),
		ExistentialDeposit::get(),
		Box::new(|runtime_event_encoded: Vec<u8>| {
			match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
				Ok(RuntimeEvent::PolkadotXcm(event)) => Some(event),
				_ => None,
			}
		}),
		bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID
	);

	#[test]
	fn initialize_bridge_by_governance_works() {
		bridge_hub_test_utils::test_cases::initialize_bridge_by_governance_works::<
			Runtime,
			BridgeGrandpaWococoInstance,
		>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			Box::new(|call| RuntimeCall::BridgeWococoGrandpa(call).encode()),
		)
	}

	#[test]
	fn change_delivery_reward_by_governance_works() {
		bridge_hub_test_utils::test_cases::change_storage_constant_by_governance_works::<
			Runtime,
			DeliveryRewardInBalance,
			u64,
		>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			Box::new(|call| RuntimeCall::System(call).encode()),
			|| (DeliveryRewardInBalance::key().to_vec(), DeliveryRewardInBalance::get()),
			|old_value| old_value.checked_mul(2).unwrap(),
		)
	}

	#[test]
	fn change_required_stake_by_governance_works() {
		bridge_hub_test_utils::test_cases::change_storage_constant_by_governance_works::<
			Runtime,
			RequiredStakeForStakeAndSlash,
			Balance,
		>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			Box::new(|call| RuntimeCall::System(call).encode()),
			|| {
				(
					RequiredStakeForStakeAndSlash::key().to_vec(),
					RequiredStakeForStakeAndSlash::get(),
				)
			},
			|old_value| old_value.checked_mul(2).unwrap(),
		)
	}

	#[test]
	fn handle_export_message_from_system_parachain_add_to_outbound_queue_works() {
		bridge_hub_test_utils::test_cases::handle_export_message_from_system_parachain_to_outbound_queue_works::<
			Runtime,
			XcmConfig,
			WithBridgeHubWococoMessagesInstance,
		>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			Box::new(|runtime_event_encoded: Vec<u8>| {
				match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
					Ok(RuntimeEvent::BridgeRococoToWococoMessages(event)) => Some(event),
					_ => None,
				}
			}),
			|| ExportMessage { network: Wococo, destination: X1(Parachain(1234)), xcm: Xcm(vec![]) },
			bridge_hub_rococo_config::DEFAULT_XCM_LANE_TO_BRIDGE_HUB_WOCOCO,
			Some((TokenLocation::get(), ExistentialDeposit::get()).into()),
			// value should be >= than value generated by `can_calculate_weight_for_paid_export_message_with_reserve_transfer`
			Some((TokenLocation::get(), bp_asset_hub_rococo::BridgeHubRococoBaseFeeInRocs::get()).into()),
			|| (),
		)
	}

	#[test]
	fn message_dispatch_routing_works() {
		bridge_hub_test_utils::test_cases::message_dispatch_routing_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			WithBridgeHubWococoMessagesInstance,
			RelayNetwork,
			bridge_hub_rococo_config::WococoGlobalConsensusNetwork,
		>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			Box::new(|runtime_event_encoded: Vec<u8>| {
				match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
					Ok(RuntimeEvent::ParachainSystem(event)) => Some(event),
					_ => None,
				}
			}),
			Box::new(|runtime_event_encoded: Vec<u8>| {
				match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
					Ok(RuntimeEvent::XcmpQueue(event)) => Some(event),
					_ => None,
				}
			}),
			bridge_hub_rococo_config::DEFAULT_XCM_LANE_TO_BRIDGE_HUB_WOCOCO,
			|| (),
		)
	}

	#[test]
	fn relayed_incoming_message_works() {
		bridge_hub_test_utils::test_cases::relayed_incoming_message_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			BridgeGrandpaWococoInstance,
			BridgeParachainWococoInstance,
			WithBridgeHubWococoMessagesInstance,
			WithBridgeHubWococoMessageBridge,
		>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			Rococo,
			DEFAULT_XCM_LANE_TO_BRIDGE_HUB_WOCOCO,
			|| (),
		)
	}

	#[test]
	pub fn complex_relay_extrinsic_works() {
		bridge_hub_test_utils::test_cases::complex_relay_extrinsic_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			BridgeGrandpaWococoInstance,
			BridgeParachainWococoInstance,
			WithBridgeHubWococoMessagesInstance,
			WithBridgeHubWococoMessageBridge,
		>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			bridge_hub_rococo_config::BridgeHubWococoChainId::get(),
			Rococo,
			DEFAULT_XCM_LANE_TO_BRIDGE_HUB_WOCOCO,
			ExistentialDeposit::get(),
			executive_init_block,
			construct_and_apply_extrinsic,
			|| (),
		);
	}

	#[test]
	pub fn can_calculate_weight_for_paid_export_message_with_reserve_transfer() {
		let estimated = bridge_hub_test_utils::test_cases::can_calculate_weight_for_paid_export_message_with_reserve_transfer::<
			Runtime,
			XcmConfig,
			WeightToFee,
		>();

		// check if estimated value is sane
		let max_expected = bp_asset_hub_rococo::BridgeHubRococoBaseFeeInRocs::get();
		assert!(
			estimated <= max_expected,
			"calculated: {:?}, max_expected: {:?}, please adjust `bp_asset_hub_rococo::BridgeHubRococoBaseFeeInRocs` value",
			estimated,
			max_expected
		);
	}
}

mod bridge_hub_wococo_tests {
	use super::*;
	use bridge_common_config::{
		BridgeGrandpaRococoInstance, BridgeParachainRococoInstance, DeliveryRewardInBalance,
		RequiredStakeForStakeAndSlash,
	};
	use bridge_hub_rococo_runtime::{xcm_config, AllPalletsWithoutSystem, RuntimeFlavor};
	use bridge_hub_wococo_config::{
		WithBridgeHubRococoMessageBridge, WithBridgeHubRococoMessagesInstance,
		DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO,
	};
	use frame_support::assert_ok;

	type RuntimeHelper = bridge_hub_test_utils::RuntimeHelper<Runtime, AllPalletsWithoutSystem>;

	pub(crate) fn set_wococo_flavor() {
		let flavor_key = xcm_config::Flavor::key().to_vec();
		let flavor = RuntimeFlavor::Wococo;

		// encode `set_storage` call
		let set_storage_call = RuntimeCall::System(frame_system::Call::<Runtime>::set_storage {
			items: vec![(flavor_key, flavor.encode())],
		})
		.encode();

		// estimate - storing just 1 value
		use frame_system::WeightInfo;
		let require_weight_at_most =
			<Runtime as frame_system::Config>::SystemWeightInfo::set_storage(1);

		// execute XCM with Transact to `set_storage` as governance does
		assert_ok!(RuntimeHelper::execute_as_governance(set_storage_call, require_weight_at_most)
			.ensure_complete());

		// check if stored
		assert_eq!(flavor, xcm_config::Flavor::get());
	}

	bridge_hub_test_utils::test_cases::include_teleports_for_native_asset_works!(
		Runtime,
		AllPalletsWithoutSystem,
		XcmConfig,
		CheckingAccount,
		WeightToFee,
		ParachainSystem,
		collator_session_keys(),
		ExistentialDeposit::get(),
		Box::new(|runtime_event_encoded: Vec<u8>| {
			match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
				Ok(RuntimeEvent::PolkadotXcm(event)) => Some(event),
				_ => None,
			}
		}),
		bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID
	);

	#[test]
	fn initialize_bridge_by_governance_works() {
		bridge_hub_test_utils::test_cases::initialize_bridge_by_governance_works::<
			Runtime,
			BridgeGrandpaRococoInstance,
		>(
			collator_session_keys(),
			bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID,
			Box::new(|call| RuntimeCall::BridgeRococoGrandpa(call).encode()),
		)
	}

	#[test]
	fn change_delivery_reward_by_governance_works() {
		bridge_hub_test_utils::test_cases::change_storage_constant_by_governance_works::<
			Runtime,
			DeliveryRewardInBalance,
			u64,
		>(
			collator_session_keys(),
			bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID,
			Box::new(|call| RuntimeCall::System(call).encode()),
			|| (DeliveryRewardInBalance::key().to_vec(), DeliveryRewardInBalance::get()),
			|old_value| old_value.checked_mul(2).unwrap(),
		)
	}

	#[test]
	fn change_required_stake_by_governance_works() {
		bridge_hub_test_utils::test_cases::change_storage_constant_by_governance_works::<
			Runtime,
			RequiredStakeForStakeAndSlash,
			Balance,
		>(
			collator_session_keys(),
			bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID,
			Box::new(|call| RuntimeCall::System(call).encode()),
			|| {
				(
					RequiredStakeForStakeAndSlash::key().to_vec(),
					RequiredStakeForStakeAndSlash::get(),
				)
			},
			|old_value| old_value.checked_mul(2).unwrap(),
		)
	}

	#[test]
	fn handle_export_message_from_system_parachain_add_to_outbound_queue_works() {
		bridge_hub_test_utils::test_cases::handle_export_message_from_system_parachain_to_outbound_queue_works::<
			Runtime,
			XcmConfig,
			WithBridgeHubRococoMessagesInstance,
		>(
			collator_session_keys(),
			bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			Box::new(|runtime_event_encoded: Vec<u8>| {
				match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
					Ok(RuntimeEvent::BridgeWococoToRococoMessages(event)) => Some(event),
					_ => None,
				}
			}),
			|| ExportMessage { network: Rococo, destination: X1(Parachain(4321)), xcm: Xcm(vec![]) },
			bridge_hub_wococo_config::DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO,
			Some((TokenLocation::get(), ExistentialDeposit::get()).into()),
			// value should be >= than value generated by `can_calculate_weight_for_paid_export_message_with_reserve_transfer`
			Some((TokenLocation::get(), bp_asset_hub_wococo::BridgeHubWococoBaseFeeInWocs::get()).into()),
			set_wococo_flavor,
		)
	}

	#[test]
	fn message_dispatch_routing_works() {
		bridge_hub_test_utils::test_cases::message_dispatch_routing_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			WithBridgeHubRococoMessagesInstance,
			RelayNetwork,
			bridge_hub_wococo_config::RococoGlobalConsensusNetwork,
		>(
			collator_session_keys(),
			bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			Box::new(|runtime_event_encoded: Vec<u8>| {
				match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
					Ok(RuntimeEvent::ParachainSystem(event)) => Some(event),
					_ => None,
				}
			}),
			Box::new(|runtime_event_encoded: Vec<u8>| {
				match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
					Ok(RuntimeEvent::XcmpQueue(event)) => Some(event),
					_ => None,
				}
			}),
			bridge_hub_wococo_config::DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO,
			set_wococo_flavor,
		)
	}

	#[test]
	fn relayed_incoming_message_works() {
		bridge_hub_test_utils::test_cases::relayed_incoming_message_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			BridgeGrandpaRococoInstance,
			BridgeParachainRococoInstance,
			WithBridgeHubRococoMessagesInstance,
			WithBridgeHubRococoMessageBridge,
		>(
			collator_session_keys(),
			bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID,
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			Wococo,
			DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO,
			set_wococo_flavor,
		)
	}

	#[test]
	pub fn complex_relay_extrinsic_works() {
		bridge_hub_test_utils::test_cases::complex_relay_extrinsic_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			BridgeGrandpaRococoInstance,
			BridgeParachainRococoInstance,
			WithBridgeHubRococoMessagesInstance,
			WithBridgeHubRococoMessageBridge,
		>(
			collator_session_keys(),
			bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID,
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			bridge_hub_wococo_config::BridgeHubRococoChainId::get(),
			Wococo,
			DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO,
			ExistentialDeposit::get(),
			executive_init_block,
			construct_and_apply_extrinsic,
			set_wococo_flavor,
		);
	}

	#[test]
	pub fn can_calculate_weight_for_paid_export_message_with_reserve_transfer() {
		let estimated = bridge_hub_test_utils::test_cases::can_calculate_weight_for_paid_export_message_with_reserve_transfer::<
			Runtime,
			XcmConfig,
			WeightToFee,
		>();

		// check if estimated value is sane
		let max_expected = bp_asset_hub_wococo::BridgeHubWococoBaseFeeInWocs::get();
		assert!(
			estimated <= max_expected,
			"calculated: {:?}, max_expected: {:?}, please adjust `bp_asset_hub_wococo::BridgeHubWococoBaseFeeInWocs` value",
			estimated,
			max_expected
		);
	}
}
