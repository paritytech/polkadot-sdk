// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use super::{mock::*, *};
use crate::{
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
	},
	messages_call_ext::{
		BaseMessagesProofInfo, ReceiveMessagesDeliveryProofInfo, ReceiveMessagesProofInfo,
		UnrewardedRelayerOccupation,
	},
};
use bp_messages::{
	DeliveredMessages, InboundLaneData, MessageNonce, MessagesOperatingMode, OutboundLaneData,
	UnrewardedRelayer, UnrewardedRelayersState,
};
use bp_parachains::{BestParaHeadHash, ParaInfo};
use bp_polkadot_core::parachains::{ParaHeadsProof, ParaId};
use bp_runtime::{BasicOperatingMode, HeaderId};
use bp_test_utils::{make_default_justification, test_keyring};
use frame_support::{
	assert_storage_noop,
	traits::{fungible::Mutate, ReservableCurrency},
	weights::Weight,
};
use pallet_bridge_grandpa::{Call as GrandpaCall, Pallet as GrandpaPallet, StoredAuthoritySet};
use pallet_bridge_messages::{Call as MessagesCall, Pallet as MessagesPallet};
use pallet_bridge_parachains::{
	Call as ParachainsCall, Pallet as ParachainsPallet, RelayBlockHash,
};
use sp_runtime::{
	traits::{DispatchTransaction, Header as HeaderT},
	transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
	DispatchError,
};

fn initial_balance_of_relayer_account_at_this_chain() -> ThisChainBalance {
	let test_stake: ThisChainBalance = TestStake::get();
	ExistentialDeposit::get().saturating_add(test_stake * 100)
}

// in tests, the following accounts are equal (because of how `into_sub_account_truncating`
// works)

fn delivery_rewards_account() -> ThisChainAccountId {
	TestPaymentProcedure::rewards_account(MsgProofsRewardsAccount::get())
}

fn confirmation_rewards_account() -> ThisChainAccountId {
	TestPaymentProcedure::rewards_account(MsgDeliveryProofsRewardsAccount::get())
}

fn relayer_account_at_this_chain() -> ThisChainAccountId {
	0
}

fn relayer_account_at_bridged_chain() -> BridgedChainAccountId {
	0
}

pub(crate) fn initialize_environment(
	best_relay_header_number: RelayBlockNumber,
	parachain_head_at_relay_header_number: RelayBlockNumber,
	best_message: MessageNonce,
) {
	let authorities = test_keyring().into_iter().map(|(a, w)| (a.into(), w)).collect();
	let best_relay_header = HeaderId(best_relay_header_number, RelayBlockHash::default());
	pallet_bridge_grandpa::CurrentAuthoritySet::<TestRuntime>::put(
		StoredAuthoritySet::try_new(authorities, 0).unwrap(),
	);
	pallet_bridge_grandpa::BestFinalized::<TestRuntime>::put(best_relay_header);

	let para_id = ParaId(TestParachain::get());
	let para_info = ParaInfo {
		best_head_hash: BestParaHeadHash {
			at_relay_block_number: parachain_head_at_relay_header_number,
			head_hash: [parachain_head_at_relay_header_number as u8; 32].into(),
		},
		next_imported_hash_position: 0,
	};
	pallet_bridge_parachains::ParasInfo::<TestRuntime>::insert(para_id, para_info);

	let lane_id = TestLaneId::get();
	let in_lane_data = InboundLaneData { last_confirmed_nonce: best_message, ..Default::default() };
	pallet_bridge_messages::InboundLanes::<TestRuntime>::insert(lane_id, in_lane_data);

	let out_lane_data =
		OutboundLaneData { latest_received_nonce: best_message, ..Default::default() };
	pallet_bridge_messages::OutboundLanes::<TestRuntime>::insert(lane_id, out_lane_data);

	Balances::mint_into(&delivery_rewards_account(), ExistentialDeposit::get()).unwrap();
	Balances::mint_into(&confirmation_rewards_account(), ExistentialDeposit::get()).unwrap();
	Balances::mint_into(
		&relayer_account_at_this_chain(),
		initial_balance_of_relayer_account_at_this_chain(),
	)
	.unwrap();
}

fn submit_relay_header_call(relay_header_number: RelayBlockNumber) -> RuntimeCall {
	let relay_header = BridgedChainHeader::new(
		relay_header_number,
		Default::default(),
		Default::default(),
		Default::default(),
		Default::default(),
	);
	let relay_justification = make_default_justification(&relay_header);

	RuntimeCall::BridgeGrandpa(GrandpaCall::submit_finality_proof {
		finality_target: Box::new(relay_header),
		justification: relay_justification,
	})
}

fn submit_parachain_head_call(
	parachain_head_at_relay_header_number: RelayBlockNumber,
) -> RuntimeCall {
	RuntimeCall::BridgeParachains(ParachainsCall::submit_parachain_heads {
		at_relay_block: (parachain_head_at_relay_header_number, RelayBlockHash::default()),
		parachains: vec![(
			ParaId(TestParachain::get()),
			[parachain_head_at_relay_header_number as u8; 32].into(),
		)],
		parachain_heads_proof: ParaHeadsProof(vec![]),
	})
}

pub(crate) fn message_delivery_call(best_message: MessageNonce) -> RuntimeCall {
	RuntimeCall::BridgeMessages(MessagesCall::receive_messages_proof {
		relayer_id_at_bridged_chain: relayer_account_at_bridged_chain(),
		proof: FromBridgedChainMessagesProof {
			bridged_header_hash: Default::default(),
			storage_proof: vec![],
			lane: TestLaneId::get(),
			nonces_start: pallet_bridge_messages::InboundLanes::<TestRuntime>::get(TEST_LANE_ID)
				.last_delivered_nonce() +
				1,
			nonces_end: best_message,
		},
		messages_count: 1,
		dispatch_weight: Weight::zero(),
	})
}

pub(crate) fn message_confirmation_call(best_message: MessageNonce) -> RuntimeCall {
	RuntimeCall::BridgeMessages(MessagesCall::receive_messages_delivery_proof {
		proof: FromBridgedChainMessagesDeliveryProof {
			bridged_header_hash: Default::default(),
			storage_proof: vec![],
			lane: TestLaneId::get(),
		},
		relayers_state: UnrewardedRelayersState {
			last_delivered_nonce: best_message,
			..Default::default()
		},
	})
}

fn parachain_finality_and_delivery_batch_call(
	parachain_head_at_relay_header_number: RelayBlockNumber,
	best_message: MessageNonce,
) -> RuntimeCall {
	RuntimeCall::Utility(UtilityCall::batch_all {
		calls: vec![
			submit_parachain_head_call(parachain_head_at_relay_header_number),
			message_delivery_call(best_message),
		],
	})
}

fn parachain_finality_and_confirmation_batch_call(
	parachain_head_at_relay_header_number: RelayBlockNumber,
	best_message: MessageNonce,
) -> RuntimeCall {
	RuntimeCall::Utility(UtilityCall::batch_all {
		calls: vec![
			submit_parachain_head_call(parachain_head_at_relay_header_number),
			message_confirmation_call(best_message),
		],
	})
}

fn relay_finality_and_delivery_batch_call(
	relay_header_number: RelayBlockNumber,
	best_message: MessageNonce,
) -> RuntimeCall {
	RuntimeCall::Utility(UtilityCall::batch_all {
		calls: vec![
			submit_relay_header_call(relay_header_number),
			message_delivery_call(best_message),
		],
	})
}

fn relay_finality_and_confirmation_batch_call(
	relay_header_number: RelayBlockNumber,
	best_message: MessageNonce,
) -> RuntimeCall {
	RuntimeCall::Utility(UtilityCall::batch_all {
		calls: vec![
			submit_relay_header_call(relay_header_number),
			message_confirmation_call(best_message),
		],
	})
}

fn all_finality_and_delivery_batch_call(
	relay_header_number: RelayBlockNumber,
	parachain_head_at_relay_header_number: RelayBlockNumber,
	best_message: MessageNonce,
) -> RuntimeCall {
	RuntimeCall::Utility(UtilityCall::batch_all {
		calls: vec![
			submit_relay_header_call(relay_header_number),
			submit_parachain_head_call(parachain_head_at_relay_header_number),
			message_delivery_call(best_message),
		],
	})
}

fn all_finality_and_confirmation_batch_call(
	relay_header_number: RelayBlockNumber,
	parachain_head_at_relay_header_number: RelayBlockNumber,
	best_message: MessageNonce,
) -> RuntimeCall {
	RuntimeCall::Utility(UtilityCall::batch_all {
		calls: vec![
			submit_relay_header_call(relay_header_number),
			submit_parachain_head_call(parachain_head_at_relay_header_number),
			message_confirmation_call(best_message),
		],
	})
}

fn all_finality_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
	PreDispatchData {
		relayer: relayer_account_at_this_chain(),
		call_info: CallInfo::AllFinalityAndMsgs(
			SubmitFinalityProofInfo {
				block_number: 200,
				extra_weight: Weight::zero(),
				extra_size: 0,
			},
			SubmitParachainHeadsInfo {
				at_relay_block_number: 200,
				para_id: ParaId(TestParachain::get()),
				para_head_hash: [200u8; 32].into(),
			},
			MessagesCallInfo::ReceiveMessagesProof(ReceiveMessagesProofInfo {
				base: BaseMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					bundled_range: 101..=200,
					best_stored_nonce: 100,
				},
				unrewarded_relayers: UnrewardedRelayerOccupation {
					free_relayer_slots: MaxUnrewardedRelayerEntriesAtInboundLane::get(),
					free_message_slots: MaxUnconfirmedMessagesAtInboundLane::get(),
				},
			}),
		),
	}
}

fn all_finality_confirmation_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
	PreDispatchData {
		relayer: relayer_account_at_this_chain(),
		call_info: CallInfo::AllFinalityAndMsgs(
			SubmitFinalityProofInfo {
				block_number: 200,
				extra_weight: Weight::zero(),
				extra_size: 0,
			},
			SubmitParachainHeadsInfo {
				at_relay_block_number: 200,
				para_id: ParaId(TestParachain::get()),
				para_head_hash: [200u8; 32].into(),
			},
			MessagesCallInfo::ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo(
				BaseMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					bundled_range: 101..=200,
					best_stored_nonce: 100,
				},
			)),
		),
	}
}

fn relay_finality_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
	PreDispatchData {
		relayer: relayer_account_at_this_chain(),
		call_info: CallInfo::RelayFinalityAndMsgs(
			SubmitFinalityProofInfo {
				block_number: 200,
				extra_weight: Weight::zero(),
				extra_size: 0,
			},
			MessagesCallInfo::ReceiveMessagesProof(ReceiveMessagesProofInfo {
				base: BaseMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					bundled_range: 101..=200,
					best_stored_nonce: 100,
				},
				unrewarded_relayers: UnrewardedRelayerOccupation {
					free_relayer_slots: MaxUnrewardedRelayerEntriesAtInboundLane::get(),
					free_message_slots: MaxUnconfirmedMessagesAtInboundLane::get(),
				},
			}),
		),
	}
}

fn relay_finality_confirmation_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
	PreDispatchData {
		relayer: relayer_account_at_this_chain(),
		call_info: CallInfo::RelayFinalityAndMsgs(
			SubmitFinalityProofInfo {
				block_number: 200,
				extra_weight: Weight::zero(),
				extra_size: 0,
			},
			MessagesCallInfo::ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo(
				BaseMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					bundled_range: 101..=200,
					best_stored_nonce: 100,
				},
			)),
		),
	}
}

fn parachain_finality_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
	PreDispatchData {
		relayer: relayer_account_at_this_chain(),
		call_info: CallInfo::ParachainFinalityAndMsgs(
			SubmitParachainHeadsInfo {
				at_relay_block_number: 200,
				para_id: ParaId(TestParachain::get()),
				para_head_hash: [200u8; 32].into(),
			},
			MessagesCallInfo::ReceiveMessagesProof(ReceiveMessagesProofInfo {
				base: BaseMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					bundled_range: 101..=200,
					best_stored_nonce: 100,
				},
				unrewarded_relayers: UnrewardedRelayerOccupation {
					free_relayer_slots: MaxUnrewardedRelayerEntriesAtInboundLane::get(),
					free_message_slots: MaxUnconfirmedMessagesAtInboundLane::get(),
				},
			}),
		),
	}
}

fn parachain_finality_confirmation_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
	PreDispatchData {
		relayer: relayer_account_at_this_chain(),
		call_info: CallInfo::ParachainFinalityAndMsgs(
			SubmitParachainHeadsInfo {
				at_relay_block_number: 200,
				para_id: ParaId(TestParachain::get()),
				para_head_hash: [200u8; 32].into(),
			},
			MessagesCallInfo::ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo(
				BaseMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					bundled_range: 101..=200,
					best_stored_nonce: 100,
				},
			)),
		),
	}
}

fn delivery_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
	PreDispatchData {
		relayer: relayer_account_at_this_chain(),
		call_info: CallInfo::Msgs(MessagesCallInfo::ReceiveMessagesProof(
			ReceiveMessagesProofInfo {
				base: BaseMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					bundled_range: 101..=200,
					best_stored_nonce: 100,
				},
				unrewarded_relayers: UnrewardedRelayerOccupation {
					free_relayer_slots: MaxUnrewardedRelayerEntriesAtInboundLane::get(),
					free_message_slots: MaxUnconfirmedMessagesAtInboundLane::get(),
				},
			},
		)),
	}
}

fn confirmation_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
	PreDispatchData {
		relayer: relayer_account_at_this_chain(),
		call_info: CallInfo::Msgs(MessagesCallInfo::ReceiveMessagesDeliveryProof(
			ReceiveMessagesDeliveryProofInfo(BaseMessagesProofInfo {
				lane_id: TEST_LANE_ID,
				bundled_range: 101..=200,
				best_stored_nonce: 100,
			}),
		)),
	}
}

fn set_bundled_range_end(
	mut pre_dispatch_data: PreDispatchData<ThisChainAccountId>,
	end: MessageNonce,
) -> PreDispatchData<ThisChainAccountId> {
	let msg_info = match pre_dispatch_data.call_info {
		CallInfo::AllFinalityAndMsgs(_, _, ref mut info) => info,
		CallInfo::RelayFinalityAndMsgs(_, ref mut info) => info,
		CallInfo::ParachainFinalityAndMsgs(_, ref mut info) => info,
		CallInfo::Msgs(ref mut info) => info,
	};

	if let MessagesCallInfo::ReceiveMessagesProof(ref mut msg_info) = msg_info {
		msg_info.base.bundled_range = *msg_info.base.bundled_range.start()..=end
	}

	pre_dispatch_data
}

fn run_validate(call: RuntimeCall) -> TransactionValidity {
	let extension: TestExtension =
		RefundTransactionExtensionAdapter(RefundBridgedParachainMessages(PhantomData));
	extension
		.validate_only(
			Some(relayer_account_at_this_chain()).into(),
			&call,
			&DispatchInfo::default(),
			0,
		)
		.map(|res| res.0)
}

fn run_grandpa_validate(call: RuntimeCall) -> TransactionValidity {
	let extension: TestGrandpaExtension =
		RefundTransactionExtensionAdapter(RefundBridgedGrandpaMessages(PhantomData));
	extension
		.validate_only(
			Some(relayer_account_at_this_chain()).into(),
			&call,
			&DispatchInfo::default(),
			0,
		)
		.map(|res| res.0)
}

fn run_validate_ignore_priority(call: RuntimeCall) -> TransactionValidity {
	run_validate(call).map(|mut tx| {
		tx.priority = 0;
		tx
	})
}

fn run_pre_dispatch(
	call: RuntimeCall,
) -> Result<Option<PreDispatchData<ThisChainAccountId>>, TransactionValidityError> {
	let extension: TestExtension =
		RefundTransactionExtensionAdapter(RefundBridgedParachainMessages(PhantomData));
	extension
		.validate_and_prepare(
			Some(relayer_account_at_this_chain()).into(),
			&call,
			&DispatchInfo::default(),
			0,
		)
		.map(|(pre, _)| pre)
}

fn run_grandpa_pre_dispatch(
	call: RuntimeCall,
) -> Result<Option<PreDispatchData<ThisChainAccountId>>, TransactionValidityError> {
	let extension: TestGrandpaExtension =
		RefundTransactionExtensionAdapter(RefundBridgedGrandpaMessages(PhantomData));
	extension
		.validate_and_prepare(
			Some(relayer_account_at_this_chain()).into(),
			&call,
			&DispatchInfo::default(),
			0,
		)
		.map(|(pre, _)| pre)
}

pub(crate) fn dispatch_info() -> DispatchInfo {
	DispatchInfo {
		weight: Weight::from_parts(
			frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND,
			0,
		),
		class: frame_support::dispatch::DispatchClass::Normal,
		pays_fee: frame_support::dispatch::Pays::Yes,
	}
}

fn post_dispatch_info() -> PostDispatchInfo {
	PostDispatchInfo { actual_weight: None, pays_fee: frame_support::dispatch::Pays::Yes }
}

fn run_post_dispatch(
	pre_dispatch_data: Option<PreDispatchData<ThisChainAccountId>>,
	dispatch_result: DispatchResult,
) {
	let post_dispatch_result = TestExtension::post_dispatch(
		pre_dispatch_data,
		&dispatch_info(),
		&post_dispatch_info(),
		1024,
		&dispatch_result,
		&(),
	);
	assert_eq!(post_dispatch_result, Ok(()));
}

fn expected_delivery_reward() -> ThisChainBalance {
	let mut post_dispatch_info = post_dispatch_info();
	let extra_weight = <TestRuntime as RelayersConfig>::WeightInfo::extra_weight_of_successful_receive_messages_proof_call();
	post_dispatch_info.actual_weight = Some(dispatch_info().weight.saturating_sub(extra_weight));
	pallet_transaction_payment::Pallet::<TestRuntime>::compute_actual_fee(
		1024,
		&dispatch_info(),
		&post_dispatch_info,
		Zero::zero(),
	)
}

fn expected_confirmation_reward() -> ThisChainBalance {
	pallet_transaction_payment::Pallet::<TestRuntime>::compute_actual_fee(
		1024,
		&dispatch_info(),
		&post_dispatch_info(),
		Zero::zero(),
	)
}

pub(crate) fn parachain_extension() -> TestExtension {
	RefundTransactionExtensionAdapter(RefundBridgedParachainMessages(PhantomData))
}

pub(crate) fn grandpa_extension() -> TestGrandpaExtension {
	RefundTransactionExtensionAdapter(RefundBridgedGrandpaMessages(PhantomData))
}

#[test]
fn validate_doesnt_boost_transaction_priority_if_relayer_is_not_registered() {
	run_test(|| {
		initialize_environment(100, 100, 100);
		Balances::set_balance(&relayer_account_at_this_chain(), ExistentialDeposit::get());

		// message delivery is failing
		assert_eq!(run_validate(message_delivery_call(200)), Ok(Default::default()),);
		assert_eq!(
			run_validate(parachain_finality_and_delivery_batch_call(200, 200)),
			Ok(Default::default()),
		);
		assert_eq!(
			run_validate(all_finality_and_delivery_batch_call(200, 200, 200)),
			Ok(Default::default()),
		);
		// message confirmation validation is passing
		assert_eq!(
			run_validate_ignore_priority(message_confirmation_call(200)),
			Ok(Default::default()),
		);
		assert_eq!(
			run_validate_ignore_priority(parachain_finality_and_confirmation_batch_call(200, 200)),
			Ok(Default::default()),
		);
		assert_eq!(
			run_validate_ignore_priority(all_finality_and_confirmation_batch_call(200, 200, 200)),
			Ok(Default::default()),
		);
	});
}

#[test]
fn validate_boosts_priority_of_message_delivery_transactons() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
			.unwrap();

		let priority_of_100_messages_delivery =
			run_validate(message_delivery_call(200)).unwrap().priority;
		let priority_of_200_messages_delivery =
			run_validate(message_delivery_call(300)).unwrap().priority;
		assert!(
			priority_of_200_messages_delivery > priority_of_100_messages_delivery,
			"Invalid priorities: {} for 200 messages vs {} for 100 messages",
			priority_of_200_messages_delivery,
			priority_of_100_messages_delivery,
		);

		let priority_of_100_messages_confirmation =
			run_validate(message_confirmation_call(200)).unwrap().priority;
		let priority_of_200_messages_confirmation =
			run_validate(message_confirmation_call(300)).unwrap().priority;
		assert_eq!(priority_of_100_messages_confirmation, priority_of_200_messages_confirmation);
	});
}

#[test]
fn validate_does_not_boost_priority_of_message_delivery_transactons_with_too_many_messages() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
			.unwrap();

		let priority_of_max_messages_delivery =
			run_validate(message_delivery_call(100 + MaxUnconfirmedMessagesAtInboundLane::get()))
				.unwrap()
				.priority;
		let priority_of_more_than_max_messages_delivery = run_validate(message_delivery_call(
			100 + MaxUnconfirmedMessagesAtInboundLane::get() + 1,
		))
		.unwrap()
		.priority;

		assert!(
			priority_of_max_messages_delivery > priority_of_more_than_max_messages_delivery,
			"Invalid priorities: {} for MAX messages vs {} for MAX+1 messages",
			priority_of_max_messages_delivery,
			priority_of_more_than_max_messages_delivery,
		);
	});
}

#[test]
fn validate_allows_non_obsolete_transactions() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		assert_eq!(
			run_validate_ignore_priority(message_delivery_call(200)),
			Ok(ValidTransaction::default()),
		);
		assert_eq!(
			run_validate_ignore_priority(message_confirmation_call(200)),
			Ok(ValidTransaction::default()),
		);

		assert_eq!(
			run_validate_ignore_priority(parachain_finality_and_delivery_batch_call(200, 200)),
			Ok(ValidTransaction::default()),
		);
		assert_eq!(
			run_validate_ignore_priority(parachain_finality_and_confirmation_batch_call(200, 200)),
			Ok(ValidTransaction::default()),
		);

		assert_eq!(
			run_validate_ignore_priority(all_finality_and_delivery_batch_call(200, 200, 200)),
			Ok(ValidTransaction::default()),
		);
		assert_eq!(
			run_validate_ignore_priority(all_finality_and_confirmation_batch_call(200, 200, 200)),
			Ok(ValidTransaction::default()),
		);
	});
}

#[test]
fn ext_rejects_batch_with_obsolete_relay_chain_header() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		assert_eq!(
			run_pre_dispatch(all_finality_and_delivery_batch_call(100, 200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);

		assert_eq!(
			run_validate(all_finality_and_delivery_batch_call(100, 200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
	});
}

#[test]
fn ext_rejects_batch_with_obsolete_parachain_head() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		assert_eq!(
			run_pre_dispatch(all_finality_and_delivery_batch_call(101, 100, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
		assert_eq!(
			run_validate(all_finality_and_delivery_batch_call(101, 100, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);

		assert_eq!(
			run_pre_dispatch(parachain_finality_and_delivery_batch_call(100, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
		assert_eq!(
			run_validate(parachain_finality_and_delivery_batch_call(100, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
	});
}

#[test]
fn ext_rejects_batch_with_obsolete_messages() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		assert_eq!(
			run_pre_dispatch(all_finality_and_delivery_batch_call(200, 200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
		assert_eq!(
			run_pre_dispatch(all_finality_and_confirmation_batch_call(200, 200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);

		assert_eq!(
			run_validate(all_finality_and_delivery_batch_call(200, 200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
		assert_eq!(
			run_validate(all_finality_and_confirmation_batch_call(200, 200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);

		assert_eq!(
			run_pre_dispatch(parachain_finality_and_delivery_batch_call(200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
		assert_eq!(
			run_pre_dispatch(parachain_finality_and_confirmation_batch_call(200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);

		assert_eq!(
			run_validate(parachain_finality_and_delivery_batch_call(200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
		assert_eq!(
			run_validate(parachain_finality_and_confirmation_batch_call(200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
	});
}

#[test]
fn ext_rejects_batch_with_grandpa_finality_proof_when_grandpa_pallet_is_halted() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		GrandpaPallet::<TestRuntime, ()>::set_operating_mode(
			RuntimeOrigin::root(),
			BasicOperatingMode::Halted,
		)
		.unwrap();

		assert_eq!(
			run_pre_dispatch(all_finality_and_delivery_batch_call(200, 200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);
		assert_eq!(
			run_pre_dispatch(all_finality_and_confirmation_batch_call(200, 200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);
	});
}

#[test]
fn ext_rejects_batch_with_parachain_finality_proof_when_parachains_pallet_is_halted() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		ParachainsPallet::<TestRuntime, ()>::set_operating_mode(
			RuntimeOrigin::root(),
			BasicOperatingMode::Halted,
		)
		.unwrap();

		assert_eq!(
			run_pre_dispatch(all_finality_and_delivery_batch_call(200, 200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);
		assert_eq!(
			run_pre_dispatch(all_finality_and_confirmation_batch_call(200, 200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);

		assert_eq!(
			run_pre_dispatch(parachain_finality_and_delivery_batch_call(200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);
		assert_eq!(
			run_pre_dispatch(parachain_finality_and_confirmation_batch_call(200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);
	});
}

#[test]
fn ext_rejects_transaction_when_messages_pallet_is_halted() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		MessagesPallet::<TestRuntime, ()>::set_operating_mode(
			RuntimeOrigin::root(),
			MessagesOperatingMode::Basic(BasicOperatingMode::Halted),
		)
		.unwrap();

		assert_eq!(
			run_pre_dispatch(all_finality_and_delivery_batch_call(200, 200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);
		assert_eq!(
			run_pre_dispatch(all_finality_and_confirmation_batch_call(200, 200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);

		assert_eq!(
			run_pre_dispatch(parachain_finality_and_delivery_batch_call(200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);
		assert_eq!(
			run_pre_dispatch(parachain_finality_and_confirmation_batch_call(200, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);

		assert_eq!(
			run_pre_dispatch(message_delivery_call(200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);
		assert_eq!(
			run_pre_dispatch(message_confirmation_call(200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);
	});
}

#[test]
fn pre_dispatch_parses_batch_with_relay_chain_and_parachain_headers() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		assert_eq!(
			run_pre_dispatch(all_finality_and_delivery_batch_call(200, 200, 200)),
			Ok(Some(all_finality_pre_dispatch_data())),
		);
		assert_eq!(
			run_pre_dispatch(all_finality_and_confirmation_batch_call(200, 200, 200)),
			Ok(Some(all_finality_confirmation_pre_dispatch_data())),
		);
	});
}

#[test]
fn pre_dispatch_parses_batch_with_parachain_header() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		assert_eq!(
			run_pre_dispatch(parachain_finality_and_delivery_batch_call(200, 200)),
			Ok(Some(parachain_finality_pre_dispatch_data())),
		);
		assert_eq!(
			run_pre_dispatch(parachain_finality_and_confirmation_batch_call(200, 200)),
			Ok(Some(parachain_finality_confirmation_pre_dispatch_data())),
		);
	});
}

#[test]
fn pre_dispatch_fails_to_parse_batch_with_multiple_parachain_headers() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		let call = RuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![
				RuntimeCall::BridgeParachains(ParachainsCall::submit_parachain_heads {
					at_relay_block: (100, RelayBlockHash::default()),
					parachains: vec![
						(ParaId(TestParachain::get()), [1u8; 32].into()),
						(ParaId(TestParachain::get() + 1), [1u8; 32].into()),
					],
					parachain_heads_proof: ParaHeadsProof(vec![]),
				}),
				message_delivery_call(200),
			],
		});

		assert_eq!(run_pre_dispatch(call), Ok(None),);
	});
}

#[test]
fn pre_dispatch_parses_message_transaction() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		assert_eq!(
			run_pre_dispatch(message_delivery_call(200)),
			Ok(Some(delivery_pre_dispatch_data())),
		);
		assert_eq!(
			run_pre_dispatch(message_confirmation_call(200)),
			Ok(Some(confirmation_pre_dispatch_data())),
		);
	});
}

pub(crate) fn pre_dispatch_data_get() -> PreDispatchData<AccountIdOf<TestRuntime>> {
	let mut pre_dispatch_data = PreDispatchData {
		relayer: relayer_account_at_this_chain(),
		call_info: CallInfo::AllFinalityAndMsgs(
			SubmitFinalityProofInfo {
				block_number: 100,
				extra_weight: Weight::zero(),
				extra_size: 0,
			},
			SubmitParachainHeadsInfo {
				at_relay_block_number: 100,
				para_id: ParaId(TestParachain::get()),
				para_head_hash: [100u8; 32].into(),
			},
			MessagesCallInfo::ReceiveMessagesProof(ReceiveMessagesProofInfo {
				base: BaseMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					bundled_range: 1..=100,
					best_stored_nonce: 1,
				},
				unrewarded_relayers: UnrewardedRelayerOccupation {
					free_relayer_slots: MaxUnrewardedRelayerEntriesAtInboundLane::get(),
					free_message_slots: MaxUnconfirmedMessagesAtInboundLane::get(),
				},
			}),
		),
	};
	match pre_dispatch_data.call_info {
		CallInfo::AllFinalityAndMsgs(ref mut info, ..) => {
			info.extra_weight
				.set_ref_time(frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND);
			info.extra_size = 32;
		},
		_ => unreachable!(),
	}
	pre_dispatch_data
}

#[test]
fn post_dispatch_ignores_unknown_transaction() {
	run_test(|| {
		assert_storage_noop!(run_post_dispatch(None, Ok(())));
	});
}

#[test]
fn post_dispatch_ignores_failed_transaction() {
	run_test(|| {
		assert_storage_noop!(run_post_dispatch(
			Some(all_finality_pre_dispatch_data()),
			Err(DispatchError::BadOrigin)
		));
	});
}

#[test]
fn post_dispatch_ignores_transaction_that_has_not_updated_relay_chain_state() {
	run_test(|| {
		initialize_environment(100, 200, 200);

		assert_storage_noop!(run_post_dispatch(Some(all_finality_pre_dispatch_data()), Ok(())));
	});
}

#[test]
fn post_dispatch_ignores_transaction_that_has_not_updated_parachain_state() {
	run_test(|| {
		initialize_environment(200, 100, 200);

		assert_storage_noop!(run_post_dispatch(Some(all_finality_pre_dispatch_data()), Ok(())));
		assert_storage_noop!(run_post_dispatch(
			Some(parachain_finality_pre_dispatch_data()),
			Ok(())
		));
	});
}

#[test]
fn post_dispatch_ignores_transaction_that_has_not_delivered_any_messages() {
	run_test(|| {
		initialize_environment(200, 200, 100);

		assert_storage_noop!(run_post_dispatch(Some(all_finality_pre_dispatch_data()), Ok(())));
		assert_storage_noop!(run_post_dispatch(
			Some(parachain_finality_pre_dispatch_data()),
			Ok(())
		));
		assert_storage_noop!(run_post_dispatch(Some(delivery_pre_dispatch_data()), Ok(())));

		assert_storage_noop!(run_post_dispatch(
			Some(all_finality_confirmation_pre_dispatch_data()),
			Ok(())
		));
		assert_storage_noop!(run_post_dispatch(
			Some(parachain_finality_confirmation_pre_dispatch_data()),
			Ok(())
		));
		assert_storage_noop!(run_post_dispatch(Some(confirmation_pre_dispatch_data()), Ok(())));
	});
}

#[test]
fn post_dispatch_ignores_transaction_that_has_not_delivered_all_messages() {
	run_test(|| {
		initialize_environment(200, 200, 150);

		assert_storage_noop!(run_post_dispatch(Some(all_finality_pre_dispatch_data()), Ok(())));
		assert_storage_noop!(run_post_dispatch(
			Some(parachain_finality_pre_dispatch_data()),
			Ok(())
		));
		assert_storage_noop!(run_post_dispatch(Some(delivery_pre_dispatch_data()), Ok(())));

		assert_storage_noop!(run_post_dispatch(
			Some(all_finality_confirmation_pre_dispatch_data()),
			Ok(())
		));
		assert_storage_noop!(run_post_dispatch(
			Some(parachain_finality_confirmation_pre_dispatch_data()),
			Ok(())
		));
		assert_storage_noop!(run_post_dispatch(Some(confirmation_pre_dispatch_data()), Ok(())));
	});
}

#[test]
fn post_dispatch_refunds_relayer_in_all_finality_batch_with_extra_weight() {
	run_test(|| {
		initialize_environment(200, 200, 200);

		let mut dispatch_info = dispatch_info();
		dispatch_info.weight = Weight::from_parts(
			frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND * 2,
			0,
		);

		// without any size/weight refund: we expect regular reward
		let pre_dispatch_data = all_finality_pre_dispatch_data();
		let regular_reward = expected_delivery_reward();
		run_post_dispatch(Some(pre_dispatch_data), Ok(()));
		assert_eq!(
			RelayersPallet::<TestRuntime>::relayer_reward(
				relayer_account_at_this_chain(),
				MsgProofsRewardsAccount::get()
			),
			Some(regular_reward),
		);

		// now repeat the same with size+weight refund: we expect smaller reward
		let mut pre_dispatch_data = all_finality_pre_dispatch_data();
		match pre_dispatch_data.call_info {
			CallInfo::AllFinalityAndMsgs(ref mut info, ..) => {
				info.extra_weight
					.set_ref_time(frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND);
				info.extra_size = 32;
			},
			_ => unreachable!(),
		}
		run_post_dispatch(Some(pre_dispatch_data), Ok(()));
		let reward_after_two_calls = RelayersPallet::<TestRuntime>::relayer_reward(
			relayer_account_at_this_chain(),
			MsgProofsRewardsAccount::get(),
		)
		.unwrap();
		assert!(
			reward_after_two_calls < 2 * regular_reward,
			"{}  must be < 2 * {}",
			reward_after_two_calls,
			2 * regular_reward,
		);
	});
}

#[test]
fn post_dispatch_refunds_relayer_in_all_finality_batch() {
	run_test(|| {
		initialize_environment(200, 200, 200);

		run_post_dispatch(Some(all_finality_pre_dispatch_data()), Ok(()));
		assert_eq!(
			RelayersPallet::<TestRuntime>::relayer_reward(
				relayer_account_at_this_chain(),
				MsgProofsRewardsAccount::get()
			),
			Some(expected_delivery_reward()),
		);

		run_post_dispatch(Some(all_finality_confirmation_pre_dispatch_data()), Ok(()));
		assert_eq!(
			RelayersPallet::<TestRuntime>::relayer_reward(
				relayer_account_at_this_chain(),
				MsgDeliveryProofsRewardsAccount::get()
			),
			Some(expected_confirmation_reward()),
		);
	});
}

#[test]
fn post_dispatch_refunds_relayer_in_parachain_finality_batch() {
	run_test(|| {
		initialize_environment(200, 200, 200);

		run_post_dispatch(Some(parachain_finality_pre_dispatch_data()), Ok(()));
		assert_eq!(
			RelayersPallet::<TestRuntime>::relayer_reward(
				relayer_account_at_this_chain(),
				MsgProofsRewardsAccount::get()
			),
			Some(expected_delivery_reward()),
		);

		run_post_dispatch(Some(parachain_finality_confirmation_pre_dispatch_data()), Ok(()));
		assert_eq!(
			RelayersPallet::<TestRuntime>::relayer_reward(
				relayer_account_at_this_chain(),
				MsgDeliveryProofsRewardsAccount::get()
			),
			Some(expected_confirmation_reward()),
		);
	});
}

#[test]
fn post_dispatch_refunds_relayer_in_message_transaction() {
	run_test(|| {
		initialize_environment(200, 200, 200);

		run_post_dispatch(Some(delivery_pre_dispatch_data()), Ok(()));
		assert_eq!(
			RelayersPallet::<TestRuntime>::relayer_reward(
				relayer_account_at_this_chain(),
				MsgProofsRewardsAccount::get()
			),
			Some(expected_delivery_reward()),
		);

		run_post_dispatch(Some(confirmation_pre_dispatch_data()), Ok(()));
		assert_eq!(
			RelayersPallet::<TestRuntime>::relayer_reward(
				relayer_account_at_this_chain(),
				MsgDeliveryProofsRewardsAccount::get()
			),
			Some(expected_confirmation_reward()),
		);
	});
}

#[test]
fn post_dispatch_slashing_relayer_stake() {
	run_test(|| {
		initialize_environment(200, 200, 100);

		let delivery_rewards_account_balance = Balances::free_balance(delivery_rewards_account());

		let test_stake: ThisChainBalance = TestStake::get();
		Balances::set_balance(
			&relayer_account_at_this_chain(),
			ExistentialDeposit::get() + test_stake * 10,
		);

		// slashing works for message delivery calls
		BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
			.unwrap();
		assert_eq!(Balances::reserved_balance(relayer_account_at_this_chain()), test_stake);
		run_post_dispatch(Some(delivery_pre_dispatch_data()), Ok(()));
		assert_eq!(Balances::reserved_balance(relayer_account_at_this_chain()), 0);
		assert_eq!(
			delivery_rewards_account_balance + test_stake,
			Balances::free_balance(delivery_rewards_account())
		);

		BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
			.unwrap();
		assert_eq!(Balances::reserved_balance(relayer_account_at_this_chain()), test_stake);
		run_post_dispatch(Some(parachain_finality_pre_dispatch_data()), Ok(()));
		assert_eq!(Balances::reserved_balance(relayer_account_at_this_chain()), 0);
		assert_eq!(
			delivery_rewards_account_balance + test_stake * 2,
			Balances::free_balance(delivery_rewards_account())
		);

		BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
			.unwrap();
		assert_eq!(Balances::reserved_balance(relayer_account_at_this_chain()), test_stake);
		run_post_dispatch(Some(all_finality_pre_dispatch_data()), Ok(()));
		assert_eq!(Balances::reserved_balance(relayer_account_at_this_chain()), 0);
		assert_eq!(
			delivery_rewards_account_balance + test_stake * 3,
			Balances::free_balance(delivery_rewards_account())
		);

		// reserve doesn't work for message confirmation calls
		let confirmation_rewards_account_balance =
			Balances::free_balance(confirmation_rewards_account());

		Balances::reserve(&relayer_account_at_this_chain(), test_stake).unwrap();
		assert_eq!(Balances::reserved_balance(relayer_account_at_this_chain()), test_stake);

		assert_eq!(
			confirmation_rewards_account_balance,
			Balances::free_balance(confirmation_rewards_account())
		);
		run_post_dispatch(Some(confirmation_pre_dispatch_data()), Ok(()));
		assert_eq!(Balances::reserved_balance(relayer_account_at_this_chain()), test_stake);

		run_post_dispatch(Some(parachain_finality_confirmation_pre_dispatch_data()), Ok(()));
		assert_eq!(Balances::reserved_balance(relayer_account_at_this_chain()), test_stake);

		run_post_dispatch(Some(all_finality_confirmation_pre_dispatch_data()), Ok(()));
		assert_eq!(Balances::reserved_balance(relayer_account_at_this_chain()), test_stake);

		// check that unreserve has happened, not slashing
		assert_eq!(
			delivery_rewards_account_balance + test_stake * 3,
			Balances::free_balance(delivery_rewards_account())
		);
		assert_eq!(
			confirmation_rewards_account_balance,
			Balances::free_balance(confirmation_rewards_account())
		);
	});
}

fn run_analyze_call_result(
	pre_dispatch_data: PreDispatchData<ThisChainAccountId>,
	dispatch_result: DispatchResult,
) -> RelayerAccountAction<ThisChainAccountId, ThisChainBalance> {
	TestExtensionProvider::analyze_call_result(
		Some(Some(pre_dispatch_data)),
		&dispatch_info(),
		&post_dispatch_info(),
		1024,
		&dispatch_result,
	)
}

#[test]
fn analyze_call_result_shall_not_slash_for_transactions_with_too_many_messages() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		// the `analyze_call_result` should return slash if number of bundled messages is
		// within reasonable limits
		assert_eq!(
			run_analyze_call_result(all_finality_pre_dispatch_data(), Ok(())),
			RelayerAccountAction::Slash(
				relayer_account_at_this_chain(),
				MsgProofsRewardsAccount::get()
			),
		);
		assert_eq!(
			run_analyze_call_result(parachain_finality_pre_dispatch_data(), Ok(())),
			RelayerAccountAction::Slash(
				relayer_account_at_this_chain(),
				MsgProofsRewardsAccount::get()
			),
		);
		assert_eq!(
			run_analyze_call_result(delivery_pre_dispatch_data(), Ok(())),
			RelayerAccountAction::Slash(
				relayer_account_at_this_chain(),
				MsgProofsRewardsAccount::get()
			),
		);

		// the `analyze_call_result` should not return slash if number of bundled messages is
		// larger than the
		assert_eq!(
			run_analyze_call_result(
				set_bundled_range_end(all_finality_pre_dispatch_data(), 1_000_000),
				Ok(())
			),
			RelayerAccountAction::None,
		);
		assert_eq!(
			run_analyze_call_result(
				set_bundled_range_end(parachain_finality_pre_dispatch_data(), 1_000_000),
				Ok(())
			),
			RelayerAccountAction::None,
		);
		assert_eq!(
			run_analyze_call_result(
				set_bundled_range_end(delivery_pre_dispatch_data(), 1_000_000),
				Ok(())
			),
			RelayerAccountAction::None,
		);
	});
}

#[test]
fn grandpa_ext_only_parses_valid_batches() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		// relay + parachain + message delivery calls batch is ignored
		assert_eq!(
			TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
				&all_finality_and_delivery_batch_call(200, 200, 200)
			),
			Ok(None),
		);

		// relay + parachain + message confirmation calls batch is ignored
		assert_eq!(
			TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
				&all_finality_and_confirmation_batch_call(200, 200, 200)
			),
			Ok(None),
		);

		// parachain + message delivery call batch is ignored
		assert_eq!(
			TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
				&parachain_finality_and_delivery_batch_call(200, 200)
			),
			Ok(None),
		);

		// parachain + message confirmation call batch is ignored
		assert_eq!(
			TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
				&parachain_finality_and_confirmation_batch_call(200, 200)
			),
			Ok(None),
		);

		// relay + message delivery call batch is accepted
		assert_eq!(
			TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
				&relay_finality_and_delivery_batch_call(200, 200)
			),
			Ok(Some(relay_finality_pre_dispatch_data().call_info)),
		);

		// relay + message confirmation call batch is accepted
		assert_eq!(
			TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
				&relay_finality_and_confirmation_batch_call(200, 200)
			),
			Ok(Some(relay_finality_confirmation_pre_dispatch_data().call_info)),
		);

		// message delivery call batch is accepted
		assert_eq!(
			TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
				&message_delivery_call(200)
			),
			Ok(Some(delivery_pre_dispatch_data().call_info)),
		);

		// message confirmation call batch is accepted
		assert_eq!(
			TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
				&message_confirmation_call(200)
			),
			Ok(Some(confirmation_pre_dispatch_data().call_info)),
		);
	});
}

#[test]
fn grandpa_ext_rejects_batch_with_obsolete_relay_chain_header() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		assert_eq!(
			run_grandpa_pre_dispatch(relay_finality_and_delivery_batch_call(100, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);

		assert_eq!(
			run_grandpa_validate(relay_finality_and_delivery_batch_call(100, 200)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
	});
}

#[test]
fn grandpa_ext_rejects_calls_with_obsolete_messages() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		assert_eq!(
			run_grandpa_pre_dispatch(relay_finality_and_delivery_batch_call(200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
		assert_eq!(
			run_grandpa_pre_dispatch(relay_finality_and_confirmation_batch_call(200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);

		assert_eq!(
			run_grandpa_validate(relay_finality_and_delivery_batch_call(200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
		assert_eq!(
			run_grandpa_validate(relay_finality_and_confirmation_batch_call(200, 100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);

		assert_eq!(
			run_grandpa_pre_dispatch(message_delivery_call(100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
		assert_eq!(
			run_grandpa_pre_dispatch(message_confirmation_call(100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);

		assert_eq!(
			run_grandpa_validate(message_delivery_call(100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
		assert_eq!(
			run_grandpa_validate(message_confirmation_call(100)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
		);
	});
}

#[test]
fn grandpa_ext_accepts_calls_with_new_messages() {
	run_test(|| {
		initialize_environment(100, 100, 100);

		assert_eq!(
			run_grandpa_pre_dispatch(relay_finality_and_delivery_batch_call(200, 200)),
			Ok(Some(relay_finality_pre_dispatch_data()),)
		);
		assert_eq!(
			run_grandpa_pre_dispatch(relay_finality_and_confirmation_batch_call(200, 200)),
			Ok(Some(relay_finality_confirmation_pre_dispatch_data())),
		);

		assert_eq!(
			run_grandpa_validate(relay_finality_and_delivery_batch_call(200, 200)),
			Ok(Default::default()),
		);
		assert_eq!(
			run_grandpa_validate(relay_finality_and_confirmation_batch_call(200, 200)),
			Ok(Default::default()),
		);

		assert_eq!(
			run_grandpa_pre_dispatch(message_delivery_call(200)),
			Ok(Some(delivery_pre_dispatch_data())),
		);
		assert_eq!(
			run_grandpa_pre_dispatch(message_confirmation_call(200)),
			Ok(Some(confirmation_pre_dispatch_data())),
		);

		assert_eq!(run_grandpa_validate(message_delivery_call(200)), Ok(Default::default()),);
		assert_eq!(run_grandpa_validate(message_confirmation_call(200)), Ok(Default::default()),);
	});
}

#[test]
fn does_not_panic_on_boosting_priority_of_empty_message_delivery_transaction() {
	run_test(|| {
		let best_delivered_message = MaxUnconfirmedMessagesAtInboundLane::get();
		initialize_environment(100, 100, best_delivered_message);

		// register relayer so it gets priority boost
		BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
			.unwrap();

		// allow empty message delivery transactions
		let lane_id = TestLaneId::get();
		let in_lane_data = InboundLaneData {
			last_confirmed_nonce: 0,
			relayers: vec![UnrewardedRelayer {
				relayer: relayer_account_at_bridged_chain(),
				messages: DeliveredMessages { begin: 1, end: best_delivered_message },
			}]
			.into(),
		};
		pallet_bridge_messages::InboundLanes::<TestRuntime>::insert(lane_id, in_lane_data);

		// now check that the priority of empty tx is the same as priority of 1-message tx
		let priority_of_zero_messages_delivery =
			run_validate(message_delivery_call(best_delivered_message)).unwrap().priority;
		let priority_of_one_messages_delivery =
			run_validate(message_delivery_call(best_delivered_message + 1))
				.unwrap()
				.priority;

		assert_eq!(priority_of_zero_messages_delivery, priority_of_one_messages_delivery);
	});
}
