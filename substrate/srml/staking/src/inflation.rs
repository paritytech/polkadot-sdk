// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! This module expose one function `P_NPoS` (Payout NPoS) or `compute_total_payout` which returns
//! the total payout for the era given the era duration and the staking rate in NPoS.
//! The staking rate in NPoS is the total amount of tokens staked by nominators and validators,
//! divided by the total token supply.

use sr_primitives::{Perbill, traits::SimpleArithmetic, curve::PiecewiseLinear};

/// The total payout to all validators (and their nominators) per era.
///
/// Defined as such:
/// `payout = yearly_inflation(npos_token_staked / total_tokens) * total_tokans / era_per_year`
///
/// `era_duration` is expressed in millisecond.
pub fn compute_total_payout<N>(
	yearly_inflation: &PiecewiseLinear<'static>,
	npos_token_staked: N,
	total_tokens: N,
	era_duration: u64
) -> N where N: SimpleArithmetic + Clone
{
	// Milliseconds per year for the Julian year (365.25 days).
	const MILLISECONDS_PER_YEAR: u64 = 1000 * 3600 * 24 * 36525 / 100;

	Perbill::from_rational_approximation(era_duration as u64, MILLISECONDS_PER_YEAR)
		* yearly_inflation.calculate_for_fraction_times_denominator(npos_token_staked, total_tokens)
}

#[cfg(test)]
mod test {
	use sr_primitives::curve::PiecewiseLinear;

	srml_staking_reward_curve::build! {
		const I_NPOS: PiecewiseLinear<'static> = curve!(
			min_inflation: 0_025_000,
			max_inflation: 0_100_000,
			ideal_stake: 0_500_000,
			falloff: 0_050_000,
			max_piece_count: 40,
			test_precision: 0_005_000,
		);
	}

	#[test]
	fn npos_curve_is_sensible() {
		const YEAR: u64 = 365 * 24 * 60 * 60 * 1000;
		//super::I_NPOS.calculate_for_fraction_times_denominator(25, 100)
		assert_eq!(super::compute_total_payout(&I_NPOS, 0, 100_000u64, YEAR), 2_498);
		assert_eq!(super::compute_total_payout(&I_NPOS, 5_000, 100_000u64, YEAR), 3_248);
		assert_eq!(super::compute_total_payout(&I_NPOS, 25_000, 100_000u64, YEAR), 6_246);
		assert_eq!(super::compute_total_payout(&I_NPOS, 40_000, 100_000u64, YEAR), 8_494);
		assert_eq!(super::compute_total_payout(&I_NPOS, 50_000, 100_000u64, YEAR), 9_993);
		assert_eq!(super::compute_total_payout(&I_NPOS, 60_000, 100_000u64, YEAR), 4_379);
		assert_eq!(super::compute_total_payout(&I_NPOS, 75_000, 100_000u64, YEAR), 2_733);
		assert_eq!(super::compute_total_payout(&I_NPOS, 95_000, 100_000u64, YEAR), 2_513);
		assert_eq!(super::compute_total_payout(&I_NPOS, 100_000, 100_000u64, YEAR), 2_505);

		const DAY: u64 = 24 * 60 * 60 * 1000;
		assert_eq!(super::compute_total_payout(&I_NPOS, 25_000, 100_000u64, DAY), 17);
		assert_eq!(super::compute_total_payout(&I_NPOS, 50_000, 100_000u64, DAY), 27);
		assert_eq!(super::compute_total_payout(&I_NPOS, 75_000, 100_000u64, DAY), 7);

		const SIX_HOURS: u64 = 6 * 60 * 60 * 1000;
		assert_eq!(super::compute_total_payout(&I_NPOS, 25_000, 100_000u64, SIX_HOURS), 4);
		assert_eq!(super::compute_total_payout(&I_NPOS, 50_000, 100_000u64, SIX_HOURS), 7);
		assert_eq!(super::compute_total_payout(&I_NPOS, 75_000, 100_000u64, SIX_HOURS), 2);

		const HOUR: u64 = 60 * 60 * 1000;
		assert_eq!(
			super::compute_total_payout(
				&I_NPOS,
				2_500_000_000_000_000_000_000_000_000u128,
				5_000_000_000_000_000_000_000_000_000u128,
				HOUR
			),
			57_038_500_000_000_000_000_000
		);
	}
}
