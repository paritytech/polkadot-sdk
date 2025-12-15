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
	evm::{tracing::Tracing, Bytes, Op, SyscallStep, SyscallTrace, SyscallTracerConfig},
	vm::pvm::env::lookup_syscall_index,
	DispatchError, ExecReturnValue, Weight,
};
use alloc::{
	format,
	string::{String, ToString},
	vec::Vec,
};
use sp_core::{H160, U256};

/// A tracer that traces syscall execution step-by-step.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct SyscallTracer {
	/// The tracer configuration.
	config: SyscallTracerConfig,

	/// The collected trace steps.
	steps: Vec<SyscallStep>,

	/// Current call depth.
	depth: u32,

	/// Number of steps captured (for limiting).
	step_count: u64,

	/// Total gas used by the transaction.
	total_gas_used: u64,

	/// Whether the transaction failed.
	failed: bool,

	/// The return value of the transaction.
	return_value: Bytes,

	/// Pending step that's waiting for gas cost to be recorded.
	pending_step: Option<SyscallStep>,
}

impl SyscallTracer {
	/// Create a new [`SyscallTracer`] instance.
	pub fn new(config: SyscallTracerConfig) -> Self {
		Self {
			config,
			steps: Vec::new(),
			depth: 0,
			step_count: 0,
			total_gas_used: 0,
			failed: false,
			return_value: Bytes::default(),
			pending_step: None,
		}
	}

	/// Collect the traces and return them.
	pub fn collect_trace(self) -> SyscallTrace {
		let Self { steps: struct_logs, return_value, total_gas_used: gas, failed, .. } = self;
		SyscallTrace { gas, failed, return_value, struct_logs }
	}

	/// Record an error in the current step.
	pub fn record_error(&mut self, error: String) {
		if let Some(last_step) = self.steps.last_mut() {
			last_step.error = Some(error);
		}
	}
}

impl Tracing for SyscallTracer {
	fn is_opcode_tracing_enabled(&self) -> bool {
		true
	}

	fn enter_ecall(
		&mut self,
		ecall: &'static str,
		gas_before: u64,
		weight_before: crate::Weight,
		last_frame_output: &crate::ExecReturnValue,
	) {
		// Check step limit - if exceeded, don't record anything
		if self.config.limit.map(|l| self.step_count >= l).unwrap_or(false) {
			return;
		}

		// Extract return data if enabled
		let return_data = if self.config.enable_return_data {
			crate::evm::Bytes(last_frame_output.data.clone())
		} else {
			crate::evm::Bytes::default()
		};

		let step = SyscallStep {
			op: Op::PvmSyscall(lookup_syscall_index(ecall).unwrap_or_default()),
			gas: gas_before,
			weight: weight_before,
			gas_cost: 0u64,                  // Will be set in exit_ecall
			weight_cost: Default::default(), // Will be set in exit_ecall
			depth: self.depth,
			return_data,
			error: None,
		};

		self.pending_step = Some(step);
		self.step_count += 1;
	}

	fn exit_ecall(&mut self, gas_left: u64, weight_left: crate::Weight) {
		if let Some(mut step) = self.pending_step.take() {
			step.gas_cost = step.gas.saturating_sub(gas_left);
			step.weight_cost = step.weight.saturating_sub(weight_left);
			self.steps.push(step);
		}
	}

	fn enter_opcode(
		&mut self,
		_pc: u64,
		opcode: u8,
		gas_before: u64,
		weight_before: Weight,
		_get_stack: &dyn Fn() -> Vec<crate::evm::Bytes>,
		_get_memory: &dyn Fn(usize) -> Vec<crate::evm::Bytes>,
		_last_frame_output: &crate::ExecReturnValue,
	) {
		// Check step limit - if exceeded, don't record anything
		if self.config.limit.map(|l| self.step_count >= l).unwrap_or(false) {
			return;
		}

		// Extract return data if enabled
		let return_data = if self.config.enable_return_data {
			crate::evm::Bytes(_last_frame_output.data.clone())
		} else {
			crate::evm::Bytes::default()
		};

		let step = SyscallStep {
			op: Op::EVMOpcode(opcode),
			gas: gas_before,
			weight: weight_before,
			gas_cost: 0u64,                  // Will be set in exit_ecall
			weight_cost: Default::default(), // Will be set in exit_ecall
			depth: self.depth,
			return_data,
			error: None,
		};

		self.pending_step = Some(step);
		self.step_count += 1;
	}

	fn exit_opcode(&mut self, gas_left: u64) {
		self.exit_ecall(gas_left, Default::default());
	}

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

		// Set total gas used if this is the top-level call (depth 1, will become 0 after decrement)
		if self.depth == 1 {
			self.total_gas_used = gas_used.try_into().unwrap_or(u64::MAX);
		}

		if self.depth > 0 {
			self.depth -= 1;
		}
	}

	fn exit_child_span_with_error(&mut self, error: DispatchError, gas_used: u64) {
		self.record_error(format!("{:?}", error));

		// Mark as failed if this is the top-level call
		if self.depth == 1 {
			self.failed = true;
			self.total_gas_used = gas_used.try_into().unwrap_or(u64::MAX);
		}

		if self.depth > 0 {
			self.depth -= 1;
		}
	}
}
