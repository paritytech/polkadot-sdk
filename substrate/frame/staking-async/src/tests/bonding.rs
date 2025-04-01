use frame_support::hypothetically_ok;

use super::*;

#[test]
fn existing_stash_cannot_bond() {
	ExtBuilder::default().build_and_execute(|| {
		assert!(StakingLedger::<T>::is_bonded(11.into()));

		// cannot bond again.
		assert_noop!(
			Staking::bond(RuntimeOrigin::signed(11), 7, RewardDestination::Staked),
			Error::<T>::AlreadyBonded,
		);
	});
}

#[test]
fn existing_controller_cannot_bond() {
	ExtBuilder::default().build_and_execute(|| {
		let (_stash, controller) = testing_utils::create_unique_stash_controller::<T>(
			0,
			7,
			RewardDestination::Staked,
			false,
		)
		.unwrap();

		assert_noop!(
			Staking::bond(RuntimeOrigin::signed(controller), 7, RewardDestination::Staked),
			Error::<T>::AlreadyPaired,
		);
	});
}

#[test]
fn cannot_transfer_staked_balance() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		assert_eq!(asset::staked::<T>(&11), 1000);
		// stake + ed
		assert_eq!(asset::total_balance::<T>(&11), 1000 + 1);
		// nothing more to stake
		assert_eq!(asset::stakeable_balance::<T>(&11), 1000);
		assert_eq!(asset::free_to_stake::<T>(&11), 0);

		// cannot transfer
		assert_noop!(
			Balances::transfer_allow_death(RuntimeOrigin::signed(11), 21, 1),
			TokenError::Frozen,
		);

		let _ = asset::set_stakeable_balance::<T>(&11, 10000);

		// now it can
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(11), 21, 1));
	});
}

#[test]
fn cannot_reserve_staked_balance() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(asset::staked::<T>(&11), 1000);

		// Confirm account 11 cannot reserve as a result
		assert_noop!(
			Balances::reserve(&11, 2),
			pallet_balances::Error::<T, _>::InsufficientBalance
		);
		assert_noop!(Balances::reserve(&11, 1), DispatchError::ConsumerRemaining);

		// Give account 11 extra free balance
		let _ = asset::set_stakeable_balance::<T>(&11, 1000 + 1000);
		assert_eq!(asset::free_to_stake::<T>(&11), 1000);

		// Confirm account 11 can now reserve balance
		assert_ok!(Balances::reserve(&11, 500));

		// free to stake balance has reduced
		assert_eq!(asset::free_to_stake::<T>(&11), 500);
	});
}

#[test]
fn cannot_bond_less_than_ed() {
	ExtBuilder::default().existential_deposit(10).build_and_execute(|| {
		// given
		assert_eq!(asset::staked_and_not::<T>(&1), (0, 10));

		// cannot bond less than existential deposit
		assert_noop!(
			Staking::bond(RuntimeOrigin::signed(1), 9, RewardDestination::Staked),
			Error::<T>::InsufficientBond,
		);

		// can bond existential deposit
		assert_ok!(Staking::bond(RuntimeOrigin::signed(1), 10, RewardDestination::Staked));
		assert_eq!(asset::staked_and_not::<T>(&1), (10, 0));
	});
}

#[test]
fn bond_truncated_to_maximum_possible() {
	ExtBuilder::default().build_and_execute(|| {
		// given
		assert_eq!(asset::free_to_stake::<T>(&1), 10);

		// then bonding 100 is equal to bonding 10
		assert_ok!(Staking::bond(RuntimeOrigin::signed(1), 100, RewardDestination::Staked));
		assert_eq!(Staking::ledger(1.into()).unwrap().total, 10);
	});
}

#[test]
fn bond_extra_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// given
		asset::set_stakeable_balance::<T>(&11, 1000000);

		// when
		assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(11), 100));

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000 + 100,
				active: 1000 + 100,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// when
		assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(11), Balance::max_value()));

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000000,
				active: 1000000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);
	});
}

#[test]
fn bond_extra_controller_bad_state_works() {
	ExtBuilder::default().try_state(false).build_and_execute(|| {
		assert_eq!(StakingLedger::<T>::get(StakingAccount::Stash(31)).unwrap().stash, 31);

		// simulate ledger in bad state: the controller 41 is associated to the stash 31 and 41.
		Bonded::<T>::insert(31, 41);

		// we confirm that the ledger is in bad state: 31 has 41 as controller and when fetching
		// the ledger associated with the controller 41, its stash is 41 (and not 31).
		assert_eq!(Ledger::<T>::get(41).unwrap().stash, 41);

		// if the ledger is in this bad state, the `bond_extra` should fail.
		// TODO: remove this BadState, we should no longer have it at all.
		assert_noop!(Staking::bond_extra(RuntimeOrigin::signed(31), 10), Error::<T>::BadState);
	})
}

#[test]
fn bond_extra_updates_exposure_later_if_exposed() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// given
		assert_eq!(active_era(), 1);
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);
		assert_eq!(
			Staking::eras_stakers(active_era(), &11),
			Exposure { total: 1000, own: 1000, others: vec![] }
		);

		// when
		asset::set_stakeable_balance::<T>(&11, 1000000);
		Staking::bond_extra(RuntimeOrigin::signed(11), 100).unwrap();

		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000 + 100,
				active: 1000 + 100,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);
		// Exposure is a snapshot! only updated after the next era update.
		assert_ne!(
			Staking::eras_stakers(active_era(), &11),
			Exposure { total: 1000 + 100, own: 1000 + 100, others: vec![] }
		);

		// when trigger next era
		Session::roll_until_active_era(2);

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000 + 100,
				active: 1000 + 100,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);
		// Exposure is now updated
		assert_eq!(
			Staking::eras_stakers(active_era(), &11),
			Exposure { total: 1000 + 100, own: 1000 + 100, others: vec![] }
		);
	})
}

#[test]
fn unbonding_works() {
	ExtBuilder::default().build_and_execute(|| {
		// given
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// when
		Staking::unbond(RuntimeOrigin::signed(11), 500).unwrap();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::Unbonded { stash: 11, amount: 500 }]
		);

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 500,
				unlocking: bounded_vec![UnlockChunk { value: 500, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			},
		);

		// when
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));
		assert_eq!(staking_events_since_last_call(), vec![]);

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 500,
				unlocking: bounded_vec![UnlockChunk { value: 500, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			},
		);

		// when
		Session::roll_until_active_era(2);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 500,
				unlocking: bounded_vec![UnlockChunk { value: 500, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			},
		);

		// when
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));
		assert_eq!(staking_events_since_last_call(), vec![]);

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 500,
				unlocking: bounded_vec![UnlockChunk { value: 500, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			},
		);

		// when
		Session::roll_until_active_era(4);
		let _ = staking_events_since_last_call();
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::Withdrawn { stash: 11, amount: 500 }]
		);

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 500,
				active: 500,
				unlocking: bounded_vec![],
				legacy_claimed_rewards: bounded_vec![],
			},
		);
	});
}

#[test]
fn unbonding_multi_chunk() {
	ExtBuilder::default().build_and_execute(|| {
		// given
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// when
		Staking::unbond(RuntimeOrigin::signed(11), 500).unwrap();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::Unbonded { stash: 11, amount: 500 }]
		);

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 500,
				unlocking: bounded_vec![UnlockChunk { value: 500, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			},
		);

		// when
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();
		Staking::unbond(RuntimeOrigin::signed(11), 250).unwrap();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::Unbonded { stash: 11, amount: 250 }]
		);

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 250,
				unlocking: bounded_vec![
					UnlockChunk { value: 500, era: 1 + 3 },
					UnlockChunk { value: 250, era: 2 + 3 }
				],
				legacy_claimed_rewards: bounded_vec![],
			},
		);

		// when
		Session::roll_until_active_era(4);
		let _ = staking_events_since_last_call();
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::Withdrawn { stash: 11, amount: 500 }]
		);

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 500,
				active: 250,
				unlocking: bounded_vec![UnlockChunk { value: 250, era: 2 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			},
		);

		// when
		Session::roll_until_active_era(5);
		let _ = staking_events_since_last_call();
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::Withdrawn { stash: 11, amount: 250 }]
		);

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 250,
				active: 250,
				unlocking: bounded_vec![],
				legacy_claimed_rewards: bounded_vec![],
			},
		);
	});
}

#[test]
fn full_unbonding_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(asset::free_to_stake::<T>(&11), 0);
		// cannot fully unbond as they are a validator
		assert_noop!(
			Staking::unbond(RuntimeOrigin::signed(11), 1000),
			Error::<T>::InsufficientBond
		);

		// first chill
		assert_ok!(Staking::chill(RuntimeOrigin::signed(11)));

		// then fully unbond
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 1000));
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::Chilled { stash: 11 }, Event::Unbonded { stash: 11, amount: 1000 }]
		);

		// wait 3 eras
		Session::roll_until_active_era(active_era() + 3);
		let _ = staking_events_since_last_call();

		// done
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 0));
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::StakerRemoved { stash: 11 }, Event::Withdrawn { stash: 11, amount: 1000 }]
		);

		// storage is clean, balance is unheld
		StakingLedger::<T>::assert_stash_killed(11);
		assert_eq!(asset::free_to_stake::<T>(&11), 1000);
	});
}

#[test]
fn unbonding_merges_if_era_exists() {
	ExtBuilder::default().build_and_execute(|| {
		// given
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// when
		Staking::unbond(RuntimeOrigin::signed(11), 500).unwrap();

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 500,
				unlocking: bounded_vec![UnlockChunk { value: 500, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			},
		);

		// when
		Staking::unbond(RuntimeOrigin::signed(11), 250).unwrap();

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 250,
				unlocking: bounded_vec![UnlockChunk { value: 500 + 250, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			},
		);
	});
}

#[test]
fn unbonding_rejects_if_max_chunks() {
	ExtBuilder::default()
		.max_unlock_chunks(3)
		.bonding_duration(7)
		.build_and_execute(|| {
			// given
			assert_eq!(
				Staking::ledger(11.into()).unwrap(),
				StakingLedgerInspect {
					stash: 11,
					total: 1000,
					active: 1000,
					unlocking: Default::default(),
					legacy_claimed_rewards: bounded_vec![],
				}
			);

			// when
			Staking::unbond(RuntimeOrigin::signed(11), 250).unwrap();
			Session::roll_until_active_era(2);
			Staking::unbond(RuntimeOrigin::signed(11), 250).unwrap();
			Session::roll_until_active_era(3);
			Staking::unbond(RuntimeOrigin::signed(11), 250).unwrap();

			// then
			assert_eq!(
				Staking::ledger(11.into()).unwrap(),
				StakingLedgerInspect {
					stash: 11,
					total: 1000,
					active: 250,
					unlocking: bounded_vec![
						UnlockChunk { value: 250, era: 1 + 7 },
						UnlockChunk { value: 250, era: 2 + 7 },
						UnlockChunk { value: 250, era: 3 + 7 },
					],
					legacy_claimed_rewards: bounded_vec![],
				},
			);

			// when
			Session::roll_until_active_era(4);
			assert_noop!(Staking::unbond(RuntimeOrigin::signed(11), 100), Error::<T>::NoMoreChunks,);
		});
}

#[test]
fn unbonding_auto_withdraws_if_any() {
	ExtBuilder::default().max_unlock_chunks(3).build_and_execute(|| {
		// given
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// when
		Staking::unbond(RuntimeOrigin::signed(11), 250).unwrap();
		Session::roll_until_active_era(2);
		Staking::unbond(RuntimeOrigin::signed(11), 250).unwrap();
		Session::roll_until_active_era(3);
		Staking::unbond(RuntimeOrigin::signed(11), 250).unwrap();

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 250,
				unlocking: bounded_vec![
					UnlockChunk { value: 250, era: 1 + 3 },
					UnlockChunk { value: 250, era: 2 + 3 },
					UnlockChunk { value: 250, era: 3 + 3 },
				],
				legacy_claimed_rewards: bounded_vec![],
			},
		);

		// when
		Session::roll_until_active_era(4);
		// then they can unbond more, as it does auto withdraw of the first chunk
		Staking::unbond(RuntimeOrigin::signed(11), 100).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 750,
				active: 150,
				unlocking: bounded_vec![
					UnlockChunk { value: 250, era: 2 + 3 },
					UnlockChunk { value: 250, era: 3 + 3 },
					UnlockChunk { value: 100, era: 4 + 3 },
				],
				legacy_claimed_rewards: bounded_vec![],
			},
		);
	});
}

#[test]
fn unbonding_caps_to_ledger_active() {
	ExtBuilder::default().set_status(11, StakerStatus::Idle).build_and_execute(|| {
		// given
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// when
		Staking::unbond(RuntimeOrigin::signed(11), 1500).unwrap();

		// then
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 0,
				unlocking: bounded_vec![UnlockChunk { value: 1000, era: 1 + 3 }],
				legacy_claimed_rewards: Default::default(),
			}
		);
	});
}

#[test]
fn unbond_avoids_dust() {
	ExtBuilder::default()
		.existential_deposit(5)
		.set_status(11, StakerStatus::Idle)
		.build_and_execute(|| {
			// given
			assert_eq!(
				Staking::ledger(11.into()).unwrap(),
				StakingLedgerInspect {
					stash: 11,
					total: 1000,
					active: 1000,
					unlocking: Default::default(),
					legacy_claimed_rewards: bounded_vec![],
				}
			);

			// when
			Staking::unbond(RuntimeOrigin::signed(11), 998).unwrap();

			// then
			assert_eq!(
				Staking::ledger(11.into()).unwrap(),
				StakingLedgerInspect {
					stash: 11,
					total: 1000,
					active: 0,
					unlocking: bounded_vec![UnlockChunk { value: 1000, era: 1 + 3 }],
					legacy_claimed_rewards: Default::default(),
				}
			);
		});
}

#[test]
fn unbond_rejects_if_min_role_bond_not_met() {
	ExtBuilder::default().min_validator_bond(100).build_and_execute(|| {
		// given
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// then
		assert_noop!(Staking::unbond(RuntimeOrigin::signed(11), 950), Error::<T>::InsufficientBond);

		// can unbond to a value less than 100 remaining
		hypothetically_ok!(Staking::unbond(RuntimeOrigin::signed(11), 850));

		hypothetically!({
			// can also chill and then unbond more.
			assert_ok!(Staking::chill(RuntimeOrigin::signed(11)));
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 950));
		})
	})
}

#[test]
fn rebond_works() {
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// given
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// nothing to rebond
		assert_noop!(Staking::rebond(RuntimeOrigin::signed(11), 500), Error::<Test>::NoUnlockChunk);

		// given
		Staking::unbond(RuntimeOrigin::signed(11), 900).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 100,
				unlocking: bounded_vec![UnlockChunk { value: 900, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// then rebond all the funds unbonded.
		Staking::rebond(RuntimeOrigin::signed(11), 900).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// Unbond almost all of the funds in stash.
		Staking::unbond(RuntimeOrigin::signed(11), 900).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 100,
				unlocking: bounded_vec![UnlockChunk { value: 900, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// Re-bond part of the funds unbonded.
		Staking::rebond(RuntimeOrigin::signed(11), 500).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 600,
				unlocking: bounded_vec![UnlockChunk { value: 400, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// Re-bond the remainder of the funds unbonded.
		Staking::rebond(RuntimeOrigin::signed(11), 500).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// Unbond parts of the funds in stash.
		Staking::unbond(RuntimeOrigin::signed(11), 300).unwrap();
		Staking::unbond(RuntimeOrigin::signed(11), 300).unwrap();
		Staking::unbond(RuntimeOrigin::signed(11), 300).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 100,
				unlocking: bounded_vec![UnlockChunk { value: 900, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// Re-bond part of the funds unbonded.
		Staking::rebond(RuntimeOrigin::signed(11), 500).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 600,
				unlocking: bounded_vec![UnlockChunk { value: 400, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			}
		);
	})
}

#[test]
fn rebond_is_fifo() {
	ExtBuilder::default().build_and_execute(|| {
		// given
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 1000,
				unlocking: Default::default(),
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// Unbond some of the funds in stash.
		Staking::unbond(RuntimeOrigin::signed(11), 400).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 600,
				unlocking: bounded_vec![UnlockChunk { value: 400, era: 1 + 3 }],
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		Session::roll_until_active_era(2);

		// Unbond more of the funds in stash.
		Staking::unbond(RuntimeOrigin::signed(11), 300).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 300,
				unlocking: bounded_vec![
					UnlockChunk { value: 400, era: 1 + 3 },
					UnlockChunk { value: 300, era: 2 + 3 },
				],
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		Session::roll_until_active_era(3);

		// Unbond yet more of the funds in stash.
		Staking::unbond(RuntimeOrigin::signed(11), 200).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 100,
				unlocking: bounded_vec![
					UnlockChunk { value: 400, era: 1 + 3 },
					UnlockChunk { value: 300, era: 2 + 3 },
					UnlockChunk { value: 200, era: 3 + 3 },
				],
				legacy_claimed_rewards: bounded_vec![],
			}
		);

		// Re-bond half of the unbonding funds.
		Staking::rebond(RuntimeOrigin::signed(11), 400).unwrap();
		assert_eq!(
			Staking::ledger(11.into()).unwrap(),
			StakingLedgerInspect {
				stash: 11,
				total: 1000,
				active: 500,
				unlocking: bounded_vec![
					UnlockChunk { value: 400, era: 1 + 3 },
					UnlockChunk { value: 100, era: 2 + 3 },
				],
				legacy_claimed_rewards: bounded_vec![],
			}
		);
	})
}

// #[test]
// fn rebond_emits_right_value_in_event() {
// 	// When a user calls rebond with more than can be rebonded, things succeed,
// 	// and the rebond event emits the actual value rebonded.
// 	ExtBuilder::default().nominate(false).build_and_execute(|| {
// 		// Set payee to stash.
// 		assert_ok!(Staking::set_payee(RuntimeOrigin::signed(11), RewardDestination::Stash));

// 		// Give account 11 some large free balance greater than total
// 		let _ = asset::set_stakeable_balance::<Test>(&11, 1000000);

// 		// confirm that 10 is a normal validator and gets paid at the end of the era.
// 		mock::start_active_era(1);

// 		// Unbond almost all of the funds in stash.
// 		Staking::unbond(RuntimeOrigin::signed(11), 900).unwrap();
// 		assert_eq!(
// 			Staking::ledger(11.into()).unwrap(),
// 			StakingLedgerInspect {
// 				stash: 11,
// 				total: 1000,
// 				active: 100,
// 				unlocking: bounded_vec![UnlockChunk { value: 900, era: 1 + 3 }],
// 				legacy_claimed_rewards: bounded_vec![],
// 			}
// 		);

// 		// Re-bond less than the total
// 		Staking::rebond(RuntimeOrigin::signed(11), 100).unwrap();
// 		assert_eq!(
// 			Staking::ledger(11.into()).unwrap(),
// 			StakingLedgerInspect {
// 				stash: 11,
// 				total: 1000,
// 				active: 200,
// 				unlocking: bounded_vec![UnlockChunk { value: 800, era: 1 + 3 }],
// 				legacy_claimed_rewards: bounded_vec![],
// 			}
// 		);
// 		// Event emitted should be correct
// 		assert_eq!(*staking_events().last().unwrap(), Event::Bonded { stash: 11, amount: 100 });

// 		// Re-bond way more than available
// 		Staking::rebond(RuntimeOrigin::signed(11), 100_000).unwrap();
// 		assert_eq!(
// 			Staking::ledger(11.into()).unwrap(),
// 			StakingLedgerInspect {
// 				stash: 11,
// 				total: 1000,
// 				active: 1000,
// 				unlocking: Default::default(),
// 				legacy_claimed_rewards: bounded_vec![],
// 			}
// 		);
// 		// Event emitted should be correct, only 800
// 		assert_eq!(*staking_events().last().unwrap(), Event::Bonded { stash: 11, amount: 800 });
// 	});
// }
