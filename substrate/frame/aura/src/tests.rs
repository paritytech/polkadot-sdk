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

use crate::{
	mock::{
		generate_equivocation_proof, new_test_ext_and_execute, progress_to_block, Aura,
		MockDisabledValidators, RuntimeOrigin, System, Test,
	},
	CurrentSlot,
};
use codec::Encode;
use frame_support::traits::OnInitialize;
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_core::crypto::Pair;
use sp_runtime::{Digest, DigestItem};

#[test]
fn initial_values() {
	new_test_ext_and_execute(4, |_| {
		assert_eq!(Aura::current_slot(), 0u64);
		assert_eq!(Aura::authorities().len(), 4);
	});
}

#[test]
#[should_panic(
	expected = "Validator with index 1 is disabled and should not be attempting to author blocks."
)]
fn disabled_validators_cannot_author_blocks() {
	new_test_ext_and_execute(4, |_| {
		// slot 1 should be authored by validator at index 1
		let slot = Slot::from(1);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())] };

		System::reset_events();
		System::initialize(&42, &System::parent_hash(), &pre_digest);

		// let's disable the validator
		MockDisabledValidators::disable_validator(1);

		// and we should not be able to initialize the block
		Aura::on_initialize(42);
	});
}

#[test]
#[should_panic(expected = "Slot must increase")]
fn pallet_requires_slot_to_increase_unless_allowed() {
	new_test_ext_and_execute(4, |_| {
		crate::mock::AllowMultipleBlocksPerSlot::set(false);

		let slot = Slot::from(1);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())] };

		System::reset_events();
		System::initialize(&42, &System::parent_hash(), &pre_digest);

		// and we should not be able to initialize the block with the same slot a second time.
		Aura::on_initialize(42);
		Aura::on_initialize(42);
	});
}

#[test]
fn pallet_can_allow_unchanged_slot() {
	new_test_ext_and_execute(4, |_| {
		let slot = Slot::from(1);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())] };

		System::reset_events();
		System::initialize(&42, &System::parent_hash(), &pre_digest);

		crate::mock::AllowMultipleBlocksPerSlot::set(true);

		// and we should be able to initialize the block with the same slot a second time.
		Aura::on_initialize(42);
		Aura::on_initialize(42);
	});
}

#[test]
#[should_panic(expected = "Slot must not decrease")]
fn pallet_always_rejects_decreasing_slot() {
	new_test_ext_and_execute(4, |_| {
		let slot = Slot::from(2);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())] };

		System::reset_events();
		System::initialize(&42, &System::parent_hash(), &pre_digest);

		crate::mock::AllowMultipleBlocksPerSlot::set(true);

		Aura::on_initialize(42);
		System::finalize();

		let earlier_slot = Slot::from(1);
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, earlier_slot.encode())] };
		System::initialize(&43, &System::parent_hash(), &pre_digest);
		Aura::on_initialize(43);
	});
}

#[test]
fn report_equivocation_works() {
	env_logger::init();
	new_test_ext_and_execute(4, |pairs| {
		progress_to_block(1);
		// start_era(1);

		let authorities = Aura::authorities();
		// let validators = Session::validators();

		// make sure that all authorities have the same balance
		// for validator in &validators {
		// 	assert_eq!(Balances::total_balance(validator), 10_000_000);
		// 	assert_eq!(Staking::slashable_balance_of(validator), 10_000);

		// 	assert_eq!(
		// 		Staking::eras_stakers(1, validator),
		// 		pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
		// 	);
		// }

		// We will use the validator at index 1 as the offending authority.
		let offending_validator_index = 1;
		// let offending_validator_id = Session::validators()[offending_validator_index];
		let offending_authority_pair = pairs
			.into_iter()
			.find(|p| p.public() == authorities[offending_validator_index])
			.unwrap();

		// Generate an equivocation proof. It creates two headers at the given
		// slot with different block hashes and signed by the given key.
		let equivocation_proof =
			generate_equivocation_proof(&offending_authority_pair, CurrentSlot::<Test>::get());

		// Create the key ownership proof
		let key = (sp_consensus_aura::KEY_TYPE, &offending_authority_pair.public());
		// Dummy key owner proof
		// let key_owner_proof = Historical::prove(key).unwrap();
		let key_owner_proof =
			sp_session::MembershipProof { session: 0, trie_nodes: vec![], validator_count: 3 };

		// report the equivocation
		Aura::report_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			key_owner_proof,
		)
		.unwrap();

		// // start a new era so that the results of the offence report
		// // are applied at era end
		// start_era(2);

		// // check that the balance of offending validator is slashed 100%.
		// assert_eq!(Balances::total_balance(&offending_validator_id), 10_000_000 - 10_000);
		// assert_eq!(Staking::slashable_balance_of(&offending_validator_id), 0);
		// assert_eq!(
		// 	Staking::eras_stakers(2, offending_validator_id),
		// 	pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
		// );

		// // check that the balances of all other validators are left intact.
		// for validator in &validators {
		// 	if *validator == offending_validator_id {
		// 		continue
		// 	}

		// 	assert_eq!(Balances::total_balance(validator), 10_000_000);
		// 	assert_eq!(Staking::slashable_balance_of(validator), 10_000);
		// 	assert_eq!(
		// 		Staking::eras_stakers(2, validator),
		// 		pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
		// 	);
		// }
	})
}
