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

use crate::{primitives::ExecReturnValue, DispatchError, Weight};
use environmental::environmental;
use sp_core::{H160, H256, U256};

environmental!(tracer: dyn Tracer + 'static);

/// Trace the execution of the given closure.
///
/// # Warning
///
/// Only meant to be called from off-chain code as its additional resource usage is
/// not accounted for in the weights or memory envelope.
pub fn trace<R, F: FnOnce() -> R>(tracer: &mut (dyn Tracer + 'static), f: F) -> R {
	tracer::using_once(tracer, f)
}

/// Run the closure when tracing is enabled.
///
/// This is safe to be called from on-chain code as tracing will never be activated
/// there. Hence the closure is not executed in this case.
pub(crate) fn if_tracing<F: FnOnce(&mut (dyn Tracer + 'static))>(f: F) {
	tracer::with(f);
}

/// Defines methods to trace contract interactions.
pub trait Tracer {
	/// Called before a contract call is executed
	fn enter_child_span(
		&mut self,
		from: H160,
		to: H160,
		is_delegate_call: bool,
		is_read_only: bool,
		value: U256,
		input: &[u8],
		gas: Weight,
	);

	/// Record a log event
	fn log_event(&mut self, event: H160, topics: &[H256], data: &[u8]);

	/// Called after a contract call is executed
	fn exit_child_span(&mut self, output: &ExecReturnValue, gas_left: Weight);

	/// Called when a contract call terminates with an error
	fn exit_child_span_with_error(&mut self, error: DispatchError, gas_left: Weight);
}
