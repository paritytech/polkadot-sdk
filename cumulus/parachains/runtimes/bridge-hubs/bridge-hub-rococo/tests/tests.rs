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
	AllPalletsWithoutSystem, BridgeRejectObsoleteHeadersAndMessages, Executive, ExistentialDeposit,
	ParachainSystem, PolkadotXcm, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin, SessionKeys,
	TransactionPayment, TxExtension, UncheckedExtrinsic,
};
use bridge_hub_test_utils::SlotDurations;
use codec::{Decode, Encode};
use frame_support::{dispatch::GetDispatchInfo, parameter_types, traits::ConstU8};
use parachains_common::{AccountId, AuraId, Balance};
use snowbridge_core::ChannelId;
use sp_consensus_aura::SlotDuration;
use sp_core::{crypto::Ss58Codec, H160};
use sp_keyring::AccountKeyring::Alice;
use sp_runtime::{
	generic::{Era, SignedPayload},
	AccountId32, Perbill,
};
use testnet_parachains_constants::rococo::{consensus::*, fee::WeightToFee};
use xcm::latest::prelude::*;
use xcm_runtime_apis::conversions::LocationToAccountHelper;

parameter_types! {
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
}

fn construct_extrinsic(
	sender: sp_keyring::AccountKeyring,
	call: RuntimeCall,
) -> UncheckedExtrinsic {
	let account_id = AccountId32::from(sender.public());
	let tx_ext: TxExtension = (
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
		(bridge_to_westend_config::OnBridgeHubRococoRefundBridgeHubWestendMessages::default(),),
		frame_metadata_hash_extension::CheckMetadataHash::new(false),
		cumulus_primitives_storage_weight_reclaim::StorageWeightReclaim::new(),
	)
		.into();
	let payload = SignedPayload::new(call.clone(), tx_ext.clone()).unwrap();
	let signature = payload.using_encoded(|e| sender.sign(e));
	UncheckedExtrinsic::new_signed(call, account_id.into(), Signature::Sr25519(signature), tx_ext)
}

fn construct_and_apply_extrinsic(
	relayer_at_target: sp_keyring::AccountKeyring,
	call: RuntimeCall,
) -> sp_runtime::DispatchOutcome {
	let xt = construct_extrinsic(relayer_at_target, call);
	let r = Executive::apply_extrinsic(xt);
	r.unwrap()
}

fn construct_and_estimate_extrinsic_fee(call: RuntimeCall) -> Balance {
	let info = call.get_dispatch_info();
	let xt = construct_extrinsic(Alice, call);
	TransactionPayment::compute_fee(xt.encoded_size() as _, &info, 0)
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

mod bridge_hub_westend_tests {
	use super::*;
	use bp_messages::LegacyLaneId;
	use bridge_common_config::{
		BridgeGrandpaWestendInstance, BridgeParachainWestendInstance, DeliveryRewardInBalance,
		RelayersForLegacyLaneIdsMessagesInstance,
	};
	use bridge_hub_rococo_runtime::{
		bridge_to_ethereum_config::EthereumGatewayAddress, xcm_config::LocationToAccountId,
	};
	use bridge_hub_test_utils::test_cases::from_parachain;
	use bridge_to_westend_config::{
		BridgeHubWestendLocation, WestendGlobalConsensusNetwork,
		WithBridgeHubWestendMessagesInstance, XcmOverBridgeHubWestendInstance,
	};

	// Random para id of sibling chain used in tests.
	pub const SIBLING_PARACHAIN_ID: u32 = 2053;
	// Random para id of sibling chain used in tests.
	pub const SIBLING_SYSTEM_PARACHAIN_ID: u32 = 1008;
	// Random para id of bridged chain from different global consensus used in tests.
	pub const BRIDGED_LOCATION_PARACHAIN_ID: u32 = 1075;

	parameter_types! {
		pub SiblingParachainLocation: Location = Location::new(1, [Parachain(SIBLING_PARACHAIN_ID)]);
		pub SiblingSystemParachainLocation: Location = Location::new(1, [Parachain(SIBLING_SYSTEM_PARACHAIN_ID)]);
		pub BridgedUniversalLocation: InteriorLocation = [GlobalConsensus(WestendGlobalConsensusNetwork::get()), Parachain(BRIDGED_LOCATION_PARACHAIN_ID)].into();
	}

	// Runtime from tests PoV
	type RuntimeTestsAdapter = from_parachain::WithRemoteParachainHelperAdapter<
		Runtime,
		AllPalletsWithoutSystem,
		BridgeGrandpaWestendInstance,
		BridgeParachainWestendInstance,
		WithBridgeHubWestendMessagesInstance,
		RelayersForLegacyLaneIdsMessagesInstance,
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
			|| ExportMessage { network: WestendGlobalConsensusNetwork::get(), destination: [Parachain(BRIDGED_LOCATION_PARACHAIN_ID)].into(), xcm: Xcm(vec![]) },
			Some((TokenLocation::get(), ExistentialDeposit::get()).into()),
			// value should be >= than value generated by `can_calculate_weight_for_paid_export_message_with_reserve_transfer`
			Some((TokenLocation::get(), bp_bridge_hub_rococo::BridgeHubRococoBaseXcmFeeInRocs::get()).into()),
			|| {
				PolkadotXcm::force_xcm_version(RuntimeOrigin::root(), Box::new(BridgeHubWestendLocation::get()), XCM_VERSION).expect("version saved!");

				// we need to create lane between sibling parachain and remote destination
				bridge_hub_test_utils::ensure_opened_bridge::<
					Runtime,
					XcmOverBridgeHubWestendInstance,
					LocationToAccountId,
					TokenLocation,
				>(
					SiblingParachainLocation::get(),
					BridgedUniversalLocation::get(),
					|locations, fee| {
						bridge_hub_test_utils::open_bridge_with_storage::<
							Runtime,
							XcmOverBridgeHubWestendInstance
						>(locations, fee, LegacyLaneId([0, 0, 0, 1]))
					}
				).1
			},
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
			SIBLING_PARACHAIN_ID,
			Rococo,
			|| {
				// we need to create lane between sibling parachain and remote destination
				bridge_hub_test_utils::ensure_opened_bridge::<
					Runtime,
					XcmOverBridgeHubWestendInstance,
					LocationToAccountId,
					TokenLocation,
				>(
					SiblingParachainLocation::get(),
					BridgedUniversalLocation::get(),
					|locations, fee| {
						bridge_hub_test_utils::open_bridge_with_storage::<
							Runtime,
							XcmOverBridgeHubWestendInstance,
						>(locations, fee, LegacyLaneId([0, 0, 0, 1]))
					},
				)
				.1
			},
			construct_and_apply_extrinsic,
			true,
		)
	}

	#[test]
	fn free_relay_extrinsic_works() {
		// from Westend
		from_parachain::free_relay_extrinsic_works::<RuntimeTestsAdapter>(
			collator_session_keys(),
			slot_durations(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			bp_bridge_hub_westend::BRIDGE_HUB_WESTEND_PARACHAIN_ID,
			SIBLING_PARACHAIN_ID,
			Rococo,
			|| {
				// we need to create lane between sibling parachain and remote destination
				bridge_hub_test_utils::ensure_opened_bridge::<
					Runtime,
					XcmOverBridgeHubWestendInstance,
					LocationToAccountId,
					TokenLocation,
				>(
					SiblingParachainLocation::get(),
					BridgedUniversalLocation::get(),
					|locations, fee| {
						bridge_hub_test_utils::open_bridge_with_storage::<
							Runtime,
							XcmOverBridgeHubWestendInstance,
						>(locations, fee, LegacyLaneId([0, 0, 0, 1]))
					},
				)
				.1
			},
			construct_and_apply_extrinsic,
			false,
		)
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
			Perbill::from_percent(25),
			Some(-25),
			&format!(
				"Estimate fee for `ExportMessage` for runtime: {:?}",
				<Runtime as frame_system::Config>::Version::get()
			),
		)
	}

	#[test]
	fn can_calculate_fee_for_standalone_message_delivery_transaction() {
		bridge_hub_test_utils::check_sane_fees_values(
			"bp_bridge_hub_rococo::BridgeHubRococoBaseDeliveryFeeInRocs",
			bp_bridge_hub_rococo::BridgeHubRococoBaseDeliveryFeeInRocs::get(),
			|| {
				from_parachain::can_calculate_fee_for_standalone_message_delivery_transaction::<
					RuntimeTestsAdapter,
				>(collator_session_keys(), construct_and_estimate_extrinsic_fee)
			},
			Perbill::from_percent(25),
			Some(-25),
			&format!(
				"Estimate fee for `single message delivery` for runtime: {:?}",
				<Runtime as frame_system::Config>::Version::get()
			),
		)
	}

	#[test]
	fn can_calculate_fee_for_standalone_message_confirmation_transaction() {
		bridge_hub_test_utils::check_sane_fees_values(
			"bp_bridge_hub_rococo::BridgeHubRococoBaseConfirmationFeeInRocs",
			bp_bridge_hub_rococo::BridgeHubRococoBaseConfirmationFeeInRocs::get(),
			|| {
				from_parachain::can_calculate_fee_for_standalone_message_confirmation_transaction::<
					RuntimeTestsAdapter,
				>(collator_session_keys(), construct_and_estimate_extrinsic_fee)
			},
			Perbill::from_percent(25),
			Some(-25),
			&format!(
				"Estimate fee for `single message confirmation` for runtime: {:?}",
				<Runtime as frame_system::Config>::Version::get()
			),
		)
	}
}

mod bridge_hub_bulletin_tests {
	use super::*;
	use bp_messages::{HashedLaneId, LaneIdType};
	use bridge_common_config::BridgeGrandpaRococoBulletinInstance;
	use bridge_hub_rococo_runtime::{
		bridge_common_config::RelayersForPermissionlessLanesInstance,
		xcm_config::LocationToAccountId,
	};
	use bridge_hub_test_utils::test_cases::from_grandpa_chain;
	use bridge_to_bulletin_config::{
		RococoBulletinGlobalConsensusNetwork, RococoBulletinGlobalConsensusNetworkLocation,
		WithRococoBulletinMessagesInstance, XcmOverPolkadotBulletinInstance,
	};

	// Random para id of sibling chain used in tests.
	pub const SIBLING_PEOPLE_PARACHAIN_ID: u32 =
		rococo_runtime_constants::system_parachain::PEOPLE_ID;

	parameter_types! {
																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																					pub SiblingPeopleParachainLocation: Location = Location::new(1, [Parachain(SIBLING_PEOPLE_PARACHAIN_ID)]);
																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																																					pub BridgedBulletinLocation: InteriorLocation = [GlobalConsensus(RococoBulletinGlobalConsensusNetwork::get())].into();
	}

	// Runtime from tests PoV
	type RuntimeTestsAdapter = from_grandpa_chain::WithRemoteGrandpaChainHelperAdapter<
		Runtime,
		AllPalletsWithoutSystem,
		BridgeGrandpaRococoBulletinInstance,
		WithRococoBulletinMessagesInstance,
		RelayersForPermissionlessLanesInstance,
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
			SIBLING_PEOPLE_PARACHAIN_ID,
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
			Some((TokenLocation::get(), ExistentialDeposit::get()).into()),
			None,
			|| {
				PolkadotXcm::force_xcm_version(RuntimeOrigin::root(), Box::new(RococoBulletinGlobalConsensusNetworkLocation::get()), XCM_VERSION).expect("version saved!");

				// we need to create lane between RococoPeople and RococoBulletin
				bridge_hub_test_utils::ensure_opened_bridge::<
					Runtime,
					XcmOverPolkadotBulletinInstance,
					LocationToAccountId,
					TokenLocation,
				>(
					SiblingPeopleParachainLocation::get(),
					BridgedBulletinLocation::get(),
					|locations, fee| {
						bridge_hub_test_utils::open_bridge_with_storage::<
							Runtime,
							XcmOverPolkadotBulletinInstance
						>(locations, fee, HashedLaneId::try_new(1, 2).unwrap())
					}
				).1
			},
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
			SIBLING_PEOPLE_PARACHAIN_ID,
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
			SIBLING_PEOPLE_PARACHAIN_ID,
			Rococo,
			|| {
				// we need to create lane between RococoPeople and RococoBulletin
				bridge_hub_test_utils::ensure_opened_bridge::<
					Runtime,
					XcmOverPolkadotBulletinInstance,
					LocationToAccountId,
					TokenLocation,
				>(
					SiblingPeopleParachainLocation::get(),
					BridgedBulletinLocation::get(),
					|locations, fee| {
						bridge_hub_test_utils::open_bridge_with_storage::<
							Runtime,
							XcmOverPolkadotBulletinInstance,
						>(locations, fee, HashedLaneId::try_new(1, 2).unwrap())
					},
				)
				.1
			},
			construct_and_apply_extrinsic,
			false,
		)
	}

	#[test]
	fn free_relay_extrinsic_works() {
		// from Bulletin
		from_grandpa_chain::free_relay_extrinsic_works::<RuntimeTestsAdapter>(
			collator_session_keys(),
			slot_durations(),
			bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID,
			SIBLING_PEOPLE_PARACHAIN_ID,
			Rococo,
			|| {
				// we need to create lane between RococoPeople and RococoBulletin
				bridge_hub_test_utils::ensure_opened_bridge::<
					Runtime,
					XcmOverPolkadotBulletinInstance,
					LocationToAccountId,
					TokenLocation,
				>(
					SiblingPeopleParachainLocation::get(),
					BridgedBulletinLocation::get(),
					|locations, fee| {
						bridge_hub_test_utils::open_bridge_with_storage::<
							Runtime,
							XcmOverPolkadotBulletinInstance,
						>(locations, fee, HashedLaneId::try_new(1, 2).unwrap())
					},
				)
				.1
			},
			construct_and_apply_extrinsic,
			false,
		)
	}
}

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

#[test]
fn location_conversion_works() {
	// the purpose of hardcoded values is to catch an unintended location conversion logic
	// change.
	struct TestCase {
		description: &'static str,
		location: Location,
		expected_account_id_str: &'static str,
	}

	let test_cases = vec![
		// DescribeTerminus
		TestCase {
			description: "DescribeTerminus Parent",
			location: Location::new(1, Here),
			expected_account_id_str: "5Dt6dpkWPwLaH4BBCKJwjiWrFVAGyYk3tLUabvyn4v7KtESG",
		},
		TestCase {
			description: "DescribeTerminus Sibling",
			location: Location::new(1, [Parachain(1111)]),
			expected_account_id_str: "5Eg2fnssmmJnF3z1iZ1NouAuzciDaaDQH7qURAy3w15jULDk",
		},
		// DescribePalletTerminal
		TestCase {
			description: "DescribePalletTerminal Parent",
			location: Location::new(1, [PalletInstance(50)]),
			expected_account_id_str: "5CnwemvaAXkWFVwibiCvf2EjqwiqBi29S5cLLydZLEaEw6jZ",
		},
		TestCase {
			description: "DescribePalletTerminal Sibling",
			location: Location::new(1, [Parachain(1111), PalletInstance(50)]),
			expected_account_id_str: "5GFBgPjpEQPdaxEnFirUoa51u5erVx84twYxJVuBRAT2UP2g",
		},
		// DescribeAccountId32Terminal
		TestCase {
			description: "DescribeAccountId32Terminal Parent",
			location: Location::new(
				1,
				[Junction::AccountId32 { network: None, id: AccountId::from(Alice).into() }],
			),
			expected_account_id_str: "5EueAXd4h8u75nSbFdDJbC29cmi4Uo1YJssqEL9idvindxFL",
		},
		TestCase {
			description: "DescribeAccountId32Terminal Sibling",
			location: Location::new(
				1,
				[
					Parachain(1111),
					Junction::AccountId32 { network: None, id: AccountId::from(Alice).into() },
				],
			),
			expected_account_id_str: "5Dmbuiq48fU4iW58FKYqoGbbfxFHjbAeGLMtjFg6NNCw3ssr",
		},
		// DescribeAccountKey20Terminal
		TestCase {
			description: "DescribeAccountKey20Terminal Parent",
			location: Location::new(1, [AccountKey20 { network: None, key: [0u8; 20] }]),
			expected_account_id_str: "5F5Ec11567pa919wJkX6VHtv2ZXS5W698YCW35EdEbrg14cg",
		},
		TestCase {
			description: "DescribeAccountKey20Terminal Sibling",
			location: Location::new(
				1,
				[Parachain(1111), AccountKey20 { network: None, key: [0u8; 20] }],
			),
			expected_account_id_str: "5CB2FbUds2qvcJNhDiTbRZwiS3trAy6ydFGMSVutmYijpPAg",
		},
		// DescribeTreasuryVoiceTerminal
		TestCase {
			description: "DescribeTreasuryVoiceTerminal Parent",
			location: Location::new(1, [Plurality { id: BodyId::Treasury, part: BodyPart::Voice }]),
			expected_account_id_str: "5CUjnE2vgcUCuhxPwFoQ5r7p1DkhujgvMNDHaF2bLqRp4D5F",
		},
		TestCase {
			description: "DescribeTreasuryVoiceTerminal Sibling",
			location: Location::new(
				1,
				[Parachain(1111), Plurality { id: BodyId::Treasury, part: BodyPart::Voice }],
			),
			expected_account_id_str: "5G6TDwaVgbWmhqRUKjBhRRnH4ry9L9cjRymUEmiRsLbSE4gB",
		},
		// DescribeBodyTerminal
		TestCase {
			description: "DescribeBodyTerminal Parent",
			location: Location::new(1, [Plurality { id: BodyId::Unit, part: BodyPart::Voice }]),
			expected_account_id_str: "5EBRMTBkDisEXsaN283SRbzx9Xf2PXwUxxFCJohSGo4jYe6B",
		},
		TestCase {
			description: "DescribeBodyTerminal Sibling",
			location: Location::new(
				1,
				[Parachain(1111), Plurality { id: BodyId::Unit, part: BodyPart::Voice }],
			),
			expected_account_id_str: "5DBoExvojy8tYnHgLL97phNH975CyT45PWTZEeGoBZfAyRMH",
		},
	];

	for tc in test_cases {
		let expected =
			AccountId::from_string(tc.expected_account_id_str).expect("Invalid AccountId string");

		let got = LocationToAccountHelper::<
			AccountId,
			bridge_hub_rococo_runtime::xcm_config::LocationToAccountId,
		>::convert_location(tc.location.into())
		.unwrap();

		assert_eq!(got, expected, "{}", tc.description);
	}
}
