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
	exec::{ExecResult, Executable, ExportedFunction, Ext},
	gas::{GasMeter, Token},
	weights::WeightInfo,
	AccountIdOf, BalanceOf, CodeInfoOf, Config, Error, HoldReason, PristineCode, Weight,
	LOG_TARGET,
};
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{dispatch::DispatchResult, traits::fungible::MutateHold};
use sp_core::{Get, H256, U256};
use sp_runtime::DispatchError;

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

/// Bytecode information including type-specific ownership data
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq, scale_info::TypeInfo, MaxEncodedLen)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(T))]
pub enum BytecodeInfo<T: Config> {
	/// PVM bytecode with ownership and refcount tracking
	Pvm {
		/// The account that has uploaded the contract code and hence is allowed to remove it.
		owner: AccountIdOf<T>,
		/// The number of instantiated contracts that use this as their code.
		#[codec(compact)]
		refcount: u64,
	},
	/// EVM bytecode
	Evm,
}

/// Contract code related data.
///
/// It is stored in a separate storage entry to avoid loading the code when not necessary.
#[derive(Clone, Encode, Decode, scale_info::TypeInfo, MaxEncodedLen)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(T))]
pub struct CodeInfo<T: Config> {
	/// The amount of balance that was deposited by the owner in order to store it on-chain.
	#[codec(compact)]
	deposit: BalanceOf<T>,
	/// Length of the code in bytes.
	code_len: u32,
	/// Bytecode information (type + ownership data for PVM)
	bytecode_info: BytecodeInfo<T>,
	/// The behaviour version that this contract operates under.
	///
	/// Whenever any observeable change (with the exception of weights) are made we need
	/// to make sure that already deployed contracts will not be affected. We do this by
	/// exposing the old behaviour depending on the set behaviour version of the contract.
	///
	/// As of right now this is a reserved field that is always set to 0.
	behaviour_version: u32,
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

/// The bytecode type, either PVM or EVM
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone, Copy)]
pub enum BytecodeType {
	Pvm,
	Evm,
}

/// Cost of code loading from storage.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone, Copy)]
struct CodeLoadToken {
	code_len: u32,
	code_type: BytecodeType,
}

impl CodeLoadToken {
	fn from_code_info<T: Config>(code_info: &CodeInfo<T>) -> Self {
		let code_type = match &code_info.bytecode_info {
			BytecodeInfo::Pvm { .. } => BytecodeType::Pvm,
			BytecodeInfo::Evm => BytecodeType::Evm,
		};
		Self { code_len: code_info.code_len, code_type }
	}
}

impl<T: Config> Token<T> for CodeLoadToken {
	fn weight(&self) -> Weight {
		match self.code_type {
			// the proof size impact is accounted for in the `call_with_pvm_code_per_byte`
			// strictly speaking we are double charging for the first BASIC_BLOCK_SIZE
			// instructions here. Let's consider this as a safety margin.
			BytecodeType::Pvm => T::WeightInfo::call_with_pvm_code_per_byte(self.code_len)
				.saturating_sub(T::WeightInfo::call_with_pvm_code_per_byte(0))
				.saturating_add(
					T::WeightInfo::basic_block_compilation(1)
						.saturating_sub(T::WeightInfo::basic_block_compilation(0))
						.set_proof_size(0),
				),
			BytecodeType::Evm => T::WeightInfo::call_with_evm_code_per_byte(self.code_len)
				.saturating_sub(T::WeightInfo::call_with_evm_code_per_byte(0)),
		}
	}
}

#[cfg(test)]
pub fn code_load_weight(code_len: u32) -> Weight {
	Token::<crate::tests::Test>::weight(&CodeLoadToken { code_len, code_type: BytecodeType::Pvm })
}

impl<T: Config> ContractBlob<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
{
	/// Puts the module blob into storage, and returns the deposit collected for the storage.
	pub fn store_code(
		&mut self,
		owner: &AccountIdOf<T>,
		skip_transfer: bool,
	) -> Result<BalanceOf<T>, Error<T>> {
		let code_hash = *self.code_hash();
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

					if !skip_transfer {
						T::Currency::hold(
							&HoldReason::CodeUploadDepositReserve.into(),
							owner,
							deposit,
						).map_err(|err| {
							log::debug!(target: LOG_TARGET, "failed to hold store code deposit {deposit:?} for owner: {:?}: {err:?}", owner);
							<Error<T>>::StorageDepositNotEnoughFunds
						})?;
					}

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
			deposit: Default::default(),
			code_len: 0,
			bytecode_info: BytecodeInfo::Pvm { owner, refcount: 0 },
			behaviour_version: Default::default(),
		}
	}

	/// Returns reference count of the module (only for PVM).
	#[cfg(test)]
	pub fn refcount(&self) -> u64 {
		match &self.bytecode_info {
			BytecodeInfo::Pvm { refcount, .. } => *refcount,
			BytecodeInfo::Evm => 0,
		}
	}

	/// Returns the deposit of the module.
	pub fn deposit(&self) -> BalanceOf<T> {
		self.deposit
	}

	/// Returns the owner of the module (only for PVM).
	pub fn owner(&self) -> Option<&AccountIdOf<T>> {
		match &self.bytecode_info {
			BytecodeInfo::Pvm { owner, .. } => Some(owner),
			BytecodeInfo::Evm => None,
		}
	}

	/// Returns the code length.
	pub fn code_len(&self) -> u64 {
		self.code_len.into()
	}

	/// Returns true if the executable is a PVM blob.
	pub fn is_pvm(&self) -> bool {
		matches!(self.bytecode_info, BytecodeInfo::Pvm { .. })
	}

	/// Increment the reference count of a stored code by one (PVM only).
	///
	/// # Errors
	///
	/// [`Error::CodeNotFound`] is returned if no stored code found having the specified
	/// `code_hash`.
	pub fn increment_refcount(code_hash: H256) -> DispatchResult {
		<CodeInfoOf<T>>::mutate(code_hash, |existing| -> Result<(), DispatchError> {
			if let Some(info) = existing {
				match &mut info.bytecode_info {
					BytecodeInfo::Pvm { refcount, .. } => {
						*refcount = refcount
							.checked_add(1)
							.ok_or_else(|| <Error<T>>::RefcountOverOrUnderflow)?;
						Ok(())
					},
					BytecodeInfo::Evm => {
						// EVM contracts don't use refcounting, so this is a no-op
						Ok(())
					},
				}
			} else {
				Err(Error::<T>::CodeNotFound.into())
			}
		})
	}

	/// Decrement the reference count of a stored code by one (PVM only).
	///
	/// # Note
	///
	/// A contract whose reference count dropped to zero isn't automatically removed. A
	/// `remove_code` transaction must be submitted by the original uploader to do so.
	pub fn decrement_refcount(code_hash: H256) -> DispatchResult {
		<CodeInfoOf<T>>::mutate(code_hash, |existing| {
			if let Some(info) = existing {
				match &mut info.bytecode_info {
					BytecodeInfo::Pvm { refcount, .. } => {
						*refcount = refcount
							.checked_sub(1)
							.ok_or_else(|| <Error<T>>::RefcountOverOrUnderflow)?;
						Ok(())
					},
					BytecodeInfo::Evm => {
						// EVM contracts don't use refcounting, so this is a no-op
						Ok(())
					},
				}
			} else {
				Err(Error::<T>::CodeNotFound.into())
			}
		})
	}
}

impl<T: Config> Executable<T> for ContractBlob<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
{
	fn from_storage(code_hash: H256, gas_meter: &mut GasMeter<T>) -> Result<Self, DispatchError> {
		let code_info = <CodeInfoOf<T>>::get(code_hash).ok_or(Error::<T>::CodeNotFound)?;
		gas_meter.charge(CodeLoadToken::from_code_info(&code_info))?;
		let code = <PristineCode<T>>::get(&code_hash).ok_or(Error::<T>::CodeNotFound)?;
		Ok(Self { code, code_info, code_hash })
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
			use crate::vm::evm::EVMInputs;
			use revm::bytecode::Bytecode;
			let inputs = EVMInputs::new(input_data);
			let bytecode = Bytecode::new_raw(self.code.into());
			evm::call(bytecode, ext, inputs)
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
