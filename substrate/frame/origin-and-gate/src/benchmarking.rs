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
use crate::{CompositeOriginId, Pallet as OriginAndGate};
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

	// Helper function to create T::OriginId from CompositeOriginId
	pub fn make_origin_id<T: Config>(id: CompositeOriginId) -> T::OriginId {
		// For benchmarking use only create simple OriginId using a hard-coded value
		let mut v = Vec::new();
		v.extend_from_slice(&id.collective_id.to_le_bytes());
		v.extend_from_slice(&id.role.to_le_bytes());

		// Codec crate used to create a T::OriginId from bytes
		// should work with most OriginId types that implement FullCodec
		match Decode::decode(&mut &v[..]) {
			Ok(proposal_origin_id) => proposal_origin_id,
			Err(_) => {
				// For benchmarking purposes only that are compiled separately use a different
				// approach if decoding fails of creating a value using unsafe methods
				let origin_bytes = [0u8; 32]; // Use a zero-filled buffer
				Decode::decode(&mut &origin_bytes[..]).unwrap_or_else(|_| {
					// Panic with a clear message otherwise
					panic!("Unable to create a valid OriginId for benchmarking")
				})
			},
		}
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

		// Convert CompositeOriginId to T::OriginId using our helper
		let proposal_origin_id = make_origin_id::<T>(CompositeOriginId::from(0));
		let proposal_expiry_at = None;

		// Phase 2: Execution
		#[extrinsic_call]
		propose(
			RawOrigin::Signed(caller),
			Box::new(call),
			proposal_origin_id,
			proposal_expiry_at,
			Some(true),
			None,
			None,
			None,
			Some(false),
		);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::proposals(call_hash, proposal_origin_id).is_some(),
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

		// Convert CompositeOriginId to T::OriginId using helper
		let proposal_origin_id = make_origin_id::<T>(CompositeOriginId::from(0));
		let approving_origin_id = make_origin_id::<T>(CompositeOriginId::from(1));
		let proposal_expiry_at = None;

		Pallet::<T>::propose(
			RawOrigin::Signed(proposer).into(),
			Box::new(call),
			proposal_origin_id,
			proposal_expiry_at,
			None,
			None,
			None,
			None,
			Some(false),
		)?;

		// Phase 2: Execution
		#[extrinsic_call]
		add_approval(
			RawOrigin::Signed(caller),
			call_hash,
			proposal_origin_id,
			approving_origin_id,
			None,
			None,
			None,
		);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::approvals((call_hash, proposal_origin_id), approving_origin_id).is_some(),
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

		// Convert CompositeOriginId to T::OriginId using our helper
		let proposal_origin_id = make_origin_id::<T>(CompositeOriginId::from(1));
		let approving_origin_id1 = make_origin_id::<T>(CompositeOriginId::from(2));
		let proposal_expiry_at = None;

		Pallet::<T>::propose(
			RawOrigin::Signed(proposer).into(),
			Box::new(call),
			proposal_origin_id,
			proposal_expiry_at,
			Some(true),
			None,
			None,
			None,
			Some(false), // Do not auto-execute
		)?;

		// Add approvals but do not auto-execute
		Pallet::<T>::add_approval(
			RawOrigin::Signed(approver1).into(),
			call_hash,
			proposal_origin_id,
			approving_origin_id1,
			None,
			None,
			None,
		)?;

		// Phase 2: Execution
		#[extrinsic_call]
		execute_proposal(RawOrigin::Signed(caller), call_hash, proposal_origin_id);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::proposals(call_hash, proposal_origin_id).is_some(),
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

		// Convert CompositeOriginId to T::OriginId using our helper
		let proposal_origin_id = make_origin_id::<T>(CompositeOriginId::from(0));
		let proposal_expiry_at = None;

		Pallet::<T>::propose(
			RawOrigin::Signed(caller.clone()).into(),
			Box::new(call),
			proposal_origin_id,
			proposal_expiry_at,
			None,
			None,
			None,
			None,
			Some(false),
		)?;

		// Phase 2: Execution
		#[extrinsic_call]
		cancel_proposal(RawOrigin::Signed(caller), call_hash, proposal_origin_id);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::proposals(call_hash, proposal_origin_id).is_none(),
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

		// Convert CompositeOriginId to T::OriginId using our helper
		let proposal_origin_id = make_origin_id::<T>(CompositeOriginId::from(0));
		let approving_origin_id = make_origin_id::<T>(CompositeOriginId::from(1));
		let proposal_expiry_at = None;

		Pallet::<T>::propose(
			RawOrigin::Signed(proposer).into(),
			Box::new(call),
			proposal_origin_id,
			proposal_expiry_at,
			None,
			None,
			None,
			None,
			Some(false),
		)?;

		Pallet::<T>::add_approval(
			RawOrigin::Signed(approver.clone()).into(),
			call_hash,
			proposal_origin_id,
			approving_origin_id,
			None,
			None,
			None,
		)?;

		// Phase 2: Execution
		#[extrinsic_call]
		withdraw_approval(
			RawOrigin::Signed(approver),
			call_hash,
			proposal_origin_id,
			approving_origin_id,
		);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::approvals((call_hash, proposal_origin_id), approving_origin_id).is_none(),
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

		// Convert CompositeOriginId to T::OriginId using our helper
		let proposal_origin_id = make_origin_id::<T>(CompositeOriginId::from(1));
		let approving_origin_id = make_origin_id::<T>(CompositeOriginId::from(2));
		let proposal_expiry_at = None;
		let auto_execute = Some(false);

		// Create a proposal with immediate expiry
		let start_block = frame_system::Pallet::<T>::block_number();
		// let expiry_at = Some(start_block);

		// Ensure the proposal is created
		Pallet::<T>::propose(
			RawOrigin::Signed(proposer.clone()).into(),
			Box::new(call.clone()),
			proposal_origin_id.clone(),
			proposal_expiry_at,
			Some(true),
			None,
			None,
			None,
			auto_execute,
		)?;

		// Verify the proposal still exists before add_approval
		assert!(
			Pallet::<T>::proposals(call_hash, proposal_origin_id).is_some(),
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
			proposal_origin_id.clone(),
			approving_origin_id.clone(),
			None,
			None,
			None,
		)?;

		// Execute proposal
		Pallet::<T>::execute_proposal(
			RawOrigin::Signed(proposer).into(),
			call_hash,
			proposal_origin_id,
		)?;

		// Advance block to make proposal eligible for cleaning
		let proposal_retention_period = T::ProposalRetentionPeriodWhenNotCancelled::get();
		let proposal_expiry_at = T::ProposalExpiry::get();
		frame_system::Pallet::<T>::set_block_number(
			start_block + proposal_expiry_at + proposal_retention_period + 1u32.into(),
		);

		// Verify proposal still exists before cleaning
		assert!(
			Pallet::<T>::proposals(call_hash, proposal_origin_id).is_some(),
			"Proposal must exist before cleaning"
		);

		// Phase 2: Execution
		#[extrinsic_call]
		clean(RawOrigin::Signed(caller), call_hash, proposal_origin_id);

		// Phase 3: Verification
		assert!(
			Pallet::<T>::proposals(call_hash, proposal_origin_id).is_none(),
			"Proposal must not exist after cleaning"
		);

		Ok(())
	}

	#[benchmark]
	fn amend_remark() -> Result<(), BenchmarkError> {
		// Phase 1: Setup - Create a proposal and add initial approval
		let caller: T::AccountId = whitelisted_caller();
		let proposer: T::AccountId = account("proposer", 0, 0);
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();

		// Get hash output and convert it to T::Hash with proper type casting
		let hash_output = <T as pallet::Config>::Hashing::hash_of(&call);
		let call_hash = convert_hash::<T>(&hash_output);

		// Convert CompositeOriginId to T::OriginId using our helper
		let proposal_origin_id = make_origin_id::<T>(CompositeOriginId::from(1));
		let approving_origin_id = make_origin_id::<T>(CompositeOriginId::from(2));

		// Store the call in storage for execution
		ProposalCalls::<T>::insert(call_hash, Box::new(call));

		// Create expiry block number
		let proposal_expiry_at = Some(frame_system::Pallet::<T>::block_number() + 100u32.into());

		// Create proposal
		Pallet::<T>::propose(
			RawOrigin::Signed(proposer.clone()).into(),
			Box::new(frame_system::Call::<T>::remark { remark: vec![] }.into()),
			proposal_origin_id.clone(),
			proposal_expiry_at,
			Some(true),
			None,
			None,
			None,
			Some(false),
		)?;

		// Add initial approval with no remark
		Pallet::<T>::add_approval(
			RawOrigin::Signed(caller.clone()).into(),
			call_hash,
			proposal_origin_id.clone(),
			approving_origin_id.clone(),
			None,
			None,
			None,
		)?;

		// Verify approval exists
		assert!(
			Pallet::<T>::approvals(
				(call_hash, proposal_origin_id.clone()),
				approving_origin_id.clone()
			)
			.is_some(),
			"Approval must exist before amending"
		);

		// Create conditional approval remark for amendment
		let remark: Vec<u8> = vec![1, 2, 3, 4];

		// Phase 2: Execution - Amend the approval with a new remark
		#[extrinsic_call]
		amend_remark(
			RawOrigin::Signed(caller),
			call_hash,
			proposal_origin_id,
			Some(approving_origin_id),
			remark,
			None,
			None,
		);

		// Phase 3: Verification
		// The verification is implicit as the extrinsic would fail if the approval didn't exist
		// or if the caller wasn't authorized to amend it

		Ok(())
	}

	impl_benchmark_test_suite!(OriginAndGate, crate::mock::new_test_ext(), crate::mock::Test);
}
