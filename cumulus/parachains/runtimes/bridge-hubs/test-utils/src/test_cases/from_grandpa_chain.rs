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
//! with remote GRANDPA chain.

use crate::{
	test_cases::{bridges_prelude::*, helpers, run_test},
	test_data,
};

use bp_header_chain::ChainWithGrandpa;
use bp_messages::{
	source_chain::TargetHeaderChain, target_chain::SourceHeaderChain, LaneId,
	UnrewardedRelayersState,
};
use bp_relayers::{RewardsAccountOwner, RewardsAccountParams};
use bp_runtime::{HashOf, UnderlyingChainOf};
use bridge_runtime_common::{
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		BridgedChain as MessageBridgedChain, MessageBridge, ThisChain as MessageThisChain,
	},
	messages_xcm_extension::XcmAsPlainPayload,
};
use frame_support::traits::{OnFinalize, OnInitialize};
use frame_system::pallet_prelude::BlockNumberFor;
use parachains_runtimes_test_utils::{
	AccountIdOf, BasicParachainRuntime, CollatorSessionKeys, RuntimeCallOf, SlotDurations,
};
use sp_keyring::AccountKeyring::*;
use sp_runtime::{traits::Header as HeaderT, AccountId32};
use xcm::latest::prelude::*;

/// Helper trait to test bridges with remote GRANDPA chain.
///
/// This is only used to decrease amount of lines, dedicated to bounds.
pub trait WithRemoteGrandpaChainHelper {
	/// This chain runtime.
	type Runtime: BasicParachainRuntime
		+ cumulus_pallet_xcmp_queue::Config
		+ BridgeGrandpaConfig<
			Self::GPI,
			BridgedChain = UnderlyingChainOf<MessageBridgedChain<Self::MB>>,
		> + BridgeMessagesConfig<
			Self::MPI,
			InboundPayload = XcmAsPlainPayload,
			InboundRelayer = bp_runtime::AccountIdOf<MessageBridgedChain<Self::MB>>,
			OutboundPayload = XcmAsPlainPayload,
		> + pallet_bridge_relayers::Config;
	/// All pallets of this chain, excluding system pallet.
	type AllPalletsWithoutSystem: OnInitialize<BlockNumberFor<Self::Runtime>>
		+ OnFinalize<BlockNumberFor<Self::Runtime>>;
	/// Instance of the `pallet-bridge-grandpa`, used to bridge with remote GRANDPA chain.
	type GPI: 'static;
	/// Instance of the `pallet-bridge-messages`, used to bridge with remote GRANDPA chain.
	type MPI: 'static;
	/// Messages bridge definition.
	type MB: MessageBridge;
}

/// Adapter struct that implements [`WithRemoteGrandpaChainHelper`].
pub struct WithRemoteGrandpaChainHelperAdapter<Runtime, AllPalletsWithoutSystem, GPI, MPI, MB>(
	sp_std::marker::PhantomData<(Runtime, AllPalletsWithoutSystem, GPI, MPI, MB)>,
);

impl<Runtime, AllPalletsWithoutSystem, GPI, MPI, MB> WithRemoteGrandpaChainHelper
	for WithRemoteGrandpaChainHelperAdapter<Runtime, AllPalletsWithoutSystem, GPI, MPI, MB>
where
	Runtime: BasicParachainRuntime
		+ cumulus_pallet_xcmp_queue::Config
		+ BridgeGrandpaConfig<GPI, BridgedChain = UnderlyingChainOf<MessageBridgedChain<MB>>>
		+ BridgeMessagesConfig<
			MPI,
			InboundPayload = XcmAsPlainPayload,
			InboundRelayer = bp_runtime::AccountIdOf<MessageBridgedChain<MB>>,
			OutboundPayload = XcmAsPlainPayload,
		> + pallet_bridge_relayers::Config,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	GPI: 'static,
	MPI: 'static,
	MB: MessageBridge,
{
	type Runtime = Runtime;
	type AllPalletsWithoutSystem = AllPalletsWithoutSystem;
	type GPI = GPI;
	type MPI = MPI;
	type MB = MB;
}

/// Test-case makes sure that Runtime can dispatch XCM messages submitted by relayer,
/// with proofs (finality, message) independently submitted.
/// Also verifies relayer transaction signed extensions work as intended.
pub fn relayed_incoming_message_works<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	slot_durations: SlotDurations,
	runtime_para_id: u32,
	bridged_chain_id: bp_runtime::ChainId,
	sibling_parachain_id: u32,
	local_relay_chain_id: NetworkId,
	lane_id: LaneId,
	prepare_configuration: impl Fn(),
	construct_and_apply_extrinsic: fn(
		sp_keyring::AccountKeyring,
		RuntimeCallOf<RuntimeHelper::Runtime>,
	) -> sp_runtime::DispatchOutcome,
) where
	RuntimeHelper: WithRemoteGrandpaChainHelper,
	AccountIdOf<RuntimeHelper::Runtime>: From<AccountId32>,
	RuntimeCallOf<RuntimeHelper::Runtime>: From<BridgeGrandpaCall<RuntimeHelper::Runtime, RuntimeHelper::GPI>>
		+ From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>,
	UnderlyingChainOf<MessageBridgedChain<RuntimeHelper::MB>>: ChainWithGrandpa,
	<RuntimeHelper::Runtime as BridgeMessagesConfig<RuntimeHelper::MPI>>::SourceHeaderChain:
		SourceHeaderChain<
			MessagesProof = FromBridgedChainMessagesProof<
				HashOf<MessageBridgedChain<RuntimeHelper::MB>>,
			>,
		>,
{
	helpers::relayed_incoming_message_works::<
		RuntimeHelper::Runtime,
		RuntimeHelper::AllPalletsWithoutSystem,
		RuntimeHelper::MPI,
	>(
		collator_session_key,
		slot_durations,
		runtime_para_id,
		sibling_parachain_id,
		local_relay_chain_id,
		construct_and_apply_extrinsic,
		|relayer_id_at_this_chain,
		 relayer_id_at_bridged_chain,
		 message_destination,
		 message_nonce,
		 xcm| {
			let relay_header_number = 5u32.into();

			prepare_configuration();

			// start with bridged relay chain block#0
			helpers::initialize_bridge_grandpa_pallet::<RuntimeHelper::Runtime, RuntimeHelper::GPI>(
				test_data::initialization_data::<RuntimeHelper::Runtime, RuntimeHelper::GPI>(0),
			);

			// generate bridged relay chain finality, parachain heads and message proofs,
			// to be submitted by relayer to this chain.
			let (relay_chain_header, grandpa_justification, message_proof) =
				test_data::from_grandpa_chain::make_complex_relayer_delivery_proofs::<
					RuntimeHelper::MB,
					(),
				>(lane_id, xcm.into(), message_nonce, message_destination, relay_header_number);

			let relay_chain_header_hash = relay_chain_header.hash();
			vec![
				(
					BridgeGrandpaCall::<RuntimeHelper::Runtime, RuntimeHelper::GPI>::submit_finality_proof {
						finality_target: Box::new(relay_chain_header),
						justification: grandpa_justification,
					}.into(),
					helpers::VerifySubmitGrandpaFinalityProofOutcome::<RuntimeHelper::Runtime, RuntimeHelper::GPI>::expect_best_header_hash(
						relay_chain_header_hash,
					),
				),
				(
					BridgeMessagesCall::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::receive_messages_proof {
						relayer_id_at_bridged_chain,
						proof: message_proof,
						messages_count: 1,
						dispatch_weight: Weight::from_parts(1000000000, 0),
					}.into(),
					Box::new((
						helpers::VerifySubmitMessagesProofOutcome::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::expect_last_delivered_nonce(
							lane_id,
							1,
						),
						helpers::VerifyRelayerRewarded::<RuntimeHelper::Runtime>::expect_relayer_reward(
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
/// with proofs (finality, message) batched together in signed extrinsic.
/// Also verifies relayer transaction signed extensions work as intended.
pub fn complex_relay_extrinsic_works<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	slot_durations: SlotDurations,
	runtime_para_id: u32,
	sibling_parachain_id: u32,
	bridged_chain_id: bp_runtime::ChainId,
	local_relay_chain_id: NetworkId,
	lane_id: LaneId,
	prepare_configuration: impl Fn(),
	construct_and_apply_extrinsic: fn(
		sp_keyring::AccountKeyring,
		RuntimeCallOf<RuntimeHelper::Runtime>,
	) -> sp_runtime::DispatchOutcome,
) where
	RuntimeHelper: WithRemoteGrandpaChainHelper,
	RuntimeHelper::Runtime:
		pallet_utility::Config<RuntimeCall = RuntimeCallOf<RuntimeHelper::Runtime>>,
	AccountIdOf<RuntimeHelper::Runtime>: From<AccountId32>,
	RuntimeCallOf<RuntimeHelper::Runtime>: From<BridgeGrandpaCall<RuntimeHelper::Runtime, RuntimeHelper::GPI>>
		+ From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>
		+ From<pallet_utility::Call<RuntimeHelper::Runtime>>,
	UnderlyingChainOf<MessageBridgedChain<RuntimeHelper::MB>>: ChainWithGrandpa,
	<RuntimeHelper::Runtime as BridgeMessagesConfig<RuntimeHelper::MPI>>::SourceHeaderChain:
		SourceHeaderChain<
			MessagesProof = FromBridgedChainMessagesProof<
				HashOf<MessageBridgedChain<RuntimeHelper::MB>>,
			>,
		>,
{
	helpers::relayed_incoming_message_works::<
		RuntimeHelper::Runtime,
		RuntimeHelper::AllPalletsWithoutSystem,
		RuntimeHelper::MPI,
	>(
		collator_session_key,
		slot_durations,
		runtime_para_id,
		sibling_parachain_id,
		local_relay_chain_id,
		construct_and_apply_extrinsic,
		|relayer_id_at_this_chain,
		 relayer_id_at_bridged_chain,
		 message_destination,
		 message_nonce,
		 xcm| {
			let relay_header_number = 1u32.into();

			prepare_configuration();

			// start with bridged relay chain block#0
			helpers::initialize_bridge_grandpa_pallet::<RuntimeHelper::Runtime, RuntimeHelper::GPI>(
				test_data::initialization_data::<RuntimeHelper::Runtime, RuntimeHelper::GPI>(0),
			);

			// generate bridged relay chain finality, parachain heads and message proofs,
			// to be submitted by relayer to this chain.
			let (relay_chain_header, grandpa_justification, message_proof) =
				test_data::from_grandpa_chain::make_complex_relayer_delivery_proofs::<
					RuntimeHelper::MB,
					(),
				>(lane_id, xcm.into(), message_nonce, message_destination, relay_header_number);

			let relay_chain_header_hash = relay_chain_header.hash();
			vec![(
				pallet_utility::Call::<RuntimeHelper::Runtime>::batch_all {
					calls: vec![
						BridgeGrandpaCall::<RuntimeHelper::Runtime, RuntimeHelper::GPI>::submit_finality_proof {
							finality_target: Box::new(relay_chain_header),
							justification: grandpa_justification,
						}.into(),
						BridgeMessagesCall::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::receive_messages_proof {
							relayer_id_at_bridged_chain,
							proof: message_proof,
							messages_count: 1,
							dispatch_weight: Weight::from_parts(1000000000, 0),
						}.into(),
					],
				}
				.into(),
				Box::new((
					helpers::VerifySubmitGrandpaFinalityProofOutcome::<
						RuntimeHelper::Runtime,
						RuntimeHelper::GPI,
					>::expect_best_header_hash(relay_chain_header_hash),
					helpers::VerifySubmitMessagesProofOutcome::<
						RuntimeHelper::Runtime,
						RuntimeHelper::MPI,
					>::expect_last_delivered_nonce(lane_id, 1),
					helpers::VerifyRelayerRewarded::<RuntimeHelper::Runtime>::expect_relayer_reward(
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
/// proofs) from bridged GRANDPA chain.
pub fn can_calculate_fee_for_complex_message_delivery_transaction<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	compute_extrinsic_fee: fn(pallet_utility::Call<RuntimeHelper::Runtime>) -> u128,
) -> u128
where
	RuntimeHelper: WithRemoteGrandpaChainHelper,
	RuntimeHelper::Runtime:
		pallet_utility::Config<RuntimeCall = RuntimeCallOf<RuntimeHelper::Runtime>>,
	RuntimeCallOf<RuntimeHelper::Runtime>: From<BridgeGrandpaCall<RuntimeHelper::Runtime, RuntimeHelper::GPI>>
		+ From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>,
	UnderlyingChainOf<MessageBridgedChain<RuntimeHelper::MB>>: ChainWithGrandpa,
	<RuntimeHelper::Runtime as BridgeMessagesConfig<RuntimeHelper::MPI>>::SourceHeaderChain:
		SourceHeaderChain<
			MessagesProof = FromBridgedChainMessagesProof<
				HashOf<MessageBridgedChain<RuntimeHelper::MB>>,
			>,
		>,
{
	run_test::<RuntimeHelper::Runtime, _>(collator_session_key, 1000, vec![], || {
		// generate bridged relay chain finality, parachain heads and message proofs,
		// to be submitted by relayer to this chain.
		//
		// we don't care about parameter values here, apart from the XCM message size. But we
		// do not need to have a large message here, because we're charging for every byte of
		// the message additionally
		let (relay_chain_header, grandpa_justification, message_proof) =
			test_data::from_grandpa_chain::make_complex_relayer_delivery_proofs::<
				RuntimeHelper::MB,
				(),
			>(
				LaneId::default(),
				vec![Instruction::<()>::ClearOrigin; 1_024].into(),
				1,
				[GlobalConsensus(Polkadot), Parachain(1_000)].into(),
				1u32.into(),
			);

		// generate batch call that provides finality for bridged relay and parachains + message
		// proof
		let batch = test_data::from_grandpa_chain::make_complex_relayer_delivery_batch::<
			RuntimeHelper::Runtime,
			RuntimeHelper::GPI,
			RuntimeHelper::MPI,
		>(
			relay_chain_header,
			grandpa_justification,
			message_proof,
			helpers::relayer_id_at_bridged_chain::<RuntimeHelper::Runtime, RuntimeHelper::MPI>(),
		);

		compute_extrinsic_fee(batch)
	})
}

/// Estimates transaction fee for default message confirmation transaction (batched with required
/// proofs) from bridged GRANDPA chain.
pub fn can_calculate_fee_for_complex_message_confirmation_transaction<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	compute_extrinsic_fee: fn(pallet_utility::Call<RuntimeHelper::Runtime>) -> u128,
) -> u128
where
	RuntimeHelper: WithRemoteGrandpaChainHelper,
	AccountIdOf<RuntimeHelper::Runtime>: From<AccountId32>,
	RuntimeHelper::Runtime:
		pallet_utility::Config<RuntimeCall = RuntimeCallOf<RuntimeHelper::Runtime>>,
	MessageThisChain<RuntimeHelper::MB>:
		bp_runtime::Chain<AccountId = AccountIdOf<RuntimeHelper::Runtime>>,
	RuntimeCallOf<RuntimeHelper::Runtime>: From<BridgeGrandpaCall<RuntimeHelper::Runtime, RuntimeHelper::GPI>>
		+ From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>,
	UnderlyingChainOf<MessageBridgedChain<RuntimeHelper::MB>>: ChainWithGrandpa,
	<RuntimeHelper::Runtime as BridgeMessagesConfig<RuntimeHelper::MPI>>::TargetHeaderChain:
		TargetHeaderChain<
			XcmAsPlainPayload,
			AccountIdOf<RuntimeHelper::Runtime>,
			MessagesDeliveryProof = FromBridgedChainMessagesDeliveryProof<
				HashOf<UnderlyingChainOf<MessageBridgedChain<RuntimeHelper::MB>>>,
			>,
		>,
{
	run_test::<RuntimeHelper::Runtime, _>(collator_session_key, 1000, vec![], || {
		// generate bridged relay chain finality, parachain heads and message proofs,
		// to be submitted by relayer to this chain.
		let unrewarded_relayers = UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			total_messages: 1,
			..Default::default()
		};
		let (relay_chain_header, grandpa_justification, message_delivery_proof) =
			test_data::from_grandpa_chain::make_complex_relayer_confirmation_proofs::<
				RuntimeHelper::MB,
				(),
			>(
				LaneId::default(),
				1u32.into(),
				AccountId32::from(Alice.public()).into(),
				unrewarded_relayers.clone(),
			);

		// generate batch call that provides finality for bridged relay and parachains + message
		// proof
		let batch = test_data::from_grandpa_chain::make_complex_relayer_confirmation_batch::<
			RuntimeHelper::Runtime,
			RuntimeHelper::GPI,
			RuntimeHelper::MPI,
		>(
			relay_chain_header,
			grandpa_justification,
			message_delivery_proof,
			unrewarded_relayers,
		);

		compute_extrinsic_fee(batch)
	})
}
