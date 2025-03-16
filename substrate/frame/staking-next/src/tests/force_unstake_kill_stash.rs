use super::*;

#[test]
fn force_unstake_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(Staking::bonded(&11), Some(11));

		// Is bonded -- cannot transfer
		assert_noop!(
			Balances::transfer_allow_death(RuntimeOrigin::signed(11), 1, 10),
			TokenError::FundsUnavailable,
		);

		// Force unstake requires root.
		assert_noop!(Staking::force_unstake(RuntimeOrigin::signed(11), 11, 0), BadOrigin);

		// slashing span doesn't matter, can be any value.
		hypothetically! {{
			assert_ok!(Staking::force_unstake(RuntimeOrigin::root(), 11, 42));
		}};

		assert_ok!(Staking::force_unstake(RuntimeOrigin::root(), 11, 0));

		// No longer bonded, can transfer out
		assert_eq!(Staking::bonded(&11), None);
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(11), 1, 10));
	});
}

#[test]
fn kill_stash_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(Staking::bonded(&11), Some(11));

		assert_noop!(Staking::kill_stash(&12, 0), Error::<Test>::NotStash);

		// slashing spans don't matter, can be any value
		hypothetically!({
			assert_ok!(Staking::kill_stash(&11, 42));
		});

		assert_ok!(Staking::kill_stash(&11, 2));
		assert_eq!(Staking::bonded(&11), None);
	});
}
