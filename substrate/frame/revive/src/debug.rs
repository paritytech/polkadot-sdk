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

use crate::{evm::TracerConfig, Config, DispatchError, Weight};
pub use crate::{
	evm::{CallLog, CallTrace, CallType, EthTraces, Traces},
	exec::{ExecResult, ExportedFunction},
	primitives::ExecReturnValue,
};
use alloc::{format, string::ToString, vec::Vec};
use sp_core::{H160, H256, U256};

/// Umbrella trait for all interfaces that serves for debugging.
pub trait Debugger<T: Config>: CallInterceptor<T> {}
impl<T: Config, V> Debugger<T> for V where V: CallInterceptor<T> {}

/// Defines methods to capture contract calls
pub trait Tracing {
	/// Called before a contract call is executed
	fn enter_child_span(
		&mut self,
		from: H160,
		to: H160,
		is_delegate_call: bool,
		is_read_only: bool,
		value: U256,
		input: &[u8],
		gas: Weight,
	);

	/// Record a log event
	fn log_event(&mut self, event: H160, topics: &[H256], data: &[u8]);

	/// Called after a contract call is executed
	fn exit_child_span(&mut self, output: &ExecReturnValue, gas_left: Weight);

	/// Called when a contract call terminates with an error
	fn exit_child_span_with_error(&mut self, error: DispatchError, gas_left: Weight);

	/// Takes the traces collected by the tracer and resets them.
	fn collect_traces(&mut self) -> Traces;
}

/// Creates a new tracer from the given config.
pub fn make_tracer(config: TracerConfig) -> Box<dyn Tracing> {
	match config {
		TracerConfig::CallTracer { with_logs } => Box::new(CallTracer::new(with_logs)),
	}
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct CallTracer {
	/// Store all in-progress CallTrace instances
	traces: Vec<CallTrace>,
	/// Stack of indices to the current active traces
	current_stack: Vec<usize>,
	/// whether or not to capture logs
	with_log: bool,
}

impl CallTracer {
	pub fn new(with_log: bool) -> Self {
		Self { traces: Vec::new(), current_stack: Vec::new(), with_log }
	}
}

impl Tracing for CallTracer {
	fn enter_child_span(
		&mut self,
		from: H160,
		to: H160,
		is_delegate_call: bool,
		is_read_only: bool,
		value: U256,
		input: &[u8],
		gas_left: Weight,
	) {
		let call_type = if is_read_only {
			CallType::StaticCall
		} else if is_delegate_call {
			CallType::DelegateCall
		} else {
			CallType::Call
		};

		self.traces.push(CallTrace {
			from,
			to,
			value,
			call_type,
			input: input.to_vec(),
			gas: gas_left,
			..Default::default()
		});

		// Push the index onto the stack of the current active trace
		self.current_stack.push(self.traces.len() - 1);
	}

	fn log_event(&mut self, address: H160, topics: &[H256], data: &[u8]) {
		if !self.with_log {
			return;
		}

		let current_index = self.current_stack.last().unwrap();
		let position = self.traces[*current_index].calls.len() as u32;
		let log =
			CallLog { address, topics: topics.to_vec(), data: data.to_vec().into(), position };

		let current_index = *self.current_stack.last().unwrap();
		self.traces[current_index].logs.push(log);
	}

	fn exit_child_span(&mut self, output: &ExecReturnValue, gas_used: Weight) {
		// Set the output of the current trace
		let current_index = self.current_stack.pop().unwrap();
		let trace = &mut self.traces[current_index];
		trace.output = output.clone();
		trace.gas_used = gas_used;

		if output.did_revert() {
			trace.error = Some("execution reverted".to_string());
		}

		//  move the current trace into its parent
		if let Some(parent_index) = self.current_stack.last() {
			let child_trace = self.traces.remove(current_index);
			self.traces[*parent_index].calls.push(child_trace);
		}
	}
	fn exit_child_span_with_error(&mut self, error: DispatchError, gas_used: Weight) {
		// Set the output of the current trace
		let current_index = self.current_stack.pop().unwrap();
		let trace = &mut self.traces[current_index];
		trace.gas_used = gas_used;

		trace.error = match error {
			DispatchError::Module(sp_runtime::ModuleError { message, .. }) =>
				Some(message.unwrap_or_default().to_string()),
			_ => Some(format!("{:?}", error)),
		};

		//  move the current trace into its parent
		if let Some(parent_index) = self.current_stack.last() {
			let child_trace = self.traces.remove(current_index);
			self.traces[*parent_index].calls.push(child_trace);
		}
	}

	fn collect_traces(&mut self) -> Traces {
		let traces = core::mem::take(&mut self.traces);
		Traces::CallTraces(traces)
	}
}

/// Provides an interface for intercepting contract calls.
pub trait CallInterceptor<T: Config> {
	/// Allows to intercept contract calls and decide whether they should be executed or not.
	/// If the call is intercepted, the mocked result of the call is returned.
	///
	/// # Arguments
	///
	/// * `contract_address` - The address of the contract that is about to be executed.
	/// * `entry_point` - Describes whether the call is the constructor or a regular call.
	/// * `input_data` - The raw input data of the call.
	///
	/// # Expected behavior
	///
	/// This method should return:
	/// * `Some(ExecResult)` - if the call should be intercepted and the mocked result of the call
	/// is returned.
	/// * `None` - otherwise, i.e. the call should be executed normally.
	fn intercept_call(
		_contract_address: &H160,
		_entry_point: ExportedFunction,
		_input_data: &[u8],
	) -> Option<ExecResult> {
		None
	}
}

impl<T: Config> CallInterceptor<T> for () {}
