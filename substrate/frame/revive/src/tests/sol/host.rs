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
    address::AddressMapper,
};

use alloy_core::{primitives::U256, primitives::I256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Host, FixtureType};
use pretty_assertions::assert_eq;
use frame_support::traits::fungible::Inspect;
use frame_support::traits::Get;

#[test]
fn balance_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc] {

        let existential_deposit_planck = <Test as pallet_balances::Config>::ExistentialDeposit::get() as u128;
        let native_to_eth = <<Test as Config>::NativeToEthRatio as Get<u32>>::get() as u128;
        let expected_balance = U256::from((100_000_000_000u128 - existential_deposit_planck) * native_to_eth);
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {

            <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
            <Test as Config>::Currency::set_balance(&BOB, 100_000_000_000);

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

                assert_eq!(
                    expected_balance,
                    result_balance,
                    "BALANCE should return BOB's balance for {:?}", fixture_type
                );
            }
		});
        
	}
}

#[test]
fn selfbalance_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
        let existential_deposit_planck = <Test as pallet_balances::Config>::ExistentialDeposit::get() as u128;
        let native_to_eth = <<Test as Config>::NativeToEthRatio as Get<u32>>::get() as u128;
        let expected_balance = U256::from((100_000_000_000u128 - existential_deposit_planck) * native_to_eth);
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {

            <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
            

            {
                let account_id32 = <Test as Config>::AddressMapper::to_account_id(&addr);

                let balance = <Test as Config>::Currency::balance(&account_id32);
                println!("selfbalance_works: balance = {:?}", balance);

                <Test as Config>::Currency::set_balance(&account_id32, 100_000_000_000);

                let balance = <Test as Config>::Currency::balance(&account_id32);
                println!("selfbalance_works: balance = {:?}", balance);
            }
            {
                let result = builder::bare_call(addr)
                    .data(
                        Host::HostCalls::selfbalance(Host::selfbalanceCall { })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                let result_balance = U256::from_be_bytes::<32>(result.data.try_into().unwrap());

                assert_eq!(
                    expected_balance,
                    result_balance,
                    "BALANCE should return contract's balance for {:?}", fixture_type
                );
            }
		});
    }
}