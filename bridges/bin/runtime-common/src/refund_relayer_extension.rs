// Copyright 2021 Parity Technologies (UK) Ltd.
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

//! Signed extension that refunds relayer if he has delivered some new messages.
//! It also refunds transaction cost if the transaction is an `utility.batchAll()`
//! with calls that are: delivering new messsage and all necessary underlying headers
//! (parachain or relay chain).

use crate::messages_call_ext::{
	MessagesCallSubType, ReceiveMessagesProofHelper, ReceiveMessagesProofInfo,
};
use bp_messages::LaneId;
use codec::{Decode, Encode};
use frame_support::{
	dispatch::{CallableCallFor, DispatchInfo, Dispatchable, PostDispatchInfo},
	traits::IsSubType,
	CloneNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use pallet_bridge_grandpa::{CallSubType as GrandpaCallSubType, SubmitFinalityProofHelper};
use pallet_bridge_messages::Config as MessagesConfig;
use pallet_bridge_parachains::{
	BoundedBridgeGrandpaConfig, CallSubType as ParachainsCallSubType, Config as ParachainsConfig,
	RelayBlockNumber, SubmitParachainHeadsHelper, SubmitParachainHeadsInfo,
};
use pallet_bridge_relayers::{Config as RelayersConfig, Pallet as RelayersPallet};
use pallet_transaction_payment::{Config as TransactionPaymentConfig, OnChargeTransaction};
use pallet_utility::{Call as UtilityCall, Config as UtilityConfig, Pallet as UtilityPallet};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, Get, PostDispatchInfoOf, SignedExtension, Zero},
	transaction_validity::{TransactionValidity, TransactionValidityError, ValidTransaction},
	DispatchResult, FixedPointOperand,
};
use sp_std::{marker::PhantomData, vec, vec::Vec};

// TODO (https://github.com/paritytech/parity-bridges-common/issues/1667):
// support multiple bridges in this extension

/// Transaction fee calculation.
pub trait TransactionFeeCalculation<Balance> {
	/// Compute fee that is paid for given transaction. The fee is later refunded to relayer.
	fn compute_fee(
		info: &DispatchInfo,
		post_info: &PostDispatchInfo,
		len: usize,
		tip: Balance,
	) -> Balance;
}

impl<R> TransactionFeeCalculation<BalanceOf<R>> for R
where
	R: TransactionPaymentConfig,
	<R as frame_system::Config>::RuntimeCall:
		Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	BalanceOf<R>: FixedPointOperand,
{
	fn compute_fee(
		info: &DispatchInfo,
		post_info: &PostDispatchInfo,
		len: usize,
		tip: BalanceOf<R>,
	) -> BalanceOf<R> {
		pallet_transaction_payment::Pallet::<R>::compute_actual_fee(len as _, info, post_info, tip)
	}
}

/// Signed extension that refunds relayer for new messages coming from the parachain.
///
/// Also refunds relayer for successful finality delivery if it comes in batch (`utility.batchAll`)
/// with message delivery transaction. Batch may deliver either both relay chain header and
/// parachain head, or just parachain head. Corresponding headers must be used in messages
/// proof verification.
///
/// Extension does not refund transaction tip due to security reasons.
#[derive(
	CloneNoBound,
	Decode,
	DefaultNoBound,
	Encode,
	EqNoBound,
	PartialEqNoBound,
	RuntimeDebugNoBound,
	TypeInfo,
)]
#[scale_info(skip_type_params(RT, GI, PI, MI, PID, LID, FEE))]
pub struct RefundRelayerForMessagesFromParachain<RT, GI, PI, MI, PID, LID, FEE>(
	PhantomData<(RT, GI, PI, MI, PID, LID, FEE)>,
);

impl<R, GI, PI, MI, PID, LID, FEE>
	RefundRelayerForMessagesFromParachain<R, GI, PI, MI, PID, LID, FEE>
where
	R: UtilityConfig<RuntimeCall = CallOf<R>>,
	CallOf<R>: IsSubType<CallableCallFor<UtilityPallet<R>, R>>,
{
	fn expand_call<'a>(&self, call: &'a CallOf<R>) -> Option<Vec<&'a CallOf<R>>> {
		let calls = match call.is_sub_type() {
			Some(UtilityCall::<R>::batch_all { ref calls }) => {
				if calls.len() > 3 {
					return None
				}

				calls.iter().collect()
			},
			Some(_) => return None,
			None => vec![call],
		};

		Some(calls)
	}
}

/// Data that is crafted in `pre_dispatch` method and used at `post_dispatch`.
#[derive(PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct PreDispatchData<AccountId> {
	/// Transaction submitter (relayer) account.
	relayer: AccountId,
	/// Type of the call.
	pub call_type: CallType,
}

/// Type of the call that the extension recognizes.
#[derive(Clone, Copy, PartialEq, RuntimeDebugNoBound)]
pub enum CallType {
	/// Relay chain finality + parachain finality + message delivery calls.
	AllFinalityAndDelivery(RelayBlockNumber, SubmitParachainHeadsInfo, ReceiveMessagesProofInfo),
	/// Parachain finality + message delivery calls.
	ParachainFinalityAndDelivery(SubmitParachainHeadsInfo, ReceiveMessagesProofInfo),
	/// Standalone message delivery call.
	Delivery(ReceiveMessagesProofInfo),
}

impl CallType {
	/// Returns the pre-dispatch messages pallet state.
	fn receive_messages_proof_info(&self) -> ReceiveMessagesProofInfo {
		match *self {
			Self::AllFinalityAndDelivery(_, _, info) => info,
			Self::ParachainFinalityAndDelivery(_, info) => info,
			Self::Delivery(info) => info,
		}
	}
}

// without this typedef rustfmt fails with internal err
type BalanceOf<R> =
	<<R as TransactionPaymentConfig>::OnChargeTransaction as OnChargeTransaction<R>>::Balance;
type CallOf<R> = <R as frame_system::Config>::RuntimeCall;

impl<R, GI, PI, MI, PID, LID, FEE> SignedExtension
	for RefundRelayerForMessagesFromParachain<R, GI, PI, MI, PID, LID, FEE>
where
	R: 'static
		+ Send
		+ Sync
		+ UtilityConfig<RuntimeCall = CallOf<R>>
		+ BoundedBridgeGrandpaConfig<GI>
		+ ParachainsConfig<PI, BridgesGrandpaPalletInstance = GI>
		+ MessagesConfig<MI>
		+ RelayersConfig,
	GI: 'static + Send + Sync,
	PI: 'static + Send + Sync,
	MI: 'static + Send + Sync,
	PID: 'static + Send + Sync + Get<u32>,
	LID: 'static + Send + Sync + Get<LaneId>,
	FEE: 'static + Send + Sync + TransactionFeeCalculation<<R as RelayersConfig>::Reward>,
	CallOf<R>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ IsSubType<CallableCallFor<UtilityPallet<R>, R>>
		+ GrandpaCallSubType<R, GI>
		+ ParachainsCallSubType<R, PI>
		+ MessagesCallSubType<R, MI>,
{
	const IDENTIFIER: &'static str = "RefundRelayerForMessagesFromParachain";
	type AccountId = R::AccountId;
	type Call = CallOf<R>;
	type AdditionalSigned = ();
	type Pre = Option<PreDispatchData<R::AccountId>>;

	fn additional_signed(&self) -> Result<(), TransactionValidityError> {
		Ok(())
	}

	fn validate(
		&self,
		_who: &Self::AccountId,
		call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> TransactionValidity {
		let calls = match self.expand_call(call) {
			Some(calls) => calls,
			None => return Ok(ValidTransaction::default()),
		};

		for call in calls {
			call.check_obsolete_submit_finality_proof()?;
			call.check_obsolete_submit_parachain_heads()?;
			call.check_obsolete_receive_messages_proof()?;
		}

		Ok(ValidTransaction::default())
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		call: &Self::Call,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		// reject batch transactions with obsolete headers
		self.validate(who, call, info, len).map(drop)?;

		// Try to check if the tx matches one of types we support.
		let parse_call_type = || {
			let mut calls = self.expand_call(call)?.into_iter();
			match calls.len() {
				3 => Some(CallType::AllFinalityAndDelivery(
					calls.next()?.submit_finality_proof_info()?,
					calls.next()?.submit_parachain_heads_info_for(PID::get())?,
					calls.next()?.receive_messages_proof_info_for(LID::get())?,
				)),
				2 => Some(CallType::ParachainFinalityAndDelivery(
					calls.next()?.submit_parachain_heads_info_for(PID::get())?,
					calls.next()?.receive_messages_proof_info_for(LID::get())?,
				)),
				1 => Some(CallType::Delivery(
					calls.next()?.receive_messages_proof_info_for(LID::get())?,
				)),
				_ => None,
			}
		};

		Ok(parse_call_type().map(|call_type| {
			log::trace!(
				target: "runtime::bridge",
				"RefundRelayerForMessagesFromParachain from parachain {} via {:?} \
				parsed bridge transaction in pre-dispatch: {:?}",
				PID::get(),
				LID::get(),
				call_type,
			);
			PreDispatchData { relayer: who.clone(), call_type }
		}))
	}

	fn post_dispatch(
		pre: Option<Self::Pre>,
		info: &DispatchInfoOf<Self::Call>,
		post_info: &PostDispatchInfoOf<Self::Call>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		// We don't refund anything if the transaction has failed.
		if result.is_err() {
			return Ok(())
		}

		// We don't refund anything for transactions that we don't support.
		let (relayer, call_type) = match pre {
			Some(Some(pre)) => (pre.relayer, pre.call_type),
			_ => return Ok(()),
		};

		// check if relay chain state has been updated
		if let CallType::AllFinalityAndDelivery(relay_block_number, _, _) = call_type {
			if !SubmitFinalityProofHelper::<R, GI>::was_successful(relay_block_number) {
				// we only refund relayer if all calls have updated chain state
				return Ok(())
			}

			// there's a conflict between how bridge GRANDPA pallet works and a `utility.batchAll`
			// transaction. If relay chain header is mandatory, the GRANDPA pallet returns
			// `Pays::No`, because such transaction is mandatory for operating the bridge. But
			// `utility.batchAll` transaction always requires payment. But in both cases we'll
			// refund relayer - either explicitly here, or using `Pays::No` if he's choosing
			// to submit dedicated transaction.
		}

		// check if parachain state has been updated
		match call_type {
			CallType::AllFinalityAndDelivery(_, parachain_heads_info, _) |
			CallType::ParachainFinalityAndDelivery(parachain_heads_info, _) => {
				if !SubmitParachainHeadsHelper::<R, PI>::was_successful(&parachain_heads_info) {
					// we only refund relayer if all calls have updated chain state
					return Ok(())
				}
			},
			_ => (),
		}

		// Check if the `ReceiveMessagesProof` call delivered at least some of the messages that
		// it contained. If this happens, we consider the transaction "helpful" and refund it.
		let messages_proof_info = call_type.receive_messages_proof_info();
		if !ReceiveMessagesProofHelper::<R, MI>::was_partially_successful(&messages_proof_info) {
			return Ok(())
		}

		// regarding the tip - refund that happens here (at this side of the bridge) isn't the whole
		// relayer compensation. He'll receive some amount at the other side of the bridge. It shall
		// (in theory) cover the tip here. Otherwise, if we'll be compensating tip here, some
		// malicious relayer may use huge tips, effectively depleting account that pay rewards. The
		// cost of this attack is nothing. Hence we use zero as tip here.
		let tip = Zero::zero();

		// compute the relayer reward
		let reward = FEE::compute_fee(info, post_info, len, tip);

		// finally - register reward in relayers pallet
		RelayersPallet::<R>::register_relayer_reward(LID::get(), &relayer, reward);

		log::trace!(
			target: "runtime::bridge",
			"RefundRelayerForMessagesFromParachain from parachain {} via {:?} has registered {:?} reward: {:?}",
			PID::get(),
			LID::get(),
			relayer,
			reward,
		);

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{messages::target::FromBridgedChainMessagesProof, mock::*};
	use bp_messages::{InboundLaneData, MessageNonce};
	use bp_parachains::{BestParaHeadHash, ParaInfo};
	use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
	use bp_runtime::HeaderId;
	use bp_test_utils::make_default_justification;
	use frame_support::{assert_storage_noop, parameter_types, weights::Weight};
	use pallet_bridge_grandpa::Call as GrandpaCall;
	use pallet_bridge_messages::Call as MessagesCall;
	use pallet_bridge_parachains::{Call as ParachainsCall, RelayBlockHash};
	use sp_runtime::{
		traits::Header as HeaderT, transaction_validity::InvalidTransaction, DispatchError,
	};

	parameter_types! {
		pub TestParachain: u32 = 1000;
		pub TestLaneId: LaneId = TEST_LANE_ID;
	}

	type TestExtension = RefundRelayerForMessagesFromParachain<
		TestRuntime,
		(),
		(),
		(),
		TestParachain,
		TestLaneId,
		TestRuntime,
	>;

	fn relayer_account_at_this_chain() -> ThisChainAccountId {
		0
	}

	fn relayer_account_at_bridged_chain() -> BridgedChainAccountId {
		0
	}

	fn initialize_environment(
		best_relay_header_number: RelayBlockNumber,
		parachain_head_at_relay_header_number: RelayBlockNumber,
		parachain_head_hash: ParaHash,
		best_delivered_message: MessageNonce,
	) {
		let best_relay_header = HeaderId(best_relay_header_number, RelayBlockHash::default());
		pallet_bridge_grandpa::BestFinalized::<TestRuntime>::put(best_relay_header);

		let para_id = ParaId(TestParachain::get());
		let para_info = ParaInfo {
			best_head_hash: BestParaHeadHash {
				at_relay_block_number: parachain_head_at_relay_header_number,
				head_hash: parachain_head_hash,
			},
			next_imported_hash_position: 0,
		};
		pallet_bridge_parachains::ParasInfo::<TestRuntime>::insert(para_id, para_info);

		let lane_id = TestLaneId::get();
		let lane_data =
			InboundLaneData { last_confirmed_nonce: best_delivered_message, ..Default::default() };
		pallet_bridge_messages::InboundLanes::<TestRuntime>::insert(lane_id, lane_data);
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
			parachains: vec![(ParaId(TestParachain::get()), [1u8; 32].into())],
			parachain_heads_proof: ParaHeadsProof(vec![]),
		})
	}

	fn message_delivery_call(best_message: MessageNonce) -> RuntimeCall {
		RuntimeCall::BridgeMessages(MessagesCall::receive_messages_proof {
			relayer_id_at_bridged_chain: relayer_account_at_bridged_chain(),
			proof: FromBridgedChainMessagesProof {
				bridged_header_hash: Default::default(),
				storage_proof: vec![],
				lane: TestLaneId::get(),
				nonces_start: best_message,
				nonces_end: best_message,
			},
			messages_count: 1,
			dispatch_weight: Weight::zero(),
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

	fn all_finality_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_type: CallType::AllFinalityAndDelivery(
				200,
				SubmitParachainHeadsInfo {
					at_relay_block_number: 200,
					para_id: ParaId(TestParachain::get()),
					para_head_hash: [1u8; 32].into(),
				},
				ReceiveMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					best_proof_nonce: 200,
					best_stored_nonce: 100,
				},
			),
		}
	}

	fn parachain_finality_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_type: CallType::ParachainFinalityAndDelivery(
				SubmitParachainHeadsInfo {
					at_relay_block_number: 200,
					para_id: ParaId(TestParachain::get()),
					para_head_hash: [1u8; 32].into(),
				},
				ReceiveMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					best_proof_nonce: 200,
					best_stored_nonce: 100,
				},
			),
		}
	}

	fn delivery_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_type: CallType::Delivery(ReceiveMessagesProofInfo {
				lane_id: TEST_LANE_ID,
				best_proof_nonce: 200,
				best_stored_nonce: 100,
			}),
		}
	}

	fn run_test(test: impl FnOnce()) {
		sp_io::TestExternalities::new(Default::default()).execute_with(test)
	}

	fn run_validate(call: RuntimeCall) -> TransactionValidity {
		let extension: TestExtension = RefundRelayerForMessagesFromParachain(PhantomData);
		extension.validate(&relayer_account_at_this_chain(), &call, &DispatchInfo::default(), 0)
	}

	fn run_pre_dispatch(
		call: RuntimeCall,
	) -> Result<Option<PreDispatchData<ThisChainAccountId>>, TransactionValidityError> {
		let extension: TestExtension = RefundRelayerForMessagesFromParachain(PhantomData);
		extension.pre_dispatch(&relayer_account_at_this_chain(), &call, &DispatchInfo::default(), 0)
	}

	fn dispatch_info() -> DispatchInfo {
		DispatchInfo {
			weight: Weight::from_ref_time(
				frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND,
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
			Some(pre_dispatch_data),
			&dispatch_info(),
			&post_dispatch_info(),
			1024,
			&dispatch_result,
		);
		assert_eq!(post_dispatch_result, Ok(()));
	}

	fn expected_reward() -> ThisChainBalance {
		pallet_transaction_payment::Pallet::<TestRuntime>::compute_actual_fee(
			1024,
			&dispatch_info(),
			&post_dispatch_info(),
			Zero::zero(),
		)
	}

	#[test]
	fn validate_allows_non_obsolete_transactions() {
		run_test(|| {
			initialize_environment(100, 100, Default::default(), 100);

			assert_eq!(run_validate(message_delivery_call(200)), Ok(ValidTransaction::default()),);

			assert_eq!(
				run_validate(parachain_finality_and_delivery_batch_call(200, 200)),
				Ok(ValidTransaction::default()),
			);

			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call(200, 200, 200)),
				Ok(ValidTransaction::default()),
			);
		});
	}

	#[test]
	fn ext_rejects_batch_with_obsolete_relay_chain_header() {
		run_test(|| {
			initialize_environment(100, 100, Default::default(), 100);

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
			initialize_environment(100, 100, Default::default(), 100);

			assert_eq!(
				run_pre_dispatch(all_finality_and_delivery_batch_call(101, 100, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);

			assert_eq!(
				run_pre_dispatch(parachain_finality_and_delivery_batch_call(100, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);

			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call(101, 100, 200)),
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
			initialize_environment(100, 100, Default::default(), 100);

			assert_eq!(
				run_pre_dispatch(all_finality_and_delivery_batch_call(200, 200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);

			assert_eq!(
				run_pre_dispatch(parachain_finality_and_delivery_batch_call(200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);

			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call(200, 200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);

			assert_eq!(
				run_validate(parachain_finality_and_delivery_batch_call(200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
		});
	}

	#[test]
	fn pre_dispatch_parses_batch_with_relay_chain_and_parachain_headers() {
		run_test(|| {
			initialize_environment(100, 100, Default::default(), 100);

			assert_eq!(
				run_pre_dispatch(all_finality_and_delivery_batch_call(200, 200, 200)),
				Ok(Some(all_finality_pre_dispatch_data())),
			);
		});
	}

	#[test]
	fn pre_dispatch_parses_batch_with_parachain_header() {
		run_test(|| {
			initialize_environment(100, 100, Default::default(), 100);

			assert_eq!(
				run_pre_dispatch(parachain_finality_and_delivery_batch_call(200, 200)),
				Ok(Some(parachain_finality_pre_dispatch_data())),
			);
		});
	}

	#[test]
	fn pre_dispatch_fails_to_parse_batch_with_multiple_parachain_headers() {
		run_test(|| {
			initialize_environment(100, 100, Default::default(), 100);

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
	fn pre_dispatch_parses_message_delivery_transaction() {
		run_test(|| {
			initialize_environment(100, 100, Default::default(), 100);

			assert_eq!(
				run_pre_dispatch(message_delivery_call(200)),
				Ok(Some(delivery_pre_dispatch_data())),
			);
		});
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
			initialize_environment(100, 200, Default::default(), 200);

			assert_storage_noop!(run_post_dispatch(Some(all_finality_pre_dispatch_data()), Ok(())));
		});
	}

	#[test]
	fn post_dispatch_ignores_transaction_that_has_not_updated_parachain_state() {
		run_test(|| {
			initialize_environment(200, 100, Default::default(), 200);

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
			initialize_environment(200, 200, Default::default(), 100);

			assert_storage_noop!(run_post_dispatch(Some(all_finality_pre_dispatch_data()), Ok(())));
			assert_storage_noop!(run_post_dispatch(
				Some(parachain_finality_pre_dispatch_data()),
				Ok(())
			));
			assert_storage_noop!(run_post_dispatch(Some(delivery_pre_dispatch_data()), Ok(())));
		});
	}

	#[test]
	fn post_dispatch_refunds_relayer_in_all_finality_batch() {
		run_test(|| {
			initialize_environment(200, 200, [1u8; 32].into(), 200);

			run_post_dispatch(Some(all_finality_pre_dispatch_data()), Ok(()));
			assert_eq!(
				RelayersPallet::<TestRuntime>::relayer_reward(
					relayer_account_at_this_chain(),
					TestLaneId::get()
				),
				Some(expected_reward()),
			);
		});
	}

	#[test]
	fn post_dispatch_refunds_relayer_in_parachain_finality_batch() {
		run_test(|| {
			initialize_environment(200, 200, [1u8; 32].into(), 200);

			run_post_dispatch(Some(parachain_finality_pre_dispatch_data()), Ok(()));
			assert_eq!(
				RelayersPallet::<TestRuntime>::relayer_reward(
					relayer_account_at_this_chain(),
					TestLaneId::get()
				),
				Some(expected_reward()),
			);
		});
	}

	#[test]
	fn post_dispatch_refunds_relayer_in_message_delivery_transaction() {
		run_test(|| {
			initialize_environment(200, 200, Default::default(), 200);

			run_post_dispatch(Some(delivery_pre_dispatch_data()), Ok(()));
			assert_eq!(
				RelayersPallet::<TestRuntime>::relayer_reward(
					relayer_account_at_this_chain(),
					TestLaneId::get()
				),
				Some(expected_reward()),
			);
		});
	}
}
