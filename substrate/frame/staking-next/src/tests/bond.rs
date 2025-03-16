use super::*;

#[test]
fn existing_stash_cannot_bond() {
	ExtBuilder::default().build_and_execute(|| {
		assert!(StakingLedger::<T>::is_bonded(11.into()));

		// cannot bond again.
		assert_noop!(
			Staking::bond(RuntimeOrigin::signed(11), 7, RewardDestination::Staked),
			Error::<Test>::AlreadyBonded,
		);
	});
}

#[test]
fn existing_controller_cannot_bond() {
	ExtBuilder::default().build_and_execute(|| {
		let (_stash, controller) = testing_utils::create_unique_stash_controller::<Test>(
			0,
			7,
			RewardDestination::Staked,
			false,
		)
		.unwrap();

		assert_noop!(
			Staking::bond(RuntimeOrigin::signed(controller), 7, RewardDestination::Staked),
			Error::<Test>::AlreadyPaired,
		);
	});
}

#[test]
fn cannot_bond_less_than_ed() {}

#[test]
fn bond_truncated_to_maximum_possible() {}
