//! Pure (storage-free) helpers for the vault math.
//!
//! All routines round in the protocol's favor unless explicitly stated:
//! - debt-side accruals round up (`ceil`),
//! - payouts round down (`floor`).
//!
//! Overflow paths use the [`Defensive`] family: in `debug_assertions` builds
//! an overflow panics so it surfaces in tests; in release it logs an error
//! and saturates. None of the inputs the protocol can produce in practice
//! drive these intermediates to overflow.

use frame::deps::{
	frame_support::traits::Defensive,
	sp_runtime::{
		helpers_128bit::multiply_by_rational_with_rounding,
		traits::{One, Zero},
		FixedPointNumber, FixedPointOperand, FixedU128, Rounding,
	},
};
use pusd_primitives::MILLIS_PER_YEAR;

/// `floor(principal * rate * delta_millis / MILLIS_PER_YEAR)`.
///
/// Used to attribute simple interest to a vault. See
/// [`simple_interest_with_rounding`] for the math.
pub fn simple_interest_floor<Balance: FixedPointOperand>(
	principal: Balance,
	rate: FixedU128,
	delta_millis: u64,
) -> Balance {
	simple_interest_with_rounding(principal, rate, delta_millis, Rounding::Down)
}

/// `ceil(principal * rate * delta_millis / MILLIS_PER_YEAR)`.
///
/// Used to mint protocol-favored aggregate interest (§7.3) and upfront fees
/// (§7.5). See [`simple_interest_with_rounding`] for the math.
pub fn simple_interest_ceil<Balance: FixedPointOperand>(
	principal: Balance,
	rate: FixedU128,
	delta_millis: u64,
) -> Balance {
	simple_interest_with_rounding(principal, rate, delta_millis, Rounding::Up)
}

/// Shared back-end for the simple-interest helpers.
///
/// Computes `principal * rate * delta_millis / (DIV * MILLIS_PER_YEAR)` in
/// one shot via [`multiply_by_rational_with_rounding`] — the U256 intermediate
/// avoids the precision loss of computing `factor = rate * (delta/year)`
/// first (for typical sub-1.0 rates over short deltas, the intermediate
/// factor would round to a tiny FixedU128 before we multiplied by principal).
fn simple_interest_with_rounding<Balance: FixedPointOperand>(
	principal: Balance,
	rate: FixedU128,
	delta_millis: u64,
	rounding: Rounding,
) -> Balance {
	if principal.is_zero() || rate.is_zero() || delta_millis == 0 {
		return Balance::zero();
	}
	let p: u128 = principal.unique_saturated_into();
	let rate_times_delta = rate.into_inner().saturating_mul(u128::from(delta_millis));
	let denom = FixedU128::DIV.saturating_mul(u128::from(MILLIS_PER_YEAR));
	multiply_by_rational_with_rounding(p, rate_times_delta, denom, rounding)
		.and_then(|raw| Balance::try_from(raw).ok())
		.defensive_unwrap_or_else(Balance::max_value)
}

/// Compute a vault's collateralization ratio (`collateral * price / debt`).
///
/// Returns `None` if `debt` is zero (the protocol treats CR as undefined and
/// callers must apply specific debt-floor rules) or if either step overflows
/// — better to surface than to silently saturate a safety-critical ratio.
///
/// `checked_mul_int` truncates the fractional part of `price * collateral` at
/// the `Balance` atom; for realistic protocol balances this is below dust and
/// orders of magnitude under CR threshold granularity.
pub fn collateralization_ratio<Balance: FixedPointOperand>(
	collateral: Balance,
	debt: Balance,
	price: FixedU128,
) -> Option<FixedU128> {
	let value = price.checked_mul_int(collateral)?;
	FixedU128::checked_from_rational(value, debt)
}

/// `ceil(weighted_sum / total_ib_debt)` reinterpreted as a `FixedU128`
/// fraction. Returns `One` if `total_ib_debt` is zero, which keeps the
/// upfront-fee formula safe in branches with no pre-existing debt (the new
/// vault dominates the post-change average).
///
/// `weighted_sum = Σ floor(debt_i * rate_i)` and `total_ib_debt = Σ debt_i`.
/// The honest average rate is therefore `weighted_sum / total_ib_debt`
/// interpreted as a fraction in `[0, max_rate]`. We compute
/// `ceil(weighted_sum * 1e18 / total_ib_debt)` via
/// [`multiply_by_rational_with_rounding`] and reinterpret the result as a
/// `FixedU128` inner, which (a) avoids the `weighted_sum < total_ib_debt`
/// integer-truncate trap (typical for sub-1.0 rates), and (b) rounds in the
/// protocol's favor for the upfront fee.
pub fn average_branch_rate<Balance: FixedPointOperand>(
	weighted_sum: Balance,
	total_ib_debt: Balance,
) -> FixedU128 {
	if total_ib_debt.is_zero() {
		return FixedU128::one();
	}
	let w: u128 = weighted_sum.unique_saturated_into();
	let t: u128 = total_ib_debt.unique_saturated_into();
	let inner = multiply_by_rational_with_rounding(w, FixedU128::DIV, t, Rounding::Up)
		.defensive_unwrap_or(u128::MAX);
	FixedU128::from_inner(inner)
}

pub fn redist_per_stake<Balance: FixedPointOperand>(
	num: Balance,
	denom: Balance,
) -> Option<FixedU128> {
	if num.is_zero() {
		return Some(FixedU128::zero());
	}
	assert!(!denom.is_zero(), "redist_per_stake: denom == 0; caller must short-circuit");
	let n: u128 = num.unique_saturated_into();
	let d: u128 = denom.unique_saturated_into();
	assert!(d > 0);
	multiply_by_rational_with_rounding(n, FixedU128::DIV, d, Rounding::Down)
		.map(FixedU128::from_inner)
}

pub fn redist_weight_per_stake<Balance: FixedPointOperand>(
	redistributed_debt: Balance,
	avg_rate: FixedU128,
	denom: Balance,
) -> Option<FixedU128> {
	if redistributed_debt.is_zero() || avg_rate.is_zero() {
		return Some(FixedU128::zero());
	}
	assert!(!denom.is_zero(), "redist_weight_per_stake: denom == 0; caller must short-circuit");
	let n: u128 = redistributed_debt.unique_saturated_into();
	let d: u128 = denom.unique_saturated_into();
	assert!(d > 0);
	multiply_by_rational_with_rounding(n, avg_rate.into_inner(), d, Rounding::Down)
		.map(FixedU128::from_inner)
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame::deps::sp_runtime::traits::Saturating;

	#[test]
	fn simple_interest_floor_zero_inputs() {
		assert_eq!(simple_interest_floor::<u128>(0, FixedU128::one(), 1_000), 0);
		assert_eq!(simple_interest_floor::<u128>(1_000, FixedU128::zero(), 1_000), 0);
		assert_eq!(simple_interest_floor::<u128>(1_000, FixedU128::one(), 0), 0);
	}

	#[test]
	fn simple_interest_floor_basic() {
		// principal=1_000_000, rate=10%, delta=full year => 100_000
		let r = FixedU128::saturating_from_rational(10u32, 100u32);
		let got = simple_interest_floor::<u128>(1_000_000, r, MILLIS_PER_YEAR);
		assert_eq!(got, 100_000);
	}

	#[test]
	fn simple_interest_ceil_rounds_up_on_remainder() {
		// principal=3, rate=1, delta=1ms — fractional, ceils to 1
		let got = simple_interest_ceil::<u128>(3, FixedU128::one(), 1);
		assert_eq!(got, 1);
	}

	#[test]
	fn collateralization_ratio_basic() {
		// collateral=200, debt=100, price=1.0 → CR = 2.0
		let cr = collateralization_ratio::<u128>(200, 100, FixedU128::one());
		assert_eq!(cr, Some(FixedU128::saturating_from_integer(2u128)));
	}

	#[test]
	fn collateralization_ratio_zero_debt_is_none() {
		// CR is undefined when there is no debt.
		let cr = collateralization_ratio::<u128>(100, 0, FixedU128::one());
		assert_eq!(cr, None);
	}

	#[test]
	fn collateralization_ratio_price_scales() {
		// collateral=100, debt=50, price=2.0 → CR = 4.0
		let price = FixedU128::saturating_from_integer(2u128);
		let cr = collateralization_ratio::<u128>(100, 50, price);
		assert_eq!(cr, Some(FixedU128::saturating_from_integer(4u128)));
	}

	#[test]
	fn average_branch_rate_recovers_rate_fraction() {
		// Single vault, debt=10_000 at 5% → weighted_sum = 500.
		// avg_rate = 500 / 10_000 = 0.05.
		let avg = average_branch_rate::<u128>(500, 10_000);
		assert_eq!(avg, FixedU128::from_rational(5u128, 100u128));
	}

	#[test]
	fn average_branch_rate_ceils_in_protocol_favor() {
		// 700 / 10_000 = 0.07 exactly — no remainder, ceil is the floor.
		let avg = average_branch_rate::<u128>(700, 10_000);
		assert_eq!(avg, FixedU128::from_rational(7u128, 100u128));

		// 1 / 3 has an infinite tail; ceil rounds up by one ULP.
		let avg = average_branch_rate::<u128>(1, 3);
		assert!(avg > FixedU128::from_rational(1u128, 3u128));
		// And the over-shoot is bounded by one ULP.
		assert!(
			avg.saturating_sub(FixedU128::from_rational(1u128, 3u128)) <= FixedU128::from_inner(1)
		);
	}

	#[test]
	fn average_branch_rate_zero_debt_returns_one() {
		// Empty branch: avg defaults to 1.0 so the upfront-fee formula is
		// safe for the very first vault.
		let avg = average_branch_rate::<u128>(0, 0);
		assert_eq!(avg, FixedU128::one());
	}

	#[test]
	fn redist_per_stake_round_trips_small_inputs() {
		// 100 / 1000 = 0.1
		let got = redist_per_stake::<u128>(100, 1_000).expect("fits");
		assert_eq!(got, FixedU128::from_rational(1u128, 10u128));
	}

	#[test]
	fn redist_per_stake_zero_num_returns_zero() {
		assert_eq!(redist_per_stake::<u128>(0, 1_000), Some(FixedU128::zero()));
		assert_eq!(redist_per_stake::<u128>(0, 1), Some(FixedU128::zero()));
	}

	#[test]
	fn redist_per_stake_overflow_returns_none() {
		// num/denom = 2^127. Multiplied by 1e18 overflows u128.
		let got = redist_per_stake::<u128>(u128::MAX / 2, 1);
		assert!(got.is_none());
	}

	#[test]
	fn redist_weight_per_stake_matches_two_step_when_safe() {
		// redistributed_debt=10_000, avg_rate=0.05, denom=100
		// → one-shot: floor(10_000 * 0.05_inner / 100) = floor(500 * 1e18 / 100) = 5e18 inner.
		// Two-step `(10_000 / 100) * 0.05` = 5.0 → inner 5e18. Match.
		let avg = FixedU128::from_rational(5u128, 100u128);
		let got = redist_weight_per_stake::<u128>(10_000, avg, 100).expect("fits");
		let two_step = FixedU128::from_rational(10_000u128, 100u128).saturating_mul(avg);
		assert!(got.into_inner().abs_diff(two_step.into_inner()) <= 1);
	}

	#[test]
	fn redist_weight_per_stake_avoids_two_step_overflow() {
		// Pick numbers where the two-step would silently zero via `checked_from_rational`
		// (num/denom > 3.4e20) but the one-shot stays within u128 because avg_rate
		// brings the magnitude back down.
		let avg = FixedU128::from_rational(1u128, u128::from(u64::MAX)); // ~5.4e-20
																   // redistributed_debt = u128::MAX/4, denom = 1: two-step debt_per_stake would
																   // overflow. One-shot folds avg_rate (tiny) into the U256 dividend.
		let got = redist_weight_per_stake::<u128>(u128::MAX / 4, avg, 1);
		assert!(
			got.is_some(),
			"one-shot weight helper should survive when avg_rate keeps the quotient bounded"
		);
	}
}
