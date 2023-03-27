// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

/// Reward amount that is (hopefully) is larger than existential deposit across all chains.
const REWARD_AMOUNT: u32 = u32::MAX;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config>(crate::Pallet<T>);

/// Trait that must be implemented by runtime.
pub trait Config: crate::Config {
	/// Prepare environment for paying given reward for serving given lane.
	fn prepare_environment(account_params: RewardsAccountParams, reward: Self::Reward);
}

benchmarks! {
	// Benchmark `claim_rewards` call.
	claim_rewards {
		let lane = LaneId([0, 0, 0, 0]);
		let account_params =
			RewardsAccountParams::new(lane, *b"test", RewardsAccountOwner::ThisChain);
		let relayer: T::AccountId = whitelisted_caller();
		let reward = T::Reward::from(REWARD_AMOUNT);

		T::prepare_environment(account_params, reward);
		RelayerRewards::<T>::insert(&relayer, account_params, reward);
	}: _(RawOrigin::Signed(relayer), account_params)
	verify {
		// we can't check anything here, because `PaymentProcedure` is responsible for
		// payment logic, so we assume that if call has succeeded, the procedure has
		// also completed successfully
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime)
}
