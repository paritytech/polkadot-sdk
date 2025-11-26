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

//! # RIP-7212 secp256r1 Precompile
//!
//! This module implements the [RIP-7212](https://github.com/ethereum/RIPs/blob/master/RIPS/rip-7212.md) precompile for
//! secp256r1 curve support.
//!
//! The main purpose of this precompile is to verify ECDSA signatures that use the secp256r1, or
//! P256 elliptic curve.

use crate::{
	precompiles::{BuiltinAddressMatcher, Error, Ext, PrimitivePrecompile},
	vm::RuntimeCosts,
	Config, U256,
};
use alloc::vec::Vec;
use core::{marker::PhantomData, num::NonZero};

pub struct P256Verify<T>(PhantomData<T>);

impl<T: Config> PrimitivePrecompile for P256Verify<T> {
	type T = T;
	const MATCHER: BuiltinAddressMatcher =
		BuiltinAddressMatcher::Fixed(NonZero::new(0x100).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	/// [RIP-7212](https://github.com/ethereum/RIPs/blob/master/RIPS/rip-7212.md#specification) secp256r1 precompile.
	///
	/// The input is encoded as follows:
	///
	/// | signed message hash |  r  |  s  | public key x | public key y |
	/// | :-----------------: | :-: | :-: | :----------: | :----------: |
	/// |          32         | 32  | 32  |     32       |      32      |
	fn call(
		_address: &[u8; 20],
		input: Vec<u8>,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		env.gas_meter_mut().charge_weight_token(RuntimeCosts::P256Verify)?;

		if revm::precompile::secp256r1::verify_impl(&input).is_some() {
			Ok(U256::one().to_big_endian().to_vec())
		} else {
			Ok(Default::default())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{precompiles::tests::run_test_vectors, tests::Test};

	#[test]
	fn test_p256_verify() {
		// https://github.com/ethereum/go-ethereum/blob/master/core/vm/testdata/precompiles/p256Verify.json
		run_test_vectors::<P256Verify<Test>>(include_str!("./testdata/256-p256_verify.json"));
	}

	#[test]
	fn test_p256_verify_address_match() {
		assert_eq!(
			<P256Verify<Test> as PrimitivePrecompile>::MATCHER.base_address(),
			hex_literal::hex!("0000000000000000000000000000000000000100")
		);
	}
}
