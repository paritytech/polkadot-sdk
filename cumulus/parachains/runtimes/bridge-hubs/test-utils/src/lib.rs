// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Module contains predefined test-case scenarios for "BridgeHub" `Runtime`s.

pub mod test_cases;
pub mod test_data;

pub use bp_test_utils::test_header;
pub use parachains_runtimes_test_utils::*;
use sp_runtime::Perbill;

/// A helper function for comparing the actual value of a fee constant with its estimated value. The
/// estimated value can be overestimated (`overestimate_in_percent`), and if the difference to the
/// actual value is below `margin_overestimate_diff_in_percent_for_lowering`, we should lower the
/// actual value.
pub fn check_sane_fees_values(
	const_name: &str,
	actual: u128,
	calculate_estimated_fee: fn() -> u128,
	overestimate_in_percent: Perbill,
	margin_overestimate_diff_in_percent_for_lowering: Option<i16>,
	label: &str,
) {
	let estimated = calculate_estimated_fee();
	let estimated_plus_overestimate = estimated + (overestimate_in_percent * estimated);
	let diff_to_estimated = diff_as_percent(actual, estimated);
	let diff_to_estimated_plus_overestimate = diff_as_percent(actual, estimated_plus_overestimate);

	sp_tracing::try_init_simple();
	log::error!(
		target: "bridges::estimate",
		"{label}:\nconstant: {const_name}\n[+] actual: {actual}\n[+] estimated: {estimated} ({diff_to_estimated:.2?})\n[+] estimated(+33%): {estimated_plus_overestimate} ({diff_to_estimated_plus_overestimate:.2?})",
	);

	// check if estimated value is sane
	assert!(
		estimated <= actual,
		"estimated: {estimated}, actual: {actual}, please adjust `{const_name}` to the value: {estimated_plus_overestimate}",
	);
	assert!(
		estimated_plus_overestimate <= actual,
		"estimated_plus_overestimate: {estimated_plus_overestimate}, actual: {actual}, please adjust `{const_name}` to the value: {estimated_plus_overestimate}",
	);

	if let Some(margin_overestimate_diff_in_percent_for_lowering) =
		margin_overestimate_diff_in_percent_for_lowering
	{
		assert!(
            diff_to_estimated_plus_overestimate > margin_overestimate_diff_in_percent_for_lowering as f64,
            "diff_to_estimated_plus_overestimate: {diff_to_estimated_plus_overestimate:.2}, overestimate_diff_in_percent_for_lowering: {margin_overestimate_diff_in_percent_for_lowering}, please adjust `{const_name}` to the value: {estimated_plus_overestimate}",
        );
	}
}

pub fn diff_as_percent(left: u128, right: u128) -> f64 {
	let left = left as f64;
	let right = right as f64;
	((left - right).abs() / left) * 100f64 * (if left >= right { -1 } else { 1 }) as f64
}

#[test]
fn diff_as_percent_works() {
	assert_eq!(-20_f64, diff_as_percent(100, 80));
	assert_eq!(25_f64, diff_as_percent(80, 100));
	assert_eq!(33_f64, diff_as_percent(13351000000, 17756830000));
}
