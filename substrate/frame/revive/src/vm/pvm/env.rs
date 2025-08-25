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

use super::*;

use crate::{
	address::AddressMapper,
	exec::Ext,
	limits,
	primitives::ExecReturnValue,
	storage::meter::Diff,
	vm::{ExportedFunction, RuntimeCosts},
	AccountIdOf, BalanceOf, CodeInfo, Config, ContractBlob, Error, Weight, SENTINEL,
};
use alloc::vec::Vec;
use codec::{Encode, MaxEncodedLen};
use core::mem;
use frame_support::traits::Get;
use pallet_revive_proc_macro::define_env;
use pallet_revive_uapi::{CallFlags, ReturnErrorCode, ReturnFlags};
use sp_core::{H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::DispatchError;

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
		instance
			.set_interpreter_cache_size_limit(Some(polkavm::SetCacheSizeLimitArgs {
				max_block_size: limits::code::BASIC_BLOCK_SIZE,
				max_cache_size_bytes: limits::code::INTERPRETER_CACHE_BYTES
					.try_into()
					.map_err(|_| Error::<T>::CodeRejected)?,
			}))
			.map_err(|_| Error::<T>::CodeRejected)?;
		instance.prepare_call_untyped(entry_program_counter, &[]);

		Ok(PreparedCall { module, instance, runtime })
	}
}

impl<T: Config> ContractBlob<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
{
	/// We only check for size and nothing else when the code is uploaded.
	pub fn from_pvm_code(code: Vec<u8>, owner: AccountIdOf<T>) -> Result<Self, DispatchError> {
		// We do validation only when new code is deployed. This allows us to increase
		// the limits later without affecting already deployed code.
		let available_syscalls = list_syscalls(T::UnsafeUnstableInterface::get());
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
}

impl<'a, E: Ext, M: PolkaVmInstance<E::T>> Runtime<'a, E, M> {
	pub fn handle_interrupt(
		&mut self,
		interrupt: Result<polkavm::InterruptKind, polkavm::Error>,
		module: &polkavm::Module,
		instance: &mut M,
	) -> Option<ExecResult> {
		use polkavm::InterruptKind::*;

		match interrupt {
			Err(error) => {
				// in contrast to the other returns this "should" not happen: log level error
				log::error!(target: LOG_TARGET, "polkavm execution error: {error}");
				Some(Err(Error::<E::T>::ExecutionFailed.into()))
			},
			Ok(Finished) =>
				Some(Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })),
			Ok(Trap) => Some(Err(Error::<E::T>::ContractTrapped.into())),
			Ok(Segfault(_)) => Some(Err(Error::<E::T>::ExecutionFailed.into())),
			Ok(NotEnoughGas) => Some(Err(Error::<E::T>::OutOfGas.into())),
			Ok(Step) => None,
			Ok(Ecalli(idx)) => {
				// This is a special hard coded syscall index which is used by benchmarks
				// to abort contract execution. It is used to terminate the execution without
				// breaking up a basic block. The fixed index is used so that the benchmarks
				// don't have to deal with import tables.
				if cfg!(feature = "runtime-benchmarks") && idx == SENTINEL {
					return Some(Ok(ExecReturnValue {
						flags: ReturnFlags::empty(),
						data: Vec::new(),
					}))
				}
				let Some(syscall_symbol) = module.imports().get(idx) else {
					return Some(Err(<Error<E::T>>::InvalidSyscall.into()));
				};
				match self.handle_ecall(instance, syscall_symbol.as_bytes()) {
					Ok(None) => None,
					Ok(Some(return_value)) => {
						instance.write_output(return_value);
						None
					},
					Err(TrapReason::Return(ReturnData { flags, data })) =>
						match ReturnFlags::from_bits(flags) {
							None => Some(Err(Error::<E::T>::InvalidCallFlags.into())),
							Some(flags) => Some(Ok(ExecReturnValue { flags, data })),
						},
					Err(TrapReason::Termination) => Some(Ok(Default::default())),
					Err(TrapReason::SupervisorError(error)) => Some(Err(error.into())),
				}
			},
		}
	}
}

// This is the API exposed to contracts.
//
// # Note
//
// Any input that leads to a out of bound error (reading or writing) or failing to decode
// data passed to the supervisor will lead to a trap. This is not documented explicitly
// for every function.
#[define_env]
pub mod env {
	/// Noop function used to benchmark the time it takes to execute an empty function.
	///
	/// Marked as stable because it needs to be called from benchmarks even when the benchmarked
	/// parachain has unstable functions disabled.
	#[cfg(feature = "runtime-benchmarks")]
	#[stable]
	fn noop(&mut self, memory: &mut M) -> Result<(), TrapReason> {
		Ok(())
	}

	/// Set the value at the given key in the contract storage.
	/// See [`pallet_revive_uapi::HostFn::set_storage_v2`]
	#[stable]
	#[mutating]
	fn set_storage(
		&mut self,
		memory: &mut M,
		flags: u32,
		key_ptr: u32,
		key_len: u32,
		value_ptr: u32,
		value_len: u32,
	) -> Result<u32, TrapReason> {
		self.set_storage(
			memory,
			flags,
			key_ptr,
			key_len,
			StorageValue::Memory { ptr: value_ptr, len: value_len },
		)
	}

	/// Sets the storage at a fixed 256-bit key with a fixed 256-bit value.
	/// See [`pallet_revive_uapi::HostFn::set_storage_or_clear`].
	#[stable]
	#[mutating]
	fn set_storage_or_clear(
		&mut self,
		memory: &mut M,
		flags: u32,
		key_ptr: u32,
		value_ptr: u32,
	) -> Result<u32, TrapReason> {
		let value = memory.read(value_ptr, 32)?;

		if value.iter().all(|&b| b == 0) {
			self.clear_storage(memory, flags, key_ptr, SENTINEL)
		} else {
			self.set_storage(memory, flags, key_ptr, SENTINEL, StorageValue::Value(value))
		}
	}

	/// Retrieve the value under the given key from storage.
	/// See [`pallet_revive_uapi::HostFn::get_storage`]
	#[stable]
	fn get_storage(
		&mut self,
		memory: &mut M,
		flags: u32,
		key_ptr: u32,
		key_len: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		self.get_storage(
			memory,
			flags,
			key_ptr,
			key_len,
			out_ptr,
			StorageReadMode::VariableOutput { output_len_ptr: out_len_ptr },
		)
	}

	/// Reads the storage at a fixed 256-bit key and writes back a fixed 256-bit value.
	/// See [`pallet_revive_uapi::HostFn::get_storage_or_zero`].
	#[stable]
	fn get_storage_or_zero(
		&mut self,
		memory: &mut M,
		flags: u32,
		key_ptr: u32,
		out_ptr: u32,
	) -> Result<(), TrapReason> {
		let _ = self.get_storage(
			memory,
			flags,
			key_ptr,
			SENTINEL,
			out_ptr,
			StorageReadMode::FixedOutput32,
		)?;

		Ok(())
	}

	/// Make a call to another contract.
	/// See [`pallet_revive_uapi::HostFn::call`].
	#[stable]
	fn call(
		&mut self,
		memory: &mut M,
		flags_and_callee: u64,
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit_and_value: u64,
		input_data: u64,
		output_data: u64,
	) -> Result<ReturnErrorCode, TrapReason> {
		let (flags, callee_ptr) = extract_hi_lo(flags_and_callee);
		let (deposit_ptr, value_ptr) = extract_hi_lo(deposit_and_value);
		let (input_data_len, input_data_ptr) = extract_hi_lo(input_data);
		let (output_len_ptr, output_ptr) = extract_hi_lo(output_data);

		self.call(
			memory,
			CallFlags::from_bits(flags).ok_or(Error::<E::T>::InvalidCallFlags)?,
			CallType::Call { value_ptr },
			callee_ptr,
			deposit_ptr,
			Weight::from_parts(ref_time_limit, proof_size_limit),
			input_data_ptr,
			input_data_len,
			output_ptr,
			output_len_ptr,
		)
	}

	/// Execute code in the context (storage, caller, value) of the current contract.
	/// See [`pallet_revive_uapi::HostFn::delegate_call`].
	#[stable]
	fn delegate_call(
		&mut self,
		memory: &mut M,
		flags_and_callee: u64,
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit_ptr: u32,
		input_data: u64,
		output_data: u64,
	) -> Result<ReturnErrorCode, TrapReason> {
		let (flags, address_ptr) = extract_hi_lo(flags_and_callee);
		let (input_data_len, input_data_ptr) = extract_hi_lo(input_data);
		let (output_len_ptr, output_ptr) = extract_hi_lo(output_data);

		self.call(
			memory,
			CallFlags::from_bits(flags).ok_or(Error::<E::T>::InvalidCallFlags)?,
			CallType::DelegateCall,
			address_ptr,
			deposit_ptr,
			Weight::from_parts(ref_time_limit, proof_size_limit),
			input_data_ptr,
			input_data_len,
			output_ptr,
			output_len_ptr,
		)
	}

	/// Instantiate a contract with the specified code hash.
	/// See [`pallet_revive_uapi::HostFn::instantiate`].
	#[stable]
	#[mutating]
	fn instantiate(
		&mut self,
		memory: &mut M,
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit_and_value: u64,
		input_data: u64,
		output_data: u64,
		address_and_salt: u64,
	) -> Result<ReturnErrorCode, TrapReason> {
		let (deposit_ptr, value_ptr) = extract_hi_lo(deposit_and_value);
		let (input_data_len, code_hash_ptr) = extract_hi_lo(input_data);
		let (output_len_ptr, output_ptr) = extract_hi_lo(output_data);
		let (address_ptr, salt_ptr) = extract_hi_lo(address_and_salt);
		let Some(input_data_ptr) = code_hash_ptr.checked_add(32) else {
			return Err(Error::<E::T>::OutOfBounds.into());
		};
		let Some(input_data_len) = input_data_len.checked_sub(32) else {
			return Err(Error::<E::T>::OutOfBounds.into());
		};

		self.instantiate(
			memory,
			code_hash_ptr,
			Weight::from_parts(ref_time_limit, proof_size_limit),
			deposit_ptr,
			value_ptr,
			input_data_ptr,
			input_data_len,
			address_ptr,
			output_ptr,
			output_len_ptr,
			salt_ptr,
		)
	}

	/// Returns the total size of the contract call input data.
	/// See [`pallet_revive_uapi::HostFn::call_data_size `].
	#[stable]
	fn call_data_size(&mut self, memory: &mut M) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::CallDataSize)?;
		Ok(self
			.input_data
			.as_ref()
			.map(|input| input.len().try_into().expect("usize fits into u64; qed"))
			.unwrap_or_default())
	}

	/// Stores the input passed by the caller into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::call_data_copy`].
	#[stable]
	fn call_data_copy(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		out_len: u32,
		offset: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::CallDataCopy(out_len))?;

		let Some(input) = self.input_data.as_ref() else {
			return Err(Error::<E::T>::InputForwarded.into());
		};

		let start = offset as usize;
		if start >= input.len() {
			memory.zero(out_ptr, out_len)?;
			return Ok(());
		}

		let end = start.saturating_add(out_len as usize).min(input.len());
		memory.write(out_ptr, &input[start..end])?;

		let bytes_written = (end - start) as u32;
		memory.zero(out_ptr.saturating_add(bytes_written), out_len - bytes_written)?;

		Ok(())
	}

	/// Stores the U256 value at given call input `offset` into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::call_data_load`].
	#[stable]
	fn call_data_load(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		offset: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::CallDataLoad)?;

		let Some(input) = self.input_data.as_ref() else {
			return Err(Error::<E::T>::InputForwarded.into());
		};

		let mut data = [0; 32];
		let start = offset as usize;
		let data = if start >= input.len() {
			data // Any index is valid to request; OOB offsets return zero.
		} else {
			let end = start.saturating_add(32).min(input.len());
			data[..end - start].copy_from_slice(&input[start..end]);
			data.reverse();
			data // Solidity expects right-padded data
		};

		self.write_fixed_sandbox_output(memory, out_ptr, &data, false, already_charged)?;

		Ok(())
	}

	/// Cease contract execution and save a data buffer as a result of the execution.
	/// See [`pallet_revive_uapi::HostFn::return_value`].
	#[stable]
	fn seal_return(
		&mut self,
		memory: &mut M,
		flags: u32,
		data_ptr: u32,
		data_len: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::CopyFromContract(data_len))?;
		if data_len > limits::CALLDATA_BYTES {
			Err(<Error<E::T>>::ReturnDataTooLarge)?;
		}
		Err(TrapReason::Return(ReturnData { flags, data: memory.read(data_ptr, data_len)? }))
	}

	/// Stores the address of the caller into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::caller`].
	#[stable]
	fn caller(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::Caller)?;
		let caller = <E::T as Config>::AddressMapper::to_address(self.ext.caller().account_id()?);
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			caller.as_bytes(),
			false,
			already_charged,
		)?)
	}

	/// Stores the address of the call stack origin into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::origin`].
	#[stable]
	fn origin(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::Origin)?;
		let origin = <E::T as Config>::AddressMapper::to_address(self.ext.origin().account_id()?);
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			origin.as_bytes(),
			false,
			already_charged,
		)?)
	}

	/// Retrieve the code hash for a specified contract address.
	/// See [`pallet_revive_uapi::HostFn::code_hash`].
	#[stable]
	fn code_hash(&mut self, memory: &mut M, addr_ptr: u32, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::CodeHash)?;
		let address = memory.read_h160(addr_ptr)?;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&self.ext.code_hash(&address).as_bytes(),
			false,
			already_charged,
		)?)
	}

	/// Retrieve the code size for a given contract address.
	/// See [`pallet_revive_uapi::HostFn::code_size`].
	#[stable]
	fn code_size(&mut self, memory: &mut M, addr_ptr: u32) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::CodeSize)?;
		let address = memory.read_h160(addr_ptr)?;
		Ok(self.ext.code_size(&address))
	}

	/// Stores the address of the current contract into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::address`].
	#[stable]
	fn address(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::Address)?;
		let address = self.ext.address();
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			address.as_bytes(),
			false,
			already_charged,
		)?)
	}

	/// Stores the price for the specified amount of weight into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::weight_to_fee`].
	#[stable]
	fn weight_to_fee(
		&mut self,
		memory: &mut M,
		ref_time_limit: u64,
		proof_size_limit: u64,
		out_ptr: u32,
	) -> Result<(), TrapReason> {
		let weight = Weight::from_parts(ref_time_limit, proof_size_limit);
		self.charge_gas(RuntimeCosts::WeightToFee)?;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&self.ext.get_weight_price(weight).encode(),
			false,
			already_charged,
		)?)
	}

	/// Stores the immutable data into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::get_immutable_data`].
	#[stable]
	fn get_immutable_data(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<(), TrapReason> {
		// quering the length is free as it is stored with the contract metadata
		let len = self.ext.immutable_data_len();
		self.charge_gas(RuntimeCosts::GetImmutableData(len))?;
		let data = self.ext.get_immutable_data()?;
		self.write_sandbox_output(memory, out_ptr, out_len_ptr, &data, false, already_charged)?;
		Ok(())
	}

	/// Attaches the supplied immutable data to the currently executing contract.
	/// See [`pallet_revive_uapi::HostFn::set_immutable_data`].
	#[stable]
	fn set_immutable_data(&mut self, memory: &mut M, ptr: u32, len: u32) -> Result<(), TrapReason> {
		if len > limits::IMMUTABLE_BYTES {
			return Err(Error::<E::T>::OutOfBounds.into());
		}
		self.charge_gas(RuntimeCosts::SetImmutableData(len))?;
		let buf = memory.read(ptr, len)?;
		let data = buf.try_into().expect("bailed out earlier; qed");
		self.ext.set_immutable_data(data)?;
		Ok(())
	}

	/// Stores the *free* balance of the current account into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::balance`].
	#[stable]
	fn balance(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::Balance)?;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&self.ext.balance().to_little_endian(),
			false,
			already_charged,
		)?)
	}

	/// Stores the *free* balance of the supplied address into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::balance`].
	#[stable]
	fn balance_of(
		&mut self,
		memory: &mut M,
		addr_ptr: u32,
		out_ptr: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::BalanceOf)?;
		let address = memory.read_h160(addr_ptr)?;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&self.ext.balance_of(&address).to_little_endian(),
			false,
			already_charged,
		)?)
	}

	/// Returns the chain ID.
	/// See [`pallet_revive_uapi::HostFn::chain_id`].
	#[stable]
	fn chain_id(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&U256::from(<E::T as Config>::ChainId::get()).to_little_endian(),
			false,
			|_| Some(RuntimeCosts::CopyToContract(32)),
		)?)
	}

	/// Returns the block ref_time limit.
	/// See [`pallet_revive_uapi::HostFn::gas_limit`].
	#[stable]
	fn gas_limit(&mut self, memory: &mut M) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::GasLimit)?;
		Ok(<E::T as frame_system::Config>::BlockWeights::get().max_block.ref_time())
	}

	/// Stores the value transferred along with this call/instantiate into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::value_transferred`].
	#[stable]
	fn value_transferred(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::ValueTransferred)?;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&self.ext.value_transferred().to_little_endian(),
			false,
			already_charged,
		)?)
	}

	/// Returns the simulated ethereum `GASPRICE` value.
	/// See [`pallet_revive_uapi::HostFn::gas_price`].
	#[stable]
	fn gas_price(&mut self, memory: &mut M) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::GasPrice)?;
		Ok(GAS_PRICE.into())
	}

	/// Returns the simulated ethereum `BASEFEE` value.
	/// See [`pallet_revive_uapi::HostFn::base_fee`].
	#[stable]
	fn base_fee(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::BaseFee)?;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&U256::zero().to_little_endian(),
			false,
			already_charged,
		)?)
	}

	/// Load the latest block timestamp into the supplied buffer
	/// See [`pallet_revive_uapi::HostFn::now`].
	#[stable]
	fn now(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::Now)?;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&self.ext.now().to_little_endian(),
			false,
			already_charged,
		)?)
	}

	/// Deposit a contract event with the data buffer and optional list of topics.
	/// See [pallet_revive_uapi::HostFn::deposit_event]
	#[stable]
	#[mutating]
	fn deposit_event(
		&mut self,
		memory: &mut M,
		topics_ptr: u32,
		num_topic: u32,
		data_ptr: u32,
		data_len: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::DepositEvent { num_topic, len: data_len })?;

		if num_topic > limits::NUM_EVENT_TOPICS {
			return Err(Error::<E::T>::TooManyTopics.into());
		}

		if data_len > self.ext.max_value_size() {
			return Err(Error::<E::T>::ValueTooLarge.into());
		}

		let topics: Vec<H256> = match num_topic {
			0 => Vec::new(),
			_ => {
				let mut v = Vec::with_capacity(num_topic as usize);
				let topics_len = num_topic * H256::len_bytes() as u32;
				let buf = memory.read(topics_ptr, topics_len)?;
				for chunk in buf.chunks_exact(H256::len_bytes()) {
					v.push(H256::from_slice(chunk));
				}
				v
			},
		};

		let event_data = memory.read(data_ptr, data_len)?;
		self.ext.deposit_event(topics, event_data);
		Ok(())
	}

	/// Stores the current block number of the current contract into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::block_number`].
	#[stable]
	fn block_number(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::BlockNumber)?;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&self.ext.block_number().to_little_endian(),
			false,
			already_charged,
		)?)
	}

	/// Stores the block hash at given block height into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::block_hash`].
	#[stable]
	fn block_hash(
		&mut self,
		memory: &mut M,
		block_number_ptr: u32,
		out_ptr: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::BlockHash)?;
		let block_number = memory.read_u256(block_number_ptr)?;
		let block_hash = self.ext.block_hash(block_number).unwrap_or(H256::zero());
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&block_hash.as_bytes(),
			false,
			already_charged,
		)?)
	}

	/// Stores the current block author into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::block_author`].
	#[stable]
	fn block_author(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::BlockAuthor)?;
		let block_author = self.ext.block_author().unwrap_or(H160::zero());

		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&block_author.as_bytes(),
			false,
			already_charged,
		)?)
	}

	/// Computes the KECCAK 256-bit hash on the given input buffer.
	/// See [`pallet_revive_uapi::HostFn::hash_keccak_256`].
	#[stable]
	fn hash_keccak_256(
		&mut self,
		memory: &mut M,
		input_ptr: u32,
		input_len: u32,
		output_ptr: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::HashKeccak256(input_len))?;
		Ok(self.compute_hash_on_intermediate_buffer(
			memory, keccak_256, input_ptr, input_len, output_ptr,
		)?)
	}

	/// Stores the length of the data returned by the last call into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::return_data_size`].
	#[stable]
	fn return_data_size(&mut self, memory: &mut M) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::ReturnDataSize)?;
		Ok(self
			.ext
			.last_frame_output()
			.data
			.len()
			.try_into()
			.expect("usize fits into u64; qed"))
	}

	/// Stores data returned by the last call, starting from `offset`, into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::return_data`].
	#[stable]
	fn return_data_copy(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		out_len_ptr: u32,
		offset: u32,
	) -> Result<(), TrapReason> {
		let output = mem::take(self.ext.last_frame_output_mut());
		let result = if offset as usize > output.data.len() {
			Err(Error::<E::T>::OutOfBounds.into())
		} else {
			self.write_sandbox_output(
				memory,
				out_ptr,
				out_len_ptr,
				&output.data[offset as usize..],
				false,
				|len| Some(RuntimeCosts::CopyToContract(len)),
			)
		};
		*self.ext.last_frame_output_mut() = output;
		Ok(result?)
	}

	/// Returns the amount of ref_time left.
	/// See [`pallet_revive_uapi::HostFn::ref_time_left`].
	#[stable]
	fn ref_time_left(&mut self, memory: &mut M) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::RefTimeLeft)?;
		Ok(self.ext.gas_meter().gas_left().ref_time())
	}

	/// Checks whether the caller of the current contract is the origin of the whole call stack.
	/// See [`pallet_revive_uapi::HostFn::caller_is_origin`].
	fn caller_is_origin(&mut self, _memory: &mut M) -> Result<u32, TrapReason> {
		self.charge_gas(RuntimeCosts::CallerIsOrigin)?;
		Ok(self.ext.caller_is_origin() as u32)
	}

	/// Checks whether the caller of the current contract is root.
	/// See [`pallet_revive_uapi::HostFn::caller_is_root`].
	fn caller_is_root(&mut self, _memory: &mut M) -> Result<u32, TrapReason> {
		self.charge_gas(RuntimeCosts::CallerIsRoot)?;
		Ok(self.ext.caller_is_root() as u32)
	}

	/// Clear the value at the given key in the contract storage.
	/// See [`pallet_revive_uapi::HostFn::clear_storage`]
	#[mutating]
	fn clear_storage(
		&mut self,
		memory: &mut M,
		flags: u32,
		key_ptr: u32,
		key_len: u32,
	) -> Result<u32, TrapReason> {
		self.clear_storage(memory, flags, key_ptr, key_len)
	}

	/// Checks whether there is a value stored under the given key.
	/// See [`pallet_revive_uapi::HostFn::contains_storage`]
	fn contains_storage(
		&mut self,
		memory: &mut M,
		flags: u32,
		key_ptr: u32,
		key_len: u32,
	) -> Result<u32, TrapReason> {
		self.contains_storage(memory, flags, key_ptr, key_len)
	}

	/// Calculates Ethereum address from the ECDSA compressed public key and stores
	/// See [`pallet_revive_uapi::HostFn::ecdsa_to_eth_address`].
	fn ecdsa_to_eth_address(
		&mut self,
		memory: &mut M,
		key_ptr: u32,
		out_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		self.charge_gas(RuntimeCosts::EcdsaToEthAddress)?;
		let mut compressed_key: [u8; 33] = [0; 33];
		memory.read_into_buf(key_ptr, &mut compressed_key)?;
		let result = self.ext.ecdsa_to_eth_address(&compressed_key);
		match result {
			Ok(eth_address) => {
				memory.write(out_ptr, eth_address.as_ref())?;
				Ok(ReturnErrorCode::Success)
			},
			Err(_) => Ok(ReturnErrorCode::EcdsaRecoveryFailed),
		}
	}

	/// Stores the minimum balance (a.k.a. existential deposit) into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::minimum_balance`].
	fn minimum_balance(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::MinimumBalance)?;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&self.ext.minimum_balance().to_little_endian(),
			false,
			already_charged,
		)?)
	}

	/// Retrieve the code hash of the currently executing contract.
	/// See [`pallet_revive_uapi::HostFn::own_code_hash`].
	fn own_code_hash(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::OwnCodeHash)?;
		let code_hash = *self.ext.own_code_hash();
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			code_hash.as_bytes(),
			false,
			already_charged,
		)?)
	}

	/// Replace the contract code at the specified address with new code.
	/// See [`pallet_revive_uapi::HostFn::set_code_hash`].
	///
	/// Disabled until the internal implementation takes care of collecting
	/// the immutable data of the new code hash.
	#[mutating]
	fn set_code_hash(&mut self, memory: &mut M, code_hash_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::SetCodeHash)?;
		let code_hash: H256 = memory.read_h256(code_hash_ptr)?;
		self.ext.set_code_hash(code_hash)?;
		Ok(())
	}

	/// Verify a sr25519 signature
	/// See [`pallet_revive_uapi::HostFn::sr25519_verify`].
	fn sr25519_verify(
		&mut self,
		memory: &mut M,
		signature_ptr: u32,
		pub_key_ptr: u32,
		message_len: u32,
		message_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		self.charge_gas(RuntimeCosts::Sr25519Verify(message_len))?;

		let mut signature: [u8; 64] = [0; 64];
		memory.read_into_buf(signature_ptr, &mut signature)?;

		let mut pub_key: [u8; 32] = [0; 32];
		memory.read_into_buf(pub_key_ptr, &mut pub_key)?;

		let message: Vec<u8> = memory.read(message_ptr, message_len)?;

		if self.ext.sr25519_verify(&signature, &message, &pub_key) {
			Ok(ReturnErrorCode::Success)
		} else {
			Ok(ReturnErrorCode::Sr25519VerifyFailed)
		}
	}

	/// Retrieve and remove the value under the given key from storage.
	/// See [`pallet_revive_uapi::HostFn::take_storage`]
	#[mutating]
	fn take_storage(
		&mut self,
		memory: &mut M,
		flags: u32,
		key_ptr: u32,
		key_len: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		self.take_storage(memory, flags, key_ptr, key_len, out_ptr, out_len_ptr)
	}

	/// Remove the calling account and transfer remaining **free** balance.
	/// See [`pallet_revive_uapi::HostFn::terminate`].
	#[mutating]
	fn terminate(&mut self, memory: &mut M, beneficiary_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::Terminate)?;
		let beneficiary = memory.read_h160(beneficiary_ptr)?;
		self.ext.terminate(&beneficiary)?;
		Err(TrapReason::Termination)
	}

	/// Stores the amount of weight left into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::weight_left`].
	fn weight_left(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::WeightLeft)?;
		let gas_left = &self.ext.gas_meter().gas_left().encode();
		Ok(self.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			gas_left,
			false,
			already_charged,
		)?)
	}
}
