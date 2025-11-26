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
	compile_module_with_type, CatchConstructorTest, Deposit, FixtureType,
};
use sp_runtime::{FixedU128, Weight};
use test_case::test_case;

// Shared test data structures for JSON-based tests
#[derive(Debug, serde::Deserialize)]
struct MeterState {
	gas_left: u64,
	weight_left: Weight,
	deposit_left: u64,
	#[serde(default)]
	gas_consumed: u64,
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type")]
enum ChargeOp {
	Weight { weight: Weight, expected: Option<MeterState> },
	Deposit { amount: i64, expected: Option<MeterState> },
}

/// A trivial token that charges the specified weight.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
struct TestToken(Weight);
impl WeightToken<Test> for TestToken {
	fn weight(&self) -> Weight {
		self.0
	}
}

#[test_case(FixtureType::Solc    ; "solc")]
#[test_case(FixtureType::Resolc  ; "resolc")]
fn max_consumed_deposit_integration(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Deposit", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_addr)
			.data(Deposit::callSetAndClearCall {}.abi_encode())
			.build();

		assert_eq!(result.storage_deposit, StorageDeposit::Charge(66));
		assert_eq!(result.max_storage_deposit, StorageDeposit::Charge(132));
	});
}

#[ignore = "TODO: Does not work yet, see https://github.com/paritytech/contract-issues/issues/213"]
#[test_case(FixtureType::Solc    ; "solc")]
#[test_case(FixtureType::Resolc  ; "resolc")]
fn max_consumed_deposit_integration_refunds_subframes(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Deposit", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(caller_addr)
			.data(Deposit::setAndClearCall {}.abi_encode())
			.build();

		assert_eq!(result.storage_deposit, StorageDeposit::Charge(66));
		assert_eq!(result.max_storage_deposit, StorageDeposit::Charge(132));

		builder::bare_call(caller_addr)
			.data(Deposit::clearAllCall {}.abi_encode())
			.build();

		let result = builder::bare_call(caller_addr)
			.data(Deposit::setAndCallClearCall {}.abi_encode())
			.build();

		assert_eq!(result.storage_deposit, StorageDeposit::Charge(66));
		assert_eq!(result.max_storage_deposit, StorageDeposit::Charge(132));
	});
}

#[test]
fn substrate_metering_initialization_works() {
	#[derive(Debug, serde::Deserialize)]
	struct InitializationTest {
		name: String,
		eth_gas_limit: u64,
		extra_weight: Weight,
		expected: Option<MeterState>,
	}

	#[derive(Debug, serde::Deserialize)]
	struct InitializationWeightLimitTest {
		name: String,
		weight_limit: Weight,
		expected: Weight,
	}

	let tests: Vec<InitializationTest> =
		serde_json::from_str(include_str!("./testdata/initialization.json"))
			.expect("Failed to parse initialization.json");

	for InitializationTest { name, eth_gas_limit, extra_weight, expected } in tests {
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				let eth_tx_info = EthTxInfo::<Test>::new(100, extra_weight);
				let transaction_meter =
					TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
						eth_gas_limit,
						maybe_weight_limit: None,
						eth_tx_info,
					});

				if let Some(expected) = expected {
					let transaction_meter = transaction_meter
						.unwrap_or_else(|_| panic!("Test '{name}' should succeed"));
					assert_eq!(
						expected.gas_left,
						transaction_meter.eth_gas_left().unwrap(),
						"Test '{name}': gas_left mismatch",
					);
					assert_eq!(
						expected.weight_left,
						transaction_meter.weight_left().unwrap(),
						"Test '{name}': weight_left mismatch",
					);
					assert_eq!(
						expected.deposit_left,
						transaction_meter.deposit_left().unwrap(),
						"Test '{name}': deposit_left mismatch",
					);
				} else {
					assert!(transaction_meter.is_err(), "Test '{name}' should have failed",);
				}
			});
	}

	let weight_limit_tests: Vec<InitializationWeightLimitTest> =
		serde_json::from_str(include_str!("./testdata/initialization_weight_limit.json"))
			.expect("Failed to parse initialization_weight_limit.json");

	for InitializationWeightLimitTest { name, weight_limit, expected } in weight_limit_tests {
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				let eth_tx_info =
					EthTxInfo::<Test>::new(100, Weight::from_parts(1_000_000_000, 2_000));
				let transaction_meter =
					TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
						eth_gas_limit: 5_000_000_000,
						maybe_weight_limit: Some(weight_limit),
						eth_tx_info,
					})
					.unwrap_or_else(|_| panic!("Test '{name}' should succeed"));

				assert_eq!(
					expected,
					transaction_meter.weight_left().unwrap(),
					"Test '{name}': weight_left mismatch",
				);
			});
	}
}

#[test]
fn substrate_metering_charges_works() {
	#[derive(Debug, serde::Deserialize)]
	struct ChargesTest {
		name: String,
		eth_gas_limit: u64,
		extra_weight: Weight,
		charges: Vec<ChargeOp>,
	}

	let tests: Vec<ChargesTest> = serde_json::from_str(include_str!("./testdata/charges.json"))
		.expect("Failed to parse charges.json");

	for ChargesTest { name, eth_gas_limit, extra_weight, charges } in tests {
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				let eth_tx_info = EthTxInfo::<Test>::new(100, extra_weight);
				let mut transaction_meter =
					TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
						eth_gas_limit,
						maybe_weight_limit: None,
						eth_tx_info,
					})
					.unwrap_or_else(|_| panic!("Test '{name}': failed to create meter"));

				for charge in charges {
					let is_ok = match charge {
						ChargeOp::Weight { weight, expected: _ } =>
							transaction_meter.charge_weight_token(TestToken(weight)).is_ok(),
						ChargeOp::Deposit { amount, expected: _ } => transaction_meter
							.charge_deposit(&if amount >= 0 {
								StorageDeposit::Charge(amount as u64)
							} else {
								StorageDeposit::Refund((-amount) as u64)
							})
							.is_ok(),
					};

					let expected = match charge {
						ChargeOp::Weight { expected, .. } | ChargeOp::Deposit { expected, .. } =>
							expected,
					};

					if let Some(expected) = expected {
						assert!(is_ok, "Test '{name}': charge should have succeeded");
						assert_eq!(
							expected.gas_left,
							transaction_meter.eth_gas_left().unwrap(),
							"Test '{name}': gas_left mismatch",
						);
						assert_eq!(
							expected.weight_left,
							transaction_meter.weight_left().unwrap(),
							"Test '{name}': weight_left mismatch",
						);
						assert_eq!(
							expected.deposit_left,
							transaction_meter.deposit_left().unwrap(),
							"Test '{name}': deposit_left mismatch",
						);
						assert_eq!(
							expected.gas_consumed,
							transaction_meter.total_consumed_gas(),
							"Test '{name}': gas_consumed mismatch",
						);
					} else {
						assert!(!is_ok, "Test '{name}': charge should have failed");
					}
				}
			});
	}
}

#[test]
fn substrate_nesting_works() {
	#[derive(Debug, serde::Deserialize)]
	struct NestingTest {
		name: String,
		eth_gas_limit: u64,
		extra_weight: Weight,
		weight_charge: Weight,
		deposit_charge: i64,
		call_resource: CallResourceType,
		expected: Option<MeterState>,
	}

	#[derive(Debug, serde::Deserialize)]
	#[serde(tag = "type")]
	enum CallResourceType {
		NoLimits,
		WeightDeposit { weight: Weight, deposit_limit: u64 },
		Ethereum { gas: u64, add_stipend: bool },
	}

	impl CallResourceType {
		fn to_call_resources(&self) -> CallResources<Test> {
			match self {
				CallResourceType::NoLimits => CallResources::NoLimits,
				CallResourceType::WeightDeposit { weight, deposit_limit } =>
					CallResources::WeightDeposit { weight: *weight, deposit_limit: *deposit_limit },
				CallResourceType::Ethereum { gas, add_stipend } =>
					CallResources::Ethereum { gas: *gas, add_stipend: *add_stipend },
			}
		}
	}

	let tests: Vec<NestingTest> = serde_json::from_str(include_str!("./testdata/nesting.json"))
		.expect("Failed to parse nesting.json");

	for NestingTest {
		name,
		eth_gas_limit,
		extra_weight,
		weight_charge,
		deposit_charge,
		call_resource,
		expected,
	} in tests
	{
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				let eth_tx_info = EthTxInfo::<Test>::new(100, extra_weight);
				let mut transaction_meter =
					TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
						eth_gas_limit,
						maybe_weight_limit: None,
						eth_tx_info,
					})
					.unwrap_or_else(|_| panic!("Test '{name}': failed to create meter"));

				transaction_meter
					.charge_deposit(&if deposit_charge >= 0 {
						StorageDeposit::Charge(deposit_charge as u64)
					} else {
						StorageDeposit::Refund((-deposit_charge) as u64)
					})
					.unwrap_or_else(|_| panic!("Test '{name}': failed to charge deposit"));

				transaction_meter
					.charge_weight_token(TestToken(weight_charge))
					.unwrap_or_else(|_| panic!("Test '{name}': failed to charge weight"));

				let nested = transaction_meter.new_nested(&call_resource.to_call_resources());

				if let Some(expected) = expected {
					let nested = nested.unwrap_or_else(|_| panic!("Test '{name}': should succeed"));
					assert_eq!(
						expected.gas_left,
						nested.eth_gas_left().unwrap(),
						"Test '{name}': gas_left mismatch",
					);
					assert_eq!(
						expected.weight_left,
						nested.weight_left().unwrap(),
						"Test '{name}': weight_left mismatch",
					);
					assert_eq!(
						expected.deposit_left,
						nested.deposit_left().unwrap(),
						"Test '{name}': deposit_left mismatch",
					);
					assert_eq!(
						expected.gas_consumed,
						nested.total_consumed_gas(),
						"Test '{name}': gas_consumed mismatch",
					);
				} else {
					assert!(nested.is_err(), "Test '{name}': should have failed");
				}
			});
	}
}

#[test]
fn substrate_nesting_charges_works() {
	#[derive(Debug, serde::Deserialize)]
	struct NestingChargesTest {
		name: String,
		eth_gas_limit: u64,
		extra_weight: Weight,
		weight_charge: Weight,
		deposit_charge: i64,
		nested_gas_limit: u64,
		charges: Vec<ChargeOp>,
	}

	let test_data = std::fs::read_to_string("src/metering/testdata/nesting_charges.json").unwrap();
	let tests: Vec<NestingChargesTest> = serde_json::from_str(&test_data).unwrap();

	for NestingChargesTest {
		name,
		eth_gas_limit,
		extra_weight,
		weight_charge,
		deposit_charge,
		nested_gas_limit,
		charges,
	} in tests
	{
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				let eth_tx_info = EthTxInfo::<Test>::new(100, extra_weight);
				let mut transaction_meter =
					TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
						eth_gas_limit,
						maybe_weight_limit: None,
						eth_tx_info,
					})
					.unwrap_or_else(|_| {
						panic!(
							"Test '{name}': failed to create transaction meter with gas limit {eth_gas_limit}"
						)
					});

				transaction_meter
					.charge_deposit(
						&(if deposit_charge >= 0 {
							StorageDeposit::Charge(deposit_charge as u64)
						} else {
							StorageDeposit::Refund((-deposit_charge) as u64)
						}),
					)
					.unwrap_or_else(|_| {
						panic!("Test '{name}': failed to charge initial deposit {deposit_charge}")
					});

				transaction_meter.charge_weight_token(TestToken(weight_charge)).unwrap_or_else(
					|_| {
						panic!(
							"Test '{name}': failed to charge initial weight ({}, {})",
							weight_charge.ref_time(),
							weight_charge.proof_size()
						)
					},
				);

				let mut nested = transaction_meter
					.new_nested(&CallResources::Ethereum {
						gas: nested_gas_limit,
						add_stipend: false,
					})
					.unwrap_or_else(|_| {
						panic!(
							"Test '{name}': failed to create nested meter with gas limit {nested_gas_limit}"
						)
					});

				for (idx, charge) in charges.iter().enumerate() {
					let is_ok = match charge {
						ChargeOp::Weight { weight, .. } =>
							nested.charge_weight_token(TestToken(*weight)).is_ok(),
						ChargeOp::Deposit { amount, .. } => nested
							.charge_deposit(
								&(if *amount >= 0 {
									StorageDeposit::Charge(*amount as u64)
								} else {
									StorageDeposit::Refund((-*amount) as u64)
								}),
							)
							.is_ok(),
					};

					let expected = match charge {
						ChargeOp::Weight { expected, .. } => expected,
						ChargeOp::Deposit { expected, .. } => expected,
					};

					if let Some(expected) = expected {
						assert!(is_ok, "Test '{name}': charge #{} should succeed", idx + 1);
						assert_eq!(
							expected.gas_left,
							nested.eth_gas_left().unwrap(),
							"Test '{name}': charge #{} gas_left mismatch",
							idx + 1
						);
						assert_eq!(
							expected.weight_left,
							nested.weight_left().unwrap(),
							"Test '{name}': charge #{} weight_left mismatch",
							idx + 1
						);
						assert_eq!(
							expected.deposit_left,
							nested.deposit_left().unwrap(),
							"Test '{name}': charge #{} deposit_left mismatch",
							idx + 1
						);
						assert_eq!(
							expected.gas_consumed,
							nested.total_consumed_gas(),
							"Test '{name}': charge #{} gas_consumed mismatch",
							idx + 1
						);
					} else {
						assert!(!is_ok, "Test '{name}': charge #{} should fail", idx + 1);
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
