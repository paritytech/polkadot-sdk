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
	address::AddressMapper,
	precompiles::{BuiltinAddressMatcher, BuiltinPrecompile, Error, ExtWithInfo},
	CodeInfoOf, Config, H256,
};
use alloc::{vec, vec::Vec};
use alloy_core::sol;
use core::{marker::PhantomData, num::NonZero};
use sp_arithmetic::traits::SaturatedConversion;
use sp_core::U256;
use sp_runtime::DispatchError;

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
			ICreate2::ICreate2Calls::create2(call) => (call.code_hash.clone(), call.salt),
		};
		let code_hash = H256::from(code_hash.as_ref());

		if !CodeInfoOf::<T>::contains_key(&code_hash) {
			log::error!("code_hash not found: {:?}", code_hash);
			return Err(crate::Error::<T>::CodeNotFound.into());
		}

		let gas_limit = env.gas_meter().gas_left();

		let storage_deposit_limit = env.storage_meter().available();
		let endowment = env.value_transferred();

		let caller = env.caller();
		let deployer_account_id = caller
			.account_id()
			.map_err(|_| DispatchError::from("caller account_id is None"))?;
		let deployer = T::AddressMapper::to_address(deployer_account_id);

		let instantiate_address = env.instantiate(
			gas_limit,
			U256::from(storage_deposit_limit.saturated_into::<u128>()),
			code_hash,
			endowment,
			vec![], // input data for constructor, if any?
			Some(&salt),
			Some(&deployer),
		)?;

		// Pad the contract address to 32 bytes (left padding with zeros)
		let mut padded = [0u8; 32];
		let addr = instantiate_address.as_ref();
		padded[32 - addr.len()..].copy_from_slice(addr);
		Ok(padded.to_vec())
	}
}
