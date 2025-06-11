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
    Config,
};
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
        println!("call create2");
        // Parse input according to CREATE2 ABI:
        // [0..32]   = value
        // [32..64]  = offset to code
        // [64..96]  = length of code
        // [96..128] = offset to salt
        // [128..160]= length of salt
        // [160..]   = code + salt

        // For simplicity, you may want to use the same ABI as OpenEthereum's CREATE2 precompile,
        // or match the EVM opcode directly.

        // ...parse input, deploy contract, handle errors...

        // For now, just return success with empty output:
        Ok(vec![])
    }
}
