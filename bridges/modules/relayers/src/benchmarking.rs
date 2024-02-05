// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Benchmarks for the relayers Pallet.

#![cfg(feature = "runtime-benchmarks")]

use crate::*;

use bp_messages::LaneId;
use bp_relayers::RewardsAccountOwner;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_system::RawOrigin;
use sp_runtime::traits::One;

/// Reward amount that is (hopefully) is larger than existential deposit across all chains.
const REWARD_AMOUNT: u32 = u32::MAX;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config>(crate::Pallet<T>);

/// Trait that must be implemented by runtime.
pub trait Config: crate::Config {
	/// Prepare environment for paying given reward for serving given lane.
	fn prepare_rewards_account(account_params: RewardsAccountParams, reward: Self::Reward);
	/// Give enough balance to given account.
	fn deposit_account(account: Self::AccountId, balance: Self::Reward);
}

benchmarks! {
	// Benchmark `claim_rewards` call.
	claim_rewards {
		let lane = LaneId([0, 0, 0, 0]);
		let account_params =
			RewardsAccountParams::new(lane, *b"test", RewardsAccountOwner::ThisChain);
		let relayer: T::AccountId = whitelisted_caller();
		let reward = T::Reward::from(REWARD_AMOUNT);

		T::prepare_rewards_account(account_params, reward);
		RelayerRewards::<T>::insert(&relayer, account_params, reward);
	}: _(RawOrigin::Signed(relayer), account_params)
	verify {
		// we can't check anything here, because `PaymentProcedure` is responsible for
		// payment logic, so we assume that if call has succeeded, the procedure has
		// also completed successfully
	}

	// Benchmark `register` call.
	register {
		let relayer: T::AccountId = whitelisted_caller();
		let valid_till = frame_system::Pallet::<T>::block_number()
			.saturating_add(crate::Pallet::<T>::required_registration_lease())
			.saturating_add(One::one())
			.saturating_add(One::one());

		T::deposit_account(relayer.clone(), crate::Pallet::<T>::required_stake());
	}: _(RawOrigin::Signed(relayer.clone()), valid_till)
	verify {
		assert!(crate::Pallet::<T>::is_registration_active(&relayer));
	}

	// Benchmark `deregister` call.
	deregister {
		let relayer: T::AccountId = whitelisted_caller();
		let valid_till = frame_system::Pallet::<T>::block_number()
			.saturating_add(crate::Pallet::<T>::required_registration_lease())
			.saturating_add(One::one())
			.saturating_add(One::one());
		T::deposit_account(relayer.clone(), crate::Pallet::<T>::required_stake());
		crate::Pallet::<T>::register(RawOrigin::Signed(relayer.clone()).into(), valid_till).unwrap();

		frame_system::Pallet::<T>::set_block_number(valid_till.saturating_add(One::one()));
	}: _(RawOrigin::Signed(relayer.clone()))
	verify {
		assert!(!crate::Pallet::<T>::is_registration_active(&relayer));
	}

	// Benchmark `slash_and_deregister` method of the pallet. We are adding this weight to
	// the weight of message delivery call if `RefundBridgedParachainMessages` signed extension
	// is deployed at runtime level.
	slash_and_deregister {
		// prepare and register relayer account
		let relayer: T::AccountId = whitelisted_caller();
		let valid_till = frame_system::Pallet::<T>::block_number()
			.saturating_add(crate::Pallet::<T>::required_registration_lease())
			.saturating_add(One::one())
			.saturating_add(One::one());
		T::deposit_account(relayer.clone(), crate::Pallet::<T>::required_stake());
		crate::Pallet::<T>::register(RawOrigin::Signed(relayer.clone()).into(), valid_till).unwrap();

		// create slash destination account
		let lane = LaneId([0, 0, 0, 0]);
		let slash_destination = RewardsAccountParams::new(lane, *b"test", RewardsAccountOwner::ThisChain);
		T::prepare_rewards_account(slash_destination, Zero::zero());
	}: {
		crate::Pallet::<T>::slash_and_deregister(&relayer, slash_destination)
	}
	verify {
		assert!(!crate::Pallet::<T>::is_registration_active(&relayer));
	}

	// Benchmark `register_relayer_reward` method of the pallet. We are adding this weight to
	// the weight of message delivery call if `RefundBridgedParachainMessages` signed extension
	// is deployed at runtime level.
	register_relayer_reward {
		let lane = LaneId([0, 0, 0, 0]);
		let relayer: T::AccountId = whitelisted_caller();
		let account_params =
			RewardsAccountParams::new(lane, *b"test", RewardsAccountOwner::ThisChain);
	}: {
		crate::Pallet::<T>::register_relayer_reward(account_params, &relayer, One::one());
	}
	verify {
		assert_eq!(RelayerRewards::<T>::get(relayer, &account_params), Some(One::one()));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime)
}
