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
	test_utils::{builder::Contract, ALICE, BOB, BOB_ADDR},
	tests::{builder, ExtBuilder, Test, test_utils},
	Code, Config,
    address::AddressMapper,
    PristineCode,
    H256,
    Key,
};

use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Host, FixtureType};
use pretty_assertions::assert_eq;
use frame_support::traits::fungible::Inspect;
use frame_support::traits::Get;

fn convert_to_free_balance(total_balance: u128) -> U256 {
    let existential_deposit_planck = <Test as pallet_balances::Config>::ExistentialDeposit::get() as u128;
    let native_to_eth = <<Test as Config>::NativeToEthRatio as Get<u32>>::get() as u128;
    U256::from((100_000_000_000u128 - existential_deposit_planck) * native_to_eth)
}

#[test]
fn balance_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {

        let expected_balance = convert_to_free_balance(100_000_000_000);
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
        let expected_balance = convert_to_free_balance(100_000_000_000);
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {

            <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
            

            {
                let account_id32 = <Test as Config>::AddressMapper::to_account_id(&addr);

                <Test as Config>::Currency::set_balance(&account_id32, 100_000_000_000);
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

#[test]
fn extcodesize_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();
        

		ExtBuilder::default().build().execute_with(|| {

            <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
            
            let expected_code_size = {
                let contract_info = test_utils::get_contract(&addr);
                let code_hash = contract_info.code_hash;
                U256::from(test_utils::ensure_stored(code_hash))
            };

            {
                let result = builder::bare_call(addr)
                    .data(
                        Host::HostCalls::extcodesize(Host::extcodesizeCall { account: addr.0.into() })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                
                let result_size = U256::from_be_bytes::<32>(result.data.try_into().unwrap());

                assert_eq!(
                    expected_code_size,
                    result_size,
                    "EXTCODESIZE should return the code size for {:?}", fixture_type
                );
            }
		});
    }
}

#[test]
fn extcodehash_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {

            <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
            
            let expected_code_hash = {
                let contract_info = test_utils::get_contract(&addr);
                contract_info.code_hash
            };

            {
                let result = builder::bare_call(addr)
                    .data(
                        Host::HostCalls::extcodehash(Host::extcodehashCall { account: addr.0.into() })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                
                let result_hash = U256::from_be_bytes::<32>(result.data.try_into().unwrap());
                let result_hash = H256::from(result_hash.to_be_bytes());

                assert_eq!(
                    expected_code_hash,
                    result_hash,
                    "EXTCODEHASH should return the code hash for {:?}", fixture_type
                );
            }
		});
    }
}

#[test]
#[ignore]
fn extcodecopy_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (_code, _) = compile_module_with_type("Host", fixture_type).unwrap();
        todo!("implement this test");
    }
}

// TODO: this test is giving out-of-gas errors. investigate why.
#[test]
#[ignore]
fn blockhash_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();
        
        ExtBuilder::default().build().execute_with(|| {

            <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
            println!("addr: {:?}", addr);
            
            let expected_code_hash = {
                let contract_info = test_utils::get_contract(&addr);
                contract_info.code_hash
            };
            

            {
                let account_id32 = <Test as Config>::AddressMapper::to_account_id(&addr);

                <Test as Config>::Currency::set_balance(&account_id32, 100_000_000_000_000);
            }

            {
                let block_hash = [1; 32];
                frame_system::BlockHash::<Test>::insert(
                    &crate::BlockNumberFor::<Test>::from(0u32),
                    <Test as frame_system::Config>::Hash::from(&block_hash),
                );
                let current_block = frame_system::Pallet::<Test>::block_number();
                println!("current_block: {:?}", current_block);
                let result = builder::bare_call(addr)
                    .data(
                        Host::HostCalls::blockhash(Host::blockhashCall { blockNumber: U256::from(0u128) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                println!("result: {:?}", result);
                
                let result_hash = U256::from_be_bytes::<32>(result.data.try_into().unwrap());
                let result_hash = H256::from(result_hash.to_be_bytes());

                println!("Result code hash: {:?}", result_hash);

                assert_eq!(
                    expected_code_hash,
                    result_hash,
                    "EXTCODEHASH should return the code hash for {:?}", fixture_type
                );
            }
		});
        break;
    }
}

#[test]
fn sload_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Host", fixture_type).unwrap();

        let index = U256::from(13);
        let expected_value = U256::from(17);

		ExtBuilder::default()
        .build().execute_with(|| {

            <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

        {
            let contract_info = test_utils::get_contract(&addr);
            let key = Key::Fix(index.to_be_bytes());
            contract_info.write(
                &key,
                Some(expected_value.to_be_bytes::<32>().to_vec()),
                None,
                false
            );
        }

            {
                let result = builder::bare_call(addr)
                    .data(
                        Host::HostCalls::sload(Host::sloadCall { slot: index })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                let result = U256::from_be_bytes::<32>(result.data.try_into().unwrap());

                assert_eq!(
                    expected_value,
                    result,
                    "result should return expected value {:?}", fixture_type
                );
            }
		});
        break;
    }
}

#[test]
fn sstore_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (_code, _) = compile_module_with_type("Host", fixture_type).unwrap();
        todo!("implement this test");
    }
}

#[test]
fn tstore_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (_code, _) = compile_module_with_type("Host", fixture_type).unwrap();
        todo!("implement this test");
    }
}

#[test]
fn tload_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (_code, _) = compile_module_with_type("Host", fixture_type).unwrap();
        todo!("implement this test");
    }
}

#[test]
fn log_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (_code, _) = compile_module_with_type("Host", fixture_type).unwrap();
        todo!("implement this test");
    }
}

#[test]
fn selfdestruct_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (_code, _) = compile_module_with_type("Host", fixture_type).unwrap();
        todo!("implement this test");
    }
}
