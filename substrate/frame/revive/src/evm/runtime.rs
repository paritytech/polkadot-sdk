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
use crate::{
	evm::{
		api::{GenericTransaction, TransactionSigned},
		create_call,
		fees::InfoT,
	},
	AccountIdOf, AddressMapper, BalanceOf, CallOf, Config, Pallet, Zero, LOG_TARGET,
};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::{
		fungible::Balanced,
		tokens::{Fortitude, Precision, Preservation},
		InherentBuilder, IsSubType, SignedTransactionBuilder,
	},
};
use pallet_transaction_payment::Config as TxConfig;
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_core::U256;
use sp_runtime::{
	generic::{self, CheckedExtrinsic, ExtrinsicFormat},
	traits::{Checkable, ExtrinsicCall, ExtrinsicLike, ExtrinsicMetadata, TransactionExtension},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	OpaqueExtrinsic, RuntimeDebug, Weight,
};

/// Used to set the weight limit argument of a `eth_call` or `eth_instantiate_with_code` call.
pub trait SetWeightLimit {
	/// Set the weight limit of this call.
	///
	/// Returns the replaced weight.
	fn set_weight_limit(&mut self, weight_limit: Weight) -> Weight;
}

/// Wraps [`generic::UncheckedExtrinsic`] to support checking unsigned
/// [`crate::Call::eth_transact`] extrinsic.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct UncheckedExtrinsic<Address, Signature, E: EthExtra>(
	pub generic::UncheckedExtrinsic<Address, CallOf<E::Config>, Signature, E::Extension>,
);

impl<Address, Signature, E: EthExtra> TypeInfo for UncheckedExtrinsic<Address, Signature, E>
where
	Address: StaticTypeInfo,
	Signature: StaticTypeInfo,
	E::Extension: StaticTypeInfo,
{
	type Identity =
		generic::UncheckedExtrinsic<Address, CallOf<E::Config>, Signature, E::Extension>;
	fn type_info() -> scale_info::Type {
		generic::UncheckedExtrinsic::<Address, CallOf<E::Config>, Signature, E::Extension>::type_info()
	}
}

impl<Address, Signature, E: EthExtra>
	From<generic::UncheckedExtrinsic<Address, CallOf<E::Config>, Signature, E::Extension>>
	for UncheckedExtrinsic<Address, Signature, E>
{
	fn from(
		utx: generic::UncheckedExtrinsic<Address, CallOf<E::Config>, Signature, E::Extension>,
	) -> Self {
		Self(utx)
	}
}

impl<Address: TypeInfo, Signature: TypeInfo, E: EthExtra> ExtrinsicLike
	for UncheckedExtrinsic<Address, Signature, E>
{
	fn is_bare(&self) -> bool {
		ExtrinsicLike::is_bare(&self.0)
	}
}

impl<Address, Signature, E: EthExtra> ExtrinsicMetadata
	for UncheckedExtrinsic<Address, Signature, E>
{
	const VERSIONS: &'static [u8] = generic::UncheckedExtrinsic::<
		Address,
		CallOf<E::Config>,
		Signature,
		E::Extension,
	>::VERSIONS;
	type TransactionExtensions = E::Extension;
}

impl<Address: TypeInfo, Signature: TypeInfo, E: EthExtra> ExtrinsicCall
	for UncheckedExtrinsic<Address, Signature, E>
{
	type Call = CallOf<E::Config>;

	fn call(&self) -> &Self::Call {
		self.0.call()
	}
}

impl<LookupSource, Signature, E, Lookup> Checkable<Lookup>
	for UncheckedExtrinsic<LookupSource, Signature, E>
where
	E: EthExtra,
	Self: Encode,
	<E::Config as frame_system::Config>::Nonce: TryFrom<U256>,
	CallOf<E::Config>: SetWeightLimit,
	// required by Checkable for `generic::UncheckedExtrinsic`
	generic::UncheckedExtrinsic<LookupSource, CallOf<E::Config>, Signature, E::Extension>:
		Checkable<
			Lookup,
			Checked = CheckedExtrinsic<AccountIdOf<E::Config>, CallOf<E::Config>, E::Extension>,
		>,
{
	type Checked = CheckedExtrinsic<AccountIdOf<E::Config>, CallOf<E::Config>, E::Extension>;

	fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
		if !self.0.is_signed() {
			if let Some(crate::Call::eth_transact { payload }) = self.0.function.is_sub_type() {
				let checked = E::try_into_checked_extrinsic(payload, self.encoded_size())?;
				return Ok(checked)
			};
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

impl<Address, Signature, E: EthExtra> GetDispatchInfo
	for UncheckedExtrinsic<Address, Signature, E>
{
	fn get_dispatch_info(&self) -> DispatchInfo {
		self.0.get_dispatch_info()
	}
}

impl<Address: Encode, Signature: Encode, E: EthExtra> serde::Serialize
	for UncheckedExtrinsic<Address, Signature, E>
{
	fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error>
	where
		S: ::serde::Serializer,
	{
		self.0.serialize(seq)
	}
}

impl<'a, Address: Decode, Signature: Decode, E: EthExtra> serde::Deserialize<'a>
	for UncheckedExtrinsic<Address, Signature, E>
{
	fn deserialize<D>(de: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'a>,
	{
		let r = sp_core::bytes::deserialize(de)?;
		Decode::decode(&mut &r[..])
			.map_err(|e| serde::de::Error::custom(alloc::format!("Decode error: {}", e)))
	}
}

impl<Address, Signature, E: EthExtra> SignedTransactionBuilder
	for UncheckedExtrinsic<Address, Signature, E>
where
	Address: TypeInfo,
	Signature: TypeInfo,
	E::Extension: TypeInfo,
{
	type Address = Address;
	type Signature = Signature;
	type Extension = E::Extension;

	fn new_signed_transaction(
		call: Self::Call,
		signed: Address,
		signature: Signature,
		tx_ext: E::Extension,
	) -> Self {
		generic::UncheckedExtrinsic::new_signed(call, signed, signature, tx_ext).into()
	}
}

impl<Address, Signature, E: EthExtra> InherentBuilder for UncheckedExtrinsic<Address, Signature, E>
where
	Address: TypeInfo,
	Signature: TypeInfo,
	E::Extension: TypeInfo,
{
	fn new_inherent(call: Self::Call) -> Self {
		generic::UncheckedExtrinsic::new_bare(call).into()
	}
}

impl<Address, Signature, E: EthExtra> From<UncheckedExtrinsic<Address, Signature, E>>
	for OpaqueExtrinsic
where
	Address: Encode,
	Signature: Encode,
	E::Extension: Encode,
{
	fn from(extrinsic: UncheckedExtrinsic<Address, Signature, E>) -> Self {
		Self::from_bytes(extrinsic.encode().as_slice()).expect(
			"both OpaqueExtrinsic and UncheckedExtrinsic have encoding that is compatible with \
				raw Vec<u8> encoding; qed",
		)
	}
}

/// EthExtra convert an unsigned [`crate::Call::eth_transact`] into a [`CheckedExtrinsic`].
pub trait EthExtra {
	/// The Runtime configuration.
	type Config: Config + TxConfig;

	/// The Runtime's transaction extension.
	/// It should include at least:
	/// - [`frame_system::CheckNonce`] to ensure that the nonce from the Ethereum transaction is
	///   correct.
	type Extension: TransactionExtension<CallOf<Self::Config>>;

	/// Get the transaction extension to apply to an unsigned [`crate::Call::eth_transact`]
	/// extrinsic.
	///
	/// # Parameters
	/// - `nonce`: The nonce extracted from the Ethereum transaction.
	/// - `tip`: The transaction tip calculated from the Ethereum transaction.
	fn get_eth_extension(
		nonce: <Self::Config as frame_system::Config>::Nonce,
		tip: BalanceOf<Self::Config>,
	) -> Self::Extension;

	/// Convert the unsigned [`crate::Call::eth_transact`] into a [`CheckedExtrinsic`].
	/// and ensure that the fees from the Ethereum transaction correspond to the fees computed from
	/// the encoded_len, the injected gas_limit and storage_deposit_limit.
	///
	/// # Parameters
	/// - `payload`: The RLP-encoded Ethereum transaction.
	/// - `encoded_len`: The encoded length of the extrinsic.
	fn try_into_checked_extrinsic(
		payload: &[u8],
		encoded_len: usize,
	) -> Result<
		CheckedExtrinsic<AccountIdOf<Self::Config>, CallOf<Self::Config>, Self::Extension>,
		InvalidTransaction,
	>
	where
		<Self::Config as frame_system::Config>::Nonce: TryFrom<U256>,
		CallOf<Self::Config>: SetWeightLimit,
	{
		let tx = TransactionSigned::decode(&payload).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to decode transaction: {err:?}");
			InvalidTransaction::Call
		})?;

		// Check transaction type and reject unsupported transaction types
		match &tx {
			crate::evm::api::TransactionSigned::Transaction1559Signed(_) |
			crate::evm::api::TransactionSigned::Transaction2930Signed(_) |
			crate::evm::api::TransactionSigned::TransactionLegacySigned(_) => {
				// Supported transaction types, continue processing
			},
			crate::evm::api::TransactionSigned::Transaction7702Signed(_) => {
				log::debug!(target: LOG_TARGET, "EIP-7702 transactions are not supported");
				return Err(InvalidTransaction::Call);
			},
			crate::evm::api::TransactionSigned::Transaction4844Signed(_) => {
				log::debug!(target: LOG_TARGET, "EIP-4844 transactions are not supported");
				return Err(InvalidTransaction::Call);
			},
		}

		let signer_addr = tx.recover_eth_address().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to recover signer: {err:?}");
			InvalidTransaction::BadProof
		})?;

		let signer = <Self::Config as Config>::AddressMapper::to_fallback_account_id(&signer_addr);
		let base_fee = <Pallet<Self::Config>>::evm_base_fee();
		let tx = GenericTransaction::from_signed(tx, base_fee, None);
		let nonce = tx.nonce.unwrap_or_default().try_into().map_err(|_| {
			log::debug!(target: LOG_TARGET, "Failed to convert nonce");
			InvalidTransaction::Call
		})?;

		log::debug!(target: LOG_TARGET, "Decoded Ethereum transaction with signer: {signer_addr:?} nonce: {nonce:?}");
		let call_info =
			create_call::<Self::Config>(tx, Some((encoded_len as u32, payload.to_vec())))?;
		let storage_credit = <Self::Config as Config>::Currency::withdraw(
					&signer,
					call_info.storage_deposit,
					Precision::Exact,
					Preservation::Preserve,
					Fortitude::Polite,
		).map_err(|_| {
			log::debug!(target: LOG_TARGET, "Not enough balance to hold additional storage deposit of {:?}", call_info.storage_deposit);
			InvalidTransaction::Payment
		})?;
		<Self::Config as Config>::FeeInfo::deposit_txfee(storage_credit);

		crate::tracing::if_tracing(|tracer| {
			tracer.watch_address(&Pallet::<Self::Config>::block_author());
			tracer.watch_address(&signer_addr);
		});

		log::debug!(target: LOG_TARGET, "\
			Created checked Ethereum transaction with: \
			weight_limit={} \
			additional_storage_deposit_held={:?} \
			nonce={nonce:?}
			",
			call_info.weight_limit,
			call_info.storage_deposit,
		);

		// We can't calculate a tip because it needs to be based on the actual gas used which we
		// cannot know pre-dispatch. Hence we never supply a tip here or it would be way too high.
		Ok(CheckedExtrinsic {
			format: ExtrinsicFormat::Signed(
				signer.into(),
				Self::get_eth_extension(nonce, Zero::zero()),
			),
			function: call_info.call,
		})
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		evm::*,
		test_utils::*,
		tests::{
			Address, ExtBuilder, RuntimeCall, RuntimeOrigin, SignedExtra, Test, UncheckedExtrinsic,
		},
		EthTransactInfo, Weight, RUNTIME_PALLETS_ADDR,
	};
	use frame_support::{error::LookupError, traits::fungible::Mutate};
	use pallet_revive_fixtures::compile_module;
	use sp_runtime::traits::{self, Checkable, DispatchTransaction, Get};

	type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

	struct TestContext;

	impl traits::Lookup for TestContext {
		type Source = Address;
		type Target = AccountIdOf<Test>;
		fn lookup(&self, s: Self::Source) -> Result<Self::Target, LookupError> {
			match s {
				Self::Source::Id(id) => Ok(id),
				_ => Err(LookupError),
			}
		}
	}

	/// A builder for creating an unchecked extrinsic, and test that the check function works.
	#[derive(Clone)]
	struct UncheckedExtrinsicBuilder {
		tx: GenericTransaction,
		before_validate: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
		dry_run: Option<EthTransactInfo<BalanceOf<Test>>>,
	}

	impl UncheckedExtrinsicBuilder {
		/// Create a new builder with default values.
		fn new() -> Self {
			Self {
				tx: GenericTransaction {
					from: Some(Account::default().address()),
					chain_id: Some(<Test as Config>::ChainId::get().into()),
					..Default::default()
				},
				before_validate: None,
				dry_run: None,
			}
		}

		fn data(mut self, data: Vec<u8>) -> Self {
			self.tx.input = Bytes(data).into();
			self
		}

		fn fund_account(account: &Account) {
			let _ = <Test as Config>::Currency::set_balance(
				&account.substrate_account(),
				100_000_000_000_000,
			);
		}

		fn estimate_gas(&mut self) {
			let account = Account::default();
			Self::fund_account(&account);

			let dry_run = crate::Pallet::<Test>::dry_run_eth_transact(self.tx.clone());
			self.tx.gas_price = Some(<Pallet<Test>>::evm_base_fee());

			match dry_run {
				Ok(dry_run) => {
					self.tx.gas = Some(dry_run.eth_gas);
					self.dry_run = Some(dry_run);
				},
				Err(err) => {
					log::debug!(target: LOG_TARGET, "Failed to estimate gas: {:?}", err);
				},
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
			builder.tx.input = Bytes(code.into_iter().chain(data.into_iter()).collect()).into();
			builder
		}

		/// Set before_validate function.
		fn before_validate(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
			self.before_validate = Some(std::sync::Arc::new(f));
			self
		}

		fn check(
			self,
		) -> Result<
			(u32, RuntimeCall, SignedExtra, GenericTransaction, Weight, TransactionSigned),
			TransactionValidityError,
		> {
			self.mutate_estimate_and_check(Box::new(|_| ()))
		}

		/// Call `check` on the unchecked extrinsic, and `pre_dispatch` on the signed extension.
		fn mutate_estimate_and_check(
			mut self,
			f: Box<dyn FnOnce(&mut GenericTransaction) -> ()>,
		) -> Result<
			(u32, RuntimeCall, SignedExtra, GenericTransaction, Weight, TransactionSigned),
			TransactionValidityError,
		> {
			ExtBuilder::default().build().execute_with(|| self.estimate_gas());
			ExtBuilder::default().build().execute_with(|| {
				f(&mut self.tx);
				let UncheckedExtrinsicBuilder { tx, before_validate, .. } = self.clone();

				// Fund the account.
				let account = Account::default();
				Self::fund_account(&account);

				let signed_transaction =
					account.sign_transaction(tx.clone().try_into_unsigned().unwrap());
				let call = RuntimeCall::Contracts(crate::Call::eth_transact {
					payload: signed_transaction.signed_payload().clone(),
				});

				let uxt: UncheckedExtrinsic = generic::UncheckedExtrinsic::new_bare(call).into();
				let encoded_len = uxt.encoded_size();
				let result: CheckedExtrinsic<_, _, _> = uxt.check(&TestContext {})?;
				let (account_id, extra): (AccountId32, SignedExtra) = match result.format {
					ExtrinsicFormat::Signed(signer, extra) => (signer, extra),
					_ => unreachable!(),
				};

				before_validate.map(|f| f());
				extra.clone().validate_and_prepare(
					RuntimeOrigin::signed(account_id),
					&result.function,
					&result.function.get_dispatch_info(),
					encoded_len,
					0,
				)?;

				Ok((
					encoded_len as u32,
					result.function,
					extra,
					tx,
					self.dry_run.unwrap().gas_required,
					signed_transaction,
				))
			})
		}
	}

	#[test]
	fn check_eth_transact_call_works() {
		let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]));
		let (expected_encoded_len, call, _, tx, gas_required, signed_transaction) =
			builder.check().unwrap();
		let expected_effective_gas_price: u32 = <Test as Config>::NativeToEthRatio::get();

		match call {
			RuntimeCall::Contracts(crate::Call::eth_call::<Test> {
				dest,
				value,
				data,
				gas_limit,
				transaction_encoded,
				effective_gas_price,
				encoded_len,
			}) if dest == tx.to.unwrap() &&
				value == tx.value.unwrap_or_default().as_u64().into() &&
				data == tx.input.to_vec() &&
				transaction_encoded == signed_transaction.signed_payload() &&
				effective_gas_price == expected_effective_gas_price.into() =>
			{
				assert_eq!(encoded_len, expected_encoded_len);
				assert!(
					gas_limit.all_gte(gas_required),
					"Assert failed: gas_limit={gas_limit:?} >= gas_required={gas_required:?}"
				);
			},
			_ => panic!("Call does not match."),
		}
	}

	#[test]
	fn check_eth_transact_instantiate_works() {
		let (expected_code, _) = compile_module("dummy").unwrap();
		let expected_data = vec![];
		let builder = UncheckedExtrinsicBuilder::instantiate_with(
			expected_code.clone(),
			expected_data.clone(),
		);
		let (expected_encoded_len, call, _, tx, gas_required, signed_transaction) =
			builder.check().unwrap();
		let expected_effective_gas_price: u32 = <Test as Config>::NativeToEthRatio::get();
		let expected_value = tx.value.unwrap_or_default().as_u64().into();

		match call {
			RuntimeCall::Contracts(crate::Call::eth_instantiate_with_code::<Test> {
				value,
				code,
				data,
				gas_limit,
				transaction_encoded,
				effective_gas_price,
				encoded_len,
			}) if value == expected_value &&
				code == expected_code &&
				data == expected_data &&
				transaction_encoded == signed_transaction.signed_payload() &&
				effective_gas_price == expected_effective_gas_price.into() =>
			{
				assert_eq!(encoded_len, expected_encoded_len);
				assert!(
					gas_limit.all_gte(gas_required),
					"Assert failed: gas_limit={gas_limit:?} >= gas_required={gas_required:?}"
				);
			},
			_ => panic!("Call does not match."),
		}
	}

	#[test]
	fn check_eth_transact_nonce_works() {
		let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]));

		assert_eq!(
			builder.mutate_estimate_and_check(Box::new(|tx| tx.nonce = Some(1u32.into()))),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Future))
		);

		let builder =
			UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20])).before_validate(|| {
				<crate::System<Test>>::inc_account_nonce(Account::default().substrate_account());
			});

		assert_eq!(
			builder.check(),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale))
		);
	}

	#[test]
	fn check_eth_transact_chain_id_works() {
		let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]));

		assert_eq!(
			builder.mutate_estimate_and_check(Box::new(|tx| tx.chain_id = Some(42.into()))),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
		);
	}

	#[test]
	fn check_instantiate_data() {
		let code: Vec<u8> = polkavm_common::program::BLOB_MAGIC
			.into_iter()
			.chain(b"invalid code".iter().cloned())
			.collect();
		let data = vec![1];

		let builder = UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone());

		// Fail because the tx input fail to get the blob length
		assert_eq!(
			builder.check(),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
		);
	}

	#[test]
	fn check_transaction_fees() {
		let scenarios: Vec<(_, Box<dyn FnOnce(&mut GenericTransaction)>, _)> = vec![
			(
				"Eth fees too low",
				Box::new(|tx| {
					tx.gas_price = Some(100u64.into());
				}),
				InvalidTransaction::Payment,
			),
			(
				"Gas fees too low",
				Box::new(|tx| {
					tx.gas = Some(tx.gas.unwrap() / 2);
				}),
				InvalidTransaction::Payment,
			),
		];

		for (msg, update_tx, err) in scenarios {
			let res = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]))
				.mutate_estimate_and_check(update_tx);

			assert_eq!(res, Err(TransactionValidityError::Invalid(err)), "{}", msg);
		}
	}

	#[test]
	fn check_transaction_tip() {
		let (code, _) = compile_module("dummy").unwrap();
		// create some dummy data to increase the gas fee
		let data = vec![42u8; crate::limits::CALLDATA_BYTES as usize];
		let (_, _, extra, _tx, _gas_required, _) =
			UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone())
				.mutate_estimate_and_check(Box::new(|tx| {
					tx.gas_price = Some(tx.gas_price.unwrap() * 103 / 100);
					log::debug!(target: LOG_TARGET, "Gas price: {:?}", tx.gas_price);
				}))
				.unwrap();

		assert_eq!(U256::from(extra.1.tip()), 0u32.into());
	}

	#[test]
	fn check_runtime_pallets_addr_works() {
		let remark: CallOf<Test> =
			frame_system::Call::remark { remark: b"Hello, world!".to_vec() }.into();

		let builder =
			UncheckedExtrinsicBuilder::call_with(RUNTIME_PALLETS_ADDR).data(remark.encode());
		let (_, call, _, _, _, _) = builder.check().unwrap();

		assert_eq!(call, remark);
	}
}
