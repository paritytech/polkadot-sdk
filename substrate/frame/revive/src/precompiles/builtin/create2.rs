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
	precompiles::{BuiltinAddressMatcher, Error, ExtWithInfo, PrimitivePrecompile},
	Config, H256,
};
use alloc::{vec, vec::Vec};
use core::{marker::PhantomData, num::NonZero};
use sp_arithmetic::traits::SaturatedConversion;
use sp_core::U256;
use sp_runtime::DispatchError;

// upload the code before instantiate like in try_upload_code
// maybe dont need to change instantiate, could be fine to always use create2 address
// take endowment/value from the env

pub struct Create2<T>(PhantomData<T>);

impl<T: Config> PrimitivePrecompile for Create2<T> {
	type T = T;
	const MATCHER: BuiltinAddressMatcher = BuiltinAddressMatcher::Fixed(NonZero::new(11).unwrap());
	const HAS_CONTRACT_INFO: bool = true;

	fn call_with_info(
		_address: &[u8; 20],
		input: Vec<u8>,
		env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		let gas_limit = env.gas_meter().gas_left();

		let storage_deposit_limit = env.storage_meter().available();

		if input.len() < 64 {
			Err(DispatchError::from("invalid input length"))?;
		}
		let endowment = env.value_transferred();
		let salt: &[u8; 32] = &input[0..32]
			.try_into()
			.map_err(|_| DispatchError::from("invalid salt length"))?;
		let code = &input[32..];

		let caller = env.caller();
		let deployer_account_id = caller
			.account_id()
			.map_err(|_| DispatchError::from("caller account_id is None"))?;
		let deployer = T::AddressMapper::to_address(deployer_account_id);

		let contract_address = crate::address::create2(&deployer, code, &[], salt);

		let code_hash = sp_io::hashing::keccak_256(&code);

		let instantiate_address = env.instantiate(
			gas_limit,
			U256::from(storage_deposit_limit.saturated_into::<u128>()), /* Convert to U256 for
			                                                             * deposit limit */
			H256::from(code_hash),
			endowment,
			vec![], // input data for constructor, if any?
			Some(salt),
			Some(&deployer),
		)?;
		if instantiate_address != contract_address {
			Err(DispatchError::from("contract address mismatch"))?;
		}

		// Pad the contract address to 32 bytes (left padding with zeros)
		let mut padded = [0u8; 32];
		let addr = contract_address.as_ref();
		padded[32 - addr.len()..].copy_from_slice(addr);
		Ok(padded.to_vec())
	}
}
