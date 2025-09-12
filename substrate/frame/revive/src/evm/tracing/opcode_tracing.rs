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
	tracing::Tracing,
	DispatchError, ExecReturnValue, Weight,
};
use alloc::{
	format,
	string::{String, ToString},
	vec::Vec,
};
use sp_core::{H160, U256};

/// A tracer that traces opcode execution step-by-step.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct OpcodeTracer {
	/// The tracer configuration.
	config: OpcodeTracerConfig,

	/// The collected trace steps.
	steps: Vec<OpcodeStep>,

	/// Current call depth.
	depth: u32,

	/// Number of steps captured (for limiting).
	step_count: u64,

	/// Total gas used by the transaction.
	total_gas_used: u64,

	/// Whether the transaction failed.
	failed: bool,

	/// The return value of the transaction.
	return_value: Vec<u8>,
}

impl OpcodeTracer {
	/// Create a new [`OpcodeTracer`] instance.
	pub fn new(config: OpcodeTracerConfig) -> Self {
		Self {
			config,
			steps: Vec::new(),
			depth: 0,
			step_count: 0,
			total_gas_used: 0,
			failed: false,
			return_value: Vec::new(),
		}
	}

	/// Collect the traces and return them.
	pub fn collect_trace(&mut self) -> OpcodeTrace {
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
	pub fn set_total_gas_used(&mut self, gas_used: u64) {
		self.total_gas_used = gas_used;
	}
}

impl Tracing for OpcodeTracer {
	fn get_opcode_tracer_config(&self) -> Option<crate::evm::OpcodeTracerConfig> {
		Some(self.config.clone())
	}

	fn record_opcode_step(
		&mut self,
		pc: u64,
		opcode: u8,
		gas_before: u64,
		gas_cost: u64,
		depth: u32,
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

		// TODO: Storage capture would need to be implemented based on the EVM storage access
		let storage = if !self.config.disable_storage {
			// For now, return empty storage since we need to track storage changes
			// This would need to be implemented with actual storage change tracking
			Some(alloc::collections::BTreeMap::new())
		} else {
			None
		};

		// Create the opcode step
		let step = OpcodeStep {
			pc,
			op: opcode,
			gas: gas_before,
			gas_cost,
			depth,
			stack: final_stack,
			memory: final_memory,
			storage,
			error: None,
		};

		self.steps.push(step);
		self.step_count += 1;
	}

	fn enter_child_span(
		&mut self,
		_from: H160,
		_to: H160,
		_is_delegate_call: bool,
		_is_read_only: bool,
		_value: U256,
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
			// Convert Weight to gas units - this is a simplified conversion
			self.set_total_gas_used(gas_used.ref_time() / 1_000_000); // Rough conversion
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
			self.set_total_gas_used(gas_used.ref_time() / 1_000_000); // Rough conversion
		}

		if self.depth > 0 {
			self.depth -= 1;
		}
	}
}
