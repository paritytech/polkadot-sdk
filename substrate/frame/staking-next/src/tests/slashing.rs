use super::*;

#[test]
fn nominators_also_get_slashed_pro_rata() {
	ExtBuilder::default()
		.validator_count(4)
		.set_status(41, StakerStatus::Validator)
		.build_and_execute(|| {
			let initial_exposure = Staking::eras_stakers(active_era(), &11);
			assert_eq!(
				initial_exposure,
				Exposure {
					total: 1250,
					own: 1000,
					others: vec![IndividualExposure { who: 101, value: 250 }]
				}
			);

			// staked values;
			let nominator_stake = Staking::ledger(101.into()).unwrap().active;
			let nominator_balance = balances(&101).0;
			let validator_stake = Staking::ledger(11.into()).unwrap().active;
			let validator_balance = balances(&11).0;
			let exposed_stake = initial_exposure.total;
			let exposed_validator = initial_exposure.own;
			let exposed_nominator = initial_exposure.others.first().unwrap().value;

			// register a slash for 11 with 10%.
			add_slash(11);
			assert_eq!(
				staking_events_since_last_call(),
				vec![Event::OffenceReported {
					offence_era: 1,
					validator: 11,
					fraction: Perbill::from_percent(10)
				}]
			);

			// roll one block until it is applied
			assert_eq!(SlashDeferDuration::get(), 0);
			Session::roll_next();
			assert_eq!(
				staking_events_since_last_call(),
				vec![
					Event::SlashComputed { offence_era: 1, slash_era: 1, offender: 11, page: 0 },
					Event::Slashed { staker: 11, amount: 100 },
					Event::Slashed { staker: 101, amount: 25 }
				]
			);

			// both stakes must have been decreased.
			assert!(Staking::ledger(101.into()).unwrap().active < nominator_stake);
			assert!(Staking::ledger(11.into()).unwrap().active < validator_stake);

			let slash_amount = Perbill::from_percent(10) * exposed_stake;
			let validator_share =
				Perbill::from_rational(exposed_validator, exposed_stake) * slash_amount;
			let nominator_share =
				Perbill::from_rational(exposed_nominator, exposed_stake) * slash_amount;

			// both slash amounts need to be positive for the test to make sense.
			assert!(validator_share > 0);
			assert!(nominator_share > 0);

			// both stakes must have been decreased pro-rata.
			assert_eq!(
				Staking::ledger(101.into()).unwrap().active,
				nominator_stake - nominator_share
			);
			assert_eq!(
				Staking::ledger(11.into()).unwrap().active,
				validator_stake - validator_share
			);
			assert_eq!(
				balances(&101).0, // free balance
				nominator_balance - nominator_share,
			);
			assert_eq!(
				balances(&11).0, // free balance
				validator_balance - validator_share,
			);
		});
}
