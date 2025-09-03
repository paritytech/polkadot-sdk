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
	vm::RuntimeCosts,
	Config,
};
use alloc::vec::Vec;
use bn::{pairing_batch, AffineG1, AffineG2, Fq, Fq2, Group, Gt, G1, G2};
use core::{marker::PhantomData, num::NonZero};
use sp_core::U256;
use sp_runtime::DispatchError;

pub struct Bn128Add<T>(PhantomData<T>);

impl<T: Config> PrimitivePrecompile for Bn128Add<T> {
	type T = T;
	const MATCHER: BuiltinAddressMatcher = BuiltinAddressMatcher::Fixed(NonZero::new(6).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: Vec<u8>,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		env.gas_meter_mut().charge(RuntimeCosts::Bn128Add)?;

		let p1 = read_point(&input, 0)?;
		let p2 = read_point(&input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p1 + p2) {
			// point not at infinity
			sum.x().to_big_endian(&mut buf[0..32]).expect("0..32 is 32-byte length; qed");
			sum.y().to_big_endian(&mut buf[32..64]).expect("32..64 is 32-byte length; qed");
		}

		Ok(buf.to_vec())
	}
}

pub struct Bn128Mul<T>(PhantomData<T>);

impl<T: Config> PrimitivePrecompile for Bn128Mul<T> {
	type T = T;
	const MATCHER: BuiltinAddressMatcher = BuiltinAddressMatcher::Fixed(NonZero::new(7).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: Vec<u8>,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		env.gas_meter_mut().charge(RuntimeCosts::Bn128Mul)?;

		let p = read_point(&input, 0)?;
		let fr = read_fr(&input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p * fr) {
			// point not at infinity
			sum.x().to_big_endian(&mut buf[0..32]).expect("0..32 is 32-byte length; qed");
			sum.y().to_big_endian(&mut buf[32..64]).expect("32..64 is 32-byte length; qed");
		}

		Ok(buf.to_vec())
	}
}

pub struct Bn128Pairing<T>(PhantomData<T>);

impl<T: Config> PrimitivePrecompile for Bn128Pairing<T> {
	type T = T;
	const MATCHER: BuiltinAddressMatcher = BuiltinAddressMatcher::Fixed(NonZero::new(8).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: Vec<u8>,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		if input.len() % 192 != 0 {
			Err(DispatchError::from("invalid input length"))?;
		}

		let ret_val = if input.is_empty() {
			env.gas_meter_mut().charge(RuntimeCosts::Bn128Pairing(0))?;
			U256::one()
		} else {
			// (a, b_a, b_b - each 64-byte affine coordinates)
			let elements = input.len() / 192;
			env.gas_meter_mut().charge(RuntimeCosts::Bn128Pairing(elements as u32))?;

			let mut vals = Vec::new();
			for i in 0..elements {
				let offset = i * 192;
				let a_x = Fq::from_slice(&input[offset..offset + 32])
					.map_err(|_| DispatchError::from("Invalid a argument x coordinate"))?;

				let a_y = Fq::from_slice(&input[offset + 32..offset + 64])
					.map_err(|_| DispatchError::from("Invalid a argument y coordinate"))?;

				let b_a_y = Fq::from_slice(&input[offset + 64..offset + 96]).map_err(|_| {
					DispatchError::from("Invalid b argument imaginary coeff x coordinate")
				})?;

				let b_a_x = Fq::from_slice(&input[offset + 96..offset + 128]).map_err(|_| {
					DispatchError::from("Invalid b argument imaginary coeff y coordinate")
				})?;

				let b_b_y = Fq::from_slice(&input[offset + 128..offset + 160]).map_err(|_| {
					DispatchError::from("Invalid b argument real coeff x coordinate")
				})?;

				let b_b_x = Fq::from_slice(&input[offset + 160..offset + 192]).map_err(|_| {
					DispatchError::from("Invalid b argument real coeff y coordinate")
				})?;

				let b_a = Fq2::new(b_a_x, b_a_y);
				let b_b = Fq2::new(b_b_x, b_b_y);
				let b =
					if b_a.is_zero() && b_b.is_zero() {
						G2::zero()
					} else {
						G2::from(AffineG2::new(b_a, b_b).map_err(|_| {
							DispatchError::from("Invalid b argument - not on curve")
						})?)
					};
				let a =
					if a_x.is_zero() && a_y.is_zero() {
						G1::zero()
					} else {
						G1::from(AffineG1::new(a_x, a_y).map_err(|_| {
							DispatchError::from("Invalid a argument - not on curve")
						})?)
					};
				vals.push((a, b));
			}

			let mul = pairing_batch(&vals);

			if mul == Gt::one() {
				U256::one()
			} else {
				U256::zero()
			}
		};

		let buf = ret_val.to_big_endian();
		Ok(buf.to_vec())
	}
}

fn read_point(input: &[u8], start_inx: usize) -> Result<bn::G1, DispatchError> {
	let mut px_buf = [0u8; 32];
	let mut py_buf = [0u8; 32];
	read_input(input, &mut px_buf, start_inx);
	read_input(input, &mut py_buf, start_inx + 32);

	let px = Fq::from_slice(&px_buf).map_err(|_| "Invalid point x coordinate")?;
	let py = Fq::from_slice(&py_buf).map_err(|_| "Invalid point y coordinate")?;

	Ok(if px == Fq::zero() && py == Fq::zero() {
		G1::zero()
	} else {
		AffineG1::new(px, py).map_err(|_| "Invalid curve point")?.into()
	})
}

fn read_fr(input: &[u8], start_inx: usize) -> Result<bn::Fr, DispatchError> {
	let mut buf = [0u8; 32];
	read_input(input, &mut buf, start_inx);

	let r = bn::Fr::from_slice(&buf).map_err(|_| "Invalid field element")?;
	Ok(r)
}

/// Copy bytes from input to target.
fn read_input(source: &[u8], target: &mut [u8], offset: usize) {
	// Out of bounds, nothing to copy.
	if source.len() <= offset {
		return;
	}

	// Find len to copy up to target len, but not out of bounds.
	let len = core::cmp::min(target.len(), source.len() - offset);
	target[..len].copy_from_slice(&source[offset..][..len]);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		precompiles::tests::{run_failure_test_vectors, run_test_vectors},
		tests::Test,
	};

	#[test]
	fn test_bn128add() {
		run_test_vectors::<Bn128Add<Test>>(include_str!("./testdata/6-bn128add.json"));
		run_failure_test_vectors::<Bn128Add<Test>>(include_str!(
			"./testdata/6-bn128add-failure.json"
		));
	}

	#[test]
	fn test_bn128mul() {
		run_test_vectors::<Bn128Mul<Test>>(include_str!("./testdata/7-bn128mul.json"));
	}

	#[test]
	fn test_bn128pairing() {
		run_test_vectors::<Bn128Pairing<Test>>(include_str!("./testdata/8-bn128pairing.json"));
	}
}
