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
	CallHelper as MessagesCallHelper, CallInfo as MessagesCallInfo, MessagesCallSubType,
};
use bp_messages::LaneId;
use bp_relayers::{RewardsAccountOwner, RewardsAccountParams};
use bp_runtime::StaticStrProvider;
use codec::{Decode, Encode};
use frame_support::{
	dispatch::{CallableCallFor, DispatchInfo, Dispatchable, PostDispatchInfo},
	traits::IsSubType,
	weights::Weight,
	CloneNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use pallet_bridge_grandpa::{
	CallSubType as GrandpaCallSubType, SubmitFinalityProofHelper, SubmitFinalityProofInfo,
};
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

// without this typedef rustfmt fails with internal err
type BalanceOf<R> =
	<<R as TransactionPaymentConfig>::OnChargeTransaction as OnChargeTransaction<R>>::Balance;
type CallOf<R> = <R as frame_system::Config>::RuntimeCall;

/// Trait identifying a bridged parachain. A relayer might be refunded for delivering messages
/// coming from this parachain.
trait RefundableParachainId {
	/// The instance of the bridge parachains pallet.
	type Instance;
	/// The parachain Id.
	type Id: Get<u32>;
}

/// Default implementation of `RefundableParachainId`.
pub struct RefundableParachain<Instance, Id>(PhantomData<(Instance, Id)>);

impl<Instance, Id> RefundableParachainId for RefundableParachain<Instance, Id>
where
	Id: Get<u32>,
{
	type Instance = Instance;
	type Id = Id;
}

/// Trait identifying a bridged messages lane. A relayer might be refunded for delivering messages
/// coming from this lane.
trait RefundableMessagesLaneId {
	/// The instance of the bridge messages pallet.
	type Instance;
	/// The messages lane id.
	type Id: Get<LaneId>;
}

/// Default implementation of `RefundableMessagesLaneId`.
pub struct RefundableMessagesLane<Instance, Id>(PhantomData<(Instance, Id)>);

impl<Instance, Id> RefundableMessagesLaneId for RefundableMessagesLane<Instance, Id>
where
	Id: Get<LaneId>,
{
	type Instance = Instance;
	type Id = Id;
}

/// Refund calculator.
pub trait RefundCalculator {
	// The underlying integer type in which the refund is calculated.
	type Balance;

	/// Compute refund for given transaction.
	fn compute_refund(
		info: &DispatchInfo,
		post_info: &PostDispatchInfo,
		len: usize,
		tip: Self::Balance,
	) -> Self::Balance;
}

/// `RefundCalculator` implementation which refunds the actual transaction fee.
pub struct ActualFeeRefund<R>(PhantomData<R>);

impl<R> RefundCalculator for ActualFeeRefund<R>
where
	R: TransactionPaymentConfig,
	CallOf<R>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	BalanceOf<R>: FixedPointOperand,
{
	type Balance = BalanceOf<R>;

	fn compute_refund(
		info: &DispatchInfo,
		post_info: &PostDispatchInfo,
		len: usize,
		tip: BalanceOf<R>,
	) -> BalanceOf<R> {
		pallet_transaction_payment::Pallet::<R>::compute_actual_fee(len as _, info, post_info, tip)
	}
}

/// Data that is crafted in `pre_dispatch` method and used at `post_dispatch`.
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct PreDispatchData<AccountId> {
	/// Transaction submitter (relayer) account.
	relayer: AccountId,
	/// Type of the call.
	call_info: CallInfo,
}

/// Type of the call that the extension recognizes.
#[derive(RuntimeDebugNoBound, PartialEq)]
pub enum CallInfo {
	/// Relay chain finality + parachain finality + message delivery/confirmation calls.
	AllFinalityAndMsgs(
		SubmitFinalityProofInfo<RelayBlockNumber>,
		SubmitParachainHeadsInfo,
		MessagesCallInfo,
	),
	/// Parachain finality + message delivery/confirmation calls.
	ParachainFinalityAndMsgs(SubmitParachainHeadsInfo, MessagesCallInfo),
	/// Standalone message delivery/confirmation call.
	Msgs(MessagesCallInfo),
}

impl CallInfo {
	/// Returns the pre-dispatch `finality_target` sent to the `SubmitFinalityProof` call.
	fn submit_finality_proof_info(&self) -> Option<SubmitFinalityProofInfo<RelayBlockNumber>> {
		match *self {
			Self::AllFinalityAndMsgs(info, _, _) => Some(info),
			_ => None,
		}
	}

	/// Returns the pre-dispatch `SubmitParachainHeadsInfo`.
	fn submit_parachain_heads_info(&self) -> Option<&SubmitParachainHeadsInfo> {
		match self {
			Self::AllFinalityAndMsgs(_, info, _) => Some(info),
			Self::ParachainFinalityAndMsgs(info, _) => Some(info),
			_ => None,
		}
	}

	/// Returns the pre-dispatch `ReceiveMessagesProofInfo`.
	fn messages_call_info(&self) -> &MessagesCallInfo {
		match self {
			Self::AllFinalityAndMsgs(_, _, info) => info,
			Self::ParachainFinalityAndMsgs(_, info) => info,
			Self::Msgs(info) => info,
		}
	}
}

/// Signed extension that refunds a relayer for new messages coming from a parachain.
///
/// Also refunds relayer for successful finality delivery if it comes in batch (`utility.batchAll`)
/// with message delivery transaction. Batch may deliver either both relay chain header and
/// parachain head, or just parachain head. Corresponding headers must be used in messages
/// proof verification.
///
/// Extension does not refund transaction tip due to security reasons.
#[derive(
	DefaultNoBound,
	CloneNoBound,
	Decode,
	Encode,
	EqNoBound,
	PartialEqNoBound,
	RuntimeDebugNoBound,
	TypeInfo,
)]
#[scale_info(skip_type_params(Runtime, Para, Msgs, Refund, Id))]
pub struct RefundBridgedParachainMessages<Runtime, Para, Msgs, Refund, Id>(
	PhantomData<(Runtime, Para, Msgs, Refund, Id)>,
);

impl<Runtime, Para, Msgs, Refund, Id>
	RefundBridgedParachainMessages<Runtime, Para, Msgs, Refund, Id>
where
	Runtime: UtilityConfig<RuntimeCall = CallOf<Runtime>>,
	CallOf<Runtime>: IsSubType<CallableCallFor<UtilityPallet<Runtime>, Runtime>>,
{
	fn expand_call<'a>(&self, call: &'a CallOf<Runtime>) -> Option<Vec<&'a CallOf<Runtime>>> {
		let calls = match call.is_sub_type() {
			Some(UtilityCall::<Runtime>::batch_all { ref calls }) => {
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

impl<Runtime, Para, Msgs, Refund, Id> SignedExtension
	for RefundBridgedParachainMessages<Runtime, Para, Msgs, Refund, Id>
where
	Self: 'static + Send + Sync,
	Runtime: UtilityConfig<RuntimeCall = CallOf<Runtime>>
		+ BoundedBridgeGrandpaConfig<Runtime::BridgesGrandpaPalletInstance>
		+ ParachainsConfig<Para::Instance>
		+ MessagesConfig<Msgs::Instance>
		+ RelayersConfig,
	Para: RefundableParachainId,
	Msgs: RefundableMessagesLaneId,
	Refund: RefundCalculator<Balance = Runtime::Reward>,
	Id: StaticStrProvider,
	CallOf<Runtime>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ IsSubType<CallableCallFor<UtilityPallet<Runtime>, Runtime>>
		+ GrandpaCallSubType<Runtime, Runtime::BridgesGrandpaPalletInstance>
		+ ParachainsCallSubType<Runtime, Para::Instance>
		+ MessagesCallSubType<Runtime, Msgs::Instance>,
{
	const IDENTIFIER: &'static str = Id::STR;
	type AccountId = Runtime::AccountId;
	type Call = CallOf<Runtime>;
	type AdditionalSigned = ();
	type Pre = Option<PreDispatchData<Runtime::AccountId>>;

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
		if let Some(calls) = self.expand_call(call) {
			for nested_call in calls {
				nested_call.check_obsolete_submit_finality_proof()?;
				nested_call.check_obsolete_submit_parachain_heads()?;
				nested_call.check_obsolete_call()?;
			}
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
		let parse_call = || {
			let mut calls = self.expand_call(call)?.into_iter();
			match calls.len() {
				3 => Some(CallInfo::AllFinalityAndMsgs(
					calls.next()?.submit_finality_proof_info()?,
					calls.next()?.submit_parachain_heads_info_for(Para::Id::get())?,
					calls.next()?.call_info_for(Msgs::Id::get())?,
				)),
				2 => Some(CallInfo::ParachainFinalityAndMsgs(
					calls.next()?.submit_parachain_heads_info_for(Para::Id::get())?,
					calls.next()?.call_info_for(Msgs::Id::get())?,
				)),
				1 => Some(CallInfo::Msgs(calls.next()?.call_info_for(Msgs::Id::get())?)),
				_ => None,
			}
		};

		Ok(parse_call().map(|call_info| {
			log::trace!(
				target: "runtime::bridge",
				"{} from parachain {} via {:?} parsed bridge transaction in pre-dispatch: {:?}",
				Self::IDENTIFIER,
				Para::Id::get(),
				Msgs::Id::get(),
				call_info,
			);
			PreDispatchData { relayer: who.clone(), call_info }
		}))
	}

	fn post_dispatch(
		pre: Option<Self::Pre>,
		info: &DispatchInfoOf<Self::Call>,
		post_info: &PostDispatchInfoOf<Self::Call>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		let mut extra_weight = Weight::zero();
		let mut extra_size = 0;

		// We don't refund anything if the transaction has failed.
		if result.is_err() {
			return Ok(())
		}

		// We don't refund anything for transactions that we don't support.
		let (relayer, call_info) = match pre {
			Some(Some(pre)) => (pre.relayer, pre.call_info),
			_ => return Ok(()),
		};

		// check if relay chain state has been updated
		if let Some(finality_proof_info) = call_info.submit_finality_proof_info() {
			if !SubmitFinalityProofHelper::<Runtime, Runtime::BridgesGrandpaPalletInstance>::was_successful(
				finality_proof_info.block_number,
			) {
				// we only refund relayer if all calls have updated chain state
				return Ok(())
			}

			// there's a conflict between how bridge GRANDPA pallet works and a `utility.batchAll`
			// transaction. If relay chain header is mandatory, the GRANDPA pallet returns
			// `Pays::No`, because such transaction is mandatory for operating the bridge. But
			// `utility.batchAll` transaction always requires payment. But in both cases we'll
			// refund relayer - either explicitly here, or using `Pays::No` if he's choosing
			// to submit dedicated transaction.

			// submitter has means to include extra weight/bytes in the `submit_finality_proof`
			// call, so let's subtract extra weight/size to avoid refunding for this extra stuff
			extra_weight = finality_proof_info.extra_weight;
			extra_size = finality_proof_info.extra_size;
		}

		// check if parachain state has been updated
		if let Some(para_proof_info) = call_info.submit_parachain_heads_info() {
			if !SubmitParachainHeadsHelper::<Runtime, Para::Instance>::was_successful(
				para_proof_info,
			) {
				// we only refund relayer if all calls have updated chain state
				return Ok(())
			}
		}

		// Check if the `ReceiveMessagesProof` call delivered at least some of the messages that
		// it contained. If this happens, we consider the transaction "helpful" and refund it.
		let msgs_call_info = call_info.messages_call_info();
		if !MessagesCallHelper::<Runtime, Msgs::Instance>::was_partially_successful(msgs_call_info)
		{
			return Ok(())
		}

		// regarding the tip - refund that happens here (at this side of the bridge) isn't the whole
		// relayer compensation. He'll receive some amount at the other side of the bridge. It shall
		// (in theory) cover the tip there. Otherwise, if we'll be compensating tip here, some
		// malicious relayer may use huge tips, effectively depleting account that pay rewards. The
		// cost of this attack is nothing. Hence we use zero as tip here.
		let tip = Zero::zero();

		// decrease post-dispatch weight/size using extra weight/size that we know now
		let post_info_len = len.saturating_sub(extra_size as usize);
		let mut post_info = *post_info;
		post_info.actual_weight =
			Some(post_info.actual_weight.unwrap_or(info.weight).saturating_sub(extra_weight));

		// compute the relayer refund
		let refund = Refund::compute_refund(info, &post_info, post_info_len, tip);

		// finally - register refund in relayers pallet
		let rewards_account_owner = match msgs_call_info {
			MessagesCallInfo::ReceiveMessagesProof(_) => RewardsAccountOwner::ThisChain,
			MessagesCallInfo::ReceiveMessagesDeliveryProof(_) => RewardsAccountOwner::BridgedChain,
		};
		RelayersPallet::<Runtime>::register_relayer_reward(
			RewardsAccountParams::new(
				Msgs::Id::get(),
				Runtime::BridgedChainId::get(),
				rewards_account_owner,
			),
			&relayer,
			refund,
		);

		log::trace!(
			target: "runtime::bridge",
			"{} from parachain {} via {:?} has registered reward: {:?} for {:?}",
			Self::IDENTIFIER,
			Para::Id::get(),
			Msgs::Id::get(),
			refund,
			relayer,
		);

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		messages::{
			source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		},
		messages_call_ext::{
			BaseMessagesProofInfo, ReceiveMessagesDeliveryProofInfo, ReceiveMessagesProofInfo,
		},
		mock::*,
	};
	use bp_messages::{InboundLaneData, MessageNonce, OutboundLaneData, UnrewardedRelayersState};
	use bp_parachains::{BestParaHeadHash, ParaInfo};
	use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
	use bp_runtime::HeaderId;
	use bp_test_utils::{make_default_justification, test_keyring};
	use frame_support::{assert_storage_noop, parameter_types, weights::Weight};
	use pallet_bridge_grandpa::{Call as GrandpaCall, StoredAuthoritySet};
	use pallet_bridge_messages::Call as MessagesCall;
	use pallet_bridge_parachains::{Call as ParachainsCall, RelayBlockHash};
	use sp_runtime::{
		traits::Header as HeaderT, transaction_validity::InvalidTransaction, DispatchError,
	};

	parameter_types! {
		TestParachain: u32 = 1000;
		pub TestLaneId: LaneId = TEST_LANE_ID;
		pub MsgProofsRewardsAccount: RewardsAccountParams = RewardsAccountParams::new(TEST_LANE_ID, TEST_BRIDGED_CHAIN_ID, RewardsAccountOwner::ThisChain);
		pub MsgDeliveryProofsRewardsAccount: RewardsAccountParams = RewardsAccountParams::new(TEST_LANE_ID, TEST_BRIDGED_CHAIN_ID, RewardsAccountOwner::BridgedChain);
	}

	bp_runtime::generate_static_str_provider!(TestExtension);
	type TestExtension = RefundBridgedParachainMessages<
		TestRuntime,
		RefundableParachain<(), TestParachain>,
		RefundableMessagesLane<(), TestLaneId>,
		ActualFeeRefund<TestRuntime>,
		StrTestExtension,
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
				head_hash: parachain_head_hash,
			},
			next_imported_hash_position: 0,
		};
		pallet_bridge_parachains::ParasInfo::<TestRuntime>::insert(para_id, para_info);

		let lane_id = TestLaneId::get();
		let in_lane_data =
			InboundLaneData { last_confirmed_nonce: best_message, ..Default::default() };
		pallet_bridge_messages::InboundLanes::<TestRuntime>::insert(lane_id, in_lane_data);

		let out_lane_data =
			OutboundLaneData { latest_received_nonce: best_message, ..Default::default() };
		pallet_bridge_messages::OutboundLanes::<TestRuntime>::insert(lane_id, out_lane_data);
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

	fn message_confirmation_call(best_message: MessageNonce) -> RuntimeCall {
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
					para_head_hash: [1u8; 32].into(),
				},
				MessagesCallInfo::ReceiveMessagesProof(ReceiveMessagesProofInfo(
					BaseMessagesProofInfo {
						lane_id: TEST_LANE_ID,
						best_bundled_nonce: 200,
						best_stored_nonce: 100,
					},
				)),
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
					para_head_hash: [1u8; 32].into(),
				},
				MessagesCallInfo::ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo(
					BaseMessagesProofInfo {
						lane_id: TEST_LANE_ID,
						best_bundled_nonce: 200,
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
					para_head_hash: [1u8; 32].into(),
				},
				MessagesCallInfo::ReceiveMessagesProof(ReceiveMessagesProofInfo(
					BaseMessagesProofInfo {
						lane_id: TEST_LANE_ID,
						best_bundled_nonce: 200,
						best_stored_nonce: 100,
					},
				)),
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
					para_head_hash: [1u8; 32].into(),
				},
				MessagesCallInfo::ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo(
					BaseMessagesProofInfo {
						lane_id: TEST_LANE_ID,
						best_bundled_nonce: 200,
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
				ReceiveMessagesProofInfo(BaseMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					best_bundled_nonce: 200,
					best_stored_nonce: 100,
				}),
			)),
		}
	}

	fn confirmation_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: CallInfo::Msgs(MessagesCallInfo::ReceiveMessagesDeliveryProof(
				ReceiveMessagesDeliveryProofInfo(BaseMessagesProofInfo {
					lane_id: TEST_LANE_ID,
					best_bundled_nonce: 200,
					best_stored_nonce: 100,
				}),
			)),
		}
	}

	fn run_test(test: impl FnOnce()) {
		sp_io::TestExternalities::new(Default::default()).execute_with(test)
	}

	fn run_validate(call: RuntimeCall) -> TransactionValidity {
		let extension: TestExtension = RefundBridgedParachainMessages(PhantomData);
		extension.validate(&relayer_account_at_this_chain(), &call, &DispatchInfo::default(), 0)
	}

	fn run_pre_dispatch(
		call: RuntimeCall,
	) -> Result<Option<PreDispatchData<ThisChainAccountId>>, TransactionValidityError> {
		let extension: TestExtension = RefundBridgedParachainMessages(PhantomData);
		extension.pre_dispatch(&relayer_account_at_this_chain(), &call, &DispatchInfo::default(), 0)
	}

	fn dispatch_info() -> DispatchInfo {
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
				run_validate(message_confirmation_call(200)),
				Ok(ValidTransaction::default()),
			);

			assert_eq!(
				run_validate(parachain_finality_and_delivery_batch_call(200, 200)),
				Ok(ValidTransaction::default()),
			);
			assert_eq!(
				run_validate(parachain_finality_and_confirmation_batch_call(200, 200)),
				Ok(ValidTransaction::default()),
			);

			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call(200, 200, 200)),
				Ok(ValidTransaction::default()),
			);
			assert_eq!(
				run_validate(all_finality_and_confirmation_batch_call(200, 200, 200)),
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
			initialize_environment(100, 100, Default::default(), 100);

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
	fn pre_dispatch_parses_batch_with_relay_chain_and_parachain_headers() {
		run_test(|| {
			initialize_environment(100, 100, Default::default(), 100);

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
			initialize_environment(100, 100, Default::default(), 100);

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
	fn pre_dispatch_parses_message_transaction() {
		run_test(|| {
			initialize_environment(100, 100, Default::default(), 100);

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
			initialize_environment(200, 200, [1u8; 32].into(), 200);

			let mut dispatch_info = dispatch_info();
			dispatch_info.weight = Weight::from_parts(
				frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND * 2,
				0,
			);

			// without any size/weight refund: we expect regular reward
			let pre_dispatch_data = all_finality_pre_dispatch_data();
			let regular_reward = expected_reward();
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
					info.extra_weight.set_ref_time(
						frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND,
					);
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
			initialize_environment(200, 200, [1u8; 32].into(), 200);

			run_post_dispatch(Some(all_finality_pre_dispatch_data()), Ok(()));
			assert_eq!(
				RelayersPallet::<TestRuntime>::relayer_reward(
					relayer_account_at_this_chain(),
					MsgProofsRewardsAccount::get()
				),
				Some(expected_reward()),
			);

			run_post_dispatch(Some(all_finality_confirmation_pre_dispatch_data()), Ok(()));
			assert_eq!(
				RelayersPallet::<TestRuntime>::relayer_reward(
					relayer_account_at_this_chain(),
					MsgDeliveryProofsRewardsAccount::get()
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
					MsgProofsRewardsAccount::get()
				),
				Some(expected_reward()),
			);

			run_post_dispatch(Some(parachain_finality_confirmation_pre_dispatch_data()), Ok(()));
			assert_eq!(
				RelayersPallet::<TestRuntime>::relayer_reward(
					relayer_account_at_this_chain(),
					MsgDeliveryProofsRewardsAccount::get()
				),
				Some(expected_reward()),
			);
		});
	}

	#[test]
	fn post_dispatch_refunds_relayer_in_message_transaction() {
		run_test(|| {
			initialize_environment(200, 200, Default::default(), 200);

			run_post_dispatch(Some(delivery_pre_dispatch_data()), Ok(()));
			assert_eq!(
				RelayersPallet::<TestRuntime>::relayer_reward(
					relayer_account_at_this_chain(),
					MsgProofsRewardsAccount::get()
				),
				Some(expected_reward()),
			);

			run_post_dispatch(Some(confirmation_pre_dispatch_data()), Ok(()));
			assert_eq!(
				RelayersPallet::<TestRuntime>::relayer_reward(
					relayer_account_at_this_chain(),
					MsgDeliveryProofsRewardsAccount::get()
				),
				Some(expected_reward()),
			);
		});
	}
}
