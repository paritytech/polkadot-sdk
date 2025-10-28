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
		alloy::{
			hex,
			sol_types::SolInterface,
		},
		H160,
	},
	ExecConfig, U256,
};
use frame_support::weights::Weight;
use codec::Encode;
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

// TODO: Add more tests:
// - Test submitLookup precompile call
// - Test origin encoding/decoding
// - Test referendum creation
// - Test error handling