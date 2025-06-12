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

mod runtime;

#[cfg(doc)]
pub use crate::vm::runtime::SyscallDoc;

#[cfg(feature = "runtime-benchmarks")]
pub use crate::vm::runtime::{ReturnData, TrapReason};

pub use crate::vm::runtime::{Runtime, RuntimeCosts};

use crate::{
	exec::{ExecResult, Executable, ExportedFunction, Ext},
	gas::{GasMeter, Token},
	limits,
	storage::meter::Diff,
	weights::WeightInfo,
	AccountIdOf, BadOrigin, BalanceOf, CodeInfoOf, CodeVec, Config, Error, ExecError, HoldReason,
	PristineCode, Weight, LOG_TARGET,
};
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::DispatchResult,
	ensure,
	traits::{fungible::MutateHold, tokens::Precision::BestEffort},
};
use sp_core::{Get, H256, U256};
use sp_runtime::DispatchError;

/// Validated Vm module ready for execution.
/// This data structure is immutable once created and stored.
#[derive(Encode, Decode, scale_info::TypeInfo)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(T))]
pub struct ContractBlob<T: Config> {
	code: CodeVec,
	// This isn't needed for contract execution and is not stored alongside it.
	#[codec(skip)]
	code_info: CodeInfo<T>,
	// This is for not calculating the hash every time we need it.
	#[codec(skip)]
	code_hash: H256,
}

/// Contract code related data, such as:
///
/// - owner of the contract, i.e. account uploaded its code,
/// - storage deposit amount,
/// - reference count,
///
/// It is stored in a separate storage entry to avoid loading the code when not necessary.
#[derive(Clone, Encode, Decode, scale_info::TypeInfo, MaxEncodedLen)]
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

/// Cost of code loading from storage.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone, Copy)]
struct CodeLoadToken(u32);

impl<T: Config> Token<T> for CodeLoadToken {
	fn weight(&self) -> Weight {
		// the proof size impact is accounted for in the `call_with_code_per_byte`
		// strictly speaking we are double charging for the first BASIC_BLOCK_SIZE
		// instructions here. Let's consider this as a safety margin.
		T::WeightInfo::call_with_code_per_byte(self.0)
			.saturating_sub(T::WeightInfo::call_with_code_per_byte(0))
			.saturating_add(
				T::WeightInfo::basic_block_compilation(1)
					.saturating_sub(T::WeightInfo::basic_block_compilation(0))
					.set_proof_size(0),
			)
	}
}

#[cfg(test)]
pub fn code_load_weight(code_len: u32) -> Weight {
	Token::<crate::tests::Test>::weight(&CodeLoadToken(code_len))
}

impl<T: Config> ContractBlob<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
{
	/// We only check for size and nothing else when the code is uploaded.
	pub fn from_code(code: Vec<u8>, owner: AccountIdOf<T>) -> Result<Self, DispatchError> {
		// We do validation only when new code is deployed. This allows us to increase
		// the limits later without affecting already deployed code.
		let available_syscalls = runtime::list_syscalls(T::UnsafeUnstableInterface::get());
		let code = limits::code::enforce::<T>(code, available_syscalls)?;

		let code_len = code.len() as u32;
		let bytes_added = code_len.saturating_add(<CodeInfo<T>>::max_encoded_len() as u32);
		let deposit = Diff { bytes_added, items_added: 2, ..Default::default() }
			.update_contract::<T>(None)
			.charge_or_zero();
		let code_info = CodeInfo {
			owner,
			deposit,
			refcount: 0,
			code_len,
			behaviour_version: Default::default(),
		};
		let code_hash = H256(sp_io::hashing::keccak_256(&code));
		Ok(ContractBlob { code, code_info, code_hash })
	}

	/// Remove the code from storage and refund the deposit to its owner.
	///
	/// Applies all necessary checks before removing the code.
	pub fn remove(origin: &T::AccountId, code_hash: H256) -> DispatchResult {
		<CodeInfoOf<T>>::try_mutate_exists(&code_hash, |existing| {
			if let Some(code_info) = existing {
				ensure!(code_info.refcount == 0, <Error<T>>::CodeInUse);
				ensure!(&code_info.owner == origin, BadOrigin);
				let _ = T::Currency::release(
					&HoldReason::CodeUploadDepositReserve.into(),
					&code_info.owner,
					code_info.deposit,
					BestEffort,
				);

				*existing = None;
				<PristineCode<T>>::remove(&code_hash);
				Ok(())
			} else {
				Err(<Error<T>>::CodeNotFound.into())
			}
		})
	}

	/// Puts the module blob into storage, and returns the deposit collected for the storage.
	pub fn store_code(&mut self, skip_transfer: bool) -> Result<BalanceOf<T>, Error<T>> {
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
						&self.code_info.owner,
						deposit,
					) .map_err(|err| {
							log::debug!(target: LOG_TARGET, "failed to hold store code deposit {deposit:?} for owner: {:?}: {err:?}", self.code_info.owner);
							<Error<T>>::StorageDepositNotEnoughFunds
					})?;
					}

					self.code_info.refcount = 0;
					<PristineCode<T>>::insert(code_hash, &self.code);
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

	/// Returns the code length.
	pub fn code_len(&self) -> u64 {
		self.code_len.into()
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
	///
	/// # Note
	///
	/// A contract whose reference count dropped to zero isn't automatically removed. A
	/// `remove_code` transaction must be submitted by the original uploader to do so.
	pub fn decrement_refcount(code_hash: H256) -> DispatchResult {
		<CodeInfoOf<T>>::mutate(code_hash, |existing| {
			if let Some(info) = existing {
				info.refcount = info
					.refcount
					.checked_sub(1)
					.ok_or_else(|| <Error<T>>::RefcountOverOrUnderflow)?;
				Ok(())
			} else {
				Err(Error::<T>::CodeNotFound.into())
			}
		})
	}
}

pub struct PreparedCall<'a, E: Ext> {
	module: polkavm::Module,
	instance: polkavm::RawInstance,
	runtime: Runtime<'a, E, polkavm::RawInstance>,
}

impl<'a, E: Ext> PreparedCall<'a, E>
where
	BalanceOf<E::T>: Into<U256>,
	BalanceOf<E::T>: TryFrom<U256>,
{
	pub fn call(mut self) -> ExecResult {
		let exec_result = loop {
			let interrupt = self.instance.run();
			if let Some(exec_result) =
				self.runtime.handle_interrupt(interrupt, &self.module, &mut self.instance)
			{
				break exec_result
			}
		};
		let _ = self.runtime.ext().gas_meter_mut().sync_from_executor(self.instance.gas())?;
		exec_result
	}

	/// The guest memory address at which the aux data is located.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn aux_data_base(&self) -> u32 {
		self.instance.module().memory_map().aux_data_address()
	}

	/// Copies `data` to the aux data at address `offset`.
	///
	/// It sets `a0` to the beginning of data inside the aux data.
	/// It sets `a1` to the value passed.
	///
	/// Only used in benchmarking so far.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn setup_aux_data(&mut self, data: &[u8], offset: u32, a1: u64) -> DispatchResult {
		let a0 = self.aux_data_base().saturating_add(offset);
		self.instance.write_memory(a0, data).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to write aux data: {err:?}");
			Error::<E::T>::CodeRejected
		})?;
		self.instance.set_reg(polkavm::Reg::A0, a0.into());
		self.instance.set_reg(polkavm::Reg::A1, a1);
		Ok(())
	}
}

impl<T: Config> ContractBlob<T> {
	/// Compile and instantiate contract.
	///
	/// `aux_data_size` is only used for runtime benchmarks. Real contracts
	/// don't make use of this buffer. Hence this should not be set to anything
	/// other than `0` when not used for benchmarking.
	pub fn prepare_call<E: Ext<T = T>>(
		self,
		mut runtime: Runtime<E, polkavm::RawInstance>,
		entry_point: ExportedFunction,
		aux_data_size: u32,
	) -> Result<PreparedCall<E>, ExecError> {
		let mut config = polkavm::Config::default();
		config.set_backend(Some(polkavm::BackendKind::Interpreter));
		config.set_cache_enabled(false);
		#[cfg(feature = "std")]
		if std::env::var_os("REVIVE_USE_COMPILER").is_some() {
			log::warn!(target: LOG_TARGET, "Using PolkaVM compiler backend because env var REVIVE_USE_COMPILER is set");
			config.set_backend(Some(polkavm::BackendKind::Compiler));
		}
		let engine = polkavm::Engine::new(&config).expect(
			"on-chain (no_std) use of interpreter is hard coded.
				interpreter is available on all platforms; qed",
		);

		let mut module_config = polkavm::ModuleConfig::new();
		module_config.set_page_size(limits::PAGE_SIZE);
		module_config.set_gas_metering(Some(polkavm::GasMeteringKind::Sync));
		module_config.set_allow_sbrk(false);
		module_config.set_aux_data_size(aux_data_size);
		let module = polkavm::Module::new(&engine, &module_config, self.code.into_inner().into())
			.map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to create polkavm module: {err:?}");
			Error::<T>::CodeRejected
		})?;

		let entry_program_counter = module
			.exports()
			.find(|export| export.symbol().as_bytes() == entry_point.identifier().as_bytes())
			.ok_or_else(|| <Error<T>>::CodeRejected)?
			.program_counter();

		let gas_limit_polkavm: polkavm::Gas = runtime.ext().gas_meter_mut().engine_fuel_left()?;

		let mut instance = module.instantiate().map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to instantiate polkavm module: {err:?}");
			Error::<T>::CodeRejected
		})?;

		instance.set_gas(gas_limit_polkavm);
		instance.prepare_call_untyped(entry_program_counter, &[]);

		Ok(PreparedCall { module, instance, runtime })
	}
}

impl<T: Config> Executable<T> for ContractBlob<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
{
	fn from_storage(code_hash: H256, gas_meter: &mut GasMeter<T>) -> Result<Self, DispatchError> {
		let code_info = <CodeInfoOf<T>>::get(code_hash).ok_or(Error::<T>::CodeNotFound)?;
		gas_meter.charge(CodeLoadToken(code_info.code_len))?;
		let code = <PristineCode<T>>::get(&code_hash).ok_or(Error::<T>::CodeNotFound)?;
		Ok(Self { code, code_info, code_hash })
	}

	fn execute<E: Ext<T = T>>(
		self,
		ext: &mut E,
		function: ExportedFunction,
		input_data: Vec<u8>,
	) -> ExecResult {
		let prepared_call = self.prepare_call(Runtime::new(ext, input_data), function, 0)?;
		prepared_call.call()
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
