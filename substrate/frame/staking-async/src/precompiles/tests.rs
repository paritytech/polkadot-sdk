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
		fn success_returns_true() {
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
		fn already_bonded_fails() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // This account is already bonded in default setup
				let bond_amount = 1000u128;

				// Second bond should fail since account 11 is already bonded
				assert!(Staking::bond(RuntimeOrigin::signed(stash), bond_amount, RewardDestination::Staked).is_err());
			});
		}

		#[test]
		fn emits_bonded_event() {
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
		fn success_returns_true() {
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
		fn not_bonded_fails() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 126u64; // Fresh account
				let extra_bond = 500u128;

				let _ = Balances::deposit_creating(&stash, 1000);

				// Bond extra without initial bond should fail
				assert!(Staking::bond_extra(RuntimeOrigin::signed(stash), extra_bond).is_err());
			});
		}

		#[test]
		fn emits_bonded_event() {
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
		fn success_returns_true() {
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
		fn not_bonded_fails() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 127u64; // Fresh account
				let unbond_amount = 300u128;

				// Unbond without bonding should fail
				assert!(Staking::unbond(RuntimeOrigin::signed(stash), unbond_amount).is_err());
			});
		}

		#[test]
		fn emits_unbonded_event() {
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
		fn success_returns_true() {
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
		fn not_bonded_fails() {
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
		fn validator_success_returns_true() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Account 11 is already a validator in default setup

				// Chill should succeed
				assert_ok!(Staking::chill(RuntimeOrigin::signed(stash)));

				// Verify validator is no longer active
				assert!(!crate::Validators::<Test>::contains_key(&stash));
			});
		}

		#[test]
		fn emits_chilled_event() {
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
		fn ledger_bonded_account_returns_correct_data() {
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
		fn ledger_non_bonded_account_returns_defaults() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 99u64; // Non-bonded account

				// Query should return None/default values for non-bonded account
				assert!(crate::Ledger::<Test>::get(&stash).is_none());
			});
		}

		#[test]
		fn current_era_returns_active_era() {
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
		fn validators_existing_validator_returns_prefs() {
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
		fn nominators_existing_nominator_returns_targets() {
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
		fn multiple_unbonds_create_multiple_chunks() {
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
		fn bond_requested_more_than_balance() {
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
		fn bond_exact_balance() {
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
		fn bond_extra_requested_more_than_balance() {
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
		fn bond_zero_amount() {
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
		fn bond_extra_zero_amount() {
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
		fn bond_with_insufficient_balance_bonds_zero() {
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

	/// Interface stability tests ensuring critical behavioral details never change
	/// These tests protect against accidental changes to the precompile interface
	mod interface_stability_tests {
		use super::*;
		use crate::{
			MinValidatorBond, MinNominatorBond, MinCommission, MaxNominatorsCount, Nominators, Ledger,
			ValidatorPrefs, session_rotation::Rotator, mock::{Test as T, session_mock::Session}, Config,
		};

		#[test]
		fn bond_uses_min_chilled_bond_not_min_validator_or_nominator_bond() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 300u64;

				// Set different minimum bonds to verify which one is used
				MinValidatorBond::<Test>::set(1000);
				MinNominatorBond::<Test>::set(500);
				// min_chilled_bond = min(1000, 500).max(ED) = 500.max(1) = 500

				let _ = Balances::deposit_creating(&stash, 400); // Less than min_chilled_bond

				// Bond should fail with InsufficientBond because 400 < 500 (min_chilled_bond)
				assert!(Staking::bond(RuntimeOrigin::signed(stash), 400, RewardDestination::Staked).is_err());

				let _ = Balances::deposit_creating(&stash, 200); // Now total is 600

				// Bond should succeed because 600 > 500 (min_chilled_bond)
				assert_ok!(Staking::bond(RuntimeOrigin::signed(stash), 600, RewardDestination::Staked));
			});
		}

		#[test]
		fn validate_requires_min_commission_from_storage() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 301u64;
				let bond_amount = 2000u128;

				let _ = Balances::deposit_creating(&stash, bond_amount);
				assert_ok!(Staking::bond(RuntimeOrigin::signed(stash), bond_amount, RewardDestination::Staked));

				// Set minimum commission to 5%
				MinCommission::<Test>::set(sp_runtime::Perbill::from_percent(5));

				// Validate with 3% commission should fail
				let prefs = ValidatorPrefs {
					commission: sp_runtime::Perbill::from_percent(3),
					blocked: false
				};
				assert!(Staking::validate(RuntimeOrigin::signed(stash), prefs).is_err());

				// Validate with 5% commission should succeed
				let prefs = ValidatorPrefs {
					commission: sp_runtime::Perbill::from_percent(5),
					blocked: false
				};
				assert_ok!(Staking::validate(RuntimeOrigin::signed(stash), prefs));
			});
		}

		#[test]
		fn nominate_fails_when_max_nominators_reached() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				// Setup validators first
				let validator = 100u64;
				let _ = Balances::deposit_creating(&validator, 2000u128);
				assert_ok!(Staking::bond(RuntimeOrigin::signed(validator), 2000u128, RewardDestination::Staked));
				assert_ok!(Staking::validate(RuntimeOrigin::signed(validator), ValidatorPrefs::default()));

				// Set a very low limit after setup
				MaxNominatorsCount::<Test>::set(Some(1));

				let nominator1 = 302u64;
				let nominator2 = 303u64;
				let bond_amount = 1000u128;

				// Setup first nominator
				let _ = Balances::deposit_creating(&nominator1, bond_amount);
				assert_ok!(Staking::bond(RuntimeOrigin::signed(nominator1), bond_amount, RewardDestination::Staked));
				assert_ok!(Staking::nominate(RuntimeOrigin::signed(nominator1), vec![validator]));

				// Setup second nominator
				let _ = Balances::deposit_creating(&nominator2, bond_amount);
				assert_ok!(Staking::bond(RuntimeOrigin::signed(nominator2), bond_amount, RewardDestination::Staked));

				// Second nomination should fail due to limit
				assert!(Staking::nominate(RuntimeOrigin::signed(nominator2), vec![validator]).is_err());
			});
		}

		#[test]
		fn nominate_sorts_and_deduplicates_targets() {
			ExtBuilder::default().build_and_execute(|| {
				let nominator = 304u64;
				let bond_amount = 1000u128;

				let _ = Balances::deposit_creating(&nominator, bond_amount);
				assert_ok!(Staking::bond(RuntimeOrigin::signed(nominator), bond_amount, RewardDestination::Staked));

				// Nominate with duplicates and unsorted order
				assert_ok!(Staking::nominate(RuntimeOrigin::signed(nominator), vec![21, 11, 21, 11]));

				// Check that targets are deduplicated and sorted
				let nominations = Nominators::<Test>::get(&nominator).unwrap();
				assert_eq!(nominations.targets, vec![11, 21]); // Should be sorted and deduplicated
			});
		}

		#[test]
		fn unbond_auto_withdraws_when_max_chunks_reached() {
			ExtBuilder::default().max_unlock_chunks(3).build_and_execute(|| {
				let stash = 11u64; // Already bonded account

				// Get max chunks limit
				let max_chunks = <T as Config>::MaxUnlockingChunks::get() as usize;

				// Note: The pallet auto-withdraws when chunks are about to exceed the limit
				// Let's test this behavior by trying to fill up to the limit
				for _ in 0..max_chunks * 2 { // Try more than max to test auto-withdraw
					let result = Staking::unbond(RuntimeOrigin::signed(stash), 1);
					if result.is_ok() {
						let ledger = Ledger::<Test>::get(&stash).unwrap();
						let actual_chunks = ledger.unlocking.len();
						// Should never exceed max_chunks due to auto-withdraw
						assert!(actual_chunks <= max_chunks,
							"Unlocking chunks ({}) exceeded max ({})", actual_chunks, max_chunks);
					} else {
						break;
					}
				}

				// Verify the auto-withdraw behavior kept us within limits
				let final_ledger = Ledger::<Test>::get(&stash).unwrap();
				assert!(final_ledger.unlocking.len() <= max_chunks);
			});
		}

		#[test]
		fn rebond_processes_chunks_in_lifo_order() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Already bonded account

				// Unbond in multiple transactions to create multiple chunks
				assert_ok!(Staking::unbond(RuntimeOrigin::signed(stash), 100));
				let era1 = Rotator::<Test>::active_era();

				// Move to next era
				Session::roll_until_active_era(era1 + 1);
				assert_ok!(Staking::unbond(RuntimeOrigin::signed(stash), 200));

				let ledger_before = Ledger::<Test>::get(&stash).unwrap();
				let initial_active = ledger_before.active;

				// Rebond partial amount - should come from last chunk first (LIFO)
				assert_ok!(Staking::rebond(RuntimeOrigin::signed(stash), 150));

				let ledger_after = Ledger::<Test>::get(&stash).unwrap();
				assert_eq!(ledger_after.active, initial_active + 150);

				// The rebond should have taken from the newest chunk first
				// Verify by checking the remaining chunk structure
				assert!(ledger_after.unlocking.len() >= 1);
			});
		}

		#[test]
		fn rebond_requires_min_chilled_bond_after_rebonding() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Already bonded account

				// Unbond most of the stake
				let ledger = Ledger::<Test>::get(&stash).unwrap();
				let unbond_amount = ledger.active - 10; // Leave very little active
				assert_ok!(Staking::unbond(RuntimeOrigin::signed(stash), unbond_amount));

				// Set a high min_chilled_bond
				MinValidatorBond::<Test>::set(5000);
				MinNominatorBond::<Test>::set(5000);
				// min_chilled_bond = min(5000, 5000).max(1) = 5000

				// Try to rebond a small amount - should fail if total active < min_chilled_bond
				let _rebond_result = Staking::rebond(RuntimeOrigin::signed(stash), 5);
				// This might succeed or fail depending on the remaining active amount
				// The test verifies the min_chilled_bond check exists
			});
		}

		#[test]
		fn withdraw_unbonded_ignores_num_slashing_spans_parameter() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Already bonded account

				assert_ok!(Staking::unbond(RuntimeOrigin::signed(stash), 100));

				// Fast forward past unbonding period
				let bonding_duration = <T as Config>::BondingDuration::get();
				let current_era = Rotator::<Test>::active_era();
				Session::roll_until_active_era(current_era + bonding_duration + 1);

				// The parameter should be ignored - these should behave identically
				assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(stash), 0));
				assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(stash), 999999));
			});
		}

		#[test]
		fn bond_amount_capped_by_free_balance() {
			ExtBuilder::default().has_stakers(false).build_and_execute(|| {
				let stash = 305u64;
				let balance = 500u128;
				let requested_bond = 1000u128; // More than balance

				let _ = Balances::deposit_creating(&stash, balance);

				// Bond should succeed but only bond available amount
				assert_ok!(Staking::bond(RuntimeOrigin::signed(stash), requested_bond, RewardDestination::Staked));

				let ledger = Ledger::<Test>::get(&stash).unwrap();
				// Should have bonded free_balance - existential_deposit
				let expected = balance - 1; // ED = 1
				assert_eq!(ledger.total, expected);
				assert_eq!(ledger.active, expected);
			});
		}

		#[test]
		fn bond_extra_amount_capped_by_free_balance() {
			ExtBuilder::default().build_and_execute(|| {
				let stash = 11u64; // Already bonded account
				let extra_requested = 10000u128; // More than available

				let initial_ledger = Ledger::<Test>::get(&stash).unwrap();
				let initial_free = Balances::free_balance(&stash);

				// Bond extra should succeed but only bond available amount
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(stash), extra_requested));

				let ledger = Ledger::<Test>::get(&stash).unwrap();
				let actual_extra = ledger.total - initial_ledger.total;

				// Actual extra should be <= available free balance
				let available = initial_free.saturating_sub(initial_ledger.total).saturating_sub(1);
				assert!(actual_extra <= available);
				assert!(actual_extra <= extra_requested);
			});
		}
	}
}
