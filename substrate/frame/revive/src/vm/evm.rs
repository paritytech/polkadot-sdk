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

use crate::{
	exec::ExecError,
	vec,
	vm::{BytecodeType, ExecResult, Ext},
	AccountIdOf, Code, CodeInfo, Config, ContractBlob, DispatchError, Error, ExecReturnValue, H256,
	LOG_TARGET, U256,
};
use alloc::{boxed::Box, vec::Vec};
use core::cmp::min;
use instructions::instruction_table;
use pallet_revive_uapi::ReturnFlags;
use revm::{
	bytecode::Bytecode,
	context::CreateScheme,
	interpreter::{
		host::DummyHost,
		interpreter::{ExtBytecode, ReturnDataImpl, RuntimeFlags},
		interpreter_action::InterpreterAction,
		interpreter_types::{InputsTr, MemoryTr, ReturnData},
		CallInput, CallInputs, CallScheme, CreateInputs, FrameInput, Gas, InstructionResult,
		Interpreter, InterpreterResult, InterpreterTypes, SharedMemory, Stack,
	},
	primitives::{self, hardfork::SpecId, Address, Bytes},
};
use sp_core::H160;
use sp_runtime::Weight;

mod instructions;

/// Hard-coded value returned by the EVM `DIFFICULTY` opcode.
///
/// After Ethereum's Merge (Sept 2022), the `DIFFICULTY` opcode was redefined to return
/// `prevrandao`, a randomness value from the beacon chain. In Substrate pallet-revive
/// a fixed constant is returned instead for compatibility with contracts that still read this
/// opcode. The value is aligned with the difficulty hardcoded for PVM contracts.
pub(crate) const DIFFICULTY: u64 = 2500000000000000_u64;

/// The base fee per gas used in the network as defined by EIP-1559.
///
/// For `pallet-revive`, this is hardcoded to 0
pub(crate) const BASE_FEE: U256 = U256::zero();

impl<T: Config> ContractBlob<T> {
	/// Create a new contract from EVM init code.
	pub fn from_evm_init_code(code: Vec<u8>, owner: AccountIdOf<T>) -> Result<Self, DispatchError> {
		if code.len() > revm::primitives::eip3860::MAX_INITCODE_SIZE {
			return Err(<Error<T>>::BlobTooLarge.into());
		}

		// EIP-3541: Reject new contract code starting with the 0xEF byte
		if code.first() == Some(&0xEF) {
			return Err(<Error<T>>::CodeRejected.into());
		}

		let code_len = code.len() as u32;
		let code_info = CodeInfo {
			owner,
			deposit: Default::default(),
			refcount: 0,
			code_len,
			code_type: BytecodeType::Evm,
			behaviour_version: Default::default(),
		};

		Bytecode::new_raw_checked(Bytes::from(code.to_vec())).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to create evm bytecode from init code: {err:?}" );
			<Error<T>>::CodeRejected
		})?;

		// Code hash is not relevant for init code, since it is not stored on-chain.
		let code_hash = H256::default();
		Ok(ContractBlob { code, code_info, code_hash })
	}

	/// Create a new contract from EVM runtime code.
	pub fn from_evm_runtime_code(
		code: Vec<u8>,
		owner: AccountIdOf<T>,
	) -> Result<Self, DispatchError> {
		if code.len() > revm::primitives::eip170::MAX_CODE_SIZE {
			return Err(<Error<T>>::BlobTooLarge.into());
		}

		let code_len = code.len() as u32;
		let deposit = super::calculate_code_deposit::<T>(code_len);

		let code_info = CodeInfo {
			owner,
			deposit,
			refcount: 0,
			code_len,
			code_type: BytecodeType::Evm,
			behaviour_version: Default::default(),
		};

		Bytecode::new_raw_checked(Bytes::from(code.to_vec())).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to create evm bytecode from code: {err:?}" );
			<Error<T>>::CodeRejected
		})?;

		let code_hash = H256(sp_io::hashing::keccak_256(&code));
		Ok(ContractBlob { code, code_info, code_hash })
	}
}

/// Calls the EVM interpreter with the provided bytecode and inputs.
pub fn call<'a, E: Ext>(bytecode: Bytecode, ext: &'a mut E, inputs: EVMInputs) -> ExecResult {
	let mut interpreter: Interpreter<EVMInterpreter<'a, E>> = Interpreter {
		gas: Gas::default(),
		bytecode: ExtBytecode::new(bytecode),
		stack: Stack::new(),
		return_data: Default::default(),
		memory: SharedMemory::new(),
		input: inputs,
		runtime_flag: RuntimeFlags { is_static: false, spec_id: SpecId::default() },
		extend: ext,
	};

	let table = instruction_table::<'a, E>();
	let result = run(&mut interpreter, &table);

	instruction_result_into_exec_error::<E>(result.result)
		.map(Err)
		.unwrap_or_else(|| {
			Ok(ExecReturnValue {
				flags: if result.is_revert() { ReturnFlags::REVERT } else { ReturnFlags::empty() },
				data: result.output.to_vec(),
			})
		})
}

/// Runs the EVM interpreter
fn run<'a, E: Ext>(
	interpreter: &mut Interpreter<EVMInterpreter<'a, E>>,
	table: &revm::interpreter::InstructionTable<EVMInterpreter<'a, E>, DummyHost>,
) -> InterpreterResult {
	let host = &mut DummyHost {};
	loop {
		let action = interpreter.run_plain(table, host);
		match action {
			InterpreterAction::Return(result) => {
				log::trace!(target: LOG_TARGET, "Evm return {:?}", result);
				debug_assert!(
					result.gas.limit() == 0 &&
						result.gas.remaining() == 0 &&
						result.gas.refunded() == 0,
					"Interpreter gas state should remain unchanged; found: {:?}",
					result.gas,
				);
				return result;
			},
			InterpreterAction::NewFrame(frame_input) => match frame_input {
				FrameInput::Call(call_input) => run_call(interpreter, call_input),
				FrameInput::Create(create_input) => run_create(interpreter, create_input),
				FrameInput::Empty => unreachable!(),
			},
		}
	}
}

fn run_call<'a, E: Ext>(
	interpreter: &mut Interpreter<EVMInterpreter<'a, E>>,
	call_input: Box<CallInputs>,
) {
	let callee: H160 = if call_input.scheme.is_delegate_call() {
		call_input.bytecode_address.0 .0.into()
	} else {
		call_input.target_address.0 .0.into()
	};

	let input = match &call_input.input {
		CallInput::Bytes(bytes) => bytes.to_vec(),
		// Consider the usage fo SharedMemory as REVM is doing
		CallInput::SharedBuffer(range) => interpreter.memory.global_slice(range.clone()).to_vec(),
	};
	let call_result = match call_input.scheme {
		CallScheme::Call | CallScheme::StaticCall => interpreter.extend.call(
			Weight::from_parts(call_input.gas_limit, u64::MAX),
			U256::MAX,
			&callee,
			U256::from_revm_u256(&call_input.call_value()),
			input,
			true,
			call_input.is_static,
		),
		CallScheme::CallCode => {
			unreachable!()
		},
		CallScheme::DelegateCall => interpreter.extend.delegate_call(
			Weight::from_parts(call_input.gas_limit, u64::MAX),
			U256::MAX,
			callee,
			input,
		),
	};

	let return_value = interpreter.extend.last_frame_output();
	let return_data: Bytes = return_value.data.clone().into();

	let mem_length = call_input.return_memory_offset.len();
	let mem_start = call_input.return_memory_offset.start;
	let returned_len = return_data.len();
	let target_len = min(mem_length, returned_len);
	// Set the interpreter with the nested frame result
	interpreter.return_data.set_buffer(return_data);

	match call_result {
		Ok(()) => {
			// success or revert
			// TODO: Charge CopyToContract
			interpreter
				.memory
				.set(mem_start, &interpreter.return_data.buffer()[..target_len]);
			let _ =
				interpreter.stack.push(primitives::U256::from(!return_value.did_revert() as u8));
		},
		Err(err) => {
			let _ = interpreter.stack.push(primitives::U256::ZERO);
			if let Some(reason) = exec_error_into_halt_reason::<E>(err) {
				interpreter.halt(reason);
			}
		},
	}
}

fn run_create<'a, E: Ext>(
	interpreter: &mut Interpreter<EVMInterpreter<'a, E>>,
	create_input: Box<CreateInputs>,
) {
	let value = U256::from_revm_u256(&create_input.value);

	let salt = match create_input.scheme {
		CreateScheme::Create => None,
		CreateScheme::Create2 { salt } => Some(salt.to_le_bytes()),
		CreateScheme::Custom { .. } => unreachable!("custom create schemes are not supported"),
	};

	let call_result = interpreter.extend.instantiate(
		Weight::from_parts(create_input.gas_limit, u64::MAX),
		U256::MAX,
		Code::Upload(create_input.init_code.to_vec()),
		value,
		vec![],
		salt.as_ref(),
	);

	let return_value = interpreter.extend.last_frame_output();
	let return_data: Bytes = return_value.data.clone().into();

	match call_result {
		Ok(address) => {
			if return_value.did_revert() {
				// Contract creation reverted — return data must be propagated
				// TODO: Charge CopyToContract
				interpreter.return_data.set_buffer(return_data);
				let _ = interpreter.stack.push(primitives::U256::ZERO);
			} else {
				// Otherwise clear it. Note that RETURN opcode should abort.
				interpreter.return_data.clear();
				let stack_item: Address = address.0.into();
				let _ = interpreter.stack.push(stack_item.into_word().into());
			}
		},
		Err(err) => {
			let _ = interpreter.stack.push(primitives::U256::ZERO);
			if let Some(reason) = exec_error_into_halt_reason::<E>(err) {
				interpreter.halt(reason);
			}
		},
	}
}

/// EVMInterpreter implements the `InterpreterTypes`.
///
/// Note:
///
/// Our implementation set the `InterpreterTypes::Extend` associated type, to the `Ext` trait, to
/// reuse all the host functions that are defined by this trait
pub struct EVMInterpreter<'a, E: Ext> {
	_phantom: core::marker::PhantomData<&'a E>,
}

impl<'a, E: Ext> InterpreterTypes for EVMInterpreter<'a, E> {
	type Stack = Stack;
	type Memory = SharedMemory;
	type Bytecode = ExtBytecode;
	type ReturnData = ReturnDataImpl;
	type Input = EVMInputs;
	type RuntimeFlag = RuntimeFlags;
	type Extend = &'a mut E;
	type Output = InterpreterAction;
}

/// EVMInputs implements the `InputsTr` trait for EVM inputs, allowing the EVM interpreter to access
/// the call input data.
///
/// Note:
///
/// In our implementation of the instruction table, Everything except the call input data will be
/// accessed through the `InterpreterTypes::Extend` associated type, our implementation will panic
/// if any of those methods are called.
#[derive(Debug, Clone, Default)]
pub struct EVMInputs(CallInput);

impl EVMInputs {
	pub fn new(input: Vec<u8>) -> Self {
		Self(CallInput::Bytes(input.into()))
	}
}

impl InputsTr for EVMInputs {
	fn target_address(&self) -> Address {
		panic!()
	}

	fn caller_address(&self) -> Address {
		panic!()
	}

	fn bytecode_address(&self) -> Option<&Address> {
		panic!()
	}

	fn input(&self) -> &CallInput {
		&self.0
	}

	fn call_value(&self) -> primitives::U256 {
		panic!()
	}
}

/// Conversion of a `ExecError` to `ReturnErrorCode`.
///
/// Used when converting the error returned from a subcall in order to map it to the
/// equivalent EVM interpreter [InstructionResult].
///
/// - Returns `None` when the caller can recover the error.
/// - Otherwise, some [InstructionResult] error code (the halt reason) is returned. Most [ExecError]
///   variants don't map to a [InstructionResult]. The conversion is lossy and defaults to
///   [InstructionResult::Revert] for most cases.
///
/// Uses the overarching [super::exec_error_into_return_code] method to determine if
/// the error is recoverable or not. This guarantees consistent behavior accross both
/// VM backends.
fn exec_error_into_halt_reason<E: Ext>(from: ExecError) -> Option<InstructionResult> {
	log::trace!("call frame execution error in EVM caller: {:?}", &from);

	if super::exec_error_into_return_code::<E>(from).is_ok() {
		return None;
	}

	let static_memory_too_large = Error::<E::T>::StaticMemoryTooLarge.into();
	let code_rejected = Error::<E::T>::CodeRejected.into();
	let transfer_failed = Error::<E::T>::TransferFailed.into();
	let duplicate_contract = Error::<E::T>::DuplicateContract.into();
	let balance_conversion_failed = Error::<E::T>::BalanceConversionFailed.into();
	let value_too_large = Error::<E::T>::ValueTooLarge.into();
	let out_of_gas = Error::<E::T>::OutOfGas.into();
	let out_of_deposit = Error::<E::T>::StorageDepositLimitExhausted.into();

	Some(match from.error {
		err if err == static_memory_too_large => InstructionResult::MemoryLimitOOG,
		err if err == code_rejected => InstructionResult::OpcodeNotFound,
		err if err == transfer_failed => InstructionResult::OutOfFunds,
		err if err == duplicate_contract => InstructionResult::CreateCollision,
		err if err == balance_conversion_failed => InstructionResult::OverflowPayment,
		err if err == value_too_large => InstructionResult::OverflowPayment,
		err if err == out_of_deposit => InstructionResult::OutOfFunds,
		err if err == out_of_gas => InstructionResult::OutOfGas,
		_ => InstructionResult::Revert,
	})
}

/// Map [InstructionResult] into an [ExecError] for passing it up the stack.
///
/// Returns `None` if the instruction result is not an error case.
fn instruction_result_into_exec_error<E: Ext>(from: InstructionResult) -> Option<ExecError> {
	match from {
		InstructionResult::OutOfGas |
		InstructionResult::InvalidOperandOOG |
		InstructionResult::ReentrancySentryOOG |
		InstructionResult::PrecompileOOG |
		InstructionResult::MemoryOOG => Some(Error::<E::T>::OutOfGas),
		InstructionResult::MemoryLimitOOG => Some(Error::<E::T>::StaticMemoryTooLarge),
		InstructionResult::OpcodeNotFound |
		InstructionResult::InvalidJump |
		InstructionResult::NotActivated |
		InstructionResult::InvalidFEOpcode |
		InstructionResult::CreateContractStartingWithEF => Some(Error::<E::T>::InvalidInstruction),
		InstructionResult::CallNotAllowedInsideStatic |
		InstructionResult::StateChangeDuringStaticCall => Some(Error::<E::T>::StateChangeDenied),
		InstructionResult::StackUnderflow |
		InstructionResult::StackOverflow |
		InstructionResult::NonceOverflow |
		InstructionResult::PrecompileError |
		InstructionResult::FatalExternalError => Some(Error::<E::T>::ContractTrapped),
		InstructionResult::OutOfOffset => Some(Error::<E::T>::OutOfBounds),
		InstructionResult::CreateCollision => Some(Error::<E::T>::DuplicateContract),
		InstructionResult::OverflowPayment => Some(Error::<E::T>::BalanceConversionFailed),
		InstructionResult::CreateContractSizeLimit | InstructionResult::CreateInitCodeSizeLimit =>
			Some(Error::<E::T>::StaticMemoryTooLarge),
		InstructionResult::CallTooDeep => Some(Error::<E::T>::MaxCallDepthReached),
		InstructionResult::OutOfFunds => Some(Error::<E::T>::TransferFailed),
		InstructionResult::CreateInitCodeStartingEF00 |
		InstructionResult::InvalidEOFInitCode |
		InstructionResult::InvalidExtDelegateCallTarget => Some(Error::<E::T>::ContractTrapped),
		InstructionResult::Stop |
		InstructionResult::Return |
		InstructionResult::Revert |
		InstructionResult::SelfDestruct => None,
	}
	.map(Into::into)
}

/// Blanket conversion trait between `sp_core::U256` and `revm::primitives::U256`
pub trait U256Converter {
	/// Convert `self` into `revm::primitives::U256`
	fn into_revm_u256(&self) -> revm::primitives::U256;

	/// Convert from `revm::primitives::U256` into `Self`
	fn from_revm_u256(value: &revm::primitives::U256) -> Self;
}

impl U256Converter for sp_core::U256 {
	fn into_revm_u256(&self) -> revm::primitives::U256 {
		let bytes = self.to_big_endian();
		revm::primitives::U256::from_be_bytes(bytes)
	}

	fn from_revm_u256(value: &revm::primitives::U256) -> Self {
		let bytes = value.to_be_bytes::<32>();
		sp_core::U256::from_big_endian(&bytes)
	}
}
