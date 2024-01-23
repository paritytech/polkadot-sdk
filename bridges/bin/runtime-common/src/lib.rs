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

//! Common types/functions that may be used by runtimes of all bridged chains.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use crate::messages_call_ext::MessagesCallSubType;
use pallet_bridge_grandpa::CallSubType as GrandpaCallSubType;
use pallet_bridge_parachains::CallSubType as ParachainsCallSubtype;
use sp_runtime::transaction_validity::TransactionValidity;

pub mod messages;
pub mod messages_api;
pub mod messages_benchmarking;
pub mod messages_call_ext;
pub mod messages_generation;
pub mod messages_xcm_extension;
pub mod parachains_benchmarking;
pub mod priority_calculator;
pub mod refund_relayer_extension;

mod mock;

#[cfg(feature = "integrity-test")]
pub mod integrity;

const LOG_TARGET_BRIDGE_DISPATCH: &str = "runtime::bridge-dispatch";

/// A duplication of the `FilterCall` trait.
///
/// We need this trait in order to be able to implement it for the messages pallet,
/// since the implementation is done outside of the pallet crate.
pub trait BridgeRuntimeFilterCall<Call> {
	/// Checks if a runtime call is valid.
	fn validate(call: &Call) -> TransactionValidity;
}

impl<T, I: 'static> BridgeRuntimeFilterCall<T::RuntimeCall> for pallet_bridge_grandpa::Pallet<T, I>
where
	T: pallet_bridge_grandpa::Config<I>,
	T::RuntimeCall: GrandpaCallSubType<T, I>,
{
	fn validate(call: &T::RuntimeCall) -> TransactionValidity {
		GrandpaCallSubType::<T, I>::check_obsolete_submit_finality_proof(call)
	}
}

impl<T, I: 'static> BridgeRuntimeFilterCall<T::RuntimeCall>
	for pallet_bridge_parachains::Pallet<T, I>
where
	T: pallet_bridge_parachains::Config<I>,
	T::RuntimeCall: ParachainsCallSubtype<T, I>,
{
	fn validate(call: &T::RuntimeCall) -> TransactionValidity {
		ParachainsCallSubtype::<T, I>::check_obsolete_submit_parachain_heads(call)
	}
}

impl<T: pallet_bridge_messages::Config<I>, I: 'static> BridgeRuntimeFilterCall<T::RuntimeCall>
	for pallet_bridge_messages::Pallet<T, I>
where
	T::RuntimeCall: MessagesCallSubType<T, I>,
{
	/// Validate messages in order to avoid "mining" messages delivery and delivery confirmation
	/// transactions, that are delivering outdated messages/confirmations. Without this validation,
	/// even honest relayers may lose their funds if there are multiple relays running and
	/// submitting the same messages/confirmations.
	fn validate(call: &T::RuntimeCall) -> TransactionValidity {
		call.check_obsolete_call()
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
			type Pre = ();

			fn additional_signed(&self) -> sp_std::result::Result<
				(),
				sp_runtime::transaction_validity::TransactionValidityError,
			> {
				Ok(())
			}

			fn validate(
				&self,
				_who: &Self::AccountId,
				call: &Self::Call,
				_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				_len: usize,
			) -> sp_runtime::transaction_validity::TransactionValidity {
				let valid = sp_runtime::transaction_validity::ValidTransaction::default();
				$(
					let valid = valid
						.combine_with(<$filter_call as $crate::BridgeRuntimeFilterCall<$call>>::validate(call)?);
				)*
				Ok(valid)
			}

			fn pre_dispatch(
				self,
				who: &Self::AccountId,
				call: &Self::Call,
				info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				len: usize,
			) -> Result<Self::Pre, sp_runtime::transaction_validity::TransactionValidityError> {
				self.validate(who, call, info, len).map(drop)
			}
		}
	};
}

#[cfg(test)]
mod tests {
	use crate::BridgeRuntimeFilterCall;
	use frame_support::{assert_err, assert_ok};
	use sp_runtime::{
		traits::SignedExtension,
		transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
	};

	pub struct MockCall {
		data: u32,
	}

	impl sp_runtime::traits::Dispatchable for MockCall {
		type RuntimeOrigin = ();
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

	struct FirstFilterCall;
	impl BridgeRuntimeFilterCall<MockCall> for FirstFilterCall {
		fn validate(call: &MockCall) -> TransactionValidity {
			if call.data <= 1 {
				return InvalidTransaction::Custom(1).into()
			}

			Ok(ValidTransaction { priority: 1, ..Default::default() })
		}
	}

	struct SecondFilterCall;
	impl BridgeRuntimeFilterCall<MockCall> for SecondFilterCall {
		fn validate(call: &MockCall) -> TransactionValidity {
			if call.data <= 2 {
				return InvalidTransaction::Custom(2).into()
			}

			Ok(ValidTransaction { priority: 2, ..Default::default() })
		}
	}

	#[test]
	fn test() {
		generate_bridge_reject_obsolete_headers_and_messages!(
			MockCall,
			(),
			FirstFilterCall,
			SecondFilterCall
		);

		assert_err!(
			BridgeRejectObsoleteHeadersAndMessages.validate(&(), &MockCall { data: 1 }, &(), 0),
			InvalidTransaction::Custom(1)
		);

		assert_err!(
			BridgeRejectObsoleteHeadersAndMessages.validate(&(), &MockCall { data: 2 }, &(), 0),
			InvalidTransaction::Custom(2)
		);

		assert_ok!(
			BridgeRejectObsoleteHeadersAndMessages.validate(&(), &MockCall { data: 3 }, &(), 0),
			ValidTransaction { priority: 3, ..Default::default() }
		)
	}
}
