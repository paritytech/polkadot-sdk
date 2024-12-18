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
//! Encodes/Decodes EVM gas values.

use crate::{BalanceOf, Config, Weight};
use core::ops::{Div, Rem};
use frame_support::pallet_prelude::CheckedShl;
use sp_arithmetic::traits::{One, Zero};
use sp_core::U256;

// We use 3 digits to store each component.
const SCALE: u128 = 100;

/// Rounds up the given value to the nearest multiple of the mask.
///
/// # Panics
/// Panics if the `mask` is zero.
fn round_up<T>(value: T, mask: T) -> T
where
	T: One + Zero + Copy + Rem<Output = T> + Div<Output = T>,
	<T as Rem>::Output: PartialEq,
{
	let rest = if value % mask == T::zero() { T::zero() } else { T::one() };
	value / mask + rest
}

/// Rounds up the log2 of the given value to the nearest integer.
fn log2_round_up<T>(val: T) -> u128
where
	T: Into<u128>,
{
	let val = val.into();
	val.checked_ilog2()
		.map(|v| if 1u128 << v == val { v } else { v + 1 })
		.unwrap_or(0) as u128
}

/// Encodes all components (deposit limit, weight reference time, and proof size) into a single
/// gas value.
///
/// The encoding follows the pattern `g...grrppdd`, where:
/// - `dd`: log2 Deposit value, encoded in the lowest 2 digits.
/// - `pp`: log2 Proof size, encoded in the next 2 digits.
/// - `rr`: log2 Reference time, encoded in the next 2 digits.
/// - `g...g`: Gas limit, encoded in the highest digits.
///
/// # Note
/// - Encoding fails if the deposit is not convertible to `u128`
/// - The deposit value is maxed by 2^99
pub fn encode<T: Config>(gas_limit: U256, weight: Weight, deposit: BalanceOf<T>) -> Option<U256> {
	let deposit: u128 = deposit.try_into().ok()?;
	let deposit_component = log2_round_up(deposit);

	let proof_size = weight.proof_size();
	let proof_size_component = SCALE * log2_round_up(proof_size);

	let ref_time = weight.ref_time();
	let ref_time_component = SCALE.pow(2) * log2_round_up(ref_time);

	let components = U256::from(deposit_component + proof_size_component + ref_time_component);

	let raw_gas_mask = U256::from(SCALE).pow(3.into());
	let raw_gas_component = if gas_limit < raw_gas_mask.saturating_add(components) {
		raw_gas_mask
	} else {
		round_up(gas_limit, raw_gas_mask).saturating_mul(raw_gas_mask)
	};

	Some(components.saturating_add(raw_gas_component))
}

/// Decodes the weight and deposit from the encoded gas value.
/// Returns `None` if the gas value is invalid
pub fn decode<T: Config>(gas: U256) -> Option<(Weight, BalanceOf<T>)> {
	let deposit = gas % SCALE;
	let gas_without_deposit = gas - deposit;

	// Casting with as_u32 is safe since all values are maxed by `SCALE`.
	let deposit = deposit.as_u32();
	let proof_time = ((gas_without_deposit / SCALE) % SCALE).as_u32();
	let ref_time = ((gas_without_deposit / SCALE.pow(2)) % SCALE).as_u32();

	let weight = Weight::from_parts(
		if ref_time == 0 { 0 } else { 1u64.checked_shl(ref_time)? },
		if proof_time == 0 { 0 } else { 1u64.checked_shl(proof_time)? },
	);
	let deposit = if deposit == 0 {
		BalanceOf::<T>::zero()
	} else {
		BalanceOf::<T>::one().checked_shl(deposit)?
	};

	Some((weight, deposit))
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::tests::Test;

	#[test]
	fn test_gas_encoding_decoding_works() {
		let raw_gas_limit = 111_111_999_999_999u128;
		let weight = Weight::from_parts(222_999_999, 333_999_999);
		let deposit = 444_999_999;

		let encoded_gas = encode::<Test>(raw_gas_limit.into(), weight, deposit).unwrap();
		assert_eq!(encoded_gas, U256::from(111_112_000_282_929u128));
		assert!(encoded_gas > raw_gas_limit.into());

		let (decoded_weight, decoded_deposit) = decode::<Test>(encoded_gas).unwrap();
		assert!(decoded_weight.all_gte(weight));
		assert!(weight.mul(2).all_gte(weight));

		assert!(decoded_deposit >= deposit);
		assert!(deposit * 2 >= decoded_deposit);
	}

	#[test]
	fn test_encoding_zero_values_work() {
		let encoded_gas =
			encode::<Test>(Default::default(), Default::default(), Default::default()).unwrap();

		assert_eq!(encoded_gas, U256::from(1_00_00_00));

		let (decoded_weight, decoded_deposit) = decode::<Test>(encoded_gas).unwrap();
		assert_eq!(Weight::default(), decoded_weight);
		assert_eq!(BalanceOf::<Test>::zero(), decoded_deposit);
	}

	#[test]
	fn test_overflow() {
		assert_eq!(None, decode::<Test>(65_00u128.into()), "Invalid proof size");
		assert_eq!(None, decode::<Test>(65_00_00u128.into()), "Invalid ref_time");
	}
}
