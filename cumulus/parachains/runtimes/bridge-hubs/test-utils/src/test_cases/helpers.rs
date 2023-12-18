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

//! Module contains tests code, that is shared by all types of bridges

use crate::test_cases::{run_test, RuntimeHelper};

use asset_test_utils::BasicParachainRuntime;
use bp_messages::{LaneId, MessageNonce};
use bp_polkadot_core::parachains::{ParaHash, ParaId};
use bp_relayers::RewardsAccountParams;
use frame_support::{
	assert_ok,
	traits::{OnFinalize, OnInitialize, PalletInfoAccess},
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_bridge_grandpa::{BridgedBlockHash, BridgedHeader};
use parachains_common::AccountId;
use parachains_runtimes_test_utils::{
	mock_open_hrmp_channel, AccountIdOf, CollatorSessionKeys, ValidatorIdOf,
};
use sp_core::Get;
use sp_keyring::AccountKeyring::*;
use sp_runtime::AccountId32;
use sp_std::marker::PhantomData;
use xcm::latest::prelude::*;

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
	Runtime: pallet_bridge_grandpa::Config<GPI>,
	GPI: 'static,
{
	expected_best_hash: BridgedBlockHash<Runtime, GPI>,
}

impl<Runtime, GPI> VerifySubmitGrandpaFinalityProofOutcome<Runtime, GPI>
where
	Runtime: pallet_bridge_grandpa::Config<GPI>,
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
	Runtime: pallet_bridge_grandpa::Config<GPI>,
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
	Runtime: pallet_bridge_parachains::Config<PPI>,
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
	Runtime: pallet_bridge_parachains::Config<PPI>,
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
pub struct VerifySubmitMessagesProofOutcome<Runtime, MPI> {
	lane: LaneId,
	expected_nonce: MessageNonce,
	_marker: PhantomData<(Runtime, MPI)>,
}

impl<Runtime, MPI> VerifySubmitMessagesProofOutcome<Runtime, MPI>
where
	Runtime: pallet_bridge_messages::Config<MPI>,
	MPI: 'static,
{
	/// Expect given delivered nonce to be the latest after transaction.
	pub fn expect_last_delivered_nonce(
		lane: LaneId,
		expected_nonce: MessageNonce,
	) -> Box<dyn VerifyTransactionOutcome> {
		Box::new(Self { lane, expected_nonce, _marker: PhantomData })
	}
}

impl<Runtime, MPI> VerifyTransactionOutcome for VerifySubmitMessagesProofOutcome<Runtime, MPI>
where
	Runtime: pallet_bridge_messages::Config<MPI>,
	MPI: 'static,
{
	fn verify_outcome(&self) {
		assert_eq!(
			pallet_bridge_messages::InboundLanes::<Runtime, MPI>::get(self.lane)
				.last_delivered_nonce(),
			self.expected_nonce,
		);
	}
}

/// Verifies that relayer is rewarded at this chain.
pub struct VerifyRelayerRewarded<Runtime: frame_system::Config> {
	relayer: Runtime::AccountId,
	reward_params: RewardsAccountParams,
}

impl<Runtime> VerifyRelayerRewarded<Runtime>
where
	Runtime: pallet_bridge_relayers::Config,
{
	/// Expect given delivered nonce to be the latest after transaction.
	pub fn expect_relayer_reward(
		relayer: Runtime::AccountId,
		reward_params: RewardsAccountParams,
	) -> Box<dyn VerifyTransactionOutcome> {
		Box::new(Self { relayer, reward_params })
	}
}

impl<Runtime> VerifyTransactionOutcome for VerifyRelayerRewarded<Runtime>
where
	Runtime: pallet_bridge_relayers::Config,
{
	fn verify_outcome(&self) {
		assert!(pallet_bridge_relayers::RelayerRewards::<Runtime>::get(
			&self.relayer,
			&self.reward_params,
		)
		.is_some());
	}
}

/// Initialize bridge GRANDPA pallet.
pub(crate) fn initialize_bridge_grandpa_pallet<Runtime, GPI>(
	init_data: bp_header_chain::InitializationData<BridgedHeader<Runtime, GPI>>,
) where
	Runtime: pallet_bridge_grandpa::Config<GPI>,
{
	pallet_bridge_grandpa::Pallet::<Runtime, GPI>::initialize(
		RuntimeHelper::<Runtime>::root_origin(),
		init_data,
	)
	.unwrap();
}

/// Runtime calls and their verifiers.
pub type CallsAndVerifiers<Runtime> =
	Vec<(<Runtime as frame_system::Config>::RuntimeCall, Box<dyn VerifyTransactionOutcome>)>;

/// Test-case makes sure that Runtime can dispatch XCM messages submitted by relayer,
/// with proofs (finality, message) independently submitted.
pub fn relayed_incoming_message_works<Runtime, AllPalletsWithoutSystem, HrmpChannelOpener, MPI>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	sibling_parachain_id: u32,
	local_relay_chain_id: NetworkId,
	construct_and_apply_extrinsic: fn(
		sp_keyring::AccountKeyring,
		<Runtime as frame_system::Config>::RuntimeCall,
	) -> sp_runtime::DispatchOutcome,
	prepare_message_proof_import: impl FnOnce(
		Runtime::AccountId,
		Runtime::InboundRelayer,
		InteriorMultiLocation,
		MessageNonce,
		Xcm<()>,
	) -> CallsAndVerifiers<Runtime>,
) where
	Runtime: BasicParachainRuntime
		+ cumulus_pallet_xcmp_queue::Config
		+ pallet_bridge_messages::Config<MPI>,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	HrmpChannelOpener: frame_support::inherent::ProvideInherent<
		Call = cumulus_pallet_parachain_system::Call<Runtime>,
	>,
	MPI: 'static,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	AccountIdOf<Runtime>: From<AccountId32> + From<sp_core::sr25519::Public>,
	<Runtime as pallet_bridge_messages::Config<MPI>>::InboundRelayer: From<AccountId32>,
{
	let relayer_at_target = Bob;
	let relayer_id_on_target: AccountId32 = relayer_at_target.public().into();
	let relayer_at_source = Dave;
	let relayer_id_on_source: AccountId32 = relayer_at_source.public().into();

	assert_ne!(runtime_para_id, sibling_parachain_id);

	run_test::<Runtime, _>(
		collator_session_key,
		runtime_para_id,
		vec![(
			relayer_id_on_target.clone().into(),
			Runtime::ExistentialDeposit::get() * 100000u32.into(),
		)],
		|| {
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

			// set up relayer details and proofs

			let message_destination =
				X2(GlobalConsensus(local_relay_chain_id), Parachain(sibling_parachain_id));
			// some random numbers (checked by test)
			let message_nonce = 1;

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

			execute_and_verify_calls::<Runtime>(
				relayer_at_target,
				construct_and_apply_extrinsic,
				prepare_message_proof_import(
					relayer_id_on_target.clone().into(),
					relayer_id_on_source.clone().into(),
					message_destination,
					message_nonce,
					xcm.clone().into(),
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
	submitter: sp_keyring::AccountKeyring,
	construct_and_apply_extrinsic: fn(
		sp_keyring::AccountKeyring,
		<Runtime as frame_system::Config>::RuntimeCall,
	) -> sp_runtime::DispatchOutcome,
	calls_and_verifiers: CallsAndVerifiers<Runtime>,
) {
	for (call, verifier) in calls_and_verifiers {
		let dispatch_outcome = construct_and_apply_extrinsic(submitter, call);
		assert_ok!(dispatch_outcome);
		verifier.verify_outcome();
	}
}
