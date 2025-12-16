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

use crate::{evm::Bytes, primitives::ExecReturnValue, Code, DispatchError, Key, Weight};
use alloc::vec::Vec;
use environmental::environmental;
use sp_core::{H160, H256, U256};

environmental!(tracer: dyn Tracing + 'static);

/// Trace the execution of the given closure.
///
/// # Warning
///
/// Only meant to be called from off-chain code as its additional resource usage is
/// not accounted for in the weights or memory envelope.
pub fn trace<R, F: FnOnce() -> R>(tracer: &mut (dyn Tracing + 'static), f: F) -> R {
	tracer::using_once(tracer, f)
}

/// Run the closure when tracing is enabled.
///
/// This is safe to be called from on-chain code as tracing will never be activated
/// there. Hence the closure is not executed in this case.
pub(crate) fn if_tracing<R, F: FnOnce(&mut (dyn Tracing + 'static)) -> R>(f: F) -> Option<R> {
	tracer::with(f)
}

/// Interface to provide frame trace information for the current execution frame.
pub trait FrameTraceInfo {
	/// Get the amount of gas remaining in the current frame.
	fn gas_left(&self) -> u64;

	/// Get the weight remaining in the current frame.
	fn weight_left(&self) -> Weight;

	/// Get the output from the last frame.
	fn last_frame_output(&self) -> Bytes;
}

/// Interface to provide EVM-specific trace information for the current execution frame.
pub trait EVMFrameTraceInfo: FrameTraceInfo {
	/// Get a snapshot of the memory at this point in execution.
	///
	/// # Parameters
	/// - `limit`: Maximum number of memory words to capture.
	fn memory_snapshot(&self, limit: usize) -> Vec<Bytes>;

	/// Get a snapshot of the stack at this point in execution.
	fn stack_snapshot(&self) -> Vec<Bytes>;
}

/// Defines methods to trace contract interactions.
pub trait Tracing {
	/// Register an address that should be traced.
	///
	/// # Parameters
	/// - `addr`: The address to watch for tracing.
	fn watch_address(&mut self, _addr: &H160) {}

	/// Called before a contract call is executed.
	///
	/// # Parameters
	/// - `from`: The address initiating the call.
	/// - `to`: The address being called.
	/// - `delegate_call`: The original caller if this is a delegate call.
	/// - `is_read_only`: Whether this is a static/read-only call.
	/// - `value`: The amount of value being transferred.
	/// - `input`: The input data for the call.
	/// - `gas_limit`: The gas limit for this call.
	fn enter_child_span(
		&mut self,
		_from: H160,
		_to: H160,
		_delegate_call: Option<H160>,
		_is_read_only: bool,
		_value: U256,
		_input: &[u8],
		_gas_limit: u64,
	) {
	}

	/// Called when a contract terminates (selfdestructs).
	///
	/// # Parameters
	/// - `contract_address`: The address of the contract being destroyed.
	/// - `beneficiary_address`: The address receiving the contract's remaining balance.
	/// - `gas_left`: The amount of gas remaining.
	/// - `value`: The value transferred to the beneficiary.
	fn terminate(
		&mut self,
		_contract_address: H160,
		_beneficiary_address: H160,
		_gas_left: u64,
		_value: U256,
	) {
	}

	/// Record the code and salt for the next contract instantiation.
	///
	/// # Parameters
	/// - `code`: The code being instantiated.
	/// - `salt`: Optional salt for CREATE2 operations.
	fn instantiate_code(&mut self, _code: &Code, _salt: Option<&[u8; 32]>) {}

	/// Called when a balance is read.
	///
	/// # Parameters
	/// - `addr`: The address whose balance was read.
	/// - `value`: The balance value.
	fn balance_read(&mut self, _addr: &H160, _value: U256) {}

	/// Called when contract storage is read.
	///
	/// # Parameters
	/// - `key`: The storage key being read.
	/// - `value`: The value read from storage.
	fn storage_read(&mut self, _key: &Key, _value: Option<&[u8]>) {}

	/// Called when contract storage is written.
	///
	/// # Parameters
	/// - `key`: The storage key being written.
	/// - `old_value`: The previous value at this key.
	/// - `new_value`: The new value being written.
	fn storage_write(
		&mut self,
		_key: &Key,
		_old_value: Option<Vec<u8>>,
		_new_value: Option<&[u8]>,
	) {
	}

	/// Record a log event.
	///
	/// # Parameters
	/// - `event`: The address emitting the event.
	/// - `topics`: The indexed topics for the event.
	/// - `data`: The event data.
	fn log_event(&mut self, _event: H160, _topics: &[H256], _data: &[u8]) {}

	/// Called after a contract call completes successfully.
	///
	/// # Parameters
	/// - `output`: The return value from the call.
	/// - `gas_used`: The amount of gas consumed.
	fn exit_child_span(&mut self, _output: &ExecReturnValue, _gas_used: u64) {}

	/// Called when a contract call terminates with an error.
	///
	/// # Parameters
	/// - `error`: The error that occurred.
	/// - `gas_used`: The amount of gas consumed before the error.
	fn exit_child_span_with_error(&mut self, _error: DispatchError, _gas_used: u64) {}

	/// Check if opcode tracing is enabled.
	///
	/// # Returns
	/// `true` if the tracer wants to trace individual opcodes.
	fn is_opcode_tracing_enabled(&self) -> bool {
		false
	}

	/// Called before an EVM opcode is executed.
	///
	/// # Parameters
	/// - `pc`: The current program counter.
	/// - `opcode`: The opcode being executed.
	/// - `trace_info`: Information about the current execution frame.
	fn enter_opcode(&mut self, _pc: u64, _opcode: u8, _trace_info: &dyn EVMFrameTraceInfo) {}

	/// Called before a PVM syscall is executed.
	///
	/// # Parameters
	/// - `ecall`: The name of the syscall being executed.
	/// - `trace_info`: Information about the current execution frame.
	fn enter_ecall(&mut self, _ecall: &'static str, _trace_info: &dyn FrameTraceInfo) {}

	/// Called after an EVM opcode or PVM syscall is executed to record the gas cost.
	///
	/// # Parameters
	/// - `trace_info`: Information about the current execution frame.
	fn exit_step(&mut self, _trace_info: &dyn FrameTraceInfo) {}
}
