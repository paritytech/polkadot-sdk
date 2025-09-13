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
	evm::{tracing::Tracing, OpcodeStep, OpcodeTrace, OpcodeTracerConfig},
	DispatchError, ExecReturnValue, Weight,
};
use alloc::{
	format,
	string::{String, ToString},
	vec,
	vec::Vec,
};
use revm::interpreter::interpreter_types::MemoryTr;
use sp_core::{H160, U256};

/// A tracer that traces opcode execution step-by-step.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct OpcodeTracer<Gas, GasMapper> {
	/// Map Weight to Gas equivalent.
	gas_mapper: GasMapper,

	/// The tracer configuration.
	config: OpcodeTracerConfig,

	/// The collected trace steps.
	steps: Vec<OpcodeStep<Gas>>,

	/// Current call depth.
	depth: u32,

	/// Number of steps captured (for limiting).
	step_count: u64,

	/// Total gas used by the transaction.
	total_gas_used: Gas,

	/// Whether the transaction failed.
	failed: bool,

	/// The return value of the transaction.
	return_value: Vec<u8>,

	/// Pending step that's waiting for gas cost to be recorded.
	pending_step: Option<OpcodeStep<Gas>>,

	/// Gas before executing the current pending step.
	pending_gas_before: Option<Weight>,
}

impl<Gas, GasMapper> OpcodeTracer<Gas, GasMapper> {
	/// Create a new [`OpcodeTracer`] instance.
	pub fn new(config: OpcodeTracerConfig, gas_mapper: GasMapper) -> Self
	where
		Gas: Default,
	{
		Self {
			gas_mapper,
			config,
			steps: Vec::new(),
			depth: 0,
			step_count: 0,
			total_gas_used: Gas::default(),
			failed: false,
			return_value: Vec::new(),
			pending_step: None,
			pending_gas_before: None,
		}
	}

	/// Collect the traces and return them.
	pub fn collect_trace(&mut self) -> OpcodeTrace<Gas>
	where
		Gas: Copy,
	{
		let struct_logs = core::mem::take(&mut self.steps);
		let return_value = crate::evm::Bytes(self.return_value.clone());

		OpcodeTrace { gas: self.total_gas_used, failed: self.failed, return_value, struct_logs }
	}

	/// Record an error in the current step.
	pub fn record_error(&mut self, error: String) {
		if let Some(last_step) = self.steps.last_mut() {
			last_step.error = Some(error);
		}
	}

	/// Record return data.
	pub fn record_return_data(&mut self, data: &[u8]) {
		self.return_value = data.to_vec();
	}

	/// Mark the transaction as failed.
	pub fn mark_failed(&mut self) {
		self.failed = true;
	}

	/// Set the total gas used by the transaction.
	pub fn set_total_gas_used(&mut self, gas_used: Gas) {
		self.total_gas_used = gas_used;
	}
}

impl<GasMapper: Fn(Weight) -> U256> Tracing for OpcodeTracer<sp_core::U256, GasMapper> {
	fn is_opcode_tracing_enabled(&self) -> bool {
		true
	}

	fn enter_opcode(
		&mut self,
		pc: u64,
		opcode: u8,
		gas_before: Weight,
		stack: &revm::interpreter::Stack,
		memory: &revm::interpreter::SharedMemory,
	) {
		// Check step limit - if exceeded, don't record anything
		if self.config.limit > 0 && self.step_count >= self.config.limit {
			return;
		}

		// Extract stack data if enabled
		let stack_data = if !self.config.disable_stack {
			// Get actual stack values using the data() method
			let stack_values = stack.data();
			let mut stack_bytes = Vec::new();

			// Convert stack values to bytes in reverse order (top of stack first)
			for value in stack_values.iter().rev() {
				let bytes = value.to_be_bytes_vec();
				stack_bytes.push(crate::evm::Bytes(bytes));
			}

			Some(stack_bytes)
		} else {
			None
		};

		// Extract memory data if enabled
		let memory_data = if self.config.enable_memory {
			let memory_size = memory.size();

			if memory_size == 0 {
				Some(Vec::new())
			} else {
				let mut memory_bytes = Vec::new();
				// Read memory in 32-byte chunks, limiting to reasonable size
				let chunks_to_read = core::cmp::min(memory_size / 32 + 1, 16); // Limit to 16 chunks

				for i in 0..chunks_to_read {
					let offset = i * 32;
					let end = core::cmp::min(offset + 32, memory_size);

					if offset < memory_size {
						let slice = memory.slice(offset..end);

						// Convert to bytes, padding to 32 bytes
						let mut chunk_bytes = vec![0u8; 32];
						for (i, &byte) in slice.iter().enumerate().take(32) {
							chunk_bytes[i] = byte;
						}
						memory_bytes.push(crate::evm::Bytes(chunk_bytes));
					}
				}

				Some(memory_bytes)
			}
		} else {
			None
		};

		// Create the pending opcode step (without gas cost)
		let gas_before_mapped = (self.gas_mapper)(gas_before);

		let step = OpcodeStep {
			pc,
			op: opcode,
			gas: gas_before_mapped,
			gas_cost: sp_core::U256::zero(), // Will be set in exit_opcode
			depth: self.depth,
			stack: stack_data,
			memory: memory_data,
			storage: None,
			error: None,
		};

		self.pending_step = Some(step);
		self.pending_gas_before = Some(gas_before);
		self.step_count += 1;
	}

	fn exit_opcode(&mut self, gas_left: Weight) {
		if let Some(mut step) = self.pending_step.take() {
			if let Some(gas_before) = self.pending_gas_before.take() {
				let gas_cost = gas_before.saturating_sub(gas_left);
				let gas_cost_mapped = (self.gas_mapper)(gas_cost);
				step.gas_cost = gas_cost_mapped;
			}
			self.steps.push(step);
		}
	}

	fn enter_child_span(
		&mut self,
		_from: H160,
		_to: H160,
		_is_delegate_call: bool,
		_is_read_only: bool,
		_value: sp_core::U256,
		_input: &[u8],
		_gas_left: Weight,
	) {
		self.depth += 1;
	}

	fn exit_child_span(&mut self, output: &ExecReturnValue, gas_used: Weight) {
		if output.did_revert() {
			self.record_error("execution reverted".to_string());
			if self.depth == 0 {
				self.mark_failed();
			}
		} else {
			self.record_return_data(&output.data);
		}

		// Set total gas used if this is the top-level call (depth 1, will become 0 after decrement)
		if self.depth == 1 {
			self.set_total_gas_used((self.gas_mapper)(gas_used));
		}

		if self.depth > 0 {
			self.depth -= 1;
		}
	}

	fn exit_child_span_with_error(&mut self, error: DispatchError, gas_used: Weight) {
		self.record_error(format!("{:?}", error));

		// Mark as failed if this is the top-level call
		if self.depth == 1 {
			self.mark_failed();
			self.set_total_gas_used((self.gas_mapper)(gas_used));
		}

		if self.depth > 0 {
			self.depth -= 1;
		}
	}
}
