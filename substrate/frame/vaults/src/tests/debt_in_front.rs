use crate::{mock::*, tests::rate_pct};
use frame::deps::frame_support::assert_ok;

// `view_debt_in_front` returns the total debt at rates strictly below a given
// rate. A debt-between-two-rates range is derivable from two calls:
// `debt_in_front(high) - debt_in_front(low)`.
#[test]
fn debt_in_front_sums_lower_rate_vaults_only() {
	build_and_execute(|| {
		register_default_branch();
		// Eight vaults: 2 each at 0.5%, 0.6%, 0.7%, 0.8% with distinct debts.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 1000))); // 0.5%
		assert_ok!(open(2, DOT, 1_000, 700, rate_pct(5, 1000)));
		assert_ok!(open(3, DOT, 1_000, 600, rate_pct(6, 1000))); // 0.6%
		assert_ok!(open(4, DOT, 1_000, 800, rate_pct(6, 1000)));
		assert_ok!(open(5, DOT, 1_000, 900, rate_pct(7, 1000))); // 0.7%
		assert_ok!(open(6, DOT, 1_000, 1_000, rate_pct(7, 1000)));
		assert_ok!(open(7, DOT, 1_000, 400, rate_pct(8, 1000))); // 0.8%
		assert_ok!(open(8, DOT, 1_000, 500, rate_pct(8, 1000)));

		// Query: total debt at rates strictly < 0.7%.
		// Sum of vaults 1..=4: 500+700+600+800 = 2600.
		let in_front = crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(7, 1000), u32::MAX);
		assert_eq!(in_front, 2_600);

		// Query: total debt at rates strictly < 0.6%.
		// Sum of vaults 1..=2: 500+700 = 1200.
		let in_front = crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(6, 1000), u32::MAX);
		assert_eq!(in_front, 1_200);

		// Query: total debt at rates strictly < 1% (covers everything).
		let in_front = crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(1, 100), u32::MAX);
		assert_eq!(in_front, 500 + 700 + 600 + 800 + 900 + 1_000 + 400 + 500);

		// The step cap bounds the walk: only the two cheapest vaults are
		// visited, returning the partial sum.
		let capped = crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(1, 100), 2);
		assert_eq!(capped, 500 + 700, "cap of 2 visits only the two tail vaults");
		// A cap at least the list length matches the uncapped result.
		let exact = crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(1, 100), 8);
		assert_eq!(exact, 500 + 700 + 600 + 800 + 900 + 1_000 + 400 + 500);
	});
}

// The walk sums recorded *principal*, never the accrued interest folded into a
// vault's stored debt on touch.
#[test]
fn debt_in_front_counts_principal_not_total_debt() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 5_000, 500, rate_pct(5, 1000))); // 0.5%
		assert_ok!(open(2, DOT, 5_000, 700, rate_pct(6, 1000))); // 0.6%
														   // Accrue a year of interest and materialise it into stored debt.
		advance_time(365 * 24 * 3_600 * 1_000);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(9), 1, DOT));
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(9), 2, DOT));
		let v1 = crate::pallet::Vaults::<Test>::get(DOT, 1).expect("v1");
		assert!(v1.debt.interest > 0, "interest must have materialised on top of principal");
		// Still just the principals: 500 + 700.
		let in_front = crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(1, 100), u32::MAX);
		assert_eq!(in_front, 1_200);
	});
}

// A vault redeemed out of the rate index (to Dormant) is no longer walked, so
// its principal leaves the debt-in-front total.
#[test]
fn debt_in_front_excludes_dormant_vaults() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 5_000, 500, rate_pct(5, 1000))); // 0.5%, tail
		assert_ok!(open(2, DOT, 5_000, 700, rate_pct(6, 1000))); // 0.6%
		assert_eq!(crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(1, 100), u32::MAX), 1_200);
		// Fully redeem the tail vault (acct 1) → Dormant, out of the rate index.
		assert_ok!(redeem(DOT, 3, 600));
		assert_eq!(
			crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(1, 100), u32::MAX),
			700,
			"dormant vault's principal is no longer counted"
		);
	});
}

// A zero step budget and an empty rate index both return zero.
#[test]
fn debt_in_front_zero_for_no_steps_or_empty_index() {
	build_and_execute(|| {
		register_default_branch();
		// Empty rate index → nothing in front.
		assert_eq!(crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(1, 100), u32::MAX), 0);
		assert_ok!(open(1, DOT, 5_000, 500, rate_pct(5, 1000)));
		// A zero step budget visits no vaults.
		assert_eq!(crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(1, 100), 0), 0);
	});
}
