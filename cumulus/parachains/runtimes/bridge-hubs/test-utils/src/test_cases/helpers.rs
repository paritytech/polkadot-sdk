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

//! Module contains tests code, that is shared by all types of bridges

use crate::test_cases::{bridges_prelude::*, run_test, RuntimeHelper};

use asset_test_utils::BasicParachainRuntime;
use bp_messages::MessageNonce;
use bp_polkadot_core::parachains::{ParaHash, ParaId};
use bp_runtime::Chain;
use codec::Decode;
use core::marker::PhantomData;
use frame_support::{
	assert_ok,
	dispatch::GetDispatchInfo,
	traits::{fungible::Mutate, Contains, OnFinalize, OnInitialize, PalletInfoAccess},
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_bridge_grandpa::{BridgedBlockHash, BridgedHeader};
use pallet_bridge_messages::{BridgedChainOf, LaneIdOf};
use parachains_common::AccountId;
use parachains_runtimes_test_utils::{
	mock_open_hrmp_channel, AccountIdOf, CollatorSessionKeys, RuntimeCallOf, SlotDurations,
};
use sp_core::Get;
use sp_keyring::Sr25519Keyring::*;
use sp_runtime::{traits::TrailingZeroInput, AccountId32};
use xcm::latest::prelude::*;
use xcm_executor::traits::ConvertLocation;

/// Verify that the transaction has succeeded.
#[impl_trait_for_tuples::impl_for_tuples(30)]
pub trait VerifyTransactionOutcome {
	fn verify_outcome(&self);
}

impl VerifyTransactionOutcome for Box<dyn VerifyTransactionOutcome> {
	fn verify_outcome(&self) {
		VerifyTransactionOutcome::verify_outcome(&**self)
	}
}

/// Checks that the best finalized header hash in the bridge GRANDPA pallet equals to given one.
pub struct VerifySubmitGrandpaFinalityProofOutcome<Runtime, GPI>
where
	Runtime: BridgeGrandpaConfig<GPI>,
	GPI: 'static,
{
	expected_best_hash: BridgedBlockHash<Runtime, GPI>,
}

impl<Runtime, GPI> VerifySubmitGrandpaFinalityProofOutcome<Runtime, GPI>
where
	Runtime: BridgeGrandpaConfig<GPI>,
	GPI: 'static,
{
	/// Expect given header hash to be the best after transaction.
	pub fn expect_best_header_hash(
		expected_best_hash: BridgedBlockHash<Runtime, GPI>,
	) -> Box<dyn VerifyTransactionOutcome> {
		Box::new(Self { expected_best_hash })
	}
}

impl<Runtime, GPI> VerifyTransactionOutcome
	for VerifySubmitGrandpaFinalityProofOutcome<Runtime, GPI>
where
	Runtime: BridgeGrandpaConfig<GPI>,
	GPI: 'static,
{
	fn verify_outcome(&self) {
		assert_eq!(
			pallet_bridge_grandpa::BestFinalized::<Runtime, GPI>::get().unwrap().1,
			self.expected_best_hash
		);
		assert!(pallet_bridge_grandpa::ImportedHeaders::<Runtime, GPI>::contains_key(
			self.expected_best_hash
		));
	}
}

/// Checks that the best parachain header hash in the bridge parachains pallet equals to given one.
pub struct VerifySubmitParachainHeaderProofOutcome<Runtime, PPI> {
	bridged_para_id: u32,
	expected_best_hash: ParaHash,
	_marker: PhantomData<(Runtime, PPI)>,
}

impl<Runtime, PPI> VerifySubmitParachainHeaderProofOutcome<Runtime, PPI>
where
	Runtime: BridgeParachainsConfig<PPI>,
	PPI: 'static,
{
	/// Expect given header hash to be the best after transaction.
	pub fn expect_best_header_hash(
		bridged_para_id: u32,
		expected_best_hash: ParaHash,
	) -> Box<dyn VerifyTransactionOutcome> {
		Box::new(Self { bridged_para_id, expected_best_hash, _marker: PhantomData })
	}
}

impl<Runtime, PPI> VerifyTransactionOutcome
	for VerifySubmitParachainHeaderProofOutcome<Runtime, PPI>
where
	Runtime: BridgeParachainsConfig<PPI>,
	PPI: 'static,
{
	fn verify_outcome(&self) {
		assert_eq!(
			pallet_bridge_parachains::ParasInfo::<Runtime, PPI>::get(ParaId(self.bridged_para_id))
				.map(|info| info.best_head_hash.head_hash),
			Some(self.expected_best_hash),
		);
	}
}

/// Checks that the latest delivered nonce in the bridge messages pallet equals to given one.
pub struct VerifySubmitMessagesProofOutcome<Runtime: BridgeMessagesConfig<MPI>, MPI: 'static> {
	lane: LaneIdOf<Runtime, MPI>,
	expected_nonce: MessageNonce,
	_marker: PhantomData<(Runtime, MPI)>,
}

impl<Runtime, MPI> VerifySubmitMessagesProofOutcome<Runtime, MPI>
where
	Runtime: BridgeMessagesConfig<MPI>,
	MPI: 'static,
{
	/// Expect given delivered nonce to be the latest after transaction.
	pub fn expect_last_delivered_nonce(
		lane: LaneIdOf<Runtime, MPI>,
		expected_nonce: MessageNonce,
	) -> Box<dyn VerifyTransactionOutcome> {
		Box::new(Self { lane, expected_nonce, _marker: PhantomData })
	}
}

impl<Runtime, MPI> VerifyTransactionOutcome for VerifySubmitMessagesProofOutcome<Runtime, MPI>
where
	Runtime: BridgeMessagesConfig<MPI>,
	MPI: 'static,
{
	fn verify_outcome(&self) {
		assert_eq!(
			pallet_bridge_messages::InboundLanes::<Runtime, MPI>::get(self.lane)
				.map(|d| d.last_delivered_nonce()),
			Some(self.expected_nonce),
		);
	}
}

/// Verifies that relayer is rewarded at this chain.
pub struct VerifyRelayerRewarded<Runtime: pallet_bridge_relayers::Config<RPI>, RPI: 'static> {
	relayer: Runtime::AccountId,
	reward_params: Runtime::Reward,
}

impl<Runtime, RPI> VerifyRelayerRewarded<Runtime, RPI>
where
	Runtime: pallet_bridge_relayers::Config<RPI>,
	RPI: 'static,
{
	/// Expect given delivered nonce to be the latest after transaction.
	pub fn expect_relayer_reward(
		relayer: Runtime::AccountId,
		reward_params: impl Into<Runtime::Reward>,
	) -> Box<dyn VerifyTransactionOutcome> {
		Box::new(Self { relayer, reward_params: reward_params.into() })
	}
}

impl<Runtime, RPI> VerifyTransactionOutcome for VerifyRelayerRewarded<Runtime, RPI>
where
	Runtime: pallet_bridge_relayers::Config<RPI>,
	RPI: 'static,
{
	fn verify_outcome(&self) {
		assert!(pallet_bridge_relayers::RelayerRewards::<Runtime, RPI>::get(
			&self.relayer,
			&self.reward_params,
		)
		.is_some());
	}
}

/// Verifies that relayer balance is equal to given value.
pub struct VerifyRelayerBalance<Runtime: pallet_balances::Config> {
	relayer: Runtime::AccountId,
	balance: Runtime::Balance,
}

impl<Runtime> VerifyRelayerBalance<Runtime>
where
	Runtime: pallet_balances::Config,
{
	/// Expect given relayer balance after transaction.
	pub fn expect_relayer_balance(
		relayer: Runtime::AccountId,
		balance: Runtime::Balance,
	) -> Box<dyn VerifyTransactionOutcome> {
		Box::new(Self { relayer, balance })
	}
}

impl<Runtime> VerifyTransactionOutcome for VerifyRelayerBalance<Runtime>
where
	Runtime: pallet_balances::Config,
{
	fn verify_outcome(&self) {
		assert_eq!(pallet_balances::Pallet::<Runtime>::free_balance(&self.relayer), self.balance,);
	}
}

/// Initialize bridge GRANDPA pallet.
pub(crate) fn initialize_bridge_grandpa_pallet<Runtime, GPI>(
	init_data: bp_header_chain::InitializationData<BridgedHeader<Runtime, GPI>>,
) where
	Runtime: BridgeGrandpaConfig<GPI>
		+ cumulus_pallet_parachain_system::Config
		+ pallet_timestamp::Config,
{
	pallet_bridge_grandpa::Pallet::<Runtime, GPI>::initialize(
		RuntimeHelper::<Runtime>::root_origin(),
		init_data,
	)
	.unwrap();
}

/// Runtime calls and their verifiers.
pub type CallsAndVerifiers<Runtime> =
	Vec<(RuntimeCallOf<Runtime>, Box<dyn VerifyTransactionOutcome>)>;

pub type InboundRelayerId<Runtime, MPI> = bp_runtime::AccountIdOf<BridgedChainOf<Runtime, MPI>>;

/// Returns relayer id at the bridged chain.
pub fn relayer_id_at_bridged_chain<Runtime: pallet_bridge_messages::Config<MPI>, MPI>(
) -> InboundRelayerId<Runtime, MPI> {
	Decode::decode(&mut TrailingZeroInput::zeroes()).unwrap()
}

/// Test-case makes sure that Runtime can dispatch XCM messages submitted by relayer,
/// with proofs (finality, message) independently submitted.
pub fn relayed_incoming_message_works<Runtime, AllPalletsWithoutSystem, MPI>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	slot_durations: SlotDurations,
	runtime_para_id: u32,
	sibling_parachain_id: u32,
	local_relay_chain_id: NetworkId,
	construct_and_apply_extrinsic: fn(
		sp_keyring::Sr25519Keyring,
		RuntimeCallOf<Runtime>,
	) -> sp_runtime::DispatchOutcome,
	prepare_message_proof_import: impl FnOnce(
		Runtime::AccountId,
		InboundRelayerId<Runtime, MPI>,
		InteriorLocation,
		MessageNonce,
		Xcm<()>,
		bp_runtime::ChainId,
	) -> CallsAndVerifiers<Runtime>,
) where
	Runtime: BasicParachainRuntime + cumulus_pallet_xcmp_queue::Config + BridgeMessagesConfig<MPI>,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	MPI: 'static,
	AccountIdOf<Runtime>: From<AccountId32>,
{
	let relayer_at_target = Bob;
	let relayer_id_on_target: AccountId32 = relayer_at_target.public().into();
	let relayer_id_on_source = relayer_id_at_bridged_chain::<Runtime, MPI>();
	let bridged_chain_id = Runtime::BridgedChain::ID;

	assert_ne!(runtime_para_id, sibling_parachain_id);

	run_test::<Runtime, _>(
		collator_session_key,
		runtime_para_id,
		vec![(
			relayer_id_on_target.clone().into(),
			// this value should be enough to cover all transaction costs, but computing the actual
			// value here is tricky - there are several transaction payment pallets and we don't
			// want to introduce additional bounds and traits here just for that, so let's just
			// select some presumably large value
			core::cmp::max::<Runtime::Balance>(Runtime::ExistentialDeposit::get(), 1u32.into()) *
				100_000_000u32.into(),
		)],
		|| {
			let mut alice = [0u8; 32];
			alice[0] = 1;

			let included_head = RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::run_to_block(
				2,
				AccountId::from(alice).into(),
			);
			mock_open_hrmp_channel::<Runtime, cumulus_pallet_parachain_system::Pallet<Runtime>>(
				runtime_para_id.into(),
				sibling_parachain_id.into(),
				included_head,
				&alice,
				&slot_durations,
			);

			// set up relayer details and proofs

			let message_destination: InteriorLocation =
				[GlobalConsensus(local_relay_chain_id), Parachain(sibling_parachain_id)].into();
			// some random numbers (checked by test)
			let message_nonce = 1;

			let xcm = vec![Instruction::<()>::ClearOrigin; 42];
			let expected_dispatch = xcm::latest::Xcm::<()>({
				let mut expected_instructions = xcm.clone();
				// dispatch prepends bridge pallet instance
				expected_instructions.insert(
					0,
					DescendOrigin([PalletInstance(
						<pallet_bridge_messages::Pallet<Runtime, MPI> as PalletInfoAccess>::index()
							as u8,
					)].into()),
				);
				expected_instructions
			});

			execute_and_verify_calls::<Runtime>(
				relayer_at_target,
				construct_and_apply_extrinsic,
				prepare_message_proof_import(
					relayer_id_on_target.clone().into(),
					relayer_id_on_source.clone().into(),
					message_destination,
					message_nonce,
					xcm.clone().into(),
					bridged_chain_id,
				),
			);

			// verify that imported XCM contains original message
			let imported_xcm =
				RuntimeHelper::<cumulus_pallet_xcmp_queue::Pallet<Runtime>>::take_xcm(
					sibling_parachain_id.into(),
				)
				.unwrap();
			let dispatched = xcm::latest::Xcm::<()>::try_from(imported_xcm).unwrap();
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
		},
	)
}

/// Execute every call and verify its outcome.
fn execute_and_verify_calls<Runtime: frame_system::Config>(
	submitter: sp_keyring::Sr25519Keyring,
	construct_and_apply_extrinsic: fn(
		sp_keyring::Sr25519Keyring,
		RuntimeCallOf<Runtime>,
	) -> sp_runtime::DispatchOutcome,
	calls_and_verifiers: CallsAndVerifiers<Runtime>,
) {
	for (call, verifier) in calls_and_verifiers {
		let dispatch_outcome = construct_and_apply_extrinsic(submitter, call);
		assert_ok!(dispatch_outcome);
		verifier.verify_outcome();
	}
}

pub(crate) mod for_pallet_xcm_bridge_hub {
	use super::{super::for_pallet_xcm_bridge_hub::*, *};

	/// Helper function to open the bridge/lane for `source` and `destination` while ensuring all
	/// required balances are placed into the SA of the source.
	pub fn ensure_opened_bridge<
		Runtime,
		XcmOverBridgePalletInstance,
		LocationToAccountId,
		TokenLocation>
	(source: Location, destination: InteriorLocation, is_paid_xcm_execution: bool, bridge_opener: impl Fn(pallet_xcm_bridge_hub::BridgeLocations, Option<Asset>)) -> (pallet_xcm_bridge_hub::BridgeLocations, pallet_xcm_bridge_hub::LaneIdOf<Runtime, XcmOverBridgePalletInstance>)
	where
		Runtime: BasicParachainRuntime + BridgeXcmOverBridgeConfig<XcmOverBridgePalletInstance>,
		XcmOverBridgePalletInstance: 'static,
		<Runtime as frame_system::Config>::RuntimeCall: GetDispatchInfo + From<BridgeXcmOverBridgeCall<Runtime, XcmOverBridgePalletInstance>>,
		<Runtime as pallet_balances::Config>::Balance: From<<<Runtime as pallet_bridge_messages::Config<<Runtime as pallet_xcm_bridge_hub::Config<XcmOverBridgePalletInstance>>::BridgeMessagesPalletInstance>>::ThisChain as bp_runtime::Chain>::Balance>,
		<Runtime as pallet_balances::Config>::Balance: From<u128>,
		LocationToAccountId: ConvertLocation<AccountIdOf<Runtime>>,
		TokenLocation: Get<Location>
	{
		// construct expected bridge configuration
		let locations =
			pallet_xcm_bridge_hub::Pallet::<Runtime, XcmOverBridgePalletInstance>::bridge_locations(
				source.clone().into(),
				destination.clone().into(),
			)
				.expect("valid bridge locations");
		assert!(pallet_xcm_bridge_hub::Bridges::<Runtime, XcmOverBridgePalletInstance>::get(
			locations.bridge_id()
		)
		.is_none());

		// SA of source location needs to have some required balance
		if !<Runtime as pallet_xcm_bridge_hub::Config<XcmOverBridgePalletInstance>>::AllowWithoutBridgeDeposit::contains(&source) {
			// required balance: ED + fee + BridgeDeposit
			let bridge_deposit =
				<Runtime as pallet_xcm_bridge_hub::Config<XcmOverBridgePalletInstance>>::BridgeDeposit::get();
			let balance_needed = <Runtime as pallet_balances::Config>::ExistentialDeposit::get() + bridge_deposit.into();

			let source_account_id = LocationToAccountId::convert_location(&source).expect("valid location");
			let _ = <pallet_balances::Pallet<Runtime>>::mint_into(&source_account_id, balance_needed)
				.expect("mint_into passes");
		};

		let maybe_paid_execution = if is_paid_xcm_execution {
			// random high enough value for `BuyExecution` fees
			let buy_execution_fee_amount = 5_000_000_000_000_u128;
			let buy_execution_fee = (TokenLocation::get(), buy_execution_fee_amount).into();

			let balance_needed = <Runtime as pallet_balances::Config>::ExistentialDeposit::get() +
				buy_execution_fee_amount.into();
			let source_account_id =
				LocationToAccountId::convert_location(&source).expect("valid location");
			let _ =
				<pallet_balances::Pallet<Runtime>>::mint_into(&source_account_id, balance_needed)
					.expect("mint_into passes");
			Some(buy_execution_fee)
		} else {
			None
		};

		// call the bridge opener
		bridge_opener(*locations.clone(), maybe_paid_execution);

		// check opened bridge
		let bridge = pallet_xcm_bridge_hub::Bridges::<Runtime, XcmOverBridgePalletInstance>::get(
			locations.bridge_id(),
		)
		.expect("opened bridge");

		// check state
		assert_ok!(
			pallet_xcm_bridge_hub::Pallet::<Runtime, XcmOverBridgePalletInstance>::do_try_state()
		);

		// return locations
		(*locations, bridge.lane_id)
	}

	/// Utility for opening bridge with dedicated `pallet_xcm_bridge_hub`'s extrinsic.
	pub fn open_bridge_with_extrinsic<Runtime, XcmOverBridgePalletInstance>(
		(origin, origin_kind): (Location, OriginKind),
		bridge_destination_universal_location: InteriorLocation,
		maybe_paid_execution: Option<Asset>,
	) where
		Runtime: frame_system::Config
			+ pallet_xcm_bridge_hub::Config<XcmOverBridgePalletInstance>
			+ cumulus_pallet_parachain_system::Config
			+ pallet_xcm::Config,
		XcmOverBridgePalletInstance: 'static,
		<Runtime as frame_system::Config>::RuntimeCall:
			GetDispatchInfo + From<BridgeXcmOverBridgeCall<Runtime, XcmOverBridgePalletInstance>>,
	{
		// open bridge with `Transact` call
		let open_bridge_call = RuntimeCallOf::<Runtime>::from(BridgeXcmOverBridgeCall::<
			Runtime,
			XcmOverBridgePalletInstance,
		>::open_bridge {
			bridge_destination_universal_location: Box::new(
				bridge_destination_universal_location.clone().into(),
			),
		});

		// execute XCM as source origin would do with `Transact -> Origin::Xcm`
		assert_ok!(RuntimeHelper::<Runtime>::execute_as_origin(
			(origin, origin_kind),
			open_bridge_call,
			maybe_paid_execution
		)
		.ensure_complete());
	}

	/// Utility for opening bridge directly inserting data to the `pallet_xcm_bridge_hub`'s storage
	/// (used only for legacy purposes).
	pub fn open_bridge_with_storage<Runtime, XcmOverBridgePalletInstance>(
		locations: pallet_xcm_bridge_hub::BridgeLocations,
		lane_id: pallet_xcm_bridge_hub::LaneIdOf<Runtime, XcmOverBridgePalletInstance>,
	) where
		Runtime: pallet_xcm_bridge_hub::Config<XcmOverBridgePalletInstance>,
		XcmOverBridgePalletInstance: 'static,
	{
		// insert bridge data directly to the storage
		assert_ok!(
			pallet_xcm_bridge_hub::Pallet::<Runtime, XcmOverBridgePalletInstance>::do_open_bridge(
				Box::new(locations),
				lane_id,
				true
			)
		);
	}

	/// Helper function to close the bridge/lane for `source` and `destination`.
	pub fn close_bridge<Runtime, XcmOverBridgePalletInstance, LocationToAccountId, TokenLocation>(
		expected_source: Location,
		bridge_destination_universal_location: InteriorLocation,
		(origin, origin_kind): (Location, OriginKind),
		is_paid_xcm_execution: bool
	) where
		Runtime: BasicParachainRuntime + BridgeXcmOverBridgeConfig<XcmOverBridgePalletInstance>,
		XcmOverBridgePalletInstance: 'static,
		<Runtime as frame_system::Config>::RuntimeCall: GetDispatchInfo + From<BridgeXcmOverBridgeCall<Runtime, XcmOverBridgePalletInstance>>,
		<Runtime as pallet_balances::Config>::Balance: From<<<Runtime as pallet_bridge_messages::Config<<Runtime as pallet_xcm_bridge_hub::Config<XcmOverBridgePalletInstance>>::BridgeMessagesPalletInstance>>::ThisChain as bp_runtime::Chain>::Balance>,
		<Runtime as pallet_balances::Config>::Balance: From<u128>,
		LocationToAccountId: ConvertLocation<AccountIdOf<Runtime>>,
		TokenLocation: Get<Location>
	{
		// construct expected bridge configuration
		let locations =
			pallet_xcm_bridge_hub::Pallet::<Runtime, XcmOverBridgePalletInstance>::bridge_locations(
				expected_source.clone().into(),
				bridge_destination_universal_location.clone().into(),
			)
				.expect("valid bridge locations");
		assert!(pallet_xcm_bridge_hub::Bridges::<Runtime, XcmOverBridgePalletInstance>::get(
			locations.bridge_id()
		)
		.is_some());

		// required balance: ED + fee + BridgeDeposit
		let maybe_paid_execution = if is_paid_xcm_execution {
			// random high enough value for `BuyExecution` fees
			let buy_execution_fee_amount = 2_500_000_000_000_u128;
			let buy_execution_fee = (TokenLocation::get(), buy_execution_fee_amount).into();

			let balance_needed = <Runtime as pallet_balances::Config>::ExistentialDeposit::get() +
				buy_execution_fee_amount.into();
			let source_account_id =
				LocationToAccountId::convert_location(&expected_source).expect("valid location");
			let _ =
				<pallet_balances::Pallet<Runtime>>::mint_into(&source_account_id, balance_needed)
					.expect("mint_into passes");
			Some(buy_execution_fee)
		} else {
			None
		};

		// close bridge with `Transact` call
		let close_bridge_call = RuntimeCallOf::<Runtime>::from(BridgeXcmOverBridgeCall::<
			Runtime,
			XcmOverBridgePalletInstance,
		>::close_bridge {
			bridge_destination_universal_location: Box::new(
				bridge_destination_universal_location.into(),
			),
			may_prune_messages: 16,
		});

		// execute XCM as source origin would do with `Transact -> Origin::Xcm`
		assert_ok!(RuntimeHelper::<Runtime>::execute_as_origin(
			(origin, origin_kind),
			close_bridge_call,
			maybe_paid_execution
		)
		.ensure_complete());

		// bridge is closed
		assert!(pallet_xcm_bridge_hub::Bridges::<Runtime, XcmOverBridgePalletInstance>::get(
			locations.bridge_id()
		)
		.is_none());

		// check state
		assert_ok!(
			pallet_xcm_bridge_hub::Pallet::<Runtime, XcmOverBridgePalletInstance>::do_try_state()
		);
	}
}
