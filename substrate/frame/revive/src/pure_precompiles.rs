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

use crate::{exec::ExecResult, Config, Error, GasMeter, H160};

mod ecrecover;
pub use ecrecover::*;

/// Determine if the given address is a precompile.
/// For now, we consider that all addresses between 0x1 and 0xff are reserved for precompiles.
pub fn is_precompile(address: &H160) -> bool {
	let bytes = address.as_bytes();
	bytes.starts_with(&[0u8; 19]) && bytes[19] != 0
}

/// The `Precompile` trait defines the functionality for executing a precompiled contract.
pub trait Precompile<T: Config> {
	/// Executes the precompile with the provided input data.
	fn execute(gas_meter: &mut GasMeter<T>, input: &[u8]) -> ExecResult;
}

pub struct Precompiles<T: Config> {
	_phantom: core::marker::PhantomData<T>,
}

impl<T: Config> Precompiles<T> {
	pub fn execute(addr: H160, gas_meter: &mut GasMeter<T>, input: &[u8]) -> ExecResult {
		if addr == ECRECOVER {
			ECRecover::execute(gas_meter, input)
		} else {
			Err(Error::<T>::UnsupportedPrecompileAddress.into())
		}
	}
}
