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
	evm::api::{GenericTransaction, TransactionSigned},
	AccountIdOf, AddressMapper, BalanceOf, MomentOf, Weight, LOG_TARGET,
};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::{ExtrinsicCall, InherentBuilder, SignedTransactionBuilder},
};
use pallet_transaction_payment::OnChargeTransaction;
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_arithmetic::Percent;
use sp_core::{Get, H256, U256};
use sp_runtime::{
	generic::{self, CheckedExtrinsic, ExtrinsicFormat},
	traits::{
		self, Checkable, Dispatchable, ExtrinsicLike, ExtrinsicMetadata, IdentifyAccount, Member,
		TransactionExtension,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	OpaqueExtrinsic, RuntimeDebug, Saturating,
};

use alloc::vec::Vec;

type CallOf<T> = <T as frame_system::Config>::RuntimeCall;

/// The EVM gas price.
/// This constant is used by the proxy to advertise it via the eth_gas_price RPC.
///
/// We use a fixed value for the gas price.
/// This let us calculate the gas estimate for a transaction with the formula:
/// `estimate_gas = substrate_fee / gas_price`.
pub const GAS_PRICE: u32 = 1u32;

/// Wraps [`generic::UncheckedExtrinsic`] to support checking unsigned
/// [`crate::Call::eth_transact`] extrinsic.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
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

use sp_runtime::traits::MaybeDisplay;
type OnChargeTransactionBalanceOf<T> = <<T as pallet_transaction_payment::Config>::OnChargeTransaction as OnChargeTransaction<T>>::Balance;

impl<LookupSource, Signature, E, Lookup> Checkable<Lookup>
	for UncheckedExtrinsic<LookupSource, Signature, E>
where
	E: EthExtra,
	Self: Encode,
	<E::Config as frame_system::Config>::Nonce: TryFrom<U256>,
	<E::Config as frame_system::Config>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
	OnChargeTransactionBalanceOf<E::Config>: Into<BalanceOf<E::Config>>,
	BalanceOf<E::Config>: Into<U256> + TryFrom<U256>,
	MomentOf<E::Config>: Into<U256>,
	CallOf<E::Config>: From<crate::Call<E::Config>> + TryInto<crate::Call<E::Config>>,
	<E::Config as frame_system::Config>::Hash: frame_support::traits::IsType<H256>,

	// required by Checkable for `generic::UncheckedExtrinsic`
	LookupSource: Member + MaybeDisplay,
	CallOf<E::Config>: Encode + Member + Dispatchable,
	Signature: Member + traits::Verify,
	<Signature as traits::Verify>::Signer: IdentifyAccount<AccountId = AccountIdOf<E::Config>>,
	E::Extension: Encode + TransactionExtension<CallOf<E::Config>>,
	Lookup: traits::Lookup<Source = LookupSource, Target = AccountIdOf<E::Config>>,
{
	type Checked = CheckedExtrinsic<AccountIdOf<E::Config>, CallOf<E::Config>, E::Extension>;

	fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
		if !self.0.is_signed() {
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
	type Config: crate::Config + pallet_transaction_payment::Config;

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
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<Self::Config>,
		encoded_len: usize,
	) -> Result<
		CheckedExtrinsic<AccountIdOf<Self::Config>, CallOf<Self::Config>, Self::Extension>,
		InvalidTransaction,
	>
	where
		<Self::Config as frame_system::Config>::Nonce: TryFrom<U256>,
		BalanceOf<Self::Config>: Into<U256> + TryFrom<U256>,
		MomentOf<Self::Config>: Into<U256>,
		<Self::Config as frame_system::Config>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
		OnChargeTransactionBalanceOf<Self::Config>: Into<BalanceOf<Self::Config>>,
		CallOf<Self::Config>: From<crate::Call<Self::Config>>,
		<Self::Config as frame_system::Config>::Hash: frame_support::traits::IsType<H256>,
	{
		let tx = TransactionSigned::decode(&payload).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to decode transaction: {err:?}");
			InvalidTransaction::Call
		})?;

		let signer = tx.recover_eth_address().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to recover signer: {err:?}");
			InvalidTransaction::BadProof
		})?;

		let signer =
			<Self::Config as crate::Config>::AddressMapper::to_fallback_account_id(&signer);
		let GenericTransaction { nonce, chain_id, to, value, input, gas, gas_price, .. } =
			GenericTransaction::from_signed(tx, None);

		if chain_id.unwrap_or_default() != <Self::Config as crate::Config>::ChainId::get().into() {
			log::debug!(target: LOG_TARGET, "Invalid chain_id {chain_id:?}");
			return Err(InvalidTransaction::Call);
		}

		let value = crate::Pallet::<Self::Config>::convert_evm_to_native(value.unwrap_or_default())
			.map_err(|err| {
				log::debug!(target: LOG_TARGET, "Failed to convert value to native: {err:?}");
				InvalidTransaction::Call
			})?;

		let data = input.unwrap_or_default().0;
		let call = if let Some(dest) = to {
			crate::Call::call::<Self::Config> {
				dest,
				value,
				gas_limit,
				storage_deposit_limit,
				data,
			}
		} else {
			let blob = match polkavm::ProgramBlob::blob_length(&data) {
				Some(blob_len) =>
					blob_len.try_into().ok().and_then(|blob_len| (data.split_at_checked(blob_len))),
				_ => None,
			};

			let Some((code, data)) = blob else {
				log::debug!(target: LOG_TARGET, "Failed to extract polkavm code & data");
				return Err(InvalidTransaction::Call);
			};

			crate::Call::instantiate_with_code::<Self::Config> {
				value,
				gas_limit,
				storage_deposit_limit,
				code: code.to_vec(),
				data: data.to_vec(),
				salt: None,
			}
		};

		let nonce = nonce.unwrap_or_default().try_into().map_err(|_| InvalidTransaction::Call)?;

		// Fees calculated with the fixed `GAS_PRICE`
		// When we dry-run the transaction, we set the gas to `Fee / GAS_PRICE`
		let eth_fee_no_tip = U256::from(GAS_PRICE)
			.saturating_mul(gas.unwrap_or_default())
			.try_into()
			.map_err(|_| InvalidTransaction::Call)?;

		// Fees with the actual gas_price from the transaction.
		let eth_fee: BalanceOf<Self::Config> = U256::from(gas_price.unwrap_or_default())
			.saturating_mul(gas.unwrap_or_default())
			.try_into()
			.map_err(|_| InvalidTransaction::Call)?;

		let info = call.get_dispatch_info();
		let function: CallOf<Self::Config> = call.into();

		// Fees calculated from the extrinsic, without the tip.
		let actual_fee: BalanceOf<Self::Config> =
			pallet_transaction_payment::Pallet::<Self::Config>::compute_fee(
				encoded_len as u32,
				&info,
				Default::default(),
			)
			.into();
		log::trace!(target: LOG_TARGET, "try_into_checked_extrinsic: encoded_len: {encoded_len:?} actual_fee: {actual_fee:?} eth_fee: {eth_fee:?}");

		// The fees from the Ethereum transaction should be greater or equal to the actual fees paid
		// by the account.
		if eth_fee < actual_fee {
			log::debug!(target: LOG_TARGET, "fees {eth_fee:?} too low for the extrinsic {actual_fee:?}");
			return Err(InvalidTransaction::Payment.into())
		}

		let min = actual_fee.min(eth_fee_no_tip);
		let max = actual_fee.max(eth_fee_no_tip);
		let diff = Percent::from_rational(max - min, min);
		if diff > Percent::from_percent(10) {
			log::trace!(target: LOG_TARGET, "Difference between the extrinsic fees {actual_fee:?} and the Ethereum gas fees {eth_fee_no_tip:?} should be no more than 10% got {diff:?}");
			return Err(InvalidTransaction::Call.into())
		} else {
			log::trace!(target: LOG_TARGET, "Difference between the extrinsic fees {actual_fee:?} and the Ethereum gas fees {eth_fee_no_tip:?}:  {diff:?}");
		}

		let tip = eth_fee.saturating_sub(eth_fee_no_tip);
		log::debug!(target: LOG_TARGET, "Created checked Ethereum transaction with nonce {nonce:?} and tip: {tip:?}");
		Ok(CheckedExtrinsic {
			format: ExtrinsicFormat::Signed(signer.into(), Self::get_eth_extension(nonce, tip)),
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
		tests::{ExtBuilder, RuntimeCall, RuntimeOrigin, Test},
	};
	use frame_support::{error::LookupError, traits::fungible::Mutate};
	use pallet_revive_fixtures::compile_module;
	use sp_runtime::{
		traits::{Checkable, DispatchTransaction},
		MultiAddress, MultiSignature,
	};
	type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

	#[derive(Clone, PartialEq, Eq, Debug)]
	pub struct Extra;
	type SignedExtra = (frame_system::CheckNonce<Test>, ChargeTransactionPayment<Test>);

	use pallet_transaction_payment::ChargeTransactionPayment;
	impl EthExtra for Extra {
		type Config = Test;
		type Extension = SignedExtra;

		fn get_eth_extension(nonce: u32, tip: BalanceOf<Test>) -> Self::Extension {
			(frame_system::CheckNonce::from(nonce), ChargeTransactionPayment::from(tip))
		}
	}

	type Ex = UncheckedExtrinsic<MultiAddress<AccountId32, u32>, MultiSignature, Extra>;
	struct TestContext;

	impl traits::Lookup for TestContext {
		type Source = MultiAddress<AccountId32, u32>;
		type Target = AccountIdOf<Test>;
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
		tx: GenericTransaction,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<Test>,
		before_validate: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
	}

	impl UncheckedExtrinsicBuilder {
		/// Create a new builder with default values.
		fn new() -> Self {
			Self {
				tx: GenericTransaction {
					from: Some(Account::default().address()),
					chain_id: Some(<Test as crate::Config>::ChainId::get().into()),
					gas_price: Some(U256::from(GAS_PRICE)),
					..Default::default()
				},
				gas_limit: Weight::zero(),
				storage_deposit_limit: 0,
				before_validate: None,
			}
		}

		fn estimate_gas(&mut self) {
			let dry_run =
				crate::Pallet::<Test>::bare_eth_transact(self.tx.clone(), Weight::MAX, |call| {
					let call = RuntimeCall::Contracts(call);
					let uxt: Ex = sp_runtime::generic::UncheckedExtrinsic::new_bare(call).into();
					uxt.encoded_size() as u32
				});

			match dry_run {
				Ok(dry_run) => {
					log::debug!(target: LOG_TARGET, "Estimated gas: {:?}", dry_run.eth_gas);
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
			ExtBuilder::default().build().execute_with(|| builder.estimate_gas());
			builder
		}

		/// Create a new builder with an instantiate call.
		fn instantiate_with(code: Vec<u8>, data: Vec<u8>) -> Self {
			let mut builder = Self::new();
			builder.tx.input = Some(Bytes(code.into_iter().chain(data.into_iter()).collect()));
			ExtBuilder::default().build().execute_with(|| builder.estimate_gas());
			builder
		}

		/// Update the transaction with the given function.
		fn update(mut self, f: impl FnOnce(&mut GenericTransaction) -> ()) -> Self {
			f(&mut self.tx);
			self
		}
		/// Set before_validate function.
		fn before_validate(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
			self.before_validate = Some(std::sync::Arc::new(f));
			self
		}

		/// Call `check` on the unchecked extrinsic, and `pre_dispatch` on the signed extension.
		fn check(&self) -> Result<(RuntimeCall, SignedExtra), TransactionValidityError> {
			ExtBuilder::default().build().execute_with(|| {
				let UncheckedExtrinsicBuilder {
					tx,
					gas_limit,
					storage_deposit_limit,
					before_validate,
				} = self.clone();

				// Fund the account.
				let account = Account::default();
				let _ = <Test as crate::Config>::Currency::set_balance(
					&account.substrate_account(),
					100_000_000_000_000,
				);

				let payload =
					account.sign_transaction(tx.try_into_unsigned().unwrap()).signed_payload();
				let call = RuntimeCall::Contracts(crate::Call::eth_transact {
					payload,
					gas_limit,
					storage_deposit_limit,
				});

				let encoded_len = call.encoded_size();
				let uxt: Ex = generic::UncheckedExtrinsic::new_bare(call).into();
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

				Ok((result.function, extra))
			})
		}
	}

	#[test]
	fn check_eth_transact_call_works() {
		let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]));
		assert_eq!(
			builder.check().unwrap().0,
			crate::Call::call::<Test> {
				dest: builder.tx.to.unwrap(),
				value: builder.tx.value.unwrap_or_default().as_u64(),
				gas_limit: builder.gas_limit,
				storage_deposit_limit: builder.storage_deposit_limit,
				data: builder.tx.input.unwrap_or_default().0
			}
			.into()
		);
	}

	#[test]
	fn check_eth_transact_instantiate_works() {
		let (code, _) = compile_module("dummy").unwrap();
		let data = vec![];
		let builder = UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone());

		assert_eq!(
			builder.check().unwrap().0,
			crate::Call::instantiate_with_code::<Test> {
				value: builder.tx.value.unwrap_or_default().as_u64(),
				gas_limit: builder.gas_limit,
				storage_deposit_limit: builder.storage_deposit_limit,
				code,
				data,
				salt: None
			}
			.into()
		);
	}

	#[test]
	fn check_eth_transact_nonce_works() {
		let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]))
			.update(|tx| tx.nonce = Some(1u32.into()));

		assert_eq!(
			builder.check(),
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
		let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]))
			.update(|tx| tx.chain_id = Some(42.into()));

		assert_eq!(
			builder.check(),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
		);
	}

	#[test]
	fn check_instantiate_data() {
		let code = b"invalid code".to_vec();
		let data = vec![1];
		let builder = UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone());

		// Fail because the tx input fail to get the blob length
		assert_eq!(
			builder.clone().update(|tx| tx.input = Some(Bytes(vec![1, 2, 3]))).check(),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
		);
	}

	#[test]
	fn check_transaction_fees() {
		let scenarios: [(_, Box<dyn FnOnce(&mut GenericTransaction)>, _); 5] = [
			(
				"Eth fees too low",
				Box::new(|tx| {
					tx.gas_price = Some(tx.gas_price.unwrap() / 2);
				}),
				InvalidTransaction::Payment,
			),
			(
				"Gas fees too high",
				Box::new(|tx| {
					tx.gas = Some(tx.gas.unwrap() * 2);
				}),
				InvalidTransaction::Call,
			),
			(
				"Gas fees too low",
				Box::new(|tx| {
					tx.gas = Some(tx.gas.unwrap() * 2);
				}),
				InvalidTransaction::Call,
			),
			(
				"Diff > 10%",
				Box::new(|tx| {
					tx.gas = Some(tx.gas.unwrap() * 111 / 100);
				}),
				InvalidTransaction::Call,
			),
			(
				"Diff < 10%",
				Box::new(|tx| {
					tx.gas_price = Some(tx.gas_price.unwrap() * 2);
					tx.gas = Some(tx.gas.unwrap() * 89 / 100);
				}),
				InvalidTransaction::Call,
			),
		];

		for (msg, update_tx, err) in scenarios {
			let builder =
				UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20])).update(update_tx);

			assert_eq!(builder.check(), Err(TransactionValidityError::Invalid(err)), "{}", msg);
		}
	}

	#[test]
	fn check_transaction_tip() {
		let (code, _) = compile_module("dummy").unwrap();
		let data = vec![];
		let builder = UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone())
			.update(|tx| {
				tx.gas_price = Some(tx.gas_price.unwrap() * 103 / 100);
				log::debug!(target: LOG_TARGET, "Gas price: {:?}", tx.gas_price);
			});

		let tx = &builder.tx;
		let expected_tip =
			tx.gas_price.unwrap() * tx.gas.unwrap() - U256::from(GAS_PRICE) * tx.gas.unwrap();
		let (_, extra) = builder.check().unwrap();
		assert_eq!(U256::from(extra.1.tip()), expected_tip);
	}
}
