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

//! The pallet-revive shared VM integration test suite.

use crate::{
	test_utils::{builder::Contract, ALICE, ALICE_ADDR, BOB, BOB_ADDR},
	tests::{builder, ExtBuilder, Test},
	Code, Config,
};

use alloy_core::{primitives::U256, primitives::I256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Host, FixtureType};
use pretty_assertions::assert_eq;
use frame_support::traits::fungible::Inspect;

#[test]
fn balance_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {

            let bobs_address20: [u8; 20] = {
                let bob_account32 = {
                    let mut data = [0u8; 32];
                    data[12..].copy_from_slice(BOB_ADDR.as_bytes());
                    sp_runtime::AccountId32::from(data)
                };
                let bobs_address32: [u8; 32] = <sp_runtime::AccountId32 as AsRef<[u8; 32]>>::as_ref(&bob_account32).clone();
                bobs_address32[12..].try_into().unwrap()
            };

            {
                <Test as Config>::Currency::set_balance(&BOB, 100_000_000_000);
                let balance = <Test as Config>::Currency::balance(&BOB);
                println!("BOB's balance: {:?}", balance);
            }
            {
                <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
                let balance = <Test as Config>::Currency::balance(&ALICE);
                println!("ALICE's balance: {:?}", balance);
            }

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                let result = builder::bare_call(addr)
                    .data(
                        Host::HostCalls::balance(Host::balanceCall { account: BOB_ADDR.0.into() })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                let result_balance = U256::from_be_bytes::<32>(result.data.try_into().unwrap());
                {
                    let balance = <Test as Config>::Currency::balance(&BOB);
                    println!("BOB's balance: {:?}", balance);
                    println!("Result balance: {:?}", result_balance);
                }

                assert_eq!(
                    U256::from((100_000_000_000u128-1u128)*1000_000u128),
                    result_balance,
                    "BALANCE should return BOB's balance for {:?}", fixture_type
                );
            }
		});
        
	}
}