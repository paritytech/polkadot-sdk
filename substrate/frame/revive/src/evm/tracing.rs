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
	evm::{extract_revert_message, CallLog, CallTrace, CallType},
	primitives::ExecReturnValue,
	tracing::Tracer,
	DispatchError, Weight,
};
use alloc::{format, string::ToString, vec::Vec};
use sp_core::{H160, H256, U256};

/// A Tracer that reports logs and nested call traces transactions.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct CallTracer<Gas, GasMapper> {
	/// Map Weight to Gas equivalent.
	gas_mapper: GasMapper,
	/// Store all in-progress CallTrace instances.
	traces: Vec<CallTrace<Gas>>,
	/// Stack of indices to the current active traces.
	current_stack: Vec<usize>,
	/// whether or not to capture logs.
	with_log: bool,
}

impl<Gas, GasMapper> CallTracer<Gas, GasMapper> {
	/// Create a new [`CallTracer`] instance.
	pub fn new(with_log: bool, gas_mapper: GasMapper) -> Self {
		Self { gas_mapper, traces: Vec::new(), current_stack: Vec::new(), with_log }
	}

	/// Collect the traces and return them.
	pub fn collect_traces(&mut self) -> Vec<CallTrace<Gas>> {
		core::mem::take(&mut self.traces)
	}
}

impl<Gas: Default, GasMapper: Fn(Weight) -> Gas> Tracer for CallTracer<Gas, GasMapper> {
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
			gas: (self.gas_mapper)(gas_left),
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
		trace.output = output.data.clone().into();
		trace.gas_used = (self.gas_mapper)(gas_used);

		if output.did_revert() {
			trace.revert_reason = extract_revert_message(&output.data);
			trace.error = Some("execution reverted".to_string());
		}

		//  Move the current trace into its parent
		if let Some(parent_index) = self.current_stack.last() {
			let child_trace = self.traces.remove(current_index);
			self.traces[*parent_index].calls.push(child_trace);
		}
	}
	fn exit_child_span_with_error(&mut self, error: DispatchError, gas_used: Weight) {
		// Set the output of the current trace
		let current_index = self.current_stack.pop().unwrap();
		let trace = &mut self.traces[current_index];
		trace.gas_used = (self.gas_mapper)(gas_used);

		trace.error = match error {
			DispatchError::Module(sp_runtime::ModuleError { message, .. }) =>
				Some(message.unwrap_or_default().to_string()),
			_ => Some(format!("{:?}", error)),
		};

		//  Move the current trace into its parent
		if let Some(parent_index) = self.current_stack.last() {
			let child_trace = self.traces.remove(current_index);
			self.traces[*parent_index].calls.push(child_trace);
		}
	}
}
