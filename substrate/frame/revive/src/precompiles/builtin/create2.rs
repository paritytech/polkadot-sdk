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
use sp_core::H160;

use crate::{
	address, limits::code, precompiles::{BuiltinAddressMatcher, Error, Ext, PrimitivePrecompile}, Config
};
use crate::address::AddressMapper;
use core::{marker::PhantomData, num::NonZero};

pub struct Create2<T>(PhantomData<T>);

impl<T: Config>  PrimitivePrecompile for Create2<T> {
	type T = T;
    const MATCHER: BuiltinAddressMatcher = BuiltinAddressMatcher::Fixed(NonZero::new(11).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

    fn call(
        address: &[u8; 20],
		input: Vec<u8>,
		env: &mut impl Ext<T = Self::T>,
    ) -> Result<Vec<u8>, Error> {
        println!("call create2, input_len(): {}", input.len());
        println!("address: {address:?}");

        
        if input.len() < 160 {
            Err(DispatchError::from("invalid input length"))?;
        }
        let endowment: &[u8; 32] = input[0..32].try_into().map_err(|_| DispatchError::from("invalid value length"))?;
        let code_offset: &[u8; 32] = input[32..64].try_into().map_err(|_| DispatchError::from("invalid code offset length"))?;
        let code_length: &[u8; 32] = input[64..96].try_into().map_err(|_| DispatchError::from("invalid code length length"))?;
        let salt_offset: &[u8; 32] = input[96..128].try_into().map_err(|_| DispatchError::from("invalid salt offset length"))?;
        let salt_length: &[u8; 32] = input[128..160].try_into().map_err(|_| DispatchError::from("invalid salt length length"))?;

        let code_offset1: u128 = u128::from_be_bytes(code_offset[0..16].try_into().map_err(|_| DispatchError::from("invalid code offset"))?);
        let code_offset2: u128 = u128::from_be_bytes(code_offset[16..32].try_into().map_err(|_| DispatchError::from("invalid code offset"))?);
        let code_length1: u128 = u128::from_be_bytes(code_length[0..16].try_into().map_err(|_| DispatchError::from("invalid code length"))?);
        let code_length2: u128 = u128::from_be_bytes(code_length[16..32].try_into().map_err(|_| DispatchError::from("invalid code length"))?);
        println!("code_offset1: {code_offset1}, code_offset2: {code_offset2}");
        println!("code_length1: {code_length1}, code_length2: {code_length2}");

        let salt_offset1: u128 = u128::from_be_bytes(salt_offset[0..16].try_into().map_err(|_| DispatchError::from("invalid salt offset"))?);
        let salt_offset2: u128 = u128::from_be_bytes(salt_offset[16..32].try_into().map_err(|_| DispatchError::from("invalid salt offset"))?);
        let salt_length1: u128 = u128::from_be_bytes(salt_length[0..16].try_into().map_err(|_| DispatchError::from("invalid salt length"))?);
        let salt_length2: u128 = u128::from_be_bytes(salt_length[16..32].try_into().map_err(|_| DispatchError::from("invalid salt length"))?);

        println!("salt_offset1: {salt_offset1}, salt_offset2: {salt_offset2}");
        println!("salt_length1: {salt_length1}, salt_length2: {salt_length2}");

        {
            assert_eq!(input.len(), salt_offset2 as usize + salt_length2 as usize, "input length does not match expected length");
        }

        let code_offset = code_offset2 as usize;
        let code_length = code_length2 as usize;
        let salt_offset = salt_offset2 as usize;
        let salt_length = salt_length2 as usize;
        let code = &input[code_offset..code_offset + code_length];
        let salt = &input[salt_offset..salt_offset + salt_length];

        println!("salt.len(): {}", salt.len());

        let caller = env.caller();
        let deployer_account_id = caller.account_id().map_err(|_| DispatchError::from("caller account_id is None"))?;
        let deployer = T::AddressMapper::to_address(deployer_account_id);


        println!("deployer: {deployer:?}");
        let salt: &[u8; 32] = salt.try_into().map_err(|_| DispatchError::from("invalid salt length"))?;

        let contract_address = crate::address::create2(&deployer, code, &[], salt);
        // CREATE2 ABI:
        // [0..32]   = value
        // [32..64]  = offset to code
        // [64..96]  = length of code
        // [96..128] = offset to salt
        // [128..160]= length of salt
        // [160..]   = code + salt

        // Pad the contract address to 32 bytes (left padding with zeros)
        let mut padded = [0u8; 32];
        let addr = contract_address.as_ref();
        padded[32 - addr.len()..].copy_from_slice(addr);
        Ok(padded.to_vec())
    }

}
