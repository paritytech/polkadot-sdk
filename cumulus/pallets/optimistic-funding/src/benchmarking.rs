// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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

//! Benchmarking for the optimistic funding pallet.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as OptimisticFunding;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use sp_runtime::traits::Bounded;
use frame_support::{traits::Currency, bounded_vec};

const SEED: u32 = 0;

benchmarks! {
    submit_request {
        let caller: T::AccountId = whitelisted_caller();
        let amount = T::MinimumRequestAmount::get();
        let description = bounded_vec![0; 100];

        // Ensure the caller has enough balance for the deposit
        let deposit = T::RequestDeposit::get();
        T::Currency::make_free_balance_be(&caller, deposit * 2u32.into());

        // Set the period end
        let period_end = frame_system::Pallet::<T>::block_number() + T::FundingPeriod::get();
        OptimisticFunding::<T>::set_period_end(RawOrigin::Root.into(), period_end)?;
    }: _(RawOrigin::Signed(caller), amount, description)
    verify {
        assert_eq!(OptimisticFunding::<T>::active_request_count(), 1);
    }

    vote {
        let caller: T::AccountId = whitelisted_caller();
        let voter: T::AccountId = account("voter", 0, SEED);
        let amount = T::MinimumRequestAmount::get();
        let description = bounded_vec![0; 100];
        let vote_amount = amount / 2u32.into();

        // Ensure the caller has enough balance for the deposit
        let deposit = T::RequestDeposit::get();
        T::Currency::make_free_balance_be(&caller, deposit * 2u32.into());

        // Ensure the voter has enough balance to vote
        T::Currency::make_free_balance_be(&voter, vote_amount * 2u32.into());

        // Set the period end
        let period_end = frame_system::Pallet::<T>::block_number() + T::FundingPeriod::get();
        OptimisticFunding::<T>::set_period_end(RawOrigin::Root.into(), period_end)?;

        // Submit a request
        OptimisticFunding::<T>::submit_request(RawOrigin::Signed(caller.clone()).into(), amount, description.clone())?;

        // Get the request hash
        let request = FundingRequest {
            proposer: caller,
            amount,
            description,
            submitted_at: frame_system::Pallet::<T>::block_number(),
            period_end,
            votes: 0u32.into(),
        };
        let request_hash = T::Hashing::hash_of(&request);
    }: _(RawOrigin::Signed(voter.clone()), request_hash, vote_amount)
    verify {
        assert!(OptimisticFunding::<T>::votes(request_hash, voter).is_some());
    }

    cancel_vote {
        let caller: T::AccountId = whitelisted_caller();
        let voter: T::AccountId = account("voter", 0, SEED);
        let amount = T::MinimumRequestAmount::get();
        let description = bounded_vec![0; 100];
        let vote_amount = amount / 2u32.into();

        // Ensure the caller has enough balance for the deposit
        let deposit = T::RequestDeposit::get();
        T::Currency::make_free_balance_be(&caller, deposit * 2u32.into());

        // Ensure the voter has enough balance to vote
        T::Currency::make_free_balance_be(&voter, vote_amount * 2u32.into());

        // Set the period end
        let period_end = frame_system::Pallet::<T>::block_number() + T::FundingPeriod::get();
        OptimisticFunding::<T>::set_period_end(RawOrigin::Root.into(), period_end)?;

        // Submit a request
        OptimisticFunding::<T>::submit_request(RawOrigin::Signed(caller.clone()).into(), amount, description.clone())?;

        // Get the request hash
        let request = FundingRequest {
            proposer: caller,
            amount,
            description,
            submitted_at: frame_system::Pallet::<T>::block_number(),
            period_end,
            votes: 0u32.into(),
        };
        let request_hash = T::Hashing::hash_of(&request);

        // Vote for the request
        OptimisticFunding::<T>::vote(RawOrigin::Signed(voter.clone()).into(), request_hash, vote_amount)?;
    }: _(RawOrigin::Signed(voter.clone()), request_hash)
    verify {
        let vote = OptimisticFunding::<T>::votes(request_hash, voter).unwrap();
        assert_eq!(vote.status, VoteStatus::Cancelled);
    }

    top_up_treasury {
        let treasury_origin = T::TreasuryOrigin::try_successful_origin().map_err(|_| "Failed to create treasury origin")?;
        let amount = T::MinimumRequestAmount::get() * 10u32.into();
    }: _<T::RuntimeOrigin>(treasury_origin, amount)
    verify {
        assert_eq!(OptimisticFunding::<T>::treasury_balance(), amount);
    }

    reject_request {
        let caller: T::AccountId = whitelisted_caller();
        let amount = T::MinimumRequestAmount::get();
        let description = bounded_vec![0; 100];

        // Ensure the caller has enough balance for the deposit
        let deposit = T::RequestDeposit::get();
        T::Currency::make_free_balance_be(&caller, deposit * 2u32.into());

        // Set the period end
        let period_end = frame_system::Pallet::<T>::block_number() + T::FundingPeriod::get();
        OptimisticFunding::<T>::set_period_end(RawOrigin::Root.into(), period_end)?;

        // Submit a request
        OptimisticFunding::<T>::submit_request(RawOrigin::Signed(caller.clone()).into(), amount, description.clone())?;

        // Get the request hash
        let request = FundingRequest {
            proposer: caller,
            amount,
            description,
            submitted_at: frame_system::Pallet::<T>::block_number(),
            period_end,
            votes: 0u32.into(),
        };
        let request_hash = T::Hashing::hash_of(&request);

        let treasury_origin = T::TreasuryOrigin::try_successful_origin().map_err(|_| "Failed to create treasury origin")?;
    }: _<T::RuntimeOrigin>(treasury_origin, request_hash)
    verify {
        assert!(OptimisticFunding::<T>::funding_requests(request_hash).is_none());
        assert_eq!(OptimisticFunding::<T>::active_request_count(), 0);
    }

    allocate_funds {
        let caller: T::AccountId = whitelisted_caller();
        let amount = T::MinimumRequestAmount::get();
        let description = bounded_vec![0; 100];

        // Ensure the caller has enough balance for the deposit
        let deposit = T::RequestDeposit::get();
        T::Currency::make_free_balance_be(&caller, deposit * 2u32.into());

        // Set the period end
        let period_end = frame_system::Pallet::<T>::block_number() + T::FundingPeriod::get();
        OptimisticFunding::<T>::set_period_end(RawOrigin::Root.into(), period_end)?;

        // Submit a request
        OptimisticFunding::<T>::submit_request(RawOrigin::Signed(caller.clone()).into(), amount, description.clone())?;

        // Get the request hash
        let request = FundingRequest {
            proposer: caller,
            amount,
            description,
            submitted_at: frame_system::Pallet::<T>::block_number(),
            period_end,
            votes: 0u32.into(),
        };
        let request_hash = T::Hashing::hash_of(&request);

        // Top up the treasury
        let treasury_amount = amount * 2u32.into();
        let treasury_origin = T::TreasuryOrigin::try_successful_origin().map_err(|_| "Failed to create treasury origin")?;
        OptimisticFunding::<T>::top_up_treasury(treasury_origin.clone(), treasury_amount)?;
    }: _<T::RuntimeOrigin>(treasury_origin, request_hash)
    verify {
        assert!(OptimisticFunding::<T>::funding_requests(request_hash).is_none());
        assert_eq!(OptimisticFunding::<T>::active_request_count(), 0);
        assert_eq!(OptimisticFunding::<T>::treasury_balance(), treasury_amount - amount);
    }

    set_period_end {
        let treasury_origin = T::TreasuryOrigin::try_successful_origin().map_err(|_| "Failed to create treasury origin")?;
        let period_end = frame_system::Pallet::<T>::block_number() + T::FundingPeriod::get();
    }: _<T::RuntimeOrigin>(treasury_origin, period_end)
    verify {
        assert_eq!(OptimisticFunding::<T>::current_period_end(), period_end);
    }

    on_initialize_end_period {
        let n = 10;
        let caller: T::AccountId = whitelisted_caller();
        let amount = T::MinimumRequestAmount::get();
        let description = bounded_vec![0; 100];

        // Ensure the caller has enough balance for the deposit
        let deposit = T::RequestDeposit::get();
        T::Currency::make_free_balance_be(&caller, deposit * 2u32.into());

        // Set the period end
        let period_end = frame_system::Pallet::<T>::block_number() + 5u32.into();
        OptimisticFunding::<T>::set_period_end(RawOrigin::Root.into(), period_end)?;

        // Submit a request
        OptimisticFunding::<T>::submit_request(RawOrigin::Signed(caller.clone()).into(), amount, description.clone())?;

        // Top up the treasury
        let treasury_amount = amount * 2u32.into();
        let treasury_origin = T::TreasuryOrigin::try_successful_origin().map_err(|_| "Failed to create treasury origin")?;
        OptimisticFunding::<T>::top_up_treasury(treasury_origin.clone(), treasury_amount)?;

        // Set the block number to the period end
        frame_system::Pallet::<T>::set_block_number(period_end);
    }: {
        OptimisticFunding::<T>::on_initialize(period_end);
    }
    verify {
        assert!(OptimisticFunding::<T>::current_period_end() > period_end);
    }

    on_initialize_no_op {
        let n = 10;

        // Set the period end far in the future
        let period_end = frame_system::Pallet::<T>::block_number() + 100u32.into();
        let treasury_origin = T::TreasuryOrigin::try_successful_origin().map_err(|_| "Failed to create treasury origin")?;
        OptimisticFunding::<T>::set_period_end(treasury_origin, period_end)?;

        // Set the block number to before the period end
        let current_block = frame_system::Pallet::<T>::block_number() + 10u32.into();
        frame_system::Pallet::<T>::set_block_number(current_block);
    }: {
        OptimisticFunding::<T>::on_initialize(current_block);
    }
    verify {
        assert_eq!(OptimisticFunding::<T>::current_period_end(), period_end);
    }
}

impl_benchmark_test_suite!(
    OptimisticFunding,
    crate::mock::new_test_ext(),
    crate::mock::Test,
);
