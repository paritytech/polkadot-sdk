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

pub use crate::exec::ExportedFunction;
use crate::{CodeHash, Config, LOG_TARGET};
use pallet_contracts_primitives::ExecReturnValue;

/// Umbrella trait for all interfaces that serves for debugging.
pub trait Debugger<T: Config>: Tracing<T> {}

impl<T: Config, V> Debugger<T> for V where V: Tracing<T> {}

/// Defines methods to capture contract calls, enabling external observers to
/// measure, trace, and react to contract interactions.
pub trait Tracing<T: Config> {
	/// The type of [`CallSpan`] that is created by this trait.
	type CallSpan: CallSpan;

	/// Creates a new call span to encompass the upcoming contract execution.
	///
	/// This method should be invoked just before the execution of a contract and
	/// marks the beginning of a traceable span of execution.
	///
	/// # Arguments
	///
	/// * `code_hash` - The code hash of the contract being called.
	/// * `entry_point` - Describes whether the call is the constructor or a regular call.
	/// * `input_data` - The raw input data of the call.
	fn new_call_span(
		code_hash: &CodeHash<T>,
		entry_point: ExportedFunction,
		input_data: &[u8],
	) -> Self::CallSpan;
}

/// Defines a span of execution for a contract call.
pub trait CallSpan {
	/// Called just after the execution of a contract.
	///
	/// # Arguments
	///
	/// * `output` - The raw output of the call.
	fn after_call(self, output: &ExecReturnValue);
}

impl<T: Config> Tracing<T> for () {
	type CallSpan = ();

	fn new_call_span(code_hash: &CodeHash<T>, entry_point: ExportedFunction, input_data: &[u8]) {
		log::trace!(target: LOG_TARGET, "call {entry_point:?} hash: {code_hash:?}, input_data: {input_data:?}")
	}
}

impl CallSpan for () {
	fn after_call(self, output: &ExecReturnValue) {
		log::trace!(target: LOG_TARGET, "call result {output:?}")
	}
}
