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
	evm::{tracing::Tracing, Bytes, OpcodeStep, OpcodeTrace, OpcodeTracerConfig},
	DispatchError, ExecReturnValue, Key,
};
use alloc::{
	collections::BTreeMap,
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
	total_gas_used: U256,

	/// Whether the transaction failed.
	failed: bool,

	/// The return value of the transaction.
	return_value: Bytes,

	/// Pending step that's waiting for gas cost to be recorded.
	pending_step: Option<OpcodeStep>,

	/// Gas before executing the current pending step.
	pending_gas_before: Option<U256>,

	/// List of storage per call
	storages_per_call: Vec<BTreeMap<Bytes, Bytes>>,
}

impl OpcodeTracer {
	/// Create a new [`OpcodeTracer`] instance.
	pub fn new(config: OpcodeTracerConfig) -> Self {
		Self {
			config,
			steps: Vec::new(),
			depth: 0,
			step_count: 0,
			total_gas_used: U256::zero(),
			failed: false,
			return_value: Bytes::default(),
			pending_step: None,
			pending_gas_before: None,
			storages_per_call: alloc::vec![Default::default()],
		}
	}

	/// Collect the traces and return them.
	pub fn collect_trace(self) -> OpcodeTrace {
		let Self { steps: struct_logs, return_value, total_gas_used: gas, failed, .. } = self;
		OpcodeTrace { gas, failed, return_value, struct_logs }
	}

	/// Record an error in the current step.
	pub fn record_error(&mut self, error: String) {
		if let Some(last_step) = self.steps.last_mut() {
			last_step.error = Some(error);
		}
	}

	/// Record return data.
	pub fn record_return_data(&mut self, data: &[u8]) {
		self.return_value = Bytes(data.to_vec());
	}

	/// Mark the transaction as failed.
	pub fn mark_failed(&mut self) {
		self.failed = true;
	}

	/// Set the total gas used by the transaction.
	pub fn set_total_gas_used(&mut self, gas_used: U256) {
		self.total_gas_used = gas_used;
	}
}

impl Tracing for OpcodeTracer {
	fn is_opcode_tracing_enabled(&self) -> bool {
		true
	}

	fn enter_opcode(
		&mut self,
		pc: u64,
		opcode: u8,
		gas_before: U256,
		get_stack: &dyn Fn() -> Vec<crate::evm::Bytes>,
		get_memory: &dyn Fn(usize) -> Vec<crate::evm::Bytes>,
		last_frame_output: &crate::ExecReturnValue,
	) {
		// Check step limit - if exceeded, don't record anything
		if self.config.limit.map(|l| self.step_count >= l).unwrap_or(false) {
			return;
		}

		// Extract stack data if enabled
		let stack_data = if !self.config.disable_stack { get_stack() } else { Vec::new() };

		// Extract memory data if enabled
		let memory_data = if self.config.enable_memory {
			get_memory(self.config.memory_word_limit as usize)
		} else {
			Vec::new()
		};

		// Extract return data if enabled
		let return_data = if self.config.enable_return_data {
			crate::evm::Bytes(last_frame_output.data.clone())
		} else {
			crate::evm::Bytes::default()
		};

		let step = OpcodeStep {
			pc,
			op: opcode,
			gas: gas_before.try_into().unwrap_or(u64::MAX),
			gas_cost: 0u64, // Will be set in exit_opcode
			depth: self.depth,
			stack: stack_data,
			memory: memory_data,
			storage: None,
			return_data,
			error: None,
		};

		self.pending_step = Some(step);
		self.pending_gas_before = Some(gas_before);
		self.step_count += 1;
	}

	fn exit_opcode(&mut self, gas_left: U256) {
		if let Some(mut step) = self.pending_step.take() {
			if let Some(gas_before) = self.pending_gas_before.take() {
				let gas_cost = gas_before.saturating_sub(gas_left);
				step.gas_cost = gas_cost.try_into().unwrap_or(u64::MAX);
			}
			self.steps.push(step);
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
		_gas_limit: U256,
	) {
		self.storages_per_call.push(Default::default());
		self.depth += 1;
	}

	fn exit_child_span(&mut self, output: &ExecReturnValue, gas_used: U256) {
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
			self.set_total_gas_used(gas_used);
		}

		self.storages_per_call.pop();

		if self.depth > 0 {
			self.depth -= 1;
		}
	}

	fn exit_child_span_with_error(&mut self, error: DispatchError, gas_used: U256) {
		self.record_error(format!("{:?}", error));

		// Mark as failed if this is the top-level call
		if self.depth == 1 {
			self.mark_failed();
			self.set_total_gas_used(gas_used);
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

		// Get the last storage map for the current call depth
		if let Some(storage) = self.storages_per_call.last_mut() {
			let key_bytes = crate::evm::Bytes(key.unhashed().to_vec());
			let value_bytes = crate::evm::Bytes(new_value.map(|v| v.to_vec()).unwrap_or_default());
			storage.insert(key_bytes, value_bytes);

			// Set storage on the pending step
			if let Some(ref mut step) = self.pending_step {
				step.storage = Some(storage.clone());
			}
		}
	}

	fn storage_read(&mut self, key: &Key, value: Option<&[u8]>) {
		// Only track storage if not disabled
		if self.config.disable_storage {
			return;
		}

		// Get the last storage map for the current call depth
		if let Some(storage) = self.storages_per_call.last_mut() {
			let key_bytes = crate::evm::Bytes(key.unhashed().to_vec());
			storage.entry(key_bytes).or_insert_with(|| {
				crate::evm::Bytes(value.map(|v| v.to_vec()).unwrap_or_default())
			});

			// Set storage on the pending step
			if let Some(ref mut step) = self.pending_step {
				step.storage = Some(storage.clone());
			}
		}
	}
}
