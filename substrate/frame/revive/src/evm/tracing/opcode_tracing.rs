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
	ExecReturnValue, Weight, DispatchError,
};
use alloc::{collections::BTreeMap, format, string::String, string::ToString, vec::Vec};
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
}

impl OpcodeTracer {
	/// Create a new [`OpcodeTracer`] instance.
	pub fn new(config: OpcodeTracerConfig) -> Self {
		Self {
			config,
			steps: Vec::new(),
			depth: 0,
			step_count: 0,
		}
	}

	/// Collect the traces and return them.
	pub fn collect_trace(&mut self) -> OpcodeTrace {
		let steps = core::mem::take(&mut self.steps);
		OpcodeTrace { steps }
	}


	/// Record an error in the current step.
	pub fn record_error(&mut self, error: String) {
		if let Some(last_step) = self.steps.last_mut() {
			last_step.error = Some(error);
		}
	}

	/// Record return data.
	pub fn record_return_data(&mut self, data: &[u8]) {
		if self.config.enable_return_data {
			if let Some(last_step) = self.steps.last_mut() {
				last_step.return_data = Some(format!("0x{}", alloy_core::hex::encode(data)));
			}
		}
	}
}

impl Tracing for OpcodeTracer {
	fn is_opcode_tracer(&self) -> bool { 
		true 
	}

	fn record_opcode_step(
		&mut self,
		pc: u64,
		opcode: &str,
		gas_before: u64,
		gas_cost: u64,
		depth: u32,
		stack: Option<Vec<String>>,
		memory: Option<Vec<String>>,
	) {
		// Check step limit
		if self.config.limit > 0 && self.step_count >= self.config.limit {
			return;
		}

		// Apply configuration settings
		let final_stack = if self.config.disable_stack {
			None
		} else {
			stack
		};
		
		let final_memory = if self.config.enable_memory {
			memory
		} else {
			None
		};

		// TODO: Storage capture would need to be implemented based on the EVM storage access
		let storage = if !self.config.disable_storage {
			// For now, return empty storage since we need to track storage changes
			// This would need to be implemented with actual storage change tracking
			Some(BTreeMap::new())
		} else {
			None
		};

		// Create the opcode step
		let step = OpcodeStep {
			pc,
			op: opcode.to_string(),
			gas: U256::from(gas_before),
			gas_cost: U256::from(gas_cost),
			depth,
			stack: final_stack,
			memory: final_memory,
			storage,
			error: None,
			return_data: None, // This would be set on return operations
		};

		self.steps.push(step);
		self.step_count += 1;

		// Debug output if enabled
		if self.config.debug {
			println!("OPCODE TRACE: PC={}, OP={}, Gas={}, Cost={}, Depth={}", 
				pc, opcode, gas_before, gas_cost, depth);
		}
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

	fn exit_child_span(&mut self, output: &ExecReturnValue, _gas_used: Weight) {
		if output.did_revert() {
			self.record_error("execution reverted".to_string());
		} else {
			self.record_return_data(&output.data);
		}
		
		if self.depth > 0 {
			self.depth -= 1;
		}
	}

	fn exit_child_span_with_error(&mut self, error: DispatchError, _gas_used: Weight) {
		self.record_error(format!("{:?}", error));
		
		if self.depth > 0 {
			self.depth -= 1;
		}
	}
}

