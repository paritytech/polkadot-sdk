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
#![allow(unused)] // TODO remove
use crate::{
	exec::ExecError,
	gas, vec,
	vm::{BytecodeType, ExecResult, Ext},
	AccountIdOf, Code, CodeInfo, Config, ContractBlob, DispatchError, Error, ExecReturnValue,
	RuntimeCosts, H256, LOG_TARGET, U256,
};
use alloc::{boxed::Box, vec::Vec};
use core::cmp::min;
use pallet_revive_uapi::ReturnFlags;
use revm::{bytecode::Bytecode, primitives::Bytes};
use sp_core::H160;
use sp_runtime::Weight;

#[cfg(feature = "runtime-benchmarks")]
pub mod instructions;
#[cfg(not(feature = "runtime-benchmarks"))]
mod instructions;

mod interpreter;
mod util;
pub use interpreter::Halt;
use interpreter::{InstructionTable, Interpreter};

mod memory;
use memory::Memory;

mod stack;
use stack::Stack;

mod ext_bytecode;
use ext_bytecode::ExtBytecode;

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
pub fn call<'a, E: Ext>(bytecode: Bytecode, ext: &'a mut E, inputs: &'a [u8]) -> ExecResult {
	todo!()
	// let mut interpreter: Interpreter<EVMInterpreter<'a, E>> = Interpreter {
	// 	gas: Gas::default(),
	// 	bytecode: ExtBytecode::new(bytecode),
	// 	stack: Stack::new(),
	// 	return_data: Default::default(),
	// 	memory: SharedMemory::new(),
	// 	input: inputs,
	// 	runtime_flag: RuntimeFlags { is_static: ext.is_read_only(), spec_id: SpecId::default() },
	// 	extend: ext,
	// };
	//
	// let table = instruction_table::<'a, E>();
	// let result = run(&mut interpreter, &table);
	//
	// instruction_result_into_exec_error::<E>(result.result)
	// 	.map(Err)
	// 	.unwrap_or_else(|| {
	// 		Ok(ExecReturnValue {
	// 			flags: if result.is_revert() { ReturnFlags::REVERT } else { ReturnFlags::empty() },
	// 			data: result.output.to_vec(),
	// 		})
	// 	})
}

/// Runs the EVM interpreter
fn run<'a, E: Ext>(
	interpreter: &mut Interpreter<'a, E>,
	table: &InstructionTable<E>,
) -> ExecResult {
	loop {
		#[cfg(not(feature = "std"))]
		let action = run_plain(interpreter, table);
		#[cfg(feature = "std")]
		let action = run_plain_with_tracing(interpreter, table);
		todo!()
		// match action {
		// 	InterpreterAction::Return(result) => {
		// 		log::trace!(target: LOG_TARGET, "Evm return {:?}", result);
		// 		return result;
		// 	},
		// 	InterpreterAction::NewFrame(frame_input) => match frame_input {
		// 		FrameInput::Call(call_input) => run_call(interpreter, call_input),
		// 		FrameInput::Create(create_input) => run_create(interpreter, create_input),
		// 		FrameInput::Empty => unreachable!(),
		// 	},
		// }
	}
}

/// Re-implementation of REVM run_plain function to add trace logging to our EVM interpreter loop.
/// NB: copied directly from revm tag v82
#[cfg(feature = "std")]
fn run_plain<'a, E: Ext>(
	interpreter: &mut Interpreter<E>,
	table: &InstructionTable<E>,
) -> ExecResult {
	todo!()
	// use crate::{alloc::string::ToString, format};
	// use revm::{
	// 	bytecode::OpCode,
	// 	interpreter::{
	// 		instruction_context::InstructionContext,
	// 		interpreter_types::{Jumps, LoopControl, MemoryTr, StackTr},
	// 	},
	// };
	// while interpreter.bytecode.is_not_end() {
	// 	// Get current opcode.
	// 	let opcode = interpreter.bytecode.opcode();
	//
	// 	// SAFETY: In analysis we are doing padding of bytecode so that we are sure that last
	// 	// byte instruction is STOP so we are safe to just increment program_counter bcs on last
	// 	// instruction it will do noop and just stop execution of this contract
	// 	interpreter.bytecode.relative_jump(1);
	// 	let context = InstructionContext { interpreter, host };
	// 	// Execute instruction.
	// 	instruction_table[opcode as usize](context);
	// }
	// interpreter.bytecode.revert_to_previous_pointer();
	//
	// interpreter.take_next_action()
}

/// Re-implementation of REVM run_plain function to add trace logging to our EVM interpreter loop.
/// NB: copied directly from revm tag v82
#[cfg(feature = "std")]
fn run_plain_with_tracing<'a, E: Ext>(
	interpreter: &mut Interpreter<'a, E>,
	table: &InstructionTable<E>,
) -> ExecResult {
	todo!()
	// use crate::{alloc::string::ToString, format};
	// use revm::{
	// 	bytecode::OpCode,
	// 	interpreter::{
	// 		instruction_context::InstructionContext,
	// 		interpreter_types::{Jumps, LoopControl, MemoryTr, StackTr},
	// 	},
	// };
	// while interpreter.bytecode.is_not_end() {
	// 	log::trace!(target: LOG_TARGET,
	// 		"[{pc}]: {opcode}, stacktop: {stacktop}, memory size: {memsize} {memory:?}",
	// 		pc = interpreter.bytecode.pc(),
	// 		opcode = OpCode::new(interpreter.bytecode.opcode())
	// 			.map_or("INVALID".to_string(), |x| format!("{:?}", x.info())),
	// 		stacktop = interpreter.stack.top().map_or("None".to_string(), |x| format!("{:#x}", x)),
	// 		memsize = interpreter.memory.size(),
	// 		// printing at most the first 32 bytes of memory
	// 		memory = interpreter
	// 			.memory
	// 			.slice_len(0, core::cmp::min(32, interpreter.memory.size()))
	// 			.to_vec(),
	// 	);
	// 	// Get current opcode.
	// 	let opcode = interpreter.bytecode.opcode();
	//
	// 	// SAFETY: In analysis we are doing padding of bytecode so that we are sure that last
	// 	// byte instruction is STOP so we are safe to just increment program_counter bcs on last
	// 	// instruction it will do noop and just stop execution of this contract
	// 	interpreter.bytecode.relative_jump(1);
	// 	let context = InstructionContext { interpreter, host };
	// 	// Execute instruction.
	// 	instruction_table[opcode as usize](context);
	// }
	// interpreter.bytecode.revert_to_previous_pointer();
	//
	// interpreter.take_next_action()
}

// /// Conversion of a `ExecError` to `ReturnErrorCode`.
// ///
// /// Used when converting the error returned from a subcall in order to map it to the
// /// equivalent EVM interpreter [InstructionResult].
// ///
// /// - Returns `None` when the caller can recover the error.
// /// - Otherwise, some [InstructionResult] error code (the halt reason) is returned. Most
// [ExecError] ///   variants don't map to a [InstructionResult]. The conversion is lossy and
// defaults to ///   [InstructionResult::Revert] for most cases.
// ///
// /// Uses the overarching [super::exec_error_into_return_code] method to determine if
// /// the error is recoverable or not. This guarantees consistent behavior accross both
// /// VM backends.
// fn exec_error_into_halt_reason<E: Ext>(from: ExecError) -> Option<InstructionResult> {
// 	log::trace!("call frame execution error in EVM caller: {:?}", &from);
//
// 	if super::exec_error_into_return_code::<E>(from).is_ok() {
// 		return None;
// 	}
//
// 	let static_memory_too_large = Error::<E::T>::StaticMemoryTooLarge.into();
// 	let code_rejected = Error::<E::T>::CodeRejected.into();
// 	let transfer_failed = Error::<E::T>::TransferFailed.into();
// 	let duplicate_contract = Error::<E::T>::DuplicateContract.into();
// 	let balance_conversion_failed = Error::<E::T>::BalanceConversionFailed.into();
// 	let value_too_large = Error::<E::T>::ValueTooLarge.into();
// 	let out_of_gas = Error::<E::T>::OutOfGas.into();
// 	let out_of_deposit = Error::<E::T>::StorageDepositLimitExhausted.into();
//
// 	Some(match from.error {
// 		err if err == static_memory_too_large => InstructionResult::MemoryLimitOOG,
// 		err if err == code_rejected => InstructionResult::OpcodeNotFound,
// 		err if err == transfer_failed => InstructionResult::OutOfFunds,
// 		err if err == duplicate_contract => InstructionResult::CreateCollision,
// 		err if err == balance_conversion_failed => InstructionResult::OverflowPayment,
// 		err if err == value_too_large => InstructionResult::OverflowPayment,
// 		err if err == out_of_deposit => InstructionResult::OutOfFunds,
// 		err if err == out_of_gas => InstructionResult::OutOfGas,
// 		_ => InstructionResult::Revert,
// 	})
// }
//
// /// Map [InstructionResult] into an [ExecError] for passing it up the stack.
// ///
// /// Returns `None` if the instruction result is not an error case.
// fn instruction_result_into_exec_error<E: Ext>(from: InstructionResult) -> Option<ExecError> {
// 	match from {
// 		InstructionResult::OutOfGas |
// 		InstructionResult::InvalidOperandOOG |
// 		InstructionResult::ReentrancySentryOOG |
// 		InstructionResult::PrecompileOOG |
// 		InstructionResult::MemoryOOG => Some(Error::<E::T>::OutOfGas),
// 		InstructionResult::MemoryLimitOOG => Some(Error::<E::T>::StaticMemoryTooLarge),
// 		InstructionResult::OpcodeNotFound |
// 		InstructionResult::InvalidJump |
// 		InstructionResult::NotActivated |
// 		InstructionResult::InvalidFEOpcode |
// 		InstructionResult::CreateContractStartingWithEF => Some(Error::<E::T>::InvalidInstruction),
// 		InstructionResult::CallNotAllowedInsideStatic |
// 		InstructionResult::StateChangeDuringStaticCall => Some(Error::<E::T>::StateChangeDenied),
// 		InstructionResult::StackUnderflow |
// 		InstructionResult::StackOverflow |
// 		InstructionResult::NonceOverflow |
// 		InstructionResult::PrecompileError |
// 		InstructionResult::FatalExternalError => Some(Error::<E::T>::ContractTrapped),
// 		InstructionResult::OutOfOffset => Some(Error::<E::T>::OutOfBounds),
// 		InstructionResult::CreateCollision => Some(Error::<E::T>::DuplicateContract),
// 		InstructionResult::OverflowPayment => Some(Error::<E::T>::BalanceConversionFailed),
// 		InstructionResult::CreateContractSizeLimit | InstructionResult::CreateInitCodeSizeLimit =>
// 			Some(Error::<E::T>::StaticMemoryTooLarge),
// 		InstructionResult::CallTooDeep => Some(Error::<E::T>::MaxCallDepthReached),
// 		InstructionResult::OutOfFunds => Some(Error::<E::T>::TransferFailed),
// 		InstructionResult::CreateInitCodeStartingEF00 |
// 		InstructionResult::InvalidEOFInitCode |
// 		InstructionResult::InvalidExtDelegateCallTarget => Some(Error::<E::T>::ContractTrapped),
// 		InstructionResult::Stop |
// 		InstructionResult::Return |
// 		InstructionResult::Revert |
// 		InstructionResult::SelfDestruct => None,
// 	}
// 	.map(Into::into)
// }
