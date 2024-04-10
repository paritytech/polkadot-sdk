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

use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_system::RawOrigin;

/// Reward amount that is (hopefully) is larger than existential deposit across all chains.
const REWARD_AMOUNT: u32 = u32::MAX;

benchmarks! {
	// Benchmark `claim_rewards` call.
	claim_rewards {
		let lane = [0, 0, 0, 0];
		let relayer: T::AccountId = whitelisted_caller();
		RelayerRewards::<T>::insert(&relayer, lane, T::Reward::from(REWARD_AMOUNT));
	}: _(RawOrigin::Signed(relayer), lane)
	verify {
		// we can't check anything here, because `PaymentProcedure` is responsible for
		// payment logic, so we assume that if call has succeeded, the procedure has
		// also completed successfully
	}
}
