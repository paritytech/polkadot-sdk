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
	bridge_common_config, bridge_to_bulletin_config, bridge_to_westend_config,
	xcm_config::{RelayNetwork, TokenLocation, XcmConfig},
	AllPalletsWithoutSystem, BridgeRejectObsoleteHeadersAndMessages, EthereumGatewayAddress,
	Executive, ExistentialDeposit, ParachainSystem, PolkadotXcm, Runtime, RuntimeCall,
	RuntimeEvent, RuntimeOrigin, SessionKeys, SignedExtra, TransactionPayment, UncheckedExtrinsic,
};
use bridge_hub_test_utils::SlotDurations;
use codec::{Decode, Encode};
use frame_support::{dispatch::GetDispatchInfo, parameter_types, traits::ConstU8};
use parachains_common::{AccountId, AuraId, Balance};
use snowbridge_core::ChannelId;
use sp_consensus_aura::SlotDuration;
use sp_core::H160;
use sp_keyring::AccountKeyring::Alice;
use sp_runtime::{
	generic::{Era, SignedPayload},
	AccountId32, Perbill,
};
use testnet_parachains_constants::rococo::{consensus::*, fee::WeightToFee};
use xcm::latest::prelude::*;

parameter_types! {
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
}

fn construct_extrinsic(
	sender: sp_keyring::AccountKeyring,
	call: RuntimeCall,
) -> UncheckedExtrinsic {
	let account_id = AccountId32::from(sender.public());
	let extra: SignedExtra = (
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
		(
			bridge_to_westend_config::OnBridgeHubRococoRefundBridgeHubWestendMessages::default(),
			bridge_to_bulletin_config::OnBridgeHubRococoRefundRococoBulletinMessages::default(),
		),
	);
	let payload = SignedPayload::new(call.clone(), extra.clone()).unwrap();
	let signature = payload.using_encoded(|e| sender.sign(e));
	UncheckedExtrinsic::new_signed(
		call,
		account_id.into(),
		Signature::Sr25519(signature.clone()),
		extra,
	)
}

fn construct_and_apply_extrinsic(
	relayer_at_target: sp_keyring::AccountKeyring,
	call: RuntimeCall,
) -> sp_runtime::DispatchOutcome {
	let xt = construct_extrinsic(relayer_at_target, call);
	let r = Executive::apply_extrinsic(xt);
	r.unwrap()
}

fn construct_and_estimate_extrinsic_fee(batch: pallet_utility::Call<Runtime>) -> Balance {
	let batch_call = RuntimeCall::Utility(batch);
	let batch_info = batch_call.get_dispatch_info();
	let xt = construct_extrinsic(Alice, batch_call);
	TransactionPayment::compute_fee(xt.encoded_size() as _, &batch_info, 0)
}

fn collator_session_keys() -> bridge_hub_test_utils::CollatorSessionKeys<Runtime> {
	bridge_hub_test_utils::CollatorSessionKeys::new(
		AccountId::from(Alice),
		AccountId::from(Alice),
		SessionKeys { aura: AuraId::from(Alice.public()) },
	)
}

fn slot_durations() -> SlotDurations {
	SlotDurations {
		relay: SlotDuration::from_millis(RELAY_CHAIN_SLOT_DURATION_MILLIS.into()),
		para: SlotDuration::from_millis(SLOT_DURATION),
	}
}

bridge_hub_test_utils::test_cases::include_teleports_for_native_asset_works!(
	Runtime,
	AllPalletsWithoutSystem,
	XcmConfig,
	CheckingAccount,
	WeightToFee,
	ParachainSystem,
	collator_session_keys(),
	slot_durations(),
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
fn change_required_stake_by_governance_works() {
	bridge_hub_test_utils::test_cases::change_storage_constant_by_governance_works::<
		Runtime,
		bridge_common_config::RequiredStakeForStakeAndSlash,
		Balance,
	>(
		collator_session_keys(),
		bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
		Box::new(|call| RuntimeCall::System(call).encode()),
		|| {
			(
				bridge_common_config::RequiredStakeForStakeAndSlash::key().to_vec(),
				bridge_common_config::RequiredStakeForStakeAndSlash::get(),
			)
		},
		|old_value| old_value.checked_mul(2).unwrap(),
	)
}

mod bridge_hub_westend_tests {
	use super::*;
	use bridge_common_config::{
		BridgeGrandpaWestendInstance, BridgeParachainWestendInstance, DeliveryRewardInBalance,
	};
	use bridge_hub_test_utils::test_cases::from_parachain;
	use bridge_to_westend_config::{
		BridgeHubWestendChainId, BridgeHubWestendLocation, WestendGlobalConsensusNetwork,
		WithBridgeHubWestendMessageBridge, WithBridgeHubWestendMessagesInstance,
		XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WESTEND,
	};

	// Para id of sibling chain used in tests.
	pub const SIBLING_PARACHAIN_ID: u32 = 1000;

	// Runtime from tests PoV
	type RuntimeTestsAdapter = from_parachain::WithRemoteParachainHelperAdapter<
		Runtime,
		AllPalletsWithoutSystem,
		BridgeGrandpaWestendInstance,
		BridgeParachainWestendInstance,
		WithBridgeHubWestendMessagesInstance,
		WithBridgeHubWestendMessageBridge,
	>;

	#[test]
	fn initialize_bridge_by_governance_works() {
		// for RococoBulletin finality
		bridge_hub_test_utils::test_cases::initialize_bridge_by_governance_works::<
			Runtime,
			BridgeGrandpaWestendInstance,
		>(collator_session_keys(), bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID)
	}

	#[test]
	fn change_bridge_grandpa_pallet_mode_by_governance_works() {
		// for Westend finality
		bridge_hub_test_utils::test_cases::change_bridge_grandpa_pallet_mode_by_governance_works::<
			Runtime,
			BridgeGrandpaWestendInstance,
		>(collator_session_keys(), bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID)
	}

	#[test]
	fn change_bridge_parachains_pallet_mode_by_governance_works() {
		// for Westend finality
		bridge_hub_test_utils::test_cases::change_bridge_parachains_pallet_mode_by_governance_works::<
			Runtime,
			BridgeParachainWestendInstance,
		>(collator_session_keys(), bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID)
	}

	#[test]
	fn change_bridge_messages_pallet_mode_by_governance_works() {
		// for Westend finality
		bridge_hub_test_utils::test_cases::change_bridge_messages_pallet_mode_by_governance_works::<
			Runtime,
			WithBridgeHubWestendMessagesInstance,
		>(collator_session_keys(), bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID)
	}

	#[test]
	fn change_ethereum_gateway_by_governance_works() {
		bridge_hub_test_utils::test_cases::change_storage_constant_by_governance_works::<
			Runtime,
			EthereumGatewayAddress,
			H160,
		>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			Box::new(|call| RuntimeCall::System(call).encode()),
			|| (EthereumGatewayAddress::key().to_vec(), EthereumGatewayAddress::get()),
			|_| [1; 20].into(),
		)
	}

	#[test]
	fn change_ethereum_nonces_by_governance_works() {
		let channel_id_one: ChannelId = [1; 32].into();
		let channel_id_two: ChannelId = [2; 32].into();
		let nonce = 42;

		// Reset a single inbound channel
		bridge_hub_test_utils::test_cases::set_storage_keys_by_governance_works::<Runtime>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			Box::new(|call| RuntimeCall::System(call).encode()),
			vec![
				(snowbridge_pallet_outbound_queue::Nonce::<Runtime>::hashed_key_for::<ChannelId>(
					channel_id_one,
				)
				.to_vec(), 0u64.encode()),
				(snowbridge_pallet_inbound_queue::Nonce::<Runtime>::hashed_key_for::<ChannelId>(
					channel_id_one,
				)
				.to_vec(), 0u64.encode()),
			],
			|| {
				// Outbound
				snowbridge_pallet_outbound_queue::Nonce::<Runtime>::insert::<ChannelId, u64>(
					channel_id_one,
					nonce,
				);
				snowbridge_pallet_outbound_queue::Nonce::<Runtime>::insert::<ChannelId, u64>(
					channel_id_two,
					nonce,
				);

				// Inbound
				snowbridge_pallet_inbound_queue::Nonce::<Runtime>::insert::<ChannelId, u64>(
					channel_id_one,
					nonce,
				);
				snowbridge_pallet_inbound_queue::Nonce::<Runtime>::insert::<ChannelId, u64>(
					channel_id_two,
					nonce,
				);
			},
			|| {
				// Outbound
				assert_eq!(
					snowbridge_pallet_outbound_queue::Nonce::<Runtime>::get(channel_id_one),
					0
				);
				assert_eq!(
					snowbridge_pallet_outbound_queue::Nonce::<Runtime>::get(channel_id_two),
					nonce
				);

				// Inbound
				assert_eq!(
					snowbridge_pallet_inbound_queue::Nonce::<Runtime>::get(channel_id_one),
					0
				);
				assert_eq!(
					snowbridge_pallet_inbound_queue::Nonce::<Runtime>::get(channel_id_two),
					nonce
				);
			},
		);
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
	fn handle_export_message_from_system_parachain_add_to_outbound_queue_works() {
		// for Westend
		bridge_hub_test_utils::test_cases::handle_export_message_from_system_parachain_to_outbound_queue_works::<
			Runtime,
			XcmConfig,
			WithBridgeHubWestendMessagesInstance,
		>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			Box::new(|runtime_event_encoded: Vec<u8>| {
				match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
					Ok(RuntimeEvent::BridgeWestendMessages(event)) => Some(event),
					_ => None,
				}
			}),
			|| ExportMessage { network: Westend, destination: [Parachain(bridge_to_westend_config::AssetHubWestendParaId::get().into())].into(), xcm: Xcm(vec![]) },
			XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WESTEND,
			Some((TokenLocation::get(), ExistentialDeposit::get()).into()),
			// value should be >= than value generated by `can_calculate_weight_for_paid_export_message_with_reserve_transfer`
			Some((TokenLocation::get(), bp_bridge_hub_rococo::BridgeHubRococoBaseXcmFeeInRocs::get()).into()),
			|| PolkadotXcm::force_xcm_version(RuntimeOrigin::root(), Box::new(BridgeHubWestendLocation::get()), XCM_VERSION).expect("version saved!"),
		)
	}

	#[test]
	fn message_dispatch_routing_works() {
		// from Westend
		bridge_hub_test_utils::test_cases::message_dispatch_routing_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			WithBridgeHubWestendMessagesInstance,
			RelayNetwork,
			WestendGlobalConsensusNetwork,
			ConstU8<2>,
		>(
			collator_session_keys(),
			slot_durations(),
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
			XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WESTEND,
			|| (),
		)
	}

	#[test]
	fn relayed_incoming_message_works() {
		// from Westend
		from_parachain::relayed_incoming_message_works::<RuntimeTestsAdapter>(
			collator_session_keys(),
			slot_durations(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			bp_bridge_hub_westend::BRIDGE_HUB_WESTEND_PARACHAIN_ID,
			BridgeHubWestendChainId::get(),
			SIBLING_PARACHAIN_ID,
			Rococo,
			XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WESTEND,
			|| (),
			construct_and_apply_extrinsic,
		)
	}

	#[test]
	pub fn complex_relay_extrinsic_works() {
		// for Westend
		from_parachain::complex_relay_extrinsic_works::<RuntimeTestsAdapter>(
			collator_session_keys(),
			slot_durations(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			bp_bridge_hub_westend::BRIDGE_HUB_WESTEND_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			BridgeHubWestendChainId::get(),
			Rococo,
			XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WESTEND,
			|| (),
			construct_and_apply_extrinsic,
		);
	}

	#[test]
	pub fn can_calculate_weight_for_paid_export_message_with_reserve_transfer() {
		bridge_hub_test_utils::check_sane_fees_values(
			"bp_bridge_hub_rococo::BridgeHubRococoBaseXcmFeeInRocs",
			bp_bridge_hub_rococo::BridgeHubRococoBaseXcmFeeInRocs::get(),
			|| {
				bridge_hub_test_utils::test_cases::can_calculate_weight_for_paid_export_message_with_reserve_transfer::<
					Runtime,
					XcmConfig,
					WeightToFee,
				>()
			},
			Perbill::from_percent(33),
			Some(-33),
			&format!(
				"Estimate fee for `ExportMessage` for runtime: {:?}",
				<Runtime as frame_system::Config>::Version::get()
			),
		)
	}

	#[test]
	pub fn can_calculate_fee_for_complex_message_delivery_transaction() {
		bridge_hub_test_utils::check_sane_fees_values(
			"bp_bridge_hub_rococo::BridgeHubRococoBaseDeliveryFeeInRocs",
			bp_bridge_hub_rococo::BridgeHubRococoBaseDeliveryFeeInRocs::get(),
			|| {
				from_parachain::can_calculate_fee_for_complex_message_delivery_transaction::<
					RuntimeTestsAdapter,
				>(collator_session_keys(), construct_and_estimate_extrinsic_fee)
			},
			Perbill::from_percent(33),
			Some(-33),
			&format!(
				"Estimate fee for `single message delivery` for runtime: {:?}",
				<Runtime as frame_system::Config>::Version::get()
			),
		)
	}

	#[test]
	pub fn can_calculate_fee_for_complex_message_confirmation_transaction() {
		bridge_hub_test_utils::check_sane_fees_values(
			"bp_bridge_hub_rococo::BridgeHubRococoBaseConfirmationFeeInRocs",
			bp_bridge_hub_rococo::BridgeHubRococoBaseConfirmationFeeInRocs::get(),
			|| {
				from_parachain::can_calculate_fee_for_complex_message_confirmation_transaction::<
					RuntimeTestsAdapter,
				>(collator_session_keys(), construct_and_estimate_extrinsic_fee)
			},
			Perbill::from_percent(33),
			Some(-33),
			&format!(
				"Estimate fee for `single message confirmation` for runtime: {:?}",
				<Runtime as frame_system::Config>::Version::get()
			),
		)
	}
}

mod bridge_hub_bulletin_tests {
	use super::*;
	use bridge_common_config::BridgeGrandpaRococoBulletinInstance;
	use bridge_hub_test_utils::test_cases::from_grandpa_chain;
	use bridge_to_bulletin_config::{
		RococoBulletinChainId, RococoBulletinGlobalConsensusNetwork,
		RococoBulletinGlobalConsensusNetworkLocation, WithRococoBulletinMessageBridge,
		WithRococoBulletinMessagesInstance, XCM_LANE_FOR_ROCOCO_PEOPLE_TO_ROCOCO_BULLETIN,
	};

	// Para id of sibling chain used in tests.
	pub const SIBLING_PARACHAIN_ID: u32 = rococo_runtime_constants::system_parachain::PEOPLE_ID;

	// Runtime from tests PoV
	type RuntimeTestsAdapter = from_grandpa_chain::WithRemoteGrandpaChainHelperAdapter<
		Runtime,
		AllPalletsWithoutSystem,
		BridgeGrandpaRococoBulletinInstance,
		WithRococoBulletinMessagesInstance,
		WithRococoBulletinMessageBridge,
	>;

	#[test]
	fn initialize_bridge_by_governance_works() {
		// for Bulletin finality
		bridge_hub_test_utils::test_cases::initialize_bridge_by_governance_works::<
			Runtime,
			BridgeGrandpaRococoBulletinInstance,
		>(collator_session_keys(), bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID)
	}

	#[test]
	fn change_bridge_grandpa_pallet_mode_by_governance_works() {
		// for Bulletin finality
		bridge_hub_test_utils::test_cases::change_bridge_grandpa_pallet_mode_by_governance_works::<
			Runtime,
			BridgeGrandpaRococoBulletinInstance,
		>(collator_session_keys(), bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID)
	}

	#[test]
	fn change_bridge_messages_pallet_mode_by_governance_works() {
		// for Bulletin finality
		bridge_hub_test_utils::test_cases::change_bridge_messages_pallet_mode_by_governance_works::<
			Runtime,
			WithRococoBulletinMessagesInstance,
		>(collator_session_keys(), bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID)
	}

	#[test]
	fn handle_export_message_from_system_parachain_add_to_outbound_queue_works() {
		// for Bulletin
		bridge_hub_test_utils::test_cases::handle_export_message_from_system_parachain_to_outbound_queue_works::<
			Runtime,
			XcmConfig,
			WithRococoBulletinMessagesInstance,
		>(
			collator_session_keys(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			Box::new(|runtime_event_encoded: Vec<u8>| {
				match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
					Ok(RuntimeEvent::BridgePolkadotBulletinMessages(event)) => Some(event),
					_ => None,
				}
			}),
			|| ExportMessage {
				network: RococoBulletinGlobalConsensusNetwork::get(),
				destination: Here,
				xcm: Xcm(vec![]),
			},
			XCM_LANE_FOR_ROCOCO_PEOPLE_TO_ROCOCO_BULLETIN,
			Some((TokenLocation::get(), ExistentialDeposit::get()).into()),
			None,
			|| PolkadotXcm::force_xcm_version(RuntimeOrigin::root(), Box::new(RococoBulletinGlobalConsensusNetworkLocation::get()), XCM_VERSION).expect("version saved!"),
		)
	}

	#[test]
	fn message_dispatch_routing_works() {
		// from Bulletin
		bridge_hub_test_utils::test_cases::message_dispatch_routing_works::<
			Runtime,
			AllPalletsWithoutSystem,
			XcmConfig,
			ParachainSystem,
			WithRococoBulletinMessagesInstance,
			RelayNetwork,
			RococoBulletinGlobalConsensusNetwork,
			ConstU8<2>,
		>(
			collator_session_keys(),
			slot_durations(),
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
			XCM_LANE_FOR_ROCOCO_PEOPLE_TO_ROCOCO_BULLETIN,
			|| (),
		)
	}

	#[test]
	fn relayed_incoming_message_works() {
		// from Bulletin
		from_grandpa_chain::relayed_incoming_message_works::<RuntimeTestsAdapter>(
			collator_session_keys(),
			slot_durations(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			RococoBulletinChainId::get(),
			SIBLING_PARACHAIN_ID,
			Rococo,
			XCM_LANE_FOR_ROCOCO_PEOPLE_TO_ROCOCO_BULLETIN,
			|| (),
			construct_and_apply_extrinsic,
		)
	}

	#[test]
	pub fn complex_relay_extrinsic_works() {
		// for Bulletin
		from_grandpa_chain::complex_relay_extrinsic_works::<RuntimeTestsAdapter>(
			collator_session_keys(),
			slot_durations(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			RococoBulletinChainId::get(),
			Rococo,
			XCM_LANE_FOR_ROCOCO_PEOPLE_TO_ROCOCO_BULLETIN,
			|| (),
			construct_and_apply_extrinsic,
		);
	}

	#[test]
	pub fn can_calculate_fee_for_complex_message_delivery_transaction() {
		bridge_hub_test_utils::check_sane_fees_values(
			"bp_bridge_hub_rococo::BridgeHubRococoBaseDeliveryFeeInRocs",
			bp_bridge_hub_rococo::BridgeHubRococoBaseDeliveryFeeInRocs::get(),
			|| {
				from_grandpa_chain::can_calculate_fee_for_complex_message_delivery_transaction::<
					RuntimeTestsAdapter,
				>(collator_session_keys(), construct_and_estimate_extrinsic_fee)
			},
			Perbill::from_percent(33),
			None, /* we don't want lowering according to the Bulletin setup, because
			       * `from_grandpa_chain` is cheaper then `from_parachain_chain` */
			&format!(
				"Estimate fee for `single message delivery` for runtime: {:?}",
				<Runtime as frame_system::Config>::Version::get()
			),
		)
	}

	#[test]
	pub fn can_calculate_fee_for_complex_message_confirmation_transaction() {
		bridge_hub_test_utils::check_sane_fees_values(
			"bp_bridge_hub_rococo::BridgeHubRococoBaseConfirmationFeeInRocs",
			bp_bridge_hub_rococo::BridgeHubRococoBaseConfirmationFeeInRocs::get(),
			|| {
				from_grandpa_chain::can_calculate_fee_for_complex_message_confirmation_transaction::<
					RuntimeTestsAdapter,
				>(collator_session_keys(), construct_and_estimate_extrinsic_fee)
			},
			Perbill::from_percent(33),
			None, /* we don't want lowering according to the Bulletin setup, because
			       * `from_grandpa_chain` is cheaper then `from_parachain_chain` */
			&format!(
				"Estimate fee for `single message confirmation` for runtime: {:?}",
				<Runtime as frame_system::Config>::Version::get()
			),
		)
	}
}
