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

use crate::{exec::ExecResult,  Config, Error, ExecReturnValue, Origin, H160, LOG_TARGET};

mod xcm;
pub use xcm::*;


/// Determine if the given address is a mutating precompile.
/// For now, we consider that all addresses between 0x1 and 0xff are reserved for precompiles.
pub fn is_mutating_precompile(address: &H160) -> bool {
	let bytes = address.as_bytes();
    let is_precompile=bytes.starts_with(&[0u8; 19]) && bytes[19] != 0;
    match bytes[19] {
        10u8 => true && is_precompile,
        _ => false,
    }
}

pub struct MutatingPrecompiles<T: Config> {
	_phantom: core::marker::PhantomData<T>,
}
pub trait MutatingPrecompile<T: Config> {
	/// Executes the precompile with the provided input data.
	fn execute(input: &[u8], origin: &Origin<T>) -> Result<ExecReturnValue, &'static str>;
}

impl<T: Config> MutatingPrecompiles<T> {
	pub fn execute(addr: H160,  input: &[u8], origin: &Origin<T>) -> ExecResult {
		match addr.as_bytes()[19] {
			10u8 => XcmPrecompile::execute(input, origin),
			_ => return Err(Error::<T>::UnsupportedPrecompileAddress.into()),
		}
		.map_err(|reason| {
			log::debug!(target: LOG_TARGET, "Precompile failed: {reason:?}");
			Error::<T>::PrecompileFailure.into()
		})
	}
	
}