// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
	mock::{self},
	pallet::pallet::{Invulnerables, MinimumValidatorCount, ValidatorCount},
	slashing,
	tests::{Staking, Test},
	ActiveEra, ActiveEraInfo, BalanceOf, CanceledSlashPayout, ClaimedRewards, CurrentEra,
	CurrentPlannedSession, EraRewardPoints, ErasRewardPoints, ErasStakersClipped,
	ErasStartSessionIndex, ErasTotalStake, ErasValidatorPrefs, ErasValidatorReward, ForceEra,
	Forcing, Nominations, Nominators, Perbill, SlashRewardFraction, SlashingSpans,
	ValidatorPrefs, Validators,
};
use sp_staking::{EraIndex, Exposure, IndividualExposure, Page, SessionIndex};

#[test]
fn get_validator_count_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let v: u32 = 12;
		ValidatorCount::<Test>::put(v);

		// when
		let result = Staking::validator_count();

		// then
		assert_eq!(result, v);
	});
}

#[test]
fn get_minimum_validator_count_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let v: u32 = 12;
		MinimumValidatorCount::<Test>::put(v);

		// when
		let result = Staking::minimum_validator_count();

		// then
		assert_eq!(result, v);
	});
}

#[test]
fn get_invulnerables_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let v: Vec<mock::AccountId> = vec![1, 2, 3];
		Invulnerables::<Test>::put(v.clone());

		// when
		let result = Staking::invulnerables();

		// then
		assert_eq!(result, v);
	});
}

#[test]
fn get_validators_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let account_id: mock::AccountId = 1;
		let validator_prefs = ValidatorPrefs::default();

		Validators::<Test>::insert(account_id, validator_prefs.clone());

		// when
		let result = Staking::validators(&account_id);

		// then
		assert_eq!(result, validator_prefs);
	});
}

#[test]
fn get_nominators_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let account_id: mock::AccountId = 1;
		let nominations: Nominations<Test> = Nominations {
			targets: Default::default(),
			submitted_in: Default::default(),
			suppressed: false,
		};

		Nominators::<Test>::insert(account_id, nominations.clone());

		// when
		let result = Staking::nominators(account_id);

		// then
		assert_eq!(result, Some(nominations));
	});
}

#[test]
fn get_current_era_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let era: EraIndex = 12;
		CurrentEra::<Test>::put(era);

		// when
		let result = Staking::current_era();

		// then
		assert_eq!(result, Some(era));
	});
}

#[test]
fn get_active_era_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let era = ActiveEraInfo { index: 2, start: None };
		ActiveEra::<Test>::put(era);

		// when
		let result: Option<ActiveEraInfo> = Staking::active_era();

		// then
		if let Some(era_info) = result {
			assert_eq!(era_info.index, 2);
			assert_eq!(era_info.start, None);
		} else {
			panic!("Expected Some(era_info), got None");
		};
	});
}

#[test]
fn get_eras_start_session_index_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let era: EraIndex = 12;
		let session_index: SessionIndex = 14;
		ErasStartSessionIndex::<Test>::insert(era, session_index);

		// when
		let result = Staking::eras_start_session_index(era);

		// then
		assert_eq!(result, Some(session_index));
	});
}

#[test]
fn get_eras_stakers_clipped_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let era: EraIndex = 12;
		let account_id: mock::AccountId = 1;
		let exposure: Exposure<mock::AccountId, BalanceOf<Test>> = Exposure {
			total: 1125,
			own: 1000,
			others: vec![IndividualExposure { who: 101, value: 125 }],
		};
		ErasStakersClipped::<Test>::insert(era, account_id, exposure.clone());

		// when
		let result = Staking::eras_stakers_clipped(era, &account_id);

		// then
		assert_eq!(result, exposure);
	});
}

#[test]
fn get_claimed_rewards_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let era: EraIndex = 12;
		let account_id: mock::AccountId = 1;
		let rewards = Vec::<Page>::new();
		ClaimedRewards::<Test>::insert(era, account_id, rewards.clone());

		// when
		let result = Staking::claimed_rewards(era, &account_id);

		// then
		assert_eq!(result, rewards);
	});
}

#[test]
fn get_eras_validator_prefs_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let era: EraIndex = 12;
		let account_id: mock::AccountId = 1;
		let validator_prefs = ValidatorPrefs::default();

		ErasValidatorPrefs::<Test>::insert(era, account_id, validator_prefs.clone());

		// when
		let result = Staking::eras_validator_prefs(era, &account_id);

		// then
		assert_eq!(result, validator_prefs);
	});
}

#[test]
fn get_eras_validator_reward_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let era: EraIndex = 12;
		let balance_of = BalanceOf::<Test>::default();

		ErasValidatorReward::<Test>::insert(era, balance_of);

		// when
		let result = Staking::eras_validator_reward(era);

		// then
		assert_eq!(result, Some(balance_of));
	});
}

#[test]
fn get_eras_reward_points_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let era: EraIndex = 12;
		let reward_points = EraRewardPoints::<mock::AccountId> {
			total: 1,
			individual: vec![(11, 1)].into_iter().collect(),
		};
		ErasRewardPoints::<Test>::insert(era, reward_points);

		// when
		let result = Staking::eras_reward_points(era);

		// then
		assert_eq!(result.total, 1);
	});
}

#[test]
fn get_eras_total_stake_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let era: EraIndex = 12;
		let balance_of = BalanceOf::<Test>::default();

		ErasTotalStake::<Test>::insert(era, balance_of);

		// when
		let result = Staking::eras_total_stake(era);

		// then
		assert_eq!(result, balance_of);
	});
}

#[test]
fn get_force_era_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let forcing = Forcing::NotForcing;
		ForceEra::<Test>::put(forcing);

		// when
		let result = Staking::force_era();

		// then
		assert_eq!(result, forcing);
	});
}

#[test]
fn get_slash_reward_fraction_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let perbill = Perbill::one();
		SlashRewardFraction::<Test>::put(perbill);

		// when
		let result = Staking::slash_reward_fraction();

		// then
		assert_eq!(result, perbill);
	});
}

#[test]
fn get_canceled_payout_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let balance_of = BalanceOf::<Test>::default();
		CanceledSlashPayout::<Test>::put(balance_of);

		// when
		let result = Staking::canceled_payout();

		// then
		assert_eq!(result, balance_of);
	});
}

#[test]
fn get_slashing_spans_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let account_id: mock::AccountId = 1;
		let spans = slashing::SlashingSpans::new(2);
		SlashingSpans::<Test>::insert(account_id, spans);

		// when
		let result: Option<slashing::SlashingSpans> = Staking::slashing_spans(&account_id);

		// then
		// simple check so as not to add extra macros to slashing::SlashingSpans struct
		assert!(result.is_some());
	});
}

#[test]
fn get_current_planned_session_returns_value_from_storage() {
	sp_io::TestExternalities::default().execute_with(|| {
		// given
		let session_index = SessionIndex::default();
		CurrentPlannedSession::<Test>::put(session_index);

		// when
		let result = Staking::current_planned_session();

		// then
		assert_eq!(result, session_index);
	});
}
