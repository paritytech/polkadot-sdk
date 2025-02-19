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
mod ecrecover;
use crate::exec::ExecResult;
pub use ecrecover::*;

/// The `Precompile` trait defines the functionality for executing a precompiled contract.
pub trait Precompile {
	/// Executes the precompile with the provided input data.
	///
	/// # Parameters
	/// - `input`: The input data passed to the precompile.
	///
	/// # Returns
	/// - `ExecResult`: The result of the precompile execution
	fn execute(input: &[u8]) -> ExecResult;
}
