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

//! This module provides a means for executing contracts
//! represented in vm bytecode.

pub mod evm;
pub mod pvm;
mod runtime_costs;

pub use runtime_costs::RuntimeCosts;

use crate::{
	AccountIdOf, BalanceOf, CodeInfoOf, CodeRemoved, Config, Error, ExecConfig, ExecError,
	HoldReason, LOG_TARGET, Pallet, PristineCode, StorageDeposit, Weight,
	access_list::CodeLoadWarmth,
	deposit_payment,
	exec::{ExecResult, Executable, ExportedFunction, Ext},
	frame_support::ensure,
	metering::{ResourceMeter, State, Token},
	weights::WeightInfo,
};
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::dispatch::DispatchResult;
use pallet_revive_uapi::ReturnErrorCode;
use sp_core::{Get, H256};
use sp_runtime::{DispatchError, Saturating, traits::BadOrigin};

/// Validated Vm module ready for execution.
/// This data structure is immutable once created and stored.
#[derive(Encode, Decode, scale_info::TypeInfo)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(T))]
pub struct ContractBlob<T: Config> {
	code: Vec<u8>,
	// This isn't needed for contract execution and is not stored alongside it.
	#[codec(skip)]
	code_info: CodeInfo<T>,
	// This is for not calculating the hash every time we need it.
	#[codec(skip)]
	code_hash: H256,
}

#[derive(
	PartialEq, Eq, Debug, Copy, Clone, Encode, Decode, MaxEncodedLen, scale_info::TypeInfo,
)]
pub enum BytecodeType {
	/// The code is a PVM bytecode.
	Pvm,
	/// The code is an EVM bytecode.
	Evm,
}

/// Contract code related data, such as:
///
/// - owner of the contract, i.e. account uploaded its code,
/// - storage deposit amount,
/// - reference count,
///
/// It is stored in a separate storage entry to avoid loading the code when not necessary.
#[derive(
	frame_support::DebugNoBound, Clone, Encode, Decode, scale_info::TypeInfo, MaxEncodedLen,
)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(T))]
pub struct CodeInfo<T: Config> {
	/// The account that has uploaded the contract code and hence is allowed to remove it.
	owner: AccountIdOf<T>,
	/// The amount of balance that was deposited by the owner in order to store it on-chain.
	#[codec(compact)]
	deposit: BalanceOf<T>,
	/// The number of instantiated contracts that use this as their code.
	#[codec(compact)]
	refcount: u64,
	/// Length of the code in bytes.
	code_len: u32,
	/// Bytecode type
	code_type: BytecodeType,
	/// The behaviour version that this contract operates under.
	///
	/// Whenever any observeable change (with the exception of weights) are made we need
	/// to make sure that already deployed contracts will not be affected. We do this by
	/// exposing the old behaviour depending on the set behaviour version of the contract.
	///
	/// As of right now this is a reserved field that is always set to 0.
	behaviour_version: u32,
}

/// Calculate the deposit required for storing code and its metadata.
pub fn calculate_code_deposit<T: Config>(code_len: u32) -> BalanceOf<T> {
	let bytes_added = code_len.saturating_add(<CodeInfo<T>>::max_encoded_len() as u32);
	T::DepositPerByte::get()
		.saturating_mul(bytes_added.into())
		.saturating_add(T::DepositPerItem::get().saturating_mul(2u32.into()))
}

impl ExportedFunction {
	/// The vm export name for the function.
	fn identifier(&self) -> &str {
		match self {
			Self::Constructor => "deploy",
			Self::Call => "call",
		}
	}
}

/// Cost of code loading from storage.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone, Copy)]
struct CodeLoadToken {
	code_len: u32,
	code_type: BytecodeType,
	warmth: CodeLoadWarmth,
	charge_refcount_write: bool,
}

impl CodeLoadToken {
	fn from_code_info<T: Config>(
		code_info: &CodeInfo<T>,
		warmth: CodeLoadWarmth,
		charge_refcount_write: bool,
	) -> Self {
		Self {
			code_len: code_info.code_len,
			code_type: code_info.code_type,
			warmth,
			charge_refcount_write,
		}
	}
}

impl<T: Config> Token<T> for CodeLoadToken {
	fn weight(&self) -> Weight {
		let len_weight_of =
			|weight_fn: fn(u32) -> Weight| weight_fn(self.code_len).saturating_sub(weight_fn(0));

		let load_weight = runtime_costs::weight_by_warmth::<T>(
			&[self.warmth.info, self.warmth.blob],
			|| {
				// Charge code_load since the call and instantiate benches whitelist the code
				// reads. This overlaps their ref_time, so it slightly overcharges.
				T::WeightInfo::code_load().saturating_add(match self.code_type {
					BytecodeType::Pvm => len_weight_of(T::WeightInfo::call_with_pvm_code_per_byte),
					BytecodeType::Evm => len_weight_of(T::WeightInfo::call_with_evm_code_per_byte),
				})
			},
			|| match self.code_type {
				BytecodeType::Pvm => len_weight_of(T::WeightInfo::call_with_pvm_code_per_byte_hot),
				BytecodeType::Evm => len_weight_of(T::WeightInfo::call_with_evm_code_per_byte_hot),
			},
		);

		let weight = match self.code_type {
			// the proof size impact is accounted for in the `call_with_pvm_code_per_byte`
			// strictly speaking we are double charging for the first BASIC_BLOCK_SIZE
			// instructions here. Let's consider this as a safety margin.
			BytecodeType::Pvm => load_weight.saturating_add(
				T::WeightInfo::basic_block_compilation(1)
					.saturating_sub(T::WeightInfo::basic_block_compilation(0))
					.set_proof_size(0),
			),
			BytecodeType::Evm => load_weight,
		};
		if self.charge_refcount_write {
			// The refcount bump writes `CodeInfoOf`, a key the instantiate benches
			// whitelist. Its trie walk is already paid by this load's read, so add
			// only the missing part of a write: the block-end re-hash of its path.
			weight.saturating_add(RuntimeCosts::deferred_write_cost::<T>())
		} else {
			weight
		}
	}
}

#[cfg(test)]
pub fn code_load_weight(code_len: u32) -> Weight {
	Token::<crate::tests::Test>::weight(&CodeLoadToken {
		code_len,
		code_type: BytecodeType::Pvm,
		warmth: CodeLoadWarmth::cold_non_revertible(),
		charge_refcount_write: false,
	})
}

impl<T: Config> ContractBlob<T> {
	/// Remove the code from storage and refund the deposit to its owner.
	///
	/// Applies all necessary checks before removing the code.
	pub fn remove(origin: &T::AccountId, code_hash: H256) -> DispatchResult {
		<CodeInfoOf<T>>::try_mutate_exists(&code_hash, |existing| {
			if let Some(code_info) = existing {
				ensure!(code_info.refcount == 0, <Error<T>>::CodeInUse);
				ensure!(&code_info.owner == origin, BadOrigin);
				<Pallet<T>>::refund_deposit(
					HoldReason::CodeUploadDepositReserve,
					&Pallet::<T>::account_id(),
					deposit_payment::Funds::Balance(&code_info.owner),
					code_info.deposit,
				)?;
				*existing = None;
				<PristineCode<T>>::remove(&code_hash);
				Ok(())
			} else {
				Err(<Error<T>>::CodeNotFound.into())
			}
		})
	}

	/// Puts the module blob into storage, and returns the deposit collected for the storage.
	pub fn store_code<S: State>(
		&mut self,
		exec_config: &ExecConfig<T>,
		meter: &mut ResourceMeter<T, S>,
	) -> Result<BalanceOf<T>, DispatchError> {
		let code_hash = *self.code_hash();
		ensure!(code_hash != H256::zero(), <Error<T>>::CodeNotFound);

		<CodeInfoOf<T>>::mutate(code_hash, |stored_code_info| {
			match stored_code_info {
				// Contract code is already stored in storage. Nothing to be done here.
				Some(_) => Ok(Default::default()),
				// Upload a new contract code.
				// We need to store the code and its code_info, and collect the deposit.
				// This `None` case happens only with freshly uploaded modules. This means that
				// the `owner` is always the origin of the current transaction.
				None => {
					let deposit = self.code_info.deposit;

					<Pallet<T>>::charge_deposit(
							HoldReason::CodeUploadDepositReserve,
							&self.code_info.owner,
							&Pallet::<T>::account_id(),
							deposit,
							exec_config,
						)
					 .inspect_err(|err| {
							log::debug!(target: LOG_TARGET, "failed to hold store code deposit {deposit:?} for owner: {:?}: {err:?}", self.code_info.owner);
					})?;

					meter.charge_deposit(&StorageDeposit::Charge(deposit))?;

					<PristineCode<T>>::insert(code_hash, &self.code.to_vec());
					*stored_code_info = Some(self.code_info.clone());
					Ok(deposit)
				},
			}
		})
	}
}

impl<T: Config> CodeInfo<T> {
	#[cfg(test)]
	pub fn new(owner: T::AccountId) -> Self {
		CodeInfo {
			owner,
			deposit: Default::default(),
			refcount: 0,
			code_len: 0,
			code_type: BytecodeType::Pvm,
			behaviour_version: Default::default(),
		}
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	pub fn new_with_deposit(owner: T::AccountId, deposit: BalanceOf<T>) -> Self {
		CodeInfo {
			owner,
			deposit,
			refcount: 0,
			code_len: 0,
			code_type: BytecodeType::Pvm,
			behaviour_version: Default::default(),
		}
	}

	/// Returns reference count of the module.
	#[cfg(test)]
	pub fn refcount(&self) -> u64 {
		self.refcount
	}

	/// Returns the deposit of the module.
	pub fn deposit(&self) -> BalanceOf<T> {
		self.deposit
	}

	/// Returns the account that uploaded the module.
	pub fn owner(&self) -> &AccountIdOf<T> {
		&self.owner
	}

	/// Returns the code length.
	pub fn code_len(&self) -> u64 {
		self.code_len.into()
	}

	/// Returns true if the executable is a PVM blob.
	pub fn is_pvm(&self) -> bool {
		matches!(self.code_type, BytecodeType::Pvm)
	}

	/// Returns the number of times the specified contract exists on the call stack. Delegated calls
	/// Increment the reference count of a stored code by one.
	///
	/// # Errors
	///
	/// [`Error::CodeNotFound`] is returned if no stored code found having the specified
	/// `code_hash`.
	pub fn increment_refcount(code_hash: H256) -> DispatchResult {
		<CodeInfoOf<T>>::mutate(code_hash, |existing| -> Result<(), DispatchError> {
			if let Some(info) = existing {
				info.refcount = info
					.refcount
					.checked_add(1)
					.ok_or_else(|| <Error<T>>::RefcountOverOrUnderflow)?;
				Ok(())
			} else {
				Err(Error::<T>::CodeNotFound.into())
			}
		})
	}

	/// Decrement the reference count of a stored code by one.
	/// Remove the code from storage when the reference count is zero.
	pub fn decrement_refcount(code_hash: H256) -> Result<CodeRemoved, DispatchError> {
		<CodeInfoOf<T>>::try_mutate_exists(code_hash, |existing| {
			let Some(code_info) = existing else { return Err(Error::<T>::CodeNotFound.into()) };

			if code_info.refcount == 1 {
				<Pallet<T>>::refund_deposit(
					HoldReason::CodeUploadDepositReserve,
					&Pallet::<T>::account_id(),
					deposit_payment::Funds::Balance(&code_info.owner),
					code_info.deposit,
				)?;

				*existing = None;
				<PristineCode<T>>::remove(&code_hash);

				Ok(CodeRemoved::Yes)
			} else {
				code_info.refcount = code_info
					.refcount
					.checked_sub(1)
					.ok_or_else(|| <Error<T>>::RefcountOverOrUnderflow)?;
				Ok(CodeRemoved::No)
			}
		})
	}
}

impl<T: Config> Executable<T> for ContractBlob<T> {
	fn from_storage<S: State>(
		code_hash: H256,
		meter: &mut ResourceMeter<T, S>,
		warmth: CodeLoadWarmth,
		charge_refcount_write: bool,
	) -> Result<Self, DispatchError> {
		let code_info = <CodeInfoOf<T>>::get(code_hash).ok_or(Error::<T>::CodeNotFound)?;
		meter.charge_weight_token(CodeLoadToken::from_code_info(
			&code_info,
			warmth,
			charge_refcount_write,
		))?;
		let code = <PristineCode<T>>::get(&code_hash).ok_or(Error::<T>::CodeNotFound)?;
		Ok(Self { code, code_info, code_hash })
	}

	fn from_evm_init_code(code: Vec<u8>, owner: AccountIdOf<T>) -> Result<Self, DispatchError> {
		ContractBlob::from_evm_init_code(code, owner)
	}

	fn execute<E: Ext<T = T>>(
		self,
		ext: &mut E,
		function: ExportedFunction,
		input_data: Vec<u8>,
	) -> ExecResult {
		if self.code_info().is_pvm() {
			let prepared_call =
				self.prepare_call(pvm::Runtime::new(ext, input_data), function, 0)?;
			prepared_call.call()
		} else if T::AllowEVMBytecode::get() {
			use revm::bytecode::Bytecode;
			let bytecode = Bytecode::new_raw(self.code.into());
			evm::call(bytecode, ext, input_data)
		} else {
			Err(Error::<T>::CodeRejected.into())
		}
	}

	fn code(&self) -> &[u8] {
		self.code.as_ref()
	}

	fn code_hash(&self) -> &H256 {
		&self.code_hash
	}

	fn code_info(&self) -> &CodeInfo<T> {
		&self.code_info
	}
}

/// Fallible conversion of a `ExecError` to `ReturnErrorCode`.
///
/// This is used when converting the error returned from a subcall in order to decide
/// whether to trap the caller or allow handling of the error.
pub(crate) fn exec_error_into_return_code<E: Ext>(
	from: ExecError,
) -> Result<ReturnErrorCode, DispatchError> {
	use crate::exec::ErrorOrigin::Callee;
	use ReturnErrorCode::*;

	let transfer_failed = Error::<E::T>::TransferFailed.into();
	let out_of_gas = Error::<E::T>::OutOfGas.into();
	let out_of_deposit = Error::<E::T>::StorageDepositLimitExhausted.into();
	let duplicate_contract = Error::<E::T>::DuplicateContract.into();
	let unsupported_precompile = Error::<E::T>::UnsupportedPrecompileAddress.into();

	// errors in the callee do not trap the caller
	match (from.error, from.origin) {
		(err, _) if err == transfer_failed => Ok(TransferFailed),
		(err, _) if err == duplicate_contract => Ok(DuplicateContractAddress),
		(err, _) if err == unsupported_precompile => Err(err),
		(err, Callee) if err == out_of_gas || err == out_of_deposit => Ok(OutOfResources),
		(_, Callee) => Ok(CalleeTrapped),
		(err, _) => Err(err),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		access_list::{Paid, Warmth},
		tests::Test,
	};

	/// Instantiating from an existing hash bumps the code's refcount, a write
	/// the instantiate benches whitelist, so the load charges it here.
	#[test]
	fn instantiate_code_load_charges_the_refcount_write() {
		let weight_of = |charge_refcount_write| {
			Token::<Test>::weight(&CodeLoadToken {
				code_len: 1024,
				code_type: BytecodeType::Pvm,
				warmth: CodeLoadWarmth::cold_non_revertible(),
				charge_refcount_write,
			})
		};

		assert_eq!(
			weight_of(true).saturating_sub(weight_of(false)),
			RuntimeCosts::deferred_write_cost::<Test>(),
			"the instantiate load must add exactly the refcount write",
		);
	}

	#[test]
	fn code_load_cold_hot_pricing() {
		let code_len = 1024_u32;
		let weight_of = |code_type, warmth| {
			Token::<Test>::weight(&CodeLoadToken {
				code_len,
				code_type,
				warmth,
				charge_refcount_write: false,
			})
		};

		for code_type in [BytecodeType::Pvm, BytecodeType::Evm] {
			let cold = weight_of(code_type, CodeLoadWarmth::cold_non_revertible());
			let hot = weight_of(
				code_type,
				CodeLoadWarmth { info: Warmth::Hot(Paid::Read), blob: Warmth::Hot(Paid::Read) },
			);
			let cold_revertible = weight_of(
				code_type,
				CodeLoadWarmth { info: Warmth::cold_revertible(), blob: Warmth::cold_revertible() },
			);
			assert!(
				cold.ref_time() > hot.ref_time(),
				"expected cold > hot ref_time for {code_type:?}: cold={cold:?} hot={hot:?}",
			);
			assert!(cold.proof_size() > 0, "cold proof_size {code_type:?}: {cold:?}");
			assert_eq!(hot.proof_size(), 0, "hot proof_size {code_type:?}: {hot:?}");

			let code_load_proof = <Test as Config>::WeightInfo::code_load().proof_size();
			assert!(
				cold.proof_size() >= code_load_proof + u64::from(code_len),
				"cold load must include the {code_load_proof}-byte code read proof plus \
				 {code_len} per-byte proof: {cold:?}",
			);
			assert!(
				cold_revertible.ref_time() > cold.ref_time(),
				"expected revertible > non-revertible ref_time for {code_type:?}: \
				 rev={cold_revertible:?} non={cold:?}",
			);

			// A mix of hot and cold items still prices cold.
			let info_only = weight_of(
				code_type,
				CodeLoadWarmth {
					info: Warmth::Hot(Paid::Read),
					blob: Warmth::cold_non_revertible(),
				},
			);
			assert_eq!(
				info_only.proof_size(),
				cold.proof_size(),
				"a cold blob prices cold even when info is hot: info_only={info_only:?} cold={cold:?}",
			);
		}
	}
}
