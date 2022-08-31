// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

pub mod currency {
	use polkadot_core_primitives::Balance;
	use westend_runtime_constants as constants;

	/// The existential deposit. Set to 1/10 of its parent Relay Chain.
	pub const EXISTENTIAL_DEPOSIT: Balance = constants::currency::EXISTENTIAL_DEPOSIT / 10;

	pub const UNITS: Balance = constants::currency::UNITS;
	pub const CENTS: Balance = constants::currency::CENTS;
	pub const MILLICENTS: Balance = constants::currency::MILLICENTS;
	pub const GRAND: Balance = constants::currency::GRAND;

	pub const fn deposit(items: u32, bytes: u32) -> Balance {
		// 1/100 of Westend testnet
		constants::currency::deposit(items, bytes) / 100
	}
}

/// Fee-related.
pub mod fee {
	use frame_support::weights::{
		constants::ExtrinsicBaseWeight, WeightToFeeCoefficient, WeightToFeeCoefficients,
		WeightToFeePolynomial,
	};
	use polkadot_core_primitives::Balance;
	use smallvec::smallvec;
	pub use sp_runtime::Perbill;

	/// The block saturation level. Fees will be updates based on this value.
	pub const TARGET_BLOCK_FULLNESS: Perbill = Perbill::from_percent(25);

	/// Handles converting a weight scalar to a fee value, based on the scale and granularity of the
	/// node's balance type.
	///
	/// This should typically create a mapping between the following ranges:
	///   - [0, MAXIMUM_BLOCK_WEIGHT]
	///   - [Balance::min, Balance::max]
	///
	/// Yet, it can be used for any other sort of change to weight-fee. Some examples being:
	///   - Setting it to `0` will essentially disable the weight fee.
	///   - Setting it to `1` will cause the literal `#[weight = x]` values to be charged.
	pub struct WeightToFee;
	impl WeightToFeePolynomial for WeightToFee {
		type Balance = Balance;
		fn polynomial() -> WeightToFeeCoefficients<Self::Balance> {
			// in Polkadot, extrinsic base weight (smallest non-zero weight) is mapped to 1/10 CENT:
			// in Statemint, we map to 1/10 of that, or 1/100 CENT
			let p = super::currency::CENTS;
			let q = 100 * Balance::from(ExtrinsicBaseWeight::get().ref_time());
			smallvec![WeightToFeeCoefficient {
				degree: 1,
				negative: false,
				coeff_frac: Perbill::from_rational(p % q, q),
				coeff_integer: p / q,
			}]
		}
	}
}
