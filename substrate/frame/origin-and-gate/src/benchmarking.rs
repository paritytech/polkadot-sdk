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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as OriginAndGate;
use frame_benchmarking::{v2::*, BenchmarkError};
use frame_support::traits::Get;
use frame_system::RawOrigin;
use sp_runtime::traits::{Bounded, DispatchTransaction, Hash};

// Import mock directly instead of through module import
#[path = "./mock.rs"]
pub mod mock;
pub use mock::{
	new_test_ext, Test, ALICE_ORIGIN_ID, BOB_ORIGIN_ID, CHARLIE_ORIGIN_ID, ROOT_ORIGIN_ID,
};

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

// Helper functions for benchmarking
mod helpers {
	use super::*;

	// Helper function to convert u8 to T::OriginId
	pub fn make_origin_id<T: Config>(id: u8) -> T::OriginId {
		let mut buf = [0u8; 128]; // Buffer large enough for any reasonable type
		buf[0] = id;
		T::OriginId::decode(&mut &buf[..]).unwrap_or_else(|_| panic!("Failed to decode OriginId"))
	}

	// Helper function to convert hash types
	pub fn convert_hash<T: Config>(
		hash: &<<T as pallet::Config>::Hashing as sp_runtime::traits::Hash>::Output,
	) -> T::Hash {
		let encoded = hash.encode();
		T::Hash::decode(&mut &encoded[..]).unwrap_or_else(|_| panic!("Failed to decode hash"))
	}
}

#[benchmarks]
mod benchmarks {
	use super::{helpers::*, *};

	// This will measure the execution time of `set_dummy`.
	#[benchmark]
	fn set_dummy() -> Result<(), BenchmarkError> {
		// Phase 1: Setup
		// `set_dummy` is a constant time function, hence we hard-code some test message here.
		let raw_value: Vec<u8> = b"message".to_vec();
		let value: DummyValueOf = BoundedVec::try_from(raw_value)
			.map_err(|_| BenchmarkError::Stop("Failed to create BoundedVec"))?;

		// Phase 2: Execution
		#[extrinsic_call]
		set_dummy(RawOrigin::Root, value.clone());

		// Phase 3: Verification
		assert_eq!(Dummy::<T>::get(), Some(value));

		Ok(())
	}

	#[benchmark]
	fn propose() -> Result<(), BenchmarkError> {
		// Phase 1: Setup
		let caller: T::AccountId = whitelisted_caller();
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		// Get hash output and convert it to T::Hash with proper type casting
		let hash_output = <T as pallet::Config>::Hashing::hash_of(&call);
		let call_hash = convert_hash::<T>(&hash_output);

		// Convert u8 to T::OriginId using our helper
		let origin_id = make_origin_id::<T>(ROOT_ORIGIN_ID);
		let expiry_at = None;

		// Phase 2: Execution
		#[extrinsic_call]
		propose(RawOrigin::Signed(caller), Box::new(call), origin_id, expiry_at, None, false);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::proposals(call_hash, origin_id).is_some(),
			"Proposal must exist after propose"
		);

		Ok(())
	}

	#[benchmark]
	fn add_approval() -> Result<(), BenchmarkError> {
		// Phase 1: Setup
		let caller: T::AccountId = whitelisted_caller();
		let proposer: T::AccountId = account("proposer", 0, 0);
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		// Get hash output and convert it to T::Hash with proper type casting
		let hash_output = <T as pallet::Config>::Hashing::hash_of(&call);
		let call_hash = convert_hash::<T>(&hash_output);

		// Convert u8 to T::OriginId using helper
		let origin_id = make_origin_id::<T>(ROOT_ORIGIN_ID);
		let approving_origin_id = make_origin_id::<T>(ALICE_ORIGIN_ID);
		let expiry_at = None;

		Pallet::<T>::propose(
			RawOrigin::Signed(proposer).into(),
			Box::new(call),
			origin_id,
			expiry_at,
			None,
			false,
		)?;

		// Phase 2: Execution
		#[extrinsic_call]
		add_approval(RawOrigin::Signed(caller), call_hash, origin_id, approving_origin_id, None);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::approvals((call_hash, origin_id), approving_origin_id).is_some(),
			"Approval must exist after add_approval"
		);

		Ok(())
	}

	#[benchmark]
	fn execute_proposal() -> Result<(), BenchmarkError> {
		// Phase 1: Setup
		let caller: T::AccountId = whitelisted_caller();
		let proposer: T::AccountId = account("proposer", 0, 0);
		let approver1: T::AccountId = account("approver1", 0, 0);
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		// Get hash output and convert it to T::Hash with proper type casting
		let hash_output = <T as pallet::Config>::Hashing::hash_of(&call);
		let call_hash = convert_hash::<T>(&hash_output);

		// Convert u8 to T::OriginId using our helper
		let origin_id = make_origin_id::<T>(ALICE_ORIGIN_ID);
		let approving_origin_id1 = make_origin_id::<T>(BOB_ORIGIN_ID);
		let expiry_at = None;

		Pallet::<T>::propose(
			RawOrigin::Signed(proposer).into(),
			Box::new(call),
			origin_id,
			expiry_at,
			None,
			false, // Do not auto-execute
		)?;

		// Add approvals but do not auto-execute
		Pallet::<T>::add_approval(
			RawOrigin::Signed(approver1).into(),
			call_hash,
			origin_id,
			approving_origin_id1,
			None,
		)?;

		// Phase 2: Execution
		#[extrinsic_call]
		execute_proposal(RawOrigin::Signed(caller), call_hash, origin_id);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::proposals(call_hash, origin_id).is_some(),
			"Proposal must still exist after execute_proposal"
		);

		Ok(())
	}

	#[benchmark]
	fn cancel_proposal() -> Result<(), BenchmarkError> {
		// Phase 1: Setup
		let caller: T::AccountId = whitelisted_caller();
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		// Get hash output and convert it to T::Hash with proper type casting
		let hash_output = <T as pallet::Config>::Hashing::hash_of(&call);
		let call_hash = convert_hash::<T>(&hash_output);

		// Convert u8 to T::OriginId using our helper
		let origin_id = make_origin_id::<T>(ROOT_ORIGIN_ID);
		let expiry_at = None;

		Pallet::<T>::propose(
			RawOrigin::Signed(caller.clone()).into(),
			Box::new(call),
			origin_id,
			expiry_at,
			None,
			false,
		)?;

		// Phase 2: Execution
		#[extrinsic_call]
		cancel_proposal(RawOrigin::Signed(caller), call_hash, origin_id);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::proposals(call_hash, origin_id).is_none(),
			"Proposal must not exist after cancel_proposal"
		);

		Ok(())
	}

	#[benchmark]
	fn withdraw_approval() -> Result<(), BenchmarkError> {
		// Phase 1: Setup
		let proposer: T::AccountId = account("proposer", 0, 0);
		let approver: T::AccountId = whitelisted_caller();
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		// Get hash output and convert it to T::Hash with proper type casting
		let hash_output = <T as pallet::Config>::Hashing::hash_of(&call);
		let call_hash = convert_hash::<T>(&hash_output);

		// Convert u8 to T::OriginId using our helper
		let origin_id = make_origin_id::<T>(ROOT_ORIGIN_ID);
		let approving_origin_id = make_origin_id::<T>(ALICE_ORIGIN_ID);
		let expiry_at = None;

		Pallet::<T>::propose(
			RawOrigin::Signed(proposer).into(),
			Box::new(call),
			origin_id,
			expiry_at,
			None,
			false,
		)?;

		Pallet::<T>::add_approval(
			RawOrigin::Signed(approver.clone()).into(),
			call_hash,
			origin_id,
			approving_origin_id,
			None,
		)?;

		// Phase 2: Execution
		#[extrinsic_call]
		withdraw_approval(RawOrigin::Signed(approver), call_hash, origin_id, approving_origin_id);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::approvals((call_hash, origin_id), approving_origin_id).is_none(),
			"Approval must not exist after withdraw_approval"
		);

		Ok(())
	}

	#[benchmark]
	fn clean() -> Result<(), BenchmarkError> {
		// Phase 1: Setup
		let caller: T::AccountId = whitelisted_caller();
		let proposer: T::AccountId = account("proposer", 0, 0);
		let approver: T::AccountId = whitelisted_caller();
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();

		// Get hash output and convert it to T::Hash with proper type casting
		let hash_output = <T as pallet::Config>::Hashing::hash_of(&call);
		let call_hash = convert_hash::<T>(&hash_output);

		// Convert u8 to T::OriginId using our helper
		let origin_id = make_origin_id::<T>(ALICE_ORIGIN_ID);
		let approving_origin_id = make_origin_id::<T>(BOB_ORIGIN_ID);
		let expiry_at = None;
		let auto_execute = false;

		// Create a proposal with immediate expiry
		let start_block = frame_system::Pallet::<T>::block_number();
		// let expiry_at = Some(start_block);

		// Ensure the proposal is created
		Pallet::<T>::propose(
			RawOrigin::Signed(proposer.clone()).into(),
			Box::new(call.clone()),
			origin_id,
			expiry_at,
			None,
			auto_execute,
		)?;

		// Verify the proposal still exists before add_approval
		assert!(
			Pallet::<T>::proposals(call_hash, origin_id).is_some(),
			"Proposal must exist before add_approval"
		);

		let required_approvals_count = T::RequiredApprovalsCount::get();
		assert!(
			required_approvals_count == 2,
			"Benchmarking assumes required_approvals_count == 2"
		);

		Pallet::<T>::add_approval(
			RawOrigin::Signed(approver.clone()).into(),
			call_hash,
			origin_id,
			approving_origin_id,
			None,
		)?;

		// Execute proposal
		Pallet::<T>::execute_proposal(RawOrigin::Signed(proposer).into(), call_hash, origin_id)?;

		// Advance block to make proposal eligible for cleaning
		let retention_period = T::NonCancelledProposalRetentionPeriod::get();
		let proposal_expiry_at = T::ProposalExpiry::get();
		frame_system::Pallet::<T>::set_block_number(
			start_block + proposal_expiry_at + retention_period + 1u32.into(),
		);

		// Verify proposal still exists before cleaning
		assert!(
			Pallet::<T>::proposals(call_hash, origin_id).is_some(),
			"Proposal must exist before cleaning"
		);

		// Phase 2: Execution
		#[extrinsic_call]
		clean(RawOrigin::Signed(caller), call_hash, origin_id);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::proposals(call_hash, origin_id).is_none(),
			"Proposal must not exist after cleaning"
		);

		Ok(())
	}

	#[benchmark]
	fn amend_approval() -> Result<(), BenchmarkError> {
		// Phase 1: Setup - Create a proposal and add initial approval
		let caller: T::AccountId = whitelisted_caller();
		let proposer: T::AccountId = account("proposer", 0, 0);
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();

		// Get hash output and convert it to T::Hash with proper type casting
		let hash_output = <T as pallet::Config>::Hashing::hash_of(&call);
		let call_hash = convert_hash::<T>(&hash_output);

		// Convert u8 to T::OriginId using our helper
		let origin_id = make_origin_id::<T>(ALICE_ORIGIN_ID);
		let approving_origin_id = make_origin_id::<T>(BOB_ORIGIN_ID);

		// Store the call in storage for execution
		ProposalCalls::<T>::insert(call_hash, Box::new(call));

		// Create expiry block number
		let expiry_at = Some(frame_system::Pallet::<T>::block_number() + 100u32.into());

		// Create proposal
		Pallet::<T>::propose(
			RawOrigin::Signed(proposer.clone()).into(),
			Box::new(frame_system::Call::<T>::remark { remark: vec![] }.into()),
			origin_id.clone(),
			expiry_at,
			None,
			false,
		)?;

		// Add initial approval with no remark
		Pallet::<T>::add_approval(
			RawOrigin::Signed(caller.clone()).into(),
			call_hash,
			origin_id.clone(),
			approving_origin_id.clone(),
			None,
		)?;

		// Verify approval exists
		assert!(
			Pallet::<T>::approvals((call_hash, origin_id.clone()), approving_origin_id.clone())
				.is_some(),
			"Approval must exist before amending"
		);

		// Create conditional approval remark for amendment
		let remark: Vec<u8> = vec![1, 2, 3, 4];

		// Phase 2: Execution - Amend the approval with a new remark
		#[extrinsic_call]
		amend_approval(
			RawOrigin::Signed(caller),
			call_hash,
			origin_id,
			approving_origin_id,
			remark,
		);

		// Phase 3: Verification
		// The verification is implicit as the extrinsic would fail if the approval didn't exist
		// or if the caller wasn't authorized to amend it

		Ok(())
	}

	impl_benchmark_test_suite!(OriginAndGate, crate::mock::new_test_ext(), crate::mock::Test);
}
