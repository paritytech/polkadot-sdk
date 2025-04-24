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
use super::Precompile;
use crate::{Config, ExecReturnValue, GasMeter, RuntimeCosts};
use alloc::vec::Vec;
use bn::{pairing_batch, AffineG1, AffineG2, Fq, Fq2, Group, Gt, G1, G2};
use pallet_revive_uapi::ReturnFlags;
use sp_core::U256;

/// The Bn128Add precompile.
pub struct Bn128Add;

impl<T: Config> Precompile<T> for Bn128Add {
	fn execute(gas_meter: &mut GasMeter<T>, input: &[u8]) -> Result<ExecReturnValue, &'static str> {
		gas_meter.charge(RuntimeCosts::Bn128Add)?;

		let p1 = read_point(input, 0)?;
		let p2 = read_point(input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p1 + p2) {
			// point not at infinity
			sum.x().to_big_endian(&mut buf[0..32]).expect("0..32 is 32-byte length; qed");
			sum.y().to_big_endian(&mut buf[32..64]).expect("32..64 is 32-byte length; qed");
		}

		Ok(ExecReturnValue { data: buf.to_vec(), flags: ReturnFlags::empty() })
	}
}

/// The Bn128Mul builtin
pub struct Bn128Mul;

impl<T: Config> Precompile<T> for Bn128Mul {
	fn execute(gas_meter: &mut GasMeter<T>, input: &[u8]) -> Result<ExecReturnValue, &'static str> {
		gas_meter.charge(RuntimeCosts::Bn128Mul)?;

		let p = read_point(input, 0)?;
		let fr = read_fr(input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p * fr) {
			// point not at infinity
			sum.x().to_big_endian(&mut buf[0..32]).expect("0..32 is 32-byte length; qed");
			sum.y().to_big_endian(&mut buf[32..64]).expect("32..64 is 32-byte length; qed");
		}

		Ok(ExecReturnValue { data: buf.to_vec(), flags: ReturnFlags::empty() })
	}
}

/// The Bn128Pairing builtin
pub struct Bn128Pairing;

impl<T: Config> Precompile<T> for Bn128Pairing {
	fn execute(gas_meter: &mut GasMeter<T>, input: &[u8]) -> Result<ExecReturnValue, &'static str> {
		if input.len() % 192 != 0 {
			return Err("invalid input length");
		}

		let ret_val = if input.is_empty() {
			gas_meter.charge(RuntimeCosts::Bn128Pairing(0))?;
			U256::one()
		} else {
			// (a, b_a, b_b - each 64-byte affine coordinates)
			let elements = input.len() / 192;
			gas_meter.charge(RuntimeCosts::Bn128Pairing(elements as u32))?;

			let mut vals = Vec::new();
			for i in 0..elements {
				let offset = i * 192;
				let a_x = Fq::from_slice(&input[offset..offset + 32])
					.map_err(|_| "Invalid a argument x coordinate")?;

				let a_y = Fq::from_slice(&input[offset + 32..offset + 64])
					.map_err(|_| "Invalid a argument y coordinate")?;

				let b_a_y = Fq::from_slice(&input[offset + 64..offset + 96])
					.map_err(|_| "Invalid b argument imaginary coeff x coordinate")?;

				let b_a_x = Fq::from_slice(&input[offset + 96..offset + 128])
					.map_err(|_| "Invalid b argument imaginary coeff y coordinate")?;

				let b_b_y = Fq::from_slice(&input[offset + 128..offset + 160])
					.map_err(|_| "Invalid b argument real coeff x coordinate")?;

				let b_b_x = Fq::from_slice(&input[offset + 160..offset + 192])
					.map_err(|_| "Invalid b argument real coeff y coordinate")?;

				let b_a = Fq2::new(b_a_x, b_a_y);
				let b_b = Fq2::new(b_b_x, b_b_y);
				let b = if b_a.is_zero() && b_b.is_zero() {
					G2::zero()
				} else {
					G2::from(
						AffineG2::new(b_a, b_b).map_err(|_| "Invalid b argument - not on curve")?,
					)
				};
				let a = if a_x.is_zero() && a_y.is_zero() {
					G1::zero()
				} else {
					G1::from(
						AffineG1::new(a_x, a_y).map_err(|_| "Invalid a argument - not on curve")?,
					)
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
		Ok(ExecReturnValue { data: buf.to_vec(), flags: ReturnFlags::empty() })
	}
}

fn read_point(input: &[u8], start_inx: usize) -> Result<bn::G1, &'static str> {
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

fn read_fr(input: &[u8], start_inx: usize) -> Result<bn::Fr, &'static str> {
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

#[cfg(feature = "runtime-benchmarks")]
pub fn generate_random_ecpairs(n: usize) -> Vec<u8> {
	use alloc::vec;
	use bn::{Fr, Group, G1, G2};
	use rand::SeedableRng;
	use rand_pcg::Pcg64;
	let mut rng = Pcg64::seed_from_u64(1);

	let mut buffer = vec![0u8; n * 192];

	let mut write = |element: &bn::Fq, offset: &mut usize| {
		element.to_big_endian(&mut buffer[*offset..*offset + 32]).unwrap();
		*offset += 32
	};

	for i in 0..n {
		let mut offset = i * 192;
		let scalar = Fr::random(&mut rng);

		let g1 = G1::one() * scalar;
		let g2 = G2::one() * scalar;
		let a = AffineG1::from_jacobian(g1).expect("G1 point should be on curve");
		let b = AffineG2::from_jacobian(g2).expect("G2 point should be on curve");

		write(&a.x(), &mut offset);
		write(&a.y(), &mut offset);
		write(&b.x().imaginary(), &mut offset);
		write(&b.x().real(), &mut offset);
		write(&b.y().imaginary(), &mut offset);
		write(&b.y().real(), &mut offset);
	}

	buffer
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::pure_precompiles::test::*;

	#[test]
	fn test_bn128add() -> Result<(), String> {
		test_precompile_test_vectors::<Bn128Add>(include_str!("./testdata/6-bn128add.json"))?;
		test_precompile_failure_test_vectors::<Bn128Add>(include_str!(
			"./testdata/6-bn128add-failure.json"
		))?;
		Ok(())
	}

	#[test]
	fn test_bn128mul() -> Result<(), String> {
		test_precompile_test_vectors::<Bn128Mul>(include_str!("./testdata/7-bn128mul.json"))?;
		Ok(())
	}

	#[test]
	fn test_bn128pairing() -> Result<(), String> {
		test_precompile_test_vectors::<Bn128Pairing>(include_str!(
			"./testdata/8-bn128pairing.json"
		))?;
		Ok(())
	}
}
