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
	test_utils::{builder::Contract, ALICE},
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
	for fixture_type in [FixtureType::Resolc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
            let alice_evm: [u8; 20] = <sp_runtime::AccountId32 as AsRef<[u8]>>::as_ref(&ALICE)[12..32].try_into().unwrap();

            let evm_account = {
                let mut data: [u8; 32] = <sp_runtime::AccountId32 as AsRef<[u8]>>::as_ref(&ALICE)[0..32].try_into().unwrap();
data[..12].copy_from_slice(&[0u8; 12]);
                sp_runtime::AccountId32::from(data)
            };
            println!("ALICE: {ALICE:?}");
            println!("alice_evm: {alice_evm:?}");
            println!("evm_account: {evm_account:?}");

            {
                <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
                let balance = <Test as Config>::Currency::balance(&ALICE);
                println!("ALICE's balance: {:?}", balance);
            }

            <Test as Config>::Currency::set_balance(&evm_account, 100_000_000_000);
            let balance = <Test as Config>::Currency::balance(&evm_account);
            println!("evm_account's balance: {:?}", balance);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
            
            println!("Contract address: {:?}", addr);

			// Call the balance function on the Host contract, passing evm_account's address
            let result = builder::bare_call(addr)
                .data(
                    Host::HostCalls::balance(Host::balanceCall { account: <sp_runtime::AccountId32 as AsRef<[u8]>>::as_ref(&evm_account)[12..32].try_into().unwrap() })
                        .abi_encode(),
                )
                .build_and_unwrap_result();
            let balance = <Test as Config>::Currency::balance(&evm_account);
            println!("evm_account's balance: {:?}", balance);
            println!("Result data: {:?}", result.data);

			// The contract should return evm_account's balance as a U256
			assert_eq!(
				U256::from(100_000_000_000u128),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				"BALANCE should return evm_account's balance for {:?}", fixture_type
			);
		});
        break;
	}
}