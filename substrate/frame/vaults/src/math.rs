//! Pure (storage-free) helpers for the vault math.
//!
//! All routines round in the protocol's favor unless explicitly stated:
//! - debt-side accruals round up (`ceil`),
//! - payouts round down (`floor`).

use frame::deps::sp_runtime::{
	helpers_128bit::multiply_by_rational_with_rounding,
	traits::{CheckedDiv, One, Saturating, Zero},
	FixedPointNumber, FixedPointOperand, FixedU128, Rounding,
};
use pusd_primitives::MILLIS_PER_YEAR;

/// `floor(principal * rate * delta_millis / MILLIS_PER_YEAR)`.
///
/// Used to attribute simple interest to a vault. Returns `0` when any factor
/// is zero. Computed in a single step via [`multiply_by_rational_with_rounding`]
/// to avoid the precision loss of computing `factor = rate * (delta/year)`
/// first and then multiplying by `principal` (the intermediate factor would
/// round to a tiny FixedU128 for typical sub-1.0 rates and short deltas).
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
/// (§7.5).
pub fn simple_interest_ceil<Balance: FixedPointOperand>(
	principal: Balance,
	rate: FixedU128,
	delta_millis: u64,
) -> Balance {
	simple_interest_with_rounding(principal, rate, delta_millis, Rounding::Up)
}

/// Shared back-end for [`simple_interest_floor`] and [`simple_interest_ceil`]:
/// computes `principal * rate * delta_millis / (DIV * MILLIS_PER_YEAR)` using
/// a 256-bit intermediate so the multiply doesn't overflow at extreme rates,
/// then rounds per `rounding`. Saturates to `Balance::max_value()` if the
/// quotient would overflow (would require absurd inputs in practice).
fn simple_interest_with_rounding<Balance: FixedPointOperand>(
	principal: Balance,
	rate: FixedU128,
	delta_millis: u64,
	rounding: Rounding,
) -> Balance {
	let principal_u128: u128 = principal.unique_saturated_into();
	if principal_u128.is_zero() || rate.is_zero() || delta_millis == 0 {
		return Balance::zero();
	}
	let rate_times_delta = rate.into_inner().saturating_mul(delta_millis as u128);
	let denom = FixedU128::DIV.saturating_mul(MILLIS_PER_YEAR as u128);
	let raw = multiply_by_rational_with_rounding(principal_u128, rate_times_delta, denom, rounding)
		.unwrap_or(u128::MAX);
	Balance::try_from(raw).unwrap_or_else(|_| Balance::max_value())
}

/// Compute a vault's collateralization ratio (`collateral_value / debt`).
///
/// Returns `None` if `debt` is zero (the protocol treats CR as undefined in
/// that case — callers must apply specific debt-floor rules instead) or if
/// the FixedU128 division overflows / fails defensively.
pub fn collateralization_ratio<Balance: FixedPointOperand>(
	collateral: Balance,
	debt: Balance,
	price: FixedU128,
) -> Option<FixedU128> {
	if debt.is_zero() {
		return None;
	}
	let collateral_value = price.saturating_mul(FixedU128::saturating_from_integer(collateral));
	collateral_value.checked_div(&FixedU128::saturating_from_integer(debt))
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
	let inner =
		multiply_by_rational_with_rounding(w, FixedU128::DIV, t, Rounding::Up).unwrap_or(u128::MAX);
	FixedU128::from_inner(inner)
}

#[cfg(test)]
mod tests {
	use super::*;

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
}
