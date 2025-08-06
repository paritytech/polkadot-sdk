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
	precompiles::{All as AllPrecompiles, Precompiles}, vm::{ExecResult, Ext, RuntimeCosts}, AccountIdOf, BalanceOf, CodeInfo, CodeVec, Config, ContractBlob, DispatchError, Error, ExecReturnValue, H256, LOG_TARGET, U256
};
use alloc::vec::Vec;
use instructions::instruction_table;
use pallet_revive_uapi::ReturnFlags;
use revm::{
	bytecode::Bytecode,
	interpreter::{
		host::DummyHost, interpreter::{ExtBytecode, ReturnDataImpl, RuntimeFlags}, interpreter_action::InterpreterAction, interpreter_types::{InputsTr, ReturnData}, CallInput, FrameInput, Gas, InstructionResult, Interpreter, InterpreterResult, InterpreterTypes, SharedMemory, Stack
	},
	primitives::{self, hardfork::SpecId, Address, Bytes},
};
use sp_core::H160;
use sp_runtime::Weight;

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
			behaviour_version: Default::default(),
		};
		let code_hash = H256(sp_io::hashing::keccak_256(&code));
		Ok(ContractBlob { code, code_info, code_hash })
	}
}

/// Calls the EVM interpreter with the provided bytecode and inputs.
pub fn call<'a, E: Ext>(bytecode: Bytecode, ext: &'a mut E, inputs: EVMInputs) -> ExecResult {
	let mut interpreter: Interpreter<EVMInterpreter<'a, E>> = Interpreter {
		gas: Gas::new(ext.gas_meter_mut().evm_engine_fuel_left()),
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

fn out_of_gas_result() -> InterpreterResult {
	InterpreterResult {
			result: InstructionResult::OutOfGas,
			gas: Gas::default(),
			output: Bytes::new(),
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
				// To check what to do with refunded gas
				if interpreter.extend.gas_meter_mut().sync_from_executor(interpreter.gas.remaining() as i64).is_err() {
					return out_of_gas_result();
				}
				return result
			},
			InterpreterAction::NewFrame(frame_input) => {
				match frame_input {
					FrameInput::Call(call_input) => {
						let callee: H160 = call_input.target_address.0.0.into();
						let precompile = <AllPrecompiles<E::T>>::get::<E>(&callee.0);
						match &precompile {
							Some(precompile) if precompile.has_contract_info() => {
								if interpreter.extend.gas_meter_mut().charge(RuntimeCosts::PrecompileWithInfoBase).is_err() {
									return out_of_gas_result();
								}
							},
							Some(_) => { 
								if interpreter.extend.gas_meter_mut().charge(RuntimeCosts::PrecompileBase).is_err() {
									return out_of_gas_result();
								}
							},
							None => {
								if interpreter.extend.gas_meter_mut().charge(RuntimeCosts::CallBase).is_err() {
								return out_of_gas_result();
							}
						},
						};

						let input = match &call_input.input {
							CallInput::Bytes(bytes) => bytes.clone(),
							// Consider the usage fo SharedMemory as REVM is doing
							_ => unimplemented!(),	
						};

						// Interpreter call
						let call_result = interpreter.extend.call(
							Weight::from_parts(call_input.gas_limit, call_input.gas_limit),
							U256::zero(),
							&callee,
							U256::from_little_endian(call_input.call_value().as_le_slice()),
							input.to_vec(),
							// EVM doesn't have an explicit `allows_reentry` flag like pallet-contracts.
							// Re-entrancy is managed by the logic of the contract itself.
							true,
							call_input.is_static,
						);

						let (success, return_data) = match call_result {
							Ok(()) => {
								let return_value = interpreter.extend.last_frame_output();
								(!return_value.did_revert(), return_value.data.clone().into())
							},
							Err(_) => (false, primitives::Bytes::new()),
						};

						// Set the interpreter with the nested frame result
						interpreter.return_data.set_buffer(return_data);
						let _ = interpreter.stack.push(primitives::U256::from(success as u8));

					},
					FrameInput::Create(create_input) => unimplemented!(),
					FrameInput::Empty => unreachable!(),
				}	
			},
		}
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
