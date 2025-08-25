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

//! Environment definition of the vm smart-contract runtime.

pub mod env;

#[cfg(doc)]
pub use env::SyscallDoc;

use crate::{
	evm::runtime::GAS_PRICE,
	exec::{ExecError, ExecResult, Ext, Key},
	gas::ChargedAmount,
	limits,
	precompiles::{All as AllPrecompiles, Precompiles},
	primitives::ExecReturnValue,
	BalanceOf, Config, Error, Pallet, RuntimeCosts, LOG_TARGET, SENTINEL,
};
use alloc::{vec, vec::Vec};
use codec::Encode;
use core::{fmt, marker::PhantomData, mem};
use frame_support::{ensure, weights::Weight};
use pallet_revive_uapi::{CallFlags, ReturnErrorCode, ReturnFlags, StorageFlags};
use sp_core::{H160, H256, U256};
use sp_runtime::{DispatchError, RuntimeDebug};

/// Abstraction over the memory access within syscalls.
///
/// The reason for this abstraction is that we run syscalls on the host machine when
/// benchmarking them. In that case we have direct access to the contract's memory. However, when
/// running within PolkaVM we need to resort to copying as we can't map the contracts memory into
/// the host (as of now).
pub trait Memory<T: Config> {
	/// Read designated chunk from the sandbox memory into the supplied buffer.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - requested buffer is not within the bounds of the sandbox memory.
	fn read_into_buf(&self, ptr: u32, buf: &mut [u8]) -> Result<(), DispatchError>;

	/// Write the given buffer to the designated location in the sandbox memory.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - designated area is not within the bounds of the sandbox memory.
	fn write(&mut self, ptr: u32, buf: &[u8]) -> Result<(), DispatchError>;

	/// Zero the designated location in the sandbox memory.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - designated area is not within the bounds of the sandbox memory.
	fn zero(&mut self, ptr: u32, len: u32) -> Result<(), DispatchError>;

	/// This will reset all compilation artifacts of the currently executing instance.
	///
	/// This is used before we call into a new contract to free up some memory. Doing
	/// so we make sure that we only ever have to hold one compilation cache at a time
	/// independtently of of our call stack depth.
	fn reset_interpreter_cache(&mut self);

	/// Read designated chunk from the sandbox memory.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - requested buffer is not within the bounds of the sandbox memory.
	fn read(&self, ptr: u32, len: u32) -> Result<Vec<u8>, DispatchError> {
		let mut buf = vec![0u8; len as usize];
		self.read_into_buf(ptr, buf.as_mut_slice())?;
		Ok(buf)
	}

	/// Same as `read` but reads into a fixed size buffer.
	fn read_array<const N: usize>(&self, ptr: u32) -> Result<[u8; N], DispatchError> {
		let mut buf = [0u8; N];
		self.read_into_buf(ptr, &mut buf)?;
		Ok(buf)
	}

	/// Read a `u32` from the sandbox memory.
	fn read_u32(&self, ptr: u32) -> Result<u32, DispatchError> {
		let buf: [u8; 4] = self.read_array(ptr)?;
		Ok(u32::from_le_bytes(buf))
	}

	/// Read a `U256` from the sandbox memory.
	fn read_u256(&self, ptr: u32) -> Result<U256, DispatchError> {
		let buf: [u8; 32] = self.read_array(ptr)?;
		Ok(U256::from_little_endian(&buf))
	}

	/// Read a `H160` from the sandbox memory.
	fn read_h160(&self, ptr: u32) -> Result<H160, DispatchError> {
		let mut buf = H160::default();
		self.read_into_buf(ptr, buf.as_bytes_mut())?;
		Ok(buf)
	}

	/// Read a `H256` from the sandbox memory.
	fn read_h256(&self, ptr: u32) -> Result<H256, DispatchError> {
		let mut code_hash = H256::default();
		self.read_into_buf(ptr, code_hash.as_bytes_mut())?;
		Ok(code_hash)
	}
}

/// Allows syscalls access to the PolkaVM instance they are executing in.
///
/// In case a contract is executing within PolkaVM its `memory` argument will also implement
/// this trait. The benchmarking implementation of syscalls will only require `Memory`
/// to be implemented.
pub trait PolkaVmInstance<T: Config>: Memory<T> {
	fn gas(&self) -> polkavm::Gas;
	fn set_gas(&mut self, gas: polkavm::Gas);
	fn read_input_regs(&self) -> (u64, u64, u64, u64, u64, u64);
	fn write_output(&mut self, output: u64);
}

// Memory implementation used in benchmarking where guest memory is mapped into the host.
//
// Please note that we could optimize the `read_as_*` functions by decoding directly from
// memory without a copy. However, we don't do that because as it would change the behaviour
// of those functions: A `read_as` with a `len` larger than the actual type can succeed
// in the streaming implementation while it could fail with a segfault in the copy implementation.
#[cfg(feature = "runtime-benchmarks")]
impl<T: Config> Memory<T> for [u8] {
	fn read_into_buf(&self, ptr: u32, buf: &mut [u8]) -> Result<(), DispatchError> {
		let ptr = ptr as usize;
		let bound_checked =
			self.get(ptr..ptr + buf.len()).ok_or_else(|| Error::<T>::OutOfBounds)?;
		buf.copy_from_slice(bound_checked);
		Ok(())
	}

	fn write(&mut self, ptr: u32, buf: &[u8]) -> Result<(), DispatchError> {
		let ptr = ptr as usize;
		let bound_checked =
			self.get_mut(ptr..ptr + buf.len()).ok_or_else(|| Error::<T>::OutOfBounds)?;
		bound_checked.copy_from_slice(buf);
		Ok(())
	}

	fn zero(&mut self, ptr: u32, len: u32) -> Result<(), DispatchError> {
		<[u8] as Memory<T>>::write(self, ptr, &vec![0; len as usize])
	}

	fn reset_interpreter_cache(&mut self) {}
}

impl<T: Config> Memory<T> for polkavm::RawInstance {
	fn read_into_buf(&self, ptr: u32, buf: &mut [u8]) -> Result<(), DispatchError> {
		self.read_memory_into(ptr, buf)
			.map(|_| ())
			.map_err(|_| Error::<T>::OutOfBounds.into())
	}

	fn write(&mut self, ptr: u32, buf: &[u8]) -> Result<(), DispatchError> {
		self.write_memory(ptr, buf).map_err(|_| Error::<T>::OutOfBounds.into())
	}

	fn zero(&mut self, ptr: u32, len: u32) -> Result<(), DispatchError> {
		self.zero_memory(ptr, len).map_err(|_| Error::<T>::OutOfBounds.into())
	}

	fn reset_interpreter_cache(&mut self) {
		self.reset_interpreter_cache();
	}
}

impl<T: Config> PolkaVmInstance<T> for polkavm::RawInstance {
	fn gas(&self) -> polkavm::Gas {
		self.gas()
	}

	fn set_gas(&mut self, gas: polkavm::Gas) {
		self.set_gas(gas)
	}

	fn read_input_regs(&self) -> (u64, u64, u64, u64, u64, u64) {
		(
			self.reg(polkavm::Reg::A0),
			self.reg(polkavm::Reg::A1),
			self.reg(polkavm::Reg::A2),
			self.reg(polkavm::Reg::A3),
			self.reg(polkavm::Reg::A4),
			self.reg(polkavm::Reg::A5),
		)
	}

	fn write_output(&mut self, output: u64) {
		self.set_reg(polkavm::Reg::A0, output);
	}
}

impl From<&ExecReturnValue> for ReturnErrorCode {
	fn from(from: &ExecReturnValue) -> Self {
		if from.flags.contains(ReturnFlags::REVERT) {
			Self::CalleeReverted
		} else {
			Self::Success
		}
	}
}

/// The data passed through when a contract uses `seal_return`.
#[derive(RuntimeDebug)]
pub struct ReturnData {
	/// The flags as passed through by the contract. They are still unchecked and
	/// will later be parsed into a `ReturnFlags` bitflags struct.
	flags: u32,
	/// The output buffer passed by the contract as return data.
	data: Vec<u8>,
}

/// Enumerates all possible reasons why a trap was generated.
///
/// This is either used to supply the caller with more information about why an error
/// occurred (the SupervisorError variant).
/// The other case is where the trap does not constitute an error but rather was invoked
/// as a quick way to terminate the application (all other variants).
#[derive(RuntimeDebug)]
pub enum TrapReason {
	/// The supervisor trapped the contract because of an error condition occurred during
	/// execution in privileged code.
	SupervisorError(DispatchError),
	/// Signals that trap was generated in response to call `seal_return` host function.
	Return(ReturnData),
	/// Signals that a trap was generated in response to a successful call to the
	/// `seal_terminate` host function.
	Termination,
}

impl<T: Into<DispatchError>> From<T> for TrapReason {
	fn from(from: T) -> Self {
		Self::SupervisorError(from.into())
	}
}

impl fmt::Display for TrapReason {
	fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
		Ok(())
	}
}

/// Same as [`Runtime::charge_gas`].
///
/// We need this access as a macro because sometimes hiding the lifetimes behind
/// a function won't work out.
macro_rules! charge_gas {
	($runtime:expr, $costs:expr) => {{
		$runtime.ext.gas_meter_mut().charge($costs)
	}};
}

/// The kind of call that should be performed.
enum CallType {
	/// Execute another instantiated contract
	Call { value_ptr: u32 },
	/// Execute another contract code in the context (storage, account ID, value) of the caller
	/// contract
	DelegateCall,
}

impl CallType {
	fn cost(&self) -> RuntimeCosts {
		match self {
			CallType::Call { .. } => RuntimeCosts::CallBase,
			CallType::DelegateCall => RuntimeCosts::DelegateCallBase,
		}
	}
}

/// This is only appropriate when writing out data of constant size that does not depend on user
/// input. In this case the costs for this copy was already charged as part of the token at
/// the beginning of the API entry point.
fn already_charged(_: u32) -> Option<RuntimeCosts> {
	None
}

/// Helper to extract two `u32` values from a given `u64` register.
fn extract_hi_lo(reg: u64) -> (u32, u32) {
	((reg >> 32) as u32, reg as u32)
}

/// Provides storage variants to support standard and Etheruem compatible semantics.
enum StorageValue {
	/// Indicates that the storage value should be read from a memory buffer.
	/// - `ptr`: A pointer to the start of the data in sandbox memory.
	/// - `len`: The length (in bytes) of the data.
	Memory { ptr: u32, len: u32 },

	/// Indicates that the storage value is provided inline as a fixed-size (256-bit) value.
	/// This is used by set_storage_or_clear() to avoid double reads.
	/// This variant is used to implement Ethereum SSTORE-like semantics.
	Value(Vec<u8>),
}

/// Controls the output behavior for storage reads, both when a key is found and when it is not.
enum StorageReadMode {
	/// VariableOutput mode: if the key exists, the full stored value is returned
	/// using the caller‑provided output length.
	VariableOutput { output_len_ptr: u32 },
	/// Ethereum compatible(FixedOutput32) mode: always write a 32-byte value into the output
	/// buffer. If the key is missing, write 32 bytes of zeros.
	FixedOutput32,
}

/// Can only be used for one call.
pub struct Runtime<'a, E: Ext, M: ?Sized> {
	ext: &'a mut E,
	input_data: Option<Vec<u8>>,
	_phantom_data: PhantomData<M>,
}

impl<'a, E: Ext, M: ?Sized + Memory<E::T>> Runtime<'a, E, M> {
	pub fn new(ext: &'a mut E, input_data: Vec<u8>) -> Self {
		Self { ext, input_data: Some(input_data), _phantom_data: Default::default() }
	}

	/// Get a mutable reference to the inner `Ext`.
	pub fn ext(&mut self) -> &mut E {
		self.ext
	}

	/// Charge the gas meter with the specified token.
	///
	/// Returns `Err(HostError)` if there is not enough gas.
	fn charge_gas(&mut self, costs: RuntimeCosts) -> Result<ChargedAmount, DispatchError> {
		charge_gas!(self, costs)
	}

	/// Adjust a previously charged amount down to its actual amount.
	///
	/// This is when a maximum a priori amount was charged and then should be partially
	/// refunded to match the actual amount.
	fn adjust_gas(&mut self, charged: ChargedAmount, actual_costs: RuntimeCosts) {
		self.ext.gas_meter_mut().adjust_gas(charged, actual_costs);
	}

	/// Write the given buffer and its length to the designated locations in sandbox memory and
	/// charge gas according to the token returned by `create_token`.
	///
	/// `out_ptr` is the location in sandbox memory where `buf` should be written to.
	/// `out_len_ptr` is an in-out location in sandbox memory. It is read to determine the
	/// length of the buffer located at `out_ptr`. If that buffer is smaller than the actual
	/// `buf.len()`, only what fits into that buffer is written to `out_ptr`.
	/// The actual amount of bytes copied to `out_ptr` is written to `out_len_ptr`.
	///
	/// If `out_ptr` is set to the sentinel value of `SENTINEL` and `allow_skip` is true the
	/// operation is skipped and `Ok` is returned. This is supposed to help callers to make copying
	/// output optional. For example to skip copying back the output buffer of an `seal_call`
	/// when the caller is not interested in the result.
	///
	/// `create_token` can optionally instruct this function to charge the gas meter with the token
	/// it returns. `create_token` receives the variable amount of bytes that are about to be copied
	/// by this function.
	///
	/// In addition to the error conditions of `Memory::write` this functions returns
	/// `Err` if the size of the buffer located at `out_ptr` is too small to fit `buf`.
	pub fn write_sandbox_output(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		out_len_ptr: u32,
		buf: &[u8],
		allow_skip: bool,
		create_token: impl FnOnce(u32) -> Option<RuntimeCosts>,
	) -> Result<(), DispatchError> {
		if allow_skip && out_ptr == SENTINEL {
			return Ok(());
		}

		let len = memory.read_u32(out_len_ptr)?;
		let buf_len = len.min(buf.len() as u32);

		if let Some(costs) = create_token(buf_len) {
			self.charge_gas(costs)?;
		}

		memory.write(out_ptr, &buf[..buf_len as usize])?;
		memory.write(out_len_ptr, &buf_len.encode())
	}

	/// Same as `write_sandbox_output` but for static size output.
	pub fn write_fixed_sandbox_output(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		buf: &[u8],
		allow_skip: bool,
		create_token: impl FnOnce(u32) -> Option<RuntimeCosts>,
	) -> Result<(), DispatchError> {
		if buf.is_empty() || (allow_skip && out_ptr == SENTINEL) {
			return Ok(());
		}

		let buf_len = buf.len() as u32;
		if let Some(costs) = create_token(buf_len) {
			self.charge_gas(costs)?;
		}

		memory.write(out_ptr, buf)
	}

	/// Computes the given hash function on the supplied input.
	///
	/// Reads from the sandboxed input buffer into an intermediate buffer.
	/// Returns the result directly to the output buffer of the sandboxed memory.
	///
	/// It is the callers responsibility to provide an output buffer that
	/// is large enough to hold the expected amount of bytes returned by the
	/// chosen hash function.
	///
	/// # Note
	///
	/// The `input` and `output` buffers may overlap.
	fn compute_hash_on_intermediate_buffer<F, R>(
		&self,
		memory: &mut M,
		hash_fn: F,
		input_ptr: u32,
		input_len: u32,
		output_ptr: u32,
	) -> Result<(), DispatchError>
	where
		F: FnOnce(&[u8]) -> R,
		R: AsRef<[u8]>,
	{
		// Copy input into supervisor memory.
		let input = memory.read(input_ptr, input_len)?;
		// Compute the hash on the input buffer using the given hash function.
		let hash = hash_fn(&input);
		// Write the resulting hash back into the sandboxed output buffer.
		memory.write(output_ptr, hash.as_ref())?;
		Ok(())
	}

	/// Fallible conversion of a `ExecError` to `ReturnErrorCode`.
	///
	/// This is used when converting the error returned from a subcall in order to decide
	/// whether to trap the caller or allow handling of the error.
	fn exec_error_into_return_code(from: ExecError) -> Result<ReturnErrorCode, DispatchError> {
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

	fn decode_key(&self, memory: &M, key_ptr: u32, key_len: u32) -> Result<Key, TrapReason> {
		let res = match key_len {
			SENTINEL => {
				let mut buffer = [0u8; 32];
				memory.read_into_buf(key_ptr, buffer.as_mut())?;
				Ok(Key::from_fixed(buffer))
			},
			len => {
				ensure!(len <= limits::STORAGE_KEY_BYTES, Error::<E::T>::DecodingFailed);
				let key = memory.read(key_ptr, len)?;
				Key::try_from_var(key)
			},
		};

		res.map_err(|_| Error::<E::T>::DecodingFailed.into())
	}

	fn is_transient(flags: u32) -> Result<bool, TrapReason> {
		StorageFlags::from_bits(flags)
			.ok_or_else(|| <Error<E::T>>::InvalidStorageFlags.into())
			.map(|flags| flags.contains(StorageFlags::TRANSIENT))
	}

	fn set_storage(
		&mut self,
		memory: &M,
		flags: u32,
		key_ptr: u32,
		key_len: u32,
		value: StorageValue,
	) -> Result<u32, TrapReason> {
		let transient = Self::is_transient(flags)?;
		let costs = |new_bytes: u32, old_bytes: u32| {
			if transient {
				RuntimeCosts::SetTransientStorage { new_bytes, old_bytes }
			} else {
				RuntimeCosts::SetStorage { new_bytes, old_bytes }
			}
		};

		let value_len = match &value {
			StorageValue::Memory { ptr: _, len } => *len,
			StorageValue::Value(data) => data.len() as u32,
		};

		let max_size = self.ext.max_value_size();
		let charged = self.charge_gas(costs(value_len, self.ext.max_value_size()))?;
		if value_len > max_size {
			return Err(Error::<E::T>::ValueTooLarge.into());
		}

		let key = self.decode_key(memory, key_ptr, key_len)?;

		let value = match value {
			StorageValue::Memory { ptr, len } => Some(memory.read(ptr, len)?),
			StorageValue::Value(data) => Some(data),
		};

		let write_outcome = if transient {
			self.ext.set_transient_storage(&key, value, false)?
		} else {
			self.ext.set_storage(&key, value, false)?
		};

		self.adjust_gas(charged, costs(value_len, write_outcome.old_len()));
		Ok(write_outcome.old_len_with_sentinel())
	}

	fn clear_storage(
		&mut self,
		memory: &M,
		flags: u32,
		key_ptr: u32,
		key_len: u32,
	) -> Result<u32, TrapReason> {
		let transient = Self::is_transient(flags)?;
		let costs = |len| {
			if transient {
				RuntimeCosts::ClearTransientStorage(len)
			} else {
				RuntimeCosts::ClearStorage(len)
			}
		};
		let charged = self.charge_gas(costs(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_ptr, key_len)?;
		let outcome = if transient {
			self.ext.set_transient_storage(&key, None, false)?
		} else {
			self.ext.set_storage(&key, None, false)?
		};
		self.adjust_gas(charged, costs(outcome.old_len()));
		Ok(outcome.old_len_with_sentinel())
	}

	fn get_storage(
		&mut self,
		memory: &mut M,
		flags: u32,
		key_ptr: u32,
		key_len: u32,
		out_ptr: u32,
		read_mode: StorageReadMode,
	) -> Result<ReturnErrorCode, TrapReason> {
		let transient = Self::is_transient(flags)?;
		let costs = |len| {
			if transient {
				RuntimeCosts::GetTransientStorage(len)
			} else {
				RuntimeCosts::GetStorage(len)
			}
		};
		let charged = self.charge_gas(costs(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_ptr, key_len)?;
		let outcome = if transient {
			self.ext.get_transient_storage(&key)
		} else {
			self.ext.get_storage(&key)
		};

		if let Some(value) = outcome {
			self.adjust_gas(charged, costs(value.len() as u32));

			match read_mode {
				StorageReadMode::FixedOutput32 => {
					let mut fixed_output = [0u8; 32];
					let len = value.len().min(fixed_output.len());
					fixed_output[..len].copy_from_slice(&value[..len]);

					self.write_fixed_sandbox_output(
						memory,
						out_ptr,
						&fixed_output,
						false,
						already_charged,
					)?;
					Ok(ReturnErrorCode::Success)
				},
				StorageReadMode::VariableOutput { output_len_ptr: out_len_ptr } => {
					self.write_sandbox_output(
						memory,
						out_ptr,
						out_len_ptr,
						&value,
						false,
						already_charged,
					)?;
					Ok(ReturnErrorCode::Success)
				},
			}
		} else {
			self.adjust_gas(charged, costs(0));

			match read_mode {
				StorageReadMode::FixedOutput32 => {
					self.write_fixed_sandbox_output(
						memory,
						out_ptr,
						&[0u8; 32],
						false,
						already_charged,
					)?;
					Ok(ReturnErrorCode::Success)
				},
				StorageReadMode::VariableOutput { .. } => Ok(ReturnErrorCode::KeyNotFound),
			}
		}
	}

	fn contains_storage(
		&mut self,
		memory: &M,
		flags: u32,
		key_ptr: u32,
		key_len: u32,
	) -> Result<u32, TrapReason> {
		let transient = Self::is_transient(flags)?;
		let costs = |len| {
			if transient {
				RuntimeCosts::ContainsTransientStorage(len)
			} else {
				RuntimeCosts::ContainsStorage(len)
			}
		};
		let charged = self.charge_gas(costs(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_ptr, key_len)?;
		let outcome = if transient {
			self.ext.get_transient_storage_size(&key)
		} else {
			self.ext.get_storage_size(&key)
		};
		self.adjust_gas(charged, costs(outcome.unwrap_or(0)));
		Ok(outcome.unwrap_or(SENTINEL))
	}

	fn take_storage(
		&mut self,
		memory: &mut M,
		flags: u32,
		key_ptr: u32,
		key_len: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		let transient = Self::is_transient(flags)?;
		let costs = |len| {
			if transient {
				RuntimeCosts::TakeTransientStorage(len)
			} else {
				RuntimeCosts::TakeStorage(len)
			}
		};
		let charged = self.charge_gas(costs(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_ptr, key_len)?;
		let outcome = if transient {
			self.ext.set_transient_storage(&key, None, true)?
		} else {
			self.ext.set_storage(&key, None, true)?
		};

		if let crate::storage::WriteOutcome::Taken(value) = outcome {
			self.adjust_gas(charged, costs(value.len() as u32));
			self.write_sandbox_output(
				memory,
				out_ptr,
				out_len_ptr,
				&value,
				false,
				already_charged,
			)?;
			Ok(ReturnErrorCode::Success)
		} else {
			self.adjust_gas(charged, costs(0));
			Ok(ReturnErrorCode::KeyNotFound)
		}
	}

	fn call(
		&mut self,
		memory: &mut M,
		flags: CallFlags,
		call_type: CallType,
		callee_ptr: u32,
		deposit_ptr: u32,
		weight: Weight,
		input_data_ptr: u32,
		input_data_len: u32,
		output_ptr: u32,
		output_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		let callee = memory.read_h160(callee_ptr)?;
		let precompile = <AllPrecompiles<E::T>>::get::<E>(&callee.as_fixed_bytes());
		match &precompile {
			Some(precompile) if precompile.has_contract_info() =>
				self.charge_gas(RuntimeCosts::PrecompileWithInfoBase)?,
			Some(_) => self.charge_gas(RuntimeCosts::PrecompileBase)?,
			None => self.charge_gas(call_type.cost())?,
		};

		let deposit_limit = memory.read_u256(deposit_ptr)?;

		// we do check this in exec.rs but we want to error out early
		if input_data_len > limits::CALLDATA_BYTES {
			Err(<Error<E::T>>::CallDataTooLarge)?;
		}

		let input_data = if flags.contains(CallFlags::CLONE_INPUT) {
			let input = self.input_data.as_ref().ok_or(Error::<E::T>::InputForwarded)?;
			charge_gas!(self, RuntimeCosts::CallInputCloned(input.len() as u32))?;
			input.clone()
		} else if flags.contains(CallFlags::FORWARD_INPUT) {
			self.input_data.take().ok_or(Error::<E::T>::InputForwarded)?
		} else {
			if precompile.is_some() {
				self.charge_gas(RuntimeCosts::PrecompileDecode(input_data_len))?;
			} else {
				self.charge_gas(RuntimeCosts::CopyFromContract(input_data_len))?;
			}
			memory.read(input_data_ptr, input_data_len)?
		};

		memory.reset_interpreter_cache();

		let call_outcome = match call_type {
			CallType::Call { value_ptr } => {
				let read_only = flags.contains(CallFlags::READ_ONLY);
				let value = memory.read_u256(value_ptr)?;
				if value > 0u32.into() {
					// If the call value is non-zero and state change is not allowed, issue an
					// error.
					if read_only || self.ext.is_read_only() {
						return Err(Error::<E::T>::StateChangeDenied.into());
					}

					self.charge_gas(RuntimeCosts::CallTransferSurcharge {
						dust_transfer: Pallet::<E::T>::has_dust(value),
					})?;
				}
				self.ext.call(
					weight,
					deposit_limit,
					&callee,
					value,
					input_data,
					flags.contains(CallFlags::ALLOW_REENTRY),
					read_only,
				)
			},
			CallType::DelegateCall => {
				if flags.intersects(CallFlags::ALLOW_REENTRY | CallFlags::READ_ONLY) {
					return Err(Error::<E::T>::InvalidCallFlags.into());
				}
				self.ext.delegate_call(weight, deposit_limit, callee, input_data)
			},
		};

		match call_outcome {
			// `TAIL_CALL` only matters on an `OK` result. Otherwise the call stack comes to
			// a halt anyways without anymore code being executed.
			Ok(_) if flags.contains(CallFlags::TAIL_CALL) => {
				let output = mem::take(self.ext.last_frame_output_mut());
				return Err(TrapReason::Return(ReturnData {
					flags: output.flags.bits(),
					data: output.data,
				}));
			},
			Ok(_) => {
				let output = mem::take(self.ext.last_frame_output_mut());
				let write_result = self.write_sandbox_output(
					memory,
					output_ptr,
					output_len_ptr,
					&output.data,
					true,
					|len| Some(RuntimeCosts::CopyToContract(len)),
				);
				*self.ext.last_frame_output_mut() = output;
				write_result?;
				Ok(self.ext.last_frame_output().into())
			},
			Err(err) => {
				let error_code = Self::exec_error_into_return_code(err)?;
				memory.write(output_len_ptr, &0u32.to_le_bytes())?;
				Ok(error_code)
			},
		}
	}

	fn instantiate(
		&mut self,
		memory: &mut M,
		code_hash_ptr: u32,
		weight: Weight,
		deposit_ptr: u32,
		value_ptr: u32,
		input_data_ptr: u32,
		input_data_len: u32,
		address_ptr: u32,
		output_ptr: u32,
		output_len_ptr: u32,
		salt_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		let value = match memory.read_u256(value_ptr) {
			Ok(value) => {
				self.charge_gas(RuntimeCosts::Instantiate {
					input_data_len,
					balance_transfer: Pallet::<E::T>::has_balance(value),
					dust_transfer: Pallet::<E::T>::has_dust(value),
				})?;
				value
			},
			Err(err) => {
				self.charge_gas(RuntimeCosts::Instantiate {
					input_data_len: 0,
					balance_transfer: false,
					dust_transfer: false,
				})?;
				return Err(err.into());
			},
		};
		let deposit_limit: U256 = memory.read_u256(deposit_ptr)?;
		let code_hash = memory.read_h256(code_hash_ptr)?;
		if input_data_len > limits::CALLDATA_BYTES {
			Err(<Error<E::T>>::CallDataTooLarge)?;
		}
		let input_data = memory.read(input_data_ptr, input_data_len)?;
		let salt = if salt_ptr == SENTINEL {
			None
		} else {
			let salt: [u8; 32] = memory.read_array(salt_ptr)?;
			Some(salt)
		};

		memory.reset_interpreter_cache();

		match self.ext.instantiate(
			weight,
			deposit_limit,
			code_hash,
			value,
			input_data,
			salt.as_ref(),
		) {
			Ok(address) => {
				if !self.ext.last_frame_output().flags.contains(ReturnFlags::REVERT) {
					self.write_fixed_sandbox_output(
						memory,
						address_ptr,
						&address.as_bytes(),
						true,
						already_charged,
					)?;
				}
				let output = mem::take(self.ext.last_frame_output_mut());
				let write_result = self.write_sandbox_output(
					memory,
					output_ptr,
					output_len_ptr,
					&output.data,
					true,
					|len| Some(RuntimeCosts::CopyToContract(len)),
				);
				*self.ext.last_frame_output_mut() = output;
				write_result?;
				Ok(self.ext.last_frame_output().into())
			},
			Err(err) => Ok(Self::exec_error_into_return_code(err)?),
		}
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
	pub fn setup_aux_data(
		&mut self,
		data: &[u8],
		offset: u32,
		a1: u64,
	) -> frame_support::dispatch::DispatchResult {
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
