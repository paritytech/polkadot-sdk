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

use crate::messages_call_ext::MessagesCallSubType;
use bp_relayers::ExplicitOrAccountParams;
use pallet_bridge_grandpa::{
	BridgedBlockNumber, CallSubType as GrandpaCallSubType, SubmitFinalityProofHelper,
};
use pallet_bridge_parachains::CallSubType as ParachainsCallSubtype;
use pallet_bridge_relayers::Pallet as RelayersPallet;
use sp_runtime::{
	traits::{Get, One, PhantomData, UniqueSaturatedInto},
	transaction_validity::{TransactionPriority, TransactionValidity, ValidTransactionBuilder},
	Saturating,
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
	fn validate(who: &AccountId, call: &Call) -> (Self::ToPostDispatch, TransactionValidity);
	/// Called after transaction is dispatched.
	fn post_dispatch(_who: &AccountId, _has_failed: bool, _to_on_failure: Self::ToPostDispatch) {}
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
		who: &T::AccountId,
		call: &T::RuntimeCall,
	) -> (Self::ToPostDispatch, TransactionValidity) {
		// we only boost priority if relayer has staked required balance
		let is_relayer_registration_active = RelayersPallet::<T>::is_registration_active(who);

		match GrandpaCallSubType::<T, I>::check_obsolete_submit_finality_proof(call) {
			Ok(Some(our_tx)) => {
				let block_number = Some(our_tx.base.block_number);
				let improved_by: TransactionPriority =
					our_tx.improved_by.saturating_sub(One::one()).unique_saturated_into();
				let boost_per_header =
					if is_relayer_registration_active { Priority::get() } else { 0 };
				let total_priority_boost = improved_by.saturating_mul(boost_per_header);
				(
					block_number,
					ValidTransactionBuilder::default().priority(total_priority_boost).build(),
				)
			},
			Ok(None) => (None, ValidTransactionBuilder::default().build()),
			Err(e) => (None, Err(e)),
		}
	}

	fn post_dispatch(
		relayer: &T::AccountId,
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
		RelayersPallet::<T>::slash_and_deregister(
			relayer,
			ExplicitOrAccountParams::Explicit(SlashAccount::get()),
		);
	}
}

impl<T, I: 'static> BridgeRuntimeFilterCall<T::AccountId, T::RuntimeCall>
	for pallet_bridge_grandpa::Pallet<T, I>
where
	T: pallet_bridge_grandpa::Config<I>,
	T::RuntimeCall: GrandpaCallSubType<T, I>,
{
	type ToPostDispatch = ();
	fn validate(_who: &T::AccountId, call: &T::RuntimeCall) -> ((), TransactionValidity) {
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
	fn validate(_who: &T::AccountId, call: &T::RuntimeCall) -> ((), TransactionValidity) {
		((), ParachainsCallSubtype::<T, I>::check_obsolete_submit_parachain_heads(call))
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
	fn validate(_who: &T::AccountId, call: &T::RuntimeCall) -> ((), TransactionValidity) {
		((), call.check_obsolete_call())
	}
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
		impl sp_runtime::traits::SignedExtension for BridgeRejectObsoleteHeadersAndMessages {
			const IDENTIFIER: &'static str = "BridgeRejectObsoleteHeadersAndMessages";
			type AccountId = $account_id;
			type Call = $call;
			type AdditionalSigned = ();
			type Pre = (
				$account_id,
				( $(
					<$filter_call as $crate::extensions::check_obsolete_extension::BridgeRuntimeFilterCall<
						$account_id,
						$call,
					>>::ToPostDispatch,
				)* ),
			);

			fn additional_signed(&self) -> sp_std::result::Result<
				(),
				sp_runtime::transaction_validity::TransactionValidityError,
			> {
				Ok(())
			}

			#[allow(unused_variables)]
			fn validate(
				&self,
				who: &Self::AccountId,
				call: &Self::Call,
				_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				_len: usize,
			) -> sp_runtime::transaction_validity::TransactionValidity {
				let tx_validity = sp_runtime::transaction_validity::ValidTransaction::default();
				let to_prepare = ();
				$(
					let (from_validate, call_filter_validity) = <
						$filter_call as
						$crate::extensions::check_obsolete_extension::BridgeRuntimeFilterCall<
							Self::AccountId,
							$call,
						>>::validate(&who, call);
					let tx_validity = tx_validity.combine_with(call_filter_validity?);
				)*
				Ok(tx_validity)
			}

			#[allow(unused_variables)]
			fn pre_dispatch(
				self,
				relayer: &Self::AccountId,
				call: &Self::Call,
				info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				len: usize,
			) -> Result<Self::Pre, sp_runtime::transaction_validity::TransactionValidityError> {
				use tuplex::PushBack;
				let to_post_dispatch = ();
				$(
					let (from_validate, call_filter_validity) = <
						$filter_call as
						$crate::extensions::check_obsolete_extension::BridgeRuntimeFilterCall<
							$account_id,
							$call,
						>>::validate(&relayer, call);
					let _ = call_filter_validity?;
					let to_post_dispatch = to_post_dispatch.push_back(from_validate);
				)*
				Ok((relayer.clone(), to_post_dispatch))
			}

			#[allow(unused_variables)]
			fn post_dispatch(
				to_post_dispatch: Option<Self::Pre>,
				info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				post_info: &sp_runtime::traits::PostDispatchInfoOf<Self::Call>,
				len: usize,
				result: &sp_runtime::DispatchResult,
			) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
				// TODO: check me: removed if result.is_ok() { return Ok(()); }
				use tuplex::PopFront;
				let has_failed = result.is_err();
				// TODO: check me: return if `to_post_dispatch` is `None`
				let Some((relayer, to_post_dispatch)) = to_post_dispatch else { return Ok(()) };
				$(
					let (item, to_post_dispatch) = to_post_dispatch.pop_front();
					<
						$filter_call as
						$crate::extensions::check_obsolete_extension::BridgeRuntimeFilterCall<
							$account_id,
							$call,
						>>::post_dispatch(&relayer, has_failed, item);
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
			initialize_environment, relayer_account_at_this_chain, submit_relay_header_call_ex,
		},
		mock::*,
	};
	use frame_support::assert_err;
	use sp_runtime::{
		traits::{ConstU64, SignedExtension},
		transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
	};

	pub struct MockCall {
		data: u32,
	}

	impl sp_runtime::traits::Dispatchable for MockCall {
		type RuntimeOrigin = u64;
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
	impl BridgeRuntimeFilterCall<u64, MockCall> for FirstFilterCall {
		type ToPostDispatch = u64;
		fn validate(_who: &u64, call: &MockCall) -> (u64, TransactionValidity) {
			if call.data <= 1 {
				return (1, InvalidTransaction::Custom(1).into())
			}

			(1, Ok(ValidTransaction { priority: 1, ..Default::default() }))
		}
	}

	pub struct SecondFilterCall;
	impl BridgeRuntimeFilterCall<u64, MockCall> for SecondFilterCall {
		type ToPostDispatch = u64;
		fn validate(_who: &u64, call: &MockCall) -> (u64, TransactionValidity) {
			if call.data <= 2 {
				return (2, InvalidTransaction::Custom(2).into())
			}

			(2, Ok(ValidTransaction { priority: 2, ..Default::default() }))
		}
	}

	#[test]
	fn test_generated_obsolete_extension() {
		generate_bridge_reject_obsolete_headers_and_messages!(
			MockCall,
			u64,
			FirstFilterCall,
			SecondFilterCall
		);

		// TODO: add tests for both validate and pre_dispatch here?

		assert_err!(
			BridgeRejectObsoleteHeadersAndMessages.validate(&42, &MockCall { data: 1 }, &(), 0),
			InvalidTransaction::Custom(1)
		);

		assert_err!(
			BridgeRejectObsoleteHeadersAndMessages.validate(&42, &MockCall { data: 2 }, &(), 0),
			InvalidTransaction::Custom(2)
		);

		assert_eq!(
			BridgeRejectObsoleteHeadersAndMessages
				.validate(&42, &MockCall { data: 3 }, &(), 0)
				.unwrap(),
			ValidTransaction { priority: 3, ..Default::default() },
		);
		assert_eq!(
			BridgeRejectObsoleteHeadersAndMessages
				.pre_dispatch(&42, &MockCall { data: 3 }, &(), 0)
				.unwrap(),
			(42, (1, 2)),
		);
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
				&relayer_account_at_this_chain(),
				&submit_relay_header_call_ex(200),
			)
			.1
			.unwrap()
			.priority;
			assert_eq!(priority_boost, 0);
		})
	}

	#[test]
	fn grandpa_wrapper_boosts_extensions_for_unregistered_relayer() {
		run_test(|| {
			initialize_environment(100, 100, 100);
			BridgeRelayers::register(RuntimeOrigin::signed(relayer_account_at_this_chain()), 1000)
				.unwrap();

			let priority_boost = BridgeGrandpaWrapper::validate(
				&relayer_account_at_this_chain(),
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
			BridgeGrandpaWrapper::post_dispatch(&relayer_account_at_this_chain(), true, Some(150));
			assert!(!BridgeRelayers::is_registration_active(&relayer_account_at_this_chain()));
		})
	}
}
