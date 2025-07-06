// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
	precompiles::{BuiltinAddressMatcher, Error, Ext, PrimitivePrecompile},
	wasm::RuntimeCosts,
	Config,
};
use alloc::vec::Vec;
use core::{marker::PhantomData, num::NonZero};

pub struct Sha256<T>(PhantomData<T>);

impl<T: Config> PrimitivePrecompile for Sha256<T> {
	type T = T;
	const MATCHER: BuiltinAddressMatcher = BuiltinAddressMatcher::Fixed(NonZero::new(2).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: Vec<u8>,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		env.gas_meter_mut().charge(RuntimeCosts::HashSha256(input.len() as _))?;
		let data = sp_io::hashing::sha2_256(&input).to_vec();
		Ok(data)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{precompiles::tests::run_test_vectors, tests::Test};

	#[test]
	fn test_sha256() {
		run_test_vectors::<Sha256<Test>>(include_str!("./testdata/2-sha256.json"));
	}
}
