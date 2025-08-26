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
	exec::ExecError,
	vec,
	vm::{BytecodeType, ExecResult, Ext},
	AccountIdOf, BalanceOf, Code, CodeInfo, CodeVec, Config, ContractBlob, DispatchError, Error,
	ExecReturnValue, H256, LOG_TARGET, U256,
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

/// EVM max deployed runtime code size (EIP-170).
/// 24,576 bytes (0x6000).
pub const EVM_BYTECODE_LIMIT: usize = 24_576;

/// EVM max initcode size (EIP-3860).
/// 49,152 bytes (0xC000).
pub const EVM_INITCODE_LIMIT: usize = EVM_BYTECODE_LIMIT * 2;

impl<T: Config> ContractBlob<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
{
	/// Create a new contract from EVM code.
	pub fn from_evm_code(code: Vec<u8>, owner: AccountIdOf<T>) -> Result<Self, DispatchError> {
		use revm::{bytecode::Bytecode, primitives::Bytes};

		let code: CodeVec = code.try_into().map_err(|_| <Error<T>>::BlobTooLarge)?;
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

/// Runs the EVM interpreter until it returns an action.
fn run<'a, E: Ext>(
	interpreter: &mut Interpreter<EVMInterpreter<'a, E>>,
	table: &revm::interpreter::InstructionTable<EVMInterpreter<'a, E>, DummyHost>,
) -> InterpreterResult {
	let host = &mut DummyHost {};
	loop {
		let action = interpreter.run_plain(table, host);
		match action {
			InterpreterAction::Return(result) => {
				log::info!("Return {:?}", result);
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
			U256::from_little_endian(call_input.call_value().as_le_slice()),
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
			interpreter
				.memory
				.set(mem_start, &interpreter.return_data.buffer()[..target_len]);
			let _ =
				interpreter.stack.push(primitives::U256::from(!return_value.did_revert() as u8));
		},
		Err(err) => {
			let _ = interpreter.stack.push(primitives::U256::ZERO);
			interpreter.halt(exec_error_into_halt_reason::<E>(err));
		},
	}
}

fn run_create<'a, E: Ext>(
	interpreter: &mut Interpreter<EVMInterpreter<'a, E>>,
	create_input: Box<CreateInputs>,
) {
	let value = U256::from_little_endian(create_input.value.as_le_slice());

	let salt = match create_input.scheme {
		CreateScheme::Create => None,
		CreateScheme::Create2 { salt } => {
			let mut arr = [0u8; 32];
			arr.copy_from_slice(salt.as_le_slice());
			Some(arr)
		},
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
			interpreter.return_data.clear();
			interpreter.halt(exec_error_into_halt_reason::<E>(err));
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

/// Infallible conversion of a `ExecError` to `ReturnErrorCode`.
///
/// This is used when converting the error returned from a subcall in order to map
/// it to the equivalent EVM interpreter [InstructionResult].
///
/// Most [ExecError] variants don't map to a [InstructionResult]. The conversion is
/// lossy and defaults to [InstructionResult::Revert] for most cases.
fn exec_error_into_halt_reason<E: Ext>(from: ExecError) -> InstructionResult {
	use crate::exec::ErrorOrigin::Callee;

	let static_memory_too_large = Error::<E::T>::StaticMemoryTooLarge.into();
	let code_rejected = Error::<E::T>::CodeRejected.into();
	let transfer_failed = Error::<E::T>::TransferFailed.into();
	let duplicate_contract = Error::<E::T>::DuplicateContract.into();
	let unsupported_precompile = Error::<E::T>::UnsupportedPrecompileAddress.into();
	let out_of_bounds = Error::<E::T>::OutOfBounds.into();
	let value_too_large = Error::<E::T>::ValueTooLarge.into();
	let out_of_gas = Error::<E::T>::OutOfGas.into();
	let out_of_deposit = Error::<E::T>::StorageDepositLimitExhausted.into();

	// errors in the callee do not trap the caller
	match (from.error, from.origin) {
		(err, _) if err == static_memory_too_large => InstructionResult::MemoryOOG,
		(err, _) if err == code_rejected => InstructionResult::OpcodeNotFound,
		(err, _) if err == transfer_failed => InstructionResult::OutOfFunds,
		(err, _) if err == duplicate_contract => InstructionResult::CreateCollision,
		(err, _) if err == unsupported_precompile => InstructionResult::PrecompileError,
		(err, _) if err == out_of_bounds => InstructionResult::OutOfOffset,
		(err, _) if err == value_too_large => InstructionResult::OverflowPayment,
		(err, Callee) if err == out_of_deposit => InstructionResult::OutOfFunds,
		(err, Callee) if err == out_of_gas => InstructionResult::OutOfGas,
		_ => InstructionResult::Revert,
	}
}
