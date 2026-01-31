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
	evm::{
		tracing::Tracing, Bytes, ExecutionStep, ExecutionStepKind, ExecutionTrace,
		ExecutionTracerConfig,
	},
	tracing::{EVMFrameTraceInfo, FrameTraceInfo},
	vm::pvm::env::lookup_syscall_index,
	DispatchError, ExecReturnValue, Key, Weight,
};
use alloc::{
	collections::BTreeMap,
	format,
	string::{String, ToString},
	vec::Vec,
};
use sp_core::{H160, U256};

/// A tracer that traces opcode and syscall execution step-by-step.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct ExecutionTracer {
	/// The tracer configuration.
	config: ExecutionTracerConfig,

	/// The collected trace steps.
	steps: Vec<ExecutionStep>,

	/// Stack of pending step indices awaiting their exit_step call.
	/// When entering an opcode/syscall, we push the step index here.
	/// When exit_step is called, we pop to find the correct step to update.
	pending_steps: Vec<usize>,

	/// Current call depth.
	depth: u16,

	/// Number of steps captured (for limiting).
	step_count: u64,

	/// Total gas used by the transaction.
	total_gas_used: u64,

	/// The base call weight of the transaction.
	base_call_weight: Weight,

	/// The Weight consumed by the transaction meter.
	weight_consumed: Weight,

	/// Whether the transaction failed.
	failed: bool,

	/// The return value of the transaction.
	return_value: Bytes,

	/// List of storage per call
	storages_per_call: Vec<BTreeMap<Bytes, Bytes>>,
}

impl ExecutionTracer {
	/// Create a new [`ExecutionTracer`] instance.
	pub fn new(config: ExecutionTracerConfig) -> Self {
		Self {
			config,
			steps: Vec::new(),
			pending_steps: Vec::new(),
			depth: 0,
			step_count: 0,
			total_gas_used: 0,
			base_call_weight: Default::default(),
			weight_consumed: Default::default(),
			failed: false,
			return_value: Bytes::default(),
			storages_per_call: alloc::vec![Default::default()],
		}
	}

	/// Collect the traces and return them.
	pub fn collect_trace(self) -> ExecutionTrace {
		let Self {
			steps: struct_logs,
			weight_consumed,
			base_call_weight,
			return_value,
			total_gas_used: gas,
			failed,
			..
		} = self;
		ExecutionTrace { gas, weight_consumed, base_call_weight, failed, return_value, struct_logs }
	}

	/// Record an error in the current step.
	fn record_error(&mut self, error: String) {
		if let Some(last_step) = self.steps.last_mut() {
			last_step.error = Some(error);
		}
	}
}

impl Tracing for ExecutionTracer {
	fn is_execution_tracer(&self) -> bool {
		true
	}

	fn dispatch_result(&mut self, base_call_weight: Weight, weight_consumed: Weight) {
		self.base_call_weight = base_call_weight;
		self.weight_consumed = weight_consumed;
	}

	fn enter_opcode(&mut self, pc: u64, opcode: u8, trace_info: &dyn EVMFrameTraceInfo) {
		if self.config.limit.map(|l| self.step_count >= l).unwrap_or(false) {
			return;
		}

		// Extract stack data if enabled
		let stack_data =
			if !self.config.disable_stack { trace_info.stack_snapshot() } else { Vec::new() };

		// Extract memory data if enabled
		let memory_data = if self.config.enable_memory {
			trace_info.memory_snapshot(self.config.memory_word_limit as usize)
		} else {
			Vec::new()
		};

		// Extract return data if enabled
		let return_data = if self.config.enable_return_data {
			trace_info.last_frame_output()
		} else {
			crate::evm::Bytes::default()
		};

		let step = ExecutionStep {
			gas: trace_info.gas_left(),
			gas_cost: Default::default(),
			weight_cost: trace_info.weight_consumed(),
			depth: self.depth,
			return_data,
			error: None,
			kind: ExecutionStepKind::EVMOpcode {
				pc: pc as u32,
				op: opcode,
				stack: stack_data,
				memory: memory_data,
				storage: None,
			},
		};

		// Track this step's index so exit_step can find it even after nested steps are added
		let step_index = self.steps.len();
		self.steps.push(step);
		self.pending_steps.push(step_index);
		self.step_count += 1;
	}

	fn enter_ecall(&mut self, ecall: &'static str, args: &[u64], trace_info: &dyn FrameTraceInfo) {
		if self.config.limit.map(|l| self.step_count >= l).unwrap_or(false) {
			return;
		}

		// Extract return data if enabled
		let return_data = if self.config.enable_return_data {
			trace_info.last_frame_output()
		} else {
			crate::evm::Bytes::default()
		};

		// Extract syscall args if enabled
		let syscall_args =
			if !self.config.disable_syscall_details { args.to_vec() } else { Vec::new() };

		let step = ExecutionStep {
			gas: trace_info.gas_left(),
			gas_cost: Default::default(),
			weight_cost: trace_info.weight_consumed(),
			depth: self.depth,
			return_data,
			error: None,
			kind: ExecutionStepKind::PVMSyscall {
				op: lookup_syscall_index(ecall).unwrap_or_default(),
				args: syscall_args,
				returned: None,
			},
		};

		let step_index = self.steps.len();
		self.steps.push(step);
		self.pending_steps.push(step_index);
		self.step_count += 1;
	}

	fn exit_step(&mut self, trace_info: &dyn FrameTraceInfo, returned: Option<u64>) {
		if let Some(step_index) = self.pending_steps.pop() {
			if let Some(step) = self.steps.get_mut(step_index) {
				// For call/instantiation opcodes, gas_cost was already set in enter_child_span
				// (opcode_cost + gas_forwarded). For other opcodes, calculate it here.
				if step.gas_cost == 0 {
					step.gas_cost = step.gas.saturating_sub(trace_info.gas_left());
				}
				// weight_cost is the total weight consumed (including child calls)
				step.weight_cost = trace_info.weight_consumed().saturating_sub(step.weight_cost);
				if !self.config.disable_syscall_details {
					if let ExecutionStepKind::PVMSyscall { returned: ref mut ret, .. } = step.kind {
						*ret = returned;
					}
				}
			}
		}
	}

	fn enter_child_span(
		&mut self,
		_from: H160,
		_to: H160,
		_delegate_call: Option<H160>,
		_is_read_only: bool,
		_value: U256,
		_input: &[u8],
		gas_limit: u64,
		parent_gas_left: Option<u64>,
	) {
		// Set gas_cost of the pending call/instantiation step.
		// gas_cost = opcode_gas_cost + gas_forwarded
		if let Some(&step_index) = self.pending_steps.last() {
			if let Some(step) = self.steps.get_mut(step_index) {
				if let Some(parent_gas) = parent_gas_left {
					let opcode_gas_cost = step.gas.saturating_sub(parent_gas);
					step.gas_cost = opcode_gas_cost.saturating_add(gas_limit);
				}
			}
		}
		self.storages_per_call.push(Default::default());
		self.depth += 1;
	}

	fn exit_child_span(&mut self, output: &ExecReturnValue, gas_used: u64) {
		if output.did_revert() {
			self.record_error("execution reverted".to_string());
			if self.depth == 0 {
				self.failed = true;
			}
		} else {
			self.return_value = Bytes(output.data.to_vec());
		}

		if self.depth == 1 {
			self.total_gas_used = gas_used;
		}

		self.storages_per_call.pop();

		if self.depth > 0 {
			self.depth -= 1;
		}
	}

	fn exit_child_span_with_error(&mut self, error: DispatchError, gas_used: u64) {
		self.record_error(format!("{:?}", error));

		// Mark as failed if this is the top-level call
		if self.depth == 1 {
			self.failed = true;
			self.total_gas_used = gas_used;
		}

		if self.depth > 0 {
			self.depth -= 1;
		}

		self.storages_per_call.pop();
	}

	fn storage_write(&mut self, key: &Key, _old_value: Option<Vec<u8>>, new_value: Option<&[u8]>) {
		// Only track storage if not disabled
		if self.config.disable_storage {
			return;
		}

		if let Some(storage) = self.storages_per_call.last_mut() {
			let key_bytes = crate::evm::Bytes(key.unhashed().to_vec());
			let value_bytes = crate::evm::Bytes(
				new_value.map(|v| v.to_vec()).unwrap_or_else(|| alloc::vec![0u8; 32]),
			);
			storage.insert(key_bytes, value_bytes);

			if let Some(step) = self.steps.last_mut() {
				if let ExecutionStepKind::EVMOpcode { storage: ref mut step_storage, .. } =
					step.kind
				{
					*step_storage = Some(storage.clone());
				}
			}
		}
	}

	fn storage_read(&mut self, key: &Key, value: Option<&[u8]>) {
		// Only track storage if not disabled
		if self.config.disable_storage {
			return;
		}

		if let Some(storage) = self.storages_per_call.last_mut() {
			let key_bytes = crate::evm::Bytes(key.unhashed().to_vec());
			storage.entry(key_bytes).or_insert_with(|| {
				crate::evm::Bytes(value.map(|v| v.to_vec()).unwrap_or_else(|| alloc::vec![0u8; 32]))
			});

			if let Some(step) = self.steps.last_mut() {
				if let ExecutionStepKind::EVMOpcode { storage: ref mut step_storage, .. } =
					step.kind
				{
					*step_storage = Some(storage.clone());
				}
			}
		}
	}
}
