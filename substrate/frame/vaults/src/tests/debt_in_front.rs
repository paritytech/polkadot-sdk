use crate::{mock::*, tests::rate_pct};
use frame::deps::frame_support::assert_ok;

// row 1: test_GetDebtBetweenInterestRates (reformulated as
// "debt-in-front-of-rate", since polkadot doesn't have the
// `getDebtBetweenInterestRates(low, high, …)` Liquity helper — it has the
// simpler `view_debt_in_front` from which the Liquity range can be derived
// as `view_debt_in_front(high) - view_debt_in_front(low)`).
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
		let in_front = crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(7, 1000));
		assert_eq!(in_front, 2_600);

		// Query: total debt at rates strictly < 0.6%.
		// Sum of vaults 1..=2: 500+700 = 1200.
		let in_front = crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(6, 1000));
		assert_eq!(in_front, 1_200);

		// Query: total debt at rates strictly < 1% (covers everything).
		let in_front = crate::Pallet::<Test>::debt_in_front(DOT, rate_pct(1, 100));
		assert_eq!(in_front, 500 + 700 + 600 + 800 + 900 + 1_000 + 400 + 500);
	});
}

// row 2: test_GetDebtBetweenInterestRateAndTrove. Polkadot's
// `view_debt_in_front` doesn't take a stop-at-vault id directly. Frontends
// would compose it: query `debt_in_front(target_rate)`, then query the
// target vault's specific debt and subtract any portion not yet in front.
// Captured here for completeness; no separate Rust test required.
