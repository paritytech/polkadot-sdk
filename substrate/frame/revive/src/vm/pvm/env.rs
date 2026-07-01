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
	Config, Error, Weight, address::AddressMapper, exec::Ext, limits, primitives::ExecReturnValue,
	vm::RuntimeCosts,
};
use alloc::vec::Vec;
use core::mem;
use frame_support::traits::Get;
use pallet_revive_proc_macro::define_env;
use pallet_revive_uapi::{CallFlags, ReturnErrorCode, ReturnFlags};
use sp_core::U256;
use sp_io::hashing::keccak_256;
use sp_runtime::SaturatedConversion;

impl<'a, E: Ext, M: PolkaVmInstance<E::T>> Runtime<'a, E, M> {
	pub fn handle_interrupt(
		&mut self,
		interrupt: Interrupt,
		instance: &mut M,
	) -> Option<ExecResult> {
		match interrupt {
			Interrupt::Error(error) => Some(Err(error.into())),
			Interrupt::Finished => {
				Some(Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() }))
			},
			Interrupt::Trap => Some(Err(Error::<E::T>::ContractTrapped.into())),
			Interrupt::OutOfGas => Some(Err(Error::<E::T>::OutOfGas.into())),
			Interrupt::Ecalli(idx) => {
				// Resolve the symbol bytes (borrowed from `instance`) to a Copy
				// syscall id inside a scoped block, so the immutable borrow on
				// `instance` is released before `handle_ecall` takes it mutably.
				let syscall_id: u32 = {
					let Some(symbol) = instance.resolve_import(idx) else {
						return Some(Err(<Error<E::T>>::InvalidSyscall.into()));
					};
					match env::lookup_syscall_index(symbol) {
						Some(id) => id,
						None => return Some(Err(<Error<E::T>>::InvalidSyscall.into())),
					}
				};
				match self.handle_ecall(instance, syscall_id) {
					Ok(None) => None,
					Ok(Some(return_value)) => {
						instance.write_output(return_value);
						None
					},
					Err(TrapReason::Return(ReturnData { flags, data })) => {
						match ReturnFlags::from_bits(flags) {
							None => Some(Err(Error::<E::T>::InvalidCallFlags.into())),
							Some(flags) => Some(Ok(ExecReturnValue { flags, data })),
						}
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
	#[cfg(feature = "runtime-benchmarks")]
	fn noop(&mut self, memory: &mut M) -> Result<(), TrapReason> {
		Ok(())
	}

	/// Set the value at the given key in the contract storage.
	/// See [`pallet_revive_uapi::HostFn::set_storage`]
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
		let weight = Weight::from_parts(ref_time_limit, proof_size_limit);

		self.charge_gas(RuntimeCosts::CopyFromContract(32))?;
		let deposit_limit = memory.read_u256(deposit_ptr)?;

		self.call(
			memory,
			CallFlags::from_bits(flags).ok_or(Error::<E::T>::InvalidCallFlags)?,
			CallType::Call { value_ptr },
			callee_ptr,
			&CallResources::from_weight_and_deposit(weight, deposit_limit),
			input_data_ptr,
			input_data_len,
			output_ptr,
			output_len_ptr,
		)
	}

	/// Make a call to another contract.
	/// See [`pallet_revive_uapi::HostFn::call_evm`].
	fn call_evm(
		&mut self,
		memory: &mut M,
		flags: u32,
		callee: u32,
		value_ptr: u32,
		gas: u64,
		input_data: u64,
		output_data: u64,
	) -> Result<ReturnErrorCode, TrapReason> {
		let (input_data_len, input_data_ptr) = extract_hi_lo(input_data);
		let (output_len_ptr, output_ptr) = extract_hi_lo(output_data);
		let resources = if gas == u64::MAX {
			CallResources::NoLimits
		} else {
			self.charge_gas(RuntimeCosts::CopyFromContract(32))?;
			let value = memory.read_u256(value_ptr)?;
			// We also need to detect the 2300: We need to add something scaled.
			let add_stipend = !value.is_zero() || gas == revm::interpreter::gas::CALL_STIPEND;
			CallResources::from_ethereum_gas(gas.into(), add_stipend)
		};

		self.call(
			memory,
			CallFlags::from_bits(flags).ok_or(Error::<E::T>::InvalidCallFlags)?,
			CallType::Call { value_ptr },
			callee,
			&resources,
			input_data_ptr,
			input_data_len,
			output_ptr,
			output_len_ptr,
		)
	}

	/// Execute code in the context (storage, caller, value) of the current contract.
	/// See [`pallet_revive_uapi::HostFn::delegate_call`].
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
		let weight = Weight::from_parts(ref_time_limit, proof_size_limit);

		self.charge_gas(RuntimeCosts::CopyFromContract(32))?;
		let deposit_limit = memory.read_u256(deposit_ptr)?;

		self.call(
			memory,
			CallFlags::from_bits(flags).ok_or(Error::<E::T>::InvalidCallFlags)?,
			CallType::DelegateCall,
			address_ptr,
			&CallResources::from_weight_and_deposit(weight, deposit_limit),
			input_data_ptr,
			input_data_len,
			output_ptr,
			output_len_ptr,
		)
	}

	/// Same as `delegate_call` but with EVM gas.
	/// See [`pallet_revive_uapi::HostFn::delegate_call_evm`].
	fn delegate_call_evm(
		&mut self,
		memory: &mut M,
		flags: u32,
		callee: u32,
		gas: u64,
		input_data: u64,
		output_data: u64,
	) -> Result<ReturnErrorCode, TrapReason> {
		let (input_data_len, input_data_ptr) = extract_hi_lo(input_data);
		let (output_len_ptr, output_ptr) = extract_hi_lo(output_data);
		let resources = if gas == u64::MAX {
			CallResources::NoLimits
		} else {
			CallResources::from_ethereum_gas(gas.into(), false)
		};

		self.call(
			memory,
			CallFlags::from_bits(flags).ok_or(Error::<E::T>::InvalidCallFlags)?,
			CallType::DelegateCall,
			callee,
			&resources,
			input_data_ptr,
			input_data_len,
			output_ptr,
			output_len_ptr,
		)
	}

	/// Instantiate a contract with the specified code hash.
	/// See [`pallet_revive_uapi::HostFn::instantiate`].
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
	fn code_size(&mut self, memory: &mut M, addr_ptr: u32) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::CodeSize)?;
		let address = memory.read_h160(addr_ptr)?;
		Ok(self.ext.code_size(&address))
	}

	/// Stores the address of the current contract into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::address`].
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

	/// Stores the immutable data into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::get_immutable_data`].
	fn get_immutable_data(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<(), TrapReason> {
		let is_delegate = self.ext.is_delegate_call();
		let charge_len =
			if is_delegate { limits::IMMUTABLE_BYTES } else { self.ext.immutable_data_len() };
		let charged = self.charge_gas(RuntimeCosts::GetImmutableData(charge_len))?;
		let data = self.ext.get_immutable_data()?;
		if is_delegate {
			self.adjust_gas(charged, RuntimeCosts::GetImmutableData(data.len() as u32));
		}
		self.write_sandbox_output(memory, out_ptr, out_len_ptr, &data, false, already_charged)?;
		Ok(())
	}

	/// Attaches the supplied immutable data to the currently executing contract.
	/// See [`pallet_revive_uapi::HostFn::set_immutable_data`].
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
	fn gas_limit(&mut self, memory: &mut M) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::GasLimit)?;
		Ok(self.ext.gas_limit())
	}

	/// Stores the value transferred along with this call/instantiate into the supplied buffer.
	/// See [`pallet_revive_uapi::HostFn::value_transferred`].
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
	fn gas_price(&mut self, memory: &mut M) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::GasPrice)?;
		Ok(self.ext.effective_gas_price().saturated_into())
	}

	/// Returns the simulated ethereum `BASEFEE` value.
	/// See [`pallet_revive_uapi::HostFn::base_fee`].
	fn base_fee(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::BaseFee)?;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&Pallet::<E::T>::evm_base_fee().to_little_endian(),
			false,
			already_charged,
		)?)
	}

	/// Load the latest block timestamp into the supplied buffer
	/// See [`pallet_revive_uapi::HostFn::now`].
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

		if data_len > limits::EVENT_BYTES {
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
	fn block_author(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::BlockAuthor)?;
		let block_author = self.ext.block_author();

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
	/// See [`pallet_revive_uapi::HostFn::return_data_copy`].
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

	/// Returns the amount of evm gas left.
	///
	/// The name is only for historical reasons as renaming functions
	/// would be a breaking change.
	///
	/// See [`pallet_revive_uapi::HostFn::gas_left`].
	fn ref_time_left(&mut self, memory: &mut M) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::RefTimeLeft)?;
		Ok(self.ext.gas_left())
	}

	/// Reverts the execution without data and cedes all remaining gas.
	///
	/// See [`pallet_revive_uapi::HostFn::consume_all_gas`].
	fn consume_all_gas(&mut self, memory: &mut M) -> Result<(), TrapReason> {
		self.ext.frame_meter_mut().consume_all_weight();
		Err(TrapReason::Return(ReturnData {
			flags: ReturnFlags::REVERT.bits(),
			data: Default::default(),
		}))
	}

	/// Calculates Ethereum address from the ECDSA compressed public key and stores
	/// See `uapi/sol/ISystem.sol`.
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

	/// Verify a sr25519 signature
	/// See `uapi/sol/ISystem.sol`.
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

	/// Remove the calling account and transfer remaining balance:
	/// **total** balance if code is deleted from storage, else **free** balance only.
	/// See [`pallet_revive_uapi::HostFn::terminate`].
	#[mutating]
	fn terminate(&mut self, memory: &mut M, beneficiary_ptr: u32) -> Result<(), TrapReason> {
		let charged = self.charge_gas(RuntimeCosts::Terminate { code_removed: true })?;
		let beneficiary = memory.read_h160(beneficiary_ptr)?;
		if matches!(self.ext.terminate_if_same_tx(&beneficiary)?, crate::CodeRemoved::No) {
			self.adjust_gas(charged, RuntimeCosts::Terminate { code_removed: false });
		}
		Err(TrapReason::Termination)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pallet_revive_types::runtime_api::*;
	use strum::VariantArray;

	#[test]
	fn wire_has_all_syscalls() {
		let wire_syscalls = PolkavmSyscallV1::VARIANTS
			.iter()
			.copied()
			.map(<&'static str>::from)
			.collect::<Vec<_>>();
		let canonical_syscalls = list_trace_ops()
			.iter()
			.map(|bytes| str::from_utf8(bytes).unwrap())
			.collect::<Vec<_>>();

		assert_eq!(
			wire_syscalls, canonical_syscalls,
			"There is a difference between the syscalls defined for the wire and the syscalls \
			available in the system. Wire = {wire_syscalls:?}. Available = {canonical_syscalls:?}"
		)
	}
}
