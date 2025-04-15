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
	exec::{ExecError, ExecResult, Ext, Key, TopicOf},
	gas::{ChargedAmount, Token},
	primitives::ExecReturnValue,
	weights::WeightInfo,
	BalanceOf, CodeHash, Config, DebugBufferVec, Error, SENTINEL,
};
use alloc::{boxed::Box, vec, vec::Vec};
use codec::{Decode, DecodeLimit, Encode, MaxEncodedLen};
use core::fmt;
use frame_support::{
	dispatch::DispatchInfo, ensure, pallet_prelude::DispatchResultWithPostInfo, parameter_types,
	traits::Get, weights::Weight,
};
use pallet_contracts_proc_macro::define_env;
use pallet_contracts_uapi::{CallFlags, ReturnFlags};
use sp_io::hashing::{blake2_128, blake2_256, keccak_256, sha2_256};
use sp_runtime::{
	traits::{Bounded, Zero},
	DispatchError, RuntimeDebug,
};
use wasmi::{core::HostError, errors::LinkerError, Linker, Memory, Store};

type CallOf<T> = <T as frame_system::Config>::RuntimeCall;

/// The maximum nesting depth a contract can use when encoding types.
const MAX_DECODE_NESTING: u32 = 256;

/// Passed to [`Environment`] to determine whether it should expose deprecated interfaces.
pub enum AllowDeprecatedInterface {
	/// No deprecated interfaces are exposed.
	No,
	/// Deprecated interfaces are exposed.
	Yes,
}

/// Passed to [`Environment`] to determine whether it should expose unstable interfaces.
pub enum AllowUnstableInterface {
	/// No unstable interfaces are exposed.
	No,
	/// Unstable interfaces are exposed.
	Yes,
}

/// Trait implemented by the [`define_env`](pallet_contracts_proc_macro::define_env) macro for the
/// emitted `Env` struct.
pub trait Environment<HostState> {
	/// Adds all declared functions to the supplied [`Linker`](wasmi::Linker) and
	/// [`Store`](wasmi::Store).
	fn define(
		store: &mut Store<HostState>,
		linker: &mut Linker<HostState>,
		allow_unstable: AllowUnstableInterface,
		allow_deprecated: AllowDeprecatedInterface,
	) -> Result<(), LinkerError>;
}

/// Type of a storage key.
enum KeyType {
	/// Legacy fix sized key `[u8;32]`.
	Fix,
	/// Variable sized key used in transparent hashing,
	/// cannot be larger than MaxStorageKeyLen.
	Var(u32),
}

pub use pallet_contracts_uapi::ReturnErrorCode;

parameter_types! {
	/// Getter types used by [`crate::api_doc::Current::call_runtime`]
	const CallRuntimeFailed: ReturnErrorCode = ReturnErrorCode::CallRuntimeFailed;
	/// Getter types used by [`crate::api_doc::Current::xcm_execute`]
	const XcmExecutionFailed: ReturnErrorCode = ReturnErrorCode::XcmExecutionFailed;
}

impl From<ExecReturnValue> for ReturnErrorCode {
	fn from(from: ExecReturnValue) -> Self {
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

impl HostError for TrapReason {}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Copy, Clone)]
pub enum RuntimeCosts {
	/// Base Weight of calling a host function.
	HostFn,
	/// Weight charged for copying data from the sandbox.
	CopyFromContract(u32),
	/// Weight charged for copying data to the sandbox.
	CopyToContract(u32),
	/// Weight of calling `seal_caller`.
	Caller,
	/// Weight of calling `seal_is_contract`.
	IsContract,
	/// Weight of calling `seal_code_hash`.
	CodeHash,
	/// Weight of calling `seal_own_code_hash`.
	OwnCodeHash,
	/// Weight of calling `seal_caller_is_origin`.
	CallerIsOrigin,
	/// Weight of calling `caller_is_root`.
	CallerIsRoot,
	/// Weight of calling `seal_address`.
	Address,
	/// Weight of calling `seal_gas_left`.
	GasLeft,
	/// Weight of calling `seal_balance`.
	Balance,
	/// Weight of calling `seal_value_transferred`.
	ValueTransferred,
	/// Weight of calling `seal_minimum_balance`.
	MinimumBalance,
	/// Weight of calling `seal_block_number`.
	BlockNumber,
	/// Weight of calling `seal_now`.
	Now,
	/// Weight of calling `seal_weight_to_fee`.
	WeightToFee,
	/// Weight of calling `seal_terminate`, passing the number of locked dependencies.
	Terminate(u32),
	/// Weight of calling `seal_random`. It includes the weight for copying the subject.
	Random,
	/// Weight of calling `seal_deposit_event` with the given number of topics and event size.
	DepositEvent { num_topic: u32, len: u32 },
	/// Weight of calling `seal_debug_message` per byte of passed message.
	DebugMessage(u32),
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
	/// Weight of calling `seal_transfer`.
	Transfer,
	/// Base weight of calling `seal_call`.
	CallBase,
	/// Weight of calling `seal_delegate_call` for the given input size.
	DelegateCallBase,
	/// Weight of the transfer performed during a call.
	CallTransferSurcharge,
	/// Weight per byte that is cloned by supplying the `CLONE_INPUT` flag.
	CallInputCloned(u32),
	/// Weight of calling `seal_instantiate` for the given input length and salt.
	Instantiate { input_data_len: u32, salt_len: u32 },
	/// Weight of calling `seal_hash_sha_256` for the given input size.
	HashSha256(u32),
	/// Weight of calling `seal_hash_keccak_256` for the given input size.
	HashKeccak256(u32),
	/// Weight of calling `seal_hash_blake2_256` for the given input size.
	HashBlake256(u32),
	/// Weight of calling `seal_hash_blake2_128` for the given input size.
	HashBlake128(u32),
	/// Weight of calling `seal_ecdsa_recover`.
	EcdsaRecovery,
	/// Weight of calling `seal_sr25519_verify` for the given input size.
	Sr25519Verify(u32),
	/// Weight charged by a chain extension through `seal_call_chain_extension`.
	ChainExtension(Weight),
	/// Weight charged for calling into the runtime.
	CallRuntime(Weight),
	/// Weight charged for calling xcm_execute.
	CallXcmExecute(Weight),
	/// Weight of calling `seal_set_code_hash`
	SetCodeHash,
	/// Weight of calling `ecdsa_to_eth_address`
	EcdsaToEthAddress,
	/// Weight of calling `reentrance_count`
	ReentranceCount,
	/// Weight of calling `account_reentrance_count`
	AccountReentranceCount,
	/// Weight of calling `instantiation_nonce`
	InstantiationNonce,
	/// Weight of calling `lock_delegate_dependency`
	LockDelegateDependency,
	/// Weight of calling `unlock_delegate_dependency`
	UnlockDelegateDependency,
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
			CopyToContract(len) => T::WeightInfo::seal_input(len),
			CopyFromContract(len) => T::WeightInfo::seal_return(len),
			Caller => T::WeightInfo::seal_caller(),
			IsContract => T::WeightInfo::seal_is_contract(),
			CodeHash => T::WeightInfo::seal_code_hash(),
			OwnCodeHash => T::WeightInfo::seal_own_code_hash(),
			CallerIsOrigin => T::WeightInfo::seal_caller_is_origin(),
			CallerIsRoot => T::WeightInfo::seal_caller_is_root(),
			Address => T::WeightInfo::seal_address(),
			GasLeft => T::WeightInfo::seal_gas_left(),
			Balance => T::WeightInfo::seal_balance(),
			ValueTransferred => T::WeightInfo::seal_value_transferred(),
			MinimumBalance => T::WeightInfo::seal_minimum_balance(),
			BlockNumber => T::WeightInfo::seal_block_number(),
			Now => T::WeightInfo::seal_now(),
			WeightToFee => T::WeightInfo::seal_weight_to_fee(),
			Terminate(locked_dependencies) => T::WeightInfo::seal_terminate(locked_dependencies),
			Random => T::WeightInfo::seal_random(),
			// Given a 2-second block time and hardcoding a `ref_time` of 60,000 picoseconds per
			// byte  (event_ref_time), the max allocation size is 32MB per block.
			DepositEvent { num_topic, len } => T::WeightInfo::seal_deposit_event(num_topic, len)
				.saturating_add(Weight::from_parts(
					T::Schedule::get().limits.event_ref_time.saturating_mul(len.into()),
					0,
				)),
			DebugMessage(len) => T::WeightInfo::seal_debug_message(len),
			SetStorage { new_bytes, old_bytes } =>
				cost_storage!(write, seal_set_storage, new_bytes, old_bytes),
			ClearStorage(len) => cost_storage!(write, seal_clear_storage, len),
			ContainsStorage(len) => cost_storage!(read, seal_contains_storage, len),
			GetStorage(len) => cost_storage!(read, seal_get_storage, len),
			TakeStorage(len) => cost_storage!(write, seal_take_storage, len),
			SetTransientStorage { new_bytes, old_bytes } =>
				cost_storage!(write_transient, seal_set_transient_storage, new_bytes, old_bytes),
			ClearTransientStorage(len) =>
				cost_storage!(write_transient, seal_clear_transient_storage, len),
			ContainsTransientStorage(len) =>
				cost_storage!(read_transient, seal_contains_transient_storage, len),
			GetTransientStorage(len) =>
				cost_storage!(read_transient, seal_get_transient_storage, len),
			TakeTransientStorage(len) =>
				cost_storage!(write_transient, seal_take_transient_storage, len),
			Transfer => T::WeightInfo::seal_transfer(),
			CallBase => T::WeightInfo::seal_call(0, 0),
			DelegateCallBase => T::WeightInfo::seal_delegate_call(),
			CallTransferSurcharge => cost_args!(seal_call, 1, 0),
			CallInputCloned(len) => cost_args!(seal_call, 0, len),
			Instantiate { input_data_len, salt_len } =>
				T::WeightInfo::seal_instantiate(input_data_len, salt_len),
			HashSha256(len) => T::WeightInfo::seal_hash_sha2_256(len),
			HashKeccak256(len) => T::WeightInfo::seal_hash_keccak_256(len),
			HashBlake256(len) => T::WeightInfo::seal_hash_blake2_256(len),
			HashBlake128(len) => T::WeightInfo::seal_hash_blake2_128(len),
			EcdsaRecovery => T::WeightInfo::seal_ecdsa_recover(),
			Sr25519Verify(len) => T::WeightInfo::seal_sr25519_verify(len),
			ChainExtension(weight) | CallRuntime(weight) | CallXcmExecute(weight) => weight,
			SetCodeHash => T::WeightInfo::seal_set_code_hash(),
			EcdsaToEthAddress => T::WeightInfo::seal_ecdsa_to_eth_address(),
			ReentranceCount => T::WeightInfo::seal_reentrance_count(),
			AccountReentranceCount => T::WeightInfo::seal_account_reentrance_count(),
			InstantiationNonce => T::WeightInfo::seal_instantiation_nonce(),
			LockDelegateDependency => T::WeightInfo::lock_delegate_dependency(),
			UnlockDelegateDependency => T::WeightInfo::unlock_delegate_dependency(),
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
	Call { callee_ptr: u32, value_ptr: u32, deposit_ptr: u32, weight: Weight },
	/// Execute deployed code in the context (storage, account ID, value) of the caller contract
	DelegateCall { code_hash_ptr: u32 },
}

impl CallType {
	fn cost(&self) -> RuntimeCosts {
		match self {
			CallType::Call { .. } => RuntimeCosts::CallBase,
			CallType::DelegateCall { .. } => RuntimeCosts::DelegateCallBase,
		}
	}
}

/// This is only appropriate when writing out data of constant size that does not depend on user
/// input. In this case the costs for this copy was already charged as part of the token at
/// the beginning of the API entry point.
fn already_charged(_: u32) -> Option<RuntimeCosts> {
	None
}

/// Can only be used for one call.
pub struct Runtime<'a, E: Ext + 'a> {
	ext: &'a mut E,
	input_data: Option<Vec<u8>>,
	memory: Option<Memory>,
	chain_extension: Option<Box<<E::T as Config>::ChainExtension>>,
}

impl<'a, E: Ext + 'a> Runtime<'a, E> {
	pub fn new(ext: &'a mut E, input_data: Vec<u8>) -> Self {
		Runtime {
			ext,
			input_data: Some(input_data),
			memory: None,
			chain_extension: Some(Box::new(Default::default())),
		}
	}

	pub fn memory(&self) -> Option<Memory> {
		self.memory
	}

	pub fn set_memory(&mut self, memory: Memory) {
		self.memory = Some(memory);
	}

	/// Converts the sandbox result and the runtime state into the execution outcome.
	pub fn to_execution_result(self, sandbox_result: Result<(), wasmi::Error>) -> ExecResult {
		use wasmi::{
			core::TrapCode,
			errors::{ErrorKind, FuelError},
		};
		use TrapReason::*;

		let Err(error) = sandbox_result else {
			// Contract returned from main function -> no data was returned.
			return Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
		};
		if let ErrorKind::Fuel(FuelError::OutOfFuel) = error.kind() {
			// `OutOfGas` when host asks engine to consume more than left in the _store_.
			// We should never get this case, as gas meter is being charged (and hence raises error)
			// first.
			return Err(Error::<E::T>::OutOfGas.into())
		}
		match error.as_trap_code() {
			Some(TrapCode::OutOfFuel) => {
				// `OutOfGas` during engine execution.
				return Err(Error::<E::T>::OutOfGas.into())
			},
			Some(_trap_code) => {
				// Otherwise the trap came from the contract itself.
				return Err(Error::<E::T>::ContractTrapped.into())
			},
			None => {},
		}
		// If we encoded a reason then it is some abort generated by a host function.
		if let Some(reason) = &error.downcast_ref::<TrapReason>() {
			match &reason {
				Return(ReturnData { flags, data }) => {
					let flags =
						ReturnFlags::from_bits(*flags).ok_or(Error::<E::T>::InvalidCallFlags)?;
					return Ok(ExecReturnValue { flags, data: data.to_vec() })
				},
				Termination =>
					return Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() }),
				SupervisorError(error) => return Err((*error).into()),
			}
		}

		// Any other error is returned only if instantiation or linking failed (i.e.
		// wasm binary tried to import a function that is not provided by the host).
		// This shouldn't happen because validation process ought to reject such binaries.
		//
		// Because panics are really undesirable in the runtime code, we treat this as
		// a trap for now. Eventually, we might want to revisit this.
		log::debug!("Code rejected: {:?}", error);
		Err(Error::<E::T>::CodeRejected.into())
	}

	/// Get a mutable reference to the inner `Ext`.
	///
	/// This is mainly for the chain extension to have access to the environment the
	/// contract is executing in.
	pub fn ext(&mut self) -> &mut E {
		self.ext
	}

	/// Charge the gas meter with the specified token.
	///
	/// Returns `Err(HostError)` if there is not enough gas.
	pub fn charge_gas(&mut self, costs: RuntimeCosts) -> Result<ChargedAmount, DispatchError> {
		charge_gas!(self, costs)
	}

	/// Adjust a previously charged amount down to its actual amount.
	///
	/// This is when a maximum a priori amount was charged and then should be partially
	/// refunded to match the actual amount.
	pub fn adjust_gas(&mut self, charged: ChargedAmount, actual_costs: RuntimeCosts) {
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
				if self.ext.debug_buffer_enabled() {
					self.ext.append_debug_buffer("call failed with: ");
					self.ext.append_debug_buffer(e.into());
				};
				Ok(ErrorReturnCode::get())
			},
		}
	}

	/// Read designated chunk from the sandbox memory.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - requested buffer is not within the bounds of the sandbox memory.
	pub fn read_sandbox_memory(
		&self,
		memory: &[u8],
		ptr: u32,
		len: u32,
	) -> Result<Vec<u8>, DispatchError> {
		ensure!(len <= self.ext.schedule().limits.max_memory_size(), Error::<E::T>::OutOfBounds);
		let mut buf = vec![0u8; len as usize];
		self.read_sandbox_memory_into_buf(memory, ptr, buf.as_mut_slice())?;
		Ok(buf)
	}

	/// Read designated chunk from the sandbox memory into the supplied buffer.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - requested buffer is not within the bounds of the sandbox memory.
	pub fn read_sandbox_memory_into_buf(
		&self,
		memory: &[u8],
		ptr: u32,
		buf: &mut [u8],
	) -> Result<(), DispatchError> {
		let ptr = ptr as usize;
		let bound_checked =
			memory.get(ptr..ptr + buf.len()).ok_or_else(|| Error::<E::T>::OutOfBounds)?;
		buf.copy_from_slice(bound_checked);
		Ok(())
	}

	/// Reads and decodes a type with a size fixed at compile time from contract memory.
	///
	/// # Note
	///
	/// The weight of reading a fixed value is included in the overall weight of any
	/// contract callable function.
	pub fn read_sandbox_memory_as<D: Decode + MaxEncodedLen>(
		&self,
		memory: &[u8],
		ptr: u32,
	) -> Result<D, DispatchError> {
		let ptr = ptr as usize;
		let mut bound_checked = memory.get(ptr..).ok_or_else(|| Error::<E::T>::OutOfBounds)?;

		let decoded = D::decode_with_depth_limit(MAX_DECODE_NESTING, &mut bound_checked)
			.map_err(|_| DispatchError::from(Error::<E::T>::DecodingFailed))?;
		Ok(decoded)
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
	/// There must be an extra benchmark for determining the influence of `len` with
	/// regard to the overall weight.
	pub fn read_sandbox_memory_as_unbounded<D: Decode>(
		&self,
		memory: &[u8],
		ptr: u32,
		len: u32,
	) -> Result<D, DispatchError> {
		let ptr = ptr as usize;
		let mut bound_checked =
			memory.get(ptr..ptr + len as usize).ok_or_else(|| Error::<E::T>::OutOfBounds)?;

		let decoded = D::decode_all_with_depth_limit(MAX_DECODE_NESTING, &mut bound_checked)
			.map_err(|_| DispatchError::from(Error::<E::T>::DecodingFailed))?;

		Ok(decoded)
	}

	/// Write the given buffer and its length to the designated locations in sandbox memory and
	/// charge gas according to the token returned by `create_token`.
	//
	/// `out_ptr` is the location in sandbox memory where `buf` should be written to.
	/// `out_len_ptr` is an in-out location in sandbox memory. It is read to determine the
	/// length of the buffer located at `out_ptr`. If that buffer is large enough the actual
	/// `buf.len()` is written to this location.
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
	/// In addition to the error conditions of `write_sandbox_memory` this functions returns
	/// `Err` if the size of the buffer located at `out_ptr` is too small to fit `buf`.
	pub fn write_sandbox_output(
		&mut self,
		memory: &mut [u8],
		out_ptr: u32,
		out_len_ptr: u32,
		buf: &[u8],
		allow_skip: bool,
		create_token: impl FnOnce(u32) -> Option<RuntimeCosts>,
	) -> Result<(), DispatchError> {
		if allow_skip && out_ptr == SENTINEL {
			return Ok(())
		}

		let buf_len = buf.len() as u32;
		let len: u32 = self.read_sandbox_memory_as(memory, out_len_ptr)?;

		if len < buf_len {
			return Err(Error::<E::T>::OutputBufferTooSmall.into())
		}

		if let Some(costs) = create_token(buf_len) {
			self.charge_gas(costs)?;
		}

		self.write_sandbox_memory(memory, out_ptr, buf)?;
		self.write_sandbox_memory(memory, out_len_ptr, &buf_len.encode())
	}

	/// Write the given buffer to the designated location in the sandbox memory.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - designated area is not within the bounds of the sandbox memory.
	fn write_sandbox_memory(
		&self,
		memory: &mut [u8],
		ptr: u32,
		buf: &[u8],
	) -> Result<(), DispatchError> {
		let ptr = ptr as usize;
		let bound_checked =
			memory.get_mut(ptr..ptr + buf.len()).ok_or_else(|| Error::<E::T>::OutOfBounds)?;
		bound_checked.copy_from_slice(buf);
		Ok(())
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
		memory: &mut [u8],
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
		let input = self.read_sandbox_memory(memory, input_ptr, input_len)?;
		// Compute the hash on the input buffer using the given hash function.
		let hash = hash_fn(&input);
		// Write the resulting hash back into the sandboxed output buffer.
		self.write_sandbox_memory(memory, output_ptr, hash.as_ref())?;
		Ok(())
	}

	/// Fallible conversion of `DispatchError` to `ReturnErrorCode`.
	fn err_into_return_code(from: DispatchError) -> Result<ReturnErrorCode, DispatchError> {
		use ReturnErrorCode::*;

		let transfer_failed = Error::<E::T>::TransferFailed.into();
		let no_code = Error::<E::T>::CodeNotFound.into();
		let not_found = Error::<E::T>::ContractNotFound.into();

		match from {
			x if x == transfer_failed => Ok(TransferFailed),
			x if x == no_code => Ok(CodeNotFound),
			x if x == not_found => Ok(NotCallable),
			err => Err(err),
		}
	}

	/// Fallible conversion of a `ExecResult` to `ReturnErrorCode`.
	fn exec_into_return_code(from: ExecResult) -> Result<ReturnErrorCode, DispatchError> {
		use crate::exec::ErrorOrigin::Callee;

		let ExecError { error, origin } = match from {
			Ok(retval) => return Ok(retval.into()),
			Err(err) => err,
		};

		match (error, origin) {
			(_, Callee) => Ok(ReturnErrorCode::CalleeTrapped),
			(err, _) => Self::err_into_return_code(err),
		}
	}
	fn decode_key(
		&self,
		memory: &[u8],
		key_type: KeyType,
		key_ptr: u32,
	) -> Result<crate::exec::Key<E::T>, TrapReason> {
		let res = match key_type {
			KeyType::Fix => {
				let key = self.read_sandbox_memory(memory, key_ptr, 32u32)?;
				Key::try_from_fix(key)
			},
			KeyType::Var(len) => {
				ensure!(
					len <= <<E as Ext>::T as Config>::MaxStorageKeyLen::get(),
					Error::<E::T>::DecodingFailed
				);
				let key = self.read_sandbox_memory(memory, key_ptr, len)?;
				Key::try_from_var(key)
			},
		};

		res.map_err(|_| Error::<E::T>::DecodingFailed.into())
	}

	fn set_storage(
		&mut self,
		memory: &[u8],
		key_type: KeyType,
		key_ptr: u32,
		value_ptr: u32,
		value_len: u32,
	) -> Result<u32, TrapReason> {
		let max_size = self.ext.max_value_size();
		let charged = self
			.charge_gas(RuntimeCosts::SetStorage { new_bytes: value_len, old_bytes: max_size })?;
		if value_len > max_size {
			return Err(Error::<E::T>::ValueTooLarge.into())
		}
		let key = self.decode_key(memory, key_type, key_ptr)?;
		let value = Some(self.read_sandbox_memory(memory, value_ptr, value_len)?);
		let write_outcome = self.ext.set_storage(&key, value, false)?;

		self.adjust_gas(
			charged,
			RuntimeCosts::SetStorage { new_bytes: value_len, old_bytes: write_outcome.old_len() },
		);
		Ok(write_outcome.old_len_with_sentinel())
	}

	fn clear_storage(
		&mut self,
		memory: &[u8],
		key_type: KeyType,
		key_ptr: u32,
	) -> Result<u32, TrapReason> {
		let charged = self.charge_gas(RuntimeCosts::ClearStorage(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_type, key_ptr)?;
		let outcome = self.ext.set_storage(&key, None, false)?;

		self.adjust_gas(charged, RuntimeCosts::ClearStorage(outcome.old_len()));
		Ok(outcome.old_len_with_sentinel())
	}

	fn get_storage(
		&mut self,
		memory: &mut [u8],
		key_type: KeyType,
		key_ptr: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		let charged = self.charge_gas(RuntimeCosts::GetStorage(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_type, key_ptr)?;
		let outcome = self.ext.get_storage(&key);

		if let Some(value) = outcome {
			self.adjust_gas(charged, RuntimeCosts::GetStorage(value.len() as u32));
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
			self.adjust_gas(charged, RuntimeCosts::GetStorage(0));
			Ok(ReturnErrorCode::KeyNotFound)
		}
	}

	fn contains_storage(
		&mut self,
		memory: &[u8],
		key_type: KeyType,
		key_ptr: u32,
	) -> Result<u32, TrapReason> {
		let charged = self.charge_gas(RuntimeCosts::ContainsStorage(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_type, key_ptr)?;
		let outcome = self.ext.get_storage_size(&key);

		self.adjust_gas(charged, RuntimeCosts::ContainsStorage(outcome.unwrap_or(0)));
		Ok(outcome.unwrap_or(SENTINEL))
	}

	fn set_transient_storage(
		&mut self,
		memory: &[u8],
		key_type: KeyType,
		key_ptr: u32,
		value_ptr: u32,
		value_len: u32,
	) -> Result<u32, TrapReason> {
		let max_size = self.ext.max_value_size();
		let charged = self.charge_gas(RuntimeCosts::SetTransientStorage {
			new_bytes: value_len,
			old_bytes: max_size,
		})?;
		if value_len > max_size {
			return Err(Error::<E::T>::ValueTooLarge.into())
		}
		let key = self.decode_key(memory, key_type, key_ptr)?;
		let value = Some(self.read_sandbox_memory(memory, value_ptr, value_len)?);
		let write_outcome = self.ext.set_transient_storage(&key, value, false)?;
		self.adjust_gas(
			charged,
			RuntimeCosts::SetTransientStorage {
				new_bytes: value_len,
				old_bytes: write_outcome.old_len(),
			},
		);
		Ok(write_outcome.old_len_with_sentinel())
	}

	fn clear_transient_storage(
		&mut self,
		memory: &[u8],
		key_type: KeyType,
		key_ptr: u32,
	) -> Result<u32, TrapReason> {
		let charged =
			self.charge_gas(RuntimeCosts::ClearTransientStorage(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_type, key_ptr)?;
		let outcome = self.ext.set_transient_storage(&key, None, false)?;

		self.adjust_gas(charged, RuntimeCosts::ClearTransientStorage(outcome.old_len()));
		Ok(outcome.old_len_with_sentinel())
	}

	fn get_transient_storage(
		&mut self,
		memory: &mut [u8],
		key_type: KeyType,
		key_ptr: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		let charged =
			self.charge_gas(RuntimeCosts::GetTransientStorage(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_type, key_ptr)?;
		let outcome = self.ext.get_transient_storage(&key);

		if let Some(value) = outcome {
			self.adjust_gas(charged, RuntimeCosts::GetTransientStorage(value.len() as u32));
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
			self.adjust_gas(charged, RuntimeCosts::GetTransientStorage(0));
			Ok(ReturnErrorCode::KeyNotFound)
		}
	}

	fn contains_transient_storage(
		&mut self,
		memory: &[u8],
		key_type: KeyType,
		key_ptr: u32,
	) -> Result<u32, TrapReason> {
		let charged =
			self.charge_gas(RuntimeCosts::ContainsTransientStorage(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_type, key_ptr)?;
		let outcome = self.ext.get_transient_storage_size(&key);

		self.adjust_gas(charged, RuntimeCosts::ContainsTransientStorage(outcome.unwrap_or(0)));
		Ok(outcome.unwrap_or(SENTINEL))
	}

	fn take_transient_storage(
		&mut self,
		memory: &mut [u8],
		key_type: KeyType,
		key_ptr: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		let charged =
			self.charge_gas(RuntimeCosts::TakeTransientStorage(self.ext.max_value_size()))?;
		let key = self.decode_key(memory, key_type, key_ptr)?;
		if let crate::storage::WriteOutcome::Taken(value) =
			self.ext.set_transient_storage(&key, None, true)?
		{
			self.adjust_gas(charged, RuntimeCosts::TakeTransientStorage(value.len() as u32));
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
			self.adjust_gas(charged, RuntimeCosts::TakeTransientStorage(0));
			Ok(ReturnErrorCode::KeyNotFound)
		}
	}

	fn call(
		&mut self,
		memory: &mut [u8],
		flags: CallFlags,
		call_type: CallType,
		input_data_ptr: u32,
		input_data_len: u32,
		output_ptr: u32,
		output_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		self.charge_gas(call_type.cost())?;

		let input_data = if flags.contains(CallFlags::CLONE_INPUT) {
			let input = self.input_data.as_ref().ok_or(Error::<E::T>::InputForwarded)?;
			charge_gas!(self, RuntimeCosts::CallInputCloned(input.len() as u32))?;
			input.clone()
		} else if flags.contains(CallFlags::FORWARD_INPUT) {
			self.input_data.take().ok_or(Error::<E::T>::InputForwarded)?
		} else {
			self.charge_gas(RuntimeCosts::CopyFromContract(input_data_len))?;
			self.read_sandbox_memory(memory, input_data_ptr, input_data_len)?
		};

		let call_outcome = match call_type {
			CallType::Call { callee_ptr, value_ptr, deposit_ptr, weight } => {
				let callee: <<E as Ext>::T as frame_system::Config>::AccountId =
					self.read_sandbox_memory_as(memory, callee_ptr)?;
				let deposit_limit: BalanceOf<<E as Ext>::T> = if deposit_ptr == SENTINEL {
					BalanceOf::<<E as Ext>::T>::zero()
				} else {
					self.read_sandbox_memory_as(memory, deposit_ptr)?
				};
				let read_only = flags.contains(CallFlags::READ_ONLY);
				let value: BalanceOf<<E as Ext>::T> =
					self.read_sandbox_memory_as(memory, value_ptr)?;
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
					callee,
					value,
					input_data,
					flags.contains(CallFlags::ALLOW_REENTRY),
					read_only,
				)
			},
			CallType::DelegateCall { code_hash_ptr } => {
				if flags.intersects(CallFlags::ALLOW_REENTRY | CallFlags::READ_ONLY) {
					return Err(Error::<E::T>::InvalidCallFlags.into())
				}
				let code_hash = self.read_sandbox_memory_as(memory, code_hash_ptr)?;
				self.ext.delegate_call(code_hash, input_data)
			},
		};

		// `TAIL_CALL` only matters on an `OK` result. Otherwise the call stack comes to
		// a halt anyways without anymore code being executed.
		if flags.contains(CallFlags::TAIL_CALL) {
			if let Ok(return_value) = call_outcome {
				return Err(TrapReason::Return(ReturnData {
					flags: return_value.flags.bits(),
					data: return_value.data,
				}))
			}
		}

		if let Ok(output) = &call_outcome {
			self.write_sandbox_output(
				memory,
				output_ptr,
				output_len_ptr,
				&output.data,
				true,
				|len| Some(RuntimeCosts::CopyToContract(len)),
			)?;
		}
		Ok(Runtime::<E>::exec_into_return_code(call_outcome)?)
	}

	fn instantiate(
		&mut self,
		memory: &mut [u8],
		code_hash_ptr: u32,
		weight: Weight,
		deposit_ptr: u32,
		value_ptr: u32,
		input_data_ptr: u32,
		input_data_len: u32,
		address_ptr: u32,
		address_len_ptr: u32,
		output_ptr: u32,
		output_len_ptr: u32,
		salt_ptr: u32,
		salt_len: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		self.charge_gas(RuntimeCosts::Instantiate { input_data_len, salt_len })?;
		let deposit_limit: BalanceOf<<E as Ext>::T> = if deposit_ptr == SENTINEL {
			BalanceOf::<<E as Ext>::T>::zero()
		} else {
			self.read_sandbox_memory_as(memory, deposit_ptr)?
		};
		let value: BalanceOf<<E as Ext>::T> = self.read_sandbox_memory_as(memory, value_ptr)?;
		let code_hash: CodeHash<<E as Ext>::T> =
			self.read_sandbox_memory_as(memory, code_hash_ptr)?;
		let input_data = self.read_sandbox_memory(memory, input_data_ptr, input_data_len)?;
		let salt = self.read_sandbox_memory(memory, salt_ptr, salt_len)?;
		let instantiate_outcome =
			self.ext.instantiate(weight, deposit_limit, code_hash, value, input_data, &salt);
		if let Ok((address, output)) = &instantiate_outcome {
			if !output.flags.contains(ReturnFlags::REVERT) {
				self.write_sandbox_output(
					memory,
					address_ptr,
					address_len_ptr,
					&address.encode(),
					true,
					already_charged,
				)?;
			}
			self.write_sandbox_output(
				memory,
				output_ptr,
				output_len_ptr,
				&output.data,
				true,
				|len| Some(RuntimeCosts::CopyToContract(len)),
			)?;
		}
		Ok(Runtime::<E>::exec_into_return_code(instantiate_outcome.map(|(_, retval)| retval))?)
	}

	fn terminate(&mut self, memory: &[u8], beneficiary_ptr: u32) -> Result<(), TrapReason> {
		let count = self.ext.locked_delegate_dependencies_count() as _;
		self.charge_gas(RuntimeCosts::Terminate(count))?;

		let beneficiary: <<E as Ext>::T as frame_system::Config>::AccountId =
			self.read_sandbox_memory_as(memory, beneficiary_ptr)?;
		self.ext.terminate(&beneficiary)?;
		Err(TrapReason::Termination)
	}
}

// This is the API exposed to contracts.
//
// # Note
//
// Any input that leads to a out of bound error (reading or writing) or failing to decode
// data passed to the supervisor will lead to a trap. This is not documented explicitly
// for every function.
#[define_env(doc)]
pub mod env {

	/// Noop function used to benchmark the time it takes to execute an empty function.
	#[cfg(feature = "runtime-benchmarks")]
	#[unstable]
	fn noop(ctx: _, memory: _) -> Result<(), TrapReason> {
		Ok(())
	}

	/// Set the value at the given key in the contract storage.
	/// See [`pallet_contracts_uapi::HostFn::set_storage`]
	#[prefixed_alias]
	#[mutating]
	fn set_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		value_ptr: u32,
		value_len: u32,
	) -> Result<(), TrapReason> {
		ctx.set_storage(memory, KeyType::Fix, key_ptr, value_ptr, value_len).map(|_| ())
	}

	/// Set the value at the given key in the contract storage.
	/// See [`pallet_contracts_uapi::HostFn::set_storage_v1`]
	#[version(1)]
	#[prefixed_alias]
	#[mutating]
	fn set_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		value_ptr: u32,
		value_len: u32,
	) -> Result<u32, TrapReason> {
		ctx.set_storage(memory, KeyType::Fix, key_ptr, value_ptr, value_len)
	}

	/// Set the value at the given key in the contract storage.
	/// See [`pallet_contracts_uapi::HostFn::set_storage_v2`]
	#[version(2)]
	#[prefixed_alias]
	#[mutating]
	fn set_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		key_len: u32,
		value_ptr: u32,
		value_len: u32,
	) -> Result<u32, TrapReason> {
		ctx.set_storage(memory, KeyType::Var(key_len), key_ptr, value_ptr, value_len)
	}

	/// Clear the value at the given key in the contract storage.
	/// See [`pallet_contracts_uapi::HostFn::clear_storage`]
	#[prefixed_alias]
	#[mutating]
	fn clear_storage(ctx: _, memory: _, key_ptr: u32) -> Result<(), TrapReason> {
		ctx.clear_storage(memory, KeyType::Fix, key_ptr).map(|_| ())
	}

	/// Clear the value at the given key in the contract storage.
	/// See [`pallet_contracts_uapi::HostFn::clear_storage_v1`]
	#[version(1)]
	#[prefixed_alias]
	#[mutating]
	fn clear_storage(ctx: _, memory: _, key_ptr: u32, key_len: u32) -> Result<u32, TrapReason> {
		ctx.clear_storage(memory, KeyType::Var(key_len), key_ptr)
	}

	/// Retrieve the value under the given key from storage.
	/// See [`pallet_contracts_uapi::HostFn::get_storage`]
	#[prefixed_alias]
	fn get_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.get_storage(memory, KeyType::Fix, key_ptr, out_ptr, out_len_ptr)
	}

	/// Retrieve the value under the given key from storage.
	/// See [`pallet_contracts_uapi::HostFn::get_storage_v1`]
	#[version(1)]
	#[prefixed_alias]
	fn get_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		key_len: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.get_storage(memory, KeyType::Var(key_len), key_ptr, out_ptr, out_len_ptr)
	}

	/// Checks whether there is a value stored under the given key.
	/// See [`pallet_contracts_uapi::HostFn::contains_storage`]
	#[prefixed_alias]
	fn contains_storage(ctx: _, memory: _, key_ptr: u32) -> Result<u32, TrapReason> {
		ctx.contains_storage(memory, KeyType::Fix, key_ptr)
	}

	/// Checks whether there is a value stored under the given key.
	/// See [`pallet_contracts_uapi::HostFn::contains_storage_v1`]
	#[version(1)]
	#[prefixed_alias]
	fn contains_storage(ctx: _, memory: _, key_ptr: u32, key_len: u32) -> Result<u32, TrapReason> {
		ctx.contains_storage(memory, KeyType::Var(key_len), key_ptr)
	}

	/// Retrieve and remove the value under the given key from storage.
	/// See [`pallet_contracts_uapi::HostFn::take_storage`]
	#[prefixed_alias]
	#[mutating]
	fn take_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		key_len: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		let charged = ctx.charge_gas(RuntimeCosts::TakeStorage(ctx.ext.max_value_size()))?;
		ensure!(
			key_len <= <<E as Ext>::T as Config>::MaxStorageKeyLen::get(),
			Error::<E::T>::DecodingFailed
		);
		let key = ctx.read_sandbox_memory(memory, key_ptr, key_len)?;
		if let crate::storage::WriteOutcome::Taken(value) = ctx.ext.set_storage(
			&Key::<E::T>::try_from_var(key).map_err(|_| Error::<E::T>::DecodingFailed)?,
			None,
			true,
		)? {
			ctx.adjust_gas(charged, RuntimeCosts::TakeStorage(value.len() as u32));
			ctx.write_sandbox_output(memory, out_ptr, out_len_ptr, &value, false, already_charged)?;
			Ok(ReturnErrorCode::Success)
		} else {
			ctx.adjust_gas(charged, RuntimeCosts::TakeStorage(0));
			Ok(ReturnErrorCode::KeyNotFound)
		}
	}

	/// Set the value at the given key in the contract transient storage.
	#[unstable]
	fn set_transient_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		key_len: u32,
		value_ptr: u32,
		value_len: u32,
	) -> Result<u32, TrapReason> {
		ctx.set_transient_storage(memory, KeyType::Var(key_len), key_ptr, value_ptr, value_len)
	}

	/// Clear the value at the given key in the contract storage.
	#[unstable]
	fn clear_transient_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		key_len: u32,
	) -> Result<u32, TrapReason> {
		ctx.clear_transient_storage(memory, KeyType::Var(key_len), key_ptr)
	}

	/// Retrieve the value under the given key from transient storage.
	#[unstable]
	fn get_transient_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		key_len: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.get_transient_storage(memory, KeyType::Var(key_len), key_ptr, out_ptr, out_len_ptr)
	}

	/// Checks whether there is a value stored under the given key in transient storage.
	#[unstable]
	fn contains_transient_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		key_len: u32,
	) -> Result<u32, TrapReason> {
		ctx.contains_transient_storage(memory, KeyType::Var(key_len), key_ptr)
	}

	/// Retrieve and remove the value under the given key from transient storage.
	#[unstable]
	fn take_transient_storage(
		ctx: _,
		memory: _,
		key_ptr: u32,
		key_len: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.take_transient_storage(memory, KeyType::Var(key_len), key_ptr, out_ptr, out_len_ptr)
	}

	/// Transfer some value to another account.
	/// See [`pallet_contracts_uapi::HostFn::transfer`].
	#[prefixed_alias]
	#[mutating]
	fn transfer(
		ctx: _,
		memory: _,
		account_ptr: u32,
		_account_len: u32,
		value_ptr: u32,
		_value_len: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.charge_gas(RuntimeCosts::Transfer)?;
		let callee: <<E as Ext>::T as frame_system::Config>::AccountId =
			ctx.read_sandbox_memory_as(memory, account_ptr)?;
		let value: BalanceOf<<E as Ext>::T> = ctx.read_sandbox_memory_as(memory, value_ptr)?;
		let result = ctx.ext.transfer(&callee, value);
		match result {
			Ok(()) => Ok(ReturnErrorCode::Success),
			Err(err) => {
				let code = Runtime::<E>::err_into_return_code(err)?;
				Ok(code)
			},
		}
	}

	/// Make a call to another contract.
	///
	/// # Note
	///
	/// The values `_callee_len` and `_value_len` are ignored because the encoded sizes of those
	/// types are fixed through [`codec::MaxEncodedLen`]. The fields exist for backwards
	/// compatibility. Consider switching to the newest version of this function.
	#[prefixed_alias]
	fn call(
		ctx: _,
		memory: _,
		callee_ptr: u32,
		_callee_len: u32,
		gas: u64,
		value_ptr: u32,
		_value_len: u32,
		input_data_ptr: u32,
		input_data_len: u32,
		output_ptr: u32,
		output_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.call(
			memory,
			CallFlags::ALLOW_REENTRY,
			CallType::Call {
				callee_ptr,
				value_ptr,
				deposit_ptr: SENTINEL,
				weight: Weight::from_parts(gas, 0),
			},
			input_data_ptr,
			input_data_len,
			output_ptr,
			output_len_ptr,
		)
	}

	/// Make a call to another contract.
	/// See [`pallet_contracts_uapi::HostFn::call_v1`].
	#[version(1)]
	#[prefixed_alias]
	fn call(
		ctx: _,
		memory: _,
		flags: u32,
		callee_ptr: u32,
		gas: u64,
		value_ptr: u32,
		input_data_ptr: u32,
		input_data_len: u32,
		output_ptr: u32,
		output_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.call(
			memory,
			CallFlags::from_bits(flags).ok_or(Error::<E::T>::InvalidCallFlags)?,
			CallType::Call {
				callee_ptr,
				value_ptr,
				deposit_ptr: SENTINEL,
				weight: Weight::from_parts(gas, 0),
			},
			input_data_ptr,
			input_data_len,
			output_ptr,
			output_len_ptr,
		)
	}

	/// Make a call to another contract.
	/// See [`pallet_contracts_uapi::HostFn::call_v2`].
	#[version(2)]
	fn call(
		ctx: _,
		memory: _,
		flags: u32,
		callee_ptr: u32,
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit_ptr: u32,
		value_ptr: u32,
		input_data_ptr: u32,
		input_data_len: u32,
		output_ptr: u32,
		output_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.call(
			memory,
			CallFlags::from_bits(flags).ok_or(Error::<E::T>::InvalidCallFlags)?,
			CallType::Call {
				callee_ptr,
				value_ptr,
				deposit_ptr,
				weight: Weight::from_parts(ref_time_limit, proof_size_limit),
			},
			input_data_ptr,
			input_data_len,
			output_ptr,
			output_len_ptr,
		)
	}

	/// Execute code in the context (storage, caller, value) of the current contract.
	/// See [`pallet_contracts_uapi::HostFn::delegate_call`].
	#[prefixed_alias]
	fn delegate_call(
		ctx: _,
		memory: _,
		flags: u32,
		code_hash_ptr: u32,
		input_data_ptr: u32,
		input_data_len: u32,
		output_ptr: u32,
		output_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.call(
			memory,
			CallFlags::from_bits(flags).ok_or(Error::<E::T>::InvalidCallFlags)?,
			CallType::DelegateCall { code_hash_ptr },
			input_data_ptr,
			input_data_len,
			output_ptr,
			output_len_ptr,
		)
	}

	/// Instantiate a contract with the specified code hash.
	/// See [`pallet_contracts_uapi::HostFn::instantiate`].
	///
	/// # Note
	///
	/// The values `_code_hash_len` and `_value_len` are ignored because the encoded sizes
	/// of those types are fixed through [`codec::MaxEncodedLen`]. The fields exist
	/// for backwards compatibility. Consider switching to the newest version of this function.
	#[prefixed_alias]
	#[mutating]
	fn instantiate(
		ctx: _,
		memory: _,
		code_hash_ptr: u32,
		_code_hash_len: u32,
		gas: u64,
		value_ptr: u32,
		_value_len: u32,
		input_data_ptr: u32,
		input_data_len: u32,
		address_ptr: u32,
		address_len_ptr: u32,
		output_ptr: u32,
		output_len_ptr: u32,
		salt_ptr: u32,
		salt_len: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.instantiate(
			memory,
			code_hash_ptr,
			Weight::from_parts(gas, 0),
			SENTINEL,
			value_ptr,
			input_data_ptr,
			input_data_len,
			address_ptr,
			address_len_ptr,
			output_ptr,
			output_len_ptr,
			salt_ptr,
			salt_len,
		)
	}

	/// Instantiate a contract with the specified code hash.
	/// See [`pallet_contracts_uapi::HostFn::instantiate_v1`].
	#[version(1)]
	#[prefixed_alias]
	#[mutating]
	fn instantiate(
		ctx: _,
		memory: _,
		code_hash_ptr: u32,
		gas: u64,
		value_ptr: u32,
		input_data_ptr: u32,
		input_data_len: u32,
		address_ptr: u32,
		address_len_ptr: u32,
		output_ptr: u32,
		output_len_ptr: u32,
		salt_ptr: u32,
		salt_len: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.instantiate(
			memory,
			code_hash_ptr,
			Weight::from_parts(gas, 0),
			SENTINEL,
			value_ptr,
			input_data_ptr,
			input_data_len,
			address_ptr,
			address_len_ptr,
			output_ptr,
			output_len_ptr,
			salt_ptr,
			salt_len,
		)
	}

	/// Instantiate a contract with the specified code hash.
	/// See [`pallet_contracts_uapi::HostFn::instantiate_v2`].
	#[version(2)]
	#[mutating]
	fn instantiate(
		ctx: _,
		memory: _,
		code_hash_ptr: u32,
		ref_time_limit: u64,
		proof_size_limit: u64,
		deposit_ptr: u32,
		value_ptr: u32,
		input_data_ptr: u32,
		input_data_len: u32,
		address_ptr: u32,
		address_len_ptr: u32,
		output_ptr: u32,
		output_len_ptr: u32,
		salt_ptr: u32,
		salt_len: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.instantiate(
			memory,
			code_hash_ptr,
			Weight::from_parts(ref_time_limit, proof_size_limit),
			deposit_ptr,
			value_ptr,
			input_data_ptr,
			input_data_len,
			address_ptr,
			address_len_ptr,
			output_ptr,
			output_len_ptr,
			salt_ptr,
			salt_len,
		)
	}

	/// Remove the calling account and transfer remaining balance.
	/// See [`pallet_contracts_uapi::HostFn::terminate`].
	///
	/// # Note
	///
	/// The value `_beneficiary_len` is ignored because the encoded sizes
	/// this type is fixed through `[`MaxEncodedLen`]. The field exist for backwards
	/// compatibility. Consider switching to the newest version of this function.
	#[prefixed_alias]
	#[mutating]
	fn terminate(
		ctx: _,
		memory: _,
		beneficiary_ptr: u32,
		_beneficiary_len: u32,
	) -> Result<(), TrapReason> {
		ctx.terminate(memory, beneficiary_ptr)
	}

	/// Remove the calling account and transfer remaining **free** balance.
	/// See [`pallet_contracts_uapi::HostFn::terminate_v1`].
	#[version(1)]
	#[prefixed_alias]
	#[mutating]
	fn terminate(ctx: _, memory: _, beneficiary_ptr: u32) -> Result<(), TrapReason> {
		ctx.terminate(memory, beneficiary_ptr)
	}

	/// Stores the input passed by the caller into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::input`].
	#[prefixed_alias]
	fn input(ctx: _, memory: _, out_ptr: u32, out_len_ptr: u32) -> Result<(), TrapReason> {
		if let Some(input) = ctx.input_data.take() {
			ctx.write_sandbox_output(memory, out_ptr, out_len_ptr, &input, false, |len| {
				Some(RuntimeCosts::CopyToContract(len))
			})?;
			ctx.input_data = Some(input);
			Ok(())
		} else {
			Err(Error::<E::T>::InputForwarded.into())
		}
	}

	/// Cease contract execution and save a data buffer as a result of the execution.
	/// See [`pallet_contracts_uapi::HostFn::return_value`].
	fn seal_return(
		ctx: _,
		memory: _,
		flags: u32,
		data_ptr: u32,
		data_len: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::CopyFromContract(data_len))?;
		Err(TrapReason::Return(ReturnData {
			flags,
			data: ctx.read_sandbox_memory(memory, data_ptr, data_len)?,
		}))
	}

	/// Stores the address of the caller into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::caller`].
	#[prefixed_alias]
	fn caller(ctx: _, memory: _, out_ptr: u32, out_len_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::Caller)?;
		let caller = ctx.ext.caller().account_id()?.clone();
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&caller.encode(),
			false,
			already_charged,
		)?)
	}

	/// Checks whether a specified address belongs to a contract.
	/// See [`pallet_contracts_uapi::HostFn::is_contract`].
	#[prefixed_alias]
	fn is_contract(ctx: _, memory: _, account_ptr: u32) -> Result<u32, TrapReason> {
		ctx.charge_gas(RuntimeCosts::IsContract)?;
		let address: <<E as Ext>::T as frame_system::Config>::AccountId =
			ctx.read_sandbox_memory_as(memory, account_ptr)?;

		Ok(ctx.ext.is_contract(&address) as u32)
	}

	/// Retrieve the code hash for a specified contract address.
	/// See [`pallet_contracts_uapi::HostFn::code_hash`].
	#[prefixed_alias]
	fn code_hash(
		ctx: _,
		memory: _,
		account_ptr: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.charge_gas(RuntimeCosts::CodeHash)?;
		let address: <<E as Ext>::T as frame_system::Config>::AccountId =
			ctx.read_sandbox_memory_as(memory, account_ptr)?;
		if let Some(value) = ctx.ext.code_hash(&address) {
			ctx.write_sandbox_output(
				memory,
				out_ptr,
				out_len_ptr,
				&value.encode(),
				false,
				already_charged,
			)?;
			Ok(ReturnErrorCode::Success)
		} else {
			Ok(ReturnErrorCode::KeyNotFound)
		}
	}

	/// Retrieve the code hash of the currently executing contract.
	/// See [`pallet_contracts_uapi::HostFn::own_code_hash`].
	#[prefixed_alias]
	fn own_code_hash(ctx: _, memory: _, out_ptr: u32, out_len_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::OwnCodeHash)?;
		let code_hash_encoded = &ctx.ext.own_code_hash().encode();
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			code_hash_encoded,
			false,
			already_charged,
		)?)
	}

	/// Checks whether the caller of the current contract is the origin of the whole call stack.
	/// See [`pallet_contracts_uapi::HostFn::caller_is_origin`].
	#[prefixed_alias]
	fn caller_is_origin(ctx: _, _memory: _) -> Result<u32, TrapReason> {
		ctx.charge_gas(RuntimeCosts::CallerIsOrigin)?;
		Ok(ctx.ext.caller_is_origin() as u32)
	}

	/// Checks whether the caller of the current contract is root.
	/// See [`pallet_contracts_uapi::HostFn::caller_is_root`].
	fn caller_is_root(ctx: _, _memory: _) -> Result<u32, TrapReason> {
		ctx.charge_gas(RuntimeCosts::CallerIsRoot)?;
		Ok(ctx.ext.caller_is_root() as u32)
	}

	/// Stores the address of the current contract into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::address`].
	#[prefixed_alias]
	fn address(ctx: _, memory: _, out_ptr: u32, out_len_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::Address)?;
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&ctx.ext.address().encode(),
			false,
			already_charged,
		)?)
	}

	/// Stores the price for the specified amount of gas into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::weight_to_fee`].
	#[prefixed_alias]
	fn weight_to_fee(
		ctx: _,
		memory: _,
		gas: u64,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<(), TrapReason> {
		let gas = Weight::from_parts(gas, 0);
		ctx.charge_gas(RuntimeCosts::WeightToFee)?;
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&ctx.ext.get_weight_price(gas).encode(),
			false,
			already_charged,
		)?)
	}

	/// Stores the price for the specified amount of weight into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::weight_to_fee_v1`].
	#[version(1)]
	#[unstable]
	fn weight_to_fee(
		ctx: _,
		memory: _,
		ref_time_limit: u64,
		proof_size_limit: u64,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<(), TrapReason> {
		let weight = Weight::from_parts(ref_time_limit, proof_size_limit);
		ctx.charge_gas(RuntimeCosts::WeightToFee)?;
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&ctx.ext.get_weight_price(weight).encode(),
			false,
			already_charged,
		)?)
	}

	/// Stores the weight left into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::gas_left`].
	#[prefixed_alias]
	fn gas_left(ctx: _, memory: _, out_ptr: u32, out_len_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::GasLeft)?;
		let gas_left = &ctx.ext.gas_meter().gas_left().ref_time().encode();
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			gas_left,
			false,
			already_charged,
		)?)
	}

	/// Stores the amount of weight left into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::gas_left_v1`].
	#[version(1)]
	#[unstable]
	fn gas_left(ctx: _, memory: _, out_ptr: u32, out_len_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::GasLeft)?;
		let gas_left = &ctx.ext.gas_meter().gas_left().encode();
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			gas_left,
			false,
			already_charged,
		)?)
	}

	/// Stores the *free* balance of the current account into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::balance`].
	#[prefixed_alias]
	fn balance(ctx: _, memory: _, out_ptr: u32, out_len_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::Balance)?;
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&ctx.ext.balance().encode(),
			false,
			already_charged,
		)?)
	}

	/// Stores the value transferred along with this call/instantiate into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::value_transferred`].
	#[prefixed_alias]
	fn value_transferred(
		ctx: _,
		memory: _,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::ValueTransferred)?;
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&ctx.ext.value_transferred().encode(),
			false,
			already_charged,
		)?)
	}

	/// Stores a random number for the current block and the given subject into the supplied buffer.
	///
	/// The value is stored to linear memory at the address pointed to by `out_ptr`.
	/// `out_len_ptr` must point to a u32 value that describes the available space at
	/// `out_ptr`. This call overwrites it with the size of the value. If the available
	/// space at `out_ptr` is less than the size of the value a trap is triggered.
	///
	/// The data is encoded as `T::Hash`.
	#[prefixed_alias]
	#[deprecated]
	fn random(
		ctx: _,
		memory: _,
		subject_ptr: u32,
		subject_len: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::Random)?;
		if subject_len > ctx.ext.schedule().limits.subject_len {
			return Err(Error::<E::T>::RandomSubjectTooLong.into())
		}
		let subject_buf = ctx.read_sandbox_memory(memory, subject_ptr, subject_len)?;
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&ctx.ext.random(&subject_buf).0.encode(),
			false,
			already_charged,
		)?)
	}

	/// Stores a random number for the current block and the given subject into the supplied buffer.
	///
	/// The value is stored to linear memory at the address pointed to by `out_ptr`.
	/// `out_len_ptr` must point to a u32 value that describes the available space at
	/// `out_ptr`. This call overwrites it with the size of the value. If the available
	/// space at `out_ptr` is less than the size of the value a trap is triggered.
	///
	/// The data is encoded as (T::Hash, frame_system::pallet_prelude::BlockNumberFor::<T>).
	///
	/// # Changes from v0
	///
	/// In addition to the seed it returns the block number since which it was determinable
	/// by chain observers.
	///
	/// # Note
	///
	/// The returned seed should only be used to distinguish commitments made before
	/// the returned block number. If the block number is too early (i.e. commitments were
	/// made afterwards), then ensure no further commitments may be made and repeatedly
	/// call this on later blocks until the block number returned is later than the latest
	/// commitment.
	#[version(1)]
	#[prefixed_alias]
	#[deprecated]
	fn random(
		ctx: _,
		memory: _,
		subject_ptr: u32,
		subject_len: u32,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::Random)?;
		if subject_len > ctx.ext.schedule().limits.subject_len {
			return Err(Error::<E::T>::RandomSubjectTooLong.into())
		}
		let subject_buf = ctx.read_sandbox_memory(memory, subject_ptr, subject_len)?;
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&ctx.ext.random(&subject_buf).encode(),
			false,
			already_charged,
		)?)
	}

	/// Load the latest block timestamp into the supplied buffer
	/// See [`pallet_contracts_uapi::HostFn::now`].
	#[prefixed_alias]
	fn now(ctx: _, memory: _, out_ptr: u32, out_len_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::Now)?;
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&ctx.ext.now().encode(),
			false,
			already_charged,
		)?)
	}

	/// Stores the minimum balance (a.k.a. existential deposit) into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::minimum_balance`].
	#[prefixed_alias]
	fn minimum_balance(
		ctx: _,
		memory: _,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::MinimumBalance)?;
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&ctx.ext.minimum_balance().encode(),
			false,
			already_charged,
		)?)
	}

	/// Stores the tombstone deposit into the supplied buffer.
	///
	/// The value is stored to linear memory at the address pointed to by `out_ptr`.
	/// `out_len_ptr` must point to a u32 value that describes the available space at
	/// `out_ptr`. This call overwrites it with the size of the value. If the available
	/// space at `out_ptr` is less than the size of the value a trap is triggered.
	///
	/// # Note
	///
	/// There is no longer a tombstone deposit. This function always returns `0`.
	#[prefixed_alias]
	#[deprecated]
	fn tombstone_deposit(
		ctx: _,
		memory: _,
		out_ptr: u32,
		out_len_ptr: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::Balance)?;
		let deposit = <BalanceOf<E::T>>::zero().encode();
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&deposit,
			false,
			already_charged,
		)?)
	}

	/// Was used to restore the given destination contract sacrificing the caller.
	///
	/// # Note
	///
	/// The state rent functionality was removed. This is stub only exists for
	/// backwards compatibility
	#[prefixed_alias]
	#[deprecated]
	fn restore_to(
		ctx: _,
		memory: _,
		_dest_ptr: u32,
		_dest_len: u32,
		_code_hash_ptr: u32,
		_code_hash_len: u32,
		_rent_allowance_ptr: u32,
		_rent_allowance_len: u32,
		_delta_ptr: u32,
		_delta_count: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::DebugMessage(0))?;
		Ok(())
	}

	/// Was used to restore the given destination contract sacrificing the caller.
	///
	/// # Note
	///
	/// The state rent functionality was removed. This is stub only exists for
	/// backwards compatibility
	#[version(1)]
	#[prefixed_alias]
	#[deprecated]
	fn restore_to(
		ctx: _,
		memory: _,
		_dest_ptr: u32,
		_code_hash_ptr: u32,
		_rent_allowance_ptr: u32,
		_delta_ptr: u32,
		_delta_count: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::DebugMessage(0))?;
		Ok(())
	}

	/// Was used to set rent allowance of the contract.
	///
	/// # Note
	///
	/// The state rent functionality was removed. This is stub only exists for
	/// backwards compatibility.
	#[prefixed_alias]
	#[deprecated]
	fn set_rent_allowance(
		ctx: _,
		memory: _,
		_value_ptr: u32,
		_value_len: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::DebugMessage(0))?;
		Ok(())
	}

	/// Was used to set rent allowance of the contract.
	///
	/// # Note
	///
	/// The state rent functionality was removed. This is stub only exists for
	/// backwards compatibility.
	#[version(1)]
	#[prefixed_alias]
	#[deprecated]
	fn set_rent_allowance(ctx: _, _memory: _, _value_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::DebugMessage(0))?;
		Ok(())
	}

	/// Was used to store the rent allowance into the supplied buffer.
	///
	/// # Note
	///
	/// The state rent functionality was removed. This is stub only exists for
	/// backwards compatibility.
	#[prefixed_alias]
	#[deprecated]
	fn rent_allowance(ctx: _, memory: _, out_ptr: u32, out_len_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::Balance)?;
		let rent_allowance = <BalanceOf<E::T>>::max_value().encode();
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&rent_allowance,
			false,
			already_charged,
		)?)
	}

	/// Deposit a contract event with the data buffer and optional list of topics.
	/// See [pallet_contracts_uapi::HostFn::deposit_event]
	#[prefixed_alias]
	#[mutating]
	fn deposit_event(
		ctx: _,
		memory: _,
		topics_ptr: u32,
		topics_len: u32,
		data_ptr: u32,
		data_len: u32,
	) -> Result<(), TrapReason> {
		let num_topic = topics_len
			.checked_div(core::mem::size_of::<TopicOf<E::T>>() as u32)
			.ok_or("Zero sized topics are not allowed")?;
		ctx.charge_gas(RuntimeCosts::DepositEvent { num_topic, len: data_len })?;
		if data_len > ctx.ext.max_value_size() {
			return Err(Error::<E::T>::ValueTooLarge.into())
		}

		let topics: Vec<TopicOf<<E as Ext>::T>> = match topics_len {
			0 => Vec::new(),
			_ => ctx.read_sandbox_memory_as_unbounded(memory, topics_ptr, topics_len)?,
		};

		// If there are more than `event_topics`, then trap.
		if topics.len() > ctx.ext.schedule().limits.event_topics as usize {
			return Err(Error::<E::T>::TooManyTopics.into())
		}

		let event_data = ctx.read_sandbox_memory(memory, data_ptr, data_len)?;

		ctx.ext.deposit_event(topics, event_data);

		Ok(())
	}

	/// Stores the current block number of the current contract into the supplied buffer.
	/// See [`pallet_contracts_uapi::HostFn::block_number`].
	#[prefixed_alias]
	fn block_number(ctx: _, memory: _, out_ptr: u32, out_len_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::BlockNumber)?;
		Ok(ctx.write_sandbox_output(
			memory,
			out_ptr,
			out_len_ptr,
			&ctx.ext.block_number().encode(),
			false,
			already_charged,
		)?)
	}

	/// Computes the SHA2 256-bit hash on the given input buffer.
	/// See [`pallet_contracts_uapi::HostFn::hash_sha2_256`].
	#[prefixed_alias]
	fn hash_sha2_256(
		ctx: _,
		memory: _,
		input_ptr: u32,
		input_len: u32,
		output_ptr: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::HashSha256(input_len))?;
		Ok(ctx.compute_hash_on_intermediate_buffer(
			memory, sha2_256, input_ptr, input_len, output_ptr,
		)?)
	}

	/// Computes the KECCAK 256-bit hash on the given input buffer.
	/// See [`pallet_contracts_uapi::HostFn::hash_keccak_256`].
	#[prefixed_alias]
	fn hash_keccak_256(
		ctx: _,
		memory: _,
		input_ptr: u32,
		input_len: u32,
		output_ptr: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::HashKeccak256(input_len))?;
		Ok(ctx.compute_hash_on_intermediate_buffer(
			memory, keccak_256, input_ptr, input_len, output_ptr,
		)?)
	}

	/// Computes the BLAKE2 256-bit hash on the given input buffer.
	/// See [`pallet_contracts_uapi::HostFn::hash_blake2_256`].
	#[prefixed_alias]
	fn hash_blake2_256(
		ctx: _,
		memory: _,
		input_ptr: u32,
		input_len: u32,
		output_ptr: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::HashBlake256(input_len))?;
		Ok(ctx.compute_hash_on_intermediate_buffer(
			memory, blake2_256, input_ptr, input_len, output_ptr,
		)?)
	}

	/// Computes the BLAKE2 128-bit hash on the given input buffer.
	/// See [`pallet_contracts_uapi::HostFn::hash_blake2_128`].
	#[prefixed_alias]
	fn hash_blake2_128(
		ctx: _,
		memory: _,
		input_ptr: u32,
		input_len: u32,
		output_ptr: u32,
	) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::HashBlake128(input_len))?;
		Ok(ctx.compute_hash_on_intermediate_buffer(
			memory, blake2_128, input_ptr, input_len, output_ptr,
		)?)
	}

	/// Call into the chain extension provided by the chain if any.
	/// See [`pallet_contracts_uapi::HostFn::call_chain_extension`].
	#[prefixed_alias]
	fn call_chain_extension(
		ctx: _,
		memory: _,
		id: u32,
		input_ptr: u32,
		input_len: u32,
		output_ptr: u32,
		output_len_ptr: u32,
	) -> Result<u32, TrapReason> {
		use crate::chain_extension::{ChainExtension, Environment, RetVal};
		if !<E::T as Config>::ChainExtension::enabled() {
			return Err(Error::<E::T>::NoChainExtension.into())
		}
		let mut chain_extension = ctx.chain_extension.take().expect(
			"Constructor initializes with `Some`. This is the only place where it is set to `None`.\
			It is always reset to `Some` afterwards. qed"
		);
		let env =
			Environment::new(ctx, memory, id, input_ptr, input_len, output_ptr, output_len_ptr);
		let ret = match chain_extension.call(env)? {
			RetVal::Converging(val) => Ok(val),
			RetVal::Diverging { flags, data } =>
				Err(TrapReason::Return(ReturnData { flags: flags.bits(), data })),
		};
		ctx.chain_extension = Some(chain_extension);
		ret
	}

	/// Emit a custom debug message.
	///
	/// No newlines are added to the supplied message.
	/// Specifying invalid UTF-8 just drops the message with no trap.
	///
	/// This is a no-op if debug message recording is disabled which is always the case
	/// when the code is executing on-chain. The message is interpreted as UTF-8 and
	/// appended to the debug buffer which is then supplied to the calling RPC client.
	///
	/// # Note
	///
	/// Even though no action is taken when debug message recording is disabled there is still
	/// a non trivial overhead (and weight cost) associated with calling this function. Contract
	/// languages should remove calls to this function (either at runtime or compile time) when
	/// not being executed as an RPC. For example, they could allow users to disable logging
	/// through compile time flags (cargo features) for on-chain deployment. Additionally, the
	/// return value of this function can be cached in order to prevent further calls at runtime.
	#[prefixed_alias]
	fn debug_message(
		ctx: _,
		memory: _,
		str_ptr: u32,
		str_len: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		let str_len = str_len.min(DebugBufferVec::<E::T>::bound() as u32);
		ctx.charge_gas(RuntimeCosts::DebugMessage(str_len))?;
		if ctx.ext.append_debug_buffer("") {
			let data = ctx.read_sandbox_memory(memory, str_ptr, str_len)?;
			if let Some(msg) = core::str::from_utf8(&data).ok() {
				ctx.ext.append_debug_buffer(msg);
			}
		}
		Ok(ReturnErrorCode::Success)
	}

	/// Call some dispatchable of the runtime.
	/// See [`frame_support::traits::call_runtime`].
	#[mutating]
	fn call_runtime(
		ctx: _,
		memory: _,
		call_ptr: u32,
		call_len: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		use frame_support::dispatch::GetDispatchInfo;
		ctx.charge_gas(RuntimeCosts::CopyFromContract(call_len))?;
		let call: <E::T as Config>::RuntimeCall =
			ctx.read_sandbox_memory_as_unbounded(memory, call_ptr, call_len)?;
		ctx.call_dispatchable::<CallRuntimeFailed>(
			call.get_dispatch_info(),
			RuntimeCosts::CallRuntime,
			|ctx| ctx.ext.call_runtime(call),
		)
	}

	/// Execute an XCM program locally, using the contract's address as the origin.
	/// See [`pallet_contracts_uapi::HostFn::execute_xcm`].
	#[mutating]
	fn xcm_execute(
		ctx: _,
		memory: _,
		msg_ptr: u32,
		msg_len: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		use frame_support::dispatch::DispatchInfo;
		use xcm::VersionedXcm;
		use xcm_builder::{ExecuteController, ExecuteControllerWeightInfo};

		ctx.charge_gas(RuntimeCosts::CopyFromContract(msg_len))?;
		let message: VersionedXcm<CallOf<E::T>> =
			ctx.read_sandbox_memory_as_unbounded(memory, msg_ptr, msg_len)?;

		let execute_weight =
			<<E::T as Config>::Xcm as ExecuteController<_, _>>::WeightInfo::execute();
		let weight = ctx.ext.gas_meter().gas_left().max(execute_weight);
		let dispatch_info = DispatchInfo { call_weight: weight, ..Default::default() };

		ctx.call_dispatchable::<XcmExecutionFailed>(
			dispatch_info,
			RuntimeCosts::CallXcmExecute,
			|ctx| {
				let origin = crate::RawOrigin::Signed(ctx.ext.address().clone()).into();
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
	/// See [`pallet_contracts_uapi::HostFn::send_xcm`].
	#[mutating]
	fn xcm_send(
		ctx: _,
		memory: _,
		dest_ptr: u32,
		msg_ptr: u32,
		msg_len: u32,
		output_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		use xcm::{VersionedLocation, VersionedXcm};
		use xcm_builder::{SendController, SendControllerWeightInfo};

		ctx.charge_gas(RuntimeCosts::CopyFromContract(msg_len))?;
		let dest: VersionedLocation = ctx.read_sandbox_memory_as(memory, dest_ptr)?;

		let message: VersionedXcm<()> =
			ctx.read_sandbox_memory_as_unbounded(memory, msg_ptr, msg_len)?;
		let weight = <<E::T as Config>::Xcm as SendController<_>>::WeightInfo::send();
		ctx.charge_gas(RuntimeCosts::CallRuntime(weight))?;
		let origin = crate::RawOrigin::Signed(ctx.ext.address().clone()).into();

		match <<E::T as Config>::Xcm>::send(origin, dest.into(), message.into()) {
			Ok(message_id) => {
				ctx.write_sandbox_memory(memory, output_ptr, &message_id.encode())?;
				Ok(ReturnErrorCode::Success)
			},
			Err(e) => {
				if ctx.ext.append_debug_buffer("") {
					ctx.ext.append_debug_buffer("seal0::xcm_send failed with: ");
					ctx.ext.append_debug_buffer(e.into());
				};
				Ok(ReturnErrorCode::XcmSendFailed)
			},
		}
	}

	/// Recovers the ECDSA public key from the given message hash and signature.
	/// See [`pallet_contracts_uapi::HostFn::ecdsa_recover`].
	#[prefixed_alias]
	fn ecdsa_recover(
		ctx: _,
		memory: _,
		signature_ptr: u32,
		message_hash_ptr: u32,
		output_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.charge_gas(RuntimeCosts::EcdsaRecovery)?;

		let mut signature: [u8; 65] = [0; 65];
		ctx.read_sandbox_memory_into_buf(memory, signature_ptr, &mut signature)?;
		let mut message_hash: [u8; 32] = [0; 32];
		ctx.read_sandbox_memory_into_buf(memory, message_hash_ptr, &mut message_hash)?;

		let result = ctx.ext.ecdsa_recover(&signature, &message_hash);

		match result {
			Ok(pub_key) => {
				// Write the recovered compressed ecdsa public key back into the sandboxed output
				// buffer.
				ctx.write_sandbox_memory(memory, output_ptr, pub_key.as_ref())?;

				Ok(ReturnErrorCode::Success)
			},
			Err(_) => Ok(ReturnErrorCode::EcdsaRecoveryFailed),
		}
	}

	/// Verify a sr25519 signature
	/// See [`pallet_contracts_uapi::HostFn::sr25519_verify`].
	fn sr25519_verify(
		ctx: _,
		memory: _,
		signature_ptr: u32,
		pub_key_ptr: u32,
		message_len: u32,
		message_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.charge_gas(RuntimeCosts::Sr25519Verify(message_len))?;

		let mut signature: [u8; 64] = [0; 64];
		ctx.read_sandbox_memory_into_buf(memory, signature_ptr, &mut signature)?;

		let mut pub_key: [u8; 32] = [0; 32];
		ctx.read_sandbox_memory_into_buf(memory, pub_key_ptr, &mut pub_key)?;

		let message: Vec<u8> = ctx.read_sandbox_memory(memory, message_ptr, message_len)?;

		if ctx.ext.sr25519_verify(&signature, &message, &pub_key) {
			Ok(ReturnErrorCode::Success)
		} else {
			Ok(ReturnErrorCode::Sr25519VerifyFailed)
		}
	}

	/// Replace the contract code at the specified address with new code.
	/// See [`pallet_contracts_uapi::HostFn::set_code_hash`].
	#[prefixed_alias]
	#[mutating]
	fn set_code_hash(ctx: _, memory: _, code_hash_ptr: u32) -> Result<ReturnErrorCode, TrapReason> {
		ctx.charge_gas(RuntimeCosts::SetCodeHash)?;
		let code_hash: CodeHash<<E as Ext>::T> =
			ctx.read_sandbox_memory_as(memory, code_hash_ptr)?;
		match ctx.ext.set_code_hash(code_hash) {
			Err(err) => {
				let code = Runtime::<E>::err_into_return_code(err)?;
				Ok(code)
			},
			Ok(()) => Ok(ReturnErrorCode::Success),
		}
	}

	/// Calculates Ethereum address from the ECDSA compressed public key and stores
	/// See [`pallet_contracts_uapi::HostFn::ecdsa_to_eth_address`].
	#[prefixed_alias]
	fn ecdsa_to_eth_address(
		ctx: _,
		memory: _,
		key_ptr: u32,
		out_ptr: u32,
	) -> Result<ReturnErrorCode, TrapReason> {
		ctx.charge_gas(RuntimeCosts::EcdsaToEthAddress)?;
		let mut compressed_key: [u8; 33] = [0; 33];
		ctx.read_sandbox_memory_into_buf(memory, key_ptr, &mut compressed_key)?;
		let result = ctx.ext.ecdsa_to_eth_address(&compressed_key);
		match result {
			Ok(eth_address) => {
				ctx.write_sandbox_memory(memory, out_ptr, eth_address.as_ref())?;
				Ok(ReturnErrorCode::Success)
			},
			Err(_) => Ok(ReturnErrorCode::EcdsaRecoveryFailed),
		}
	}

	/// Returns the number of times the currently executing contract exists on the call stack in
	/// addition to the calling instance.
	/// See [`pallet_contracts_uapi::HostFn::reentrance_count`].
	#[unstable]
	fn reentrance_count(ctx: _, memory: _) -> Result<u32, TrapReason> {
		ctx.charge_gas(RuntimeCosts::ReentranceCount)?;
		Ok(ctx.ext.reentrance_count())
	}

	/// Returns the number of times specified contract exists on the call stack. Delegated calls are
	/// not counted as separate calls.
	/// See [`pallet_contracts_uapi::HostFn::account_reentrance_count`].
	#[unstable]
	fn account_reentrance_count(ctx: _, memory: _, account_ptr: u32) -> Result<u32, TrapReason> {
		ctx.charge_gas(RuntimeCosts::AccountReentranceCount)?;
		let account_id: <<E as Ext>::T as frame_system::Config>::AccountId =
			ctx.read_sandbox_memory_as(memory, account_ptr)?;
		Ok(ctx.ext.account_reentrance_count(&account_id))
	}

	/// Returns a nonce that is unique per contract instantiation.
	/// See [`pallet_contracts_uapi::HostFn::instantiation_nonce`].
	fn instantiation_nonce(ctx: _, _memory: _) -> Result<u64, TrapReason> {
		ctx.charge_gas(RuntimeCosts::InstantiationNonce)?;
		Ok(ctx.ext.nonce())
	}

	/// Adds a new delegate dependency to the contract.
	/// See [`pallet_contracts_uapi::HostFn::lock_delegate_dependency`].
	#[mutating]
	fn lock_delegate_dependency(ctx: _, memory: _, code_hash_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::LockDelegateDependency)?;
		let code_hash = ctx.read_sandbox_memory_as(memory, code_hash_ptr)?;
		ctx.ext.lock_delegate_dependency(code_hash)?;
		Ok(())
	}

	/// Removes the delegate dependency from the contract.
	/// see [`pallet_contracts_uapi::HostFn::unlock_delegate_dependency`].
	#[mutating]
	fn unlock_delegate_dependency(ctx: _, memory: _, code_hash_ptr: u32) -> Result<(), TrapReason> {
		ctx.charge_gas(RuntimeCosts::UnlockDelegateDependency)?;
		let code_hash = ctx.read_sandbox_memory_as(memory, code_hash_ptr)?;
		ctx.ext.unlock_delegate_dependency(&code_hash)?;
		Ok(())
	}
}
