// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

#![cfg(test)]
extern crate alloc;

use super::*;

use core::num::NonZeroU32;
use cumulus_pallet_parachain_system::{
	consensus_hook::ExpectParentIncluded, AnyRelayNumber, ParachainSetCode,
};
use cumulus_primitives_core::ParaId;
use frame_support::{
	derive_impl,
	pallet_prelude::ConstU32,
	parameter_types,
	traits::{ConstBool, EnqueueWithOrigin},
};
use sp_core::Get;
use sp_io::TestExternalities;
use sp_version::RuntimeVersion;
use std::cell::RefCell;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		ParachainSystem: cumulus_pallet_parachain_system,
		Aura: pallet_aura,
		AuraExt: crate,
	}
);

parameter_types! {
	pub Version: RuntimeVersion = RuntimeVersion {
		spec_name: "test".into(),
		impl_name: "system-test".into(),
		authoring_version: 1,
		spec_version: 1,
		impl_version: 1,
		apis: sp_version::create_apis_vec!([]),
		transaction_version: 1,
		system_version: 1,
	};
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type Version = Version;
	type OnSetCode = ParachainSetCode<Test>;
	type RuntimeEvent = ();
}

impl crate::Config for Test {}

std::thread_local! {
	pub static PARA_SLOT_DURATION: RefCell<u64> = RefCell::new(6000);
}

pub struct TestSlotDuration;

impl TestSlotDuration {
	pub fn set_slot_duration(slot_duration: u64) {
		PARA_SLOT_DURATION.with(|v| *v.borrow_mut() = slot_duration);
	}
}
impl Get<u64> for TestSlotDuration {
	fn get() -> u64 {
		PARA_SLOT_DURATION.with(|v| v.clone().into_inner())
	}
}

impl pallet_aura::Config for Test {
	type AuthorityId = sp_consensus_aura::sr25519::AuthorityId;
	type MaxAuthorities = ConstU32<100_000>;
	type DisabledValidators = ();
	type AllowMultipleBlocksPerSlot = ConstBool<true>;
	type SlotDuration = TestSlotDuration;
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ();
	type WeightInfo = ();
}

impl cumulus_pallet_parachain_system::Config for Test {
	type WeightInfo = ();
	type RuntimeEvent = ();
	type OnSystemEvent = ();
	type SelfParaId = ();
	type OutboundXcmpMessageSource = ();
	// Ignore all DMP messages by enqueueing them into `()`:
	type DmpQueue = EnqueueWithOrigin<(), sp_core::ConstU8<0>>;
	type ReservedDmpWeight = ();
	type XcmpMessageHandler = ();
	type ReservedXcmpWeight = ();
	type CheckAssociatedRelayNumber = AnyRelayNumber;
	type ConsensusHook = ExpectParentIncluded;
	type RelayParentOffset = ConstU32<0>;
}

#[cfg(test)]
mod test {
	use crate::test::*;
	use cumulus_pallet_parachain_system::{
		Ancestor, ConsensusHook, RelayChainStateProof, UsedBandwidth,
	};
	use rstest::rstest;
	use sp_core::H256;

	fn set_ancestors() {
		let mut ancestors = Vec::new();
		for i in 0..3 {
			let mut ancestor = Ancestor::new_unchecked(UsedBandwidth::default(), None);
			ancestor.replace_para_head_hash(H256::repeat_byte(i + 1));
			ancestors.push(ancestor);
		}
		cumulus_pallet_parachain_system::UnincludedSegment::<Test>::put(ancestors);
	}

	pub fn new_test_ext(para_slot: u64) -> sp_io::TestExternalities {
		let mut ext = TestExternalities::new_empty();
		ext.execute_with(|| {
			set_ancestors();
			// Set initial parachain slot
			pallet_aura::CurrentSlot::<Test>::put(Slot::from(para_slot));
		});
		ext
	}

	fn set_relay_slot(slot: u64, authored: u32) {
		RelaySlotInfo::<Test>::put((Slot::from(slot), authored))
	}

	fn relay_chain_state_proof(relay_slot: u64) -> RelayChainStateProof {
		let mut builder = cumulus_test_relay_sproof_builder::RelayStateSproofBuilder::default();
		builder.current_slot = relay_slot.into();

		let (hash, state_proof) = builder.into_state_root_and_proof();

		RelayChainStateProof::new(ParaId::from(200), hash, state_proof)
			.expect("Should be able to construct state proof.")
	}

	fn assert_slot_info(expected_slot: u64, expected_authored: u32) {
		let (slot, authored) = pallet::RelaySlotInfo::<Test>::get().unwrap();
		assert_eq!(slot, Slot::from(expected_slot), "Slot stored in RelaySlotInfo is incorrect.");
		assert_eq!(
			authored, expected_authored,
			"Number of authored blocks stored in RelaySlotInfo is incorrect."
		);
	}

	const DEFAULT_TEST_VELOCITY: u32 = 2;

	#[test]
	fn test_velocity() {
		type Hook = FixedVelocityConsensusHook<Test, 6000, DEFAULT_TEST_VELOCITY, 1>;

		new_test_ext(1).execute_with(|| {
			let state_proof = relay_chain_state_proof(10);
			let (_, capacity) = Hook::on_state_proof(&state_proof);
			assert_eq!(capacity, NonZeroU32::new(1).unwrap().into());
			assert_slot_info(10, 1);

			let (_, capacity) = Hook::on_state_proof(&state_proof);
			assert_eq!(capacity, NonZeroU32::new(1).unwrap().into());
			assert_slot_info(10, 2);
		});
	}

	#[test]
	fn test_velocity_2() {
		type Hook = FixedVelocityConsensusHook<Test, 6000, DEFAULT_TEST_VELOCITY, 3>;

		new_test_ext(1).execute_with(|| {
			let state_proof = relay_chain_state_proof(10);
			let (_, capacity) = Hook::on_state_proof(&state_proof);
			assert_eq!(capacity, NonZeroU32::new(3).unwrap().into());
			assert_slot_info(10, 1);

			let (_, capacity) = Hook::on_state_proof(&state_proof);
			assert_eq!(capacity, NonZeroU32::new(3).unwrap().into());
			assert_slot_info(10, 2);
		});
	}

	#[test]
	#[should_panic(expected = "authored blocks limit is reached for the slot")]
	fn test_exceeding_velocity_limit() {
		type Hook = FixedVelocityConsensusHook<Test, 6000, DEFAULT_TEST_VELOCITY, 1>;

		new_test_ext(1).execute_with(|| {
			let state_proof = relay_chain_state_proof(10);
			for authored in 0..=DEFAULT_TEST_VELOCITY + 1 {
				Hook::on_state_proof(&state_proof);
				assert_slot_info(10, authored + 1);
			}
		});
	}

	#[test]
	fn test_para_slot_calculated_from_slot_duration() {
		type Hook = FixedVelocityConsensusHook<Test, 3000, DEFAULT_TEST_VELOCITY, 1>;

		new_test_ext(5).execute_with(|| {
			let state_proof = relay_chain_state_proof(10);
			Hook::on_state_proof(&state_proof);
		});
	}

	#[rstest]
	#[should_panic(
		expected = "too far in the future: parachain_slot=Slot(31), derived_from_relay_slot=Slot(30)"
	)]
	#[case::short_para_slot(2000, 31, 10)]
	#[should_panic(
		expected = "too far in the future: parachain_slot=Slot(32), derived_from_relay_slot=Slot(30)"
	)]
	#[case::short_para_slot(2000, 32, 10)]
	#[case::short_para_slot(2000, 30, 10)]
	#[case::short_para_slot(2000, 33, 11)]
	#[case::short_para_slot(2000, 29, 10)]
	#[case::short_para_slot(2000, 1, 10)]
	#[case::normal_para_slot(6000, 1, 10)]
	#[case::normal_para_slot(6000, 9, 10)]
	#[case::normal_para_slot(6000, 10, 10)]
	#[should_panic(
		expected = "too far in the future: parachain_slot=Slot(11), derived_from_relay_slot=Slot(10)"
	)]
	#[case::normal_para_slot(6000, 11, 10)]
	fn test_para_slot_too_high(
		#[case] para_slot_duration: u64,
		#[case] para_slot: u64,
		#[case] relay_slot: u64,
	) {
		type Hook = FixedVelocityConsensusHook<Test, 6000, DEFAULT_TEST_VELOCITY, 1>;

		TestSlotDuration::set_slot_duration(para_slot_duration);
		new_test_ext(para_slot).execute_with(|| {
			let state_proof = relay_chain_state_proof(relay_slot);
			Hook::on_state_proof(&state_proof);
		});
	}

	#[test]
	fn test_velocity_at_least_one() {
		// Even though this is 0, one block should always be allowed.
		const VELOCITY: u32 = 0;
		type Hook = FixedVelocityConsensusHook<Test, 6000, VELOCITY, 1>;

		new_test_ext(6).execute_with(|| {
			let state_proof = relay_chain_state_proof(10);
			Hook::on_state_proof(&state_proof);
		});
	}

	#[test]
	#[should_panic(
		expected = "Parachain slot is too far in the future: parachain_slot=Slot(8), derived_from_relay_slot=Slot(5) velocity=2"
	)]
	fn test_para_slot_calculated_from_slot_duration_2() {
		// Note: In contrast to tests below, relay chain slot duration is 3000 here.
		type Hook = FixedVelocityConsensusHook<Test, 3000, DEFAULT_TEST_VELOCITY, 1>;

		new_test_ext(8).execute_with(|| {
			let state_proof = relay_chain_state_proof(10);
			let (_, _) = Hook::on_state_proof(&state_proof);
		});
	}

	#[test]
	fn test_velocity_resets_on_new_relay_slot() {
		type Hook = FixedVelocityConsensusHook<Test, 6000, DEFAULT_TEST_VELOCITY, 1>;

		new_test_ext(1).execute_with(|| {
			let state_proof = relay_chain_state_proof(10);
			for authored in 0..=DEFAULT_TEST_VELOCITY {
				Hook::on_state_proof(&state_proof);
				assert_slot_info(10, authored + 1);
			}

			let state_proof = relay_chain_state_proof(11);
			for authored in 0..=DEFAULT_TEST_VELOCITY {
				Hook::on_state_proof(&state_proof);
				assert_slot_info(11, authored + 1);
			}
		});
	}

	#[test]
	#[should_panic(
		expected = "Slot moved backwards: stored_slot=Slot(10), relay_chain_slot=Slot(9)"
	)]
	fn test_backward_relay_slot_not_tolerated() {
		type Hook = FixedVelocityConsensusHook<Test, 6000, 2, 1>;

		new_test_ext(1).execute_with(|| {
			let state_proof = relay_chain_state_proof(10);
			Hook::on_state_proof(&state_proof);
			assert_slot_info(10, 1);

			let state_proof = relay_chain_state_proof(9);
			Hook::on_state_proof(&state_proof);
		});
	}

	#[test]
	fn test_can_build_upon_true_when_empty() {
		type Hook = FixedVelocityConsensusHook<Test, 6000, DEFAULT_TEST_VELOCITY, 1>;

		new_test_ext(1).execute_with(|| {
			let hash = H256::repeat_byte(0x1);
			assert!(Hook::can_build_upon(hash, Slot::from(1)));
		});
	}

	#[rstest]
	#[case::slot_higher_ok(10, 11, DEFAULT_TEST_VELOCITY, true)]
	#[case::slot_same_ok(10, 10, DEFAULT_TEST_VELOCITY, true)]
	#[case::slot_decrease_illegal(10, 9, DEFAULT_TEST_VELOCITY, false)]
	#[case::velocity_small_ok(10, 10, DEFAULT_TEST_VELOCITY - 1 , true)]
	#[case::velocity_small_ok(10, 10, DEFAULT_TEST_VELOCITY - 2 , true)]
	#[case::velocity_too_high_illegal(10, 10, DEFAULT_TEST_VELOCITY + 1 , false)]
	fn test_can_build_upon_slot_can_not_decrease(
		#[case] state_relay_slot: u64,
		#[case] test_relay_slot: u64,
		#[case] authored_in_slot: u32,
		#[case] expected_result: bool,
	) {
		type Hook = FixedVelocityConsensusHook<Test, 6000, DEFAULT_TEST_VELOCITY, 10>;

		new_test_ext(1).execute_with(|| {
			let hash = H256::repeat_byte(0x1);

			set_relay_slot(state_relay_slot, authored_in_slot);
			// Slot moves backwards
			assert_eq!(Hook::can_build_upon(hash, Slot::from(test_relay_slot)), expected_result);
		});
	}

	#[test]
	fn test_can_build_upon_unincluded_segment_size() {
		type Hook = FixedVelocityConsensusHook<Test, 6000, DEFAULT_TEST_VELOCITY, 2>;

		new_test_ext(1).execute_with(|| {
			let relay_slot = Slot::from(10);

			set_relay_slot(10, DEFAULT_TEST_VELOCITY);
			// Size after included is two, we can not build
			assert!(!Hook::can_build_upon(H256::repeat_byte(0x1), relay_slot));

			// Size after included is one, we can build
			assert!(Hook::can_build_upon(H256::repeat_byte(0x2), relay_slot));
		});
	}
}
