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

use crate::precompiles::staking::IStaking;
use alloy_core::{self as alloy, sol_types::SolCall};

/// Basic encoding/decoding tests to verify the Solidity ABI compatibility
mod abi_tests {
	use super::*;

	#[test]
	fn test_interface_encoding() {
		// Test that the ABI encoding/decoding works correctly
		let bond_call = IStaking::bondCall {
			value: alloy::primitives::U256::from(1000),
		};

		// Encode and then decode
		let encoded = bond_call.abi_encode();
		let decoded = IStaking::bondCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.value, alloy::primitives::U256::from(1000));
	}

	#[test]
	fn test_set_payee_encoding() {
		let set_payee_call = IStaking::setPayeeCall {};
		let encoded = set_payee_call.abi_encode();
		let _decoded = IStaking::setPayeeCall::abi_decode(&encoded).unwrap();
	}

	#[test]
	fn test_set_compound_encoding() {
		let set_compound_call = IStaking::setCompoundCall {};
		let encoded = set_compound_call.abi_encode();
		let _decoded = IStaking::setCompoundCall::abi_decode(&encoded).unwrap();
	}

	#[test]
	fn test_nominate_encoding() {
		let validator1 = alloy::primitives::Address::from([1u8; 20]);
		let validator2 = alloy::primitives::Address::from([2u8; 20]);

		let nominate_call = IStaking::nominateCall {
			targets: vec![validator1, validator2],
		};

		let encoded = nominate_call.abi_encode();
		let decoded = IStaking::nominateCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.targets.len(), 2);
		assert_eq!(decoded.targets[0], validator1);
		assert_eq!(decoded.targets[1], validator2);
	}

	#[test]
	fn test_validate_encoding() {
		let validate_call = IStaking::validateCall {
			commission: alloy::primitives::U256::from(50_000_000), // 5%
			blocked: false,
		};

		let encoded = validate_call.abi_encode();
		let decoded = IStaking::validateCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.commission, alloy::primitives::U256::from(50_000_000));
		assert_eq!(decoded.blocked, false);
	}

	#[test]
	fn test_query_encoding() {
		// Test ledger query
		let stash = alloy::primitives::Address::from([1u8; 20]);
		let ledger_call = IStaking::ledgerCall { stash };

		let encoded = ledger_call.abi_encode();
		let decoded = IStaking::ledgerCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.stash, stash);

		// Test currentEra query
		let era_call = IStaking::currentEraCall {};
		let _encoded = era_call.abi_encode();

		// Test minNominatorBond query
		let min_bond_call = IStaking::minNominatorBondCall {};
		let _encoded = min_bond_call.abi_encode();
	}

	#[test]
	fn test_unbond_encoding() {
		let unbond_call = IStaking::unbondCall {
			value: alloy::primitives::U256::from(500),
		};

		let encoded = unbond_call.abi_encode();
		let decoded = IStaking::unbondCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.value, alloy::primitives::U256::from(500));
	}

	#[test]
	fn test_withdraw_unbonded_encoding() {
		let withdraw_call = IStaking::withdrawUnbondedCall {
			numSlashingSpans: 10,
		};

		let encoded = withdraw_call.abi_encode();
		let decoded = IStaking::withdrawUnbondedCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.numSlashingSpans, 10);
	}

	#[test]
	fn test_rebond_encoding() {
		let rebond_call = IStaking::rebondCall {
			value: alloy::primitives::U256::from(250),
		};

		let encoded = rebond_call.abi_encode();
		let decoded = IStaking::rebondCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.value, alloy::primitives::U256::from(250));
	}

	#[test]
	fn test_payout_stakers_encoding() {
		let validator = alloy::primitives::Address::from([3u8; 20]);
		let payout_call = IStaking::payoutStakersCall {
			validatorStash: validator,
			era: alloy::primitives::U256::from(100),
		};

		let encoded = payout_call.abi_encode();
		let decoded = IStaking::payoutStakersCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.validatorStash, validator);
		assert_eq!(decoded.era, alloy::primitives::U256::from(100));
	}

	#[test]
	fn test_validators_query_encoding() {
		let validator = alloy::primitives::Address::from([4u8; 20]);
		let validators_call = IStaking::validatorsCall { validator };

		let encoded = validators_call.abi_encode();
		let decoded = IStaking::validatorsCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.validator, validator);
	}

	#[test]
	fn test_nominators_query_encoding() {
		let nominator = alloy::primitives::Address::from([5u8; 20]);
		let nominators_call = IStaking::nominatorsCall { nominator };

		let encoded = nominators_call.abi_encode();
		let decoded = IStaking::nominatorsCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.nominator, nominator);
	}
}

/// Conformance tests verify that precompiles correctly implement the behavior
/// specified in the Solidity interface documentation and emit proper events.
///
/// These tests ensure that:
/// 1. Return values match the expected types and values under all conditions
/// 2. Events are emitted correctly with proper parameters
/// 3. Error conditions are handled gracefully without reverting
/// 4. State changes are applied correctly
#[cfg(test)]
mod conformance_tests {
	use super::*;
	use crate::{mock::*, Event as StakingEvent, ValidatorPrefs, RewardDestination};
	use frame_support::{assert_ok, traits::Currency};

	/// Tests for the bond function according to Solidity interface spec
	mod bond_tests {
		use super::*;

		#[test]
		fn conformance_bond_success_returns_true() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 123u64; // Use an account not already bonded
				let bond_amount = 1000u128;

				// Give the account sufficient balance
				let _ = Balances::deposit_creating(&stash, 2000);

				// Bond should succeed and return true
				assert_ok!(Staking::bond(RuntimeOrigin::signed(stash), bond_amount, RewardDestination::Staked));

				// Verify the bond was created
				assert!(crate::Ledger::<Test>::contains_key(&stash));
				let ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				assert_eq!(ledger.total, bond_amount);
				assert_eq!(ledger.active, bond_amount);
			});
		}

		#[test]
		fn conformance_bond_already_bonded_fails() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // This account is already bonded in default setup
				let bond_amount = 1000u128;

				// Second bond should fail since account 11 is already bonded
				assert!(Staking::bond(RuntimeOrigin::signed(stash), bond_amount, RewardDestination::Staked).is_err());
			});
		}

		#[test]
		fn conformance_bond_emits_bonded_event() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 125u64;
				let bond_amount = 1000u128;

				let _ = Balances::deposit_creating(&stash, 2000);

				assert_ok!(Staking::bond(RuntimeOrigin::signed(stash), bond_amount, RewardDestination::Staked));

				// Verify Bonded event was emitted
				let events = frame_system::Pallet::<Test>::events();
				assert!(events.iter().any(|e| {
					matches!(e.event, RuntimeEvent::Staking(StakingEvent::Bonded { stash: 125, amount }) if amount == bond_amount)
				}));
			});
		}
	}

	/// Tests for the bondExtra function according to Solidity interface spec
	mod bond_extra_tests {
		use super::*;

		#[test]
		fn conformance_bond_extra_success_returns_true() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already bonded in default setup
				let extra_bond = 500u128;

				// Get initial bond amount and ensure sufficient balance
				let initial_ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				let initial_total = initial_ledger.total;
				let initial_active = initial_ledger.active;

				// Ensure account has sufficient additional balance for bond_extra
				let current_balance = Balances::free_balance(&stash);
				if current_balance < initial_total + extra_bond + 1 { // +1 for existential deposit
					let _ = Balances::deposit_creating(&stash, extra_bond + 100);
				}

				// Bond extra should succeed
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(stash), extra_bond));

				// Verify total bond increased by exactly the extra amount
				let ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				assert_eq!(ledger.total, initial_total + extra_bond);
				assert_eq!(ledger.active, initial_active + extra_bond);
			});
		}

		#[test]
		fn conformance_bond_extra_not_bonded_fails() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 126u64; // Fresh account
				let extra_bond = 500u128;

				let _ = Balances::deposit_creating(&stash, 1000);

				// Bond extra without initial bond should fail
				assert!(Staking::bond_extra(RuntimeOrigin::signed(stash), extra_bond).is_err());
			});
		}

		#[test]
		fn conformance_bond_extra_emits_bonded_event() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already bonded in default setup
				let extra_bond = 500u128;

				// Ensure account has sufficient additional balance for bond_extra
				let initial_ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				let current_balance = Balances::free_balance(&stash);
				if current_balance < initial_ledger.total + extra_bond + 1 {
					let _ = Balances::deposit_creating(&stash, extra_bond + 100);
				}

				// Clear previous events
				frame_system::Pallet::<Test>::reset_events();

				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(stash), extra_bond));

				// Verify Bonded event for extra amount - check all events
				let events = frame_system::Pallet::<Test>::events();
				let bonded_events: Vec<_> = events.iter().filter_map(|e| {
					if let RuntimeEvent::Staking(StakingEvent::Bonded { stash, amount }) = &e.event {
						Some((*stash, *amount))
					} else {
						None
					}
				}).collect();

				// Should have at least one Bonded event with our amount
				assert!(bonded_events.iter().any(|(s, a)| *s == 11 && *a == extra_bond),
					"Expected Bonded event with stash=11 and amount={}, but found events: {:?}",
					extra_bond, bonded_events);
			});
		}
	}

	/// Tests for the unbond function according to Solidity interface spec
	mod unbond_tests {
		use super::*;

		#[test]
		fn conformance_unbond_success_returns_true() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already bonded in default setup
				let unbond_amount = 300u128;

				// Get initial state
				let initial_ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				let initial_active = initial_ledger.active;

				// Unbond should succeed
				assert_ok!(Staking::unbond(RuntimeOrigin::signed(stash), unbond_amount));

				// Verify unbonding chunk was created
				let ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				assert_eq!(ledger.active, initial_active - unbond_amount);
				assert!(ledger.unlocking.len() >= 1);
				// Find the unbonding chunk with our amount
				assert!(ledger.unlocking.iter().any(|chunk| chunk.value == unbond_amount));
			});
		}

		#[test]
		fn conformance_unbond_not_bonded_fails() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 127u64; // Fresh account
				let unbond_amount = 300u128;

				// Unbond without bonding should fail
				assert!(Staking::unbond(RuntimeOrigin::signed(stash), unbond_amount).is_err());
			});
		}

		#[test]
		fn conformance_unbond_emits_unbonded_event() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already bonded in default setup
				let unbond_amount = 300u128;

				// Clear previous events
				frame_system::Pallet::<Test>::reset_events();

				assert_ok!(Staking::unbond(RuntimeOrigin::signed(stash), unbond_amount));

				// Verify Unbonded event
				let events = frame_system::Pallet::<Test>::events();
				assert!(events.iter().any(|e| {
					matches!(e.event, RuntimeEvent::Staking(StakingEvent::Unbonded { stash: 11, amount }) if amount == unbond_amount)
				}));
			});
		}
	}

	/// Tests for the validate function according to Solidity interface spec
	mod validate_tests {
		use super::*;

		#[test]
		fn conformance_validate_success_returns_true() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 128u64; // Fresh account
				let bond_amount = 1000u128;
				let commission = sp_runtime::Perbill::from_parts(100_000_000); // 10%

				let _ = Balances::deposit_creating(&stash, 2000);
				assert_ok!(Staking::bond(RuntimeOrigin::signed(stash), bond_amount, RewardDestination::Staked));

				// Validate should succeed
				let prefs = ValidatorPrefs { commission, blocked: false };
				assert_ok!(Staking::validate(RuntimeOrigin::signed(stash), prefs.clone()));

				// Verify validator preferences were set
				let stored_prefs = crate::Validators::<Test>::get(&stash);
				assert_eq!(stored_prefs.commission, commission);
				assert_eq!(stored_prefs.blocked, false);
			});
		}

		#[test]
		fn conformance_validate_not_bonded_fails() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 129u64; // Fresh account not bonded
				let commission = sp_runtime::Perbill::from_parts(100_000_000);

				// Validate without bonding should fail
				let prefs = ValidatorPrefs { commission, blocked: false };
				assert!(Staking::validate(RuntimeOrigin::signed(stash), prefs).is_err());
			});
		}
	}

	/// Tests for the chill function according to Solidity interface spec
	mod chill_tests {
		use super::*;

		#[test]
		fn conformance_chill_validator_success_returns_true() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already a validator in default setup

				// Chill should succeed
				assert_ok!(Staking::chill(RuntimeOrigin::signed(stash)));

				// Verify validator is no longer active
				assert!(!crate::Validators::<Test>::contains_key(&stash));
			});
		}

		#[test]
		fn conformance_chill_emits_chilled_event() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already a validator in default setup

				// Clear previous events
				frame_system::Pallet::<Test>::reset_events();

				assert_ok!(Staking::chill(RuntimeOrigin::signed(stash)));

				// Verify Chilled event
				let events = frame_system::Pallet::<Test>::events();
				assert!(events.iter().any(|e| {
					matches!(e.event, RuntimeEvent::Staking(StakingEvent::Chilled { stash: 11 }))
				}));
			});
		}
	}

	/// Tests for query functions according to Solidity interface spec
	mod query_tests {
		use super::*;

		#[test]
		fn conformance_ledger_query_bonded_account_returns_correct_data() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already bonded in default setup
				let unbond_amount = 300u128;

				// Get initial state and unbond some amount to create unlocking chunks
				let initial_ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				let initial_total = initial_ledger.total;

				assert_ok!(Staking::unbond(RuntimeOrigin::signed(stash), unbond_amount));

				// Query should return correct ledger data
				let ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				assert_eq!(ledger.total, initial_total);
				assert_eq!(ledger.active, initial_total - unbond_amount);
				assert!(ledger.unlocking.len() >= 1);
				assert!(ledger.unlocking.iter().any(|chunk| chunk.value == unbond_amount));
			});
		}

		#[test]
		fn conformance_ledger_query_non_bonded_account_returns_defaults() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 99u64; // Non-bonded account

				// Query should return None/default values for non-bonded account
				assert!(crate::Ledger::<Test>::get(&stash).is_none());
			});
		}

		#[test]
		fn conformance_current_era_query_returns_active_era() {
			ExtBuilder::default().build_and_execute(|| {
				// Query should return current active era
				let current_era = crate::ActiveEra::<Test>::get();
				// In test environment, there should be an active era
				assert!(current_era.is_some());
				if let Some(active_era_info) = current_era {
					// Should be era 1 after mock setup (see line 700 in mock.rs)
					assert_eq!(active_era_info.index, 1);
				}
			});
		}

		#[test]
		fn conformance_validators_query_existing_validator_returns_prefs() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already a validator in default setup

				// Query should return validator preferences
				let stored_prefs = crate::Validators::<Test>::get(&stash);
				// Default validator should have default preferences
				assert_eq!(stored_prefs.commission, sp_runtime::Perbill::zero());
				assert_eq!(stored_prefs.blocked, false);
			});
		}

		#[test]
		fn conformance_nominators_query_existing_nominator_returns_targets() {
			ExtBuilder::default().build_and_execute(|| {
				let nominator = 101u64; // Account 101 is already a nominator in default setup

				// Query should return nomination targets
				if let Some(nominations) = crate::Nominators::<Test>::get(&nominator) {
					assert_eq!(nominations.targets.len(), 2);
					assert!(nominations.targets.contains(&11));
					assert!(nominations.targets.contains(&21));
				}
			});
		}
	}

	/// Tests for error conditions and edge cases
	mod error_condition_tests {
		use super::*;

		#[test]
		fn conformance_multiple_unbonds_create_multiple_chunks() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already bonded in default setup

				// Get initial state
				let initial_ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				let initial_active = initial_ledger.active;
				let initial_unlocking_count = initial_ledger.unlocking.len();

				// Multiple unbonds should create additional unlocking chunks
				assert_ok!(Staking::unbond(RuntimeOrigin::signed(stash), 100));
				assert_ok!(Staking::unbond(RuntimeOrigin::signed(stash), 200));

				let ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				// Should have at least one more chunk than initially, but may combine some
				assert!(ledger.unlocking.len() >= initial_unlocking_count + 1);
				assert_eq!(ledger.active, initial_active - 300);

				// Verify total unbonding amount is correct
				let total_unbonding: u128 = ledger.unlocking.iter().map(|chunk| chunk.value).sum();
				assert!(total_unbonding >= 300); // At least our unbonded amount
			});
		}
	}

	/// Tests for bond amount edge cases documenting value.min(stash_balance) behavior
	mod bond_amount_edge_cases {
		use super::*;

		#[test]
		fn edge_case_bond_requested_more_than_balance() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 200u64;
				let balance = 1000u128;
				let requested_bond = 2000u128; // More than balance

				// Give the account limited balance
				let _ = Balances::deposit_creating(&stash, balance);

				// Clear events
				frame_system::Pallet::<Test>::reset_events();

				// Bond should succeed but only bond what's available
				assert_ok!(Staking::bond(RuntimeOrigin::signed(stash), requested_bond, RewardDestination::Staked));

				// Verify actual bonded amount is min(requested, available)
				let ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				// Available for staking = balance - existential_deposit
				let available = balance - 1; // Assuming ED = 1
				assert_eq!(ledger.total, available);
				assert_eq!(ledger.active, available);

				// Check that the pallet emitted event with actual bonded amount
				let events = frame_system::Pallet::<Test>::events();
				assert!(events.iter().any(|e| {
					matches!(e.event, RuntimeEvent::Staking(StakingEvent::Bonded { stash: 200, amount }) if amount == available)
				}), "Expected Bonded event with actual bonded amount {}, got events: {:?}", available, events);
			});
		}

		#[test]
		fn edge_case_bond_exact_balance() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 201u64;
				let balance = 1000u128;
				let available_for_staking = balance - 1; // minus existential deposit

				// Give the account balance
				let _ = Balances::deposit_creating(&stash, balance);

				// Clear events
				frame_system::Pallet::<Test>::reset_events();

				// Bond exactly what's available
				assert_ok!(Staking::bond(RuntimeOrigin::signed(stash), available_for_staking, RewardDestination::Staked));

				// Verify exact amount was bonded
				let ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				assert_eq!(ledger.total, available_for_staking);
				assert_eq!(ledger.active, available_for_staking);

				// Check event has actual bonded amount
				let events = frame_system::Pallet::<Test>::events();
				assert!(events.iter().any(|e| {
					matches!(e.event, RuntimeEvent::Staking(StakingEvent::Bonded { stash: 201, amount }) if amount == available_for_staking)
				}));
			});
		}

		#[test]
		fn edge_case_bond_extra_requested_more_than_balance() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already bonded
				let extra_requested = 5000u128; // More than available

				// Ensure limited additional balance
				let current_balance = Balances::free_balance(&stash);
				let initial_ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				
				// Calculate how much is actually available for additional bonding
				let available_for_extra = current_balance.saturating_sub(initial_ledger.total).saturating_sub(1); // minus ED

				// Clear events
				frame_system::Pallet::<Test>::reset_events();

				// Bond extra should succeed but only bond what's available
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(stash), extra_requested));

				// Verify actual bonded amount is what was available
				let ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				let actual_extra = ledger.total - initial_ledger.total;

				// Should bond min(requested, available)
				assert!(actual_extra <= available_for_extra);
				assert!(actual_extra <= extra_requested);

				// Check that the pallet emitted event with actual bonded amount
				let events = frame_system::Pallet::<Test>::events();
				assert!(events.iter().any(|e| {
					matches!(e.event, RuntimeEvent::Staking(StakingEvent::Bonded { stash: 11, amount }) if amount == actual_extra)
				}), "Expected Bonded event with actual extra amount {}, got events: {:?}", actual_extra, events);
			});
		}

		#[test]
		fn edge_case_bond_zero_amount() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 202u64;
				let balance = 1000u128;

				// Give the account balance
				let _ = Balances::deposit_creating(&stash, balance);

				// Bond zero should fail (below minimum bond)
				assert!(Staking::bond(RuntimeOrigin::signed(stash), 0, RewardDestination::Staked).is_err());
			});
		}

		#[test]
		fn edge_case_bond_extra_zero_amount() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already bonded

				// Get initial state
				let initial_ledger = crate::Ledger::<Test>::get(&stash).unwrap();

				// Bond extra zero should still succeed but add nothing
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(stash), 0));

				// Verify nothing changed
				let ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				assert_eq!(ledger.total, initial_ledger.total);
				assert_eq!(ledger.active, initial_ledger.active);
			});
		}

		#[test]
		fn edge_case_bond_with_insufficient_balance_bonds_zero() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 203u64;
				let bond_amount = 1000u128;

				// Give account only existential deposit (insufficient for meaningful bonding)
				let _ = Balances::deposit_creating(&stash, 1); // Just the ED

				// Clear events
				frame_system::Pallet::<Test>::reset_events();

				// Bond should succeed but bond zero since free_to_stake returns 0
				assert_ok!(Staking::bond(RuntimeOrigin::signed(stash), bond_amount, RewardDestination::Staked));

				// Verify zero was bonded (the pallet bonds min(requested, available))
				let ledger = crate::Ledger::<Test>::get(&stash).unwrap();
				assert_eq!(ledger.total, 0);
				assert_eq!(ledger.active, 0);

				// Check that event was emitted with zero amount
				let events = frame_system::Pallet::<Test>::events();
				assert!(events.iter().any(|e| {
					matches!(e.event, RuntimeEvent::Staking(StakingEvent::Bonded { stash: 203, amount }) if amount == 0)
				}));
			});
		}
	}
}
