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

mod instructions;

use crate::{
	vm::{BytecodeType, ExecResult, Ext},
	AccountIdOf, BalanceOf, CodeInfo, CodeVec, Config, ContractBlob, DispatchError, Error,
	ExecReturnValue, H256, LOG_TARGET, U256,
};
use alloc::vec::Vec;
use instructions::instruction_table;
use pallet_revive_uapi::ReturnFlags;
use revm::{
	bytecode::Bytecode,
	interpreter::{
		host::DummyHost,
		interpreter::{ExtBytecode, ReturnDataImpl, RuntimeFlags},
		interpreter_action::InterpreterAction,
		interpreter_types::{InputsTr, Jumps, LoopControl, MemoryTr, StackTr},
		CallInput, Gas, Interpreter, InterpreterResult, InterpreterTypes, SharedMemory, Stack,
	},
	primitives::{self, hardfork::SpecId, Address},
};

impl<T: Config> ContractBlob<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
{
	/// Create a new contract from EVM code.
	pub fn from_evm_init_code(code: Vec<u8>, owner: AccountIdOf<T>) -> Result<Self, DispatchError> {
		use revm::{bytecode::Bytecode, primitives::Bytes};

		let code: CodeVec = code.try_into().map_err(|_| <Error<T>>::BlobTooLarge)?;

		// Also enforce the EIP-3860 limit on initcode size.
		if code.len() > revm::primitives::eip3860::MAX_INITCODE_SIZE {
			return Err(<Error<T>>::BlobTooLarge.into());
		}

		Bytecode::new_raw_checked(Bytes::from(code.to_vec())).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to create evm bytecode from code: {err:?}" );
			<Error<T>>::CodeRejected
		})?;

		let code_len = code.len() as u32;
		let code_info = CodeInfo {
			owner,
			deposit: Default::default(),
			refcount: 0,
			code_len,
			code_type: BytecodeType::Evm,
			behaviour_version: Default::default(),
		};
		let code_hash = H256(sp_io::hashing::keccak_256(&code));
		Ok(ContractBlob { code, code_info, code_hash })
	}
}

/// Calls the EVM interpreter with the provided bytecode and inputs.
pub fn call<'a, E: Ext>(bytecode: Bytecode, ext: &'a mut E, inputs: EVMInputs) -> ExecResult {
	ext.gas_meter_mut().charge_evm_init_cost()?;

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

	if result.is_error() {
		Err(Error::<E::T>::ContractTrapped.into())
	} else {
		Ok(ExecReturnValue {
			flags: if result.is_revert() { ReturnFlags::REVERT } else { ReturnFlags::empty() },
			data: result.output.to_vec(),
		})
	}
}

/// Runs the EVM interpreter
fn run<WIRE: InterpreterTypes>(
	interpreter: &mut Interpreter<WIRE>,
	table: &revm::interpreter::InstructionTable<WIRE, DummyHost>,
) -> InterpreterResult {
	let host = &mut DummyHost {};
	let action = run_plain(interpreter, table, host);
	#[cfg(feature = "std")]
	log::trace!(target: LOG_TARGET, "{:?}", action);
	match action {
		InterpreterAction::Return(result) => return result,
		InterpreterAction::NewFrame(_) => {
			// TODO handle new frame
			InterpreterResult::new(
				revm::interpreter::InstructionResult::FatalExternalError,
				Default::default(),
				interpreter.gas,
			)
		},
	}
}

/// Re-implementation of REVM run_plain function to add trace logging to our EVM interpreter loop.
fn run_plain<WIRE: InterpreterTypes>(
	interpreter: &mut Interpreter<WIRE>,
	instruction_table: &revm::interpreter::InstructionTable<WIRE, DummyHost>,
	host: &mut DummyHost,
) -> InterpreterAction {
	use crate::{alloc::string::ToString, format};
	use revm::bytecode::OpCode;
	while interpreter.bytecode.is_not_end() {
		#[cfg(feature = "std")]
		log::trace!(target: LOG_TARGET, 
			"[{pc}]: {opcode}, stacktop: {stacktop}, memory size: {memsize} {memory:?}",
			pc = interpreter.bytecode.pc(),
			opcode = OpCode::new(interpreter.bytecode.opcode())
				.map_or("INVALID".to_string(), |x| format!("{:?}", x.info())),
			stacktop = interpreter.stack.top().map_or("None".to_string(), |x| format!("{:#x}", x)),
			memsize = interpreter.memory.size(),
			// printing at most the first 32 bytes of memory
			memory = interpreter
				.memory
				.slice_len(0, core::cmp::min(32, interpreter.memory.size()))
				.to_vec(),
		);
		interpreter.step(instruction_table, host);
	}
	interpreter.take_next_action()
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
		// TODO replae by panic once instruction that use call_value are updated
		primitives::U256::ZERO
	}
}
