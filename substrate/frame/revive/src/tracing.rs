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

use crate::{primitives::ExecReturnValue, Code, DispatchError, Key, Weight};
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

/// Defines methods to trace contract interactions.
pub trait Tracing {
	/// Check if this tracer requires opcode-level tracing.
	fn is_opcode_tracer(&self) -> bool { false }

	/// Check if stack capture is enabled for opcode tracing.
	fn is_stack_capture_enabled(&self) -> bool { false }

	/// Check if memory capture is enabled for opcode tracing.
	fn is_memory_capture_enabled(&self) -> bool { false }

	/// Record an opcode step for opcode tracers.
	fn record_opcode_step(
		&mut self,
		_pc: u64,
		_opcode: u8,
		_gas_before: u64,
		_gas_cost: u64,
		_depth: u32,
		_stack: Option<Vec<crate::evm::Bytes>>,
		_memory: Option<Vec<crate::evm::Bytes>>,
	) {}

	/// Register an address that should be traced.
	fn watch_address(&mut self, _addr: &H160) {}

	/// Called before a contract call is executed
	fn enter_child_span(
		&mut self,
		_from: H160,
		_to: H160,
		_is_delegate_call: bool,
		_is_read_only: bool,
		_value: U256,
		_input: &[u8],
		_gas: Weight,
	) {
	}

	/// Record the next code and salt to be instantiated.
	fn instantiate_code(&mut self, _code: &Code, _salt: Option<&[u8; 32]>) {}

	/// Called when a balance is read
	fn balance_read(&mut self, _addr: &H160, _value: U256) {}

	/// Called when storage read is called
	fn storage_read(&mut self, _key: &Key, _value: Option<&[u8]>) {}

	/// Called when storage write is called
	fn storage_write(
		&mut self,
		_key: &Key,
		_old_value: Option<Vec<u8>>,
		_new_value: Option<&[u8]>,
	) {
	}

	/// Record a log event
	fn log_event(&mut self, _event: H160, _topics: &[H256], _data: &[u8]) {}

	/// Called after a contract call is executed
	fn exit_child_span(&mut self, _output: &ExecReturnValue, _gas_left: Weight) {}

	/// Called when a contract call terminates with an error
	fn exit_child_span_with_error(&mut self, _error: DispatchError, _gas_left: Weight) {}
}
