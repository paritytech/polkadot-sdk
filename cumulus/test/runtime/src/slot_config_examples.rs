// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Examples and tests for dynamic slot configuration

#![cfg(test)]

use super::*;
use frame_support::{
	assert_noop, assert_ok,
	traits::{OnFinalize, OnInitialize},
};
use frame_system::RawOrigin;
use sp_runtime::traits::BadOrigin;

type Block = frame_system::mocking::MockBlock<Runtime>;

/// Helper function to run to given block number
fn run_to_block(n: u32) {
	while System::block_number() < n {
		let block_number = System::block_number() + 1;
		System::set_block_number(block_number);
		
		// Initialize all pallets
		System::on_initialize(block_number);
		SlotConfig::on_initialize(block_number);
		
		// Finalize all pallets  
		SlotConfig::on_finalize(block_number);
		System::on_finalize(block_number);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Test initial slot duration setup
	#[test]
	fn test_initial_slot_duration() {
		// In genesis, slot duration should be set to default value (SLOT_DURATION constant)
		new_test_ext().execute_with(|| {
			let initial_duration = SlotConfig::current_slot_duration();
			assert_eq!(initial_duration, SLOT_DURATION);
		});
	}

	/// Test successful slot duration update via Root
	#[test]
	fn test_set_slot_duration_root_success() {
		new_test_ext().execute_with(|| {
			let new_duration = 8000u64; // 8 seconds
			
			// Root should be able to update slot duration
			assert_ok!(SlotConfig::set_slot_duration(
				RawOrigin::Root.into(),
				new_duration
			));
			
			// Check that duration was updated
			assert_eq!(SlotConfig::current_slot_duration(), new_duration);
			
			// Check that event was emitted
			let events = System::events();
			assert!(!events.is_empty());
			
			// Last event should be SlotDurationUpdated
			let last_event = &events[events.len() - 1];
			match &last_event.event {
				RuntimeEvent::SlotConfig(slot_config::Event::SlotDurationUpdated { 
					old_duration, 
					new_duration: updated_duration 
				}) => {
					assert_eq!(*old_duration, SLOT_DURATION);
					assert_eq!(*updated_duration, new_duration);
				},
				_ => panic!("Expected SlotDurationUpdated event"),
			}
		});
	}

	/// Test slot duration update with invalid origin should fail
	#[test]
	fn test_set_slot_duration_invalid_origin() {
		new_test_ext().execute_with(|| {
			let new_duration = 8000u64;
			
			// Signed origin should not be able to update slot duration
			assert_noop!(
				SlotConfig::set_slot_duration(
					RawOrigin::Signed(AccountId::from([1u8; 32])).into(),
					new_duration
				),
				BadOrigin
			);
			
			// Duration should remain unchanged
			assert_eq!(SlotConfig::current_slot_duration(), SLOT_DURATION);
		});
	}

	/// Test slot duration validation - zero duration should fail
	#[test]
	fn test_set_slot_duration_zero_fails() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				SlotConfig::set_slot_duration(RawOrigin::Root.into(), 0),
				slot_config::Error::<Runtime>::ZeroSlotDuration
			);
		});
	}

	/// Test slot duration validation - too small duration should fail
	#[test]
	fn test_set_slot_duration_too_small() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				SlotConfig::set_slot_duration(RawOrigin::Root.into(), 500), // 500ms < 1000ms minimum
				slot_config::Error::<Runtime>::SlotDurationTooSmall
			);
		});
	}

	/// Test slot duration validation - too large duration should fail
	#[test]
	fn test_set_slot_duration_too_large() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				SlotConfig::set_slot_duration(RawOrigin::Root.into(), 70000), // 70s > 60s maximum
				slot_config::Error::<Runtime>::SlotDurationTooLarge
			);
		});
	}

	/// Test that DynamicSlotDuration trait correctly reads from storage
	#[test]
	fn test_dynamic_slot_duration_trait() {
		new_test_ext().execute_with(|| {
			// Initially should return default value
			assert_eq!(
				slot_config::DynamicSlotDuration::<Runtime>::get(),
				SLOT_DURATION
			);
			
			// Update slot duration
			let new_duration = 10000u64;
			assert_ok!(SlotConfig::set_slot_duration(
				RawOrigin::Root.into(),
				new_duration
			));
			
			// DynamicSlotDuration should now return updated value
			assert_eq!(
				slot_config::DynamicSlotDuration::<Runtime>::get(),
				new_duration
			);
		});
	}

	/// Test multiple slot duration updates
	#[test]
	fn test_multiple_slot_duration_updates() {
		new_test_ext().execute_with(|| {
			let durations = vec![4000u64, 8000u64, 12000u64, 6000u64];
			
			for (i, &duration) in durations.iter().enumerate() {
				let old_duration = SlotConfig::current_slot_duration();
				
				run_to_block((i + 1) as u32);
				
				assert_ok!(SlotConfig::set_slot_duration(
					RawOrigin::Root.into(),
					duration
				));
				
				assert_eq!(SlotConfig::current_slot_duration(), duration);
				
				// Check that the trait also returns the correct value
				assert_eq!(
					slot_config::DynamicSlotDuration::<Runtime>::get(),
					duration
				);
			}
		});
	}
}

/// Example of how to use with Sudo pallet
#[cfg(test)]
mod sudo_examples {
	use super::*;
	use pallet_sudo::Call as SudoCall;

	#[test]
	fn test_slot_duration_update_via_sudo() {
		new_test_ext().execute_with(|| {
			let new_duration = 15000u64; // 15 seconds
			
			// Create a call to update slot duration
			let call = RuntimeCall::SlotConfig(
				slot_config::Call::set_slot_duration { new_duration }
			);
			
			// Wrap it in a sudo call
			let sudo_call = RuntimeCall::Sudo(SudoCall::sudo { call: Box::new(call) });
			
			// Execute via sudo (would be called by the sudo key holder)
			// Note: In real scenario, this would be executed as an extrinsic
			// For testing, we just verify the call can be created
			match sudo_call {
				RuntimeCall::Sudo(SudoCall::sudo { call }) => {
					match *call {
						RuntimeCall::SlotConfig(slot_config::Call::set_slot_duration { new_duration: duration }) => {
							assert_eq!(duration, new_duration);
						},
						_ => panic!("Wrong call type"),
					}
				},
				_ => panic!("Wrong sudo call type"),
			}
		});
	}
}

/// Mock test environment setup
fn new_test_ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Runtime>::default()
		.build_storage()
		.unwrap();

	// Initialize SlotConfig with default duration
	slot_config::GenesisConfig::<Runtime> {
		slot_duration: SLOT_DURATION,
		_config: Default::default(),
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	storage.into()
}


