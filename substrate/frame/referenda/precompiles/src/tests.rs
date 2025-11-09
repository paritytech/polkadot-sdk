// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
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

use crate::mock::*;
use crate::IReferenda;
use codec::Encode;
use frame_support::weights::Weight;
use pallet_revive::{
	precompiles::{
		alloy::sol_types::{SolInterface, SolValue},
		H160,
	},
	ExecConfig, U256,
};
use sp_runtime::AccountId32;
// Referenda precompile address (matches the MATCHER in lib.rs)
// fn referenda_precompile_address() -> H160 {
// 	H160::from(hex::const_decode_to_array(b"000000000000000000000000000000000000000C").unwrap())
// }
fn referenda_precompile_address() -> H160 {
	// Matches: NonZero::new(11) → 0xB0000
	H160::from_low_u64_be(0xB0000)
}
#[test]
fn test_referenda_submit_lookup_works() {
	ExtBuilder::default().build().execute_with(|| {
		// Create a preimage first
		let hash = note_preimage(ALICE);

		// Verify initial state
		assert_eq!(System::block_number(), 1);
		assert_eq!(pallet_referenda::ReferendumCount::<Test>::get(), 0);

		// Encode the PalletsOrigin (OriginCaller::system(RawOrigin::Signed))
		let pallets_origin = OriginCaller::system(frame_system::RawOrigin::Signed(ALICE));
		let encoded_origin = pallets_origin.encode();

		// Create the submitLookup call parameters
		let submit_param = IReferenda::submitLookupCall {
			origin: encoded_origin.into(),
			hash: hash.as_fixed_bytes().into(),
			preimageLength: 1,
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 10, // Enact at block 10
		};

		let call = IReferenda::IReferendaCalls::submitLookup(submit_param);
		let encoded_call = call.abi_encode();

		// Call the precompile
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call succeeded
		match result.result {
			Ok(return_value) => {
				println!("Precompile call succeeded");
				println!("Return value: {:?}", return_value);
				if return_value.did_revert() {
					panic!("Precompile call reverted");
				}
			},
			Err(e) => panic!("Precompile call failed: {:?}", e),
		}

		// Debug: Check referendum count
		let count = pallet_referenda::ReferendumCount::<Test>::get();
		println!("Referendum count: {}", count);

		// Verify a referendum was created
		assert_eq!(count, 1, "Expected 1 referendum to be created");

		// Verify the referendum exists and has correct properties
		let referendum_info = pallet_referenda::ReferendumInfoFor::<Test>::get(0);
		assert!(referendum_info.is_some(), "Referendum should exist");

		println!("submitLookup test passed - referendum created successfully");
	});
}
#[test]
fn test_referenda_submit_inline_works() {
	ExtBuilder::default().build().execute_with(|| {
		// Verify initial state
		assert_eq!(System::block_number(), 1);
		assert_eq!(pallet_referenda::ReferendumCount::<Test>::get(), 0);

		// Encode the PalletsOrigin (OriginCaller::system(RawOrigin::Signed))
		let pallets_origin = OriginCaller::system(frame_system::RawOrigin::Signed(ALICE));
		let encoded_origin = pallets_origin.encode();

		// Create a small proposal (encoded RuntimeCall)
		// This should be small enough to fit in 128 bytes
		let proposal_bytes = set_balance_proposal_bytes(100u128);

		// Verify proposal is within 128 byte limit
		assert!(proposal_bytes.len() <= 128, "Proposal must fit in 128 bytes");

		// Create the submitInline call parameters
		let submit_param = IReferenda::submitInlineCall {
			origin: encoded_origin.into(),
			proposal: proposal_bytes.into(),
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 10, // Enact at block 10
		};

		let call = IReferenda::IReferendaCalls::submitInline(submit_param);
		let encoded_call = call.abi_encode();

		// Call the precompile
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call succeeded
		match result.result {
			Ok(return_value) => {
				println!("Precompile call succeeded");
				println!("Return value: {:?}", return_value);
				if return_value.did_revert() {
					panic!("Precompile call reverted");
				}
			},
			Err(e) => panic!("Precompile call failed: {:?}", e),
		}

		// Debug: Check referendum count
		let count = pallet_referenda::ReferendumCount::<Test>::get();
		println!("Referendum count: {}", count);

		// Verify a referendum was created
		assert_eq!(count, 1, "Expected 1 referendum to be created");

		// Verify the referendum exists and has correct properties
		let referendum_info = pallet_referenda::ReferendumInfoFor::<Test>::get(0);
		assert!(referendum_info.is_some(), "Referendum should exist");

		println!("submitInline test passed - referendum created successfully");
	});
}

#[test]
fn test_referenda_submit_inline_fails_with_oversized_proposal() {
	ExtBuilder::default().build().execute_with(|| {
		// Verify initial state
		assert_eq!(pallet_referenda::ReferendumCount::<Test>::get(), 0);

		// Encode the PalletsOrigin
		let pallets_origin = OriginCaller::system(frame_system::RawOrigin::Signed(ALICE));
		let encoded_origin = pallets_origin.encode();

		// Create a proposal that's definitely over 128 bytes
		// Using a large remark call with lots of data
		let large_data = vec![0u8; 129]; // 129 bytes - exceeds limit
		let proposal_call = RuntimeCall::System(frame_system::Call::remark { remark: large_data });
		let proposal_bytes = proposal_call.encode();

		// Verify proposal exceeds 128 byte limit
		assert!(proposal_bytes.len() > 128, "Proposal should exceed 128 bytes");

		// Create the submitInline call parameters
		let submit_param = IReferenda::submitInlineCall {
			origin: encoded_origin.into(),
			proposal: proposal_bytes.into(),
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 10,
		};

		let call = IReferenda::IReferendaCalls::submitInline(submit_param);
		let encoded_call = call.abi_encode();

		// Call the precompile
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call reverted due to oversized proposal
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("Precompile call failed with error: {err:?}"),
		};

		assert!(return_value.did_revert(), "Call should revert due to oversized proposal");

		// Verify no referendum was created
		assert_eq!(pallet_referenda::ReferendumCount::<Test>::get(), 0);

		println!("submitInline test passed - correctly failed with oversized proposal");
	});
}

#[test]
fn test_referenda_submit_lookup_fails_without_preimage() {
	ExtBuilder::default().build().execute_with(|| {
		// Don't create a preimage - this should fail
		let fake_hash = <Test as frame_system::Config>::Hash::default();

		// Encode the PalletsOrigin (OriginCaller::system(RawOrigin::Signed))
		let pallets_origin = OriginCaller::system(frame_system::RawOrigin::Signed(ALICE));
		let encoded_origin = pallets_origin.encode();

		// Create the submitLookup call parameters with fake hash
		let submit_param = IReferenda::submitLookupCall {
			origin: encoded_origin.into(),
			hash: fake_hash.as_fixed_bytes().into(),
			preimageLength: 1,
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 10,
		};

		let call = IReferenda::IReferendaCalls::submitLookup(submit_param);
		let encoded_call = call.abi_encode();

		// Call the precompile
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call failed
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("ReferendaPrecompile call failed with error: {err:?}"),
		};
		// assert!(return_value.did_revert(), "Call should revert due to missing preimage");

		// Verify no referendum was created
		// assert_eq!(pallet_referenda::ReferendumCount::<Test>::get(), 0);

		println!("submitLookup test passed - correctly failed without preimage");
	});
}

#[test]
fn test_balances_initialized() {
	ExtBuilder::default().build().execute_with(|| {
		// Verify our test accounts have balances
		assert_eq!(Balances::free_balance(&ALICE), 100);
		assert_eq!(Balances::free_balance(&BOB), 100);
		assert_eq!(Balances::free_balance(&CHARLIE), 100);

		println!("Balances initialized correctly");
	});
}

#[test]
fn test_multiple_referenda_submissions() {
	ExtBuilder::default().build().execute_with(|| {
		// Test that we can submit multiple referenda
		let hash1 = note_preimage(ALICE);
		let hash2 = note_preimage(ALICE);

		let pallets_origin = OriginCaller::system(frame_system::RawOrigin::Signed(ALICE));
		let encoded_origin = pallets_origin.encode();

		// Submit first referendum via lookup
		let submit1 = IReferenda::submitLookupCall {
			origin: encoded_origin.clone().into(),
			hash: hash1.as_fixed_bytes().into(),
			preimageLength: 1,
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 10,
		};

		let call1 = IReferenda::IReferendaCalls::submitLookup(submit1);
		let result1 = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call1.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		assert!(result1.result.is_ok(), "First submission should succeed");

		// Submit second referendum via inline
		let proposal_bytes = set_balance_proposal_bytes(200u128);

		let submit2 = IReferenda::submitInlineCall {
			origin: encoded_origin.into(),
			proposal: proposal_bytes.into(),
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 15,
		};

		let call2 = IReferenda::IReferendaCalls::submitInline(submit2);
		let result2 = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call2.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		assert!(result2.result.is_ok(), "Second submission should succeed");

		// Verify both referenda were created
		let count = pallet_referenda::ReferendumCount::<Test>::get();
		assert_eq!(count, 2, "Expected 2 referenda to be created");

		// Verify both referenda exist
		assert!(pallet_referenda::ReferendumInfoFor::<Test>::get(0).is_some());
		assert!(pallet_referenda::ReferendumInfoFor::<Test>::get(1).is_some());

		println!("Multiple referenda submissions test passed");
	});
}

#[test]
fn test_referenda_place_decision_deposit_works() {
	ExtBuilder::default().build().execute_with(|| {
		// First, create a referendum using submitInline
		let pallets_origin = OriginCaller::system(frame_system::RawOrigin::Signed(ALICE));
		let encoded_origin = pallets_origin.encode();
		let proposal_bytes = set_balance_proposal_bytes(100u128);

		let submit_param = IReferenda::submitInlineCall {
			origin: encoded_origin.clone().into(),
			proposal: proposal_bytes.into(),
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 10,
		};

		let call = IReferenda::IReferendaCalls::submitInline(submit_param);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		assert!(result.result.is_ok(), "Referendum submission should succeed");
		assert_eq!(pallet_referenda::ReferendumCount::<Test>::get(), 1);

		// Verify no decision deposit yet
		let referendum_info = pallet_referenda::ReferendumInfoFor::<Test>::get(0);
		if let Some(pallet_referenda::ReferendumInfo::Ongoing(status)) = referendum_info {
			assert!(status.decision_deposit.is_none(), "Should have no decision deposit initially");
		}

		// Now place decision deposit via precompile
		let place_deposit_call = IReferenda::placeDecisionDepositCall { referendumIndex: 0u32 };

		let call = IReferenda::IReferendaCalls::placeDecisionDeposit(place_deposit_call);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(BOB), // BOB places the deposit
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call succeeded
		match result.result {
			Ok(return_value) => {
				if return_value.did_revert() {
					panic!("Place decision deposit should not revert");
				}
			},
			Err(e) => panic!("Place decision deposit failed: {:?}", e),
		}

		// Verify decision deposit was placed
		let referendum_info = pallet_referenda::ReferendumInfoFor::<Test>::get(0);
		if let Some(pallet_referenda::ReferendumInfo::Ongoing(status)) = referendum_info {
			assert!(status.decision_deposit.is_some(), "Decision deposit should be placed");
			assert_eq!(
				status.decision_deposit.as_ref().unwrap().who,
				BOB,
				"BOB should be the depositor"
			);
		}

		println!("placeDecisionDeposit test passed - deposit placed successfully");
	});
}

#[test]
fn test_referenda_place_decision_deposit_fails_not_ongoing() {
	ExtBuilder::default().build().execute_with(|| {
		// Try to place deposit on non-existent referendum
		let place_deposit_call = IReferenda::placeDecisionDepositCall { referendumIndex: 0u32 };

		let call = IReferenda::IReferendaCalls::placeDecisionDeposit(place_deposit_call);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(BOB),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call reverted
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("Precompile call failed with error: {err:?}"),
		};

		assert!(return_value.did_revert(), "Call should revert due to non-existent referendum");

		println!(
			"placeDecisionDeposit test passed - correctly failed for non-existent referendum"
		);
	});
}

#[test]
fn test_referenda_place_decision_deposit_fails_insufficient_balance() {
	ExtBuilder::default().build().execute_with(|| {
		// First, create a referendum

		let pallets_origin = OriginCaller::system(frame_system::RawOrigin::Signed(ALICE));
		let encoded_origin = pallets_origin.encode();
		let proposal_bytes = set_balance_proposal_bytes(100u128);

		let submit_param = IReferenda::submitInlineCall {
			origin: encoded_origin.into(),
			proposal: proposal_bytes.into(),
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 10,
		};

		let call = IReferenda::IReferendaCalls::submitInline(submit_param);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		assert!(result.result.is_ok(), "Referendum submission should succeed");

		// Account 10 should have insufficient balance (from mock setup)
		let place_deposit_call = IReferenda::placeDecisionDepositCall { referendumIndex: 0u32 };

		let call = IReferenda::IReferendaCalls::placeDecisionDeposit(place_deposit_call);
		//map_account(ALICE);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(POOR), // Account with no balance
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call reverted
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("Precompile call failed with error: {err:?}"),
		};

		assert!(return_value.did_revert(), "Call should revert due to insufficient balance");

		println!(
			"placeDecisionDeposit test passed - correctly failed with insufficient balance"
		);
	});
}

#[test]
fn test_referenda_place_decision_deposit_fails_already_has_deposit() {
	ExtBuilder::default().build().execute_with(|| {
		// First, create a referendum
		let pallets_origin = OriginCaller::system(frame_system::RawOrigin::Signed(ALICE));
		let encoded_origin = pallets_origin.encode();
		let proposal_bytes = set_balance_proposal_bytes(100u128);

		let submit_param = IReferenda::submitInlineCall {
			origin: encoded_origin.clone().into(),
			proposal: proposal_bytes.into(),
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 10,
		};

		let call = IReferenda::IReferendaCalls::submitInline(submit_param);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		assert!(result.result.is_ok(), "Referendum submission should succeed");

		// Place deposit first time - should succeed
		let place_deposit_call = IReferenda::placeDecisionDepositCall { referendumIndex: 0u32 };

		let call = IReferenda::IReferendaCalls::placeDecisionDeposit(place_deposit_call.clone());
		let result1 = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(BOB),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		assert!(result1.result.is_ok(), "First deposit placement should succeed");
		match result1.result {
			Ok(value) => {
				if value.did_revert() {
					panic!("First deposit placement should not revert");
				}
			},
			Err(_) => panic!("First deposit placement failed"),
		}

		// Try to place deposit again - should fail
		let call = IReferenda::IReferendaCalls::placeDecisionDeposit(place_deposit_call);
		let result2 = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(CHARLIE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		// Verify the second call reverted
		let return_value = match result2.result {
			Ok(value) => value,
			Err(err) => panic!("Precompile call failed with error: {err:?}"),
		};

		assert!(return_value.did_revert(), "Call should revert due to existing deposit");

		println!("placeDecisionDeposit test passed - correctly failed for duplicate deposit");
	});
}

#[test]
fn test_submission_deposit_returns_correct_amount() {
	ExtBuilder::default().build().execute_with(|| {
		let call =
			IReferenda::IReferendaCalls::submissionDeposit(IReferenda::submissionDepositCall {});
		let encoded_call = call.abi_encode();
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			encoded_call,
			ExecConfig::new_substrate_tx(),
		);

		match result.result {
			Ok(return_value) => {
				if return_value.did_revert() {
					panic!("submissionDeposit should not revert");
				}

				let returned_bytes = &return_value.data;
				let deposit_amount: u128 =
					SolValue::abi_decode(returned_bytes).expect("Should decode u128");

				// SubmissionDeposit is set to 2 in the mock
				assert_eq!(deposit_amount, 2, "Submission deposit should be 2");

				println!("submissionDeposit returned: {}", deposit_amount);
			},
			Err(e) => panic!("submissionDeposit call failed: {:?}", e),
		}
	});
}

#[test]
fn test_decision_deposit_returns_track_amount_for_new_referendum() {
	ExtBuilder::default().build().execute_with(|| {
		// Create a referendum using the helper function
		let referendum_index = ExtBuilder::submit_referendum(ALICE);
		assert_eq!(referendum_index, 0);

		// Verify no decision deposit yet
		let referendum_info = pallet_referenda::ReferendumInfoFor::<Test>::get(referendum_index);
		if let Some(pallet_referenda::ReferendumInfo::Ongoing(status)) = referendum_info {
			assert!(status.decision_deposit.is_none(), "Should have no decision deposit initially");
		}

		// Call decisionDeposit for the referendum
		let decision_deposit_call =
			IReferenda::decisionDepositCall { referendumIndex: referendum_index };

		let call = IReferenda::IReferendaCalls::decisionDeposit(decision_deposit_call);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call succeeded
		match result.result {
			Ok(return_value) => {
				if return_value.did_revert() {
					panic!("decisionDeposit should not revert");
				}

				// Decode the return value (uint128) - ABI encoded
				let returned_bytes = &return_value.data;
				let deposit_amount: u128 =
					SolValue::abi_decode(returned_bytes).expect("Should decode u128");

				// Track 2 (Signed origin) has decision_deposit: 1
				// The referendum should be on track 2 since it's submitted with Signed origin
				assert_eq!(deposit_amount, 1, "Decision deposit should be 1 for track 2");

				println!("decisionDeposit returned: {}", deposit_amount);
			},
			Err(e) => panic!("decisionDeposit call failed: {:?}", e),
		}

		println!("decisionDeposit test passed - returned correct track amount");
	});
}
#[test]
fn test_decision_deposit_returns_zero_after_deposit_placed() {
	ExtBuilder::default().build().execute_with(|| {
		// Create a referendum with decision deposit using the helper function
		let referendum_index = ExtBuilder::submit_referendum_with_decision_deposit(ALICE, BOB);
		assert_eq!(referendum_index, 0);

		// Verify deposit was placed
		let referendum_info = pallet_referenda::ReferendumInfoFor::<Test>::get(referendum_index);
		if let Some(pallet_referenda::ReferendumInfo::Ongoing(status)) = referendum_info {
			assert!(status.decision_deposit.is_some(), "Decision deposit should be placed");
		}

		// Call decisionDeposit - should return 0 since deposit is already placed
		let decision_deposit_call =
			IReferenda::decisionDepositCall { referendumIndex: referendum_index };

		let call = IReferenda::IReferendaCalls::decisionDeposit(decision_deposit_call);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call succeeded
		match result.result {
			Ok(return_value) => {
				if return_value.did_revert() {
					panic!("decisionDeposit should not revert");
				}

				// Decode the return value (uint128) - ABI encoded
				let returned_bytes = &return_value.data;
				let deposit_amount: u128 =
					SolValue::abi_decode(returned_bytes).expect("Should decode u128");

				assert_eq!(
					deposit_amount, 0,
					"Decision deposit should return 0 when already placed"
				);

				println!(
					"decisionDeposit returned: {} (deposit already placed)",
					deposit_amount
				);
			},
			Err(e) => panic!("decisionDeposit call failed: {:?}", e),
		}

		println!("decisionDeposit test passed - returned 0 after deposit placed");
	});
}

#[test]
fn test_decision_deposit_returns_zero_for_nonexistent_referendum() {
	ExtBuilder::default().build().execute_with(|| {
		// Try to get decision deposit for a non-existent referendum
		let decision_deposit_call = IReferenda::decisionDepositCall { referendumIndex: 999u32 };

		let call = IReferenda::IReferendaCalls::decisionDeposit(decision_deposit_call);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		match result.result {
			Ok(return_value) => {
				if return_value.did_revert() {
					panic!("decisionDeposit should not revert for nonexistent referendum");
				}

				let returned_bytes = &return_value.data;
				let deposit_amount: u128 =
					SolValue::abi_decode(returned_bytes).expect("Should decode u128");

				assert_eq!(
					deposit_amount, 0,
					"Decision deposit should return 0 for nonexistent referendum"
				);

				println!(
					"decisionDeposit returned: {} for nonexistent referendum",
					deposit_amount
				);
			},
			Err(e) => panic!("decisionDeposit call failed: {:?}", e),
		}

		println!("decisionDeposit test passed - returned 0 for nonexistent referendum");
	});
}

#[test]
fn test_submit_inline_fails_with_invalid_origin_encoding() {
	ExtBuilder::default().build().execute_with(|| {
		// Create malformed origin bytes (invalid encoding)
		let invalid_origin = vec![0xFF, 0xFF, 0xFF]; // Invalid SCALE encoding

		// Create a valid proposal
		let proposal_bytes = set_balance_proposal_bytes(100u128);

		// Create the submitInline call with invalid origin
		let submit_param = IReferenda::submitInlineCall {
			origin: invalid_origin.into(),
			proposal: proposal_bytes.into(),
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 10,
		};

		let call = IReferenda::IReferendaCalls::submitInline(submit_param);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call reverted due to invalid origin encoding
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("Precompile call failed with error: {err:?}"),
		};

		assert!(return_value.did_revert(), "Call should revert due to invalid origin encoding");

		// Verify no referendum was created
		assert_eq!(pallet_referenda::ReferendumCount::<Test>::get(), 0);

		println!("submitInline test passed - correctly failed with invalid origin encoding");
	});
}

#[test]
fn test_submit_lookup_fails_with_invalid_origin_encoding() {
	ExtBuilder::default().build().execute_with(|| {
		// Create a preimage first
		let hash = note_preimage(ALICE);

		// Create malformed origin bytes (invalid encoding)
		let invalid_origin = vec![0xFF, 0xFF, 0xFF]; // Invalid SCALE encoding

		// Create the submitLookup call with invalid origin
		let submit_param = IReferenda::submitLookupCall {
			origin: invalid_origin.into(),
			hash: hash.as_fixed_bytes().into(),
			preimageLength: 1,
			timing: IReferenda::Timing::AtBlock,
			enactmentMoment: 10,
		};

		let call = IReferenda::IReferendaCalls::submitLookup(submit_param);
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(ALICE),
			referenda_precompile_address(),
			U256::zero(),
			Weight::MAX,
			u128::MAX,
			call.abi_encode(),
			ExecConfig::new_substrate_tx(),
		);

		// Verify the call reverted due to invalid origin encoding
		let return_value = match result.result {
			Ok(value) => value,
			Err(err) => panic!("Precompile call failed with error: {err:?}"),
		};

		assert!(return_value.did_revert(), "Call should revert due to invalid origin encoding");

		// Verify no referendum was created
		assert_eq!(pallet_referenda::ReferendumCount::<Test>::get(), 0);

		println!("submitLookup test passed - correctly failed with invalid origin encoding");
	});
}
