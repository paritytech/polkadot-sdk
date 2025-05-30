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

//! Environment definition of the wasm smart-contract runtime.

use crate::{
	address::AddressMapper,
	evm::runtime::GAS_PRICE,
	exec::{ExecError, ExecResult, Ext, Key},
	gas::{ChargedAmount, Token},
	limits,
	precompiles::{All as AllPrecompiles, Precompiles},
	primitives::ExecReturnValue,
	weights::WeightInfo,
	Config, Error, LOG_TARGET, SENTINEL,
};
use alloc::{boxed::Box, vec, vec::Vec};
use codec::{Decode, DecodeLimit, Encode};
use core::{fmt, marker::PhantomData, mem};
use frame_support::{
	dispatch::DispatchInfo, ensure, pallet_prelude::DispatchResultWithPostInfo, parameter_types,
	traits::Get, weights::Weight,
};
use pallet_revive_proc_macro::define_env;
use pallet_revive_uapi::{CallFlags, ReturnErrorCode, ReturnFlags, StorageFlags};
use sp_core::{H160, H256, U256};
use sp_io::hashing::{blake2_128, blake2_256, keccak_256};
use sp_runtime::{DispatchError, RuntimeDebug};

type CallOf<T> = <T as frame_system::Config>::RuntimeCall;

/// The maximum nesting depth a contract can use when encoding types.
const MAX_DECODE_NESTING: u32 = 256;

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

	/// Read designated chunk from the sandbox memory and attempt to decode into the specified type.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - requested buffer is not within the bounds of the sandbox memory.
	/// - the buffer contents cannot be decoded as the required type.
	///
	/// # Note
	///
	/// Make sure to charge a proportional amount of weight if `len` is not fixed.
	fn read_as_unbounded<D: Decode>(&self, ptr: u32, len: u32) -> Result<D, DispatchError> {
		let buf = self.read(ptr, len)?;
		let decoded = D::decode_all_with_depth_limit(MAX_DECODE_NESTING, &mut buf.as_ref())
			.map_err(|_| DispatchError::from(Error::<T>::DecodingFailed))?;
		Ok(decoded)
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

parameter_types! {
	/// Getter types used by [`crate::SyscallDoc:call_runtime`]
	const CallRuntimeFailed: ReturnErrorCode = ReturnErrorCode::CallRuntimeFailed;
	/// Getter types used by [`crate::SyscallDoc::xcm_execute`]
	const XcmExecutionFailed: ReturnErrorCode = ReturnErrorCode::XcmExecutionFailed;
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

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Copy, Clone)]
pub enum RuntimeCosts {
	/// Base Weight of calling a host function.
	HostFn,
	/// Weight charged for copying data from the sandbox.
	CopyFromContract(u32),
	/// Weight charged for copying data to the sandbox.
	CopyToContract(u32),
	/// Weight of calling `seal_call_data_load``.
	CallDataLoad,
	/// Weight of calling `seal_call_data_copy`.
	CallDataCopy(u32),
	/// Weight of calling `seal_caller`.
	Caller,
	/// Weight of calling `seal_call_data_size`.
	CallDataSize,
	/// Weight of calling `seal_return_data_size`.
	ReturnDataSize,
	/// Weight of calling `seal_to_account_id`.
	ToAccountId,
	/// Weight of calling `seal_origin`.
	Origin,
	/// Weight of calling `seal_is_contract`.
	IsContract,
	/// Weight of calling `seal_code_hash`.
	CodeHash,
	/// Weight of calling `seal_own_code_hash`.
	OwnCodeHash,
	/// Weight of calling `seal_code_size`.
	CodeSize,
	/// Weight of calling `seal_caller_is_origin`.
	CallerIsOrigin,
	/// Weight of calling `caller_is_root`.
	CallerIsRoot,
	/// Weight of calling `seal_address`.
	Address,
	/// Weight of calling `seal_ref_time_left`.
	RefTimeLeft,
	/// Weight of calling `seal_weight_left`.
	WeightLeft,
	/// Weight of calling `seal_balance`.
	Balance,
	/// Weight of calling `seal_balance_of`.
	BalanceOf,
	/// Weight of calling `seal_value_transferred`.
	ValueTransferred,
	/// Weight of calling `seal_minimum_balance`.
	MinimumBalance,
	/// Weight of calling `seal_block_number`.
	BlockNumber,
	/// Weight of calling `seal_block_hash`.
	BlockHash,
	/// Weight of calling `seal_block_author`.
	BlockAuthor,
	/// Weight of calling `seal_gas_price`.
	GasPrice,
	/// Weight of calling `seal_base_fee`.
	BaseFee,
	/// Weight of calling `seal_now`.
	Now,
	/// Weight of calling `seal_gas_limit`.
	GasLimit,
	/// Weight of calling `seal_weight_to_fee`.
	WeightToFee,
	/// Weight of calling `seal_terminate`.
	Terminate,
	/// Weight of calling `seal_deposit_event` with the given number of topics and event size.
	DepositEvent { num_topic: u32, len: u32 },
	/// Weight of calling `seal_set_storage` for the given storage item sizes.
	SetStorage { old_bytes: u32, new_bytes: u32 },
	/// Weight of calling `seal_clear_storage` per cleared byte.
	ClearStorage(u32),
	/// Weight of calling `seal_contains_storage` per byte of the checked item.
	ContainsStorage(u32),
	/// Weight of calling `seal_get_storage` with the specified size in storage.
	GetStorage(u32),
	/// Weight of calling `seal_take_storage` for the given size.
	TakeStorage(u32),
	/// Weight of calling `seal_set_transient_storage` for the given storage item sizes.
	SetTransientStorage { old_bytes: u32, new_bytes: u32 },
	/// Weight of calling `seal_clear_transient_storage` per cleared byte.
	ClearTransientStorage(u32),
	/// Weight of calling `seal_contains_transient_storage` per byte of the checked item.
	ContainsTransientStorage(u32),
	/// Weight of calling `seal_get_transient_storage` with the specified size in storage.
	GetTransientStorage(u32),
	/// Weight of calling `seal_take_transient_storage` for the given size.
	TakeTransientStorage(u32),
	/// Base weight of calling `seal_call`.
	CallBase,
	/// Weight of calling `seal_delegate_call` for the given input size.
	DelegateCallBase,
	/// Weight of calling a precompile.
	PrecompileBase,
	/// Weight of calling a precompile that has a contract info.
	PrecompileWithInfoBase,
	/// Weight of reading and decoding the input to a precompile.
	PrecompileDecode(u32),
	/// Weight of the transfer performed during a call.
	CallTransferSurcharge,
	/// Weight per byte that is cloned by supplying the `CLONE_INPUT` flag.
	CallInputCloned(u32),
	/// Weight of calling `seal_instantiate` for the given input length.
	Instantiate { input_data_len: u32 },
	/// Weight of calling `Ripemd160` precompile for the given input size.
	Ripemd160(u32),
	/// Weight of calling `Sha256` precompile for the given input size.
	HashSha256(u32),
	/// Weight of calling `seal_hash_keccak_256` for the given input size.
	HashKeccak256(u32),
	/// Weight of calling `seal_hash_blake2_256` for the given input size.
	HashBlake256(u32),
	/// Weight of calling `seal_hash_blake2_128` for the given input size.
	HashBlake128(u32),
	/// Weight of calling `ECERecover` precompile.
	EcdsaRecovery,
	/// Weight of calling `seal_sr25519_verify` for the given input size.
	Sr25519Verify(u32),
	/// Weight charged for calling into the runtime.
	CallRuntime(Weight),
	/// Weight charged by a precompile.
	Precompile(Weight),
	/// Weight charged for calling xcm_execute.
	CallXcmExecute(Weight),
	/// Weight of calling `seal_set_code_hash`
	SetCodeHash,
	/// Weight of calling `ecdsa_to_eth_address`
	EcdsaToEthAddress,
	/// Weight of calling `get_immutable_dependency`
	GetImmutableData(u32),
	/// Weight of calling `set_immutable_dependency`
	SetImmutableData(u32),
	/// Weight of calling `Bn128Add` precompile
	Bn128Add,
	/// Weight of calling `Bn128Add` precompile
	Bn128Mul,
	/// Weight of calling `Bn128Pairing` precompile for the given number of input pairs.
	Bn128Pairing(u32),
	/// Weight of calling `Identity` precompile for the given number of input length.
	Identity(u32),
	/// Weight of calling `Blake2F` precompile for the given number of rounds.
	Blake2F(u32),
	/// Weight of calling `Modexp` precompile
	Modexp(u64),
}

/// For functions that modify storage, benchmarks are performed with one item in the
/// storage. To account for the worst-case scenario, the weight of the overhead of
/// writing to or reading from full storage is included. For transient storage writes,
/// the rollback weight is added to reflect the worst-case scenario for this operation.
macro_rules! cost_storage {
    (write_transient, $name:ident $(, $arg:expr )*) => {
        T::WeightInfo::$name($( $arg ),*)
            .saturating_add(T::WeightInfo::rollback_transient_storage())
            .saturating_add(T::WeightInfo::set_transient_storage_full()
            .saturating_sub(T::WeightInfo::set_transient_storage_empty()))
    };

    (read_transient, $name:ident $(, $arg:expr )*) => {
        T::WeightInfo::$name($( $arg ),*)
            .saturating_add(T::WeightInfo::get_transient_storage_full()
            .saturating_sub(T::WeightInfo::get_transient_storage_empty()))
    };

    (write, $name:ident $(, $arg:expr )*) => {
        T::WeightInfo::$name($( $arg ),*)
            .saturating_add(T::WeightInfo::set_storage_full()
            .saturating_sub(T::WeightInfo::set_storage_empty()))
    };

    (read, $name:ident $(, $arg:expr )*) => {
        T::WeightInfo::$name($( $arg ),*)
            .saturating_add(T::WeightInfo::get_storage_full()
            .saturating_sub(T::WeightInfo::get_storage_empty()))
    };
}

macro_rules! cost_args {
	// cost_args!(name, a, b, c) -> T::WeightInfo::name(a, b, c).saturating_sub(T::WeightInfo::name(0, 0, 0))
	($name:ident, $( $arg: expr ),+) => {
		(T::WeightInfo::$name($( $arg ),+).saturating_sub(cost_args!(@call_zero $name, $( $arg ),+)))
	};
	// Transform T::WeightInfo::name(a, b, c) into T::WeightInfo::name(0, 0, 0)
	(@call_zero $name:ident, $( $arg:expr ),*) => {
		T::WeightInfo::$name($( cost_args!(@replace_token $arg) ),*)
	};
	// Replace the token with 0.
	(@replace_token $_in:tt) => { 0 };
}

impl<T: Config> Token<T> for RuntimeCosts {
	fn influence_lowest_gas_limit(&self) -> bool {
		match self {
			&Self::CallXcmExecute(_) => false,
			_ => true,
		}
	}

	fn weight(&self) -> Weight {
		use self::RuntimeCosts::*;
		match *self {
			HostFn => cost_args!(noop_host_fn, 1),
			CopyToContract(len) => T::WeightInfo::seal_copy_to_contract(len),
			CopyFromContract(len) => T::WeightInfo::seal_return(len),
			CallDataSize => T::WeightInfo::seal_call_data_size(),
			ReturnDataSize => T::WeightInfo::seal_return_data_size(),
			CallDataLoad => T::WeightInfo::seal_call_data_load(),
			CallDataCopy(len) => T::WeightInfo::seal_call_data_copy(len),
			Caller => T::WeightInfo::seal_caller(),
			Origin => T::WeightInfo::seal_origin(),
			IsContract => T::WeightInfo::seal_is_contract(),
			ToAccountId => T::WeightInfo::seal_to_account_id(),
			CodeHash => T::WeightInfo::seal_code_hash(),
			CodeSize => T::WeightInfo::seal_code_size(),
			OwnCodeHash => T::WeightInfo::seal_own_code_hash(),
			CallerIsOrigin => T::WeightInfo::seal_caller_is_origin(),
			CallerIsRoot => T::WeightInfo::seal_caller_is_root(),
			Address => T::WeightInfo::seal_address(),
			RefTimeLeft => T::WeightInfo::seal_ref_time_left(),
			WeightLeft => T::WeightInfo::seal_weight_left(),
			Balance => T::WeightInfo::seal_balance(),
			BalanceOf => T::WeightInfo::seal_balance_of(),
			ValueTransferred => T::WeightInfo::seal_value_transferred(),
			MinimumBalance => T::WeightInfo::seal_minimum_balance(),
			BlockNumber => T::WeightInfo::seal_block_number(),
			BlockHash => T::WeightInfo::seal_block_hash(),
			BlockAuthor => T::WeightInfo::seal_block_author(),
			GasPrice => T::WeightInfo::seal_gas_price(),
			BaseFee => T::WeightInfo::seal_base_fee(),
			Now => T::WeightInfo::seal_now(),
			GasLimit => T::WeightInfo::seal_gas_limit(),
			WeightToFee => T::WeightInfo::seal_weight_to_fee(),
			Terminate => T::WeightInfo::seal_terminate(),
			DepositEvent { num_topic, len } => T::WeightInfo::seal_deposit_event(num_topic, len),
			SetStorage { new_bytes, old_bytes } => {
				cost_storage!(write, seal_set_storage, new_bytes, old_bytes)
			},
			ClearStorage(len) => cost_storage!(write, seal_clear_storage, len),
			ContainsStorage(len) => cost_storage!(read, seal_contains_storage, len),
			GetStorage(len) => cost_storage!(read, seal_get_storage, len),
			TakeStorage(len) => cost_storage!(write, seal_take_storage, len),
			SetTransientStorage { new_bytes, old_bytes } => {
				cost_storage!(write_transient, seal_set_transient_storage, new_bytes, old_bytes)
			},
			ClearTransientStorage(len) => {
				cost_storage!(write_transient, seal_clear_transient_storage, len)
			},
			ContainsTransientStorage(len) => {
				cost_storage!(read_transient, seal_contains_transient_storage, len)
			},
			GetTransientStorage(len) => {
				cost_storage!(read_transient, seal_get_transient_storage, len)
			},
			TakeTransientStorage(len) => {
				cost_storage!(write_transient, seal_take_transient_storage, len)
			},
			CallBase => T::WeightInfo::seal_call(0, 0),
			DelegateCallBase => T::WeightInfo::seal_delegate_call(),
			PrecompileBase => T::WeightInfo::seal_call_precompile(0, 0),
			PrecompileWithInfoBase => T::WeightInfo::seal_call_precompile(1, 0),
			PrecompileDecode(len) => cost_args!(seal_call_precompile, 0, len),
			CallTransferSurcharge => cost_args!(seal_call, 1, 0),
			CallInputCloned(len) => cost_args!(seal_call, 0, len),
			Instantiate { input_data_len } => T::WeightInfo::seal_instantiate(input_data_len),
			HashSha256(len) => T::WeightInfo::sha2_256(len),
			Ripemd160(len) => T::WeightInfo::ripemd_160(len),
			HashKeccak256(len) => T::WeightInfo::seal_hash_keccak_256(len),
			HashBlake256(len) => T::WeightInfo::seal_hash_blake2_256(len),
			HashBlake128(len) => T::WeightInfo::seal_hash_blake2_128(len),
			EcdsaRecovery => T::WeightInfo::ecdsa_recover(),
			Sr25519Verify(len) => T::WeightInfo::seal_sr25519_verify(len),
			Precompile(weight) | CallRuntime(weight) | CallXcmExecute(weight) => weight,
			SetCodeHash => T::WeightInfo::seal_set_code_hash(),
			EcdsaToEthAddress => T::WeightInfo::seal_ecdsa_to_eth_address(),
			GetImmutableData(len) => T::WeightInfo::seal_get_immutable_data(len),
			SetImmutableData(len) => T::WeightInfo::seal_set_immutable_data(len),
			Bn128Add => T::WeightInfo::bn128_add(),
			Bn128Mul => T::WeightInfo::bn128_mul(),
			Bn128Pairing(len) => T::WeightInfo::bn128_pairing(len),
			Identity(len) => T::WeightInfo::identity(len),
			Blake2F(rounds) => T::WeightInfo::blake2f(rounds),
			Modexp(gas) => {
				use frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND;
				/// Current approximation of the gas/s consumption considering
				/// EVM execution over compiled WASM (on 4.4Ghz CPU).
				/// Given the 2000ms Weight, from which 75% only are used for transactions,
				/// the total EVM execution gas limit is: GAS_PER_SECOND * 2 * 0.75 ~= 60_000_000.
				const GAS_PER_SECOND: u64 = 40_000_000;

				/// Approximate ratio of the amount of Weight per Gas.
				/// u64 works for approximations because Weight is a very small unit compared to
				/// gas.
				const WEIGHT_PER_GAS: u64 = WEIGHT_REF_TIME_PER_SECOND / GAS_PER_SECOND;
				Weight::from_parts(gas.saturating_mul(WEIGHT_PER_GAS), 0)
			},
		}
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
	/// Ethereum commpatible(FixedOutput32) mode: always write a 32-byte value into the output
	/// buffer. If the key is missing, write 32 bytes of zeros.
	FixedOutput32,
}

/// Can only be used for one call.
pub struct Runtime<'a, E: Ext, M: ?Sized> {
	ext: &'a mut E,
	input_data: Option<Vec<u8>>,
	_phantom_data: PhantomData<M>,
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

	/// Charge, Run and adjust gas, for executing the given dispatchable.
	fn call_dispatchable<ErrorReturnCode: Get<ReturnErrorCode>>(
		&mut self,
		dispatch_info: DispatchInfo,
		runtime_cost: impl Fn(Weight) -> RuntimeCosts,
		run: impl FnOnce(&mut Self) -> DispatchResultWithPostInfo,
	) -> Result<ReturnErrorCode, TrapReason> {
		use frame_support::dispatch::extract_actual_weight;
		let charged = self.charge_gas(runtime_cost(dispatch_info.call_weight))?;
		let result = run(self);
		let actual_weight = extract_actual_weight(&result, &dispatch_info);
		self.adjust_gas(charged, runtime_cost(actual_weight));
		match result {
			Ok(_) => Ok(ReturnErrorCode::Success),
			Err(e) => {
				log::debug!(target: LOG_TARGET, "call failed with: {e:?}");
				Ok(ErrorReturnCode::get())
			},
		}
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
					self.charge_gas(RuntimeCosts::CallTransferSurcharge)?;
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
		self.charge_gas(RuntimeCosts::Instantiate { input_data_len })?;
		let deposit_limit: U256 = memory.read_u256(deposit_ptr)?;
		let value = memory.read_u256(value_ptr)?;
		let code_hash = memory.read_h256(code_hash_ptr)?;
		let input_data = memory.read(input_data_ptr, input_data_len)?;
		let salt = if salt_ptr == SENTINEL {
			None
		} else {
			let salt: [u8; 32] = memory.read_array(salt_ptr)?;
			Some(salt)
		};

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
		let block_author = self
			.ext
			.block_author()
			.map(|account| <E::T as Config>::AddressMapper::to_address(&account))
			.unwrap_or(H160::zero());
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

	/// Call some dispatchable of the runtime.
	/// See [`frame_support::traits::call_runtime`].
	#[mutating]
	fn call_runtime(
		&mut self,
		memory: &mut M,
		call_ptr: u32,
		call_len: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		use frame_support::dispatch::GetDispatchInfo;
		self.charge_gas(RuntimeCosts::CopyFromContract(call_len))?;
		let call: <E::T as Config>::RuntimeCall = memory.read_as_unbounded(call_ptr, call_len)?;
		self.call_dispatchable::<CallRuntimeFailed>(
			call.get_dispatch_info(),
			RuntimeCosts::CallRuntime,
			|runtime| runtime.ext.call_runtime(call),
		)
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

	/// Computes the BLAKE2 128-bit hash on the given input buffer.
	/// See [`pallet_revive_uapi::HostFn::hash_blake2_128`].
	fn hash_blake2_128(
		&mut self,
		memory: &mut M,
		input_ptr: u32,
		input_len: u32,
		output_ptr: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::HashBlake128(input_len))?;
		Ok(self.compute_hash_on_intermediate_buffer(
			memory, blake2_128, input_ptr, input_len, output_ptr,
		)?)
	}

	/// Computes the BLAKE2 256-bit hash on the given input buffer.
	/// See [`pallet_revive_uapi::HostFn::hash_blake2_256`].
	fn hash_blake2_256(
		&mut self,
		memory: &mut M,
		input_ptr: u32,
		input_len: u32,
		output_ptr: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::HashBlake256(input_len))?;
		Ok(self.compute_hash_on_intermediate_buffer(
			memory, blake2_256, input_ptr, input_len, output_ptr,
		)?)
	}

	/// Checks whether a specified address belongs to a contract.
	/// See [`pallet_revive_uapi::HostFn::is_contract`].
	fn is_contract(&mut self, memory: &mut M, account_ptr: u32) -> Result<u32, TrapReason> {
		self.charge_gas(RuntimeCosts::IsContract)?;
		let address = memory.read_h160(account_ptr)?;
		Ok(self.ext.is_contract(&address) as u32)
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

	/// Execute an XCM program locally, using the contract's address as the origin.
	/// See [`pallet_revive_uapi::HostFn::execute_xcm`].
	#[mutating]
	fn xcm_execute(
		&mut self,
		memory: &mut M,
		msg_ptr: u32,
		msg_len: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		use frame_support::dispatch::DispatchInfo;
		use xcm::VersionedXcm;
		use xcm_builder::{ExecuteController, ExecuteControllerWeightInfo};

		self.charge_gas(RuntimeCosts::CopyFromContract(msg_len))?;
		let message: VersionedXcm<CallOf<E::T>> = memory.read_as_unbounded(msg_ptr, msg_len)?;

		let execute_weight =
			<<E::T as Config>::Xcm as ExecuteController<_, _>>::WeightInfo::execute();
		let weight = self.ext.gas_meter().gas_left().max(execute_weight);
		let dispatch_info = DispatchInfo { call_weight: weight, ..Default::default() };

		self.call_dispatchable::<XcmExecutionFailed>(
			dispatch_info,
			RuntimeCosts::CallXcmExecute,
			|runtime| {
				let origin = crate::RawOrigin::Signed(runtime.ext.account_id().clone()).into();
				let weight_used = <<E::T as Config>::Xcm>::execute(
					origin,
					Box::new(message),
					weight.saturating_sub(execute_weight),
				)?;

				Ok(Some(weight_used.saturating_add(execute_weight)).into())
			},
		)
	}

	/// Send an XCM program from the contract to the specified destination.
	/// See [`pallet_revive_uapi::HostFn::send_xcm`].
	#[mutating]
	fn xcm_send(
		&mut self,
		memory: &mut M,
		dest_ptr: u32,
		dest_len: u32,
		msg_ptr: u32,
		msg_len: u32,
		output_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		use xcm::{VersionedLocation, VersionedXcm};
		use xcm_builder::{SendController, SendControllerWeightInfo};

		self.charge_gas(RuntimeCosts::CopyFromContract(dest_len))?;
		let dest: VersionedLocation = memory.read_as_unbounded(dest_ptr, dest_len)?;

		self.charge_gas(RuntimeCosts::CopyFromContract(msg_len))?;
		let message: VersionedXcm<()> = memory.read_as_unbounded(msg_ptr, msg_len)?;

		let weight = <<E::T as Config>::Xcm as SendController<_>>::WeightInfo::send();
		self.charge_gas(RuntimeCosts::CallRuntime(weight))?;
		let origin = crate::RawOrigin::Signed(self.ext.account_id().clone()).into();

		match <<E::T as Config>::Xcm>::send(origin, dest.into(), message.into()) {
			Ok(message_id) => {
				memory.write(output_ptr, &message_id.encode())?;
				Ok(ReturnErrorCode::Success)
			},
			Err(e) => {
				log::debug!(target: LOG_TARGET, "seal0::xcm_send failed with: {e:?}");
				Ok(ReturnErrorCode::XcmSendFailed)
			},
		}
	}

	/// Retrieves the account id for a specified contract address.
	///
	/// See [`pallet_revive_uapi::HostFn::to_account_id`].
	fn to_account_id(
		&mut self,
		memory: &mut M,
		addr_ptr: u32,
		out_ptr: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::ToAccountId)?;
		let address = memory.read_h160(addr_ptr)?;
		let account_id = self.ext.to_account_id(&address);
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			&account_id.encode(),
			false,
			already_charged,
		)?)
	}
}
