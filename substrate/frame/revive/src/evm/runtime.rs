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

#[cfg(not(feature = "std"))]
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
	generic::UncheckedExtrinsic<MultiAddress, Call, MultiSignature, E::Extra>,
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

impl<Call, E: EthExtra>
	Into<generic::UncheckedExtrinsic<MultiAddress, Call, MultiSignature, E::Extra>>
	for UncheckedExtrinsic<Call, E>
{
	fn into(self) -> generic::UncheckedExtrinsic<MultiAddress, Call, MultiSignature, E::Extra> {
		self.0
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

impl<Call, E, Lookup> Checkable<Lookup> for UncheckedExtrinsic<Call, E>
where
	Call: Encode + Member,
	E: EthExtra,
	<E::Config as frame_system::Config>::Nonce: TryFrom<U256>,
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

/// A [`SignedExtension`] that performs pre-dispatch checks on the Ethereum transaction's fees.
#[derive(DebugNoBound, DefaultNoBound, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckEthTransact<T: Config> {
	/// The gas fee, computed from the signed Ethereum transaction.
	///
	/// # Note
	///
	/// This is marked as `#[codec(skip)]` as this extracted from the Ethereum transaction and not
	/// encoded as additional signed data.
	#[codec(skip)]
	eth_fee: Option<BalanceOf<T>>,
	_phantom: core::marker::PhantomData<T>,
}

impl<T: Config> CheckEthTransact<T> {
	/// Create a new `CheckEthTransact` with the given eth_fee.
	pub fn from(eth_fee: BalanceOf<T>) -> Self {
		Self { eth_fee: Some(eth_fee), _phantom: Default::default() }
	}
}

impl<T: Send + Sync> SignedExtension for CheckEthTransact<T>
where
	T: Config + pallet_transaction_payment::Config,
	<T as frame_system::Config>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	<<T as pallet_transaction_payment::Config>::OnChargeTransaction as OnChargeTransaction<T>>::Balance: Into<BalanceOf<T>>,
{
	const IDENTIFIER: &'static str = "CheckEthTransact";
	type AccountId = T::AccountId;
	type Call = <T as frame_system::Config>::RuntimeCall;
	type AdditionalSigned = ();
	type Pre = ();

	fn additional_signed(
		&self,
	) -> Result<Self::AdditionalSigned, sp_runtime::transaction_validity::TransactionValidityError>
	{
		Ok(())
	}

	fn pre_dispatch(
		self,
		_who: &Self::AccountId,
		call: &Self::Call,
		info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, sp_runtime::transaction_validity::TransactionValidityError> {
		let Some(eth_fee) = self.eth_fee else {
			return Ok(())
		};

		let tip = Default::default();

		let actual_fee: BalanceOf<T> =
			pallet_transaction_payment::Pallet::<T>::compute_fee(len as u32, info, tip).into();

		// Make sure that that the fee are not more than 5% higher than the eth_fee
		if actual_fee > eth_fee &&
			Percent::from_rational(actual_fee - eth_fee, eth_fee) > Percent::from_percent(5)
		{
			log::debug!(target: LOG_TARGET, "fees {actual_fee:?} should be no more than 5% higher than fees calculated from the Ethereum transaction {eth_fee:?}");
			return Err(InvalidTransaction::Call.into())
		}

		Ok(())
	}
}

/// EthExtra convert an unsigned [`crate::Call::eth_transact`] into a [`CheckedExtrinsic`].
pub trait EthExtra {
	/// The Runtime configuration.
	type Config: crate::Config;

	/// The Runtime's signed extension.
	/// It should include at least:
	/// [`CheckNonce`] to ensure that the nonce from the Ethereum transaction is correct.
	/// [`CheckEthTransact`] to ensure that the fees from the Ethereum transaction correspond to
	/// the pre-dispatch fees computed from the extrinsic.
	type Extra: SignedExtension<AccountId = AccountId32>;

	/// Get the signed extensions to apply to an unsigned [`crate::Call::eth_transact`] extrinsic.
	///
	/// # Parameters
	/// - `nonce`: The nonce from the Ethereum transaction.
	/// - `gas_fee`: The gas fee from the Ethereum transaction.
	fn get_eth_transact_extra(
		nonce: <Self::Config as frame_system::Config>::Nonce,
		gas_fee: BalanceOf<Self::Config>,
	) -> Self::Extra;

	/// Convert the unsigned [`crate::Call::eth_transact`] into a [`CheckedExtrinsic`].
	fn try_into_checked_extrinsic<Call>(
		payload: Vec<u8>,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<Self::Config>,
		transact_kind: EthTransactKind,
	) -> Result<CheckedExtrinsic<AccountId32, Call, Self::Extra>, InvalidTransaction>
	where
		<Self::Config as frame_system::Config>::Nonce: TryFrom<U256>,
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

		let nonce = nonce.try_into().map_err(|_| InvalidTransaction::Call)?;
		let eth_fee =
			gas_price.saturating_mul(gas).try_into().map_err(|_| InvalidTransaction::Call)?;

		log::debug!(target: LOG_TARGET, "Created checked Ethereum transaction with nonce {nonce:?}");
		Ok(CheckedExtrinsic {
			signed: Some((signer.into(), Self::get_eth_transact_extra(nonce, eth_fee))),
			function: call.into(),
		})
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		evm::*,
		test_utils::*,
		tests::{builder, ExtBuilder, RuntimeCall, Test},
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
		type Extra = (frame_system::CheckNonce<Test>, CheckEthTransact<Test>);

		fn get_eth_transact_extra(nonce: u32, eth_fee: u64) -> Self::Extra {
			(frame_system::CheckNonce::from(nonce), CheckEthTransact::from(eth_fee))
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
		transact_kind: EthTransactKind,
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
				transact_kind: EthTransactKind::Call,
			}
		}

		/// Create a new builder with a call to the given address.
		fn call_with(dest: H160) -> Self {
			let mut builder = Self::new();
			builder.tx.to = Some(dest);
			builder.transact_kind = EthTransactKind::Call;
			builder
		}

		/// Create a new builder with an instantiate call.
		fn instantiate_with(code: Vec<u8>, data: Vec<u8>) -> Self {
			let mut builder = Self::new();
			builder.transact_kind = EthTransactKind::InstantiateWithCode {
				code_len: code.len() as u32,
				data_len: data.len() as u32,
			};
			builder.tx.input = Bytes(EthInstantiateInput { code, data }.encode());
			builder
		}

		/// Update the transaction with the given function.
		fn update(mut self, f: impl FnOnce(&mut TransactionLegacyUnsigned) -> ()) -> Self {
			f(&mut self.tx);
			self
		}

		/// Set the transact kind
		fn transact_kind(mut self, kind: EthTransactKind) -> Self {
			self.transact_kind = kind;
			self
		}

		/// Call `check` on the unchecked extrinsic, and `pre_dispatch` on the signed extension.
		fn check(&self) -> Result<RuntimeCall, TransactionValidityError> {
			let UncheckedExtrinsicBuilder { tx, gas_limit, storage_deposit_limit, transact_kind } =
				self.clone();

			// Fund the account.
			let account = Account::default();
			let _ = <Test as Config>::Currency::set_balance(&account.account_id(), 100_000_000);

			let payload = account.sign_transaction(tx).rlp_bytes().to_vec();
			let call = RuntimeCall::Contracts(crate::Call::eth_transact {
				payload,
				gas_limit,
				storage_deposit_limit,
				transact_kind,
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
			let code = vec![];
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
			let code = vec![1, 2, 3];
			let data = vec![1];
			let builder = UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone());

			// Fail because the tx input should decode as an `EthInstantiateInput`
			assert_eq!(
				builder.clone().update(|tx| tx.input = Bytes(vec![1, 2, 3])).check(),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
			);

			let builder = UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone())
				.transact_kind(EthTransactKind::InstantiateWithCode {
					code_len: 0,
					data_len: data.len() as u32,
				});

			// Fail because we are passing the wrong code length
			assert_eq!(
				builder
					.clone()
					.transact_kind(EthTransactKind::InstantiateWithCode {
						code_len: 0,
						data_len: data.len() as u32
					})
					.check(),
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
			);

			// Fail because we are passing the wrong data length
			assert_eq!(
				builder
					.clone()
					.transact_kind(EthTransactKind::InstantiateWithCode {
						code_len: code.len() as u32,
						data_len: 0
					})
					.check(),
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
