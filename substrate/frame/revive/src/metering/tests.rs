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

use crate::{
	test_utils::{builder::Contract, ALICE, ALICE_ADDR},
	tests::{builder, ExtBuilder, Test},
	CallResources, Code, Config, EthTxInfo, StorageDeposit, TransactionLimits, TransactionMeter,
	WeightToken,
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{
	compile_module_with_type, CatchConstructorTest, DepositPrecompile, FixtureType,
};
use sp_runtime::{FixedU128, Weight};
use test_case::test_case;

/// A trivial token that charges the specified number of weight units.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
struct TestToken(u64, u64);
impl WeightToken<Test> for TestToken {
	fn weight(&self) -> Weight {
		Weight::from_parts(self.0, self.1)
	}
}

enum Charge {
	W(u64, u64),
	D(i64),
}

#[test_case(FixtureType::Solc   , "DepositPrecompile" ; "solc precompiles")]
#[test_case(FixtureType::Resolc , "DepositPrecompile" ; "resolc precompiles")]
#[test_case(FixtureType::Solc   , "DepositDirect" ; "solc direct")]
#[test_case(FixtureType::Resolc , "DepositDirect" ; "resolc direct")]
fn max_consumed_deposit_integration(fixture_type: FixtureType, fixture_name: &str) {
	let (code, _) = compile_module_with_type(fixture_name, fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_addr)
			.data(DepositPrecompile::callSetAndClearCall {}.abi_encode())
			.build();

		assert_eq!(result.storage_deposit, StorageDeposit::Charge(66));
		assert_eq!(result.max_storage_deposit, StorageDeposit::Charge(132));
	});
}

#[ignore = "TODO: Does not work yet, see https://github.com/paritytech/contract-issues/issues/213"]
#[test_case(FixtureType::Solc   , "DepositPrecompile" ; "solc precompiles")]
#[test_case(FixtureType::Resolc , "DepositPrecompile" ; "resolc precompiles")]
#[test_case(FixtureType::Solc   , "DepositDirect" ; "solc direct")]
#[test_case(FixtureType::Resolc , "DepositDirect" ; "resolc direct")]
fn max_consumed_deposit_integration_refunds_subframes(
	fixture_type: FixtureType,
	fixture_name: &str,
) {
	let (code, _) = compile_module_with_type(fixture_name, fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_addr)
			.data(DepositPrecompile::setAndClearCall {}.abi_encode())
			.build();

		assert_eq!(result.storage_deposit, StorageDeposit::Charge(66));
		assert_eq!(result.max_storage_deposit, StorageDeposit::Charge(132));

		builder::bare_call(caller_addr)
			.data(DepositPrecompile::clearAllCall {}.abi_encode())
			.build();

		let result = builder::bare_call(caller_addr)
			.data(DepositPrecompile::setAndCallClearCall {}.abi_encode())
			.build();

		assert_eq!(result.storage_deposit, StorageDeposit::Charge(66));
		assert_eq!(result.max_storage_deposit, StorageDeposit::Charge(132));
	});
}

#[test]
fn substrate_metering_initialization_works() {
	let gas_scale = <Test as Config>::GasScale::get();

	let tests = vec![
		(5_000_000_000, 1_000_000_000, 2_000, Some((2999999500, 1499999750, 11107, 599999900))),
		(6_000_000_000, 1_000_000_000, 2_000, Some((3999999500, 1999999750, 13728, 799999900))),
		(6_000_000_000, 1_000_000_000, 10_000, Some((2185302235, 1999999750, 5728, 437060447))),
		(2_000_000_000, 1_000_000_000, 2_000, None),
		(4_000_000_000, 100_000_000, 2_000, Some((3237060047, 1899999750, 8485, 647412009))),
		(5_000_000_000, 1_000_000_000, 8_000, Some((1948241688, 1499999750, 5107, 389648337))),
		(10_000_000_000, 1_000_000_000, 8_000, Some((6948241688, 3999999750, 18214, 1389648337))),
		(3_052_000_000, 1_000_000_000, 8_000, Some((241688, 525999750, 0, 48337))),
		(3_051_000_000, 1_000_000_000, 8_000, None),
	];

	for (eth_gas_limit, extra_ref_time, extra_proof, remaining) in tests {
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				let eth_tx_info =
					EthTxInfo::<Test>::new(100, Weight::from_parts(extra_ref_time, extra_proof));
				let transaction_meter =
					TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
						eth_gas_limit: eth_gas_limit / gas_scale,
						maybe_weight_limit: None,
						eth_tx_info,
					});

				if let Some((gas_left, ref_time_left, proof_size_left, deposit_left)) = remaining {
					let transaction_meter = transaction_meter.unwrap();
					assert_eq!(gas_left / gas_scale, transaction_meter.eth_gas_left().unwrap());
					assert_eq!(
						Weight::from_parts(ref_time_left, proof_size_left),
						transaction_meter.weight_left().unwrap()
					);
					assert_eq!(deposit_left, transaction_meter.deposit_left().unwrap());
				} else {
					assert!(transaction_meter.is_err());
				}
			});
	}

	let tests = vec![
		((1_000_000_000, 2_000), (1_000_000_000, 2_000)),
		((2_000_000_000, 2_000), (1_499_999_750, 2_000)),
		((2_000_000_000, 20_000), (1_499_999_750, 11_107)),
		((1_000_000_000, 20_000), (1_000_000_000, 11_107)),
	];

	for ((ref_time_limit, proof_size_limit), (ref_time_left, proof_size_left)) in tests {
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				let eth_tx_info =
					EthTxInfo::<Test>::new(100, Weight::from_parts(1_000_000_000, 2_000));
				let transaction_meter =
					TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
						eth_gas_limit: 5_000_000_000 / gas_scale,
						maybe_weight_limit: Some(Weight::from_parts(
							ref_time_limit,
							proof_size_limit,
						)),
						eth_tx_info,
					})
					.unwrap();

				assert_eq!(
					Weight::from_parts(ref_time_left, proof_size_left),
					transaction_meter.weight_left().unwrap()
				);
			});
	}
}

#[test]
fn substrate_metering_charges_works() {
	use Charge::{D, W};

	let gas_scale = <Test as Config>::GasScale::get();
	let tests = vec![
		(
			(5_000_000_000, 1_000_000_000, 2_000),
			vec![(W(1000, 100), Some((2999997500, 1499998750, 11007, 599999500, 2000002500u64)))],
		),
		(
			(5_000_000_000, 1_000_000_000, 2_000),
			vec![(W(1000, 300), Some((2999997500, 1499998750, 10807, 599999500, 2000002500)))],
		),
		(
			(5_000_000_000, 1_000_000_000, 2_000),
			vec![(W(1300000000, 10000), Some((399999500, 199999750, 1107, 79999900, 4600000500)))],
		),
		(
			(5_000_000_000, 1_000_000_000, 2_000),
			vec![(W(1400000000, 10000), Some((199999500, 99999750, 1107, 39999900, 4800000500)))],
		),
		(
			(5_000_000_000, 1_000_000_000, 2_000),
			vec![(W(1400000000, 11000), Some((40893055, 99999750, 107, 8178611, 4959106945)))],
		),
		((5_000_000_000, 1_000_000_000, 2_000), vec![(W(1400000000, 12000), None)]),
		((5_000_000_000, 1_000_000_000, 2_000), vec![(W(1500000000, 11000), None)]),
		(
			(5_000_000_000, 1_000_000_000, 2_000),
			vec![(D(1000), Some((2999994500, 1499997250, 11107, 599998900, 2000005500)))],
		),
		(
			(5_000_000_000, 1_000_000_000, 2_000),
			vec![(D(500000000), Some((499999500, 249999750, 4553, 99999900, 4500000500)))],
		),
		((5_000_000_000, 1_000_000_000, 2_000), vec![(D(600000000), None)]),
		(
			(5_000_000_000, 1_000_000_000, 2_000),
			vec![
				(D(-100000), Some((3000499500, 1500249750, 11108, 600099900, 1999500500))),
				(D(-1000000000), Some((8000499500, 4000249750, 24215, 1600099900, 0))),
			],
		),
		(
			(5_000_000_000, 1_000_000_000, 2_000),
			vec![
				(D(-200000), Some((3000999500, 1500499750, 11109, 600199900, 1999000500))),
				(D(50000), Some((3000749500, 1500374750, 11109, 600149900, 1999250500))),
				(D(100000), Some((3000249500, 1500124750, 11107, 600049900, 1999750500))),
			],
		),
		(
			(5_000_000_000, 1_000_000_000, 2_000),
			vec![
				(W(1000, 300), Some((2999997500, 1499998750, 10807, 599999500, 2000002500))),
				(D(1000), Some((2999992500, 1499996250, 10807, 599998500, 2000007500))),
				(W(100000, 300), Some((2999792500, 1499896250, 10507, 599958500, 2000207500))),
				(D(-10000), Some((2999842500, 1499921250, 10507, 599968500, 2000157500))),
				(W(500000, 900), Some((2998842500, 1499421250, 9607, 599768500, 2001157500))),
				(W(0, 10000), None),
			],
		),
	];

	for (input, charges) in tests {
		let (eth_gas_limit, extra_ref_time, extra_proof) = input;
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				let eth_tx_info =
					EthTxInfo::<Test>::new(100, Weight::from_parts(extra_ref_time, extra_proof));
				let mut transaction_meter =
					TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
						eth_gas_limit: eth_gas_limit / gas_scale,
						maybe_weight_limit: None,
						eth_tx_info,
					})
					.unwrap();

				for (charge, remaining) in charges {
					let is_ok = match charge {
						W(ref_time_charge, proof_size_charge) => transaction_meter
							.charge_weight_token(TestToken(ref_time_charge, proof_size_charge))
							.is_ok(),
						D(deposit_charge) => transaction_meter
							.charge_deposit(
								&(if deposit_charge >= 0 {
									StorageDeposit::Charge(deposit_charge as u64)
								} else {
									StorageDeposit::Refund(-deposit_charge as u64)
								}),
							)
							.is_ok(),
					};

					if let Some((
						gas_left,
						ref_time_left,
						proof_size_left,
						deposit_left,
						gas_consumed,
					)) = remaining
					{
						assert!(is_ok);
						assert_eq!(gas_left / gas_scale, transaction_meter.eth_gas_left().unwrap());
						assert_eq!(
							Weight::from_parts(ref_time_left, proof_size_left),
							transaction_meter.weight_left().unwrap()
						);
						assert_eq!(deposit_left, transaction_meter.deposit_left().unwrap());
						assert_eq!(
							gas_consumed / gas_scale,
							transaction_meter.total_consumed_gas()
						);
					} else {
						assert!(!is_ok);
					}
				}
			});
	}
}

#[test]
fn substrate_nesting_works() {
	use CallResources::{Ethereum, NoLimits, WeightDeposit};

	let gas_scale = <Test as Config>::GasScale::get();
	let tests = vec![
		(
			((5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000i64), NoLimits),
			Some((2999992500, 1499996250, 10107, 599998500, 2000007500)),
		),
		(
			((5_000_000_000, 1_000_000_000, 2_000, 1000000000, 10000, 50000), NoLimits),
			Some((422112782, 499874750, 1106, 84422556, 4577887218)),
		),
		(
			((5_000_000_000, 1_000_000_000, 3000, 2000, 100000, -7000000000), NoLimits),
			Some((708617665, 18999997750, 1857, 141723533, 4291382335)),
		),
		(
			((5_000_000_000, 1_000_000_000, 3000, 2000, 100000, -70000000000), NoLimits),
			Some((315708617665, 176499997750, 827611, 63141723533, 0)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
				WeightDeposit {
					weight: Weight::from_parts(10000000000, 100000),
					deposit_limit: 1000000000,
				},
			),
			Some((2999992500, 1499996250, 10107, 599998500, 2000007500)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
				WeightDeposit {
					weight: Weight::from_parts(1000000000, 100000),
					deposit_limit: 1000000000,
				},
			),
			Some((2999992500, 1000000000, 10107, 599998500, 2000007500)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
				WeightDeposit {
					weight: Weight::from_parts(10000000000, 10000),
					deposit_limit: 1000000000,
				},
			),
			Some((2999992500, 1499996250, 10000, 599998500, 2000007500)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
				WeightDeposit {
					weight: Weight::from_parts(10000000000, 100000),
					deposit_limit: 100000000,
				},
			),
			Some((2999992500, 1499996250, 10107, 100000000, 2000007500)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
				WeightDeposit { weight: Weight::from_parts(40000, 200), deposit_limit: 300000 },
			),
			Some((1580000, 40000, 200, 300000, 2000007500)),
		),
		(
			(
				(4_000_000_000, 100_000_000, 3_000, 1000, 1000, 100),
				WeightDeposit { weight: Weight::from_parts(40000, 200), deposit_limit: 300000 },
			),
			Some((77793945, 40000, 200, 300000, 1525879906)),
		),
		(
			(
				(4_000_000_000, 100_000_000, 3_000, 1800000000, 1000, 100),
				WeightDeposit { weight: Weight::from_parts(40000, 200), deposit_limit: 300000 },
			),
			Some((1580000, 40000, 200, 300000, 3800001000)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
				Ethereum { gas: 2999992501, add_stipend: false },
			),
			Some((2999992500, 1499996250, 10107, 599998500, 2000007500)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
				Ethereum { gas: 2999992490, add_stipend: false },
			),
			Some((2999992490, 1499996245, 10107, 599998498, 2000007500)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 2_000, 1000000000, 10000, 50000),
				Ethereum { gas: 10000, add_stipend: false },
			),
			Some((10000, 288823359, 0, 2000, 4577887218)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 3000, 2000, 100000, -7000000000),
				Ethereum { gas: 708617660, add_stipend: false },
			),
			Some((708617660, 18999997747, 1857, 141723532, 4291382335)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 3000, 2000, 100000, -7000000000),
				Ethereum { gas: 3157000000, add_stipend: false },
			),
			Some((708617665, 18999997750, 1857, 141723533, 4291382335)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 3000, 2000, 10106, 91452),
				Ethereum { gas: 500, add_stipend: false },
			),
			Some((4, 1499769120, 0, 0, 4999999996)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 3000, 2000, 10106, 91452),
				Ethereum { gas: 300, add_stipend: false },
			),
			Some((4, 1499769120, 0, 0, 4999999996)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 3000, 2000, 1010, 91452),
				Ethereum { gas: 300, add_stipend: false },
			),
			Some((300, 150, 1232, 60, 2000461760)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 3000, 2000, 2242, 91452),
				Ethereum { gas: 600, add_stipend: false },
			),
			Some((600, 300, 0, 120, 2000461760)),
		),
		(
			(
				(5_000_000_000, 1_000_000_000, 3000, 2000, 2243, 91452),
				Ethereum { gas: 600, add_stipend: false },
			),
			Some((600, 21188, 0, 120, 2000503536)),
		),
	];

	for (input, remaining) in tests {
		let (
			(
				eth_gas_limit,
				extra_ref_time,
				extra_proof,
				ref_time_charge,
				proof_size_charge,
				deposit_charge,
			),
			call_resource,
		) = input;
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				#[cfg(test)]
				let eth_tx_info = EthTxInfo::<Test>::new(100, Weight::from_parts(extra_ref_time, extra_proof));
				let mut transaction_meter =
					TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
						eth_gas_limit: eth_gas_limit / gas_scale,
						maybe_weight_limit: None,
						eth_tx_info: eth_tx_info.clone(),
					})
					.unwrap();

				transaction_meter
					.charge_deposit(
						&(if deposit_charge >= 0 {
							StorageDeposit::Charge(deposit_charge as u64)
						} else {
							StorageDeposit::Refund(-deposit_charge as u64)
						}),
					)
					.unwrap();

				transaction_meter
					.charge_weight_token(TestToken(ref_time_charge, proof_size_charge))
					.unwrap();

				let scaled_call_resource = match call_resource {
					Ethereum { gas, add_stipend } => Ethereum { gas: gas / gas_scale, add_stipend },
					_ => call_resource,
				};
				let nested = transaction_meter.new_nested(&scaled_call_resource);

				if let Some((
					gas_left,
					ref_time_left,
					proof_size_left,
					deposit_left,
					gas_consumed,
				)) = remaining
				{
					let nested = nested.unwrap();
					assert_eq!(gas_left / gas_scale, nested.eth_gas_left().unwrap());
					assert_eq!(
						Weight::from_parts(ref_time_left, proof_size_left),
						nested.weight_left().unwrap()
					);
					assert_eq!(deposit_left, nested.deposit_left().unwrap());
					assert_eq!(gas_consumed / gas_scale, nested.total_consumed_gas());
				} else {
					assert!(nested.is_err());
				}
			});
	}
}

#[test]
fn substrate_nesting_charges_works() {
	use Charge::{D, W};

	let gas_scale = <Test as Config>::GasScale::get();
	let tests = vec![
		(
			(5_000_000_000, 1_000_000_000, 2_000, 1000, 100, 1000i64, 1000),
			vec![
				(W(100, 100), Some((800, 400, 3042, 160, 2000007700))),
				(D(100), Some((300, 150, 3042, 60, 2000008200))),
			],
		),
		(
			(5_000_000_000, 419_615_482, 2_000, 1000, 100, 100, 1000),
			vec![
				(W(100, 100), Some((566, 400, 0, 113, 839234398))),
				(W(100, 0), Some((566, 300, 0, 113, 839234398))),
				(D(100), Some((66, 50, 0, 13, 839234898))),
				(W(50, 0), Some((0, 0, 0, 0, 839234964))),
				(D(-300), Some((1500, 750, 0, 300, 839233464))),
				(W(50, 0), Some((1400, 700, 0, 280, 839233564))),
				(W(0, 1), None),
			],
		),
		(
			(5_000_000_000, 100_000_000, 2_000, 1000, 100, 100, 10000000),
			vec![
				(D(100), Some((9999500, 305541962, 26, 1999900, 801087925))),
				(W(100, 0), Some((9999500, 305541862, 26, 1999900, 801087925))),
				(W(0, 20), Some((2370105, 305541862, 6, 474021, 808717320))),
			],
		),
	];

	for (input, charges) in tests {
		let (
			eth_gas_limit,
			extra_ref_time,
			extra_proof,
			ref_time_charge,
			proof_size_charge,
			deposit_charge,
			gas_limit,
		) = input;
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				let eth_tx_info =
					EthTxInfo::<Test>::new(100, Weight::from_parts(extra_ref_time, extra_proof));
				let mut transaction_meter =
					TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
						eth_gas_limit: eth_gas_limit / gas_scale,
						maybe_weight_limit: None,
						eth_tx_info,
					})
					.unwrap();

				transaction_meter
					.charge_deposit(
						&(if deposit_charge >= 0 {
							StorageDeposit::Charge(deposit_charge as u64)
						} else {
							StorageDeposit::Refund((-deposit_charge) as u64)
						}),
					)
					.unwrap();

				transaction_meter
					.charge_weight_token(TestToken(ref_time_charge, proof_size_charge))
					.unwrap();

				let mut nested = transaction_meter
					.new_nested(&CallResources::Ethereum {
						gas: gas_limit / gas_scale,
						add_stipend: false,
					})
					.unwrap();

				for (charge, remaining) in charges {
					let is_ok = match charge {
						W(ref_time_charge, proof_size_charge) => nested
							.charge_weight_token(TestToken(ref_time_charge, proof_size_charge))
							.is_ok(),
						D(deposit_charge) => nested
							.charge_deposit(
								&(if deposit_charge >= 0 {
									StorageDeposit::Charge(deposit_charge as u64)
								} else {
									StorageDeposit::Refund(-deposit_charge as u64)
								}),
							)
							.is_ok(),
					};

					if let Some((
						gas_left,
						ref_time_left,
						proof_size_left,
						deposit_left,
						gas_consumed,
					)) = remaining
					{
						assert!(is_ok);
						assert_eq!(gas_left / gas_scale, nested.eth_gas_left().unwrap());
						assert_eq!(
							Weight::from_parts(ref_time_left, proof_size_left),
							nested.weight_left().unwrap()
						);
						assert_eq!(deposit_left, nested.deposit_left().unwrap());
						assert_eq!(gas_consumed / gas_scale, nested.total_consumed_gas());
					} else {
						assert!(!is_ok);
					}
				}
			});
	}
}

#[test]
fn catch_constructor_test() {
	use crate::{evm::*, tracing::trace};
	use frame_support::assert_ok;

	let (code, _) = compile_module_with_type("CatchConstructorTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);

		let Contract { addr: test_address, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let first_estimate = crate::Pallet::<Test>::dry_run_eth_transact(
			GenericTransaction {
				from: Some(ALICE_ADDR),
				to: Some(test_address),
				input: CatchConstructorTest::tryCatchNewContractCall { _owner: [0u8; 20].into() }
					.abi_encode()
					.into(),
				..Default::default()
			},
			Default::default(),
		);

		assert_ok!(first_estimate.as_ref());

		let second_estimate = crate::Pallet::<Test>::dry_run_eth_transact(
			GenericTransaction {
				from: Some(ALICE_ADDR),
				to: Some(test_address),
				gas: Some(first_estimate.unwrap().eth_gas.into()),
				input: CatchConstructorTest::tryCatchNewContractCall { _owner: [0u8; 20].into() }
					.abi_encode()
					.into(),
				..Default::default()
			},
			Default::default(),
		);

		assert_ok!(second_estimate);

		let make_call = |eth_gas_limit: u64| {
			builder::bare_call(test_address)
				.data(
					CatchConstructorTest::tryCatchNewContractCall { _owner: [0u8; 20].into() }
						.abi_encode(),
				)
				.transaction_limits(crate::TransactionLimits::EthereumGas {
					eth_gas_limit: eth_gas_limit.into(),
					maybe_weight_limit: None,
					eth_tx_info: crate::EthTxInfo::new(0, Default::default()),
				})
				.build()
		};

		let results = make_call(u64::MAX);

		let mut tracer =
			CallTracer::new(CallTracerConfig { with_logs: true, only_top_call: false });

		trace(&mut tracer, || {
			let results = make_call(
				results
					.gas_consumed
					.saturating_add(<Test as pallet_balances::Config>::ExistentialDeposit::get()),
			);
			assert_ok!(results.result);
		});
		let gas_trace = tracer.collect_trace().unwrap();
		assert_eq!("revert: invalid address", gas_trace.calls[0].revert_reason.as_ref().unwrap());
	});
}
