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

//! Module contains predefined test-case scenarios for `Runtime` with bridging capabilities
//! with remote parachain.

use crate::{
	test_cases::{helpers, run_test},
	test_data,
};

use bp_messages::{
	source_chain::TargetHeaderChain, target_chain::SourceHeaderChain, LaneId,
	UnrewardedRelayersState,
};
use bp_polkadot_core::parachains::ParaHash;
use bp_relayers::{RewardsAccountOwner, RewardsAccountParams};
use bp_runtime::{Parachain, UnderlyingChainOf};
use bridge_runtime_common::{
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		BridgedChain as MessageBridgedChain, MessageBridge,
	},
	messages_xcm_extension::XcmAsPlainPayload,
};
use frame_support::traits::{Get, OnFinalize, OnInitialize, OriginTrait};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_bridge_parachains::{RelayBlockHash, RelayBlockNumber};
use parachains_runtimes_test_utils::{
	AccountIdOf, BasicParachainRuntime, CollatorSessionKeys, ValidatorIdOf,
};
use sp_keyring::AccountKeyring::*;
use sp_runtime::{traits::Header as HeaderT, AccountId32};
use xcm::latest::prelude::*;

/// Test-case makes sure that Runtime can dispatch XCM messages submitted by relayer,
/// with proofs (finality, para heads, message) independently submitted.
/// Also verifies relayer transaction signed extensions work as intended.
pub fn relayed_incoming_message_works<
	Runtime,
	AllPalletsWithoutSystem,
	HrmpChannelOpener,
	GPI,
	PPI,
	MPI,
	MB,
>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	bridged_para_id: u32,
	bridged_chain_id: bp_runtime::ChainId,
	sibling_parachain_id: u32,
	local_relay_chain_id: NetworkId,
	lane_id: LaneId,
	prepare_configuration: impl Fn(),
	construct_and_apply_extrinsic: fn(
		sp_keyring::AccountKeyring,
		<Runtime as frame_system::Config>::RuntimeCall,
	) -> sp_runtime::DispatchOutcome,
) where
	Runtime: BasicParachainRuntime
		+ cumulus_pallet_xcmp_queue::Config
		+ cumulus_pallet_parachain_system::Config
		+ pallet_bridge_grandpa::Config<GPI>
		+ pallet_bridge_parachains::Config<PPI>
		+ pallet_bridge_messages::Config<MPI>
		+ pallet_bridge_relayers::Config,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	GPI: 'static,
	PPI: 'static,
	MPI: 'static,
	MB: MessageBridge,
	<MB as MessageBridge>::BridgedChain: Send + Sync + 'static,
	<MB as MessageBridge>::ThisChain: Send + Sync + 'static,
	UnderlyingChainOf<MessageBridgedChain<MB>>: bp_runtime::Chain<Hash = ParaHash> + Parachain,
	HrmpChannelOpener: frame_support::inherent::ProvideInherent<
		Call = cumulus_pallet_parachain_system::Call<Runtime>,
	>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<Runtime as pallet_bridge_messages::Config<MPI>>::SourceHeaderChain:
		SourceHeaderChain<MessagesProof = FromBridgedChainMessagesProof<ParaHash>>,
	<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain:
		bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber>,
	ParaHash: From<
		<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash,
	>,
	<Runtime as frame_system::Config>::AccountId:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<Runtime as frame_system::Config>::AccountId: From<AccountId32>,
	AccountIdOf<Runtime>: From<sp_core::sr25519::Public>,
	<Runtime as pallet_bridge_messages::Config<MPI>>::InboundRelayer: From<AccountId32>,
	<Runtime as frame_system::Config>::RuntimeCall: From<pallet_bridge_grandpa::Call<Runtime, GPI>>
		+ From<pallet_bridge_parachains::Call<Runtime, PPI>>
		+ From<pallet_bridge_messages::Call<Runtime, MPI>>,
{
	helpers::relayed_incoming_message_works::<
		Runtime,
		AllPalletsWithoutSystem,
		HrmpChannelOpener,
		MPI,
	>(
		collator_session_key,
		runtime_para_id,
		sibling_parachain_id,
		local_relay_chain_id,
		construct_and_apply_extrinsic,
		|relayer_id_at_this_chain,
		 relayer_id_at_bridged_chain,
		 message_destination,
		 message_nonce,
		 xcm| {
			let para_header_number = 5;
			let relay_header_number = 1;

			prepare_configuration();

			// start with bridged relay chain block#0
			helpers::initialize_bridge_grandpa_pallet::<Runtime, GPI>(
				test_data::initialization_data::<Runtime, GPI>(0),
			);

			// generate bridged relay chain finality, parachain heads and message proofs,
			// to be submitted by relayer to this chain.
			let (
				relay_chain_header,
				grandpa_justification,
				parachain_head,
				parachain_heads,
				para_heads_proof,
				message_proof,
			) = test_data::from_parachain::make_complex_relayer_delivery_proofs::<
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

			let parachain_head_hash = parachain_head.hash();
			let relay_chain_header_hash = relay_chain_header.hash();
			let relay_chain_header_number = *relay_chain_header.number();
			vec![
				(
					pallet_bridge_grandpa::Call::<Runtime, GPI>::submit_finality_proof {
						finality_target: Box::new(relay_chain_header),
						justification: grandpa_justification,
					}.into(),
					helpers::VerifySubmitGrandpaFinalityProofOutcome::<Runtime, GPI>::expect_best_header_hash(relay_chain_header_hash),
				),
				(
					pallet_bridge_parachains::Call::<Runtime, PPI>::submit_parachain_heads {
						at_relay_block: (relay_chain_header_number, relay_chain_header_hash),
						parachains: parachain_heads,
						parachain_heads_proof: para_heads_proof,
					}.into(),
					helpers::VerifySubmitParachainHeaderProofOutcome::<Runtime, PPI>::expect_best_header_hash(bridged_para_id, parachain_head_hash),
				),
				(
					pallet_bridge_messages::Call::<Runtime, MPI>::receive_messages_proof {
						relayer_id_at_bridged_chain,
						proof: message_proof,
						messages_count: 1,
						dispatch_weight: Weight::from_parts(1000000000, 0),
					}.into(),
					Box::new((
						helpers::VerifySubmitMessagesProofOutcome::<Runtime, MPI>::expect_last_delivered_nonce(lane_id, 1),
						helpers::VerifyRelayerRewarded::<Runtime>::expect_relayer_reward(
							relayer_id_at_this_chain,
							RewardsAccountParams::new(
								lane_id,
								bridged_chain_id,
								RewardsAccountOwner::ThisChain,
							),
						),
					)),
				),
			]
		},
	);
}

/// Test-case makes sure that Runtime can dispatch XCM messages submitted by relayer,
/// with proofs (finality, para heads, message) batched together in signed extrinsic.
/// Also verifies relayer transaction signed extensions work as intended.
pub fn complex_relay_extrinsic_works<
	Runtime,
	AllPalletsWithoutSystem,
	XcmConfig,
	HrmpChannelOpener,
	GPI,
	PPI,
	MPI,
	MB,
>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	bridged_para_id: u32,
	sibling_parachain_id: u32,
	bridged_chain_id: bp_runtime::ChainId,
	local_relay_chain_id: NetworkId,
	lane_id: LaneId,
	prepare_configuration: impl Fn(),
	construct_and_apply_extrinsic: fn(
		sp_keyring::AccountKeyring,
		<Runtime as frame_system::Config>::RuntimeCall,
	) -> sp_runtime::DispatchOutcome,
) where
	Runtime: BasicParachainRuntime
		+ cumulus_pallet_xcmp_queue::Config
		+ cumulus_pallet_parachain_system::Config
		+ pallet_bridge_grandpa::Config<GPI>
		+ pallet_bridge_parachains::Config<PPI>
		+ pallet_bridge_messages::Config<MPI>
		+ pallet_bridge_relayers::Config
		+ pallet_utility::Config,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	GPI: 'static,
	PPI: 'static,
	MPI: 'static,
	MB: MessageBridge,
	<MB as MessageBridge>::BridgedChain: Send + Sync + 'static,
	<MB as MessageBridge>::ThisChain: Send + Sync + 'static,
	UnderlyingChainOf<MessageBridgedChain<MB>>: bp_runtime::Chain<Hash = ParaHash> + Parachain,
	HrmpChannelOpener: frame_support::inherent::ProvideInherent<
		Call = cumulus_pallet_parachain_system::Call<Runtime>,
	>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<Runtime as pallet_bridge_messages::Config<MPI>>::SourceHeaderChain:
		SourceHeaderChain<MessagesProof = FromBridgedChainMessagesProof<ParaHash>>,
	<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain:
		bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber>,
	ParaHash: From<
		<<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain as bp_runtime::Chain>::Hash,
	>,
	<Runtime as frame_system::Config>::AccountId:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<Runtime as frame_system::Config>::AccountId: From<AccountId32>,
	AccountIdOf<Runtime>: From<sp_core::sr25519::Public>,
	<Runtime as pallet_bridge_messages::Config<MPI>>::InboundRelayer: From<AccountId32>,
	<Runtime as pallet_utility::Config>::RuntimeCall: From<pallet_bridge_grandpa::Call<Runtime, GPI>>
		+ From<pallet_bridge_parachains::Call<Runtime, PPI>>
		+ From<pallet_bridge_messages::Call<Runtime, MPI>>,
	<Runtime as frame_system::Config>::RuntimeCall: From<pallet_utility::Call<Runtime>>,
{
	helpers::relayed_incoming_message_works::<
		Runtime,
		AllPalletsWithoutSystem,
		HrmpChannelOpener,
		MPI,
	>(
		collator_session_key,
		runtime_para_id,
		sibling_parachain_id,
		local_relay_chain_id,
		construct_and_apply_extrinsic,
		|relayer_id_at_this_chain,
		 relayer_id_at_bridged_chain,
		 message_destination,
		 message_nonce,
		 xcm| {
			let para_header_number = 5;
			let relay_header_number = 1;

			prepare_configuration();

			// start with bridged relay chain block#0
			helpers::initialize_bridge_grandpa_pallet::<Runtime, GPI>(
				test_data::initialization_data::<Runtime, GPI>(0),
			);

			// generate bridged relay chain finality, parachain heads and message proofs,
			// to be submitted by relayer to this chain.
			let (
				relay_chain_header,
				grandpa_justification,
				parachain_head,
				parachain_heads,
				para_heads_proof,
				message_proof,
			) = test_data::from_parachain::make_complex_relayer_delivery_proofs::<
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

			let parachain_head_hash = parachain_head.hash();
			let relay_chain_header_hash = relay_chain_header.hash();
			let relay_chain_header_number = *relay_chain_header.number();
			vec![(
				pallet_utility::Call::<Runtime>::batch_all {
					calls: vec![
						pallet_bridge_grandpa::Call::<Runtime, GPI>::submit_finality_proof {
							finality_target: Box::new(relay_chain_header),
							justification: grandpa_justification,
						}.into(),
						pallet_bridge_parachains::Call::<Runtime, PPI>::submit_parachain_heads {
							at_relay_block: (relay_chain_header_number, relay_chain_header_hash),
							parachains: parachain_heads,
							parachain_heads_proof: para_heads_proof,
						}.into(),
						pallet_bridge_messages::Call::<Runtime, MPI>::receive_messages_proof {
							relayer_id_at_bridged_chain,
							proof: message_proof,
							messages_count: 1,
							dispatch_weight: Weight::from_parts(1000000000, 0),
						}.into(),
					],
				}.into(),
				Box::new((
					helpers::VerifySubmitGrandpaFinalityProofOutcome::<Runtime, GPI>::expect_best_header_hash(relay_chain_header_hash),
					helpers::VerifySubmitParachainHeaderProofOutcome::<Runtime, PPI>::expect_best_header_hash(bridged_para_id, parachain_head_hash),
					helpers::VerifySubmitMessagesProofOutcome::<Runtime, MPI>::expect_last_delivered_nonce(lane_id, 1),
					helpers::VerifyRelayerRewarded::<Runtime>::expect_relayer_reward(
						relayer_id_at_this_chain,
						RewardsAccountParams::new(
							lane_id,
							bridged_chain_id,
							RewardsAccountOwner::ThisChain,
						),
					),
				)),
			)]
		},
	);
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
		<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain: bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber>,
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
	run_test::<Runtime, _>(collator_session_key, 1000, vec![], || {
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
		) = test_data::from_parachain::make_complex_relayer_delivery_proofs::<
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
		let batch = test_data::from_parachain::make_complex_relayer_delivery_batch::<
			Runtime,
			GPI,
			PPI,
			MPI,
		>(
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
	<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain: bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber>,
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
	run_test::<Runtime, _>(collator_session_key, 1000, vec![], || {
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
		) = test_data::from_parachain::make_complex_relayer_confirmation_proofs::<
			<Runtime as pallet_bridge_grandpa::Config<GPI>>::BridgedChain,
			MB,
			(),
		>(LaneId::default(), 1, 5, 1_000, Alice.public().into(), unrewarded_relayers.clone());

		// generate batch call that provides finality for bridged relay and parachains + message
		// proof
		let batch = test_data::from_parachain::make_complex_relayer_confirmation_batch::<
			Runtime,
			GPI,
			PPI,
			MPI,
		>(
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
