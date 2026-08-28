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

pub use runtime_costs::{RuntimeCosts, StorageAccessKind};

use crate::{
	AccountIdOf, BalanceOf, CodeInfoOf, CodeRemoved, Config, Error, ExecConfig, ExecError,
	HoldReason, LOG_TARGET, Pallet, PristineCode, StorageDeposit, Weight,
	access_list::{Access, CodeLoad, CodeLoadWarmth},
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

/// The two charges of a code load, one per read: the trie path both reads share, then the code's
/// bytes, whose length only the first read reveals.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone, Copy)]
enum CodeLoadToken {
	Flat(CodeLoadWarmth),
	Blob { warmth: CodeLoadWarmth, code_len: u32, code_type: BytecodeType },
}

impl CodeLoadToken {
	/// Both reads, at the warmth of the code's own keys, never the calling frame's.
	fn flat<T: Config>(warmth: CodeLoadWarmth) -> Weight {
		runtime_costs::weight_by_warmth::<T, _>(
			[warmth.info, warmth.blob],
			CodeLoad::KEY_FAMILY,
			T::WeightInfo::code_load,
			// A hot read pays only the overlay lookup `weight_by_warmth` adds per item.
			Weight::zero,
		)
	}

	/// The code's bytes at the blob's warmth, plus PVM compilation.
	fn blob<T: Config>(warmth: CodeLoadWarmth, code_len: u32, code_type: BytecodeType) -> Weight {
		let per_byte: fn(u32) -> Weight = match (code_type, warmth.blob.is_hot()) {
			(BytecodeType::Pvm, false) => T::WeightInfo::call_with_pvm_code_per_byte,
			(BytecodeType::Pvm, true) => T::WeightInfo::call_with_pvm_code_per_byte_hot,
			(BytecodeType::Evm, false) => T::WeightInfo::call_with_evm_code_per_byte,
			(BytecodeType::Evm, true) => T::WeightInfo::call_with_evm_code_per_byte_hot,
		};
		let bytes_weight = per_byte(code_len).saturating_sub(per_byte(0));
		let compilation_weight = match code_type {
			// The proof size impact is accounted for in `call_with_pvm_code_per_byte`, so the
			// compilation term drops its proof. It double-charges the first BASIC_BLOCK_SIZE
			// instructions; we keep that as a safety margin.
			BytecodeType::Pvm => T::WeightInfo::basic_block_compilation(1)
				.saturating_sub(T::WeightInfo::basic_block_compilation(0))
				.set_proof_size(0),
			BytecodeType::Evm => Weight::zero(),
		};
		bytes_weight.saturating_add(compilation_weight)
	}
}

impl<T: Config> Token<T> for CodeLoadToken {
	fn weight(&self) -> Weight {
		match *self {
			Self::Flat(warmth) => Self::flat::<T>(warmth),
			Self::Blob { warmth, code_len, code_type } => {
				Self::blob::<T>(warmth, code_len, code_type)
			},
		}
	}
}

/// The weight a load of `code_len` bytes is charged.
#[cfg(test)]
pub fn code_load_weight(code_len: u32, code_type: BytecodeType, warmth: CodeLoadWarmth) -> Weight {
	let flat = Token::<crate::tests::Test>::weight(&CodeLoadToken::Flat(warmth));
	let blob =
		Token::<crate::tests::Test>::weight(&CodeLoadToken::Blob { warmth, code_len, code_type });
	flat.saturating_add(blob)
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
	) -> Result<Self, DispatchError> {
		meter.charge_weight_token(CodeLoadToken::Flat(warmth))?;
		let code_info = <CodeInfoOf<T>>::get(code_hash).ok_or(Error::<T>::CodeNotFound)?;
		meter.charge_weight_token(CodeLoadToken::Blob {
			warmth,
			code_len: code_info.code_len,
			code_type: code_info.code_type,
		})?;
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
		access_list::{StorageOp, Warmth},
		metering::TransactionMeter,
		test_utils::ALICE,
		tests::{ExtBuilder, Test},
	};

	fn warmth(blob: Warmth) -> CodeLoadWarmth {
		CodeLoadWarmth { info: blob, blob }
	}

	#[test]
	fn code_load_cold_hot_pricing() {
		let code_len = 1024_u32;
		for code_type in [BytecodeType::Pvm, BytecodeType::Evm] {
			let cold = code_load_weight(code_len, code_type, warmth(Warmth::cold_non_revertible()));
			let hot = code_load_weight(
				code_len,
				code_type,
				warmth(Warmth::Hot { charged: StorageOp::Read }),
			);
			assert!(
				cold.ref_time() > hot.ref_time(),
				"expected cold > hot ref_time for {code_type:?}: cold={cold:?} hot={hot:?}",
			);
			assert_eq!(hot.proof_size(), 0, "a hot blob is already in the proof: {hot:?}");
			let both_reads_proof = <Test as Config>::WeightInfo::code_load().proof_size();
			assert!(
				cold.proof_size() >= both_reads_proof + u64::from(code_len),
				"a cold load proves both reads and the blob's {code_len} bytes: {cold:?}",
			);
			let hot_info_cold_blob = code_load_weight(
				code_len,
				code_type,
				CodeLoadWarmth {
					info: Warmth::Hot { charged: StorageOp::Read },
					blob: Warmth::cold_non_revertible(),
				},
			);
			assert!(
				hot_info_cold_blob.proof_size() >= both_reads_proof + u64::from(code_len),
				"a cold blob walks its trie path even when the info is hot: \
				 {hot_info_cold_blob:?}",
			);

			let twice_as_long =
				code_load_weight(code_len * 2, code_type, warmth(Warmth::cold_non_revertible()));
			assert!(
				twice_as_long.proof_size().saturating_sub(cold.proof_size()) >= u64::from(code_len),
				"doubling the code adds at least {code_len} bytes of proof: \
				 twice={twice_as_long:?} cold={cold:?}",
			);

			// The bytes alone, with the entries' touches subtracted out.
			let bytes_of = |load: CodeLoadWarmth| {
				code_load_weight(code_len, code_type, load)
					.saturating_sub(code_load_weight(0, code_type, load))
			};
			let hot_blob = Warmth::Hot { charged: StorageOp::Read };
			assert_eq!(
				bytes_of(CodeLoadWarmth { info: Warmth::cold_non_revertible(), blob: hot_blob }),
				bytes_of(warmth(hot_blob)),
				"the metadata's warmth does not price the bytes: only the blob is read by length",
			);
			assert!(
				bytes_of(warmth(hot_blob)).ref_time() > 0,
				"a hot blob still pays for its bytes: {code_type:?}",
			);
			let empty_blob = Token::<Test>::weight(&CodeLoadToken::Blob {
				warmth: warmth(hot_blob),
				code_len: 0,
				code_type,
			});
			assert_eq!(
				empty_blob.proof_size(),
				0,
				"the two reads' proof lives in `Flat`, not in `Blob`: {code_type:?}",
			);
			if matches!(code_type, BytecodeType::Evm) {
				assert_eq!(
					empty_blob,
					Weight::zero(),
					"the trie path and the entries' touches live in `Flat`, not in `Blob`",
				);
			}
		}

		// One rollback prepay per entry.
		let rollback = <Test as Config>::WeightInfo::access_list_rollback_amortization();
		let empty_blob = |load| code_load_weight(0, BytecodeType::Evm, load);
		assert_eq!(
			empty_blob(warmth(Warmth::cold_revertible()))
				.saturating_sub(empty_blob(warmth(Warmth::cold_non_revertible()))),
			rollback.saturating_mul(2),
			"one rollback prepay per code entry",
		);
	}

	#[test]
	fn a_load_charges_each_read_before_making_it() {
		ExtBuilder::default().build().execute_with(|| {
			let code = vec![0u8; 1024];
			let code_len = code.len() as u32;
			let mut code_info = CodeInfo::<Test>::new(ALICE);
			code_info.code_len = code_len;
			let stored = H256::repeat_byte(1);
			<CodeInfoOf<Test>>::insert(stored, code_info.clone());
			<PristineCode<Test>>::insert(stored, code.clone());
			// Metadata without a blob: a load that read the blob before paying for it would fail
			// with `CodeNotFound` instead of running out of weight.
			let blob_missing = H256::repeat_byte(2);
			<CodeInfoOf<Test>>::insert(blob_missing, code_info);

			let cold = warmth(Warmth::cold_non_revertible());
			let new_meter = |weight_limit| {
				TransactionMeter::<Test>::new_from_limits(weight_limit, u128::MAX)
					.expect("a weight-and-deposit meter always builds")
			};

			let mut meter = new_meter(Weight::MAX);
			let loaded = ContractBlob::<Test>::from_storage(stored, &mut meter, cold)
				.expect("the load fits");
			assert_eq!(loaded.code(), &code[..], "the blob read back is the stored one");
			assert_eq!(
				meter.weight_consumed(),
				code_load_weight(code_len, BytecodeType::Pvm, cold),
				"a load consumes exactly the blob's weight",
			);

			let flat = Token::<Test>::weight(&CodeLoadToken::Flat(cold));
			let mut meter = new_meter(flat);
			assert_eq!(
				ContractBlob::<Test>::from_storage(blob_missing, &mut meter, cold).map(|_| ()),
				Err(Error::<Test>::OutOfGas.into()),
				"with weight for the two reads only, the blob's charge fails before the blob is read",
			);
			assert_eq!(meter.weight_consumed(), flat, "only the first charge landed");

			let mut meter = new_meter(Weight::zero());
			assert_eq!(
				ContractBlob::<Test>::from_storage(stored, &mut meter, cold).map(|_| ()),
				Err(Error::<Test>::OutOfGas.into()),
				"with no weight at all, the load pays for its reads before making the first",
			);
		});
	}
}
