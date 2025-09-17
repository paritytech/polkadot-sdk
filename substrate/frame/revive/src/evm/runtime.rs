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
		fees::InfoT,
	},
	vm::pvm::extract_code_and_data,
	AccountIdOf, AddressMapper, BalanceOf, Config, DispatchClass, MomentOf, Pallet, Zero,
	LOG_TARGET, RUNTIME_PALLETS_ADDR,
};
use alloc::vec::Vec;
use codec::{Decode, DecodeLimit, DecodeWithMemTracking, Encode};
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::{fungible::MutateHold, InherentBuilder, IsSubType, SignedTransactionBuilder},
	MAX_EXTRINSIC_DEPTH,
};
use num_traits::Bounded;
use pallet_transaction_payment::Config as TxConfig;
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_core::{Get, H256, U256};
use sp_runtime::{
	generic::{self, CheckedExtrinsic, ExtrinsicFormat},
	traits::{
		Checkable, Dispatchable, ExtrinsicCall, ExtrinsicLike, ExtrinsicMetadata,
		TransactionExtension,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	FixedPointNumber, FixedU128, OpaqueExtrinsic, RuntimeDebug, SaturatedConversion, Weight,
};

type CallOf<T> = <T as frame_system::Config>::RuntimeCall;

/// Used to set the weight limit argument of a `eth_call` or `eth_instantiate_with_code` call.
pub trait SetWeightLimit {
	/// Set the weight limit of this call.
	fn set_weight_limit(&mut self, weight_limit: Weight);
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
	BalanceOf<E::Config>: Into<U256> + TryFrom<U256>,
	MomentOf<E::Config>: Into<U256>,
	CallOf<E::Config>:
		From<crate::Call<E::Config>> + IsSubType<crate::Call<E::Config>> + SetWeightLimit,
	<E::Config as frame_system::Config>::Hash: frame_support::traits::IsType<H256>,
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
				let checked = E::try_into_checked_extrinsic(payload.to_vec(), self.encoded_size())?;
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

impl<Address, Signature, E: EthExtra> GetDispatchInfo for UncheckedExtrinsic<Address, Signature, E>
where
	CallOf<E::Config>: GetDispatchInfo + Dispatchable,
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
	CallOf<E::Config>: TypeInfo,
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
	CallOf<E::Config>: TypeInfo,
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
	CallOf<E::Config>: Encode,
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
	/// - `gas_limit`: The gas limit for the extrinsic
	/// - `storage_deposit_limit`: The storage deposit limit for the extrinsic,
	/// - `encoded_len`: The encoded length of the extrinsic.
	fn try_into_checked_extrinsic(
		payload: Vec<u8>,
		encoded_len: usize,
	) -> Result<
		CheckedExtrinsic<AccountIdOf<Self::Config>, CallOf<Self::Config>, Self::Extension>,
		InvalidTransaction,
	>
	where
		<Self::Config as frame_system::Config>::Nonce: TryFrom<U256>,
		BalanceOf<Self::Config>: Into<U256> + TryFrom<U256>,
		MomentOf<Self::Config>: Into<U256>,
		CallOf<Self::Config>: From<crate::Call<Self::Config>> + SetWeightLimit,
		<Self::Config as frame_system::Config>::Hash: frame_support::traits::IsType<H256>,
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

		let base_fee = <Pallet<Self::Config>>::evm_gas_price();

		let signer = <Self::Config as Config>::AddressMapper::to_fallback_account_id(&signer_addr);
		let tx = GenericTransaction::from_signed(tx, None);

		let Some(gas) = tx.gas else {
			log::debug!(target: LOG_TARGET, "No gas provided");
			return Err(InvalidTransaction::Call);
		};

		let Some(effective_gas_price) = tx.effective_gas_price(base_fee) else {
			log::debug!(target: LOG_TARGET, "No gas_price provided");
			return Err(InvalidTransaction::Payment);
		};

		let chain_id = tx.chain_id.unwrap_or_default();

		if chain_id != <Self::Config as Config>::ChainId::get().into() {
			log::debug!(target: LOG_TARGET, "Invalid chain_id {chain_id:?}");
			return Err(InvalidTransaction::Call);
		}

		if effective_gas_price < base_fee {
			log::debug!(
				target: LOG_TARGET,
				"Specified gas_price is too low. effective_gas_price={effective_gas_price} base_fee={base_fee}"
			);
			return Err(InvalidTransaction::Payment);
		}

		let value = tx.value.unwrap_or_default();
		let data = tx.input.to_vec();

		let mut call = if let Some(dest) = tx.to {
			if dest == RUNTIME_PALLETS_ADDR {
				let call = CallOf::<Self::Config>::decode_all_with_depth_limit(
					MAX_EXTRINSIC_DEPTH,
					&mut &data[..],
				)
				.map_err(|_| {
					log::debug!(target: LOG_TARGET, "Failed to decode data as Call");
					InvalidTransaction::Call
				})?;

				if !value.is_zero() {
					log::debug!(target: LOG_TARGET, "Runtime pallets address cannot be called with value");
					return Err(InvalidTransaction::Call)
				}

				call
			} else {
				let call = crate::Call::eth_call::<Self::Config> {
					dest,
					value,
					gas_limit: Zero::zero(),
					storage_deposit_limit: BalanceOf::<Self::Config>::max_value(),
					data,
					effective_gas_price,
				}
				.into();
				call
			}
		} else {
			let (code, data) = if data.starts_with(&polkavm_common::program::BLOB_MAGIC) {
				let Some((code, data)) = extract_code_and_data(&data) else {
					log::debug!(target: LOG_TARGET, "Failed to extract polkavm code & data");
					return Err(InvalidTransaction::Call);
				};
				(code, data)
			} else {
				(data, Default::default())
			};

			let call = crate::Call::eth_instantiate_with_code::<Self::Config> {
				value,
				gas_limit: Zero::zero(),
				storage_deposit_limit: BalanceOf::<Self::Config>::max_value(),
				code,
				data,
				effective_gas_price,
			}
			.into();

			call
		};

		let mut info = call.get_dispatch_info();
		let nonce = tx.nonce.unwrap_or_default().try_into().map_err(|_| {
			log::debug!(target: LOG_TARGET, "Failed to convert nonce");
			InvalidTransaction::Call
		})?;

		info.extension_weight = Self::get_eth_extension(nonce, 0u32.into()).weight(&call);
		let extrinsic_fee = <Self::Config as Config>::FeeInfo::tx_fee(encoded_len as u32, &info);

		// the fee as signed off by the eth wallet. we cannot consume more.
		let eth_fee = effective_gas_price.saturating_mul(gas) /
			<Self::Config as Config>::NativeToEthRatio::get();

		// this is the fee left after accounting for the extrinsic itself
		// the rest if for the weight and storage deposit limit
		let remaining_fee = eth_fee.checked_sub(extrinsic_fee.into()).ok_or_else(|| {
			log::debug!(target: LOG_TARGET, "Not enough gas supplied to cover the extrinsic base fee. eth_fee={eth_fee:?} extrinsic_fee={extrinsic_fee:?}");
			InvalidTransaction::Payment
		})?;

		let weight_limit = {
			let remaining_unadjusted_fee =
				<Self::Config as Config>::FeeInfo::next_fee_multiplier_reciprocal()
					.saturating_mul_int(<BalanceOf<Self::Config>>::saturated_from(remaining_fee));
			let weight_limit =
				<Self::Config as Config>::FeeInfo::fee_to_weight(remaining_unadjusted_fee);
			call.set_weight_limit(weight_limit);
			let factor = FixedU128::from_rational(3, 4);
			let max_weight = <Self::Config as frame_system::Config>::BlockWeights::get()
				.get(DispatchClass::Normal)
				.max_extrinsic
				.unwrap_or_else(|| {
					<Self::Config as frame_system::Config>::BlockWeights::get().max_block
				});
			let max_weight = Weight::from_parts(
				factor.saturating_mul_int(max_weight.ref_time()),
				factor.saturating_mul_int(max_weight.proof_size()),
			);
			let mut info = call.get_dispatch_info();
			info.extension_weight = Self::get_eth_extension(nonce, 0u32.into()).weight(&call);
			let overweight_by = info.total_weight().saturating_sub(max_weight);
			let capped_weight = weight_limit.saturating_sub(overweight_by);
			call.set_weight_limit(capped_weight);
			capped_weight
		};

		// the overall fee of the extrinsic including the gas limit
		let mut info = call.get_dispatch_info();
		info.extension_weight = Self::get_eth_extension(nonce, 0u32.into()).weight(&call);
		let tx_fee = <Self::Config as Config>::FeeInfo::tx_fee(encoded_len as u32, &info);

		// the leftover we make available to the deposit collection system
		let deposit_source = <Self::Config as Config>::FeeInfo::deposit_source().ok_or_else(|| {
			log::debug!(target: LOG_TARGET, "You need to supply a proper T::FeeInfo implemention. This is a bug.");
			InvalidTransaction::Payment
		})?;
		let storage_deposit = eth_fee.saturating_sub(tx_fee.into()).saturated_into();
		<Self::Config as Config>::Currency::hold(&deposit_source, &signer, storage_deposit)
			.map_err(|_| {
				log::debug!(target: LOG_TARGET, "Failed to hold storage deposit");
				InvalidTransaction::Call
			})?;

		crate::tracing::if_tracing(|tracer| {
			tracer.watch_address(&Pallet::<Self::Config>::block_author().unwrap_or_default());
			tracer.watch_address(&signer_addr);
		});

		log::debug!(target: LOG_TARGET, "\
			Created checked Ethereum transaction with: \
			gas={gas} \
			extrinsic_fee={extrinsic_fee:?} \
			weight_limit={weight_limit} \
			additional_storage_deposit_held={storage_deposit:?} \
			effective_gas_price={effective_gas_price} \
			base_fee={base_fee} \
			nonce={nonce:?}
			"
		);

		// We can't calculate a tip because it needs to be based on the actual gas used which we
		// cannot know pre-dispatch. Hence we never supply a tip here or it would be way too high.
		Ok(CheckedExtrinsic {
			format: ExtrinsicFormat::Signed(
				signer.into(),
				Self::get_eth_extension(nonce, Zero::zero()),
			),
			function: call,
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
		Weight,
	};
	use frame_support::{error::LookupError, traits::fungible::Mutate};
	use pallet_revive_fixtures::compile_module;
	use sp_runtime::traits::{self, Checkable, DispatchTransaction};

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
			}
		}

		fn data(mut self, data: Vec<u8>) -> Self {
			self.tx.input = Bytes(data).into();
			self
		}

		fn estimate_gas(&mut self) {
			let dry_run = crate::Pallet::<Test>::dry_run_eth_transact(self.tx.clone(), Weight::MAX);

			self.tx.gas_price = Some(<Pallet<Test>>::evm_gas_price());

			match dry_run {
				Ok(dry_run) => {
					self.tx.gas = Some(dry_run.eth_gas);
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
		) -> Result<(RuntimeCall, SignedExtra, GenericTransaction), TransactionValidityError> {
			self.mutate_estimate_and_check(Box::new(|_| ()))
		}

		/// Call `check` on the unchecked extrinsic, and `pre_dispatch` on the signed extension.
		fn mutate_estimate_and_check(
			mut self,
			f: Box<dyn FnOnce(&mut GenericTransaction) -> ()>,
		) -> Result<(RuntimeCall, SignedExtra, GenericTransaction), TransactionValidityError> {
			ExtBuilder::default().build().execute_with(|| self.estimate_gas());
			f(&mut self.tx);
			ExtBuilder::default().build().execute_with(|| {
				let UncheckedExtrinsicBuilder { tx, before_validate, .. } = self.clone();

				// Fund the account.
				let account = Account::default();
				let _ = <Test as Config>::Currency::set_balance(
					&account.substrate_account(),
					100_000_000_000_000,
				);

				let payload = account
					.sign_transaction(tx.clone().try_into_unsigned().unwrap())
					.signed_payload();
				let call = RuntimeCall::Contracts(crate::Call::eth_transact { payload });

				let encoded_len = call.encoded_size();
				let uxt: UncheckedExtrinsic = generic::UncheckedExtrinsic::new_bare(call).into();
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

				Ok((result.function, extra, tx))
			})
		}
	}

	#[test]
	fn check_eth_transact_call_works() {
		let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]));
		let (call, _, tx) = builder.check().unwrap();
		let effective_gas_price: u32 = <Test as Config>::NativeToEthRatio::get();

		assert_eq!(
			call,
			crate::Call::eth_call::<Test> {
				dest: tx.to.unwrap(),
				value: tx.value.unwrap_or_default().as_u64().into(),
				data: tx.input.to_vec(),
				// its a transfer to a non contract: does not use any gas
				gas_limit: Zero::zero(),
				storage_deposit_limit: <BalanceOf<Test>>::max_value(),
				effective_gas_price: effective_gas_price.into(),
			}
			.into()
		);
	}

	#[test]
	fn check_eth_transact_instantiate_works() {
		let (code, _) = compile_module("dummy").unwrap();
		let data = vec![];
		let builder = UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone());
		let (call, _, tx) = builder.check().unwrap();
		let effective_gas_price: u32 = <Test as Config>::NativeToEthRatio::get();

		assert_eq!(
			call,
			crate::Call::eth_instantiate_with_code::<Test> {
				value: tx.value.unwrap_or_default().as_u64().into(),
				code,
				data,
				gas_limit: Weight::from_parts(54753, 0),
				storage_deposit_limit: <BalanceOf<Test>>::max_value(),
				effective_gas_price: effective_gas_price.into(),
			}
			.into()
		);
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
					tx.gas_price = Some(tx.gas_price.unwrap() / 2);
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
		let data = vec![];
		let (_, extra, _tx) =
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
		let (call, _, _) = builder.check().unwrap();

		assert_eq!(call, remark);
	}
}
