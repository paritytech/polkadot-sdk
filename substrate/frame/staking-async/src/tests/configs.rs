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

use super::*;

#[test]
fn set_staking_configs_works() {
	ExtBuilder::default().build_and_execute(|| {
		// setting works
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Set(1_500),
			ConfigOp::Set(2_000),
			ConfigOp::Set(10),
			ConfigOp::Set(20),
			ConfigOp::Set(Percent::from_percent(75)),
			ConfigOp::Set(Zero::zero()),
			ConfigOp::Set(Zero::zero())
		));
		assert_eq!(MinNominatorBond::<Test>::get(), 1_500);
		assert_eq!(MinValidatorBond::<Test>::get(), 2_000);
		assert_eq!(MaxNominatorsCount::<Test>::get(), Some(10));
		assert_eq!(MaxValidatorsCount::<Test>::get(), Some(20));
		assert_eq!(ChillThreshold::<Test>::get(), Some(Percent::from_percent(75)));
		assert_eq!(MinCommission::<Test>::get(), Perbill::from_percent(0));
		assert_eq!(MaxStakedRewards::<Test>::get(), Some(Percent::from_percent(0)));

		// noop does nothing
		assert_storage_noop!(assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop
		)));

		// removing works
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove
		));
		assert_eq!(MinNominatorBond::<Test>::get(), 0);
		assert_eq!(MinValidatorBond::<Test>::get(), 0);
		assert_eq!(MaxNominatorsCount::<Test>::get(), None);
		assert_eq!(MaxValidatorsCount::<Test>::get(), None);
		assert_eq!(ChillThreshold::<Test>::get(), None);
		assert_eq!(MinCommission::<Test>::get(), Perbill::from_percent(0));
		assert_eq!(MaxStakedRewards::<Test>::get(), None);
	});
}
