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
use pallet_revive_fixtures::{compile_module_with_type, Arithmetic, FixtureType};
use pretty_assertions::assert_eq;

#[test]
fn add_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {            
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::add(Arithmetic::addCall { a: U256::from(20u32), b: U256::from(22u32) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(42u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "ADD(20, 22) should equal 42 for {:?}", fixture_type
                );
            }

            {
                // Test large numbers but not MAX overflow
                let large_a = U256::from(u64::MAX);
                let large_b = U256::from(1000u32);
                let expected = large_a + large_b;
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::add(Arithmetic::addCall { a: large_a, b: large_b })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    expected,
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "ADD({}, {}) should equal {} for {:?}", large_a, large_b, expected, fixture_type
                );
            }
		});
	}
}

#[test]
fn mul_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::mul(Arithmetic::mulCall { a: U256::from(20u32), b: U256::from(22u32) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(440u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "MUL(20, 22) should equal 440 for {:?}", fixture_type
                );
            }

            {
                // Test large numbers but not MAX overflow
                let large_a = U256::from(u64::MAX);
                let large_b = U256::from(1000u32);
                let expected = large_a * large_b;
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::mul(Arithmetic::mulCall { a: large_a, b: large_b })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    expected,
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "MUL({}, {}) should equal {} for {:?}", large_a, large_b, expected, fixture_type
                );
            }
		});
	}
}

#[test]
fn sub_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::sub(Arithmetic::subCall { a: U256::from(20u32), b: U256::from(18u32) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(2u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "SUB(20, 18) should equal 2 for {:?}", fixture_type
                );
            }

            {
                // Test large numbers but not MAX overflow
                let large_a = U256::from(u64::MAX);
                let large_b = U256::from(1000u32);
                let expected = large_a - large_b;
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::sub(Arithmetic::subCall { a: large_a, b: large_b })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    expected,
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "SUB({}, {}) should equal {} for {:?}", large_a, large_b, expected, fixture_type
                );
            }
		});
	}
}

#[test]
fn div_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::div(Arithmetic::divCall { a: U256::from(20u32), b: U256::from(5u32) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(4u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "DIV(20, 5) should equal 4 for {:?}", fixture_type
                );
            }

            {
                // Test large numbers but not MAX overflow
                let large_a = U256::from(u64::MAX);
                let large_b = U256::from(1000u32);
                let expected = large_a / large_b;
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::div(Arithmetic::divCall { a: large_a, b: large_b })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    expected,
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "DIV({}, {}) should equal {} for {:?}", large_a, large_b, expected, fixture_type
                );
            }
		});
	}
}

#[test]
fn sdiv_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::sdiv(Arithmetic::sdivCall { a: I256::from_raw(U256::from(20u32)), b: I256::from_raw(U256::from(5u32)) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    I256::from_raw(U256::from(4u32)),
                    I256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "SDIV(20, 5) should equal 4 for {:?}", fixture_type
                );
            }

            {
                // Test large numbers but not MAX overflow
                let large_a = I256::from_raw(U256::from(i64::MAX as u64));
                let large_b = -I256::from_raw(U256::from(1000u32));
                let expected = large_a / large_b;
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::sdiv(Arithmetic::sdivCall { a: large_a, b: large_b })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    expected,
                    I256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "SDIV({}, {}) should equal {} for {:?}", large_a, large_b, expected, fixture_type
                );
            }
		});
	}
}

#[test]
fn rem_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::rem(Arithmetic::remCall { a: U256::from(20u32), b: U256::from(5u32) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(0u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "REM(20, 5) should equal 0 for {:?}", fixture_type
                );
            }

            {
                // Test with remainder: 23 % 5 = 3
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::rem(Arithmetic::remCall { a: U256::from(23u32), b: U256::from(5u32) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(3u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "REM(23, 5) should equal 3 for {:?}", fixture_type
                );
            }

            {
                // Test large numbers with positive divisor
                let large_a = U256::from(i64::MAX as u64);
                let large_b = U256::from(1000u32);
                let expected = large_a % large_b;
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::rem(Arithmetic::remCall { a: large_a, b: large_b })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    expected,
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "REM({}, {}) should equal {} for {:?}", large_a, large_b, expected, fixture_type
                );
            }
		});
	}
}

#[test]
fn smod_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::smod(Arithmetic::smodCall { a: I256::from_raw(U256::from(20u32)), b: I256::from_raw(U256::from(5u32)) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    I256::from_raw(U256::from(0u32)),
                    I256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "SMOD(20, 5) should equal 0 for {:?}", fixture_type
                );
            }

            {
                // Test with remainder: 23 % 5 = 3
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::smod(Arithmetic::smodCall { a: I256::from_raw(U256::from(23u32)), b: I256::from_raw(U256::from(5u32)) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    I256::from_raw(U256::from(3u32)),
                    I256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "SMOD(23, 5) should equal 3 for {:?}", fixture_type
                );
            }

            {
                // Test large numbers with positive divisor
                let large_a = I256::from_raw(U256::from(i64::MAX as u64));
                let large_b = I256::from_raw(U256::from(1000u32));
                let expected = large_a % large_b;
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::smod(Arithmetic::smodCall { a: large_a, b: large_b })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    expected,
                    I256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "SMOD({}, {}) should equal {} for {:?}", large_a, large_b, expected, fixture_type
                );
            }

            {
                // Test negative numbers: -23 % 5 should equal -3 in most implementations
                // We need to use two's complement representation for negative numbers
                let neg_23 = I256::from_raw(U256::MAX - U256::from(22u32)); // -23 in two's complement
                let pos_5 = I256::from_raw(U256::from(5u32));
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::smod(Arithmetic::smodCall { a: neg_23, b: pos_5 })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                let neg_3 = I256::from_raw(U256::MAX - U256::from(2u32)); // -3 in two's complement
                assert_eq!(
                    neg_3,
                    I256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "REM(-23, 5) should equal -3 for {:?}", fixture_type
                );
            }
		});
	}
}

#[test]
fn umod_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::umod(Arithmetic::umodCall { a: U256::from(23u32), b: U256::from(5u32) })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(3u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "UMOD(23, 5) should equal 3 for {:?}", fixture_type
                );
            }
		});
	}
}

#[test]
fn addmod_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                // Test ADDMOD: (10 + 15) % 7 = 25 % 7 = 4
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::addmod(Arithmetic::addmodCall { 
                            a: U256::from(10u32), 
                            b: U256::from(15u32), 
                            n: U256::from(7u32) 
                        })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(4u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "ADDMOD(10, 15, 7) should equal 4 for {:?}", fixture_type
                );
            }
		});
	}
}

#[test]
fn mulmod_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                // Test MULMOD: (6 * 7) % 10 = 42 % 10 = 2
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::mulmod(Arithmetic::mulmodCall { 
                            a: U256::from(6u32), 
                            b: U256::from(7u32), 
                            n: U256::from(10u32) 
                        })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(2u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "MULMOD(6, 7, 10) should equal 2 for {:?}", fixture_type
                );
            }
		});
	}
}

#[test]
fn exp_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {
                // Test EXP: 2 ** 3 = 8
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::exp(Arithmetic::expCall { 
                            a: U256::from(2u32), 
                            b: U256::from(3u32)
                        })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(8u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "EXP(2, 3) should equal 8 for {:?}", fixture_type
                );
            }

            {
                // Test EXP: 5 ** 2 = 25
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::exp(Arithmetic::expCall { 
                            a: U256::from(5u32), 
                            b: U256::from(2u32)
                        })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(25u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "EXP(5, 2) should equal 25 for {:?}", fixture_type
                );
            }

            {
                // Test EXP: 10 ** 0 = 1 (anything to power 0 is 1)
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::exp(Arithmetic::expCall { 
                            a: U256::from(10u32), 
                            b: U256::from(0u32)
                        })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(1u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "EXP(10, 0) should equal 1 for {:?}", fixture_type
                );
            }

            {
                // Test EXP: 1 ** 100 = 1 (1 to any power is 1)
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::exp(Arithmetic::expCall { 
                            a: U256::from(1u32), 
                            b: U256::from(100u32)
                        })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(1u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "EXP(1, 100) should equal 1 for {:?}", fixture_type
                );
            }

            {
                // Test EXP with larger numbers: 3 ** 4 = 81
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::exp(Arithmetic::expCall { 
                            a: U256::from(3u32), 
                            b: U256::from(4u32)
                        })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                assert_eq!(
                    U256::from(81u32),
                    U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                    "EXP(3, 4) should equal 81 for {:?}", fixture_type
                );
            }
		});
	}
}