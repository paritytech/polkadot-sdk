//! Port of liquity_v2/contracts/test/HintHelpers.t.sol (lines 22994-23028).
//!
//! Single test (row 1): `getApproxHint` must skip "Zombie" (Dormant) vaults
//! when serving frontends a starting position to seed `findInsertPosition`.
//!
//! polkadot equivalent: when a vault transitions to `Dormant` it is removed
//! from the rate index entirely (helpers.rs::apply_redemption — see the
//! `was_active` guard around `T::RateIndex::remove`). The hint query
//! `find_rate_position` therefore can never return a Dormant vault — it's
//! a structural invariant rather than a hint-helper filter.

use crate::{mock::*, pallet::Vaults, tests::rate_pct};
use frame::deps::frame_support::assert_ok;
use pallet_linked_list::SortedListInterface;

// row 1: test_GetApproxHintNeverReturnsZombies.
#[test]
fn find_rate_position_skips_dormant_vaults() {
	build_and_execute(|| {
		register_default_branch();
		// Five vaults at 1%, 2%, 3%, 4%, 5%.
		for (who, pct) in [(1u64, 1), (2, 2), (3, 3), (4, 4), (5, 5)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}

		// Redeem acct 1's full debt. apply_redemption transitions vault to
		// Dormant when residual debt is zero (see interfaces.rs).
		let target = redeem(DOT, 5, 600).expect("redeem ok"); // 600 > vault 1's debt to fully clear it
		assert_eq!(target, 1);
		let v_after = Vaults::<Test>::get(DOT, 1).unwrap();
		// Vault is Dormant or its debt is below MinimumDebt — either way it
		// should be out of the rate index.
		assert!(v_after.status.is_dormant());
		assert!(!<LinkedList as SortedListInterface<u32, u64>>::contains(&DOT, &1));

		// Now query a hint at a rate near acct 1's old rate. The result
		// must not name acct 1 — it's no longer in the index.
		let pos = crate::Pallet::<Test>::find_rate_position(DOT, rate_pct(15, 1000)); // 1.5%
		assert_ne!(pos.prev, Some(1));
		assert_ne!(pos.next, Some(1));
	});
}
