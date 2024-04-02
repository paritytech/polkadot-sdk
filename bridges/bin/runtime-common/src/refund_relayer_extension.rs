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

//! Signed extension that refunds relayer if he has delivered some new messages.
//! It also refunds transaction cost if the transaction is an `utility.batchAll()`
//! with calls that are: delivering new message and all necessary underlying headers
//! (parachain or relay chain).

use crate::messages_call_ext::{
	CallHelper as MessagesCallHelper, CallInfo as MessagesCallInfo, MessagesCallSubType,
};
use bp_messages::{LaneId, MessageNonce};
use bp_relayers::{RewardsAccountOwner, RewardsAccountParams};
use bp_runtime::{Chain, Parachain, ParachainIdOf, RangeInclusiveExt, StaticStrProvider};
use codec::{Codec, Decode, Encode};
use frame_support::{
	dispatch::{CallableCallFor, DispatchInfo, PostDispatchInfo},
	traits::IsSubType,
	weights::Weight,
	CloneNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use pallet_bridge_grandpa::{
	CallSubType as GrandpaCallSubType, Config as GrandpaConfig, SubmitFinalityProofHelper,
	SubmitFinalityProofInfo,
};
use pallet_bridge_messages::Config as MessagesConfig;
use pallet_bridge_parachains::{
	BoundedBridgeGrandpaConfig, CallSubType as ParachainsCallSubType, Config as ParachainsConfig,
	RelayBlockNumber, SubmitParachainHeadsHelper, SubmitParachainHeadsInfo,
};
use pallet_bridge_relayers::{
	Config as RelayersConfig, Pallet as RelayersPallet, WeightInfoExt as _,
};
use pallet_transaction_payment::{Config as TransactionPaymentConfig, OnChargeTransaction};
use pallet_utility::{Call as UtilityCall, Config as UtilityConfig, Pallet as UtilityPallet};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, Get, PostDispatchInfoOf, SignedExtension, Zero},
	transaction_validity::{
		TransactionPriority, TransactionValidity, TransactionValidityError, ValidTransactionBuilder,
	},
	DispatchResult, FixedPointOperand, RuntimeDebug,
};
use sp_std::{marker::PhantomData, vec, vec::Vec};

type AccountIdOf<R> = <R as frame_system::Config>::AccountId;
// without this typedef rustfmt fails with internal err
type BalanceOf<R> =
	<<R as TransactionPaymentConfig>::OnChargeTransaction as OnChargeTransaction<R>>::Balance;
type CallOf<R> = <R as frame_system::Config>::RuntimeCall;

/// Trait identifying a bridged parachain. A relayer might be refunded for delivering messages
/// coming from this parachain.
pub trait RefundableParachainId {
	/// The instance of the bridge parachains pallet.
	type Instance;
	/// The parachain Id.
	type Id: Get<u32>;
}

/// Default implementation of `RefundableParachainId`.
pub struct DefaultRefundableParachainId<Instance, Id>(PhantomData<(Instance, Id)>);

impl<Instance, Id> RefundableParachainId for DefaultRefundableParachainId<Instance, Id>
where
	Id: Get<u32>,
{
	type Instance = Instance;
	type Id = Id;
}

/// Implementation of `RefundableParachainId` for `trait Parachain`.
pub struct RefundableParachain<Instance, Para>(PhantomData<(Instance, Para)>);

impl<Instance, Para> RefundableParachainId for RefundableParachain<Instance, Para>
where
	Para: Parachain,
{
	type Instance = Instance;
	type Id = ParachainIdOf<Para>;
}

/// Trait identifying a bridged messages lane. A relayer might be refunded for delivering messages
/// coming from this lane.
pub trait RefundableMessagesLaneId {
	/// The instance of the bridge messages pallet.
	type Instance: 'static;
	/// The messages lane id.
	type Id: Get<LaneId>;
}

/// Default implementation of `RefundableMessagesLaneId`.
pub struct RefundableMessagesLane<Instance, Id>(PhantomData<(Instance, Id)>);

impl<Instance, Id> RefundableMessagesLaneId for RefundableMessagesLane<Instance, Id>
where
	Instance: 'static,
	Id: Get<LaneId>,
{
	type Instance = Instance;
	type Id = Id;
}

/// Refund calculator.
pub trait RefundCalculator {
	/// The underlying integer type in which the refund is calculated.
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
	/// Relay chain finality + message delivery/confirmation calls.
	RelayFinalityAndMsgs(SubmitFinalityProofInfo<RelayBlockNumber>, MessagesCallInfo),
	/// Parachain finality + message delivery/confirmation calls.
	///
	/// This variant is used only when bridging with parachain.
	ParachainFinalityAndMsgs(SubmitParachainHeadsInfo, MessagesCallInfo),
	/// Standalone message delivery/confirmation call.
	Msgs(MessagesCallInfo),
}

impl CallInfo {
	/// Returns true if call is a message delivery call (with optional finality calls).
	fn is_receive_messages_proof_call(&self) -> bool {
		match self.messages_call_info() {
			MessagesCallInfo::ReceiveMessagesProof(_) => true,
			MessagesCallInfo::ReceiveMessagesDeliveryProof(_) => false,
		}
	}

	/// Returns the pre-dispatch `finality_target` sent to the `SubmitFinalityProof` call.
	fn submit_finality_proof_info(&self) -> Option<SubmitFinalityProofInfo<RelayBlockNumber>> {
		match *self {
			Self::AllFinalityAndMsgs(info, _, _) => Some(info),
			Self::RelayFinalityAndMsgs(info, _) => Some(info),
			_ => None,
		}
	}

	/// Returns mutable reference to pre-dispatch `finality_target` sent to the
	/// `SubmitFinalityProof` call.
	#[cfg(test)]
	fn submit_finality_proof_info_mut(
		&mut self,
	) -> Option<&mut SubmitFinalityProofInfo<RelayBlockNumber>> {
		match *self {
			Self::AllFinalityAndMsgs(ref mut info, _, _) => Some(info),
			Self::RelayFinalityAndMsgs(ref mut info, _) => Some(info),
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
			Self::RelayFinalityAndMsgs(_, info) => info,
			Self::ParachainFinalityAndMsgs(_, info) => info,
			Self::Msgs(info) => info,
		}
	}
}

/// The actions on relayer account that need to be performed because of his actions.
#[derive(RuntimeDebug, PartialEq)]
pub enum RelayerAccountAction<AccountId, Reward> {
	/// Do nothing with relayer account.
	None,
	/// Reward the relayer.
	Reward(AccountId, RewardsAccountParams, Reward),
	/// Slash the relayer.
	Slash(AccountId, RewardsAccountParams),
}

/// Everything common among our refund signed extensions.
pub trait RefundSignedExtension:
	'static + Clone + Codec + sp_std::fmt::Debug + Default + Eq + PartialEq + Send + Sync + TypeInfo
where
	<Self::Runtime as GrandpaConfig<Self::GrandpaInstance>>::BridgedChain:
		Chain<BlockNumber = RelayBlockNumber>,
{
	/// This chain runtime.
	type Runtime: UtilityConfig<RuntimeCall = CallOf<Self::Runtime>>
		+ GrandpaConfig<Self::GrandpaInstance>
		+ MessagesConfig<<Self::Msgs as RefundableMessagesLaneId>::Instance>
		+ RelayersConfig;
	/// Grandpa pallet reference.
	type GrandpaInstance: 'static;
	/// Messages pallet and lane reference.
	type Msgs: RefundableMessagesLaneId;
	/// Refund amount calculator.
	type Refund: RefundCalculator<Balance = <Self::Runtime as RelayersConfig>::Reward>;
	/// Priority boost calculator.
	type Priority: Get<TransactionPriority>;
	/// Signed extension unique identifier.
	type Id: StaticStrProvider;

	/// Unpack batch runtime call.
	fn expand_call(call: &CallOf<Self::Runtime>) -> Vec<&CallOf<Self::Runtime>>;

	/// Given runtime call, check if it has supported format. Additionally, check if any of
	/// (optionally batched) calls are obsolete and we shall reject the transaction.
	fn parse_and_check_for_obsolete_call(
		call: &CallOf<Self::Runtime>,
	) -> Result<Option<CallInfo>, TransactionValidityError>;

	/// Check if parsed call is already obsolete.
	fn check_obsolete_parsed_call(
		call: &CallOf<Self::Runtime>,
	) -> Result<&CallOf<Self::Runtime>, TransactionValidityError>;

	/// Called from post-dispatch and shall perform additional checks (apart from relay
	/// chain finality and messages transaction finality) of given call result.
	fn additional_call_result_check(
		relayer: &AccountIdOf<Self::Runtime>,
		call_info: &CallInfo,
	) -> bool;

	/// Given post-dispatch information, analyze the outcome of relayer call and return
	/// actions that need to be performed on relayer account.
	fn analyze_call_result(
		pre: Option<Option<PreDispatchData<AccountIdOf<Self::Runtime>>>>,
		info: &DispatchInfo,
		post_info: &PostDispatchInfo,
		len: usize,
		result: &DispatchResult,
	) -> RelayerAccountAction<AccountIdOf<Self::Runtime>, <Self::Runtime as RelayersConfig>::Reward>
	{
		let mut extra_weight = Weight::zero();
		let mut extra_size = 0;

		// We don't refund anything for transactions that we don't support.
		let (relayer, call_info) = match pre {
			Some(Some(pre)) => (pre.relayer, pre.call_info),
			_ => return RelayerAccountAction::None,
		};

		// now we know that the relayer either needs to be rewarded, or slashed
		// => let's prepare the correspondent account that pays reward/receives slashed amount
		let reward_account_params =
			RewardsAccountParams::new(
				<Self::Msgs as RefundableMessagesLaneId>::Id::get(),
				<Self::Runtime as MessagesConfig<
					<Self::Msgs as RefundableMessagesLaneId>::Instance,
				>>::BridgedChainId::get(),
				if call_info.is_receive_messages_proof_call() {
					RewardsAccountOwner::ThisChain
				} else {
					RewardsAccountOwner::BridgedChain
				},
			);

		// prepare return value for the case if the call has failed or it has not caused
		// expected side effects (e.g. not all messages have been accepted)
		//
		// we are not checking if relayer is registered here - it happens during the slash attempt
		//
		// there are couple of edge cases here:
		//
		// - when the relayer becomes registered during message dispatch: this is unlikely + relayer
		//   should be ready for slashing after registration;
		//
		// - when relayer is registered after `validate` is called and priority is not boosted:
		//   relayer should be ready for slashing after registration.
		let may_slash_relayer =
			Self::bundled_messages_for_priority_boost(Some(&call_info)).is_some();
		let slash_relayer_if_delivery_result = may_slash_relayer
			.then(|| RelayerAccountAction::Slash(relayer.clone(), reward_account_params))
			.unwrap_or(RelayerAccountAction::None);

		// We don't refund anything if the transaction has failed.
		if let Err(e) = result {
			log::trace!(
				target: "runtime::bridge",
				"{} via {:?}: relayer {:?} has submitted invalid messages transaction: {:?}",
				Self::Id::STR,
				<Self::Msgs as RefundableMessagesLaneId>::Id::get(),
				relayer,
				e,
			);
			return slash_relayer_if_delivery_result
		}

		// check if relay chain state has been updated
		if let Some(finality_proof_info) = call_info.submit_finality_proof_info() {
			if !SubmitFinalityProofHelper::<Self::Runtime, Self::GrandpaInstance>::was_successful(
				finality_proof_info.block_number,
			) {
				// we only refund relayer if all calls have updated chain state
				log::trace!(
					target: "runtime::bridge",
					"{} via {:?}: relayer {:?} has submitted invalid relay chain finality proof",
					Self::Id::STR,
					<Self::Msgs as RefundableMessagesLaneId>::Id::get(),
					relayer,
				);
				return slash_relayer_if_delivery_result
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

		// Check if the `ReceiveMessagesProof` call delivered at least some of the messages that
		// it contained. If this happens, we consider the transaction "helpful" and refund it.
		let msgs_call_info = call_info.messages_call_info();
		if !MessagesCallHelper::<Self::Runtime, <Self::Msgs as RefundableMessagesLaneId>::Instance>::was_successful(msgs_call_info) {
			log::trace!(
				target: "runtime::bridge",
				"{} via {:?}: relayer {:?} has submitted invalid messages call",
				Self::Id::STR,
				<Self::Msgs as RefundableMessagesLaneId>::Id::get(),
				relayer,
			);
			return slash_relayer_if_delivery_result
		}

		// do additional check
		if !Self::additional_call_result_check(&relayer, &call_info) {
			return slash_relayer_if_delivery_result
		}

		// regarding the tip - refund that happens here (at this side of the bridge) isn't the whole
		// relayer compensation. He'll receive some amount at the other side of the bridge. It shall
		// (in theory) cover the tip there. Otherwise, if we'll be compensating tip here, some
		// malicious relayer may use huge tips, effectively depleting account that pay rewards. The
		// cost of this attack is nothing. Hence we use zero as tip here.
		let tip = Zero::zero();

		// decrease post-dispatch weight/size using extra weight/size that we know now
		let post_info_len = len.saturating_sub(extra_size as usize);
		let mut post_info_weight =
			post_info.actual_weight.unwrap_or(info.weight).saturating_sub(extra_weight);

		// let's also replace the weight of slashing relayer with the weight of rewarding relayer
		if call_info.is_receive_messages_proof_call() {
			post_info_weight = post_info_weight.saturating_sub(
				<Self::Runtime as RelayersConfig>::WeightInfo::extra_weight_of_successful_receive_messages_proof_call(),
			);
		}

		// compute the relayer refund
		let mut post_info = *post_info;
		post_info.actual_weight = Some(post_info_weight);
		let refund = Self::Refund::compute_refund(info, &post_info, post_info_len, tip);

		// we can finally reward relayer
		RelayerAccountAction::Reward(relayer, reward_account_params, refund)
	}

	/// Returns number of bundled messages `Some(_)`, if the given call info is a:
	///
	/// - message delivery transaction;
	///
	/// - with reasonable bundled messages that may be accepted by the messages pallet.
	///
	/// This function is used to check whether the transaction priority should be
	/// virtually boosted. The relayer registration (we only boost priority for registered
	/// relayer transactions) must be checked outside.
	fn bundled_messages_for_priority_boost(call_info: Option<&CallInfo>) -> Option<MessageNonce> {
		// we only boost priority of message delivery transactions
		let parsed_call = match call_info {
			Some(parsed_call) if parsed_call.is_receive_messages_proof_call() => parsed_call,
			_ => return None,
		};

		// compute total number of messages in transaction
		let bundled_messages = parsed_call.messages_call_info().bundled_messages().saturating_len();

		// a quick check to avoid invalid high-priority transactions
		let max_unconfirmed_messages_in_confirmation_tx = <Self::Runtime as MessagesConfig<
			<Self::Msgs as RefundableMessagesLaneId>::Instance,
		>>::MaxUnconfirmedMessagesAtInboundLane::get(
		);
		if bundled_messages > max_unconfirmed_messages_in_confirmation_tx {
			return None
		}

		Some(bundled_messages)
	}
}

/// Adapter that allow implementing `sp_runtime::traits::SignedExtension` for any
/// `RefundSignedExtension`.
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
pub struct RefundSignedExtensionAdapter<T: RefundSignedExtension>(T)
where
	<T::Runtime as GrandpaConfig<T::GrandpaInstance>>::BridgedChain:
		Chain<BlockNumber = RelayBlockNumber>;

impl<T: RefundSignedExtension> SignedExtension for RefundSignedExtensionAdapter<T>
where
	<T::Runtime as GrandpaConfig<T::GrandpaInstance>>::BridgedChain:
		Chain<BlockNumber = RelayBlockNumber>,
	CallOf<T::Runtime>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ IsSubType<CallableCallFor<UtilityPallet<T::Runtime>, T::Runtime>>
		+ GrandpaCallSubType<T::Runtime, T::GrandpaInstance>
		+ MessagesCallSubType<T::Runtime, <T::Msgs as RefundableMessagesLaneId>::Instance>,
{
	const IDENTIFIER: &'static str = T::Id::STR;
	type AccountId = AccountIdOf<T::Runtime>;
	type Call = CallOf<T::Runtime>;
	type AdditionalSigned = ();
	type Pre = Option<PreDispatchData<AccountIdOf<T::Runtime>>>;

	fn additional_signed(&self) -> Result<(), TransactionValidityError> {
		Ok(())
	}

	fn validate(
		&self,
		who: &Self::AccountId,
		call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> TransactionValidity {
		// this is the only relevant line of code for the `pre_dispatch`
		//
		// we're not calling `validate` from `pre_dispatch` directly because of performance
		// reasons, so if you're adding some code that may fail here, please check if it needs
		// to be added to the `pre_dispatch` as well
		let parsed_call = T::parse_and_check_for_obsolete_call(call)?;

		// the following code just plays with transaction priority and never returns an error

		// we only boost priority of presumably correct message delivery transactions
		let bundled_messages = match T::bundled_messages_for_priority_boost(parsed_call.as_ref()) {
			Some(bundled_messages) => bundled_messages,
			None => return Ok(Default::default()),
		};

		// we only boost priority if relayer has staked required balance
		if !RelayersPallet::<T::Runtime>::is_registration_active(who) {
			return Ok(Default::default())
		}

		// compute priority boost
		let priority_boost =
			crate::priority_calculator::compute_priority_boost::<T::Priority>(bundled_messages);
		let valid_transaction = ValidTransactionBuilder::default().priority(priority_boost);

		log::trace!(
			target: "runtime::bridge",
			"{} via {:?} has boosted priority of message delivery transaction \
			of relayer {:?}: {} messages -> {} priority",
			Self::IDENTIFIER,
			<T::Msgs as RefundableMessagesLaneId>::Id::get(),
			who,
			bundled_messages,
			priority_boost,
		);

		valid_transaction.build()
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		// this is a relevant piece of `validate` that we need here (in `pre_dispatch`)
		let parsed_call = T::parse_and_check_for_obsolete_call(call)?;

		Ok(parsed_call.map(|call_info| {
			log::trace!(
				target: "runtime::bridge",
				"{} via {:?} parsed bridge transaction in pre-dispatch: {:?}",
				Self::IDENTIFIER,
				<T::Msgs as RefundableMessagesLaneId>::Id::get(),
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
		let call_result = T::analyze_call_result(pre, info, post_info, len, result);

		match call_result {
			RelayerAccountAction::None => (),
			RelayerAccountAction::Reward(relayer, reward_account, reward) => {
				RelayersPallet::<T::Runtime>::register_relayer_reward(
					reward_account,
					&relayer,
					reward,
				);

				log::trace!(
					target: "runtime::bridge",
					"{} via {:?} has registered reward: {:?} for {:?}",
					Self::IDENTIFIER,
					<T::Msgs as RefundableMessagesLaneId>::Id::get(),
					reward,
					relayer,
				);
			},
			RelayerAccountAction::Slash(relayer, slash_account) =>
				RelayersPallet::<T::Runtime>::slash_and_deregister(&relayer, slash_account),
		}

		Ok(())
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
#[scale_info(skip_type_params(Runtime, Para, Msgs, Refund, Priority, Id))]
pub struct RefundBridgedParachainMessages<Runtime, Para, Msgs, Refund, Priority, Id>(
	PhantomData<(
		// runtime with `frame-utility`, `pallet-bridge-grandpa`, `pallet-bridge-parachains`,
		// `pallet-bridge-messages` and `pallet-bridge-relayers` pallets deployed
		Runtime,
		// implementation of `RefundableParachainId` trait, which specifies the instance of
		// the used `pallet-bridge-parachains` pallet and the bridged parachain id
		Para,
		// implementation of `RefundableMessagesLaneId` trait, which specifies the instance of
		// the used `pallet-bridge-messages` pallet and the lane within this pallet
		Msgs,
		// implementation of the `RefundCalculator` trait, that is used to compute refund that
		// we give to relayer for his transaction
		Refund,
		// getter for per-message `TransactionPriority` boost that we give to message
		// delivery transactions
		Priority,
		// the runtime-unique identifier of this signed extension
		Id,
	)>,
);

impl<Runtime, Para, Msgs, Refund, Priority, Id> RefundSignedExtension
	for RefundBridgedParachainMessages<Runtime, Para, Msgs, Refund, Priority, Id>
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
	Priority: Get<TransactionPriority>,
	Id: StaticStrProvider,
	CallOf<Runtime>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ IsSubType<CallableCallFor<UtilityPallet<Runtime>, Runtime>>
		+ GrandpaCallSubType<Runtime, Runtime::BridgesGrandpaPalletInstance>
		+ ParachainsCallSubType<Runtime, Para::Instance>
		+ MessagesCallSubType<Runtime, Msgs::Instance>,
{
	type Runtime = Runtime;
	type GrandpaInstance = Runtime::BridgesGrandpaPalletInstance;
	type Msgs = Msgs;
	type Refund = Refund;
	type Priority = Priority;
	type Id = Id;

	fn expand_call(call: &CallOf<Runtime>) -> Vec<&CallOf<Runtime>> {
		match call.is_sub_type() {
			Some(UtilityCall::<Runtime>::batch_all { ref calls }) if calls.len() <= 3 =>
				calls.iter().collect(),
			Some(_) => vec![],
			None => vec![call],
		}
	}

	fn parse_and_check_for_obsolete_call(
		call: &CallOf<Runtime>,
	) -> Result<Option<CallInfo>, TransactionValidityError> {
		let calls = Self::expand_call(call);
		let total_calls = calls.len();
		let mut calls = calls.into_iter().map(Self::check_obsolete_parsed_call).rev();

		let msgs_call = calls.next().transpose()?.and_then(|c| c.call_info_for(Msgs::Id::get()));
		let para_finality_call = calls
			.next()
			.transpose()?
			.and_then(|c| c.submit_parachain_heads_info_for(Para::Id::get()));
		let relay_finality_call =
			calls.next().transpose()?.and_then(|c| c.submit_finality_proof_info());

		Ok(match (total_calls, relay_finality_call, para_finality_call, msgs_call) {
			(3, Some(relay_finality_call), Some(para_finality_call), Some(msgs_call)) => Some(
				CallInfo::AllFinalityAndMsgs(relay_finality_call, para_finality_call, msgs_call),
			),
			(2, None, Some(para_finality_call), Some(msgs_call)) =>
				Some(CallInfo::ParachainFinalityAndMsgs(para_finality_call, msgs_call)),
			(1, None, None, Some(msgs_call)) => Some(CallInfo::Msgs(msgs_call)),
			_ => None,
		})
	}

	fn check_obsolete_parsed_call(
		call: &CallOf<Runtime>,
	) -> Result<&CallOf<Runtime>, TransactionValidityError> {
		call.check_obsolete_submit_finality_proof()?;
		call.check_obsolete_submit_parachain_heads()?;
		call.check_obsolete_call()?;
		Ok(call)
	}

	fn additional_call_result_check(relayer: &Runtime::AccountId, call_info: &CallInfo) -> bool {
		// check if parachain state has been updated
		if let Some(para_proof_info) = call_info.submit_parachain_heads_info() {
			if !SubmitParachainHeadsHelper::<Runtime, Para::Instance>::was_successful(
				para_proof_info,
			) {
				// we only refund relayer if all calls have updated chain state
				log::trace!(
					target: "runtime::bridge",
					"{} from parachain {} via {:?}: relayer {:?} has submitted invalid parachain finality proof",
					Id::STR,
					Para::Id::get(),
					Msgs::Id::get(),
					relayer,
				);
				return false
			}
		}

		true
	}
}

/// Signed extension that refunds a relayer for new messages coming from a standalone (GRANDPA)
/// chain.
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
#[scale_info(skip_type_params(Runtime, GrandpaInstance, Msgs, Refund, Priority, Id))]
pub struct RefundBridgedGrandpaMessages<Runtime, GrandpaInstance, Msgs, Refund, Priority, Id>(
	PhantomData<(
		// runtime with `frame-utility`, `pallet-bridge-grandpa`,
		// `pallet-bridge-messages` and `pallet-bridge-relayers` pallets deployed
		Runtime,
		// bridge GRANDPA pallet instance, used to track bridged chain state
		GrandpaInstance,
		// implementation of `RefundableMessagesLaneId` trait, which specifies the instance of
		// the used `pallet-bridge-messages` pallet and the lane within this pallet
		Msgs,
		// implementation of the `RefundCalculator` trait, that is used to compute refund that
		// we give to relayer for his transaction
		Refund,
		// getter for per-message `TransactionPriority` boost that we give to message
		// delivery transactions
		Priority,
		// the runtime-unique identifier of this signed extension
		Id,
	)>,
);

impl<Runtime, GrandpaInstance, Msgs, Refund, Priority, Id> RefundSignedExtension
	for RefundBridgedGrandpaMessages<Runtime, GrandpaInstance, Msgs, Refund, Priority, Id>
where
	Self: 'static + Send + Sync,
	Runtime: UtilityConfig<RuntimeCall = CallOf<Runtime>>
		+ BoundedBridgeGrandpaConfig<GrandpaInstance>
		+ MessagesConfig<Msgs::Instance>
		+ RelayersConfig,
	GrandpaInstance: 'static,
	Msgs: RefundableMessagesLaneId,
	Refund: RefundCalculator<Balance = Runtime::Reward>,
	Priority: Get<TransactionPriority>,
	Id: StaticStrProvider,
	CallOf<Runtime>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ IsSubType<CallableCallFor<UtilityPallet<Runtime>, Runtime>>
		+ GrandpaCallSubType<Runtime, GrandpaInstance>
		+ MessagesCallSubType<Runtime, Msgs::Instance>,
{
	type Runtime = Runtime;
	type GrandpaInstance = GrandpaInstance;
	type Msgs = Msgs;
	type Refund = Refund;
	type Priority = Priority;
	type Id = Id;

	fn expand_call(call: &CallOf<Runtime>) -> Vec<&CallOf<Runtime>> {
		match call.is_sub_type() {
			Some(UtilityCall::<Runtime>::batch_all { ref calls }) if calls.len() <= 2 =>
				calls.iter().collect(),
			Some(_) => vec![],
			None => vec![call],
		}
	}

	fn parse_and_check_for_obsolete_call(
		call: &CallOf<Runtime>,
	) -> Result<Option<CallInfo>, TransactionValidityError> {
		let calls = Self::expand_call(call);
		let total_calls = calls.len();
		let mut calls = calls.into_iter().map(Self::check_obsolete_parsed_call).rev();

		let msgs_call = calls.next().transpose()?.and_then(|c| c.call_info_for(Msgs::Id::get()));
		let relay_finality_call =
			calls.next().transpose()?.and_then(|c| c.submit_finality_proof_info());

		Ok(match (total_calls, relay_finality_call, msgs_call) {
			(2, Some(relay_finality_call), Some(msgs_call)) =>
				Some(CallInfo::RelayFinalityAndMsgs(relay_finality_call, msgs_call)),
			(1, None, Some(msgs_call)) => Some(CallInfo::Msgs(msgs_call)),
			_ => None,
		})
	}

	fn check_obsolete_parsed_call(
		call: &CallOf<Runtime>,
	) -> Result<&CallOf<Runtime>, TransactionValidityError> {
		call.check_obsolete_submit_finality_proof()?;
		call.check_obsolete_call()?;
		Ok(call)
	}

	fn additional_call_result_check(_relayer: &Runtime::AccountId, _call_info: &CallInfo) -> bool {
		true
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
			UnrewardedRelayerOccupation,
		},
		mock::*,
	};
	use bp_messages::{
		DeliveredMessages, InboundLaneData, MessageNonce, MessagesOperatingMode, OutboundLaneData,
		UnrewardedRelayer, UnrewardedRelayersState,
	};
	use bp_parachains::{BestParaHeadHash, ParaInfo};
	use bp_polkadot_core::parachains::{ParaHeadsProof, ParaId};
	use bp_runtime::{BasicOperatingMode, HeaderId};
	use bp_test_utils::{make_default_justification, test_keyring, TEST_GRANDPA_SET_ID};
	use frame_support::{
		assert_storage_noop, parameter_types,
		traits::{fungible::Mutate, ReservableCurrency},
		weights::Weight,
	};
	use pallet_bridge_grandpa::{Call as GrandpaCall, Pallet as GrandpaPallet, StoredAuthoritySet};
	use pallet_bridge_messages::{Call as MessagesCall, Pallet as MessagesPallet};
	use pallet_bridge_parachains::{
		Call as ParachainsCall, Pallet as ParachainsPallet, RelayBlockHash,
	};
	use sp_runtime::{
		traits::{ConstU64, Header as HeaderT},
		transaction_validity::{InvalidTransaction, ValidTransaction},
		DispatchError,
	};

	parameter_types! {
		TestParachain: u32 = 1000;
		pub TestLaneId: LaneId = TEST_LANE_ID;
		pub MsgProofsRewardsAccount: RewardsAccountParams = RewardsAccountParams::new(
			TEST_LANE_ID,
			TEST_BRIDGED_CHAIN_ID,
			RewardsAccountOwner::ThisChain,
		);
		pub MsgDeliveryProofsRewardsAccount: RewardsAccountParams = RewardsAccountParams::new(
			TEST_LANE_ID,
			TEST_BRIDGED_CHAIN_ID,
			RewardsAccountOwner::BridgedChain,
		);
	}

	bp_runtime::generate_static_str_provider!(TestExtension);

	type TestGrandpaExtensionProvider = RefundBridgedGrandpaMessages<
		TestRuntime,
		(),
		RefundableMessagesLane<(), TestLaneId>,
		ActualFeeRefund<TestRuntime>,
		ConstU64<1>,
		StrTestExtension,
	>;
	type TestGrandpaExtension = RefundSignedExtensionAdapter<TestGrandpaExtensionProvider>;
	type TestExtensionProvider = RefundBridgedParachainMessages<
		TestRuntime,
		DefaultRefundableParachainId<(), TestParachain>,
		RefundableMessagesLane<(), TestLaneId>,
		ActualFeeRefund<TestRuntime>,
		ConstU64<1>,
		StrTestExtension,
	>;
	type TestExtension = RefundSignedExtensionAdapter<TestExtensionProvider>;

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

	fn initialize_environment(
		best_relay_header_number: RelayBlockNumber,
		parachain_head_at_relay_header_number: RelayBlockNumber,
		best_message: MessageNonce,
	) {
		let authorities = test_keyring().into_iter().map(|(a, w)| (a.into(), w)).collect();
		let best_relay_header = HeaderId(best_relay_header_number, RelayBlockHash::default());
		pallet_bridge_grandpa::CurrentAuthoritySet::<TestRuntime>::put(
			StoredAuthoritySet::try_new(authorities, TEST_GRANDPA_SET_ID).unwrap(),
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
		let in_lane_data =
			InboundLaneData { last_confirmed_nonce: best_message, ..Default::default() };
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

	fn submit_relay_header_call_ex(relay_header_number: RelayBlockNumber) -> RuntimeCall {
		let relay_header = BridgedChainHeader::new(
			relay_header_number,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		let relay_justification = make_default_justification(&relay_header);

		RuntimeCall::BridgeGrandpa(GrandpaCall::submit_finality_proof_ex {
			finality_target: Box::new(relay_header),
			justification: relay_justification,
			current_set_id: TEST_GRANDPA_SET_ID,
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
			parachain_heads_proof: ParaHeadsProof { storage_proof: vec![] },
		})
	}

	fn message_delivery_call(best_message: MessageNonce) -> RuntimeCall {
		RuntimeCall::BridgeMessages(MessagesCall::receive_messages_proof {
			relayer_id_at_bridged_chain: relayer_account_at_bridged_chain(),
			proof: FromBridgedChainMessagesProof {
				bridged_header_hash: Default::default(),
				storage_proof: vec![],
				lane: TestLaneId::get(),
				nonces_start: pallet_bridge_messages::InboundLanes::<TestRuntime>::get(
					TEST_LANE_ID,
				)
				.last_delivered_nonce() +
					1,
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

	fn relay_finality_and_delivery_batch_call_ex(
		relay_header_number: RelayBlockNumber,
		best_message: MessageNonce,
	) -> RuntimeCall {
		RuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![
				submit_relay_header_call_ex(relay_header_number),
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

	fn relay_finality_and_confirmation_batch_call_ex(
		relay_header_number: RelayBlockNumber,
		best_message: MessageNonce,
	) -> RuntimeCall {
		RuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![
				submit_relay_header_call_ex(relay_header_number),
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

	fn all_finality_and_delivery_batch_call_ex(
		relay_header_number: RelayBlockNumber,
		parachain_head_at_relay_header_number: RelayBlockNumber,
		best_message: MessageNonce,
	) -> RuntimeCall {
		RuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![
				submit_relay_header_call_ex(relay_header_number),
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

	fn all_finality_and_confirmation_batch_call_ex(
		relay_header_number: RelayBlockNumber,
		parachain_head_at_relay_header_number: RelayBlockNumber,
		best_message: MessageNonce,
	) -> RuntimeCall {
		RuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![
				submit_relay_header_call_ex(relay_header_number),
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
					current_set_id: None,
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

	fn all_finality_pre_dispatch_data_ex() -> PreDispatchData<ThisChainAccountId> {
		let mut data = all_finality_pre_dispatch_data();
		data.call_info.submit_finality_proof_info_mut().unwrap().current_set_id =
			Some(TEST_GRANDPA_SET_ID);
		data
	}

	fn all_finality_confirmation_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: CallInfo::AllFinalityAndMsgs(
				SubmitFinalityProofInfo {
					block_number: 200,
					current_set_id: None,
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

	fn all_finality_confirmation_pre_dispatch_data_ex() -> PreDispatchData<ThisChainAccountId> {
		let mut data = all_finality_confirmation_pre_dispatch_data();
		data.call_info.submit_finality_proof_info_mut().unwrap().current_set_id =
			Some(TEST_GRANDPA_SET_ID);
		data
	}

	fn relay_finality_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: CallInfo::RelayFinalityAndMsgs(
				SubmitFinalityProofInfo {
					block_number: 200,
					current_set_id: None,
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

	fn relay_finality_pre_dispatch_data_ex() -> PreDispatchData<ThisChainAccountId> {
		let mut data = relay_finality_pre_dispatch_data();
		data.call_info.submit_finality_proof_info_mut().unwrap().current_set_id =
			Some(TEST_GRANDPA_SET_ID);
		data
	}

	fn relay_finality_confirmation_pre_dispatch_data() -> PreDispatchData<ThisChainAccountId> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: CallInfo::RelayFinalityAndMsgs(
				SubmitFinalityProofInfo {
					block_number: 200,
					current_set_id: None,
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

	fn relay_finality_confirmation_pre_dispatch_data_ex() -> PreDispatchData<ThisChainAccountId> {
		let mut data = relay_finality_confirmation_pre_dispatch_data();
		data.call_info.submit_finality_proof_info_mut().unwrap().current_set_id =
			Some(TEST_GRANDPA_SET_ID);
		data
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
			RefundSignedExtensionAdapter(RefundBridgedParachainMessages(PhantomData));
		extension.validate(&relayer_account_at_this_chain(), &call, &DispatchInfo::default(), 0)
	}

	fn run_grandpa_validate(call: RuntimeCall) -> TransactionValidity {
		let extension: TestGrandpaExtension =
			RefundSignedExtensionAdapter(RefundBridgedGrandpaMessages(PhantomData));
		extension.validate(&relayer_account_at_this_chain(), &call, &DispatchInfo::default(), 0)
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
			RefundSignedExtensionAdapter(RefundBridgedParachainMessages(PhantomData));
		extension.pre_dispatch(&relayer_account_at_this_chain(), &call, &DispatchInfo::default(), 0)
	}

	fn run_grandpa_pre_dispatch(
		call: RuntimeCall,
	) -> Result<Option<PreDispatchData<ThisChainAccountId>>, TransactionValidityError> {
		let extension: TestGrandpaExtension =
			RefundSignedExtensionAdapter(RefundBridgedGrandpaMessages(PhantomData));
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

	fn expected_delivery_reward() -> ThisChainBalance {
		let mut post_dispatch_info = post_dispatch_info();
		let extra_weight = <TestRuntime as RelayersConfig>::WeightInfo::extra_weight_of_successful_receive_messages_proof_call();
		post_dispatch_info.actual_weight =
			Some(dispatch_info().weight.saturating_sub(extra_weight));
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
			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call_ex(200, 200, 200)),
				Ok(Default::default()),
			);
			// message confirmation validation is passing
			assert_eq!(
				run_validate_ignore_priority(message_confirmation_call(200)),
				Ok(Default::default()),
			);
			assert_eq!(
				run_validate_ignore_priority(parachain_finality_and_confirmation_batch_call(
					200, 200
				)),
				Ok(Default::default()),
			);
			assert_eq!(
				run_validate_ignore_priority(all_finality_and_confirmation_batch_call(
					200, 200, 200
				)),
				Ok(Default::default()),
			);
			assert_eq!(
				run_validate_ignore_priority(all_finality_and_confirmation_batch_call_ex(
					200, 200, 200
				)),
				Ok(Default::default()),
			);
		});
	}

	#[test]
	fn validate_boosts_priority_of_message_delivery_transactions() {
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
			assert_eq!(
				priority_of_100_messages_confirmation,
				priority_of_200_messages_confirmation
			);
		});
	}

	#[test]
	fn validate_does_not_boost_priority_of_message_delivery_transactions_with_too_many_messages() {
		run_test(|| {
			initialize_environment(100, 100, 100);

			BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
				.unwrap();

			let priority_of_max_messages_delivery = run_validate(message_delivery_call(
				100 + MaxUnconfirmedMessagesAtInboundLane::get(),
			))
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
				run_validate_ignore_priority(parachain_finality_and_confirmation_batch_call(
					200, 200
				)),
				Ok(ValidTransaction::default()),
			);

			assert_eq!(
				run_validate_ignore_priority(all_finality_and_delivery_batch_call(200, 200, 200)),
				Ok(ValidTransaction::default()),
			);
			assert_eq!(
				run_validate_ignore_priority(all_finality_and_delivery_batch_call_ex(
					200, 200, 200
				)),
				Ok(ValidTransaction::default()),
			);
			assert_eq!(
				run_validate_ignore_priority(all_finality_and_confirmation_batch_call(
					200, 200, 200
				)),
				Ok(ValidTransaction::default()),
			);
			assert_eq!(
				run_validate_ignore_priority(all_finality_and_confirmation_batch_call_ex(
					200, 200, 200
				)),
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
				run_pre_dispatch(all_finality_and_delivery_batch_call_ex(100, 200, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);

			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call(100, 200, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call_ex(100, 200, 200)),
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
				run_pre_dispatch(all_finality_and_delivery_batch_call_ex(101, 100, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call(101, 100, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call_ex(101, 100, 200)),
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
				run_pre_dispatch(all_finality_and_delivery_batch_call_ex(200, 200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_pre_dispatch(all_finality_and_confirmation_batch_call(200, 200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_pre_dispatch(all_finality_and_confirmation_batch_call_ex(200, 200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);

			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call(200, 200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_validate(all_finality_and_delivery_batch_call_ex(200, 200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_validate(all_finality_and_confirmation_batch_call(200, 200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_validate(all_finality_and_confirmation_batch_call_ex(200, 200, 100)),
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
				run_pre_dispatch(all_finality_and_delivery_batch_call_ex(200, 200, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
			);
			assert_eq!(
				run_pre_dispatch(all_finality_and_confirmation_batch_call(200, 200, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
			);
			assert_eq!(
				run_pre_dispatch(all_finality_and_confirmation_batch_call_ex(200, 200, 200)),
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
				run_pre_dispatch(all_finality_and_delivery_batch_call_ex(200, 200, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
			);
			assert_eq!(
				run_pre_dispatch(all_finality_and_confirmation_batch_call(200, 200, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
			);
			assert_eq!(
				run_pre_dispatch(all_finality_and_confirmation_batch_call_ex(200, 200, 200)),
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
				run_pre_dispatch(all_finality_and_delivery_batch_call_ex(200, 200, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
			);
			assert_eq!(
				run_pre_dispatch(all_finality_and_confirmation_batch_call(200, 200, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
			);
			assert_eq!(
				run_pre_dispatch(all_finality_and_confirmation_batch_call_ex(200, 200, 200)),
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
				run_pre_dispatch(all_finality_and_delivery_batch_call_ex(200, 200, 200)),
				Ok(Some(all_finality_pre_dispatch_data_ex())),
			);
			assert_eq!(
				run_pre_dispatch(all_finality_and_confirmation_batch_call(200, 200, 200)),
				Ok(Some(all_finality_confirmation_pre_dispatch_data())),
			);
			assert_eq!(
				run_pre_dispatch(all_finality_and_confirmation_batch_call_ex(200, 200, 200)),
				Ok(Some(all_finality_confirmation_pre_dispatch_data_ex())),
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
						parachain_heads_proof: ParaHeadsProof { storage_proof: vec![] },
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

			let delivery_rewards_account_balance =
				Balances::free_balance(delivery_rewards_account());

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
			assert_eq!(
				TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
					&all_finality_and_delivery_batch_call_ex(200, 200, 200)
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
			assert_eq!(
				TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
					&all_finality_and_confirmation_batch_call_ex(200, 200, 200)
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
			assert_eq!(
				TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
					&relay_finality_and_delivery_batch_call_ex(200, 200)
				),
				Ok(Some(relay_finality_pre_dispatch_data_ex().call_info)),
			);

			// relay + message confirmation call batch is accepted
			assert_eq!(
				TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
					&relay_finality_and_confirmation_batch_call(200, 200)
				),
				Ok(Some(relay_finality_confirmation_pre_dispatch_data().call_info)),
			);
			assert_eq!(
				TestGrandpaExtensionProvider::parse_and_check_for_obsolete_call(
					&relay_finality_and_confirmation_batch_call_ex(200, 200)
				),
				Ok(Some(relay_finality_confirmation_pre_dispatch_data_ex().call_info)),
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
				run_grandpa_pre_dispatch(relay_finality_and_delivery_batch_call_ex(100, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);

			assert_eq!(
				run_grandpa_validate(relay_finality_and_delivery_batch_call(100, 200)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_grandpa_validate(relay_finality_and_delivery_batch_call_ex(100, 200)),
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
				run_grandpa_pre_dispatch(relay_finality_and_delivery_batch_call_ex(200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_grandpa_pre_dispatch(relay_finality_and_confirmation_batch_call(200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_grandpa_pre_dispatch(relay_finality_and_confirmation_batch_call_ex(200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);

			assert_eq!(
				run_grandpa_validate(relay_finality_and_delivery_batch_call(200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_grandpa_validate(relay_finality_and_delivery_batch_call_ex(200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_grandpa_validate(relay_finality_and_confirmation_batch_call(200, 100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_grandpa_validate(relay_finality_and_confirmation_batch_call_ex(200, 100)),
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
				run_grandpa_pre_dispatch(relay_finality_and_delivery_batch_call_ex(200, 200)),
				Ok(Some(relay_finality_pre_dispatch_data_ex()),)
			);
			assert_eq!(
				run_grandpa_pre_dispatch(relay_finality_and_confirmation_batch_call(200, 200)),
				Ok(Some(relay_finality_confirmation_pre_dispatch_data())),
			);
			assert_eq!(
				run_grandpa_pre_dispatch(relay_finality_and_confirmation_batch_call_ex(200, 200)),
				Ok(Some(relay_finality_confirmation_pre_dispatch_data_ex())),
			);

			assert_eq!(
				run_grandpa_validate(relay_finality_and_delivery_batch_call(200, 200)),
				Ok(Default::default()),
			);
			assert_eq!(
				run_grandpa_validate(relay_finality_and_delivery_batch_call_ex(200, 200)),
				Ok(Default::default()),
			);
			assert_eq!(
				run_grandpa_validate(relay_finality_and_confirmation_batch_call(200, 200)),
				Ok(Default::default()),
			);
			assert_eq!(
				run_grandpa_validate(relay_finality_and_confirmation_batch_call_ex(200, 200)),
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
			assert_eq!(
				run_grandpa_validate(message_confirmation_call(200)),
				Ok(Default::default()),
			);
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
}
