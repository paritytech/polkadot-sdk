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

//! Tests for the module.

#![cfg(test)]

use super::pallet;
use crate::mock::{
	build_ext, build_ext_and_execute_test, Aura, MockDisabledValidators, System, Test, Timestamp,
};
use codec::Encode;
use frame_support::traits::OnInitialize;
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{Digest, DigestItem, TryRuntimeError};

#[test]
fn initial_values() {
	build_ext_and_execute_test(vec![0, 1, 2, 3], || {
		assert_eq!(pallet::CurrentSlot::<Test>::get(), 0u64);
		assert_eq!(pallet::Authorities::<Test>::get().len(), Aura::authorities_len());
		assert_eq!(Aura::authorities_len(), 4);
	});
}

#[test]
#[should_panic(
	expected = "Validator with index 1 is disabled and should not be attempting to author blocks."
)]
fn disabled_validators_cannot_author_blocks() {
	build_ext_and_execute_test(vec![0, 1, 2, 3], || {
		// slot 1 should be authored by validator at index 1
		let slot = Slot::from(1);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())] };

		System::reset_events();
		System::initialize(&1, &System::parent_hash(), &pre_digest);

		// let's disable the validator
		MockDisabledValidators::disable_validator(1);

		// and we should not be able to initialize the block
		Aura::on_initialize(1);
	});
}

#[test]
#[should_panic(expected = "Slot must increase")]
fn pallet_requires_slot_to_increase_unless_allowed() {
	build_ext_and_execute_test(vec![0, 1, 2, 3], || {
		crate::mock::AllowMultipleBlocksPerSlot::set(false);

		let slot = Slot::from(1);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())] };

		System::reset_events();
		System::initialize(&1, &System::parent_hash(), &pre_digest);

		// and we should not be able to initialize the block with the same slot a second time.
		Aura::on_initialize(1);
		Aura::on_initialize(1);
	});
}

#[test]
fn pallet_can_allow_unchanged_slot() {
	build_ext_and_execute_test(vec![0, 1, 2, 3], || {
		let slot = Slot::from(1);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())] };

		System::reset_events();
		System::initialize(&1, &System::parent_hash(), &pre_digest);

		crate::mock::AllowMultipleBlocksPerSlot::set(true);

		// and we should be able to initialize the block with the same slot a second time.
		Aura::on_initialize(1);
		Aura::on_initialize(1);
	});
}

#[test]
#[should_panic(expected = "Slot must not decrease")]
fn pallet_always_rejects_decreasing_slot() {
	build_ext_and_execute_test(vec![0, 1, 2, 3], || {
		let slot = Slot::from(2);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())] };

		System::reset_events();
		System::initialize(&1, &System::parent_hash(), &pre_digest);

		crate::mock::AllowMultipleBlocksPerSlot::set(true);

		Aura::on_initialize(1);
		System::finalize();

		let earlier_slot = Slot::from(1);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, earlier_slot.encode())] };
		System::initialize(&2, &System::parent_hash(), &pre_digest);
		Aura::on_initialize(2);
	});
}

#[test]
fn try_state_validates_timestamp_slot_consistency() {
	build_ext(vec![0, 1, 2, 3]).execute_with(|| {
		let slot = Slot::from(5);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())] };

		System::reset_events();
		System::initialize(&1, &System::parent_hash(), &pre_digest);

		Aura::on_initialize(1);

		// Slot duration is 2, so timestamp for slot 5 should be 10.
		// Setting it to 10 should pass try_state.
		Timestamp::set_timestamp(10);
		assert!(Aura::do_try_state().is_ok());

		// Setting timestamp to a value that doesn't match the slot should fail.
		// Timestamp 12 / slot_duration 2 = slot 6, but current slot is 5.
		pallet_timestamp::Now::<Test>::put(12u64);
		assert_eq!(
			Aura::do_try_state(),
			Err(TryRuntimeError::Other("Timestamp slot must match CurrentSlot."))
		);
	});
}
