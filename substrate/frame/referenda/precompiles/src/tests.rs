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
use pallet_revive::{
	precompiles::{
		alloy::sol_types::SolInterface,
		H160,
	},
	ExecConfig, U256,
};
use frame_support::weights::Weight;
use codec::Encode;
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
				println!("✅ Precompile call succeeded");
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
		
		println!("✅ submitLookup test passed - referendum created successfully");
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
				println!("✅ Precompile call succeeded");
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
		
		println!("✅ submitInline test passed - referendum created successfully");
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
		let proposal_call = RuntimeCall::System(frame_system::Call::remark {
			remark: large_data,
		});
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
		
		println!("✅ submitInline test passed - correctly failed with oversized proposal");
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
		assert!(return_value.did_revert(), "Call should revert due to missing preimage");
		
		// Verify no referendum was created
		assert_eq!(pallet_referenda::ReferendumCount::<Test>::get(), 0);
		
		println!("✅ submitLookup test passed - correctly failed without preimage");
	});
}


#[test]
fn test_balances_initialized() {
	ExtBuilder::default().build().execute_with(|| {
		// Verify our test accounts have balances
		assert_eq!(Balances::free_balance(&ALICE), 100);
		assert_eq!(Balances::free_balance(&BOB), 100);
		assert_eq!(Balances::free_balance(&CHARLIE), 100);
		
		println!("✅ Balances initialized correctly");
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
		
		println!("✅ Multiple referenda submissions test passed");
	});
}