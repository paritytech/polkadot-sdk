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

use crate::FunctionContext;
use alloc::{string::String, vec::Vec};

/// The maximum number of arguments of a host call, the number of PolkaVM argument registers.
pub const MAX_ARGS: usize = 9;

/// The argument registers of a host call.
pub trait Registers {
	/// Returns the value of the argument register `index`, with `index < MAX_ARGS`.
	fn arg(&self, index: usize) -> u64;
}

/// The node side of a host call.
///
/// Every argument and the return value is passed as one register.
pub trait HostFunction: Sync {
	/// The name of the host call.
	fn name(&self) -> &'static str;

	/// Runs the host call with the argument registers `regs`.
	///
	/// Returns the register of the return value; an error traps the runtime.
	fn execute(
		&self,
		ctx: &mut dyn FunctionContext,
		regs: &dyn Registers,
	) -> Result<Option<u64>, String>;
}

/// A set of host calls.
pub trait HostFunctions {
	/// Returns the host calls of the set.
	fn host_functions() -> Vec<&'static dyn HostFunction>;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl HostFunctions for Tuple {
	fn host_functions() -> Vec<&'static dyn HostFunction> {
		let mut host_functions = Vec::new();
		for_tuples!( #( host_functions.extend(Tuple::host_functions()); )* );
		host_functions
	}
}
