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
	evm::{OpcodeStep, OpcodeTrace, OpcodeTracerConfig},
	DispatchError, ExecReturnValue, Weight,
};
use alloc::{
	format,
	string::{String, ToString},
	vec::Vec,
};
use sp_core::H160;

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

impl<GasMapper: Fn(Weight) -> sp_core::U256> crate::tracing::OpcodeTracing
	for OpcodeTracer<sp_core::U256, GasMapper>
{
	fn stack_recording_enabled(&self) -> bool {
		!self.config.disable_stack
	}

	fn memory_recording_enabled(&self) -> bool {
		self.config.enable_memory
	}

	fn record_opcode_step(
		&mut self,
		pc: u64,
		opcode: u8,
		gas_before: Weight,
		gas_cost: Weight,
		stack: Option<Vec<crate::evm::Bytes>>,
		memory: Option<Vec<crate::evm::Bytes>>,
	) {
		// Check step limit
		if self.config.limit > 0 && self.step_count >= self.config.limit {
			return;
		}

		// Apply configuration settings
		let final_stack = if self.config.disable_stack { None } else { stack };

		let final_memory = if self.config.enable_memory { memory } else { None };

		// TODO: Storage capture

		// Create the opcode step
		let gas_before_mapped = (self.gas_mapper)(gas_before);
		let gas_cost_mapped = (self.gas_mapper)(gas_cost);

		let step = OpcodeStep {
			pc,
			op: opcode,
			gas: gas_before_mapped,
			gas_cost: gas_cost_mapped,
			depth: self.depth,
			stack: final_stack,
			memory: final_memory,
			storage: None,
			error: None,
		};

		self.steps.push(step);
		self.step_count += 1;
	}
}

impl<GasMapper: Fn(Weight) -> sp_core::U256 + 'static> crate::tracing::Tracing
	for OpcodeTracer<sp_core::U256, GasMapper>
{
	fn as_opcode_tracer(&mut self) -> Option<&mut dyn crate::tracing::OpcodeTracing> {
		Some(self)
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
