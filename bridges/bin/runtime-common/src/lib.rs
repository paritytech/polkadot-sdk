// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

#![cfg_attr(not(feature = "std"), no_std)]

use bp_runtime::FilterCall;
use sp_runtime::transaction_validity::TransactionValidity;
use xcm::v3::NetworkId;

pub mod messages;
pub mod messages_api;
pub mod messages_benchmarking;
pub mod messages_extension;
pub mod parachains_benchmarking;

#[cfg(feature = "integrity-test")]
pub mod integrity;

/// A duplication of the `FilterCall` trait.
///
/// We need this trait in order to be able to implement it for the messages pallet,
/// since the implementation is done outside of the pallet crate.
pub trait BridgeRuntimeFilterCall<Call> {
	/// Checks if a runtime call is valid.
	fn validate(call: &Call) -> TransactionValidity;
}

impl<Call, T, I> BridgeRuntimeFilterCall<Call> for pallet_bridge_grandpa::Pallet<T, I>
where
	pallet_bridge_grandpa::Pallet<T, I>: FilterCall<Call>,
{
	fn validate(call: &Call) -> TransactionValidity {
		<pallet_bridge_grandpa::Pallet<T, I> as FilterCall<Call>>::validate(call)
	}
}

impl<Call, T, I> BridgeRuntimeFilterCall<Call> for pallet_bridge_parachains::Pallet<T, I>
where
	pallet_bridge_parachains::Pallet<T, I>: FilterCall<Call>,
{
	fn validate(call: &Call) -> TransactionValidity {
		<pallet_bridge_parachains::Pallet<T, I> as FilterCall<Call>>::validate(call)
	}
}

/// Declares a runtime-specific `BridgeRejectObsoleteHeadersAndMessages` signed extension.
///
/// ## Example
///
/// ```nocompile
/// generate_bridge_reject_obsolete_headers_and_messages!{
///     Call, AccountId
///     BridgeRialtoGrandpa, BridgeWestendGrandpa,
///     BridgeRialtoParachains
/// }
/// ```
///
/// The goal of this extension is to avoid "mining" transactions that provide outdated bridged
/// headers and messages. Without that extension, even honest relayers may lose their funds if
/// there are multiple relays running and submitting the same information.
#[macro_export]
macro_rules! generate_bridge_reject_obsolete_headers_and_messages {
	($call:ty, $account_id:ty, $($filter_call:ty),*) => {
		#[derive(Clone, codec::Decode, codec::Encode, Eq, PartialEq, frame_support::RuntimeDebug, scale_info::TypeInfo)]
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

/// A mapping over `NetworkId`.
/// Since `NetworkId` doesn't include `Millau`, `Rialto` and `RialtoParachain`, we create some
/// synthetic associations between these chains and `NetworkId` chains.
pub enum CustomNetworkId {
	/// The Millau network ID, associated with Kusama.
	Millau,
	/// The Rialto network ID, associated with Polkadot.
	Rialto,
	/// The RialtoParachain network ID, associated with Westend.
	RialtoParachain,
}

impl CustomNetworkId {
	pub const fn as_network_id(&self) -> NetworkId {
		match *self {
			CustomNetworkId::Millau => NetworkId::Kusama,
			CustomNetworkId::Rialto => NetworkId::Polkadot,
			CustomNetworkId::RialtoParachain => NetworkId::Westend,
		}
	}
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
