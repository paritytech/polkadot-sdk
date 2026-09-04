// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Decimal scaling between a PSM pair's external and internal asset units.
//!
//! [`DecimalScale`] is the single source of truth for converting amounts between the two
//! precisions of a PSM pair: it is validated once, at construction from a [`DecimalsPair`],
//! and every conversion afterwards relies on the established invariant. Swaps use
//! [`DecimalScale::pair_from_external`] / [`DecimalScale::pair_from_internal`], which round
//! an amount down to the largest value both assets can represent exactly and return it as a
//! [`ScaledPair`].
//!
//! This module is pure math over an [`AtLeast32BitUnsigned`] balance and carries no pallet
//! dependencies; should a second consumer show up it can move to `sp-arithmetic` or
//! `frame-support` unchanged.

use sp_runtime::traits::AtLeast32BitUnsigned;

/// Unvalidated decimals snapshot for a PSM pair, as read from asset metadata.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DecimalsPair {
	/// The external asset's decimals.
	pub external: u8,
	/// The internal asset's decimals.
	pub internal: u8,
}

/// An amount represented exactly in both of a PSM pair's units.
///
/// Produced by [`DecimalScale::pair_from_external`] / [`DecimalScale::pair_from_internal`]:
/// `external` and `internal` always round-trip to each other without truncation, so any
/// dust cut off the input stays with its owner.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScaledPair<B> {
	/// The amount in external-asset units.
	pub external: B,
	/// The amount in internal-asset units.
	pub internal: B,
}

/// Why a [`DecimalScale`] could not be constructed from a [`DecimalsPair`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecimalScaleError {
	/// The pair's decimals differ by more than [`DecimalScale::MAX_DIFF`].
	DiffOutOfRange,
	/// The scaling factor `10^diff` does not fit the balance type.
	Overflow,
}

/// Validated conversion between a PSM pair's external- and internal-asset units.
///
/// Invariant, established at construction: the factor is `10^diff` with
/// `1 <= diff <= MAX_DIFF` and representable in `B`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecimalScale<B> {
	/// Both assets have the same precision.
	Identity,
	/// Internal is the higher-precision side: multiply toward internal, divide toward
	/// external.
	MulToInternal(B),
	/// Internal is the lower-precision side: divide toward internal, multiply toward
	/// external.
	DivToInternal(B),
}

impl<B> DecimalScale<B> {
	/// Maximum absolute difference between the two assets' decimals. Bounds the scaling
	/// factor `10^diff` well below `u128::MAX` so realistic balances cannot overflow
	/// during conversion.
	pub const MAX_DIFF: u8 = 24;
}

impl<B: AtLeast32BitUnsigned + Copy> DecimalScale<B> {
	/// Validate a [`DecimalsPair`] into a scale usable for conversions.
	pub fn try_new(decimals: DecimalsPair) -> Result<Self, DecimalScaleError> {
		let diff = decimals.external.abs_diff(decimals.internal);
		if diff > Self::MAX_DIFF {
			return Err(DecimalScaleError::DiffOutOfRange);
		}
		if diff == 0 {
			return Ok(Self::Identity);
		}
		let factor = 10u128
			.checked_pow(u32::from(diff))
			.and_then(|factor| B::try_from(factor).ok())
			.ok_or(DecimalScaleError::Overflow)?;
		if decimals.internal > decimals.external {
			Ok(Self::MulToInternal(factor))
		} else {
			Ok(Self::DivToInternal(factor))
		}
	}

	/// Convert an external-unit amount into internal units, flooring on division.
	/// `None` on multiplication overflow.
	pub fn to_internal(self, external_amount: B) -> Option<B> {
		match self {
			Self::Identity => Some(external_amount),
			Self::MulToInternal(factor) => external_amount.checked_mul(&factor),
			// Factor is `10^diff` with `diff >= 1` by construction, so never zero; qed.
			Self::DivToInternal(factor) => Some(external_amount / factor),
		}
	}

	/// Convert an internal-unit amount into external units, flooring on division.
	/// `None` on multiplication overflow.
	pub fn to_external(self, internal_amount: B) -> Option<B> {
		match self {
			Self::Identity => Some(internal_amount),
			// Factor is `10^diff` with `diff >= 1` by construction, so never zero; qed.
			Self::MulToInternal(factor) => Some(internal_amount / factor),
			Self::DivToInternal(factor) => internal_amount.checked_mul(&factor),
		}
	}

	/// Round an external-unit amount down to the largest share of `external_amount` that
	/// converts without truncation, paired with its exact internal equivalent.
	/// `None` on multiplication overflow.
	pub fn pair_from_external(self, external_amount: B) -> Option<ScaledPair<B>> {
		match self {
			Self::Identity => {
				Some(ScaledPair { external: external_amount, internal: external_amount })
			},
			Self::MulToInternal(factor) => {
				let internal = external_amount.checked_mul(&factor)?;
				Some(ScaledPair { external: external_amount, internal })
			},
			Self::DivToInternal(factor) => {
				let internal = external_amount / factor;
				// Floored by `factor` above, so scaling back cannot exceed
				// `external_amount`; qed.
				let external = internal.checked_mul(&factor)?;
				Some(ScaledPair { external, internal })
			},
		}
	}

	/// Round an internal-unit amount down the same way: mirror of
	/// [`Self::pair_from_external`].
	pub fn pair_from_internal(self, internal_amount: B) -> Option<ScaledPair<B>> {
		match self {
			Self::Identity => {
				Some(ScaledPair { external: internal_amount, internal: internal_amount })
			},
			Self::MulToInternal(factor) => {
				let external = internal_amount / factor;
				// Floored by `factor` above, so scaling back cannot exceed
				// `internal_amount`; qed.
				let internal = external.checked_mul(&factor)?;
				Some(ScaledPair { external, internal })
			},
			Self::DivToInternal(factor) => {
				let external = internal_amount.checked_mul(&factor)?;
				Some(ScaledPair { external, internal: internal_amount })
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Validated scale for an `(external, internal)` decimals pair.
	fn scale(external: u8, internal: u8) -> DecimalScale<u128> {
		DecimalScale::try_new(DecimalsPair { external, internal }).expect("pair within MAX_DIFF")
	}

	#[test]
	fn to_internal_same_decimals_is_identity() {
		assert_eq!(scale(6, 6).to_internal(1_000_000), Some(1_000_000));
	}

	#[test]
	fn to_internal_scale_up_is_exact() {
		// 2 decimals -> 6 decimals: multiply by 10^4.
		assert_eq!(scale(2, 6).to_internal(100), Some(1_000_000));
	}

	#[test]
	fn to_internal_scale_down_truncates() {
		// 18 decimals -> 6 decimals: divide by 10^12, floor.
		assert_eq!(scale(18, 6).to_internal(1_500_000_000_000_000_123), Some(1_500_000));
	}

	#[test]
	fn round_trip_bounds() {
		// For any amount, round-trip should shrink or preserve.
		for (external, internal) in [(2u8, 6u8), (6, 6), (18, 6), (6, 18), (6, 2)] {
			for amount in [0u128, 1, 100, 1_234_567, 10u128.pow(18)] {
				let fwd = scale(external, internal).to_internal(amount).unwrap();
				let rtp = scale(external, internal).to_external(fwd).unwrap();
				assert!(rtp <= amount, "round-trip grew: amount={} got {}", amount, rtp);
			}
		}
	}

	#[test]
	fn pair_from_external_rounds_down_to_round_trip_boundary() {
		// 18 -> 6 decimals: 123 units of dust are cut off both sides.
		let pair = scale(18, 6).pair_from_external(1_500_000_000_000_000_123).unwrap();
		assert_eq!(pair, ScaledPair { external: 1_500_000_000_000_000_000, internal: 1_500_000 });
		// 2 -> 6 decimals: scale-up is always exact.
		let pair = scale(2, 6).pair_from_external(123).unwrap();
		assert_eq!(pair, ScaledPair { external: 123, internal: 1_230_000 });
	}

	#[test]
	fn pair_from_internal_rounds_down_to_round_trip_boundary() {
		// 6 -> 2 decimals: 45 units of dust are cut off both sides.
		let pair = scale(2, 6).pair_from_internal(1_230_045).unwrap();
		assert_eq!(pair, ScaledPair { external: 123, internal: 1_230_000 });
		// 6 -> 18 decimals: scale-up is always exact.
		let pair = scale(18, 6).pair_from_internal(1_500_000).unwrap();
		assert_eq!(pair, ScaledPair { external: 1_500_000_000_000_000_000, internal: 1_500_000 });
	}

	#[test]
	fn construction_rejects_out_of_range_and_overflow() {
		// |0 - 40| = 40 exceeds MAX_DIFF (24).
		assert_eq!(
			DecimalScale::<u128>::try_new(DecimalsPair { external: 0, internal: 40 }),
			Err(DecimalScaleError::DiffOutOfRange)
		);
		// 10^24 is within MAX_DIFF but does not fit a u64 balance type.
		assert_eq!(
			DecimalScale::<u64>::try_new(DecimalsPair { external: 0, internal: 24 }),
			Err(DecimalScaleError::Overflow)
		);
	}

	#[test]
	fn max_diff_const_is_protective() {
		// Compile-time sanity: the chosen bound is wide but below the overflow point.
		// 10^24 fits comfortably in u128 (< 10^38), and leaves ~10^14 headroom on
		// balances. The const is documented; this asserts it has not been widened
		// beyond the safe range.
		assert!(DecimalScale::<u128>::MAX_DIFF <= 30);
	}
}
