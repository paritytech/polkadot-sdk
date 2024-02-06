use frame_support::storage::with_storage_layer;
use sp_staking::StakingInterface;

use super::*;

#[test]
fn force_unstake_with_slash_works() {
	ExtBuilder::default().build_and_execute(|| {
		// without slash
		let _ = with_storage_layer::<(), _, _>(|| {
			// bond an account, can unstake
			assert_eq!(Staking::bonded(&11), Some(11));
			assert_ok!(<Staking as StakingInterface>::force_unstake(11));
			Err(DispatchError::from("revert"))
		});

		// bond again and add a slash, still can unstake.
		assert_eq!(Staking::bonded(&11), Some(11));
		add_slash(&11);
		assert_ok!(<Staking as StakingInterface>::force_unstake(11));
	});
}

#[test]
fn do_withdraw_unbonded_with_wrong_slash_spans_works_as_expected() {
	ExtBuilder::default().build_and_execute(|| {
		on_offence_now(
			&[OffenceDetails {
				offender: (11, Staking::eras_stakers(active_era(), &11)),
				reporters: vec![],
			}],
			&[Perbill::from_percent(100)],
		);

		assert_eq!(Staking::bonded(&11), Some(11));

		assert_noop!(
			Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0),
			Error::<Test>::IncorrectSlashingSpans
		);

		let num_slashing_spans = Staking::slashing_spans(&11).map_or(0, |s| s.iter().count());
		assert_ok!(Staking::withdraw_unbonded(
			RuntimeOrigin::signed(11),
			num_slashing_spans as u32
		));
	});
}

#[test]
fn status() {
	ExtBuilder::default().build_and_execute(|| {
		// stash of a validator is identified as a validator
		assert_eq!(Staking::status(&11).unwrap(), StakerStatus::Validator);
		// .. but not the controller.
		assert!(Staking::status(&10).is_err());

		// stash of nominator is identified as a nominator
		assert_eq!(Staking::status(&101).unwrap(), StakerStatus::Nominator(vec![11, 21]));
		// .. but not the controller.
		assert!(Staking::status(&100).is_err());

		// stash of chilled is identified as a chilled
		assert_eq!(Staking::status(&41).unwrap(), StakerStatus::Idle);
		// .. but not the controller.
		assert!(Staking::status(&40).is_err());

		// random other account.
		assert!(Staking::status(&42).is_err());
	})
}
