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

//! Module contains predefined test-case scenarios for `Runtime` with bridging capabilities
//! with remote parachain.

use crate::{
	test_cases::{bridges_prelude::*, helpers, run_test},
	test_data,
	test_data::XcmAsPlainPayload,
};

use alloc::{boxed::Box, vec};
use bp_header_chain::ChainWithGrandpa;
use bp_messages::UnrewardedRelayersState;
use bp_polkadot_core::parachains::ParaHash;
use bp_relayers::{RewardsAccountOwner, RewardsAccountParams};
use bp_runtime::{Chain, Parachain};
use frame_support::traits::{OnFinalize, OnInitialize};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_bridge_messages::{BridgedChainOf, LaneIdOf, ThisChainOf};
use parachains_runtimes_test_utils::{
	AccountIdOf, BasicParachainRuntime, CollatorSessionKeys, RuntimeCallOf, SlotDurations,
};
use sp_core::Get;
use sp_keyring::Sr25519Keyring::*;
use sp_runtime::{traits::Header as HeaderT, AccountId32};
use xcm::latest::prelude::*;

/// Helper trait to test bridges with remote parachain.
///
/// This is only used to decrease amount of lines, dedicated to bounds.
pub trait WithRemoteParachainHelper {
	/// This chain runtime.
	type Runtime: BasicParachainRuntime
		+ cumulus_pallet_xcmp_queue::Config
		+ BridgeGrandpaConfig<Self::GPI>
		+ BridgeParachainsConfig<Self::PPI>
		+ BridgeMessagesConfig<
			Self::MPI,
			InboundPayload = XcmAsPlainPayload,
			OutboundPayload = XcmAsPlainPayload,
		> + pallet_bridge_relayers::Config<Self::RPI, Reward = Self::RelayerReward>;
	/// All pallets of this chain, excluding system pallet.
	type AllPalletsWithoutSystem: OnInitialize<BlockNumberFor<Self::Runtime>>
		+ OnFinalize<BlockNumberFor<Self::Runtime>>;
	/// Instance of the `pallet-bridge-grandpa`, used to bridge with remote relay chain.
	type GPI: 'static;
	/// Instance of the `pallet-bridge-parachains`, used to bridge with remote parachain.
	type PPI: 'static;
	/// Instance of the `pallet-bridge-messages`, used to bridge with remote parachain.
	type MPI: 'static;
	/// Instance of the `pallet-bridge-relayers`, used to collect rewards from messages `MPI`
	/// instance.
	type RPI: 'static;
	/// Relayer reward type.
	type RelayerReward: From<RewardsAccountParams<LaneIdOf<Self::Runtime, Self::MPI>>>;
}

/// Adapter struct that implements `WithRemoteParachainHelper`.
pub struct WithRemoteParachainHelperAdapter<Runtime, AllPalletsWithoutSystem, GPI, PPI, MPI, RPI>(
	core::marker::PhantomData<(Runtime, AllPalletsWithoutSystem, GPI, PPI, MPI, RPI)>,
);

impl<Runtime, AllPalletsWithoutSystem, GPI, PPI, MPI, RPI> WithRemoteParachainHelper
	for WithRemoteParachainHelperAdapter<Runtime, AllPalletsWithoutSystem, GPI, PPI, MPI, RPI>
where
	Runtime: BasicParachainRuntime
		+ cumulus_pallet_xcmp_queue::Config
		+ BridgeGrandpaConfig<GPI>
		+ BridgeParachainsConfig<PPI>
		+ BridgeMessagesConfig<
			MPI,
			InboundPayload = XcmAsPlainPayload,
			OutboundPayload = XcmAsPlainPayload,
		> + pallet_bridge_relayers::Config<RPI>,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	<Runtime as pallet_bridge_relayers::Config<RPI>>::Reward:
		From<RewardsAccountParams<LaneIdOf<Runtime, MPI>>>,
	GPI: 'static,
	PPI: 'static,
	MPI: 'static,
	RPI: 'static,
{
	type Runtime = Runtime;
	type AllPalletsWithoutSystem = AllPalletsWithoutSystem;
	type GPI = GPI;
	type PPI = PPI;
	type MPI = MPI;
	type RPI = RPI;
	type RelayerReward = Runtime::Reward;
}

/// Test-case makes sure that Runtime can dispatch XCM messages submitted by relayer,
/// with proofs (finality, para heads, message) independently submitted.
/// Also verifies relayer transaction signed extensions work as intended.
pub fn relayed_incoming_message_works<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	slot_durations: SlotDurations,
	runtime_para_id: u32,
	bridged_para_id: u32,
	sibling_parachain_id: u32,
	local_relay_chain_id: NetworkId,
	prepare_configuration: impl Fn() -> LaneIdOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
	construct_and_apply_extrinsic: fn(
		sp_keyring::Sr25519Keyring,
		<RuntimeHelper::Runtime as frame_system::Config>::RuntimeCall,
	) -> sp_runtime::DispatchOutcome,
	expect_rewards: bool,
) where
	RuntimeHelper: WithRemoteParachainHelper,
	AccountIdOf<RuntimeHelper::Runtime>: From<AccountId32>,
	RuntimeCallOf<RuntimeHelper::Runtime>: From<BridgeGrandpaCall<RuntimeHelper::Runtime, RuntimeHelper::GPI>>
		+ From<BridgeParachainsCall<RuntimeHelper::Runtime, RuntimeHelper::PPI>>
		+ From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>,
	BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>: Chain<Hash = ParaHash> + Parachain,
	<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain:
		bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber> + ChainWithGrandpa,
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
		 xcm,
		 bridged_chain_id| {
			let para_header_number = 5;
			let relay_header_number = 1;

			let lane_id = prepare_configuration();

			// start with bridged relay chain block#0
			helpers::initialize_bridge_grandpa_pallet::<RuntimeHelper::Runtime, RuntimeHelper::GPI>(
				test_data::initialization_data::<RuntimeHelper::Runtime, RuntimeHelper::GPI>(0),
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
				<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain,
				BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
				ThisChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
				LaneIdOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
			>(
				lane_id,
				xcm.into(),
				message_nonce,
				message_destination,
				para_header_number,
				relay_header_number,
				bridged_para_id,
				false,
			);

			let parachain_head_hash = parachain_head.hash();
			let relay_chain_header_hash = relay_chain_header.hash();
			let relay_chain_header_number = *relay_chain_header.number();
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
					BridgeParachainsCall::<RuntimeHelper::Runtime, RuntimeHelper::PPI>::submit_parachain_heads {
						at_relay_block: (relay_chain_header_number, relay_chain_header_hash),
						parachains: parachain_heads,
						parachain_heads_proof: para_heads_proof,
					}.into(),
					helpers::VerifySubmitParachainHeaderProofOutcome::<RuntimeHelper::Runtime, RuntimeHelper::PPI>::expect_best_header_hash(
						bridged_para_id,
						parachain_head_hash,
					),
				),
				(
					BridgeMessagesCall::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::receive_messages_proof {
						relayer_id_at_bridged_chain,
						proof: Box::new(message_proof),
						messages_count: 1,
						dispatch_weight: Weight::from_parts(1000000000, 0),
					}.into(),
					Box::new((
						helpers::VerifySubmitMessagesProofOutcome::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::expect_last_delivered_nonce(
							lane_id,
							1,
						),
						if expect_rewards {
                            helpers::VerifyRelayerRewarded::<RuntimeHelper::Runtime, RuntimeHelper::RPI>::expect_relayer_reward(
                                relayer_id_at_this_chain,
                                RewardsAccountParams::new(
                                    lane_id,
                                    bridged_chain_id,
                                    RewardsAccountOwner::ThisChain,
                                ),
                            )
						} else {
							Box::new(())
						}
					)),
				),
			]
		},
	);
}

/// Test-case makes sure that Runtime can dispatch XCM messages submitted by relayer,
/// with proofs (finality, para heads, message) independently submitted.
/// Finality and para heads are submitted for free in this test.
/// Also verifies relayer transaction signed extensions work as intended.
pub fn free_relay_extrinsic_works<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	slot_durations: SlotDurations,
	runtime_para_id: u32,
	bridged_para_id: u32,
	sibling_parachain_id: u32,
	local_relay_chain_id: NetworkId,
	prepare_configuration: impl Fn() -> LaneIdOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
	construct_and_apply_extrinsic: fn(
		sp_keyring::Sr25519Keyring,
		<RuntimeHelper::Runtime as frame_system::Config>::RuntimeCall,
	) -> sp_runtime::DispatchOutcome,
	expect_rewards: bool,
) where
	RuntimeHelper: WithRemoteParachainHelper,
	RuntimeHelper::Runtime: pallet_balances::Config,
	AccountIdOf<RuntimeHelper::Runtime>: From<AccountId32>,
	RuntimeCallOf<RuntimeHelper::Runtime>: From<BridgeGrandpaCall<RuntimeHelper::Runtime, RuntimeHelper::GPI>>
		+ From<BridgeParachainsCall<RuntimeHelper::Runtime, RuntimeHelper::PPI>>
		+ From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>,
	BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>: Chain<Hash = ParaHash> + Parachain,
	<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain:
		bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber> + ChainWithGrandpa,
{
	// ensure that the runtime allows free header submissions
	let free_headers_interval = <RuntimeHelper::Runtime as BridgeGrandpaConfig<
		RuntimeHelper::GPI,
	>>::FreeHeadersInterval::get()
	.expect("this test requires runtime, configured to accept headers for free; qed");

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
		 xcm,
		 bridged_chain_id| {
			let lane_id = prepare_configuration();

			// start with bridged relay chain block#0
			let initial_block_number = 0;
			helpers::initialize_bridge_grandpa_pallet::<RuntimeHelper::Runtime, RuntimeHelper::GPI>(
				test_data::initialization_data::<RuntimeHelper::Runtime, RuntimeHelper::GPI>(
					initial_block_number,
				),
			);

			// free relay chain header is `0 + free_headers_interval`
			let relay_header_number = initial_block_number + free_headers_interval;
			// first parachain header is always submitted for free
			let para_header_number = 1;

			// relayer balance shall not change after relay and para header submissions
			let initial_relayer_balance =
				pallet_balances::Pallet::<RuntimeHelper::Runtime>::free_balance(
					relayer_id_at_this_chain.clone(),
				);

			// initialize the `FreeHeadersRemaining` storage value
			pallet_bridge_grandpa::Pallet::<RuntimeHelper::Runtime, RuntimeHelper::GPI>::on_initialize(
				0u32.into(),
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
				<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain,
				BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
				ThisChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
				LaneIdOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
			>(
				lane_id,
				xcm.into(),
				message_nonce,
				message_destination,
				para_header_number,
				relay_header_number,
				bridged_para_id,
				true,
			);

			let parachain_head_hash = parachain_head.hash();
			let relay_chain_header_hash = relay_chain_header.hash();
			let relay_chain_header_number = *relay_chain_header.number();
			vec![
				(
					BridgeGrandpaCall::<RuntimeHelper::Runtime, RuntimeHelper::GPI>::submit_finality_proof {
						finality_target: Box::new(relay_chain_header),
						justification: grandpa_justification,
					}.into(),
					Box::new((
						helpers::VerifySubmitGrandpaFinalityProofOutcome::<RuntimeHelper::Runtime, RuntimeHelper::GPI>::expect_best_header_hash(
							relay_chain_header_hash,
						),
						helpers::VerifyRelayerBalance::<RuntimeHelper::Runtime>::expect_relayer_balance(
							relayer_id_at_this_chain.clone(),
							initial_relayer_balance,
						),
					)),
				),
				(
					BridgeParachainsCall::<RuntimeHelper::Runtime, RuntimeHelper::PPI>::submit_parachain_heads {
						at_relay_block: (relay_chain_header_number, relay_chain_header_hash),
						parachains: parachain_heads,
						parachain_heads_proof: para_heads_proof,
					}.into(),
					Box::new((
						helpers::VerifySubmitParachainHeaderProofOutcome::<RuntimeHelper::Runtime, RuntimeHelper::PPI>::expect_best_header_hash(
							bridged_para_id,
							parachain_head_hash,
						),
						helpers::VerifyRelayerBalance::<RuntimeHelper::Runtime>::expect_relayer_balance(
							relayer_id_at_this_chain.clone(),
							initial_relayer_balance,
						),
					)),
				),
				(
					BridgeMessagesCall::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::receive_messages_proof {
						relayer_id_at_bridged_chain,
						proof: Box::new(message_proof),
						messages_count: 1,
						dispatch_weight: Weight::from_parts(1000000000, 0),
					}.into(),
					Box::new((
						helpers::VerifySubmitMessagesProofOutcome::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::expect_last_delivered_nonce(
							lane_id,
							1,
						),
						if expect_rewards {
                            helpers::VerifyRelayerRewarded::<RuntimeHelper::Runtime, RuntimeHelper::RPI>::expect_relayer_reward(
                                relayer_id_at_this_chain,
                                RewardsAccountParams::new(
                                    lane_id,
                                    bridged_chain_id,
                                    RewardsAccountOwner::ThisChain,
                                ),
                            )
						} else {
							Box::new(())
						}
					)),
				),
			]
		},
	);
}

/// Test-case makes sure that Runtime can dispatch XCM messages submitted by relayer,
/// with proofs (finality, para heads, message) batched together in signed extrinsic.
/// Also verifies relayer transaction signed extensions work as intended.
pub fn complex_relay_extrinsic_works<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	slot_durations: SlotDurations,
	runtime_para_id: u32,
	bridged_para_id: u32,
	sibling_parachain_id: u32,
	local_relay_chain_id: NetworkId,
	prepare_configuration: impl Fn() -> LaneIdOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
	construct_and_apply_extrinsic: fn(
		sp_keyring::Sr25519Keyring,
		<RuntimeHelper::Runtime as frame_system::Config>::RuntimeCall,
	) -> sp_runtime::DispatchOutcome,
) where
	RuntimeHelper: WithRemoteParachainHelper,
	RuntimeHelper::Runtime:
		pallet_utility::Config<RuntimeCall = RuntimeCallOf<RuntimeHelper::Runtime>>,
	AccountIdOf<RuntimeHelper::Runtime>: From<AccountId32>,
	RuntimeCallOf<RuntimeHelper::Runtime>: From<BridgeGrandpaCall<RuntimeHelper::Runtime, RuntimeHelper::GPI>>
		+ From<BridgeParachainsCall<RuntimeHelper::Runtime, RuntimeHelper::PPI>>
		+ From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>
		+ From<pallet_utility::Call<RuntimeHelper::Runtime>>,
	BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>: Chain<Hash = ParaHash> + Parachain,
	<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain:
		bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber> + ChainWithGrandpa,
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
		 xcm,
		 bridged_chain_id| {
			let para_header_number = 5;
			let relay_header_number = 1;

			let lane_id = prepare_configuration();

			// start with bridged relay chain block#0
			helpers::initialize_bridge_grandpa_pallet::<RuntimeHelper::Runtime, RuntimeHelper::GPI>(
				test_data::initialization_data::<RuntimeHelper::Runtime, RuntimeHelper::GPI>(0),
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
				<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain,
				BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
				ThisChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
				LaneIdOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
			>(
				lane_id,
				xcm.into(),
				message_nonce,
				message_destination,
				para_header_number,
				relay_header_number,
				bridged_para_id,
				false,
			);

			let parachain_head_hash = parachain_head.hash();
			let relay_chain_header_hash = relay_chain_header.hash();
			let relay_chain_header_number = *relay_chain_header.number();
			vec![
				(
					pallet_utility::Call::<RuntimeHelper::Runtime>::batch_all {
						calls: vec![
						BridgeGrandpaCall::<RuntimeHelper::Runtime, RuntimeHelper::GPI>::submit_finality_proof {
							finality_target: Box::new(relay_chain_header),
							justification: grandpa_justification,
						}.into(),
						BridgeParachainsCall::<RuntimeHelper::Runtime, RuntimeHelper::PPI>::submit_parachain_heads {
							at_relay_block: (relay_chain_header_number, relay_chain_header_hash),
							parachains: parachain_heads,
							parachain_heads_proof: para_heads_proof,
						}.into(),
						BridgeMessagesCall::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::receive_messages_proof {
							relayer_id_at_bridged_chain,
							proof: Box::new(message_proof),
							messages_count: 1,
							dispatch_weight: Weight::from_parts(1000000000, 0),
						}.into(),
					],
					}
					.into(),
					Box::new(
						(
							helpers::VerifySubmitGrandpaFinalityProofOutcome::<
								RuntimeHelper::Runtime,
								RuntimeHelper::GPI,
							>::expect_best_header_hash(relay_chain_header_hash),
							helpers::VerifySubmitParachainHeaderProofOutcome::<
								RuntimeHelper::Runtime,
								RuntimeHelper::PPI,
							>::expect_best_header_hash(bridged_para_id, parachain_head_hash),
							helpers::VerifySubmitMessagesProofOutcome::<
								RuntimeHelper::Runtime,
								RuntimeHelper::MPI,
							>::expect_last_delivered_nonce(lane_id, 1),
							helpers::VerifyRelayerRewarded::<
								RuntimeHelper::Runtime,
								RuntimeHelper::RPI,
							>::expect_relayer_reward(
								relayer_id_at_this_chain,
								RewardsAccountParams::new(
									lane_id,
									bridged_chain_id,
									RewardsAccountOwner::ThisChain,
								),
							),
						),
					),
				),
			]
		},
	);
}

/// Estimates transaction fee for default message delivery transaction (batched with required
/// proofs) from bridged parachain.
pub fn can_calculate_fee_for_complex_message_delivery_transaction<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	compute_extrinsic_fee: fn(pallet_utility::Call<RuntimeHelper::Runtime>) -> u128,
) -> u128
where
	RuntimeHelper: WithRemoteParachainHelper,
	RuntimeHelper::Runtime:
		pallet_utility::Config<RuntimeCall = RuntimeCallOf<RuntimeHelper::Runtime>>,
	RuntimeCallOf<RuntimeHelper::Runtime>: From<BridgeGrandpaCall<RuntimeHelper::Runtime, RuntimeHelper::GPI>>
		+ From<BridgeParachainsCall<RuntimeHelper::Runtime, RuntimeHelper::PPI>>
		+ From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>,
	BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>: Chain<Hash = ParaHash> + Parachain,
	<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain:
		bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber> + ChainWithGrandpa,
{
	run_test::<RuntimeHelper::Runtime, _>(collator_session_key, 1000, vec![], || {
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
			<RuntimeHelper::Runtime as pallet_bridge_grandpa::Config<RuntimeHelper::GPI>>::BridgedChain,
			BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
			ThisChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
			LaneIdOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>
		>(
			LaneIdOf::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::default(),
			vec![Instruction::<()>::ClearOrigin; 1_024].into(),
			1,
			[GlobalConsensus(Polkadot), Parachain(1_000)].into(),
			1,
			5,
			1_000,
			false,
		);

		// generate batch call that provides finality for bridged relay and parachains + message
		// proof
		let batch = test_data::from_parachain::make_complex_relayer_delivery_batch::<
			RuntimeHelper::Runtime,
			RuntimeHelper::GPI,
			RuntimeHelper::PPI,
			RuntimeHelper::MPI,
		>(
			relay_chain_header,
			grandpa_justification,
			parachain_heads,
			para_heads_proof,
			message_proof,
			helpers::relayer_id_at_bridged_chain::<RuntimeHelper::Runtime, RuntimeHelper::MPI>(),
		);

		compute_extrinsic_fee(batch)
	})
}

/// Estimates transaction fee for default message confirmation transaction (batched with required
/// proofs) from bridged parachain.
pub fn can_calculate_fee_for_complex_message_confirmation_transaction<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	compute_extrinsic_fee: fn(pallet_utility::Call<RuntimeHelper::Runtime>) -> u128,
) -> u128
where
	RuntimeHelper: WithRemoteParachainHelper,
	AccountIdOf<RuntimeHelper::Runtime>: From<AccountId32>,
	RuntimeHelper::Runtime:
		pallet_utility::Config<RuntimeCall = RuntimeCallOf<RuntimeHelper::Runtime>>,
	ThisChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>:
		Chain<AccountId = AccountIdOf<RuntimeHelper::Runtime>>,
	RuntimeCallOf<RuntimeHelper::Runtime>: From<BridgeGrandpaCall<RuntimeHelper::Runtime, RuntimeHelper::GPI>>
		+ From<BridgeParachainsCall<RuntimeHelper::Runtime, RuntimeHelper::PPI>>
		+ From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>,
	BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>: Chain<Hash = ParaHash> + Parachain,
	<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain:
		bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber> + ChainWithGrandpa,
{
	run_test::<RuntimeHelper::Runtime, _>(collator_session_key, 1000, vec![], || {
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
			<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain,
			BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
			ThisChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
			LaneIdOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
		>(
			LaneIdOf::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::default(),
			1,
			5,
			1_000,
			AccountId32::from(Alice.public()).into(),
			unrewarded_relayers.clone(),
		);

		// generate batch call that provides finality for bridged relay and parachains + message
		// proof
		let batch = test_data::from_parachain::make_complex_relayer_confirmation_batch::<
			RuntimeHelper::Runtime,
			RuntimeHelper::GPI,
			RuntimeHelper::PPI,
			RuntimeHelper::MPI,
		>(
			relay_chain_header,
			grandpa_justification,
			parachain_heads,
			para_heads_proof,
			message_delivery_proof,
			unrewarded_relayers,
		);

		compute_extrinsic_fee(batch)
	})
}

/// Estimates transaction fee for default message delivery transaction from bridged parachain.
pub fn can_calculate_fee_for_standalone_message_delivery_transaction<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	compute_extrinsic_fee: fn(
		<RuntimeHelper::Runtime as frame_system::Config>::RuntimeCall,
	) -> u128,
) -> u128
where
	RuntimeHelper: WithRemoteParachainHelper,
	RuntimeCallOf<RuntimeHelper::Runtime>:
		From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>,
	BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>: Chain<Hash = ParaHash> + Parachain,
	<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain:
		bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber> + ChainWithGrandpa,
{
	run_test::<RuntimeHelper::Runtime, _>(collator_session_key, 1000, vec![], || {
		// generate bridged relay chain finality, parachain heads and message proofs,
		// to be submitted by relayer to this chain.
		//
		// we don't care about parameter values here, apart from the XCM message size. But we
		// do not need to have a large message here, because we're charging for every byte of
		// the message additionally
		let (
			_,
			_,
			_,
			_,
			_,
			message_proof,
		) = test_data::from_parachain::make_complex_relayer_delivery_proofs::<
			<RuntimeHelper::Runtime as pallet_bridge_grandpa::Config<RuntimeHelper::GPI>>::BridgedChain,
			BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
			ThisChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
			LaneIdOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
		>(
			LaneIdOf::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::default(),
			vec![Instruction::<()>::ClearOrigin; 1_024].into(),
			1,
			[GlobalConsensus(Polkadot), Parachain(1_000)].into(),
			1,
			5,
			1_000,
			false,
		);

		let call = test_data::from_parachain::make_standalone_relayer_delivery_call::<
			RuntimeHelper::Runtime,
			RuntimeHelper::MPI,
		>(
			message_proof,
			helpers::relayer_id_at_bridged_chain::<RuntimeHelper::Runtime, RuntimeHelper::MPI>(),
		);

		compute_extrinsic_fee(call)
	})
}

/// Estimates transaction fee for default message confirmation transaction (batched with required
/// proofs) from bridged parachain.
pub fn can_calculate_fee_for_standalone_message_confirmation_transaction<RuntimeHelper>(
	collator_session_key: CollatorSessionKeys<RuntimeHelper::Runtime>,
	compute_extrinsic_fee: fn(
		<RuntimeHelper::Runtime as frame_system::Config>::RuntimeCall,
	) -> u128,
) -> u128
where
	RuntimeHelper: WithRemoteParachainHelper,
	AccountIdOf<RuntimeHelper::Runtime>: From<AccountId32>,
	ThisChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>:
		Chain<AccountId = AccountIdOf<RuntimeHelper::Runtime>>,
	RuntimeCallOf<RuntimeHelper::Runtime>:
		From<BridgeMessagesCall<RuntimeHelper::Runtime, RuntimeHelper::MPI>>,
	BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>: Chain<Hash = ParaHash> + Parachain,
	<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain:
		bp_runtime::Chain<Hash = RelayBlockHash, BlockNumber = RelayBlockNumber> + ChainWithGrandpa,
{
	run_test::<RuntimeHelper::Runtime, _>(collator_session_key, 1000, vec![], || {
		// generate bridged relay chain finality, parachain heads and message proofs,
		// to be submitted by relayer to this chain.
		let unrewarded_relayers = UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			total_messages: 1,
			..Default::default()
		};
		let (_, _, _, _, _, message_delivery_proof) =
			test_data::from_parachain::make_complex_relayer_confirmation_proofs::<
				<RuntimeHelper::Runtime as BridgeGrandpaConfig<RuntimeHelper::GPI>>::BridgedChain,
				BridgedChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
				ThisChainOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
				LaneIdOf<RuntimeHelper::Runtime, RuntimeHelper::MPI>,
			>(
				LaneIdOf::<RuntimeHelper::Runtime, RuntimeHelper::MPI>::default(),
				1,
				5,
				1_000,
				AccountId32::from(Alice.public()).into(),
				unrewarded_relayers.clone(),
			);

		let call = test_data::from_parachain::make_standalone_relayer_confirmation_call::<
			RuntimeHelper::Runtime,
			RuntimeHelper::MPI,
		>(message_delivery_proof, unrewarded_relayers);

		compute_extrinsic_fee(call)
	})
}
