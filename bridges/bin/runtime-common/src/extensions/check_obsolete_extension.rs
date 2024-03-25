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

//! Transaction extension that rejects bridge-related transactions, that include
//! obsolete (duplicated) data or do not pass some additional pallet-specific
//! checks.

use crate::{messages_call_ext::MessagesCallSubType, RefundableParachainId};
use bp_relayers::ExplicitOrAccountParams;
use pallet_bridge_grandpa::{
	BridgedBlockNumber, CallSubType as GrandpaCallSubType, SubmitFinalityProofHelper,
};
use pallet_bridge_parachains::{
	CallSubType as ParachainsCallSubtype, SubmitParachainHeadsHelper, SubmitParachainHeadsInfo,
};
use pallet_bridge_relayers::Pallet as RelayersPallet;
use sp_runtime::{
	traits::{Get, PhantomData, UniqueSaturatedInto},
	transaction_validity::{TransactionPriority, TransactionValidity, ValidTransactionBuilder},
};

/// A duplication of the `FilterCall` trait.
///
/// We need this trait in order to be able to implement it for the messages pallet,
/// since the implementation is done outside of the pallet crate.
pub trait BridgeRuntimeFilterCall<AccountId, Call> {
	/// Data that may be passed from the validate to `on_failure`.
	type ToPostDispatch;
	/// Called during validation. Needs to checks whether a runtime call, submitted
	/// by the `who` is valid. `who` may be `None` if transaction is not signed
	/// by a regular account.
	fn validate(
		who: &Option<AccountId>,
		call: &Call,
	) -> (Self::ToPostDispatch, TransactionValidity);
	/// Called after transaction is dispatched.
	fn post_dispatch(
		_who: &Option<AccountId>,
		_has_failed: bool,
		_to_on_failure: Self::ToPostDispatch,
	) {
	}
}

/// Wrapper for the bridge GRANDPA pallet that checks calls for obsolete submissions
/// and also boosts transaction priority if it has submitted by registered relayer.
/// The boost is computed as
/// `(BundledHeaderNumber - 1 - BestFinalizedHeaderNumber) * Priority::get()`.
/// The boost is only applied if submitter has active registration in the relayers
/// pallet.
pub struct CheckAndBoostBridgeGrandpaTransactions<T, I, Priority, SlashAccount>(
	PhantomData<(T, I, Priority, SlashAccount)>,
);

impl<T, I: 'static, Priority: Get<TransactionPriority>, SlashAccount: Get<T::AccountId>>
	BridgeRuntimeFilterCall<T::AccountId, T::RuntimeCall>
	for CheckAndBoostBridgeGrandpaTransactions<T, I, Priority, SlashAccount>
where
	T: pallet_bridge_relayers::Config + pallet_bridge_grandpa::Config<I>,
	T::RuntimeCall: GrandpaCallSubType<T, I>,
{
	// bridged header number, bundled in transaction
	type ToPostDispatch = Option<BridgedBlockNumber<T, I>>;

	fn validate(
		who: &Option<T::AccountId>,
		call: &T::RuntimeCall,
	) -> (Self::ToPostDispatch, TransactionValidity) {
		match GrandpaCallSubType::<T, I>::check_obsolete_submit_finality_proof(call) {
			Ok(Some(our_tx)) => {
				let to_post_dispatch = Some(our_tx.base.block_number);
				let total_priority_boost =
					compute_priority_boost::<T, _, Priority>(&who, our_tx.improved_by);
				(
					to_post_dispatch,
					ValidTransactionBuilder::default().priority(total_priority_boost).build(),
				)
			},
			Ok(None) => (None, ValidTransactionBuilder::default().build()),
			Err(e) => (None, Err(e)),
		}
	}

	fn post_dispatch(
		who: &Option<T::AccountId>,
		has_failed: bool,
		bundled_block_number: Self::ToPostDispatch,
	) {
		// we are only interested in associated pallet submissions
		let Some(bundled_block_number) = bundled_block_number else { return };
		// we are only interested in failed or unneeded transactions
		let has_failed =
			has_failed || !SubmitFinalityProofHelper::<T, I>::was_successful(bundled_block_number);

		if !has_failed {
			return
		}

		// let's slash registered relayer
		if let Some(ref relayer) = *who {
			RelayersPallet::<T>::slash_and_deregister(
				relayer,
				ExplicitOrAccountParams::Explicit(SlashAccount::get()),
			);
		}
	}
}

/// Wrapper for the bridge parachains pallet that checks calls for obsolete submissions
/// and also boosts transaction priority if it has submitted by registered relayer.
/// The boost is computed as
/// `(BundledHeaderNumber - 1 - BestKnownHeaderNumber) * Priority::get()`.
/// The boost is only applied if submitter has active registration in the relayers
/// pallet.
pub struct CheckAndBoostBridgeParachainsTransactions<T, RefPara, Priority, SlashAccount>(
	PhantomData<(T, RefPara, Priority, SlashAccount)>,
);

impl<T, RefPara, Priority: Get<TransactionPriority>, SlashAccount: Get<T::AccountId>>
	BridgeRuntimeFilterCall<T::AccountId, T::RuntimeCall>
	for CheckAndBoostBridgeParachainsTransactions<T, RefPara, Priority, SlashAccount>
where
	T: pallet_bridge_relayers::Config + pallet_bridge_parachains::Config<RefPara::Instance>,
	RefPara: RefundableParachainId,
	T::RuntimeCall: ParachainsCallSubtype<T, RefPara::Instance>,
{
	// bridged header number, bundled in transaction
	type ToPostDispatch = Option<SubmitParachainHeadsInfo>;

	fn validate(
		who: &Option<T::AccountId>,
		call: &T::RuntimeCall,
	) -> (Self::ToPostDispatch, TransactionValidity) {
		match ParachainsCallSubtype::<T, RefPara::Instance>::check_obsolete_submit_parachain_heads(
			call,
		) {
			Ok(Some(our_tx)) if our_tx.base.para_id.0 == RefPara::Id::get() => {
				let to_post_dispatch = Some(our_tx.base);
				let total_priority_boost =
					compute_priority_boost::<T, _, Priority>(&who, our_tx.improved_by);
				(
					to_post_dispatch,
					ValidTransactionBuilder::default().priority(total_priority_boost).build(),
				)
			},
			Ok(_) => (None, ValidTransactionBuilder::default().build()),
			Err(e) => (None, Err(e)),
		}
	}

	fn post_dispatch(
		who: &Option<T::AccountId>,
		has_failed: bool,
		maybe_update: Self::ToPostDispatch,
	) {
		// we are only interested in associated pallet submissions
		let Some(update) = maybe_update else { return };
		// we are only interested in failed or unneeded transactions
		let has_failed = has_failed ||
			!SubmitParachainHeadsHelper::<T, RefPara::Instance>::was_successful(&update);

		if !has_failed {
			return
		}

		// let's slash registered relayer
		if let Some(ref relayer) = *who {
			RelayersPallet::<T>::slash_and_deregister(
				relayer,
				ExplicitOrAccountParams::Explicit(SlashAccount::get()),
			);
		}
	}
}

impl<T, I: 'static> BridgeRuntimeFilterCall<T::AccountId, T::RuntimeCall>
	for pallet_bridge_grandpa::Pallet<T, I>
where
	T: pallet_bridge_grandpa::Config<I>,
	T::RuntimeCall: GrandpaCallSubType<T, I>,
{
	type ToPostDispatch = ();
	fn validate(_who: &Option<T::AccountId>, call: &T::RuntimeCall) -> ((), TransactionValidity) {
		(
			(),
			GrandpaCallSubType::<T, I>::check_obsolete_submit_finality_proof(call)
				.and_then(|_| ValidTransactionBuilder::default().build()),
		)
	}
}

impl<T, I: 'static> BridgeRuntimeFilterCall<T::AccountId, T::RuntimeCall>
	for pallet_bridge_parachains::Pallet<T, I>
where
	T: pallet_bridge_parachains::Config<I>,
	T::RuntimeCall: ParachainsCallSubtype<T, I>,
{
	type ToPostDispatch = ();
	fn validate(_who: &Option<T::AccountId>, call: &T::RuntimeCall) -> ((), TransactionValidity) {
		(
			(),
			ParachainsCallSubtype::<T, I>::check_obsolete_submit_parachain_heads(call)
				.and_then(|_| ValidTransactionBuilder::default().build()),
		)
	}
}

impl<T: pallet_bridge_messages::Config<I>, I: 'static>
	BridgeRuntimeFilterCall<T::AccountId, T::RuntimeCall> for pallet_bridge_messages::Pallet<T, I>
where
	T::RuntimeCall: MessagesCallSubType<T, I>,
{
	type ToPostDispatch = ();
	/// Validate messages in order to avoid "mining" messages delivery and delivery confirmation
	/// transactions, that are delivering outdated messages/confirmations. Without this validation,
	/// even honest relayers may lose their funds if there are multiple relays running and
	/// submitting the same messages/confirmations.
	fn validate(_who: &Option<T::AccountId>, call: &T::RuntimeCall) -> ((), TransactionValidity) {
		((), call.check_obsolete_call())
	}
}

/// Computes priority boost that improved known header by `improved_by`
fn compute_priority_boost<T, N, Priority>(
	who: &Option<T::AccountId>,
	improved_by: N,
) -> TransactionPriority
where
	T: pallet_bridge_relayers::Config,
	N: UniqueSaturatedInto<TransactionPriority>,
	Priority: Get<TransactionPriority>,
{
	// we only boost priority if relayer has staked required balance
	let is_relayer_registration_active = who
		.as_ref()
		.map(|relayer| RelayersPallet::<T>::is_registration_active(relayer))
		.unwrap_or(false);
	// if tx improves by just one, there's no need to bump its priority
	let improved_by: TransactionPriority = improved_by.unique_saturated_into().saturating_sub(1);
	// if relayer is registered, for every skipped header we improve by `Priority`
	let boost_per_header = if is_relayer_registration_active { Priority::get() } else { 0 };
	improved_by.saturating_mul(boost_per_header)
}

/// Declares a runtime-specific `BridgeRejectObsoleteHeadersAndMessages` signed extension.
///
/// ## Example
///
/// ```nocompile
/// generate_bridge_reject_obsolete_headers_and_messages!{
///     Call, AccountId
///     BridgeRococoGrandpa, BridgeRococoMessages,
///     BridgeRococoParachains
/// }
/// ```
///
/// The goal of this extension is to avoid "mining" transactions that provide outdated bridged
/// headers and messages. Without that extension, even honest relayers may lose their funds if
/// there are multiple relays running and submitting the same information.
#[macro_export]
macro_rules! generate_bridge_reject_obsolete_headers_and_messages {
	($call:ty, $account_id:ty, $($filter_call:ty),*) => {
		#[derive(Clone, codec::Decode, Default, codec::Encode, Eq, PartialEq, sp_runtime::RuntimeDebug, scale_info::TypeInfo)]
		pub struct BridgeRejectObsoleteHeadersAndMessages;
		impl sp_runtime::traits::TransactionExtensionBase for BridgeRejectObsoleteHeadersAndMessages {
			const IDENTIFIER: &'static str = "BridgeRejectObsoleteHeadersAndMessages";
			type Implicit = ();
		}
		impl<Context> sp_runtime::traits::TransactionExtension<$call, Context> for BridgeRejectObsoleteHeadersAndMessages
		where
			$account_id: Clone,
			<$call as sp_runtime::traits::Dispatchable>::RuntimeOrigin: sp_runtime::traits::AsSystemOriginSigner<$account_id>,
		{
			type Val = (
				Option<$account_id>,
				( $(
					<$filter_call as $crate::extensions::check_obsolete_extension::BridgeRuntimeFilterCall<
						$account_id,
						$call,
					>>::ToPostDispatch,
				)* ),
			);
			type Pre = Self::Val;

			fn validate(
				&self,
				origin: <$call as sp_runtime::traits::Dispatchable>::RuntimeOrigin,
				call: &$call,
				_info: &sp_runtime::traits::DispatchInfoOf<$call>,
				_len: usize,
				_context: &mut Context,
				_self_implicit: Self::Implicit,
				_inherited_implication: &impl codec::Encode,
			) -> Result<
				(
					sp_runtime::transaction_validity::ValidTransaction,
					Self::Val,
					<$call as sp_runtime::traits::Dispatchable>::RuntimeOrigin,
				), sp_runtime::transaction_validity::TransactionValidityError
			> {
				use tuplex::PushBack;
				use sp_runtime::traits::AsSystemOriginSigner;
				let maybe_relayer = origin.as_system_origin_signer().cloned();
				let tx_validity = sp_runtime::transaction_validity::ValidTransaction::default();
				let to_prepare = ();
				$(
					let (from_validate, call_filter_validity) = <
						$filter_call as
						$crate::extensions::check_obsolete_extension::BridgeRuntimeFilterCall<
							$account_id,
							$call,
						>>::validate(&maybe_relayer, call);
					let tx_validity = tx_validity.combine_with(call_filter_validity?);
					let to_prepare = to_prepare.push_back(from_validate);
				)*
				Ok((tx_validity, (maybe_relayer, to_prepare), origin))
			}

			fn prepare(
				self,
				to_post_dispatch: Self::Val,
				_origin: &<$call as sp_runtime::traits::Dispatchable>::RuntimeOrigin,
				_call: &$call,
				_info: &sp_runtime::traits::DispatchInfoOf<$call>,
				_len: usize,
				_context: &Context,
			) -> Result<Self::Pre, sp_runtime::transaction_validity::TransactionValidityError> {
				Ok(to_post_dispatch)
			}

			#[allow(unused_variables)]
			fn post_dispatch(
				to_post_dispatch: Self::Pre,
				_info: &sp_runtime::traits::DispatchInfoOf<$call>,
				_post_info: &sp_runtime::traits::PostDispatchInfoOf<$call>,
				_len: usize,
				result: &sp_runtime::DispatchResult,
				_context: &Context,
			) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
				if result.is_ok() {
					return Ok(());
				}

				use tuplex::PopFront;
				let has_failed = result.is_err();
				let (maybe_relayer, to_post_dispatch) = to_post_dispatch;
				$(
					let (item, to_post_dispatch) = to_post_dispatch.pop_front();
					<
						$filter_call as
						$crate::extensions::check_obsolete_extension::BridgeRuntimeFilterCall<
							$account_id,
							$call,
						>>::post_dispatch(&maybe_relayer, has_failed, item);
				)*
				Ok(())
			}
		}
	};
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		extensions::refund_relayer_extension::tests::{
			initialize_environment, relayer_account_at_this_chain, submit_parachain_head_call_ex,
			submit_relay_header_call_ex, TestParachain,
		},
		mock::*,
		DefaultRefundableParachainId,
	};
	use bp_polkadot_core::parachains::ParaId;
	use codec::Encode;
	use frame_support::assert_err;
	use sp_runtime::{
		traits::{ConstU64, DispatchTransaction},
		transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
	};

	#[derive(Clone, Debug, PartialEq)]
	pub struct AccountId;

	impl sp_runtime::traits::AsSystemOriginSigner<AccountId> for AccountId {
		fn as_system_origin_signer(&self) -> Option<&AccountId> {
			None
		}
	}

	#[derive(Encode)]
	pub struct MockCall {
		data: u32,
	}

	impl sp_runtime::traits::Dispatchable for MockCall {
		type RuntimeOrigin = AccountId;
		type Config = ();
		type Info = ();
		type PostInfo = ();

		fn dispatch(
			self,
			_origin: Self::RuntimeOrigin,
		) -> sp_runtime::DispatchResultWithInfo<Self::PostInfo> {
			unimplemented!()
		}
	}

	pub struct FirstFilterCall;
	impl BridgeRuntimeFilterCall<AccountId, MockCall> for FirstFilterCall {
		type ToPostDispatch = u64;
		fn validate(_who: &Option<AccountId>, call: &MockCall) -> (u64, TransactionValidity) {
			if call.data <= 1 {
				return (1, InvalidTransaction::Custom(1).into())
			}

			(1, Ok(ValidTransaction { priority: 1, ..Default::default() }))
		}
	}

	pub struct SecondFilterCall;
	impl BridgeRuntimeFilterCall<AccountId, MockCall> for SecondFilterCall {
		type ToPostDispatch = u64;
		fn validate(_who: &Option<AccountId>, call: &MockCall) -> (u64, TransactionValidity) {
			if call.data <= 2 {
				return (2, InvalidTransaction::Custom(2).into())
			}

			(2, Ok(ValidTransaction { priority: 2, ..Default::default() }))
		}
	}

	#[test]
	fn test() {
		generate_bridge_reject_obsolete_headers_and_messages!(
			MockCall,
			AccountId,
			FirstFilterCall,
			SecondFilterCall
		);

		assert_err!(
			BridgeRejectObsoleteHeadersAndMessages.validate_only(
				AccountId,
				&MockCall { data: 1 },
				&(),
				0
			),
			InvalidTransaction::Custom(1)
		);

		assert_err!(
			BridgeRejectObsoleteHeadersAndMessages.validate_only(
				AccountId,
				&MockCall { data: 2 },
				&(),
				0
			),
			InvalidTransaction::Custom(2)
		);

		let result = BridgeRejectObsoleteHeadersAndMessages
			.validate_only(AccountId, &MockCall { data: 3 }, &(), 0)
			.unwrap();
		assert_eq!(result.0, ValidTransaction { priority: 3, ..Default::default() });
		assert_eq!(result.1, (None, (1, 2)));
	}

	frame_support::parameter_types! {
		pub SlashDestination: ThisChainAccountId = 42;
	}

	type BridgeGrandpaWrapper =
		CheckAndBoostBridgeGrandpaTransactions<TestRuntime, (), ConstU64<1_000>, SlashDestination>;

	#[test]
	fn grandpa_wrapper_does_not_boost_extensions_for_unregistered_relayer() {
		run_test(|| {
			initialize_environment(100, 100, 100);

			let priority_boost = BridgeGrandpaWrapper::validate(
				&Some(relayer_account_at_this_chain()),
				&submit_relay_header_call_ex(200),
			)
			.1
			.unwrap()
			.priority;
			assert_eq!(priority_boost, 0);
		})
	}

	#[test]
	fn grandpa_wrapper_boosts_extensions_for_registered_relayer() {
		run_test(|| {
			initialize_environment(100, 100, 100);
			BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
				.unwrap();

			let priority_boost = BridgeGrandpaWrapper::validate(
				&Some(relayer_account_at_this_chain()),
				&submit_relay_header_call_ex(200),
			)
			.1
			.unwrap()
			.priority;
			assert_eq!(priority_boost, 99_000);
		})
	}

	#[test]
	fn grandpa_wrapper_slashes_registered_relayer_if_transaction_fails() {
		run_test(|| {
			initialize_environment(100, 100, 100);
			BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
				.unwrap();

			assert!(BridgeRelayers::is_registration_active(&relayer_account_at_this_chain()));
			BridgeGrandpaWrapper::post_dispatch(
				&Some(relayer_account_at_this_chain()),
				true,
				Some(150),
			);
			assert!(!BridgeRelayers::is_registration_active(&relayer_account_at_this_chain()));
		})
	}

	#[test]
	fn grandpa_wrapper_does_not_slash_registered_relayer_if_transaction_succeeds() {
		run_test(|| {
			initialize_environment(100, 100, 100);
			BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
				.unwrap();

			assert!(BridgeRelayers::is_registration_active(&relayer_account_at_this_chain()));
			BridgeGrandpaWrapper::post_dispatch(
				&Some(relayer_account_at_this_chain()),
				false,
				Some(100),
			);
			assert!(BridgeRelayers::is_registration_active(&relayer_account_at_this_chain()));
		})
	}

	type BridgeParachainsWrapper = CheckAndBoostBridgeParachainsTransactions<
		TestRuntime,
		DefaultRefundableParachainId<(), TestParachain>,
		ConstU64<1_000>,
		SlashDestination,
	>;

	#[test]
	fn parachains_wrapper_does_not_boost_extensions_for_unregistered_relayer() {
		run_test(|| {
			initialize_environment(100, 100, 100);

			let priority_boost = BridgeParachainsWrapper::validate(
				&Some(relayer_account_at_this_chain()),
				&submit_parachain_head_call_ex(200),
			)
			.1
			.unwrap()
			.priority;
			assert_eq!(priority_boost, 0);
		})
	}

	#[test]
	fn parachains_wrapper_boosts_extensions_for_registered_relayer() {
		run_test(|| {
			initialize_environment(100, 100, 100);
			BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
				.unwrap();

			let priority_boost = BridgeParachainsWrapper::validate(
				&Some(relayer_account_at_this_chain()),
				&submit_parachain_head_call_ex(200),
			)
			.1
			.unwrap()
			.priority;
			assert_eq!(priority_boost, 99_000);
		})
	}

	#[test]
	fn parachains_wrapper_slashes_registered_relayer_if_transaction_fails() {
		run_test(|| {
			initialize_environment(100, 100, 100);
			BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
				.unwrap();

			assert!(BridgeRelayers::is_registration_active(&relayer_account_at_this_chain()));
			BridgeParachainsWrapper::post_dispatch(
				&Some(relayer_account_at_this_chain()),
				true,
				Some(SubmitParachainHeadsInfo {
					at_relay_block: (150, Default::default()),
					para_id: ParaId(TestParachain::get()),
					para_head_hash: [150u8; 32].into(),
					is_free_execution_expected: false,
				}),
			);
			assert!(!BridgeRelayers::is_registration_active(&relayer_account_at_this_chain()));
		})
	}

	#[test]
	fn parachains_wrapper_does_not_slash_registered_relayer_if_transaction_succeeds() {
		run_test(|| {
			initialize_environment(100, 100, 100);
			BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
				.unwrap();

			assert!(BridgeRelayers::is_registration_active(&relayer_account_at_this_chain()));
			BridgeParachainsWrapper::post_dispatch(
				&Some(relayer_account_at_this_chain()),
				false,
				Some(SubmitParachainHeadsInfo {
					at_relay_block: (100, Default::default()),
					para_id: ParaId(TestParachain::get()),
					para_head_hash: [100u8; 32].into(),
					is_free_execution_expected: false,
				}),
			);
			assert!(BridgeRelayers::is_registration_active(&relayer_account_at_this_chain()));
		})
	}
}
