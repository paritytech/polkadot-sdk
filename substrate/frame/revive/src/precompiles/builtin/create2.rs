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

use sp_runtime::DispatchError;
use crate::Pallet as Contracts;
use crate::{
	precompiles::{BuiltinAddressMatcher, Error, ExtWithInfo, PrimitivePrecompile}, Config
};
use crate::H256;
use sp_core::U256;
use crate::BalanceOf;
use crate::address::AddressMapper;
use core::{marker::PhantomData, num::NonZero};

pub struct Create2<T>(PhantomData<T>);

impl<T: Config>  PrimitivePrecompile for Create2<T> 
{
	type T = T;
    const MATCHER: BuiltinAddressMatcher = BuiltinAddressMatcher::Fixed(NonZero::new(11).unwrap());
	const HAS_CONTRACT_INFO: bool = true;

    fn call_with_info(
        address: &[u8; 20],
		input: Vec<u8>,
		env: &mut impl ExtWithInfo<T = Self::T>,
    ) -> Result<Vec<u8>, Error> {

        // TODO(RVE): replace asserts with Err?

        // TODO(RVE): what value to put here?
        let gas_limit = frame_support::weights::Weight::MAX;

        // TODO(RVE): what value to put here?
        let storage_deposit_limit = crate::DepositLimit::<BalanceOf::<Self::T>>::UnsafeOnlyForDryRun;

        if input.len() < 160 {
            Err(DispatchError::from("invalid input length"))?;
        }
        let endowment = U256::from_big_endian(input[0..32].try_into().map_err(|_| DispatchError::from("invalid endowment"))?);
        let code_offset = U256::from_big_endian(input[32..64].try_into().map_err(|_| DispatchError::from("invalid code offset length"))?);
        let code_length = U256::from_big_endian(input[64..96].try_into().map_err(|_| DispatchError::from("invalid code length length"))?);
        let salt_offset = U256::from_big_endian(input[96..128].try_into().map_err(|_| DispatchError::from("invalid salt offset length"))?);
        let salt_length = U256::from_big_endian(input[128..160].try_into().map_err(|_| DispatchError::from("invalid salt length length"))?);

        // check offsets and lengths are not out of bounds for u64
        assert!(code_offset.low_u64() <= u64::MAX, "code_offset is out of bounds");
        assert!(code_length.low_u64() <= u64::MAX, "code_length is out of bounds");
        assert!(salt_offset.low_u64() <= u64::MAX, "salt_offset is out of bounds");
        assert!(salt_length.low_u64() <= u64::MAX, "salt_length is out of bounds");

        assert!((code_offset + code_length).low_u64() <= u64::MAX, "code_offset + code_length is out of bounds");
        assert!((salt_offset + salt_length).low_u64() <= u64::MAX, "salt_offset + salt_length is out of bounds");

        assert_eq!(input.len(), salt_offset.low_u64() as usize + salt_length.low_u64() as usize, "input length does not match expected length");

        let code_offset = code_offset.low_u64() as usize;
        let code_length = code_length.low_u64() as usize;
        let salt_offset: usize = salt_offset.low_u64() as usize;
        let salt_length = salt_length.low_u64() as usize;
        let code = &input[code_offset..code_offset + code_length];
        let salt = &input[salt_offset..salt_offset + salt_length];

        let caller = env.caller();
        let deployer_account_id = caller.account_id().map_err(|_| DispatchError::from("caller account_id is None"))?;
        let deployer = T::AddressMapper::to_address(deployer_account_id);

        // TODO(RVE): address::create2 requires 32 byte salt so salt_length is pointless?
        assert_eq!(salt_length, 32, "salt length must be 32 bytes");
        let salt: &[u8; 32] = salt.try_into().map_err(|_| DispatchError::from("invalid salt length"))?;
        let contract_address = crate::address::create2(&deployer, code, &[], salt);
        println!("deployer: {:?}", deployer);
        println!("salt: {:?}", salt);
        println!("code: {:?}", code);

        let code_hash = sp_io::hashing::keccak_256(&code);

        let instantiate_result = env.instantiate(
            gas_limit,
            U256::MAX, // TODO(RVE): what value to put here?
            H256::from(code_hash),
            endowment,
            vec![], // input data for constructor, if any
            Some(salt),
            Some(&deployer),
        );
        assert!(instantiate_result.is_ok());
        println!("instantiate_result: {:?}", instantiate_result.unwrap());
        println!("contract_address: {:?}", contract_address);
        println!("address: {:?}", address);
        assert_eq!(instantiate_result.unwrap(), contract_address);

        // Pad the contract address to 32 bytes (left padding with zeros)
        let mut padded = [0u8; 32];
        let addr = contract_address.as_ref();
        padded[32 - addr.len()..].copy_from_slice(addr);
        Ok(padded.to_vec())
    }

}
