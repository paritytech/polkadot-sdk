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
	evm::api::{TransactionLegacySigned, TransactionLegacyUnsigned},
	AccountIdOf, AddressMapper, BalanceOf, MomentOf, Weight, LOG_TARGET,
};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::{ExtrinsicCall, InherentBuilder, SignedTransactionBuilder},
};
use pallet_transaction_payment::OnChargeTransaction;
use scale_info::TypeInfo;
use sp_arithmetic::Percent;
use sp_core::{Get, U256};
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
pub const GAS_PRICE: u32 = 1_000u32;

/// Wraps [`generic::UncheckedExtrinsic`] to support checking unsigned
/// [`crate::Call::eth_transact`] extrinsic.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(E))]
pub struct UncheckedExtrinsic<Address, Signature, E: EthExtra>(
	pub generic::UncheckedExtrinsic<Address, CallOf<E::Config>, Signature, E::Extension>,
);

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
	const VERSION: u8 =
		generic::UncheckedExtrinsic::<Address, CallOf<E::Config>, Signature, E::Extension>::VERSION;
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
			.map_err(|e| serde::de::Error::custom(sp_runtime::format!("Decode error: {}", e)))
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

		// Fees calculated with the fixed `GAS_PRICE` that should be used to estimate the gas.
		let eth_fee_no_tip = U256::from(GAS_PRICE)
			.saturating_mul(gas)
			.try_into()
			.map_err(|_| InvalidTransaction::Call)?;

		// Fees with the actual gas_price from the transaction.
		let eth_fee: BalanceOf<Self::Config> = U256::from(gas_price)
			.saturating_mul(gas)
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

		log::debug!(target: LOG_TARGET, "Checking Ethereum transaction fees:
			dispatch_info: {info:?}
			encoded_len: {encoded_len:?}
			fees: {actual_fee:?}
		");

		if eth_fee < actual_fee {
			log::debug!(target: LOG_TARGET, "fees {eth_fee:?} too low for the extrinsic {actual_fee:?}");
			return Err(InvalidTransaction::Payment.into())
		}

		let min = actual_fee.min(eth_fee_no_tip);
		let max = actual_fee.max(eth_fee_no_tip);
		let diff = Percent::from_rational(max - min, min);
		if diff > Percent::from_percent(10) {
			log::debug!(target: LOG_TARGET, "Difference between the extrinsic fees {actual_fee:?} and the Ethereum gas fees {eth_fee_no_tip:?} should be no more than 10% got {diff:?}");
			return Err(InvalidTransaction::Call.into())
		} else {
			log::debug!(target: LOG_TARGET, "Difference between the extrinsic fees {actual_fee:?} and the Ethereum gas fees {eth_fee_no_tip:?}:  {diff:?}");
		}

		let tip = eth_fee.saturating_sub(eth_fee_no_tip);
		log::debug!(target: LOG_TARGET, "Created checked Ethereum transaction with nonce {nonce:?} and tip: {tip:?}");
		Ok(CheckedExtrinsic {
			format: ExtrinsicFormat::Signed(signer.into(), Self::get_eth_extension(nonce, tip)),
			function,
		})
	}
}

#[cfg(feature = "riscv")]
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
	use rlp::Encodable;
	use sp_runtime::{
		traits::{Checkable, DispatchTransaction},
		MultiAddress, MultiSignature,
	};
	type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

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
		pub fn account_id(&self) -> AccountIdOf<Test> {
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
					gas_price: U256::from(GAS_PRICE),
					..Default::default()
				},
				gas_limit: Weight::zero(),
				storage_deposit_limit: 0,
			}
		}

		/// Create a new builder with a call to the given address.
		fn call_with(dest: H160) -> Self {
			let mut builder = Self::new();
			builder.tx.to = Some(dest);
			builder.tx.gas = U256::from(516_708u128);
			builder
		}

		/// Create a new builder with an instantiate call.
		fn instantiate_with(code: Vec<u8>, data: Vec<u8>) -> Self {
			let mut builder = Self::new();
			builder.tx.input = Bytes(code.into_iter().chain(data.into_iter()).collect());
			builder.tx.gas = U256::from(1_035_070u128);
			builder
		}

		/// Update the transaction with the given function.
		fn update(mut self, f: impl FnOnce(&mut TransactionLegacyUnsigned) -> ()) -> Self {
			f(&mut self.tx);
			self
		}

		/// Call `check` on the unchecked extrinsic, and `pre_dispatch` on the signed extension.
		fn check(&self) -> Result<(RuntimeCall, SignedExtra), TransactionValidityError> {
			let UncheckedExtrinsicBuilder { tx, gas_limit, storage_deposit_limit } = self.clone();

			// Fund the account.
			let account = Account::default();
			let _ = <Test as crate::Config>::Currency::set_balance(
				&account.account_id(),
				100_000_000_000_000,
			);

			let payload = account.sign_transaction(tx).rlp_bytes().to_vec();
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

			extra.clone().validate_and_prepare(
				RuntimeOrigin::signed(account_id),
				&result.function,
				&result.function.get_dispatch_info(),
				encoded_len,
			)?;

			Ok((result.function, extra))
		}
	}

	#[test]
	fn check_eth_transact_call_works() {
		ExtBuilder::default().build().execute_with(|| {
			let builder = UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20]));
			assert_eq!(
				builder.check().unwrap().0,
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
				builder.check().unwrap().0,
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
	fn check_transaction_fees() {
		ExtBuilder::default().build().execute_with(|| {
			let scenarios: [(_, Box<dyn FnOnce(&mut TransactionLegacyUnsigned)>, _); 5] = [
				("Eth fees too low", Box::new(|tx| tx.gas_price /= 2), InvalidTransaction::Payment),
				("Gas fees too high", Box::new(|tx| tx.gas *= 2), InvalidTransaction::Call),
				("Gas fees too low", Box::new(|tx| tx.gas *= 2), InvalidTransaction::Call),
				(
					"Diff > 10%",
					Box::new(|tx| tx.gas = tx.gas * 111 / 100),
					InvalidTransaction::Call,
				),
				(
					"Diff < 10%",
					Box::new(|tx| {
						tx.gas_price *= 2;
						tx.gas = tx.gas * 89 / 100
					}),
					InvalidTransaction::Call,
				),
			];

			for (msg, update_tx, err) in scenarios {
				let builder =
					UncheckedExtrinsicBuilder::call_with(H160::from([1u8; 20])).update(update_tx);

				assert_eq!(builder.check(), Err(TransactionValidityError::Invalid(err)), "{}", msg);
			}
		});
	}

	#[test]
	fn check_transaction_tip() {
		ExtBuilder::default().build().execute_with(|| {
			let (code, _) = compile_module("dummy").unwrap();
			let data = vec![];
			let builder = UncheckedExtrinsicBuilder::instantiate_with(code.clone(), data.clone())
				.update(|tx| tx.gas_price = tx.gas_price * 103 / 100);

			let tx = &builder.tx;
			let expected_tip = tx.gas_price * tx.gas - U256::from(GAS_PRICE) * tx.gas;
			let (_, extra) = builder.check().unwrap();
			assert_eq!(U256::from(extra.1.tip()), expected_tip);
		});
	}
}
