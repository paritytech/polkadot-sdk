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
	precompiles::{BuiltinAddressMatcher, BuiltinPrecompile, Error, ExtWithInfo},
	CodeInfoOf, Config, H256,
};
use alloc::{vec, vec::Vec};
use alloy_core::{sol, sol_types::SolValue};
use core::{marker::PhantomData, num::NonZero};
use sp_core::U256;

sol! {
	interface ICreate2 {
		function create2(bytes32 salt, bytes32 code_hash) external payable returns (address);
	}
}

#[allow(unused_imports)]
pub use ICreate2::*;

pub struct Create2<T>(PhantomData<T>);

impl<T: Config> BuiltinPrecompile for Create2<T> {
	type T = T;
	type Interface = ICreate2::ICreate2Calls;
	const MATCHER: BuiltinAddressMatcher =
		BuiltinAddressMatcher::Fixed(NonZero::new(0x0B).unwrap());
	const HAS_CONTRACT_INFO: bool = true;

	fn call_with_info(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		let (code_hash, salt) = match input {
			ICreate2::ICreate2Calls::create2(call) => (call.code_hash, call.salt),
		};
		let code_hash = H256::from(code_hash.as_ref());

		if !CodeInfoOf::<T>::contains_key(&code_hash) {
			log::error!("code_hash not found: {:?}", code_hash);
			return Err(crate::Error::<T>::CodeNotFound.into());
		}

		let gas_limit = env.gas_meter().gas_left();

		let endowment = env.value_transferred();

		let instantiate_address = env.instantiate(
			gas_limit,
			U256::MAX,
			code_hash,
			endowment,
			vec![], // input data for constructor, if any?
			Some(&salt),
		)?;

		let addr_bytes: [u8; 20] = instantiate_address.0;
		let address = alloy_core::primitives::Address::new(addr_bytes);
		Ok(address.abi_encode())
	}
}
