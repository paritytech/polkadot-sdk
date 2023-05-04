// Copyright 2023 Parity Technologies (UK) Ltd.
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

//! Module contains predefined test-case scenarios for `Runtime` with bridging capabilities.

use codec::Encode;
use frame_support::{assert_ok, traits::Get};
use xcm::latest::prelude::*;
use xcm_builder::DispatchBlobError;
use xcm_executor::XcmExecutor;

// Lets re-use this stuff from assets (later we plan to move it outside of assets as `runtimes/test-utils`)
use asset_test_utils::{
	mock_open_hrmp_channel, AccountIdOf, ExtBuilder, RuntimeHelper, ValidatorIdOf,
};

// Re-export test_cases from assets
pub use asset_test_utils::{
	include_teleports_for_native_asset_works, CollatorSessionKeys, XcmReceivedFrom,
};
use bp_messages::{
	target_chain::{DispatchMessage, DispatchMessageData, MessageDispatch},
	LaneId, MessageKey, OutboundLaneData,
};
use bridge_runtime_common::messages_xcm_extension::{
	XcmAsPlainPayload, XcmBlobMessageDispatchResult,
};

/// Test-case makes sure that `Runtime` can process bridging initialize via governance-like call
pub fn initialize_bridge_by_governance_works<Runtime, GrandpaPalletInstance>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	runtime_call_encode: Box<
		dyn Fn(pallet_bridge_grandpa::Call<Runtime, GrandpaPalletInstance>) -> Vec<u8>,
	>,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_dmp_queue::Config
		+ cumulus_pallet_parachain_system::Config
		+ pallet_bridge_grandpa::Config<GrandpaPalletInstance>,
	GrandpaPalletInstance: 'static,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			// check mode before
			assert_eq!(
				pallet_bridge_grandpa::PalletOperatingMode::<Runtime, GrandpaPalletInstance>::try_get(),
				Err(())
			);

			// encode `initialize` call
			let initialize_call = runtime_call_encode(pallet_bridge_grandpa::Call::<
				Runtime,
				GrandpaPalletInstance,
			>::initialize {
				init_data: test_data::initialization_data::<Runtime, GrandpaPalletInstance>(12345),
			});

			// overestimate - check weight for `pallet_bridge_grandpa::Pallet::initialize()` call
			let require_weight_at_most =
				<Runtime as frame_system::Config>::DbWeight::get().reads_writes(7, 7);

			// execute XCM with Transacts to initialize bridge as governance does
			// prepare data for xcm::Transact(create)
			assert_ok!(RuntimeHelper::<Runtime>::execute_as_governance(
				initialize_call,
				require_weight_at_most
			)
			.ensure_complete());

			// check mode after
			assert_eq!(
				pallet_bridge_grandpa::PalletOperatingMode::<Runtime, GrandpaPalletInstance>::try_get(),
				Ok(bp_runtime::BasicOperatingMode::Normal)
			);
		})
}

#[macro_export]
macro_rules! include_initialize_bridge_by_governance_works(
	(
		$runtime:path,
		$pallet_bridge_grandpa_instance:path,
		$collator_session_key:expr,
		$runtime_para_id:expr,
		$runtime_call_encode:expr
	) => {
		#[test]
		fn initialize_bridge_by_governance_works() {
			$crate::test_cases::initialize_bridge_by_governance_works::<
				$runtime,
				$pallet_bridge_grandpa_instance,
			>(
				$collator_session_key,
				$runtime_para_id,
				$runtime_call_encode
			)
		}
	}
);

/// Test-case makes sure that `Runtime` can handle xcm `ExportMessage`:
/// Checks if received XCM messages is correctly added to the message outbound queue for delivery.
/// For SystemParachains we expect unpaid execution.
pub fn handle_export_message_from_system_parachain_to_outbound_queue_works<
	Runtime,
	XcmConfig,
	MessagesPalletInstance,
>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	sibling_parachain_id: u32,
	unwrap_pallet_bridge_messages_event: Box<
		dyn Fn(Vec<u8>) -> Option<pallet_bridge_messages::Event<Runtime, MessagesPalletInstance>>,
	>,
	export_message_instruction: fn() -> Instruction<XcmConfig::RuntimeCall>,
	expected_lane_id: LaneId,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_dmp_queue::Config
		+ cumulus_pallet_parachain_system::Config
		+ pallet_bridge_messages::Config<MessagesPalletInstance>,
	XcmConfig: xcm_executor::Config,
	MessagesPalletInstance: 'static,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
{
	assert_ne!(runtime_para_id, sibling_parachain_id);
	let sibling_parachain_location = MultiLocation::new(1, Parachain(sibling_parachain_id));

	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			// check queue before
			assert_eq!(
				pallet_bridge_messages::OutboundLanes::<Runtime, MessagesPalletInstance>::try_get(
					&expected_lane_id
				),
				Err(())
			);

			// prepare `ExportMessage`
			let xcm = Xcm(vec![
				UnpaidExecution { weight_limit: Unlimited, check_origin: None },
				export_message_instruction(),
			]);

			// execute XCM
			let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
			assert_ok!(XcmExecutor::<XcmConfig>::execute_xcm(
				sibling_parachain_location,
				xcm,
				hash,
				RuntimeHelper::<Runtime>::xcm_max_weight(XcmReceivedFrom::Sibling),
			)
			.ensure_complete());

			// check queue after
			assert_eq!(
				pallet_bridge_messages::OutboundLanes::<Runtime, MessagesPalletInstance>::try_get(
					&expected_lane_id
				),
				Ok(OutboundLaneData {
					oldest_unpruned_nonce: 1,
					latest_received_nonce: 0,
					latest_generated_nonce: 1,
				})
			);

			// check events
			let mut events = <frame_system::Pallet<Runtime>>::events()
				.into_iter()
				.filter_map(|e| unwrap_pallet_bridge_messages_event(e.event.encode()));
			assert!(
				events.any(|e| matches!(e, pallet_bridge_messages::Event::MessageAccepted { .. }))
			);
		})
}

#[macro_export]
macro_rules! include_handle_export_message_from_system_parachain_to_outbound_queue_works(
	(
		$runtime:path,
		$xcm_config:path,
		$pallet_bridge_messages_instance:path,
		$collator_session_key:expr,
		$runtime_para_id:expr,
		$sibling_parachain_id:expr,
		$unwrap_pallet_bridge_messages_event:expr,
		$export_message_instruction:expr,
		$expected_lane_id:expr
	) => {
		#[test]
		fn handle_export_message_from_system_parachain_add_to_outbound_queue_works() {
			$crate::test_cases::handle_export_message_from_system_parachain_to_outbound_queue_works::<
				$runtime,
				$xcm_config,
				$pallet_bridge_messages_instance
			>(
				$collator_session_key,
				$runtime_para_id,
				$sibling_parachain_id,
				$unwrap_pallet_bridge_messages_event,
				$export_message_instruction,
				$expected_lane_id
			)
		}
	}
);

/// Test-case makes sure that Runtime can route XCM messages received in inbound queue,
/// We just test here `MessageDispatch` configuration.
/// We expect that runtime can route messages:
/// 	1. to Parent (relay chain)
/// 	2. to Sibling parachain
pub fn message_dispatch_routing_works<
	Runtime,
	XcmConfig,
	HrmpChannelOpener,
	MessagesPalletInstance,
	RuntimeNetwork,
	BridgedNetwork,
>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	sibling_parachain_id: u32,
	unwrap_cumulus_pallet_parachain_system_event: Box<
		dyn Fn(Vec<u8>) -> Option<cumulus_pallet_parachain_system::Event<Runtime>>,
	>,
	unwrap_cumulus_pallet_xcmp_queue_event: Box<
		dyn Fn(Vec<u8>) -> Option<cumulus_pallet_xcmp_queue::Event<Runtime>>,
	>,
	expected_lane_id: LaneId,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_dmp_queue::Config
		+ cumulus_pallet_parachain_system::Config
		+ cumulus_pallet_xcmp_queue::Config
		+ pallet_bridge_messages::Config<MessagesPalletInstance, InboundPayload = XcmAsPlainPayload>,
	XcmConfig: xcm_executor::Config,
	MessagesPalletInstance: 'static,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	HrmpChannelOpener: frame_support::inherent::ProvideInherent<
		Call = cumulus_pallet_parachain_system::Call<Runtime>,
	>,
	// MessageDispatcher: MessageDispatch<AccountIdOf<Runtime>, DispatchLevelResult = XcmBlobMessageDispatchResult, DispatchPayload = XcmAsPlainPayload>,
	RuntimeNetwork: Get<NetworkId>,
	BridgedNetwork: Get<NetworkId>,
{
	assert_ne!(runtime_para_id, sibling_parachain_id);

	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_safe_xcm_version(XCM_VERSION)
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
				// 1. this message is sent from other global consensus with destination of this Runtime relay chain (UMP)
				let bridging_message =
					test_data::simulate_message_exporter_on_bridged_chain::<BridgedNetwork, RuntimeNetwork>(
						(RuntimeNetwork::get(), Here)
					);
				let result = <<Runtime as pallet_bridge_messages::Config<MessagesPalletInstance>>::MessageDispatch>::dispatch(
					test_data::dispatch_message(expected_lane_id, 1, bridging_message)
				);
				assert_eq!(format!("{:?}", result.dispatch_level_result), format!("{:?}", XcmBlobMessageDispatchResult::Dispatched));

				// check events - UpwardMessageSent
				let mut events = <frame_system::Pallet<Runtime>>::events()
					.into_iter()
					.filter_map(|e| unwrap_cumulus_pallet_parachain_system_event(e.event.encode()));
				assert!(
					events.any(|e| matches!(e, cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }))
				);

				// 2. this message is sent from other global consensus with destination of this Runtime sibling parachain (HRMP)
				let bridging_message =
					test_data::simulate_message_exporter_on_bridged_chain::<BridgedNetwork, RuntimeNetwork>(
						(RuntimeNetwork::get(), X1(Parachain(sibling_parachain_id))),
					);

				// 2.1. WITHOUT opened hrmp channel -> RoutingError
				let result =
					<<Runtime as pallet_bridge_messages::Config<MessagesPalletInstance>>::MessageDispatch>::dispatch(
						DispatchMessage {
							key: MessageKey { lane_id: expected_lane_id, nonce: 1 },
							data: DispatchMessageData { payload: Ok(bridging_message.clone()) },
						}
					);
				assert_eq!(format!("{:?}", result.dispatch_level_result), format!("{:?}", XcmBlobMessageDispatchResult::NotDispatched(Some(DispatchBlobError::RoutingError))));

				// check events - no XcmpMessageSent
				assert_eq!(<frame_system::Pallet<Runtime>>::events()
					.into_iter()
					.filter_map(|e| unwrap_cumulus_pallet_xcmp_queue_event(e.event.encode()))
					.count(), 0);

				// 2.1. WITH hrmp channel -> Ok
				mock_open_hrmp_channel::<Runtime, HrmpChannelOpener>(runtime_para_id.into(), sibling_parachain_id.into());
				let result = <<Runtime as pallet_bridge_messages::Config<MessagesPalletInstance>>::MessageDispatch>::dispatch(
					DispatchMessage {
						key: MessageKey { lane_id: expected_lane_id, nonce: 1 },
						data: DispatchMessageData { payload: Ok(bridging_message) },
					}
				);
				assert_eq!(format!("{:?}", result.dispatch_level_result), format!("{:?}", XcmBlobMessageDispatchResult::Dispatched));

				// check events - XcmpMessageSent
				let mut events = <frame_system::Pallet<Runtime>>::events()
					.into_iter()
					.filter_map(|e| unwrap_cumulus_pallet_xcmp_queue_event(e.event.encode()));
				assert!(
					events.any(|e| matches!(e, cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }))
				);
		})
}

#[macro_export]
macro_rules! include_message_dispatch_routing_works(
	(
		$runtime:path,
		$xcm_config:path,
		$hrmp_channel_opener:path,
		$pallet_bridge_messages_instance:path,
		$runtime_network:path,
		$bridged_network:path,
		$collator_session_key:expr,
		$runtime_para_id:expr,
		$sibling_parachain_id:expr,
		$unwrap_cumulus_pallet_parachain_system_event:expr,
		$unwrap_cumulus_pallet_xcmp_queue_event:expr,
		$expected_lane_id:expr
	) => {
		#[test]
		fn message_dispatch_routing_works() {
			$crate::test_cases::message_dispatch_routing_works::<
				$runtime,
				$xcm_config,
				$hrmp_channel_opener,
				$pallet_bridge_messages_instance,
				$runtime_network,
				$bridged_network
			>(
				$collator_session_key,
				$runtime_para_id,
				$sibling_parachain_id,
				$unwrap_cumulus_pallet_parachain_system_event,
				$unwrap_cumulus_pallet_xcmp_queue_event,
				$expected_lane_id,
			)
		}
	}
);

mod test_data {
	use super::*;
	use bp_messages::MessageNonce;
	use xcm_builder::{HaulBlob, HaulBlobError, HaulBlobExporter};
	use xcm_executor::traits::{validate_export, ExportXcm};

	/// Helper that creates InitializationData mock data, that can be used to initialize bridge GRANDPA pallet
	pub(crate) fn initialization_data<
		Runtime: pallet_bridge_grandpa::Config<GrandpaPalletInstance>,
		GrandpaPalletInstance: 'static,
	>(
		block_number: u32,
	) -> bp_header_chain::InitializationData<
		pallet_bridge_grandpa::BridgedHeader<Runtime, GrandpaPalletInstance>,
	> {
		bp_header_chain::InitializationData {
			header: Box::new(bp_test_utils::test_header(block_number.into())),
			authority_list: Default::default(),
			set_id: 6,
			operating_mode: bp_runtime::BasicOperatingMode::Normal,
		}
	}

	/// Dummy xcm
	pub(crate) fn dummy_xcm() -> Xcm<()> {
		vec![Trap(42)].into()
	}

	pub(crate) fn dispatch_message(
		lane_id: LaneId,
		nonce: MessageNonce,
		payload: Vec<u8>,
	) -> DispatchMessage<Vec<u8>> {
		DispatchMessage {
			key: MessageKey { lane_id, nonce },
			data: DispatchMessageData { payload: Ok(payload) },
		}
	}

	/// Macro used for simulate_export_message and capturing bytes
	macro_rules! grab_haul_blob (
		($name:ident, $grabbed_payload:ident) => {
			std::thread_local! {
				static $grabbed_payload: std::cell::RefCell<Option<Vec<u8>>> = std::cell::RefCell::new(None);
			}

			struct $name;
			impl HaulBlob for $name {
				fn haul_blob(blob: Vec<u8>) -> Result<(), HaulBlobError>{
					$grabbed_payload.with(|rm| *rm.borrow_mut() = Some(blob));
					Ok(())
				}
			}
		}
	);

	/// Simulates `HaulBlobExporter` and all its wrapping and captures generated plain bytes,
	/// which are transfered over bridge.
	pub(crate) fn simulate_message_exporter_on_bridged_chain<
		SourceNetwork: Get<NetworkId>,
		DestinationNetwork: Get<NetworkId>,
	>(
		(destination_network, destination_junctions): (NetworkId, Junctions),
	) -> Vec<u8> {
		grab_haul_blob!(GrabbingHaulBlob, GRABBED_HAUL_BLOB_PAYLOAD);

		// lets pretend that some parachain on bridged chain exported the message
		let universal_source_on_bridged_chain =
			X2(GlobalConsensus(SourceNetwork::get()), Parachain(5678));
		let channel = 1_u32;

		// simulate XCM message export
		let (ticket, fee) =
			validate_export::<HaulBlobExporter<GrabbingHaulBlob, DestinationNetwork, ()>>(
				destination_network,
				channel,
				universal_source_on_bridged_chain,
				destination_junctions,
				dummy_xcm(),
			)
			.expect("validate_export to pass");
		log::info!(
			target: "simulate_message_exporter_on_bridged_chain",
			"HaulBlobExporter::validate fee: {:?}",
			fee
		);
		let xcm_hash =
			HaulBlobExporter::<GrabbingHaulBlob, DestinationNetwork, ()>::deliver(ticket)
				.expect("deliver to pass");
		log::info!(
			target: "simulate_message_exporter_on_bridged_chain",
			"HaulBlobExporter::deliver xcm_hash: {:?}",
			xcm_hash
		);

		GRABBED_HAUL_BLOB_PAYLOAD.with(|r| r.take().expect("Encoded message should be here"))
	}
}
