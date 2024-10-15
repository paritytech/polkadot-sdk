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

pub use crate::{
	evm::{CallTrace, CallType},
	exec::{ExecResult, ExportedFunction},
	primitives::ExecReturnValue,
	BalanceOf,
};
use crate::{Config, LOG_TARGET};
use sp_core::{H160, U256};
use sp_weights::Weight;

/// Umbrella trait for all interfaces that serves for debugging.
pub trait Debugger<T: Config>: Tracing<T> + CallInterceptor<T> {}

impl<T: Config, V> Debugger<T> for V where V: Tracing<T> + CallInterceptor<T> {}

pub type TraceOf<T> = <T as Config>::Debug;

/// Defines methods to capture contract calls, enabling external observers to
/// measure, trace, and react to contract interactions.
pub trait Tracing<T: Config>: Default {
	fn enter_child_span(
		&mut self,
		from: &H160,
		to: &H160,
		is_delegate_call: bool,
		is_read_only: bool,
		value: &crate::BalanceOf<T>,
		gas_limit: &Weight,
		input_data: &[u8],
	) -> &mut Self;

	fn after_call(&mut self, output: &ExecReturnValue);
}

impl<T: Config> Tracing<T> for () {
	fn enter_child_span(
		&mut self,
		from: &H160,
		to: &H160,
		is_delegate_call: bool,
		is_read_only: bool,
		value: &crate::BalanceOf<T>,
		gas_limit: &Weight,
		input_data: &[u8],
	) -> &mut Self {
		log::trace!(target: LOG_TARGET, "call (delegate: {is_delegate_call:?}, read_only: {is_read_only:?}) from: {from:?}, to: {to:?} value: {value:?} gas_limit: {gas_limit:?} input_data: {input_data:?}");
		self
	}

	fn after_call(&mut self, output: &ExecReturnValue) {
		log::trace!(target: LOG_TARGET, "call result {output:?}")
	}
}

impl<T: Config> Tracing<T> for CallTrace
where
	BalanceOf<T>: Into<U256>,
{
	fn enter_child_span(
		&mut self,
		from: &H160,
		to: &H160,
		is_delegate_call: bool,
		is_read_only: bool,
		value: &crate::BalanceOf<T>,
		_gas_limit: &Weight,
		input: &[u8],
	) -> &mut Self {
		let call_type = if is_read_only {
			CallType::StaticCall
		} else if is_delegate_call {
			CallType::DelegateCall
		} else {
			CallType::Call
		};

		let child_trace = CallTrace {
			from: *from,
			to: *to,
			value: (*value).into(),
			call_type,
			input: input.to_vec(),
			..Default::default()
		};

		self.calls.push(child_trace);
		self.calls.last_mut().unwrap()
	}

	fn after_call(&mut self, output: &ExecReturnValue) {
		log::trace!(target: LOG_TARGET, "call result {output:?}")
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
impl<T: Config> CallInterceptor<T> for CallTrace {}
