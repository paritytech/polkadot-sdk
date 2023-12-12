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
//! with calls that are: delivering new messsage and all necessary underlying headers
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
	traits::{
		AsSystemOriginSigner, DispatchInfoOf, Dispatchable, Get, PostDispatchInfoOf,
		TransactionExtension, TransactionExtensionBase, Zero,
	},
	transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionValidityError, ValidTransactionBuilder,
	},
	DispatchResult, FixedPointOperand, RuntimeDebug,
};
use sp_std::{marker::PhantomData, vec, vec::Vec};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
pub mod mock;
#[cfg(test)]
pub(crate) mod tests;

mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::Config as ExtBenchmarkingConfig;

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

/// Everything common among our refund transaction extensions.
pub trait RefundTransactionExtension:
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

/// Adapter that allow implementing `sp_runtime::traits::TransactionExtension` for any
/// `RefundTransactionExtension`.
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
pub struct RefundTransactionExtensionAdapter<T: RefundTransactionExtension>(T)
where
	<T::Runtime as GrandpaConfig<T::GrandpaInstance>>::BridgedChain:
		Chain<BlockNumber = RelayBlockNumber>;

impl<T: RefundTransactionExtension> TransactionExtensionBase
	for RefundTransactionExtensionAdapter<T>
where
	<T::Runtime as GrandpaConfig<T::GrandpaInstance>>::BridgedChain:
		Chain<BlockNumber = RelayBlockNumber>,
	CallOf<T::Runtime>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ IsSubType<CallableCallFor<UtilityPallet<T::Runtime>, T::Runtime>>
		+ GrandpaCallSubType<T::Runtime, T::GrandpaInstance>
		+ MessagesCallSubType<T::Runtime, <T::Msgs as RefundableMessagesLaneId>::Instance>,
{
	const IDENTIFIER: &'static str = T::Id::STR;
	type Implicit = ();
}

impl<T: RefundTransactionExtension, Context> TransactionExtension<CallOf<T::Runtime>, Context>
	for RefundTransactionExtensionAdapter<T>
where
	<T::Runtime as GrandpaConfig<T::GrandpaInstance>>::BridgedChain:
		Chain<BlockNumber = RelayBlockNumber>,
	CallOf<T::Runtime>: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ IsSubType<CallableCallFor<UtilityPallet<T::Runtime>, T::Runtime>>
		+ GrandpaCallSubType<T::Runtime, T::GrandpaInstance>
		+ MessagesCallSubType<T::Runtime, <T::Msgs as RefundableMessagesLaneId>::Instance>,
	<CallOf<T::Runtime> as Dispatchable>::RuntimeOrigin:
		AsSystemOriginSigner<AccountIdOf<T::Runtime>> + Clone,
{
	type Pre = Option<PreDispatchData<AccountIdOf<T::Runtime>>>;
	type Val = Option<CallInfo>;

	fn validate(
		&self,
		origin: <CallOf<T::Runtime> as Dispatchable>::RuntimeOrigin,
		call: &CallOf<T::Runtime>,
		_info: &DispatchInfoOf<CallOf<T::Runtime>>,
		_len: usize,
		_context: &mut Context,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> Result<
		(
			sp_runtime::transaction_validity::ValidTransaction,
			Self::Val,
			<CallOf<T::Runtime> as Dispatchable>::RuntimeOrigin,
		),
		sp_runtime::transaction_validity::TransactionValidityError,
	> {
		let who = origin.as_system_origin_signer().ok_or(InvalidTransaction::BadSigner)?;
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
			None => return Ok((Default::default(), parsed_call, origin)),
		};

		// we only boost priority if relayer has staked required balance
		if !RelayersPallet::<T::Runtime>::is_registration_active(who) {
			return Ok((Default::default(), parsed_call, origin))
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

		let validity = valid_transaction.build()?;
		Ok((validity, parsed_call, origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &<CallOf<T::Runtime> as Dispatchable>::RuntimeOrigin,
		_call: &CallOf<T::Runtime>,
		_info: &DispatchInfoOf<CallOf<T::Runtime>>,
		_len: usize,
		_context: &Context,
	) -> Result<Self::Pre, TransactionValidityError> {
		let who = origin.as_system_origin_signer().ok_or(InvalidTransaction::BadSigner)?;
		Ok(val.map(|call_info| {
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
		pre: Self::Pre,
		info: &DispatchInfoOf<CallOf<T::Runtime>>,
		post_info: &PostDispatchInfoOf<CallOf<T::Runtime>>,
		len: usize,
		result: &DispatchResult,
		_context: &Context,
	) -> Result<(), TransactionValidityError> {
		let call_result = T::analyze_call_result(Some(pre), info, post_info, len, result);

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

/// Transaction extension that refunds a relayer for new messages coming from a parachain.
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

impl<Runtime, Para, Msgs, Refund, Priority, Id> RefundTransactionExtension
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

/// Transaction extension that refunds a relayer for new messages coming from a standalone (GRANDPA)
/// chain.
///
/// Also refunds relayer for successful finality delivery if it comes in batch (`utility.batchAll`)
/// with message delivery transaction. Batch may deliver either both relay chain header and
/// parachain head, or just parachain head. Corresponding headers must be used in messages proof
/// verification.
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

impl<Runtime, GrandpaInstance, Msgs, Refund, Priority, Id> RefundTransactionExtension
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
