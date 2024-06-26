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

//! Salary pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as CoreFellowship;

use frame_benchmarking::v2::*;
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use sp_arithmetic::traits::Bounded;

const SEED: u32 = 0;

type BenchResult = Result<(), BenchmarkError>;

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	fn ensure_evidence<T: Config<I>, I: 'static>(who: &T::AccountId) -> BenchResult {
		let evidence = BoundedVec::try_from(vec![0; Evidence::<T, I>::bound()]).unwrap();
		let wish = Wish::Retention;
		let origin = RawOrigin::Signed(who.clone()).into();
		CoreFellowship::<T, I>::submit_evidence(origin, wish, evidence)?;
		assert!(MemberEvidence::<T, I>::contains_key(who));
		Ok(())
	}

	fn make_member<T: Config<I>, I: 'static>(rank: u16) -> Result<T::AccountId, BenchmarkError> {
		let member = account("member", 0, SEED);
		T::Members::induct(&member)?;
		for _ in 0..rank {
			T::Members::promote(&member)?;
		}
		CoreFellowship::<T, I>::import(RawOrigin::Signed(member.clone()).into())?;
		Ok(member)
	}

	fn set_benchmark_params<T: Config<I>, I: 'static>() -> Result<(), BenchmarkError> {
		let max_rank = T::MaxRank::get().try_into().unwrap();
		let params = ParamsType {
			active_salary: BoundedVec::try_from(vec![100u32.into(); max_rank]).unwrap(),
			passive_salary: BoundedVec::try_from(vec![10u32.into(); max_rank]).unwrap(),
			demotion_period: BoundedVec::try_from(vec![100u32.into(); max_rank]).unwrap(),
			min_promotion_period: BoundedVec::try_from(vec![100u32.into(); max_rank]).unwrap(),
			offboard_timeout: 1u32.into(),
		};

		CoreFellowship::<T, I>::set_params(RawOrigin::Root.into(), Box::new(params))?;
		Ok(())
	}

	#[benchmark]
	fn set_params() -> Result<(), BenchmarkError> {
		let max_rank = T::MaxRank::get().try_into().unwrap();
		let params = ParamsType {
			active_salary: BoundedVec::try_from(vec![100u32.into(); max_rank]).unwrap(),
			passive_salary: BoundedVec::try_from(vec![10u32.into(); max_rank]).unwrap(),
			demotion_period: BoundedVec::try_from(vec![100u32.into(); max_rank]).unwrap(),
			min_promotion_period: BoundedVec::try_from(vec![100u32.into(); max_rank]).unwrap(),
			offboard_timeout: 1u32.into(),
		};

		#[extrinsic_call]
		_(RawOrigin::Root, Box::new(params.clone()));

		assert_eq!(Params::<T, I>::get(), params);
		Ok(())
	}

	#[benchmark]
	fn set_partial_params() -> Result<(), BenchmarkError> {
		let max_rank = T::MaxRank::get().try_into().unwrap();

		// Set up the initial default state for the Params storage
		let params = ParamsType {
			active_salary: BoundedVec::try_from(vec![100u32.into(); max_rank]).unwrap(),
			passive_salary: BoundedVec::try_from(vec![10u32.into(); max_rank]).unwrap(),
			demotion_period: BoundedVec::try_from(vec![100u32.into(); max_rank]).unwrap(),
			min_promotion_period: BoundedVec::try_from(vec![100u32.into(); max_rank]).unwrap(),
			offboard_timeout: 1u32.into(),
		};
		CoreFellowship::<T, I>::set_params(RawOrigin::Root.into(), Box::new(params))?;

		let default_params = Params::<T, I>::get();
		let expected_params = ParamsType {
			active_salary: default_params.active_salary,
			passive_salary: BoundedVec::try_from(vec![10u32.into(); max_rank]).unwrap(),
			demotion_period: default_params.demotion_period,
			min_promotion_period: BoundedVec::try_from(vec![100u32.into(); max_rank]).unwrap(),
			offboard_timeout: 1u32.into(),
		};

		let params_payload = ParamsType {
			active_salary: BoundedVec::try_from(vec![None; max_rank]).unwrap(),
			passive_salary: BoundedVec::try_from(vec![Some(10u32.into()); max_rank]).unwrap(),
			demotion_period: BoundedVec::try_from(vec![None; max_rank]).unwrap(),
			min_promotion_period: BoundedVec::try_from(vec![Some(100u32.into()); max_rank])
				.unwrap(),
			offboard_timeout: None,
		};

		#[extrinsic_call]
		_(RawOrigin::Root, Box::new(params_payload.clone()));

		assert_eq!(Params::<T, I>::get(), expected_params);
		Ok(())
	}

	#[benchmark]
	fn bump_offboard() -> Result<(), BenchmarkError> {
		set_benchmark_params::<T, I>()?;

		let member = make_member::<T, I>(0)?;

		// Set it to the max value to ensure that any possible auto-demotion period has passed.
		frame_system::Pallet::<T>::set_block_number(BlockNumberFor::<T>::max_value());
		ensure_evidence::<T, I>(&member)?;
		assert!(Member::<T, I>::contains_key(&member));

		#[extrinsic_call]
		CoreFellowship::<T, I>::bump(RawOrigin::Signed(member.clone()), member.clone());

		assert!(!Member::<T, I>::contains_key(&member));
		assert!(!MemberEvidence::<T, I>::contains_key(&member));
		Ok(())
	}

	#[benchmark]
	fn bump_demote() -> Result<(), BenchmarkError> {
		set_benchmark_params::<T, I>()?;

		let member = make_member::<T, I>(2)?;

		// Set it to the max value to ensure that any possible auto-demotion period has passed.
		frame_system::Pallet::<T>::set_block_number(BlockNumberFor::<T>::max_value());
		ensure_evidence::<T, I>(&member)?;
		assert!(Member::<T, I>::contains_key(&member));
		assert_eq!(T::Members::rank_of(&member), Some(2));

		#[extrinsic_call]
		CoreFellowship::<T, I>::bump(RawOrigin::Signed(member.clone()), member.clone());

		assert!(Member::<T, I>::contains_key(&member));
		assert_eq!(T::Members::rank_of(&member), Some(1));
		assert!(!MemberEvidence::<T, I>::contains_key(&member));
		Ok(())
	}

	#[benchmark]
	fn set_active() -> Result<(), BenchmarkError> {
		let member = make_member::<T, I>(1)?;
		assert!(Member::<T, I>::get(&member).unwrap().is_active);

		#[extrinsic_call]
		_(RawOrigin::Signed(member.clone()), false);

		assert!(!Member::<T, I>::get(&member).unwrap().is_active);
		Ok(())
	}

	#[benchmark]
	fn induct() -> Result<(), BenchmarkError> {
		let candidate: T::AccountId = account("candidate", 0, SEED);

		#[extrinsic_call]
		_(RawOrigin::Root, candidate.clone());

		assert_eq!(T::Members::rank_of(&candidate), Some(0));
		assert!(Member::<T, I>::contains_key(&candidate));
		Ok(())
	}

	#[benchmark]
	fn promote() -> Result<(), BenchmarkError> {
		// Ensure that the `min_promotion_period` wont get in our way.
		let mut params = Params::<T, I>::get();
		let max_rank = T::MaxRank::get().try_into().unwrap();
		params.min_promotion_period = BoundedVec::try_from(vec![Zero::zero(); max_rank]).unwrap();
		Params::<T, I>::put(&params);

		let member = make_member::<T, I>(1)?;

		// Set it to the max value to ensure that any possible auto-demotion period has passed.
		frame_system::Pallet::<T>::set_block_number(BlockNumberFor::<T>::max_value());
		ensure_evidence::<T, I>(&member)?;

		#[extrinsic_call]
		_(RawOrigin::Root, member.clone(), 2u8.into());

		assert_eq!(T::Members::rank_of(&member), Some(2));
		assert!(!MemberEvidence::<T, I>::contains_key(&member));
		Ok(())
	}

	/// Benchmark the `promote_fast` extrinsic to promote someone up to `r`.
	#[benchmark]
	fn promote_fast(r: Linear<1, { T::MaxRank::get() as u32 }>) -> Result<(), BenchmarkError> {
		let r = r.try_into().expect("r is too large");
		let member = make_member::<T, I>(0)?;

		ensure_evidence::<T, I>(&member)?;

		#[extrinsic_call]
		_(RawOrigin::Root, member.clone(), r);

		assert_eq!(T::Members::rank_of(&member), Some(r));
		assert!(!MemberEvidence::<T, I>::contains_key(&member));
		Ok(())
	}

	#[benchmark]
	fn offboard() -> Result<(), BenchmarkError> {
		let member = make_member::<T, I>(0)?;
		T::Members::demote(&member)?;
		ensure_evidence::<T, I>(&member)?;

		assert!(T::Members::rank_of(&member).is_none());
		assert!(Member::<T, I>::contains_key(&member));
		assert!(MemberEvidence::<T, I>::contains_key(&member));

		#[extrinsic_call]
		_(RawOrigin::Signed(member.clone()), member.clone());

		assert!(!Member::<T, I>::contains_key(&member));
		assert!(!MemberEvidence::<T, I>::contains_key(&member));
		Ok(())
	}

	#[benchmark]
	fn import() -> Result<(), BenchmarkError> {
		let member = account("member", 0, SEED);
		T::Members::induct(&member)?;
		T::Members::promote(&member)?;

		assert!(!Member::<T, I>::contains_key(&member));

		#[extrinsic_call]
		_(RawOrigin::Signed(member.clone()));

		assert!(Member::<T, I>::contains_key(&member));
		Ok(())
	}

	#[benchmark]
	fn approve() -> Result<(), BenchmarkError> {
		let member = make_member::<T, I>(1)?;
		let then = frame_system::Pallet::<T>::block_number();
		let now = then.saturating_plus_one();
		frame_system::Pallet::<T>::set_block_number(now);
		ensure_evidence::<T, I>(&member)?;

		assert_eq!(Member::<T, I>::get(&member).unwrap().last_proof, then);

		#[extrinsic_call]
		_(RawOrigin::Root, member.clone(), 1u8.into());

		assert_eq!(Member::<T, I>::get(&member).unwrap().last_proof, now);
		assert!(!MemberEvidence::<T, I>::contains_key(&member));
		Ok(())
	}

	#[benchmark]
	fn submit_evidence() -> Result<(), BenchmarkError> {
		let member = make_member::<T, I>(1)?;
		let evidence = vec![0; Evidence::<T, I>::bound()].try_into().unwrap();

		assert!(!MemberEvidence::<T, I>::contains_key(&member));

		#[extrinsic_call]
		_(RawOrigin::Signed(member.clone()), Wish::Retention, evidence);

		assert!(MemberEvidence::<T, I>::contains_key(&member));
		Ok(())
	}

	impl_benchmark_test_suite! {
		CoreFellowship,
		crate::tests::unit::new_test_ext(),
		crate::tests::unit::Test,
	}
}
