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
	AccountIdOf, AddressMapper, BalanceOf, Config, MomentOf, Weight, LOG_TARGET,
};
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo, PostDispatchInfo},
	traits::ExtrinsicCall,
	CloneNoBound, DebugNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound,
};
use pallet_transaction_payment::OnChargeTransaction;
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

use alloc::{vec, vec::Vec};

/// Some way of identifying an account on the chain.
pub type AccountId = AccountId32;

/// The type for looking up accounts. We don't expect more than 4 billion of them.
pub type AccountIndex = u32;

/// The address format for describing accounts.
pub type MultiAddress = sp_runtime::MultiAddress<AccountId, AccountIndex>;

/// Wraps [`generic::UncheckedExtrinsic`] to support checking unsigned
/// [`crate::Call::eth_transact`] extrinsic.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(E))]
pub struct UncheckedExtrinsic<Call, E: EthExtra>(
	pub generic::UncheckedExtrinsic<MultiAddress, Call, MultiSignature, E::Extra>,
);

impl<Call, E: EthExtra>
	From<generic::UncheckedExtrinsic<MultiAddress, Call, MultiSignature, E::Extra>>
	for UncheckedExtrinsic<Call, E>
{
	fn from(
		utx: generic::UncheckedExtrinsic<MultiAddress, Call, MultiSignature, E::Extra>,
	) -> Self {
		Self(utx)
	}
}

impl<Call, E: EthExtra> From<UncheckedExtrinsic<Call, E>>
	for generic::UncheckedExtrinsic<MultiAddress, Call, MultiSignature, E::Extra>
{
	fn from(extrinsic: UncheckedExtrinsic<Call, E>) -> Self {
		extrinsic.0
	}
}

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

type OnChargeTransactionBalanceOf<T> = <<T as pallet_transaction_payment::Config>::OnChargeTransaction as OnChargeTransaction<T>>::Balance;

impl<Call, E, Lookup> Checkable<Lookup> for UncheckedExtrinsic<Call, E>
where
	Call: Encode + Member,
	E: EthExtra,
	<E::Config as frame_system::Config>::Nonce: TryFrom<U256>,
	<E::Config as frame_system::Config>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
	OnChargeTransactionBalanceOf<E::Config>: Into<BalanceOf<E::Config>>,
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
				if let crate::Call::eth_transact { payload, gas_limit, storage_deposit_limit } =
					call
				{
					let checked = E::try_into_checked_extrinsic(
						payload,
						gas_limit,
						storage_deposit_limit,
						self.encoded_size(),
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
		lookup: &Lookup,
	) -> Result<Self::Checked, TransactionValidityError> {
		self.0.unchecked_into_checked_i_know_what_i_am_doing(lookup)
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
	type Config: crate::Config + pallet_transaction_payment::Config;

	/// The Runtime's signed extension.
	/// It should include at least:
	/// - [`frame_system::CheckNonce`] to ensure that the nonce from the Ethereum transaction is
	///   correct.
	type Extra: SignedExtension<AccountId = AccountId32>;

	/// Get the signed extensions to apply to an unsigned [`crate::Call::eth_transact`] extrinsic.
	///
	/// # Parameters
	/// - `nonce`: The nonce from the Ethereum transaction.
	fn get_eth_transact_extra(nonce: <Self::Config as frame_system::Config>::Nonce) -> Self::Extra;

	/// Convert the unsigned [`crate::Call::eth_transact`] into a [`CheckedExtrinsic`].
	/// and ensure that the fees from the Ethereum transaction correspond to the fees computed from
	/// the encoded_len, the injected gas_limit and storage_deposit_limit.
	///
	/// # Parameters
	/// - `payload`: The RLP-encoded Ethereum transaction.
	/// - `gas_limit`: The gas limit for the extrinsic
	/// - `storage_deposit_limit`: The storage deposit limit for the extrinsic,
	/// - `encoded_len`: The encoded length of the extrinsic.
	fn try_into_checked_extrinsic<Call>(
		payload: Vec<u8>,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<Self::Config>,
		encoded_len: usize,
	) -> Result<CheckedExtrinsic<AccountId32, Call, Self::Extra>, InvalidTransaction>
	where
		<Self::Config as frame_system::Config>::Nonce: TryFrom<U256>,
		BalanceOf<Self::Config>: Into<U256> + TryFrom<U256>,
		MomentOf<Self::Config>: Into<U256>,
		AccountIdOf<Self::Config>: Into<AccountId32>,
		<Self::Config as frame_system::Config>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
		OnChargeTransactionBalanceOf<Self::Config>: Into<BalanceOf<Self::Config>>,
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

		let call = if let Some(dest) = to {
			crate::Call::call::<Self::Config> {
				dest,
				value: value.try_into().map_err(|_| InvalidTransaction::Call)?,
				gas_limit,
				storage_deposit_limit,
				data: input.0,
			}
		} else {
			let blob = match polkavm::ProgramBlob::blob_length(&input.0) {
				Some(blob_len) => blob_len
					.try_into()
					.ok()
					.and_then(|blob_len| (input.0.split_at_checked(blob_len))),
				_ => None,
			};

			let Some((code, data)) = blob else {
				log::debug!(target: LOG_TARGET, "Failed to extract polkavm code & data");
				return Err(InvalidTransaction::Call);
			};

			crate::Call::instantiate_with_code::<Self::Config> {
				value: value.try_into().map_err(|_| InvalidTransaction::Call)?,
				gas_limit,
				storage_deposit_limit,
				code: code.to_vec(),
				data: data.to_vec(),
				salt: None,
			}
		};

		let nonce = nonce.try_into().map_err(|_| InvalidTransaction::Call)?;
		let eth_fee =
			gas_price.saturating_mul(gas).try_into().map_err(|_| InvalidTransaction::Call)?;
		let info = call.get_dispatch_info();
		let function: Call = call.into();

		let tip = Default::default();
		let actual_fee: BalanceOf<Self::Config> =
			pallet_transaction_payment::Pallet::<Self::Config>::compute_fee(
				encoded_len as u32,
				&info,
				tip,
			)
			.into();

		if actual_fee > eth_fee &&
			Percent::from_rational(actual_fee - eth_fee, eth_fee) > Percent::from_percent(5)
		{
			log::debug!(target: LOG_TARGET, "fees {actual_fee:?} should be no more than 5% higher
		 than fees calculated from the Ethereum transaction {eth_fee:?}");
			return Err(InvalidTransaction::Call.into())
		}

		log::debug!(target: LOG_TARGET, "Created checked Ethereum transaction with nonce {nonce:?}");
		Ok(CheckedExtrinsic {
			signed: Some((signer.into(), Self::get_eth_transact_extra(nonce))),
			function,
		})
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		evm::*,
		test_utils::*,
		tests::{ExtBuilder, RuntimeCall, Test},
	};
	use frame_support::{error::LookupError, traits::fungible::Mutate};
	use pallet_revive_fixtures::compile_module;
	use rlp::Encodable;
	use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
	use sp_core::keccak_256;
	use sp_runtime::traits::{Checkable, IdentityLookup};

	/// A simple account that can sign transactions
	pub struct Account(subxt_signer::eth::Keypair);

	impl Default for Account {
		fn default() -> Self {
			Self(subxt_signer::eth::dev::alith())
		}
	}

	impl From<subxt_signer::eth::Keypair> for Account {
		fn from(kp: subxt_signer::eth::Keypair) -> Self {
			Self(kp)
		}
	}

	impl Account {
		/// Get the [`AccountId`] of the account.
		pub fn account_id(&self) -> AccountId {
			let address = self.address();
			<Test as crate::Config>::AddressMapper::to_account_id_contract(&address)
		}

		/// Get the [`H160`] address of the account.
		pub fn address(&self) -> H160 {
			H160::from_slice(&self.0.account_id().as_ref())
		}

		/// Sign a transaction.
		pub fn sign_transaction(&self, tx: TransactionLegacyUnsigned) -> TransactionLegacySigned {
			let rlp_encoded = tx.rlp_bytes();
			let signature = self.0.sign(&rlp_encoded);
			TransactionLegacySigned::from(tx, signature.as_ref())
		}
	}

	#[derive(Clone, PartialEq, Eq, Debug)]
	pub struct Extra;

	impl EthExtra for Extra {
		type Config = Test;
		type Extra = frame_system::CheckNonce<Test>;

		fn get_eth_transact_extra(nonce: u32) -> Self::Extra {
			frame_system::CheckNonce::from(nonce)
		}
	}

	type Ex = UncheckedExtrinsic<RuntimeCall, Extra>;
	struct TestContext;

	impl traits::Lookup for TestContext {
		type Source = MultiAddress;
		type Target = AccountId;
		fn lookup(&self, s: Self::Source) -> Result<Self::Target, LookupError> {
			match s {
				MultiAddress::Id(id) => Ok(id),
				_ => Err(LookupError),
			}
		}
	}

	/// A builder for creating an unchecked extrinsic, and test that the check function works.
	#[derive(Clone)]
	struct UncheckedExtrinsicBuilder {
		tx: TransactionLegacyUnsigned,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<Test>,
	}

	impl UncheckedExtrinsicBuilder {
		/// Create a new builder with default values.
		fn new() -> Self {
			Self {
				tx: TransactionLegacyUnsigned {
					chain_id: Some(<Test as crate::Config>::ChainId::get().into()),
					gas: U256::from(21000),
					nonce: U256::from(0),
					gas_price: U256::from(100_000),
					to: None,
					value: U256::from(0),
					input: Bytes(vec![]),
					r#type: Type0,
				},
				gas_limit: Weight::zero(),
				storage_deposit_limit: 0,
			}
		}

		/// Create a new builder with a call to the given address.
		fn call_with(dest: H160) -> Self {
			let mut builder = Self::new();
			builder.tx.to = Some(dest);
			builder
		}

		/// Create a new builder with an instantiate call.
		fn instantiate_with(code: Vec<u8>, data: Vec<u8>) -> Self {
			let mut builder = Self::new();
			builder.tx.input = Bytes(code.into_iter().chain(data.into_iter()).collect());
			builder
		}

		/// Update the transaction with the given function.
		fn update(mut self, f: impl FnOnce(&mut TransactionLegacyUnsigned) -> ()) -> Self {
			f(&mut self.tx);
			self
		}

		/// Call `check` on the unchecked extrinsic, and `pre_dispatch` on the signed extension.
		fn check(&self) -> Result<RuntimeCall, TransactionValidityError> {
			let UncheckedExtrinsicBuilder { tx, gas_limit, storage_deposit_limit } = self.clone();

			// Fund the account.
			let account = Account::default();
			let _ = <Test as Config>::Currency::set_balance(&account.account_id(), 100_000_000);

			let payload = account.sign_transaction(tx).rlp_bytes().to_vec();
			let call = RuntimeCall::Contracts(crate::Call::eth_transact {
				payload,
				gas_limit,
				storage_deposit_limit,
			});

			let encoded_len = call.encode().len();
			let result = Ex::new(call, None).unwrap().check(&TestContext {})?;
			let (account_id, extra) = result.signed.unwrap();

			extra.pre_dispatch(
				&account_id,
				&result.function,
				&result.function.get_dispatch_info(),
				encoded_len,
			)?;

			Ok(result.function)
		}
	}

	#[test]
	fn check_eth_transact_call_works() {
		ExtBuilder::default().build().execute_with(|| {
			let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]));
			assert_eq!(
				builder.check().unwrap(),
				crate::Call::call::<Test> {
					dest: builder.tx.to.unwrap(),
					value: builder.tx.value.as_u64(),
					gas_limit: builder.gas_limit,
					storage_deposit_limit: builder.storage_deposit_limit,
					data: builder.tx.input.0
				}
				.into()
			);
		});
	}

	#[test]
	fn check_eth_transact_instantiate_works() {
		ExtBuilder::default().build().execute_with(|| {
			let (code, _) = compile_module("dummy").unwrap();
			let data = vec![];
			let builder = UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone());

			assert_eq!(
				builder.check().unwrap(),
				crate::Call::instantiate_with_code::<Test> {
					value: builder.tx.value.as_u64(),
					gas_limit: builder.gas_limit,
					storage_deposit_limit: builder.storage_deposit_limit,
					code,
					data,
					salt: None
				}
				.into()
			);
		});
	}

	#[test]
	fn check_eth_transact_nonce_works() {
		ExtBuilder::default().build().execute_with(|| {
			let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]))
				.update(|tx| tx.nonce = 1u32.into());

			assert_eq!(
				builder.check(),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Future))
			);

			<crate::System<Test>>::inc_account_nonce(Account::default().account_id());

			let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]));
			assert_eq!(
				builder.check(),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Stale))
			);
		});
	}

	#[test]
	fn check_eth_transact_chain_id_works() {
		ExtBuilder::default().build().execute_with(|| {
			let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]))
				.update(|tx| tx.chain_id = Some(42.into()));

			assert_eq!(
				builder.check(),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
			);
		});
	}

	#[test]
	fn check_instantiate_data() {
		ExtBuilder::default().build().execute_with(|| {
			let code = b"invalid code".to_vec();
			let data = vec![1];
			let builder = UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone());

			// Fail because the tx input fail to get the blob length
			assert_eq!(
				builder.clone().update(|tx| tx.input = Bytes(vec![1, 2, 3])).check(),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
			);
		});
	}

	#[test]
	fn check_injected_weight() {
		ExtBuilder::default().build().execute_with(|| {
			// Lower the gas_price to make the tx fees lower than the actual fees
			let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]))
				.update(|tx| tx.gas_price = U256::from(1));

			assert_eq!(
				builder.check(),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
			);
		});
	}
}
