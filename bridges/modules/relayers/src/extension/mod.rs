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

//! Signed extension, built around `pallet-bridge-relayers`. It is able to:
//!
//! - refund the cost of successful message delivery and confirmation transactions to the submitter
//!   by registering corresponding reward in the pallet;
//!
//! - bump priority of messages delivery and confirmation transactions, signed by the registered
//!   relayers.

use crate::{Config as RelayersConfig, Pallet as RelayersPallet, WeightInfoExt, LOG_TARGET};

use bp_messages::{ChainWithMessages, MessageNonce};
use bp_relayers::{
	ExplicitOrAccountParams, ExtensionCallData, ExtensionCallInfo, ExtensionConfig,
	RewardsAccountOwner, RewardsAccountParams,
};
use bp_runtime::{Chain, RangeInclusiveExt, StaticStrProvider};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	weights::Weight,
	CloneNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use frame_system::Config as SystemConfig;
use pallet_bridge_messages::{
	CallHelper as MessagesCallHelper, Config as BridgeMessagesConfig, LaneIdOf,
};
use pallet_transaction_payment::{
	Config as TransactionPaymentConfig, OnChargeTransaction, Pallet as TransactionPaymentPallet,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AsSystemOriginSigner, DispatchInfoOf, Dispatchable, PostDispatchInfoOf,
		TransactionExtension, ValidateResult, Zero,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransactionBuilder},
	DispatchResult, RuntimeDebug,
};
use sp_std::{fmt::Debug, marker::PhantomData};

pub use grandpa_adapter::WithGrandpaChainExtensionConfig;
pub use messages_adapter::WithMessagesExtensionConfig;
pub use parachain_adapter::WithParachainExtensionConfig;
pub use priority::*;

mod grandpa_adapter;
mod messages_adapter;
mod parachain_adapter;
mod priority;

/// Data that is crafted in `validate`, passed to `prepare` and used at `post_dispatch` method.
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct PreDispatchData<
	AccountId,
	RemoteGrandpaChainBlockNumber: Debug,
	LaneId: Clone + Copy + Debug,
> {
	/// Transaction submitter (relayer) account.
	relayer: AccountId,
	/// Type of the call.
	call_info: ExtensionCallInfo<RemoteGrandpaChainBlockNumber, LaneId>,
}

impl<AccountId, RemoteGrandpaChainBlockNumber: Debug, LaneId: Clone + Copy + Debug>
	PreDispatchData<AccountId, RemoteGrandpaChainBlockNumber, LaneId>
{
	/// Returns mutable reference to `finality_target` sent to the
	/// `SubmitFinalityProof` call.
	#[cfg(test)]
	pub fn submit_finality_proof_info_mut(
		&mut self,
	) -> Option<&mut bp_header_chain::SubmitFinalityProofInfo<RemoteGrandpaChainBlockNumber>> {
		match self.call_info {
			ExtensionCallInfo::AllFinalityAndMsgs(ref mut info, _, _) => Some(info),
			ExtensionCallInfo::RelayFinalityAndMsgs(ref mut info, _) => Some(info),
			_ => None,
		}
	}
}

/// The actions on relayer account that need to be performed because of his actions.
#[derive(RuntimeDebug, PartialEq)]
pub enum RelayerAccountAction<AccountId, Reward, LaneId> {
	/// Do nothing with relayer account.
	None,
	/// Reward the relayer.
	Reward(AccountId, RewardsAccountParams<LaneId>, Reward),
	/// Slash the relayer.
	Slash(AccountId, RewardsAccountParams<LaneId>),
}

/// A signed extension, built around `pallet-bridge-relayers`.
///
/// It may be incorporated into runtime to refund relayers for submitting correct
/// message delivery and confirmation transactions, optionally batched with required
/// finality proofs.
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
#[scale_info(skip_type_params(Runtime, Config, LaneId))]
pub struct BridgeRelayersTransactionExtension<Runtime, Config, LaneId>(
	PhantomData<(Runtime, Config, LaneId)>,
);

impl<R, C, LaneId> BridgeRelayersTransactionExtension<R, C, LaneId>
where
	Self: 'static + Send + Sync,
	R: RelayersConfig<LaneId = LaneId>
		+ BridgeMessagesConfig<C::BridgeMessagesPalletInstance, LaneId = LaneId>
		+ TransactionPaymentConfig,
	C: ExtensionConfig<Runtime = R, LaneId = LaneId>,
	R::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	<R::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<R::AccountId> + Clone,
	<R as TransactionPaymentConfig>::OnChargeTransaction:
		OnChargeTransaction<R, Balance = R::Reward>,
	LaneId: Clone + Copy + Decode + Encode + Debug + TypeInfo,
{
	/// Returns number of bundled messages `Some(_)`, if the given call info is a:
	///
	/// - message delivery transaction;
	///
	/// - with reasonable bundled messages that may be accepted by the messages pallet.
	///
	/// This function is used to check whether the transaction priority should be
	/// virtually boosted. The relayer registration (we only boost priority for registered
	/// relayer transactions) must be checked outside.
	fn bundled_messages_for_priority_boost(
		parsed_call: &ExtensionCallInfo<C::RemoteGrandpaChainBlockNumber, LaneId>,
	) -> Option<MessageNonce> {
		// we only boost priority of message delivery transactions
		if !parsed_call.is_receive_messages_proof_call() {
			return None;
		}

		// compute total number of messages in transaction
		let bundled_messages = parsed_call.messages_call_info().bundled_messages().saturating_len();

		// a quick check to avoid invalid high-priority transactions
		let max_unconfirmed_messages_in_confirmation_tx = <R as BridgeMessagesConfig<C::BridgeMessagesPalletInstance>>::BridgedChain
			::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
		if bundled_messages > max_unconfirmed_messages_in_confirmation_tx {
			return None
		}

		Some(bundled_messages)
	}

	/// Given post-dispatch information, analyze the outcome of relayer call and return
	/// actions that need to be performed on relayer account.
	fn analyze_call_result(
		pre: Option<PreDispatchData<R::AccountId, C::RemoteGrandpaChainBlockNumber, LaneId>>,
		info: &DispatchInfo,
		post_info: &PostDispatchInfo,
		len: usize,
		result: &DispatchResult,
	) -> RelayerAccountAction<R::AccountId, R::Reward, LaneId> {
		// We don't refund anything for transactions that we don't support.
		let (relayer, call_info) = match pre {
			Some(pre) => (pre.relayer, pre.call_info),
			_ => return RelayerAccountAction::None,
		};

		// now we know that the call is supported and we may need to reward or slash relayer
		// => let's prepare the correspondent account that pays reward/receives slashed amount
		let lane_id = call_info.messages_call_info().lane_id();
		let reward_account_params = RewardsAccountParams::new(
			lane_id,
			<R as BridgeMessagesConfig<C::BridgeMessagesPalletInstance>>::BridgedChain::ID,
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
		// there are a couple of edge cases here:
		//
		// - when the relayer becomes registered during message dispatch: this is unlikely + relayer
		//   should be ready for slashing after registration;
		//
		// - when relayer is registered after `validate` is called and priority is not boosted:
		//   relayer should be ready for slashing after registration.
		let may_slash_relayer = Self::bundled_messages_for_priority_boost(&call_info).is_some();
		let slash_relayer_if_delivery_result = may_slash_relayer
			.then(|| RelayerAccountAction::Slash(relayer.clone(), reward_account_params))
			.unwrap_or(RelayerAccountAction::None);

		// We don't refund anything if the transaction has failed.
		if let Err(e) = result {
			log::trace!(
				target: LOG_TARGET,
				"{}.{:?}: relayer {:?} has submitted invalid messages transaction: {:?}",
				Self::IDENTIFIER,
				lane_id,
				relayer,
				e,
			);
			return slash_relayer_if_delivery_result
		}

		// check whether the call has succeeded
		let mut call_data = ExtensionCallData::default();
		if !C::check_call_result(&call_info, &mut call_data, &relayer) {
			return slash_relayer_if_delivery_result
		}

		// regarding the tip - refund that happens here (at this side of the bridge) isn't the whole
		// relayer compensation. He'll receive some amount at the other side of the bridge. It shall
		// (in theory) cover the tip there. Otherwise, if we'll be compensating tip here, some
		// malicious relayer may use huge tips, effectively depleting account that pay rewards. The
		// cost of this attack is nothing. Hence we use zero as tip here.
		let tip = Zero::zero();

		// decrease post-dispatch weight/size using extra weight/size that we know now
		let post_info_len = len.saturating_sub(call_data.extra_size as usize);
		let mut post_info_weight = post_info
			.actual_weight
			.unwrap_or(info.total_weight())
			.saturating_sub(call_data.extra_weight);

		// let's also replace the weight of slashing relayer with the weight of rewarding relayer
		if call_info.is_receive_messages_proof_call() {
			post_info_weight = post_info_weight.saturating_sub(
				<R as RelayersConfig>::WeightInfo::extra_weight_of_successful_receive_messages_proof_call(),
			);
		}

		// compute the relayer refund
		let mut post_info = *post_info;
		post_info.actual_weight = Some(post_info_weight);
		let refund = Self::compute_refund(info, &post_info, post_info_len, tip);

		// we can finally reward relayer
		RelayerAccountAction::Reward(relayer, reward_account_params, refund)
	}

	/// Compute refund for the successful relayer transaction
	fn compute_refund(
		info: &DispatchInfo,
		post_info: &PostDispatchInfo,
		len: usize,
		tip: R::Reward,
	) -> R::Reward {
		TransactionPaymentPallet::<R>::compute_actual_fee(len as _, info, post_info, tip)
	}
}

impl<R, C, LaneId> TransactionExtension<R::RuntimeCall>
	for BridgeRelayersTransactionExtension<R, C, LaneId>
where
	Self: 'static + Send + Sync,
	R: RelayersConfig<LaneId = LaneId>
		+ BridgeMessagesConfig<C::BridgeMessagesPalletInstance, LaneId = LaneId>
		+ TransactionPaymentConfig,
	C: ExtensionConfig<Runtime = R, LaneId = LaneId>,
	R::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
	<R::RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<R::AccountId> + Clone,
	<R as TransactionPaymentConfig>::OnChargeTransaction:
		OnChargeTransaction<R, Balance = R::Reward>,
	LaneId: Clone + Copy + Decode + Encode + Debug + TypeInfo,
{
	const IDENTIFIER: &'static str = C::IdProvider::STR;
	type Implicit = ();
	type Pre = Option<PreDispatchData<R::AccountId, C::RemoteGrandpaChainBlockNumber, LaneId>>;
	type Val = Self::Pre;

	fn weight(&self, _call: &R::RuntimeCall) -> Weight {
		Weight::zero()
	}

	fn validate(
		&self,
		origin: <R::RuntimeCall as Dispatchable>::RuntimeOrigin,
		call: &R::RuntimeCall,
		_info: &DispatchInfoOf<R::RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> ValidateResult<Self::Val, R::RuntimeCall> {
		// Prepare relevant data for `prepare`
		let parsed_call = match C::parse_and_check_for_obsolete_call(call)? {
			Some(parsed_call) => parsed_call,
			None => return Ok((Default::default(), None, origin)),
		};
		// Those calls are only for signed transactions.
		let relayer = origin.as_system_origin_signer().ok_or(InvalidTransaction::BadSigner)?;

		let data = PreDispatchData { relayer: relayer.clone(), call_info: parsed_call };

		// the following code just plays with transaction priority

		// we only boost priority of presumably correct message delivery transactions
		let bundled_messages = match Self::bundled_messages_for_priority_boost(&data.call_info) {
			Some(bundled_messages) => bundled_messages,
			None => return Ok((Default::default(), Some(data), origin)),
		};

		// we only boost priority if relayer has staked required balance
		if !RelayersPallet::<R>::is_registration_active(&data.relayer) {
			return Ok((Default::default(), Some(data), origin))
		}

		// compute priority boost
		let priority_boost =
			priority::compute_priority_boost::<C::PriorityBoostPerMessage>(bundled_messages);
		let valid_transaction = ValidTransactionBuilder::default().priority(priority_boost);

		log::trace!(
			target: LOG_TARGET,
			"{}.{:?}: has boosted priority of message delivery transaction \
			of relayer {:?}: {} messages -> {} priority",
			Self::IDENTIFIER,
			data.call_info.messages_call_info().lane_id(),
			data.relayer,
			bundled_messages,
			priority_boost,
		);

		let validity = valid_transaction.build()?;
		Ok((validity, Some(data), origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		_origin: &<R::RuntimeCall as Dispatchable>::RuntimeOrigin,
		_call: &R::RuntimeCall,
		_info: &DispatchInfoOf<R::RuntimeCall>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(val.inspect(|data| {
			log::trace!(
				target: LOG_TARGET,
				"{}.{:?}: parsed bridge transaction in prepare: {:?}",
				Self::IDENTIFIER,
				data.call_info.messages_call_info().lane_id(),
				data.call_info,
			);
		}))
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		info: &DispatchInfoOf<R::RuntimeCall>,
		post_info: &PostDispatchInfoOf<R::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		let lane_id = pre.as_ref().map(|p| p.call_info.messages_call_info().lane_id());
		let call_result = Self::analyze_call_result(pre, info, post_info, len, result);

		match call_result {
			RelayerAccountAction::None => (),
			RelayerAccountAction::Reward(relayer, reward_account, reward) => {
				RelayersPallet::<R>::register_relayer_reward(reward_account, &relayer, reward);

				log::trace!(
					target: LOG_TARGET,
					"{}.{:?}: has registered reward: {:?} for {:?}",
					Self::IDENTIFIER,
					lane_id,
					reward,
					relayer,
				);
			},
			RelayerAccountAction::Slash(relayer, slash_account) =>
				RelayersPallet::<R>::slash_and_deregister(
					&relayer,
					ExplicitOrAccountParams::Params(slash_account),
				),
		}

		Ok(Weight::zero())
	}
}

/// Verify that the messages pallet call, supported by extension has succeeded.
pub(crate) fn verify_messages_call_succeeded<C>(
	call_info: &ExtensionCallInfo<
		C::RemoteGrandpaChainBlockNumber,
		LaneIdOf<C::Runtime, C::BridgeMessagesPalletInstance>,
	>,
	_call_data: &mut ExtensionCallData,
	relayer: &<C::Runtime as SystemConfig>::AccountId,
) -> bool
where
	C: ExtensionConfig,
	C::Runtime: BridgeMessagesConfig<C::BridgeMessagesPalletInstance>,
{
	let messages_call = call_info.messages_call_info();

	if !MessagesCallHelper::<C::Runtime, C::BridgeMessagesPalletInstance>::was_successful(
		messages_call,
	) {
		log::trace!(
			target: LOG_TARGET,
			"{}.{:?}: relayer {:?} has submitted invalid messages call",
			C::IdProvider::STR,
			call_info.messages_call_info().lane_id(),
			relayer,
		);
		return false
	}

	true
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;

	use bp_header_chain::{StoredHeaderDataBuilder, SubmitFinalityProofInfo};
	use bp_messages::{
		source_chain::FromBridgedChainMessagesDeliveryProof,
		target_chain::FromBridgedChainMessagesProof, BaseMessagesProofInfo, DeliveredMessages,
		InboundLaneData, MessageNonce, MessagesCallInfo, MessagesOperatingMode, OutboundLaneData,
		ReceiveMessagesDeliveryProofInfo, ReceiveMessagesProofInfo, UnrewardedRelayer,
		UnrewardedRelayerOccupation, UnrewardedRelayersState,
	};
	use bp_parachains::{BestParaHeadHash, ParaInfo, SubmitParachainHeadsInfo};
	use bp_polkadot_core::parachains::{ParaHeadsProof, ParaId};
	use bp_relayers::RuntimeWithUtilityPallet;
	use bp_runtime::{BasicOperatingMode, HeaderId, Parachain};
	use bp_test_utils::{make_default_justification, test_keyring, TEST_GRANDPA_SET_ID};
	use frame_support::{
		__private::sp_tracing,
		assert_storage_noop, parameter_types,
		traits::{fungible::Mutate, ReservableCurrency},
		weights::Weight,
	};
	use pallet_bridge_grandpa::{Call as GrandpaCall, Pallet as GrandpaPallet, StoredAuthoritySet};
	use pallet_bridge_messages::{Call as MessagesCall, Pallet as MessagesPallet};
	use pallet_bridge_parachains::{Call as ParachainsCall, Pallet as ParachainsPallet};
	use pallet_utility::Call as UtilityCall;
	use sp_runtime::{
		traits::{ConstU64, DispatchTransaction, Header as HeaderT},
		transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
		DispatchError,
	};

	parameter_types! {
		TestParachain: u32 = BridgedUnderlyingParachain::PARACHAIN_ID;
		pub MsgProofsRewardsAccount: RewardsAccountParams<TestLaneIdType> = RewardsAccountParams::new(
			test_lane_id(),
			TEST_BRIDGED_CHAIN_ID,
			RewardsAccountOwner::ThisChain,
		);
		pub MsgDeliveryProofsRewardsAccount: RewardsAccountParams<TestLaneIdType> = RewardsAccountParams::new(
			test_lane_id(),
			TEST_BRIDGED_CHAIN_ID,
			RewardsAccountOwner::BridgedChain,
		);
	}

	bp_runtime::generate_static_str_provider!(TestGrandpaExtension);
	bp_runtime::generate_static_str_provider!(TestExtension);
	bp_runtime::generate_static_str_provider!(TestMessagesExtension);

	type TestGrandpaExtensionConfig = grandpa_adapter::WithGrandpaChainExtensionConfig<
		StrTestGrandpaExtension,
		TestRuntime,
		RuntimeWithUtilityPallet<TestRuntime>,
		(),
		(),
		(),
		ConstU64<1>,
	>;
	type TestGrandpaExtension =
		BridgeRelayersTransactionExtension<TestRuntime, TestGrandpaExtensionConfig, TestLaneIdType>;
	type TestExtensionConfig = parachain_adapter::WithParachainExtensionConfig<
		StrTestExtension,
		TestRuntime,
		RuntimeWithUtilityPallet<TestRuntime>,
		(),
		(),
		(),
		ConstU64<1>,
	>;
	type TestExtension =
		BridgeRelayersTransactionExtension<TestRuntime, TestExtensionConfig, TestLaneIdType>;
	type TestMessagesExtensionConfig = messages_adapter::WithMessagesExtensionConfig<
		StrTestMessagesExtension,
		TestRuntime,
		(),
		(),
		ConstU64<1>,
	>;
	type TestMessagesExtension = BridgeRelayersTransactionExtension<
		TestRuntime,
		TestMessagesExtensionConfig,
		TestLaneIdType,
	>;

	fn initial_balance_of_relayer_account_at_this_chain() -> ThisChainBalance {
		let test_stake: ThisChainBalance = Stake::get();
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
		best_relay_header_number: BridgedChainBlockNumber,
		parachain_head_at_relay_header_number: BridgedChainBlockNumber,
		best_message: MessageNonce,
	) {
		let authorities = test_keyring().into_iter().map(|(a, w)| (a.into(), w)).collect();
		let best_relay_header = HeaderId(best_relay_header_number, BridgedChainHash::default());
		pallet_bridge_grandpa::CurrentAuthoritySet::<TestRuntime>::put(
			StoredAuthoritySet::try_new(authorities, TEST_GRANDPA_SET_ID).unwrap(),
		);
		pallet_bridge_grandpa::BestFinalized::<TestRuntime>::put(best_relay_header);
		pallet_bridge_grandpa::ImportedHeaders::<TestRuntime>::insert(
			best_relay_header.hash(),
			bp_test_utils::test_header::<BridgedChainHeader>(0).build(),
		);

		let para_id = ParaId(TestParachain::get());
		let para_info = ParaInfo {
			best_head_hash: BestParaHeadHash {
				at_relay_block_number: parachain_head_at_relay_header_number,
				head_hash: [parachain_head_at_relay_header_number as u8; 32].into(),
			},
			next_imported_hash_position: 0,
		};
		pallet_bridge_parachains::ParasInfo::<TestRuntime>::insert(para_id, para_info);

		let lane_id = test_lane_id();
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

	fn submit_relay_header_call(relay_header_number: BridgedChainBlockNumber) -> RuntimeCall {
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

	fn submit_relay_header_call_ex(relay_header_number: BridgedChainBlockNumber) -> RuntimeCall {
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
			is_free_execution_expected: false,
		})
	}

	fn submit_parachain_head_call(
		parachain_head_at_relay_header_number: BridgedChainBlockNumber,
	) -> RuntimeCall {
		RuntimeCall::BridgeParachains(ParachainsCall::submit_parachain_heads {
			at_relay_block: (parachain_head_at_relay_header_number, BridgedChainHash::default()),
			parachains: vec![(
				ParaId(TestParachain::get()),
				[parachain_head_at_relay_header_number as u8; 32].into(),
			)],
			parachain_heads_proof: ParaHeadsProof { storage_proof: Default::default() },
		})
	}

	pub fn submit_parachain_head_call_ex(
		parachain_head_at_relay_header_number: BridgedChainBlockNumber,
	) -> RuntimeCall {
		RuntimeCall::BridgeParachains(ParachainsCall::submit_parachain_heads_ex {
			at_relay_block: (parachain_head_at_relay_header_number, BridgedChainHash::default()),
			parachains: vec![(
				ParaId(TestParachain::get()),
				[parachain_head_at_relay_header_number as u8; 32].into(),
			)],
			parachain_heads_proof: ParaHeadsProof { storage_proof: Default::default() },
			is_free_execution_expected: false,
		})
	}

	fn message_delivery_call(best_message: MessageNonce) -> RuntimeCall {
		RuntimeCall::BridgeMessages(MessagesCall::receive_messages_proof {
			relayer_id_at_bridged_chain: relayer_account_at_bridged_chain(),
			proof: Box::new(FromBridgedChainMessagesProof {
				bridged_header_hash: Default::default(),
				storage_proof: Default::default(),
				lane: test_lane_id(),
				nonces_start: pallet_bridge_messages::InboundLanes::<TestRuntime>::get(
					test_lane_id(),
				)
				.unwrap()
				.last_delivered_nonce() +
					1,
				nonces_end: best_message,
			}),
			messages_count: 1,
			dispatch_weight: Weight::zero(),
		})
	}

	fn message_confirmation_call(best_message: MessageNonce) -> RuntimeCall {
		RuntimeCall::BridgeMessages(MessagesCall::receive_messages_delivery_proof {
			proof: FromBridgedChainMessagesDeliveryProof {
				bridged_header_hash: Default::default(),
				storage_proof: Default::default(),
				lane: test_lane_id(),
			},
			relayers_state: UnrewardedRelayersState {
				last_delivered_nonce: best_message,
				..Default::default()
			},
		})
	}

	fn parachain_finality_and_delivery_batch_call(
		parachain_head_at_relay_header_number: BridgedChainBlockNumber,
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
		parachain_head_at_relay_header_number: BridgedChainBlockNumber,
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
		relay_header_number: BridgedChainBlockNumber,
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
		relay_header_number: BridgedChainBlockNumber,
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
		relay_header_number: BridgedChainBlockNumber,
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
		relay_header_number: BridgedChainBlockNumber,
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
		relay_header_number: BridgedChainBlockNumber,
		parachain_head_at_relay_header_number: BridgedChainBlockNumber,
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
		relay_header_number: BridgedChainBlockNumber,
		parachain_head_at_relay_header_number: BridgedChainBlockNumber,
		best_message: MessageNonce,
	) -> RuntimeCall {
		RuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![
				submit_relay_header_call_ex(relay_header_number),
				submit_parachain_head_call_ex(parachain_head_at_relay_header_number),
				message_delivery_call(best_message),
			],
		})
	}

	fn all_finality_and_confirmation_batch_call(
		relay_header_number: BridgedChainBlockNumber,
		parachain_head_at_relay_header_number: BridgedChainBlockNumber,
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
		relay_header_number: BridgedChainBlockNumber,
		parachain_head_at_relay_header_number: BridgedChainBlockNumber,
		best_message: MessageNonce,
	) -> RuntimeCall {
		RuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![
				submit_relay_header_call_ex(relay_header_number),
				submit_parachain_head_call_ex(parachain_head_at_relay_header_number),
				message_confirmation_call(best_message),
			],
		})
	}

	fn all_finality_pre_dispatch_data(
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: ExtensionCallInfo::AllFinalityAndMsgs(
				SubmitFinalityProofInfo {
					block_number: 200,
					current_set_id: None,
					extra_weight: Weight::zero(),
					extra_size: 0,
					is_mandatory: false,
					is_free_execution_expected: false,
				},
				SubmitParachainHeadsInfo {
					at_relay_block: HeaderId(200, [0u8; 32].into()),
					para_id: ParaId(TestParachain::get()),
					para_head_hash: [200u8; 32].into(),
					is_free_execution_expected: false,
				},
				MessagesCallInfo::ReceiveMessagesProof(ReceiveMessagesProofInfo {
					base: BaseMessagesProofInfo {
						lane_id: test_lane_id(),
						bundled_range: 101..=200,
						best_stored_nonce: 100,
					},
					unrewarded_relayers: UnrewardedRelayerOccupation {
						free_relayer_slots:
							BridgedUnderlyingParachain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
						free_message_slots:
							BridgedUnderlyingParachain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
					},
				}),
			),
		}
	}

	#[cfg(test)]
	fn all_finality_pre_dispatch_data_ex(
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		let mut data = all_finality_pre_dispatch_data();
		data.submit_finality_proof_info_mut().unwrap().current_set_id = Some(TEST_GRANDPA_SET_ID);
		data
	}

	fn all_finality_confirmation_pre_dispatch_data(
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: ExtensionCallInfo::AllFinalityAndMsgs(
				SubmitFinalityProofInfo {
					block_number: 200,
					current_set_id: None,
					extra_weight: Weight::zero(),
					extra_size: 0,
					is_mandatory: false,
					is_free_execution_expected: false,
				},
				SubmitParachainHeadsInfo {
					at_relay_block: HeaderId(200, [0u8; 32].into()),
					para_id: ParaId(TestParachain::get()),
					para_head_hash: [200u8; 32].into(),
					is_free_execution_expected: false,
				},
				MessagesCallInfo::ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo(
					BaseMessagesProofInfo {
						lane_id: test_lane_id(),
						bundled_range: 101..=200,
						best_stored_nonce: 100,
					},
				)),
			),
		}
	}

	fn all_finality_confirmation_pre_dispatch_data_ex(
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		let mut data = all_finality_confirmation_pre_dispatch_data();
		data.submit_finality_proof_info_mut().unwrap().current_set_id = Some(TEST_GRANDPA_SET_ID);
		data
	}

	fn relay_finality_pre_dispatch_data(
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: ExtensionCallInfo::RelayFinalityAndMsgs(
				SubmitFinalityProofInfo {
					block_number: 200,
					current_set_id: None,
					extra_weight: Weight::zero(),
					extra_size: 0,
					is_mandatory: false,
					is_free_execution_expected: false,
				},
				MessagesCallInfo::ReceiveMessagesProof(ReceiveMessagesProofInfo {
					base: BaseMessagesProofInfo {
						lane_id: test_lane_id(),
						bundled_range: 101..=200,
						best_stored_nonce: 100,
					},
					unrewarded_relayers: UnrewardedRelayerOccupation {
						free_relayer_slots:
							BridgedUnderlyingParachain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
						free_message_slots:
							BridgedUnderlyingParachain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
					},
				}),
			),
		}
	}

	fn relay_finality_pre_dispatch_data_ex(
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		let mut data = relay_finality_pre_dispatch_data();
		data.submit_finality_proof_info_mut().unwrap().current_set_id = Some(TEST_GRANDPA_SET_ID);
		data
	}

	fn relay_finality_confirmation_pre_dispatch_data(
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: ExtensionCallInfo::RelayFinalityAndMsgs(
				SubmitFinalityProofInfo {
					block_number: 200,
					current_set_id: None,
					extra_weight: Weight::zero(),
					extra_size: 0,
					is_mandatory: false,
					is_free_execution_expected: false,
				},
				MessagesCallInfo::ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo(
					BaseMessagesProofInfo {
						lane_id: test_lane_id(),
						bundled_range: 101..=200,
						best_stored_nonce: 100,
					},
				)),
			),
		}
	}

	fn relay_finality_confirmation_pre_dispatch_data_ex(
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		let mut data = relay_finality_confirmation_pre_dispatch_data();
		data.submit_finality_proof_info_mut().unwrap().current_set_id = Some(TEST_GRANDPA_SET_ID);
		data
	}

	fn parachain_finality_pre_dispatch_data(
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: ExtensionCallInfo::ParachainFinalityAndMsgs(
				SubmitParachainHeadsInfo {
					at_relay_block: HeaderId(200, [0u8; 32].into()),
					para_id: ParaId(TestParachain::get()),
					para_head_hash: [200u8; 32].into(),
					is_free_execution_expected: false,
				},
				MessagesCallInfo::ReceiveMessagesProof(ReceiveMessagesProofInfo {
					base: BaseMessagesProofInfo {
						lane_id: test_lane_id(),
						bundled_range: 101..=200,
						best_stored_nonce: 100,
					},
					unrewarded_relayers: UnrewardedRelayerOccupation {
						free_relayer_slots:
							BridgedUnderlyingParachain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
						free_message_slots:
							BridgedUnderlyingParachain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
					},
				}),
			),
		}
	}

	fn parachain_finality_confirmation_pre_dispatch_data(
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: ExtensionCallInfo::ParachainFinalityAndMsgs(
				SubmitParachainHeadsInfo {
					at_relay_block: HeaderId(200, [0u8; 32].into()),
					para_id: ParaId(TestParachain::get()),
					para_head_hash: [200u8; 32].into(),
					is_free_execution_expected: false,
				},
				MessagesCallInfo::ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo(
					BaseMessagesProofInfo {
						lane_id: test_lane_id(),
						bundled_range: 101..=200,
						best_stored_nonce: 100,
					},
				)),
			),
		}
	}

	fn delivery_pre_dispatch_data<RemoteGrandpaChainBlockNumber: Debug>(
	) -> PreDispatchData<ThisChainAccountId, RemoteGrandpaChainBlockNumber, TestLaneIdType> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: ExtensionCallInfo::Msgs(MessagesCallInfo::ReceiveMessagesProof(
				ReceiveMessagesProofInfo {
					base: BaseMessagesProofInfo {
						lane_id: test_lane_id(),
						bundled_range: 101..=200,
						best_stored_nonce: 100,
					},
					unrewarded_relayers: UnrewardedRelayerOccupation {
						free_relayer_slots:
							BridgedUnderlyingParachain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
						free_message_slots:
							BridgedUnderlyingParachain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
					},
				},
			)),
		}
	}

	fn confirmation_pre_dispatch_data<RemoteGrandpaChainBlockNumber: Debug>(
	) -> PreDispatchData<ThisChainAccountId, RemoteGrandpaChainBlockNumber, TestLaneIdType> {
		PreDispatchData {
			relayer: relayer_account_at_this_chain(),
			call_info: ExtensionCallInfo::Msgs(MessagesCallInfo::ReceiveMessagesDeliveryProof(
				ReceiveMessagesDeliveryProofInfo(BaseMessagesProofInfo {
					lane_id: test_lane_id(),
					bundled_range: 101..=200,
					best_stored_nonce: 100,
				}),
			)),
		}
	}

	fn set_bundled_range_end(
		mut pre_dispatch_data: PreDispatchData<
			ThisChainAccountId,
			BridgedChainBlockNumber,
			TestLaneIdType,
		>,
		end: MessageNonce,
	) -> PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType> {
		let msg_info = match pre_dispatch_data.call_info {
			ExtensionCallInfo::AllFinalityAndMsgs(_, _, ref mut info) => info,
			ExtensionCallInfo::RelayFinalityAndMsgs(_, ref mut info) => info,
			ExtensionCallInfo::ParachainFinalityAndMsgs(_, ref mut info) => info,
			ExtensionCallInfo::Msgs(ref mut info) => info,
		};

		if let MessagesCallInfo::ReceiveMessagesProof(ref mut msg_info) = msg_info {
			msg_info.base.bundled_range = *msg_info.base.bundled_range.start()..=end
		}

		pre_dispatch_data
	}

	fn run_validate(call: RuntimeCall) -> TransactionValidity {
		let extension: TestExtension = BridgeRelayersTransactionExtension(PhantomData);
		extension
			.validate_only(
				Some(relayer_account_at_this_chain()).into(),
				&call,
				&DispatchInfo::default(),
				0,
			)
			.map(|t| t.0)
	}

	fn run_grandpa_validate(call: RuntimeCall) -> TransactionValidity {
		let extension: TestGrandpaExtension = BridgeRelayersTransactionExtension(PhantomData);
		extension
			.validate_only(
				Some(relayer_account_at_this_chain()).into(),
				&call,
				&DispatchInfo::default(),
				0,
			)
			.map(|t| t.0)
	}

	fn run_messages_validate(call: RuntimeCall) -> TransactionValidity {
		let extension: TestMessagesExtension = BridgeRelayersTransactionExtension(PhantomData);
		extension
			.validate_only(
				Some(relayer_account_at_this_chain()).into(),
				&call,
				&DispatchInfo::default(),
				0,
			)
			.map(|t| t.0)
	}

	fn ignore_priority(tx: TransactionValidity) -> TransactionValidity {
		tx.map(|mut tx| {
			tx.priority = 0;
			tx
		})
	}

	fn run_pre_dispatch(
		call: RuntimeCall,
	) -> Result<
		Option<PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType>>,
		TransactionValidityError,
	> {
		sp_tracing::try_init_simple();
		let extension: TestExtension = BridgeRelayersTransactionExtension(PhantomData);
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
	) -> Result<
		Option<PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType>>,
		TransactionValidityError,
	> {
		let extension: TestGrandpaExtension = BridgeRelayersTransactionExtension(PhantomData);
		extension
			.validate_and_prepare(
				Some(relayer_account_at_this_chain()).into(),
				&call,
				&DispatchInfo::default(),
				0,
			)
			.map(|(pre, _)| pre)
	}

	fn run_messages_pre_dispatch(
		call: RuntimeCall,
	) -> Result<
		Option<PreDispatchData<ThisChainAccountId, (), TestLaneIdType>>,
		TransactionValidityError,
	> {
		let extension: TestMessagesExtension = BridgeRelayersTransactionExtension(PhantomData);
		extension
			.validate_and_prepare(
				Some(relayer_account_at_this_chain()).into(),
				&call,
				&DispatchInfo::default(),
				0,
			)
			.map(|(pre, _)| pre)
	}

	fn dispatch_info() -> DispatchInfo {
		DispatchInfo {
			call_weight: Weight::from_parts(
				frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND,
				0,
			),
			extension_weight: Weight::zero(),
			class: frame_support::dispatch::DispatchClass::Normal,
			pays_fee: frame_support::dispatch::Pays::Yes,
		}
	}

	fn post_dispatch_info() -> PostDispatchInfo {
		PostDispatchInfo { actual_weight: None, pays_fee: frame_support::dispatch::Pays::Yes }
	}

	fn run_post_dispatch(
		pre_dispatch_data: Option<
			PreDispatchData<ThisChainAccountId, BridgedChainBlockNumber, TestLaneIdType>,
		>,
		dispatch_result: DispatchResult,
	) {
		let post_dispatch_result = TestExtension::post_dispatch_details(
			pre_dispatch_data,
			&dispatch_info(),
			&post_dispatch_info(),
			1024,
			&dispatch_result,
		);
		assert_eq!(post_dispatch_result, Ok(Weight::zero()));
	}

	fn expected_delivery_reward() -> ThisChainBalance {
		let mut post_dispatch_info = post_dispatch_info();
		let extra_weight = <TestRuntime as RelayersConfig>::WeightInfo::extra_weight_of_successful_receive_messages_proof_call();
		post_dispatch_info.actual_weight =
			Some(dispatch_info().call_weight.saturating_sub(extra_weight));
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
			// message confirmation validation is passing
			assert_eq!(
				ignore_priority(run_validate(message_confirmation_call(200))),
				Ok(Default::default()),
			);
			assert_eq!(
				ignore_priority(run_validate(parachain_finality_and_confirmation_batch_call(
					200, 200
				))),
				Ok(Default::default()),
			);
			assert_eq!(
				ignore_priority(run_validate(all_finality_and_confirmation_batch_call(
					200, 200, 200
				))),
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
				100 + BridgedUnderlyingParachain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			))
			.unwrap()
			.priority;
			let priority_of_more_than_max_messages_delivery = run_validate(message_delivery_call(
				100 + BridgedUnderlyingParachain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX + 1,
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
				ignore_priority(run_validate(message_delivery_call(200))),
				Ok(ValidTransaction::default()),
			);
			assert_eq!(
				ignore_priority(run_validate(message_confirmation_call(200))),
				Ok(ValidTransaction::default()),
			);
			assert_eq!(
				ignore_priority(run_messages_validate(message_delivery_call(200))),
				Ok(ValidTransaction::default()),
			);

			assert_eq!(
				ignore_priority(run_validate(parachain_finality_and_delivery_batch_call(200, 200))),
				Ok(ValidTransaction::default()),
			);
			assert_eq!(
				ignore_priority(run_validate(parachain_finality_and_confirmation_batch_call(
					200, 200
				))),
				Ok(ValidTransaction::default()),
			);

			assert_eq!(
				ignore_priority(run_validate(all_finality_and_delivery_batch_call(200, 200, 200))),
				Ok(ValidTransaction::default()),
			);
			assert_eq!(
				ignore_priority(run_validate(all_finality_and_confirmation_batch_call(
					200, 200, 200
				))),
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
						at_relay_block: (100, BridgedChainHash::default()),
						parachains: vec![
							(ParaId(TestParachain::get()), [1u8; 32].into()),
							(ParaId(TestParachain::get() + 1), [1u8; 32].into()),
						],
						parachain_heads_proof: ParaHeadsProof { storage_proof: Default::default() },
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
			dispatch_info.call_weight = Weight::from_parts(
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
				ExtensionCallInfo::AllFinalityAndMsgs(ref mut info, ..) => {
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

			let test_stake: ThisChainBalance = Stake::get();
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
		pre_dispatch_data: PreDispatchData<
			ThisChainAccountId,
			BridgedChainBlockNumber,
			TestLaneIdType,
		>,
		dispatch_result: DispatchResult,
	) -> RelayerAccountAction<ThisChainAccountId, ThisChainBalance, TestLaneIdType> {
		TestExtension::analyze_call_result(
			Some(pre_dispatch_data),
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
				TestGrandpaExtensionConfig::parse_and_check_for_obsolete_call(
					&all_finality_and_delivery_batch_call(200, 200, 200)
				),
				Ok(None),
			);

			// relay + parachain + message confirmation calls batch is ignored
			assert_eq!(
				TestGrandpaExtensionConfig::parse_and_check_for_obsolete_call(
					&all_finality_and_confirmation_batch_call(200, 200, 200)
				),
				Ok(None),
			);

			// parachain + message delivery call batch is ignored
			assert_eq!(
				TestGrandpaExtensionConfig::parse_and_check_for_obsolete_call(
					&parachain_finality_and_delivery_batch_call(200, 200)
				),
				Ok(None),
			);

			// parachain + message confirmation call batch is ignored
			assert_eq!(
				TestGrandpaExtensionConfig::parse_and_check_for_obsolete_call(
					&parachain_finality_and_confirmation_batch_call(200, 200)
				),
				Ok(None),
			);

			// relay + message delivery call batch is accepted
			assert_eq!(
				TestGrandpaExtensionConfig::parse_and_check_for_obsolete_call(
					&relay_finality_and_delivery_batch_call(200, 200)
				),
				Ok(Some(relay_finality_pre_dispatch_data().call_info)),
			);

			// relay + message confirmation call batch is accepted
			assert_eq!(
				TestGrandpaExtensionConfig::parse_and_check_for_obsolete_call(
					&relay_finality_and_confirmation_batch_call(200, 200)
				),
				Ok(Some(relay_finality_confirmation_pre_dispatch_data().call_info)),
			);

			// message delivery call batch is accepted
			assert_eq!(
				TestGrandpaExtensionConfig::parse_and_check_for_obsolete_call(
					&message_delivery_call(200)
				),
				Ok(Some(delivery_pre_dispatch_data().call_info)),
			);

			// message confirmation call batch is accepted
			assert_eq!(
				TestGrandpaExtensionConfig::parse_and_check_for_obsolete_call(
					&message_confirmation_call(200)
				),
				Ok(Some(confirmation_pre_dispatch_data().call_info)),
			);
		});
	}

	#[test]
	fn messages_ext_only_parses_standalone_transactions() {
		run_test(|| {
			initialize_environment(100, 100, 100);

			// relay + parachain + message delivery calls batch is ignored
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&all_finality_and_delivery_batch_call(200, 200, 200)
				),
				Ok(None),
			);
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&all_finality_and_delivery_batch_call_ex(200, 200, 200)
				),
				Ok(None),
			);

			// relay + parachain + message confirmation calls batch is ignored
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&all_finality_and_confirmation_batch_call(200, 200, 200)
				),
				Ok(None),
			);
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&all_finality_and_confirmation_batch_call_ex(200, 200, 200)
				),
				Ok(None),
			);

			// parachain + message delivery call batch is ignored
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&parachain_finality_and_delivery_batch_call(200, 200)
				),
				Ok(None),
			);

			// parachain + message confirmation call batch is ignored
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&parachain_finality_and_confirmation_batch_call(200, 200)
				),
				Ok(None),
			);

			// relay + message delivery call batch is ignored
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&relay_finality_and_delivery_batch_call(200, 200)
				),
				Ok(None),
			);
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&relay_finality_and_delivery_batch_call_ex(200, 200)
				),
				Ok(None),
			);

			// relay + message confirmation call batch is ignored
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&relay_finality_and_confirmation_batch_call(200, 200)
				),
				Ok(None),
			);
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&relay_finality_and_confirmation_batch_call_ex(200, 200)
				),
				Ok(None),
			);

			// message delivery call batch is accepted
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&message_delivery_call(200)
				),
				Ok(Some(delivery_pre_dispatch_data().call_info)),
			);

			// message confirmation call batch is accepted
			assert_eq!(
				TestMessagesExtensionConfig::parse_and_check_for_obsolete_call(
					&message_confirmation_call(200)
				),
				Ok(Some(confirmation_pre_dispatch_data().call_info)),
			);
		});
	}

	#[test]
	fn messages_ext_rejects_calls_with_obsolete_messages() {
		run_test(|| {
			initialize_environment(100, 100, 100);

			assert_eq!(
				run_messages_pre_dispatch(message_delivery_call(100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_messages_pre_dispatch(message_confirmation_call(100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);

			assert_eq!(
				run_messages_validate(message_delivery_call(100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
			assert_eq!(
				run_messages_validate(message_confirmation_call(100)),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
			);
		});
	}

	#[test]
	fn messages_ext_accepts_calls_with_new_messages() {
		run_test(|| {
			initialize_environment(100, 100, 100);

			assert_eq!(
				run_messages_pre_dispatch(message_delivery_call(200)),
				Ok(Some(delivery_pre_dispatch_data())),
			);
			assert_eq!(
				run_messages_pre_dispatch(message_confirmation_call(200)),
				Ok(Some(confirmation_pre_dispatch_data())),
			);

			assert_eq!(run_messages_validate(message_delivery_call(200)), Ok(Default::default()),);
			assert_eq!(
				run_messages_validate(message_confirmation_call(200)),
				Ok(Default::default()),
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
			let best_delivered_message =
				BridgedUnderlyingParachain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
			initialize_environment(100, 100, best_delivered_message);

			// register relayer so it gets priority boost
			BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
				.unwrap();

			// allow empty message delivery transactions
			let lane_id = test_lane_id();
			let in_lane_data = InboundLaneData {
				last_confirmed_nonce: 0,
				relayers: vec![UnrewardedRelayer {
					relayer: relayer_account_at_bridged_chain(),
					messages: DeliveredMessages { begin: 1, end: best_delivered_message },
				}]
				.into(),
				..Default::default()
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
