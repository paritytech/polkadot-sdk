// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! Runtime types for integrating `pallet-revive` with the EVM.
#![allow(unused_imports, unused_variables)]
use crate::{
	evm::api::{TransactionLegacySigned, TransactionLegacyUnsigned, TransactionUnsigned},
	AccountIdOf, AddressMapper, BalanceOf, Config, EthInstantiateInput, EthTransactKind, MomentOf,
	Weight, LOG_TARGET,
};
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::ExtrinsicCall,
	CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::TypeInfo;
use sp_arithmetic::Percent;
use sp_core::{ecdsa, ed25519, sr25519, Get, H160, U256};
use sp_runtime::{
	generic::{self, CheckedExtrinsic},
	traits::{
		self, Checkable, Convert, DispatchInfoOf, Dispatchable, Extrinsic, ExtrinsicMetadata, Lazy,
		Member, SignedExtension, Verify,
	},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransaction,
	},
	AccountId32, MultiSignature, MultiSigner, RuntimeDebug, Saturating,
};

#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

/// Some way of identifying an account on the chain.
pub type AccountId = AccountId32;

/// The type for looking up accounts. We don't expect more than 4 billion of them.
pub type AccountIndex = u32;

/// The address format for describing accounts.
pub type MultiAddress = sp_runtime::MultiAddress<AccountId, AccountIndex>;

/// Wraps [`generic::UncheckedExtrinsic`] to support checking [`crate::Call::eth_transact`].
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(E))]
pub struct UncheckedExtrinsic<Call, E: EthExtra>(
	generic::UncheckedExtrinsic<MultiAddress, Call, MultiSignature, E::Extra>,
);

impl<Call: TypeInfo, E: EthExtra> Extrinsic for UncheckedExtrinsic<Call, E> {
	type Call = Call;

	type SignaturePayload = (MultiAddress, MultiSignature, E::Extra);

	fn is_signed(&self) -> Option<bool> {
		self.0.is_signed()
	}

	fn new(function: Call, signed_data: Option<Self::SignaturePayload>) -> Option<Self> {
		Some(if let Some((address, signature, extra)) = signed_data {
			Self(generic::UncheckedExtrinsic::new_signed(function, address, signature, extra))
		} else {
			Self(generic::UncheckedExtrinsic::new_unsigned(function))
		})
	}
}

impl<Call, E: EthExtra> ExtrinsicMetadata for UncheckedExtrinsic<Call, E> {
	const VERSION: u8 =
		generic::UncheckedExtrinsic::<MultiAddress, Call, MultiSignature, E::Extra>::VERSION;
	type SignedExtensions = E::Extra;
}

impl<Call: TypeInfo, E: EthExtra> ExtrinsicCall for UncheckedExtrinsic<Call, E> {
	fn call(&self) -> &Self::Call {
		self.0.call()
	}
}

impl<Call, E, Lookup> Checkable<Lookup> for UncheckedExtrinsic<Call, E>
where
	Call: Encode + Member,
	E: EthExtra,
	<E::Config as frame_system::Config>::Nonce: Into<U256>,
	BalanceOf<E::Config>: Into<U256> + TryFrom<U256>,
	MomentOf<E::Config>: Into<U256>,

	AccountIdOf<E::Config>: Into<AccountId32>,
	Call: From<crate::Call<E::Config>> + TryInto<crate::Call<E::Config>>,
	E::Extra: SignedExtension<AccountId = AccountId32>,
	Lookup: traits::Lookup<Source = MultiAddress, Target = AccountId32>,
{
	type Checked = CheckedExtrinsic<AccountId32, Call, E::Extra>;

	fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
		if self.0.signature.is_none() {
			if let Ok(call) = self.0.function.clone().try_into() {
				if let crate::Call::eth_transact {
					payload,
					gas_limit,
					storage_deposit_limit,
					transact_kind,
				} = call
				{
					let checked = E::try_into_checked_extrinsic(
						payload,
						gas_limit,
						storage_deposit_limit,
						transact_kind,
					)?;
					return Ok(checked)
				};
			}
		}
		self.0.check(lookup)
	}

	#[cfg(feature = "try-runtime")]
	fn unchecked_into_checked_i_know_what_i_am_doing(
		self,
		_: &Lookup,
	) -> Result<Self::Checked, TransactionValidityError> {
		unreachable!();
	}
}

impl<Call, E> GetDispatchInfo for UncheckedExtrinsic<Call, E>
where
	Call: GetDispatchInfo,
	E: EthExtra,
{
	fn get_dispatch_info(&self) -> DispatchInfo {
		self.0.get_dispatch_info()
	}
}

impl<Call: Encode, E: EthExtra> serde::Serialize for UncheckedExtrinsic<Call, E> {
	fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error>
	where
		S: ::serde::Serializer,
	{
		self.0.serialize(seq)
	}
}

impl<'a, Call: Decode, E: EthExtra> serde::Deserialize<'a> for UncheckedExtrinsic<Call, E> {
	fn deserialize<D>(de: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'a>,
	{
		let r = sp_core::bytes::deserialize(de)?;
		Decode::decode(&mut &r[..])
			.map_err(|e| serde::de::Error::custom(sp_runtime::format!("Decode error: {}", e)))
	}
}

/// EthExtra convert an unsigned [`crate::Call::eth_transact`] into a [`CheckedExtrinsic`].
pub trait EthExtra {
	/// The Runtime configuration.
	type Config: crate::Config;

	/// The Runtime's signed extension.
	type Extra: SignedExtension<AccountId = AccountId32>;

	/// Get the signed extensions to apply to an unsigned [`crate::Call::eth_transact`] extrinsic.
	fn get_eth_transact_extra(nonce: U256) -> Self::Extra;

	/// Convert the unsigned [`crate::Call::eth_transact`] into a [`CheckedExtrinsic`].
	fn try_into_checked_extrinsic<Call>(
		payload: Vec<u8>,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<Self::Config>,
		transact_kind: EthTransactKind,
	) -> Result<CheckedExtrinsic<AccountId32, Call, Self::Extra>, InvalidTransaction>
	where
		<Self::Config as frame_system::Config>::Nonce: Into<U256>,
		BalanceOf<Self::Config>: Into<U256> + TryFrom<U256>,
		MomentOf<Self::Config>: Into<U256>,
		AccountIdOf<Self::Config>: Into<AccountId32>,
		Call: From<crate::Call<Self::Config>> + TryInto<crate::Call<Self::Config>>,
	{
		let tx = rlp::decode::<TransactionLegacySigned>(&payload).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to decode transaction: {err:?}");
			InvalidTransaction::Call
		})?;

		let signer = tx.recover_eth_address().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to recover signer: {err:?}");
			InvalidTransaction::BadProof
		})?;

		let signer =
			<Self::Config as crate::Config>::AddressMapper::to_account_id_contract(&signer);
		let TransactionLegacyUnsigned { nonce, chain_id, to, value, input, gas, gas_price, .. } =
			tx.transaction_legacy_unsigned;

		if chain_id.unwrap_or_default() != <Self::Config as crate::Config>::ChainId::get().into() {
			log::debug!(target: LOG_TARGET, "Invalid chain_id {chain_id:?}");
			return Err(InvalidTransaction::Call);
		}

		let account_nonce: U256 = <crate::System<Self::Config>>::account_nonce(&signer).into();
		if nonce > account_nonce {
			log::debug!(target: LOG_TARGET, "Invalid nonce: expected {account_nonce}, got {nonce}");
			return Err(InvalidTransaction::Future);
		} else if nonce < account_nonce {
			log::debug!(target: LOG_TARGET, "Invalid nonce: expected {account_nonce}, got {nonce}");
			return Err(InvalidTransaction::Stale);
		}

		let call = if let Some(dest) = to {
			if !matches!(transact_kind, EthTransactKind::Call) {
				log::debug!(target: LOG_TARGET, "Invalid transact_kind, expected Call");
				return Err(InvalidTransaction::Call);
			}

			crate::Call::call::<Self::Config> {
				dest,
				value: value.try_into().map_err(|_| InvalidTransaction::Call)?,
				gas_limit,
				storage_deposit_limit,
				data: input.0,
			}
		} else {
			let EthTransactKind::InstantiateWithCode { code_len, data_len } = transact_kind else {
				log::debug!(target: LOG_TARGET, "Invalid transact_kind, expected InstantiateWithCode");
				return Err(InvalidTransaction::Call);
			};

			let EthInstantiateInput { code, data } = EthInstantiateInput::decode(&mut &input.0[..])
				.map_err(|_| {
					log::debug!(target: LOG_TARGET, "Failed to decoded eth_transact input");
					InvalidTransaction::Call
				})?;

			if code.len() as u32 != code_len || data.len() as u32 != data_len {
				log::debug!(target: LOG_TARGET, "Invalid code or data length");
				return Err(InvalidTransaction::Call);
			}

			crate::Call::instantiate_with_code::<Self::Config> {
				value: value.try_into().map_err(|_| InvalidTransaction::Call)?,
				gas_limit,
				storage_deposit_limit,
				code,
				data,
				salt: None,
			}
		};

		let dispatch_info = call.get_dispatch_info();
		let call_fee = <Self::Config as crate::Config>::WeightPrice::convert(dispatch_info.weight)
			.saturating_add(storage_deposit_limit);

		let eth_fee =
			gas_price.saturating_mul(gas).try_into().map_err(|_| InvalidTransaction::Call)?;

		// Make sure that that the fee computed from the signed payload is no more than 5% greater
		// than the actual fee computed with the injected transaction parameters.
		if Percent::from_rational(eth_fee, call_fee) > Percent::from_percent(105) {
			log::debug!(target: LOG_TARGET, "Expected fees {eth_fee:?} to be within 5% of calculated fees {call_fee:?}");
			return Err(InvalidTransaction::Call.into())
		}

		Ok(CheckedExtrinsic {
			signed: Some((signer.into(), Self::get_eth_transact_extra(nonce))),
			function: call.into(),
		})
	}
}
