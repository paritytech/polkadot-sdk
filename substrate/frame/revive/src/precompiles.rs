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

use crate::{
	exec::{ExecResult, Ext},
	Config, Error, H160,
};
pub use ecrecover::*;

/// The `Precompile` trait defines the functionality for executing a precompiled contract.
pub trait Precompile<T: Config> {
	/// Executes the precompile with the provided input data.
	fn execute<E: Ext<T = T>>(ext: &mut E, input: &[u8]) -> ExecResult;
}

pub struct Precompiles<T: Config> {
	_phantom: core::marker::PhantomData<T>,
}

impl<T: Config> Precompiles<T> {
	pub fn execute<E: Ext<T = T>>(addr: H160, ext: &mut E, input: &[u8]) -> ExecResult {
		if addr == ECRECOVER {
			ECRecover::execute(ext, input)
		} else {
			Err(Error::<T>::UnsupportedPrecompileAddress.into())
		}
	}
}
