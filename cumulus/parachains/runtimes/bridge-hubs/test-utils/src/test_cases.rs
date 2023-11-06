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

//! Module contains predefined test-case scenarios for `Runtime` with bridging capabilities.

use bp_messages::{
	source_chain::TargetHeaderChain,
	target_chain::{DispatchMessage, DispatchMessageData, MessageDispatch, SourceHeaderChain},
	LaneId, MessageKey, OutboundLaneData, UnrewardedRelayersState, Weight,
};
use bp_parachains::{BestParaHeadHash, ParaInfo};
use bp_polkadot_core::parachains::{ParaHash, ParaId};
use bp_relayers::{RewardsAccountOwner, RewardsAccountParams};
use bp_runtime::{HeaderOf, Parachain, StorageProofSize, UnderlyingChainOf};
use bp_test_utils::{make_default_justification, prepare_parachain_heads_proof};
use bridge_runtime_common::{
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		BridgedChain as MessageBridgedChain, MessageBridge,
	},
	messages_generation::{
		encode_all_messages, encode_lane_data, prepare_message_delivery_storage_proof,
		prepare_messages_storage_proof,
	},
	messages_xcm_extension::{XcmAsPlainPayload, XcmBlobMessageDispatchResult},
};
use codec::Encode;
use frame_support::{
	assert_ok,
	traits::{Get, OnFinalize, OnInitialize, OriginTrait, PalletInfoAccess},
};
use frame_system::pallet_prelude::{BlockNumberFor, HeaderFor};
use pallet_bridge_grandpa::BridgedHeader;
use parachains_common::AccountId;
use parachains_runtimes_test_utils::{
	mock_open_hrmp_channel, AccountIdOf, BalanceOf, CollatorSessionKeys, ExtBuilder, ValidatorIdOf,
	XcmReceivedFrom,
};
use sp_core::H256;
use sp_keyring::AccountKeyring::*;
use sp_runtime::{
	traits::{Header as HeaderT, Zero},
	AccountId32,
};
use xcm::latest::prelude::*;
use xcm_builder::DispatchBlobError;
use xcm_executor::{
	traits::{TransactAsset, WeightBounds},
	XcmExecutor,
};

// Re-export test_case from assets
pub use asset_test_utils::include_teleports_for_native_asset_works;

type RuntimeHelper<Runtime, AllPalletsWithoutSystem = ()> =
	parachains_runtimes_test_utils::RuntimeHelper<Runtime, AllPalletsWithoutSystem>;

// Re-export test_case from `parachains-runtimes-test-utils`
pub use parachains_runtimes_test_utils::test_cases::change_storage_constant_by_governance_works;

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

			// execute XCM with Transacts to `initialize bridge` as governance does
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
	existential_deposit: Option<MultiAsset>,
	maybe_paid_export_message: Option<MultiAsset>,
	prepare_configuration: impl Fn(),
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
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
			prepare_configuration();

			// check queue before
			assert_eq!(
				pallet_bridge_messages::OutboundLanes::<Runtime, MessagesPalletInstance>::try_get(
					expected_lane_id
				),
				Err(())
			);

			// prepare `ExportMessage`
			let xcm = if let Some(fee) = maybe_paid_export_message {
				// deposit ED to origin (if needed)
				if let Some(ed) = existential_deposit {
					XcmConfig::AssetTransactor::deposit_asset(
						&ed,
						&sibling_parachain_location,
						Some(&XcmContext::with_message_id([0; 32])),
					)
					.expect("deposited ed");
				}
				// deposit fee to origin
				XcmConfig::AssetTransactor::deposit_asset(
					&fee,
					&sibling_parachain_location,
					Some(&XcmContext::with_message_id([0; 32])),
				)
				.expect("deposited fee");

				Xcm(vec![
					WithdrawAsset(MultiAssets::from(vec![fee.clone()])),
					BuyExecution { fees: fee, weight_limit: Unlimited },
					export_message_instruction(),
				])
			} else {
				Xcm(vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					export_message_instruction(),
				])
			};

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
					expected_lane_id
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

/// Test-case makes sure that Runtime can route XCM messages received in inbound queue,
/// We just test here `MessageDispatch` configuration.
/// We expect that runtime can route messages:
///     1. to Parent (relay chain)
///     2. to Sibling parachain
pub fn message_dispatch_routing_works<
	Runtime,
	AllPalletsWithoutSystem,
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
	prepare_configuration: impl Fn(),
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ cumulus_pallet_xcmp_queue::Config
		+ pallet_bridge_messages::Config<MessagesPalletInstance, InboundPayload = XcmAsPlainPayload>,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	<Runtime as frame_system::Config>::AccountId:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	XcmConfig: xcm_executor::Config,
	MessagesPalletInstance: 'static,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<Runtime as frame_system::Config>::AccountId: From<AccountId32>,
	HrmpChannelOpener: frame_support::inherent::ProvideInherent<
		Call = cumulus_pallet_parachain_system::Call<Runtime>,
	>,
	// MessageDispatcher: MessageDispatch<AccountIdOf<Runtime>, DispatchLevelResult =
	// XcmBlobMessageDispatchResult, DispatchPayload = XcmAsPlainPayload>,
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
			prepare_configuration();

			let mut alice = [0u8; 32];
			alice[0] = 1;

			let included_head = RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::run_to_block(
				2,
				AccountId::from(alice).into(),
			);
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
			mock_open_hrmp_channel::<Runtime, HrmpChannelOpener>(runtime_para_id.into(), sibling_parachain_id.into(), included_head, &alice);
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

/// Test-case makes sure that Runtime can dispatch XCM messages submitted by relayer,
/// with proofs (finality, para heads, message) independently submitted.
pub fn relayed_incoming_message_works<Runtime, AllPalletsWithoutSystem, XcmConfig, HrmpChannelOpener, GPI, PPI, MPI, MB>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	bridged_para_id: u32,
	sibling_parachain_id: u32,
	local_relay_chain_id: NetworkId,
	lane_id: LaneId,
	prepare_configuration: impl Fn(),
) where
	Runtime: frame_system::Config
	+ pallet_balances::Config
	+ pallet_session::Config
	+ pallet_xcm::Config
	+ parachain_info::Config
	+ pallet_collator_selection::Config
	+ cumulus_pallet_parachain_system::Config
	+ cumulus_pallet_xcmp_queue::Config
	+ pallet_bridge_grandpa::Config<GPI>
	+ pallet_bridge_parachains::Config<PPI>
	+ pallet_bridge_messages::Config<MPI, InboundPayload = XcmAsPlainPayload>,
	AllPalletsWithoutSystem: OnInitialize<BlockNumberFor<Runtime>>
		+ OnFinalize<BlockNumberFor<Runtime>>,
	GPI: 'static,
	PPI: 'static,
	MPI: 'static,
	MB: MessageBridge,
	<MB as MessageBridge>::BridgedChain: Send + Sync + 'static,
	<MB as MessageBridge>::ThisChain: Send + Sync + 'static,
	UnderlyingChainOf<MessageBridgedChain<MB>>: bp_runtime::Chain<Hash = ParaHash> + Parachain,
	XcmConfig: xcm_executor::Config,
	HrmpChannelOpener: frame_support::inherent::ProvideInherent<
		Call = cumulus_pallet_parachain_system::Call<Runtime>,
	>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<<Runtime as pallet_bridge_messages::Config<MPI>>::SourceHeaderChain as SourceHeaderChain>::MessagesProof: From<FromBridgedChainMessagesProof<ParaHash>>,
	<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash: From<ParaHash>,
	ParaHash: From<<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash>,
	<Runtime as frame_system::Config>::AccountId:
	Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<Runtime as frame_system::Config>::AccountId: From<AccountId32>,
	AccountIdOf<Runtime>: From<sp_core::sr25519::Public>,
	<Runtime as pallet_bridge_messages::Config<MPI>>::InboundRelayer: From<AccountId32>,
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
			prepare_configuration();

			let mut alice = [0u8; 32];
			alice[0] = 1;

			let included_head = RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::run_to_block(
				2,
				AccountId::from(alice).into(),
			);
			mock_open_hrmp_channel::<Runtime, HrmpChannelOpener>(
				runtime_para_id.into(),
				sibling_parachain_id.into(),
				included_head,
				&alice,
			);

			// start with bridged chain block#0
			let init_data = test_data::initialization_data::<Runtime, GPI>(0);
			pallet_bridge_grandpa::Pallet::<Runtime, GPI>::initialize(
				RuntimeHelper::<Runtime>::root_origin(),
				init_data,
			)
			.unwrap();

			// set up relayer details and proofs

			let message_destination =
				X2(GlobalConsensus(local_relay_chain_id), Parachain(sibling_parachain_id));
			// some random numbers (checked by test)
			let message_nonce = 1;
			let para_header_number = 5;
			let relay_header_number = 1;

			let relayer_at_target = Bob;
			let relayer_id_on_target: AccountIdOf<Runtime> = relayer_at_target.public().into();
			let relayer_at_source = Dave;
			let relayer_id_on_source: AccountId32 = relayer_at_source.public().into();

			let xcm = vec![xcm::v3::Instruction::<()>::ClearOrigin; 42];
			let expected_dispatch = xcm::latest::Xcm::<()>({
				let mut expected_instructions = xcm.clone();
				// dispatch prepends bridge pallet instance
				expected_instructions.insert(
					0,
					DescendOrigin(X1(PalletInstance(
						<pallet_bridge_messages::Pallet<Runtime, MPI> as PalletInfoAccess>::index()
							as u8,
					))),
				);
				expected_instructions
			});
			// generate bridged relay chain finality, parachain heads and message proofs,
			// to be submitted by relayer to this chain.
			let (
				relay_chain_header,
				grandpa_justification,
				bridged_para_head,
				parachain_heads,
				para_heads_proof,
				message_proof,
			) = test_data::make_complex_relayer_delivery_proofs::<
				<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain,
				MB,
				(),
			>(
				lane_id,
				xcm.into(),
				message_nonce,
				message_destination,
				para_header_number,
				relay_header_number,
				bridged_para_id,
			);

			// submit bridged relay chain finality proof
			{
				let result = pallet_bridge_grandpa::Pallet::<Runtime, GPI>::submit_finality_proof(
					RuntimeHelper::<Runtime>::origin_of(relayer_id_on_target.clone()),
					Box::new(relay_chain_header.clone()),
					grandpa_justification,
				);
				assert_ok!(result);
				assert_eq!(result.unwrap().pays_fee, frame_support::dispatch::Pays::Yes);
			}

			// verify finality proof correctly imported
			assert_eq!(
				pallet_bridge_grandpa::BestFinalized::<Runtime, GPI>::get().unwrap().1,
				relay_chain_header.hash()
			);
			assert!(pallet_bridge_grandpa::ImportedHeaders::<Runtime, GPI>::contains_key(
				relay_chain_header.hash()
			));

			// submit parachain heads proof
			{
				let result =
					pallet_bridge_parachains::Pallet::<Runtime, PPI>::submit_parachain_heads(
						RuntimeHelper::<Runtime>::origin_of(relayer_id_on_target.clone()),
						(relay_header_number, relay_chain_header.hash().into()),
						parachain_heads,
						para_heads_proof,
					);
				assert_ok!(result);
				assert_eq!(result.unwrap().pays_fee, frame_support::dispatch::Pays::Yes);
			}
			// verify parachain head proof correctly imported
			assert_eq!(
				pallet_bridge_parachains::ParasInfo::<Runtime, PPI>::get(ParaId(bridged_para_id)),
				Some(ParaInfo {
					best_head_hash: BestParaHeadHash {
						at_relay_block_number: relay_header_number,
						head_hash: bridged_para_head.hash()
					},
					next_imported_hash_position: 1,
				})
			);

			// import message
			assert!(RuntimeHelper::<cumulus_pallet_xcmp_queue::Pallet<Runtime>>::take_xcm(
				sibling_parachain_id.into()
			)
			.is_none());
			assert_eq!(
				pallet_bridge_messages::InboundLanes::<Runtime, MPI>::get(lane_id)
					.last_delivered_nonce(),
				0,
			);
			// submit message proof
			{
				let result = pallet_bridge_messages::Pallet::<Runtime, MPI>::receive_messages_proof(
					RuntimeHelper::<Runtime>::origin_of(relayer_id_on_target),
					relayer_id_on_source.into(),
					message_proof.into(),
					1,
					Weight::MAX / 1000,
				);
				assert_ok!(result);
				assert_eq!(result.unwrap().pays_fee, frame_support::dispatch::Pays::Yes);
			}
			// verify message correctly imported and dispatched
			assert_eq!(
				pallet_bridge_messages::InboundLanes::<Runtime, MPI>::get(lane_id)
					.last_delivered_nonce(),
				1,
			);

			// verify relayed bridged XCM message is dispatched to destination sibling para
			let dispatched = RuntimeHelper::<cumulus_pallet_xcmp_queue::Pallet<Runtime>>::take_xcm(
				sibling_parachain_id.into(),
			)
			.unwrap();
			// verify contains original message
			let dispatched = xcm::latest::Xcm::<()>::try_from(dispatched).unwrap();
			let mut dispatched_clone = dispatched.clone();
			for (idx, expected_instr) in expected_dispatch.0.iter().enumerate() {
				assert_eq!(expected_instr, &dispatched.0[idx]);
				assert_eq!(expected_instr, &dispatched_clone.0.remove(0));
			}
			match dispatched_clone.0.len() {
				0 => (),
				1 => assert!(matches!(dispatched_clone.0[0], SetTopic(_))),
				count => assert!(false, "Unexpected messages count: {:?}", count),
			}
		})
}

/// Test-case makes sure that Runtime can dispatch XCM messages submitted by relayer,
/// with proofs (finality, para heads, message) batched together in signed extrinsic.
/// Also verifies relayer transaction signed extensions work as intended.
pub fn complex_relay_extrinsic_works<Runtime, AllPalletsWithoutSystem, XcmConfig, HrmpChannelOpener, GPI, PPI, MPI, MB>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	bridged_para_id: u32,
	sibling_parachain_id: u32,
	bridged_chain_id: bp_runtime::ChainId,
	local_relay_chain_id: NetworkId,
	lane_id: LaneId,
	existential_deposit: BalanceOf<Runtime>,
	executive_init_block: fn(&HeaderFor<Runtime>),
	construct_and_apply_extrinsic: fn(
		sp_keyring::AccountKeyring,
		pallet_utility::Call::<Runtime>
	) -> sp_runtime::DispatchOutcome,
	prepare_configuration: impl Fn(),
) where
	Runtime: frame_system::Config
	+ pallet_balances::Config
	+ pallet_utility::Config
	+ pallet_session::Config
	+ pallet_xcm::Config
	+ parachain_info::Config
	+ pallet_collator_selection::Config
	+ cumulus_pallet_parachain_system::Config
	+ cumulus_pallet_xcmp_queue::Config
	+ pallet_bridge_grandpa::Config<GPI>
	+ pallet_bridge_parachains::Config<PPI>
	+ pallet_bridge_messages::Config<MPI, InboundPayload = XcmAsPlainPayload>
	+ pallet_bridge_relayers::Config,
	AllPalletsWithoutSystem: OnInitialize<BlockNumberFor<Runtime>>
		+ OnFinalize<BlockNumberFor<Runtime>>,
	GPI: 'static,
	PPI: 'static,
	MPI: 'static,
	MB: MessageBridge,
	<MB as MessageBridge>::BridgedChain: Send + Sync + 'static,
	<MB as MessageBridge>::ThisChain: Send + Sync + 'static,
	UnderlyingChainOf<MessageBridgedChain<MB>>: bp_runtime::Chain<Hash = ParaHash> + Parachain,
	XcmConfig: xcm_executor::Config,
	HrmpChannelOpener: frame_support::inherent::ProvideInherent<
		Call = cumulus_pallet_parachain_system::Call<Runtime>,
	>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<<Runtime as pallet_bridge_messages::Config<MPI>>::SourceHeaderChain as SourceHeaderChain>::MessagesProof: From<FromBridgedChainMessagesProof<ParaHash>>,
	<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash: From<ParaHash>,
	ParaHash: From<<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash>,
	<Runtime as frame_system::Config>::AccountId:
	Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	AccountIdOf<Runtime>: From<sp_core::sr25519::Public>,
	<Runtime as frame_system::Config>::AccountId: From<AccountId32>,
	<Runtime as pallet_bridge_messages::Config<MPI>>::InboundRelayer: From<AccountId32>,
	<Runtime as pallet_utility::Config>::RuntimeCall:
	From<pallet_bridge_grandpa::Call<Runtime, GPI>>
	+ From<pallet_bridge_parachains::Call<Runtime, PPI>>
	+ From<pallet_bridge_messages::Call<Runtime, MPI>>
{
	assert_ne!(runtime_para_id, sibling_parachain_id);

	// Relayer account at local/this BH.
	let relayer_at_target = Bob;
	let relayer_id_on_target: AccountIdOf<Runtime> = relayer_at_target.public().into();
	let relayer_initial_balance = existential_deposit * 100000u32.into();
	// Relayer account at remote/bridged BH.
	let relayer_at_source = Dave;
	let relayer_id_on_source: AccountId32 = relayer_at_source.public().into();

	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_safe_xcm_version(XCM_VERSION)
		.with_para_id(runtime_para_id.into())
		.with_balances(vec![(relayer_id_on_target.clone(), relayer_initial_balance)])
		.with_tracing()
		.build()
		.execute_with(|| {
			prepare_configuration();

			let mut alice = [0u8; 32];
			alice[0] = 1;

			let included_head = RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::run_to_block(
				2,
				AccountId::from(alice).into(),
			);
			let zero: BlockNumberFor<Runtime> = 0u32.into();
			let genesis_hash = frame_system::Pallet::<Runtime>::block_hash(zero);
			let mut header: HeaderFor<Runtime> = bp_test_utils::test_header(1u32.into());
			header.set_parent_hash(genesis_hash);
			executive_init_block(&header);

			mock_open_hrmp_channel::<Runtime, HrmpChannelOpener>(
				runtime_para_id.into(),
				sibling_parachain_id.into(),
				included_head,
				&alice,
			);

			// start with bridged chain block#0
			let init_data = test_data::initialization_data::<Runtime, GPI>(0);
			pallet_bridge_grandpa::Pallet::<Runtime, GPI>::initialize(
				RuntimeHelper::<Runtime>::root_origin(),
				init_data,
			)
			.unwrap();

			// set up relayer details and proofs

			let message_destination =
				X2(GlobalConsensus(local_relay_chain_id), Parachain(sibling_parachain_id));
			// some random numbers (checked by test)
			let message_nonce = 1;
			let para_header_number = 5;
			let relay_header_number = 1;

			let xcm = vec![xcm::latest::Instruction::<()>::ClearOrigin; 42];
			let expected_dispatch = xcm::latest::Xcm::<()>({
				let mut expected_instructions = xcm.clone();
				// dispatch prepends bridge pallet instance
				expected_instructions.insert(
					0,
					DescendOrigin(X1(PalletInstance(
						<pallet_bridge_messages::Pallet<Runtime, MPI> as PalletInfoAccess>::index()
							as u8,
					))),
				);
				expected_instructions
			});
			// generate bridged relay chain finality, parachain heads and message proofs,
			// to be submitted by relayer to this chain.
			let (
				relay_chain_header,
				grandpa_justification,
				bridged_para_head,
				parachain_heads,
				para_heads_proof,
				message_proof,
			) = test_data::make_complex_relayer_delivery_proofs::<
				<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain,
				MB,
				(),
			>(
				lane_id,
				xcm.clone().into(),
				message_nonce,
				message_destination,
				para_header_number,
				relay_header_number,
				bridged_para_id,
			);

			let relay_chain_header_hash = relay_chain_header.hash();
			let batch = test_data::make_complex_relayer_delivery_batch::<Runtime, GPI, PPI, MPI>(
				relay_chain_header,
				grandpa_justification,
				parachain_heads,
				para_heads_proof,
				message_proof,
				relayer_id_on_source,
			);

			// sanity checks - before relayer extrinsic
			assert!(RuntimeHelper::<cumulus_pallet_xcmp_queue::Pallet<Runtime>>::take_xcm(
				sibling_parachain_id.into()
			)
			.is_none());
			assert_eq!(
				pallet_bridge_messages::InboundLanes::<Runtime, MPI>::get(lane_id)
					.last_delivered_nonce(),
				0,
			);
			let msg_proofs_rewards_account = RewardsAccountParams::new(
				lane_id,
				bridged_chain_id,
				RewardsAccountOwner::ThisChain,
			);
			assert_eq!(
				pallet_bridge_relayers::RelayerRewards::<Runtime>::get(
					relayer_id_on_target.clone(),
					msg_proofs_rewards_account
				),
				None,
			);

			// construct and apply extrinsic containing batch calls:
			//   bridged relay chain finality proof
			//   + parachain heads proof
			//   + submit message proof
			let dispatch_outcome = construct_and_apply_extrinsic(relayer_at_target, batch);

			// verify finality proof correctly imported
			assert_ok!(dispatch_outcome);
			assert_eq!(
				<pallet_bridge_grandpa::BestFinalized<Runtime, GPI>>::get().unwrap().1,
				relay_chain_header_hash
			);
			assert!(<pallet_bridge_grandpa::ImportedHeaders<Runtime, GPI>>::contains_key(
				relay_chain_header_hash
			));
			// verify parachain head proof correctly imported
			assert_eq!(
				pallet_bridge_parachains::ParasInfo::<Runtime, PPI>::get(ParaId(bridged_para_id)),
				Some(ParaInfo {
					best_head_hash: BestParaHeadHash {
						at_relay_block_number: relay_header_number,
						head_hash: bridged_para_head.hash()
					},
					next_imported_hash_position: 1,
				})
			);
			// verify message correctly imported and dispatched
			assert_eq!(
				pallet_bridge_messages::InboundLanes::<Runtime, MPI>::get(lane_id)
					.last_delivered_nonce(),
				1,
			);
			// verify relayer is refunded
			assert!(pallet_bridge_relayers::RelayerRewards::<Runtime>::get(
				relayer_id_on_target,
				msg_proofs_rewards_account
			)
			.is_some());

			// verify relayed bridged XCM message is dispatched to destination sibling para
			let dispatched = RuntimeHelper::<cumulus_pallet_xcmp_queue::Pallet<Runtime>>::take_xcm(
				sibling_parachain_id.into(),
			)
			.unwrap();
			// verify contains original message
			let dispatched = xcm::latest::Xcm::<()>::try_from(dispatched).unwrap();
			let mut dispatched_clone = dispatched.clone();
			for (idx, expected_instr) in expected_dispatch.0.iter().enumerate() {
				assert_eq!(expected_instr, &dispatched.0[idx]);
				assert_eq!(expected_instr, &dispatched_clone.0.remove(0));
			}
			match dispatched_clone.0.len() {
				0 => (),
				1 => assert!(matches!(dispatched_clone.0[0], SetTopic(_))),
				count => assert!(false, "Unexpected messages count: {:?}", count),
			}
		})
}

/// Estimates XCM execution fee for paid `ExportMessage` processing.
pub fn can_calculate_weight_for_paid_export_message_with_reserve_transfer<
	Runtime,
	XcmConfig,
	WeightToFee,
>() -> u128
where
	Runtime: frame_system::Config + pallet_balances::Config,
	XcmConfig: xcm_executor::Config,
	WeightToFee: frame_support::weights::WeightToFee<Balance = BalanceOf<Runtime>>,
	<WeightToFee as frame_support::weights::WeightToFee>::Balance: From<u128> + Into<u128>,
{
	// data here are not relevant for weighing
	let mut xcm = Xcm(vec![
		WithdrawAsset(MultiAssets::from(vec![MultiAsset {
			id: Concrete(MultiLocation { parents: 1, interior: Here }),
			fun: Fungible(34333299),
		}])),
		BuyExecution {
			fees: MultiAsset {
				id: Concrete(MultiLocation { parents: 1, interior: Here }),
				fun: Fungible(34333299),
			},
			weight_limit: Unlimited,
		},
		ExportMessage {
			network: Polkadot,
			destination: X1(Parachain(1000)),
			xcm: Xcm(vec![
				ReserveAssetDeposited(MultiAssets::from(vec![MultiAsset {
					id: Concrete(MultiLocation {
						parents: 2,
						interior: X1(GlobalConsensus(Kusama)),
					}),
					fun: Fungible(1000000000000),
				}])),
				ClearOrigin,
				BuyExecution {
					fees: MultiAsset {
						id: Concrete(MultiLocation {
							parents: 2,
							interior: X1(GlobalConsensus(Kusama)),
						}),
						fun: Fungible(1000000000000),
					},
					weight_limit: Unlimited,
				},
				DepositAsset {
					assets: Wild(AllCounted(1)),
					beneficiary: MultiLocation {
						parents: 0,
						interior: X1(xcm::latest::prelude::AccountId32 {
							network: None,
							id: [
								212, 53, 147, 199, 21, 253, 211, 28, 97, 20, 26, 189, 4, 169, 159,
								214, 130, 44, 133, 88, 133, 76, 205, 227, 154, 86, 132, 231, 165,
								109, 162, 125,
							],
						}),
					},
				},
				SetTopic([
					116, 82, 194, 132, 171, 114, 217, 165, 23, 37, 161, 177, 165, 179, 247, 114,
					137, 101, 147, 70, 28, 157, 168, 32, 154, 63, 74, 228, 152, 180, 5, 63,
				]),
			]),
		},
		RefundSurplus,
		DepositAsset {
			assets: Wild(All),
			beneficiary: MultiLocation { parents: 1, interior: X1(Parachain(1000)) },
		},
		SetTopic([
			36, 224, 250, 165, 82, 195, 67, 110, 160, 170, 140, 87, 217, 62, 201, 164, 42, 98, 219,
			157, 124, 105, 248, 25, 131, 218, 199, 36, 109, 173, 100, 122,
		]),
	]);

	// get weight
	let weight = XcmConfig::Weigher::weight(&mut xcm);
	assert_ok!(weight);
	let weight = weight.unwrap();
	// check if sane
	let max_expected = Runtime::BlockWeights::get().max_block / 10;
	assert!(
		weight.all_lte(max_expected),
		"calculated weight: {:?}, max_expected: {:?}",
		weight,
		max_expected
	);

	// check fee, should not be 0
	let estimated_fee = WeightToFee::weight_to_fee(&weight);
	assert!(estimated_fee > BalanceOf::<Runtime>::zero());

	sp_tracing::try_init_simple();
	log::error!(
		target: "bridges::estimate",
		"Estimate fee: {:?} for `ExportMessage` for runtime: {:?}",
		estimated_fee,
		Runtime::Version::get(),
	);

	estimated_fee.into()
}

/// Estimates transaction fee for default message delivery transaction (batched with required
/// proofs) from bridged parachain.
pub fn can_calculate_fee_for_complex_message_delivery_transaction<Runtime, GPI, PPI, MPI, MB>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	compute_extrinsic_fee: fn(pallet_utility::Call::<Runtime>) -> u128,
) -> u128
where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ pallet_bridge_grandpa::Config<GPI>
		+ pallet_bridge_parachains::Config<PPI>
		+ pallet_bridge_messages::Config<MPI, InboundPayload = XcmAsPlainPayload>
		+ pallet_utility::Config,
	GPI: 'static,
	PPI: 'static,
	MPI: 'static,
	MB: MessageBridge,
	<MB as MessageBridge>::BridgedChain: Send + Sync + 'static,
	<MB as MessageBridge>::ThisChain: Send + Sync + 'static,
	UnderlyingChainOf<MessageBridgedChain<MB>>: bp_runtime::Chain<Hash = ParaHash> + Parachain,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<<Runtime as pallet_bridge_messages::Config<MPI>>::SourceHeaderChain as SourceHeaderChain>::MessagesProof:
		From<FromBridgedChainMessagesProof<ParaHash>>,
	<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash: From<ParaHash>,
	ParaHash: From<<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash>,
	<Runtime as frame_system::Config>::AccountId:
	Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<Runtime as frame_system::Config>::AccountId: From<AccountId32>,
	AccountIdOf<Runtime>: From<sp_core::sr25519::Public>,
	<Runtime as pallet_bridge_messages::Config<MPI>>::InboundRelayer: From<AccountId32>,
	<Runtime as pallet_utility::Config>::RuntimeCall:
	From<pallet_bridge_grandpa::Call<Runtime, GPI>>
	+ From<pallet_bridge_parachains::Call<Runtime, PPI>>
	+ From<pallet_bridge_messages::Call<Runtime, MPI>>,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_safe_xcm_version(XCM_VERSION)
		.with_para_id(1000.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			// generate bridged relay chain finality, parachain heads and message proofs,
			// to be submitted by relayer to this chain.
			//
			// we don't care about parameter values here, apart from the XCM message size. But we
			// do not need to have a large message here, because we're charging for every byte of
			// the message additionally
			let (
				relay_chain_header,
				grandpa_justification,
				_,
				parachain_heads,
				para_heads_proof,
				message_proof,
			) = test_data::make_complex_relayer_delivery_proofs::<
				<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain,
				MB,
				(),
			>(
				LaneId::default(),
				vec![xcm::v3::Instruction::<()>::ClearOrigin; 1_024].into(),
				1,
				X2(GlobalConsensus(Polkadot), Parachain(1_000)),
				1,
				5,
				1_000,
			);

			// generate batch call that provides finality for bridged relay and parachains + message
			// proof
			let batch = test_data::make_complex_relayer_delivery_batch::<Runtime, GPI, PPI, MPI>(
				relay_chain_header,
				grandpa_justification,
				parachain_heads,
				para_heads_proof,
				message_proof,
				Dave.public().into(),
			);
			let estimated_fee = compute_extrinsic_fee(batch);

			log::error!(
				target: "bridges::estimate",
				"Estimate fee: {:?} for single message delivery for runtime: {:?}",
				estimated_fee,
				Runtime::Version::get(),
			);

			estimated_fee
		})
}

/// Estimates transaction fee for default message confirmation transaction (batched with required
/// proofs) from bridged parachain.
pub fn can_calculate_fee_for_complex_message_confirmation_transaction<Runtime, GPI, PPI, MPI, MB>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	compute_extrinsic_fee: fn(pallet_utility::Call::<Runtime>) -> u128,
) -> u128
where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ pallet_bridge_grandpa::Config<GPI>
		+ pallet_bridge_parachains::Config<PPI>
		+ pallet_bridge_messages::Config<MPI, OutboundPayload = XcmAsPlainPayload>
		+ pallet_utility::Config,
	GPI: 'static,
	PPI: 'static,
	MPI: 'static,
	MB: MessageBridge,
	<MB as MessageBridge>::BridgedChain: Send + Sync + 'static,
	<MB as MessageBridge>::ThisChain: Send + Sync + 'static,
	<<MB as MessageBridge>::ThisChain as bp_runtime::Chain>::AccountId: From<AccountId32>,
	UnderlyingChainOf<MessageBridgedChain<MB>>: bp_runtime::Chain<Hash = ParaHash> + Parachain,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<<Runtime as pallet_bridge_messages::Config<MPI>>::SourceHeaderChain as SourceHeaderChain>::MessagesProof:
		From<FromBridgedChainMessagesProof<ParaHash>>,
	<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash: From<ParaHash>,
	ParaHash: From<<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash>,
	<Runtime as frame_system::Config>::AccountId:
	Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<Runtime as frame_system::Config>::AccountId: From<AccountId32>,
	AccountIdOf<Runtime>: From<sp_core::sr25519::Public>,
	<Runtime as pallet_bridge_messages::Config<MPI>>::InboundRelayer: From<AccountId32>,
		<<Runtime as pallet_bridge_messages::Config<MPI>>::TargetHeaderChain as TargetHeaderChain<
			XcmAsPlainPayload,
			Runtime::AccountId,
		>>::MessagesDeliveryProof: From<FromBridgedChainMessagesDeliveryProof<ParaHash>>,
	<Runtime as pallet_utility::Config>::RuntimeCall:
	From<pallet_bridge_grandpa::Call<Runtime, GPI>>
	+ From<pallet_bridge_parachains::Call<Runtime, PPI>>
	+ From<pallet_bridge_messages::Call<Runtime, MPI>>,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_safe_xcm_version(XCM_VERSION)
		.with_para_id(1000.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			// generate bridged relay chain finality, parachain heads and message proofs,
			// to be submitted by relayer to this chain.
			let unrewarded_relayers = UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 1,
				..Default::default()
			};
			let (
				relay_chain_header,
				grandpa_justification,
				_,
				parachain_heads,
				para_heads_proof,
				message_delivery_proof,
			) = test_data::make_complex_relayer_confirmation_proofs::<
				<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain,
				MB,
				(),
			>(LaneId::default(), 1, 5, 1_000, Alice.public().into(), unrewarded_relayers.clone());

			// generate batch call that provides finality for bridged relay and parachains + message
			// proof
			let batch = test_data::make_complex_relayer_confirmation_batch::<Runtime, GPI, PPI, MPI>(
				relay_chain_header,
				grandpa_justification,
				parachain_heads,
				para_heads_proof,
				message_delivery_proof,
				unrewarded_relayers,
			);
			let estimated_fee = compute_extrinsic_fee(batch);

			log::error!(
				target: "bridges::estimate",
				"Estimate fee: {:?} for single message confirmation for runtime: {:?}",
				estimated_fee,
				Runtime::Version::get(),
			);

			estimated_fee
		})
}

pub mod test_data {
	use super::*;
	use bp_header_chain::{justification::GrandpaJustification, ChainWithGrandpa};
	use bp_messages::{DeliveredMessages, InboundLaneData, MessageNonce, UnrewardedRelayer};
	use bp_polkadot_core::parachains::{ParaHash, ParaHead, ParaHeadsProof, ParaId};
	use bp_runtime::{BasicOperatingMode, HashOf};
	use bp_test_utils::authority_list;
	use sp_runtime::{DigestItem, SaturatedConversion};
	use xcm_builder::{HaulBlob, HaulBlobError, HaulBlobExporter};
	use xcm_executor::traits::{validate_export, ExportXcm};

	pub fn prepare_inbound_xcm<InnerXcmRuntimeCall>(
		xcm_message: Xcm<InnerXcmRuntimeCall>,
		destination: InteriorMultiLocation,
	) -> Vec<u8> {
		let location = xcm::VersionedInteriorMultiLocation::V3(destination);
		let xcm = xcm::VersionedXcm::<InnerXcmRuntimeCall>::V3(xcm_message);
		// this is the `BridgeMessage` from polkadot xcm builder, but it has no constructor
		// or public fields, so just tuple
		// (double encoding, because `.encode()` is called on original Xcm BLOB when it is pushed
		// to the storage)
		(location, xcm).encode().encode()
	}

	/// Prepare a batch call with relay finality proof, parachain head proof and message proof.
	pub fn make_complex_relayer_delivery_batch<Runtime, GPI, PPI, MPI>(
		relay_chain_header: BridgedHeader<Runtime, GPI>,
		grandpa_justification: GrandpaJustification<BridgedHeader<Runtime, GPI>>,
		parachain_heads: Vec<(ParaId, ParaHash)>,
		para_heads_proof: ParaHeadsProof,
		message_proof: FromBridgedChainMessagesProof<ParaHash>,
		relayer_id_at_bridged_chain: AccountId32,
	) -> pallet_utility::Call<Runtime> where
		Runtime:pallet_bridge_grandpa::Config<GPI>
			+ pallet_bridge_parachains::Config<PPI>
			+ pallet_bridge_messages::Config<MPI, InboundPayload = XcmAsPlainPayload>
			+ pallet_utility::Config,
		GPI: 'static,
		PPI: 'static,
		MPI: 'static,
		ParaHash: From<<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash>,
		<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash: From<ParaHash>,
		<<Runtime as pallet_bridge_messages::Config<MPI>>::SourceHeaderChain as SourceHeaderChain>::MessagesProof:
			From<FromBridgedChainMessagesProof<ParaHash>>,
		<Runtime as pallet_bridge_messages::Config<MPI>>::InboundRelayer: From<AccountId32>,
		<Runtime as pallet_utility::Config>::RuntimeCall:
			From<pallet_bridge_grandpa::Call<Runtime, GPI>>
			+ From<pallet_bridge_parachains::Call<Runtime, PPI>>
			+ From<pallet_bridge_messages::Call<Runtime, MPI>>,
	{
		let relay_chain_header_hash = relay_chain_header.hash();
		let relay_chain_header_number = *relay_chain_header.number();
		let submit_grandpa = pallet_bridge_grandpa::Call::<Runtime, GPI>::submit_finality_proof {
			finality_target: Box::new(relay_chain_header),
			justification: grandpa_justification,
		};
		let submit_para_head =
			pallet_bridge_parachains::Call::<Runtime, PPI>::submit_parachain_heads {
				at_relay_block: (
					relay_chain_header_number.saturated_into(),
					relay_chain_header_hash.into(),
				),
				parachains: parachain_heads,
				parachain_heads_proof: para_heads_proof,
			};
		let submit_message = pallet_bridge_messages::Call::<Runtime, MPI>::receive_messages_proof {
			relayer_id_at_bridged_chain: relayer_id_at_bridged_chain.into(),
			proof: message_proof.into(),
			messages_count: 1,
			dispatch_weight: Weight::from_parts(1000000000, 0),
		};
		pallet_utility::Call::<Runtime>::batch_all {
			calls: vec![submit_grandpa.into(), submit_para_head.into(), submit_message.into()],
		}
	}

	/// Prepare a batch call with relay finality proof, parachain head proof and message delivery
	/// proof.
	pub fn make_complex_relayer_confirmation_batch<Runtime, GPI, PPI, MPI>(
		relay_chain_header: BridgedHeader<Runtime, GPI>,
		grandpa_justification: GrandpaJustification<BridgedHeader<Runtime, GPI>>,
		parachain_heads: Vec<(ParaId, ParaHash)>,
		para_heads_proof: ParaHeadsProof,
		message_delivery_proof: FromBridgedChainMessagesDeliveryProof<ParaHash>,
		relayers_state: UnrewardedRelayersState,
	) -> pallet_utility::Call<Runtime> where
		Runtime:pallet_bridge_grandpa::Config<GPI>
			+ pallet_bridge_parachains::Config<PPI>
			+ pallet_bridge_messages::Config<MPI, OutboundPayload = XcmAsPlainPayload>
			+ pallet_utility::Config,
		GPI: 'static,
		PPI: 'static,
		MPI: 'static,
		ParaHash: From<<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash>,
		<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash: From<ParaHash>,
		<<Runtime as pallet_bridge_messages::Config<MPI>>::TargetHeaderChain as TargetHeaderChain<
			XcmAsPlainPayload,
			Runtime::AccountId,
		>>::MessagesDeliveryProof: From<FromBridgedChainMessagesDeliveryProof<ParaHash>>,
		<Runtime as pallet_utility::Config>::RuntimeCall:
			From<pallet_bridge_grandpa::Call<Runtime, GPI>>
			+ From<pallet_bridge_parachains::Call<Runtime, PPI>>
			+ From<pallet_bridge_messages::Call<Runtime, MPI>>,
	{
		let relay_chain_header_hash = relay_chain_header.hash();
		let relay_chain_header_number = *relay_chain_header.number();
		let submit_grandpa = pallet_bridge_grandpa::Call::<Runtime, GPI>::submit_finality_proof {
			finality_target: Box::new(relay_chain_header),
			justification: grandpa_justification,
		};
		let submit_para_head =
			pallet_bridge_parachains::Call::<Runtime, PPI>::submit_parachain_heads {
				at_relay_block: (
					relay_chain_header_number.saturated_into(),
					relay_chain_header_hash.into(),
				),
				parachains: parachain_heads,
				parachain_heads_proof: para_heads_proof,
			};
		let submit_message_delivery_proof =
			pallet_bridge_messages::Call::<Runtime, MPI>::receive_messages_delivery_proof {
				proof: message_delivery_proof.into(),
				relayers_state,
			};
		pallet_utility::Call::<Runtime>::batch_all {
			calls: vec![
				submit_grandpa.into(),
				submit_para_head.into(),
				submit_message_delivery_proof.into(),
			],
		}
	}

	/// Prepare storage proofs of messages, stored at the source chain.
	pub fn make_complex_relayer_delivery_proofs<BridgedRelayChain, MB, InnerXcmRuntimeCall>(
		lane_id: LaneId,
		xcm_message: Xcm<InnerXcmRuntimeCall>,
		message_nonce: MessageNonce,
		message_destination: Junctions,
		para_header_number: u32,
		relay_header_number: u32,
		bridged_para_id: u32,
	) -> (
		HeaderOf<BridgedRelayChain>,
		GrandpaJustification<HeaderOf<BridgedRelayChain>>,
		ParaHead,
		Vec<(ParaId, ParaHash)>,
		ParaHeadsProof,
		FromBridgedChainMessagesProof<ParaHash>,
	)
	where
		BridgedRelayChain: ChainWithGrandpa,
		HashOf<BridgedRelayChain>: From<H256>,
		MB: MessageBridge,
		<MB as MessageBridge>::BridgedChain: Send + Sync + 'static,
		<MB as MessageBridge>::ThisChain: Send + Sync + 'static,
		UnderlyingChainOf<MessageBridgedChain<MB>>: bp_runtime::Chain<Hash = ParaHash> + Parachain,
	{
		let message_payload = prepare_inbound_xcm(xcm_message, message_destination);
		let message_size = StorageProofSize::Minimal(message_payload.len() as u32);
		// prepare para storage proof containing message
		let (para_state_root, para_storage_proof) = prepare_messages_storage_proof::<MB>(
			lane_id,
			message_nonce..=message_nonce,
			None,
			message_size,
			message_payload,
			encode_all_messages,
			encode_lane_data,
		);

		let (
			relay_chain_header,
			justification,
			bridged_para_head,
			parachain_heads,
			para_heads_proof,
		) = make_complex_bridged_heads_proof::<BridgedRelayChain, MB>(
			para_state_root,
			para_header_number,
			relay_header_number,
			bridged_para_id,
		);

		let message_proof = FromBridgedChainMessagesProof {
			bridged_header_hash: bridged_para_head.hash(),
			storage_proof: para_storage_proof,
			lane: lane_id,
			nonces_start: message_nonce,
			nonces_end: message_nonce,
		};

		(
			relay_chain_header,
			justification,
			bridged_para_head,
			parachain_heads,
			para_heads_proof,
			message_proof,
		)
	}

	/// Prepare storage proofs of message confirmations, stored at the target chain.
	pub fn make_complex_relayer_confirmation_proofs<BridgedRelayChain, MB, InnerXcmRuntimeCall>(
		lane_id: LaneId,
		para_header_number: u32,
		relay_header_number: u32,
		bridged_para_id: u32,
		relayer_id_at_this_chain: AccountId32,
		relayers_state: UnrewardedRelayersState,
	) -> (
		HeaderOf<BridgedRelayChain>,
		GrandpaJustification<HeaderOf<BridgedRelayChain>>,
		ParaHead,
		Vec<(ParaId, ParaHash)>,
		ParaHeadsProof,
		FromBridgedChainMessagesDeliveryProof<ParaHash>,
	)
	where
		BridgedRelayChain: ChainWithGrandpa,
		HashOf<BridgedRelayChain>: From<H256>,
		MB: MessageBridge,
		<MB as MessageBridge>::BridgedChain: Send + Sync + 'static,
		<MB as MessageBridge>::ThisChain: Send + Sync + 'static,
		<<MB as MessageBridge>::ThisChain as bp_runtime::Chain>::AccountId: From<AccountId32>,
		UnderlyingChainOf<MessageBridgedChain<MB>>: bp_runtime::Chain<Hash = ParaHash> + Parachain,
	{
		// prepare para storage proof containing message delivery proof
		let (para_state_root, para_storage_proof) = prepare_message_delivery_storage_proof::<MB>(
			lane_id,
			InboundLaneData {
				relayers: vec![
					UnrewardedRelayer {
						relayer: relayer_id_at_this_chain.into(),
						messages: DeliveredMessages::new(1)
					};
					relayers_state.unrewarded_relayer_entries as usize
				]
				.into(),
				last_confirmed_nonce: 1,
			},
			StorageProofSize::Minimal(0),
		);

		let (
			relay_chain_header,
			justification,
			bridged_para_head,
			parachain_heads,
			para_heads_proof,
		) = make_complex_bridged_heads_proof::<BridgedRelayChain, MB>(
			para_state_root,
			para_header_number,
			relay_header_number,
			bridged_para_id,
		);

		let message_delivery_proof = FromBridgedChainMessagesDeliveryProof {
			bridged_header_hash: bridged_para_head.hash(),
			storage_proof: para_storage_proof,
			lane: lane_id,
		};

		(
			relay_chain_header,
			justification,
			bridged_para_head,
			parachain_heads,
			para_heads_proof,
			message_delivery_proof,
		)
	}

	/// Make bridged parachain header with given state root and relay header that is finalizing it.
	pub fn make_complex_bridged_heads_proof<BridgedRelayChain, MB>(
		para_state_root: ParaHash,
		para_header_number: u32,
		relay_header_number: u32,
		bridged_para_id: u32,
	) -> (
		HeaderOf<BridgedRelayChain>,
		GrandpaJustification<HeaderOf<BridgedRelayChain>>,
		ParaHead,
		Vec<(ParaId, ParaHash)>,
		ParaHeadsProof,
	)
	where
		BridgedRelayChain: ChainWithGrandpa,
		HashOf<BridgedRelayChain>: From<H256>,
		MB: MessageBridge,
		<MB as MessageBridge>::BridgedChain: Send + Sync + 'static,
		<MB as MessageBridge>::ThisChain: Send + Sync + 'static,
		UnderlyingChainOf<MessageBridgedChain<MB>>: bp_runtime::Chain<Hash = ParaHash> + Parachain,
	{
		let bridged_para_head = ParaHead(
			bp_test_utils::test_header_with_root::<HeaderOf<MB::BridgedChain>>(
				para_header_number.into(),
				para_state_root.into(),
			)
			.encode(),
		);
		let (relay_state_root, para_heads_proof, parachain_heads) =
			prepare_parachain_heads_proof::<HeaderOf<MB::BridgedChain>>(vec![(
				bridged_para_id,
				bridged_para_head.clone(),
			)]);
		assert_eq!(bridged_para_head.hash(), parachain_heads[0].1);

		// import bridged relay chain block#1 with state root containing head#5 of bridged parachain
		let mut relay_chain_header: BridgedRelayChain::Header =
			bp_test_utils::test_header_with_root(
				relay_header_number.into(),
				relay_state_root.into(),
			);
		// to compute proper cost of GRANDPA call, let's add some dummy bytes to header, so that the
		// `submit_finality_proof` call size would be close to maximal expected (and refundable)
		let expected_bytes_in_grandpa_call = BridgedRelayChain::AVERAGE_HEADER_SIZE_IN_JUSTIFICATION
			.saturating_mul(BridgedRelayChain::REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY)
			.saturating_add(BridgedRelayChain::MAX_HEADER_SIZE)
			as usize;
		let extra_bytes_required =
			expected_bytes_in_grandpa_call.saturating_sub(relay_chain_header.encoded_size());
		relay_chain_header
			.digest_mut()
			.push(DigestItem::Other(vec![42; extra_bytes_required]));

		let justification = make_default_justification(&relay_chain_header);
		(relay_chain_header, justification, bridged_para_head, parachain_heads, para_heads_proof)
	}

	/// Helper that creates InitializationData mock data, that can be used to initialize bridge
	/// GRANDPA pallet
	pub fn initialization_data<
		Runtime: pallet_bridge_grandpa::Config<GrandpaPalletInstance>,
		GrandpaPalletInstance: 'static,
	>(
		block_number: u32,
	) -> bp_header_chain::InitializationData<BridgedHeader<Runtime, GrandpaPalletInstance>> {
		bp_header_chain::InitializationData {
			header: Box::new(bp_test_utils::test_header(block_number.into())),
			authority_list: authority_list(),
			set_id: 1,
			operating_mode: BasicOperatingMode::Normal,
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
	/// which are transferred over bridge.
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
