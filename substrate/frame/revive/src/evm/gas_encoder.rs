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

use crate::Weight;
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

mod private {
	pub trait Sealed {}
	impl Sealed for () {}
}

/// Encodes/Decodes EVM gas values.
///
/// # Note
///
/// This is defined as a trait rather than standalone functions to allow
/// it to be added as an associated type to [`crate::Config`]. This way,
/// it can be invoked without requiring the implementation bounds to be
/// explicitly specified.
///
/// This trait is sealed and cannot be implemented by downstream crates.
pub trait GasEncoder<Balance>: private::Sealed {
	/// Encodes all components (deposit limit, weight reference time, and proof size) into a single
	/// gas value.
	fn encode(gas_limit: U256, weight: Weight, deposit: Balance) -> U256;

	/// Decodes the weight and deposit from the encoded gas value.
	/// Returns `None` if the gas value is invalid
	fn decode(gas: U256) -> Option<(Weight, Balance)>;

	/// Returns the encoded values of the specified weight and deposit.
	fn as_encoded_values(weight: Weight, deposit: Balance) -> (Weight, Balance) {
		let encoded = Self::encode(U256::zero(), weight, deposit);
		Self::decode(encoded).expect("encoded values should be decodable; qed")
	}
}

impl<Balance> GasEncoder<Balance> for ()
where
	Balance: Zero + One + CheckedShl + Into<u128>,
{
	/// The encoding follows the pattern `g...grrppdd`, where:
	/// - `dd`: log2 Deposit value, encoded in the lowest 2 digits.
	/// - `pp`: log2 Proof size, encoded in the next 2 digits.
	/// - `rr`: log2 Reference time, encoded in the next 2 digits.
	/// - `g...g`: Gas limit, encoded in the highest digits.
	///
	/// # Note
	/// - The deposit value is maxed by 2^99 for u128 balance, and 2^63 for u64 balance.
	fn encode(gas_limit: U256, weight: Weight, deposit: Balance) -> U256 {
		let deposit: u128 = deposit.into();
		let deposit_component = log2_round_up(deposit);

		let proof_size = weight.proof_size();
		let proof_size_component = SCALE * log2_round_up(proof_size);

		let ref_time = weight.ref_time();
		let ref_time_component = SCALE.pow(2) * log2_round_up(ref_time);

		let components = U256::from(deposit_component + proof_size_component + ref_time_component);

		let raw_gas_mask = U256::from(SCALE).pow(3.into());
		let raw_gas_component = if gas_limit <= components {
			U256::zero()
		} else {
			round_up(gas_limit, raw_gas_mask).saturating_mul(raw_gas_mask)
		};

		components.saturating_add(raw_gas_component)
	}

	fn decode(gas: U256) -> Option<(Weight, Balance)> {
		let deposit = gas % SCALE;

		// Casting with as_u32 is safe since all values are maxed by `SCALE`.
		let deposit = deposit.as_u32();
		let proof_time = ((gas / SCALE) % SCALE).as_u32();
		let ref_time = ((gas / SCALE.pow(2)) % SCALE).as_u32();

		let ref_weight = match ref_time {
			0 => 0,
			64 => u64::MAX,
			_ => 1u64.checked_shl(ref_time)?,
		};

		let proof_weight = match proof_time {
			0 => 0,
			64 => u64::MAX,
			_ => 1u64.checked_shl(proof_time)?,
		};

		let weight = Weight::from_parts(ref_weight, proof_weight);

		let deposit = match deposit {
			0 => Balance::zero(),
			_ => Balance::one().checked_shl(deposit)?,
		};

		Some((weight, deposit))
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_gas_encoding_decoding_works() {
		let raw_gas_limit = 111_111_999_999_999u128;
		let weight = Weight::from_parts(222_999_999, 333_999_999);
		let deposit = 444_999_999u64;

		let encoded_gas = <() as GasEncoder<u64>>::encode(raw_gas_limit.into(), weight, deposit);
		assert_eq!(encoded_gas, U256::from(111_112_000_282_929u128));
		assert!(encoded_gas > raw_gas_limit.into());

		let (decoded_weight, decoded_deposit) =
			<() as GasEncoder<u64>>::decode(encoded_gas).unwrap();
		assert!(decoded_weight.all_gte(weight));
		assert!(weight.mul(2).all_gte(weight));

		assert!(decoded_deposit >= deposit);
		assert!(deposit * 2 >= decoded_deposit);

		assert_eq!(
			(decoded_weight, decoded_deposit),
			<() as GasEncoder<u64>>::as_encoded_values(weight, deposit)
		);
	}

	#[test]
	fn test_encoding_zero_values_work() {
		let encoded_gas = <() as GasEncoder<u64>>::encode(
			Default::default(),
			Default::default(),
			Default::default(),
		);

		assert_eq!(encoded_gas, U256::from(0));

		let (decoded_weight, decoded_deposit) =
			<() as GasEncoder<u64>>::decode(encoded_gas).unwrap();
		assert_eq!(Weight::default(), decoded_weight);
		assert_eq!(0u64, decoded_deposit);

		let encoded_gas =
			<() as GasEncoder<u64>>::encode(U256::from(1), Default::default(), Default::default());
		assert_eq!(encoded_gas, U256::from(1000000));
	}

	#[test]
	fn test_encoding_max_values_work() {
		let max_weight = Weight::from_parts(u64::MAX, u64::MAX);
		let max_deposit = 1u64 << 63;
		let encoded_gas =
			<() as GasEncoder<u64>>::encode(Default::default(), max_weight, max_deposit);

		assert_eq!(encoded_gas, U256::from(646463));

		let (decoded_weight, decoded_deposit) =
			<() as GasEncoder<u64>>::decode(encoded_gas).unwrap();
		assert_eq!(max_weight, decoded_weight);
		assert_eq!(max_deposit, decoded_deposit);
	}

	#[test]
	fn test_overflow() {
		assert_eq!(None, <() as GasEncoder<u64>>::decode(65_00u128.into()), "Invalid proof size");
		assert_eq!(None, <() as GasEncoder<u64>>::decode(65_00_00u128.into()), "Invalid ref_time");
	}
}
