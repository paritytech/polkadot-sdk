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

//! Benchmarks for pallet-dap.

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Time;
use frame_system::RawOrigin;
use sp_staking::{BudgetKey, BudgetRecipientList};

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_budget_allocation() {
		// Build a valid allocation from registered recipients summing to 100%.
		let recipients = T::BudgetRecipients::recipients();
		let count = recipients.len() as u32;
		let mut allocations = BudgetAllocationMap::new();

		for (i, (key, _)) in recipients.into_iter().enumerate() {
			let perbill = if i as u32 == count - 1 {
				// Last recipient gets the remainder to ensure exact 100%.
				let used: u32 = allocations.values().map(|p| p.deconstruct()).sum();
				Perbill::from_parts(Perbill::one().deconstruct().saturating_sub(used))
			} else {
				Perbill::from_rational(1u32, count)
			};
			allocations.try_insert(key, perbill).expect("bounded by MAX_BUDGET_RECIPIENTS");
		}

		#[extrinsic_call]
		_(RawOrigin::Root, allocations.clone());

		assert_eq!(BudgetAllocation::<T>::get(), allocations);
	}

	#[benchmark]
	fn drip_issuance() {
		// Set up a valid budget allocation.
		let recipients = T::BudgetRecipients::recipients();
		let count = recipients.len() as u32;
		let mut allocations = BudgetAllocationMap::new();

		for (i, (key, _)) in recipients.iter().enumerate() {
			let perbill = if i as u32 == count - 1 {
				let used: u32 = allocations.values().map(|p| p.deconstruct()).sum();
				Perbill::from_parts(Perbill::one().deconstruct().saturating_sub(used))
			} else {
				Perbill::from_rational(1u32, count)
			};
			allocations.try_insert(key.clone(), perbill).expect("bounded");
		}
		BudgetAllocation::<T>::put(allocations);

		// Seed the timestamp so the drip fires.
		let now: u64 = T::Time::now().saturated_into();
		let past = now.saturating_sub(T::IssuanceCadence::get() + 1);
		LastIssuanceTimestamp::<T>::put(past);

		#[block]
		{
			Pallet::<T>::drip_issuance();
		}

		// Timestamp should be updated.
		assert!(LastIssuanceTimestamp::<T>::get() > past);
	}

}
