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

use super::{SignedGas, math::eip_150};
use crate::{
	BalanceOf, CallResources, Code, Config, Error, EthTxInfo, ExecConfig, StorageDeposit,
	TransactionLimits, TransactionMeter, WeightToken,
	storage::AccountInfo,
	test_utils::{ALICE, ALICE_ADDR, CHARLIE, builder::Contract},
	tests::{ExtBuilder, Test, builder},
};
use alloy_core::sol_types::SolCall;
use frame_support::{
	storage::{TransactionOutcome, with_transaction},
	traits::fungible::Mutate,
};
use frame_system::RawOrigin;
use pallet_revive_fixtures::{
	CatchConstructorTest, DepositPrecompile, FixtureType, ReentryStorage, compile_module_with_type,
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
	D(i128),
}

#[test]
fn test_deposit_calculation() {
	ExtBuilder::default()
		.with_next_fee_multiplier(FixedU128::from_rational(2, 1))
		.build()
		.execute_with(|| {
			let deposit1 = StorageDeposit::Refund(10);
			let gas_result1 = SignedGas::<Test>::from_adjusted_deposit_charge(&deposit1);
			assert_eq!(gas_result1, SignedGas::Negative(BalanceOf::<Test>::from(5u32)));

			let deposit2 = StorageDeposit::Refund(1);
			let gas_result2 = SignedGas::<Test>::from_adjusted_deposit_charge(&deposit2);
			assert_eq!(gas_result2, SignedGas::Positive(BalanceOf::<Test>::from(0u32)));
		});
}

#[test]
fn test_apply_eip_150_to_signed_gas() {
	ExtBuilder::default().build().execute_with(|| {
		let test_cases: Vec<(SignedGas<Test>, SignedGas<Test>)> = vec![
			(SignedGas::Positive(6400), SignedGas::Positive(6300)),
			(SignedGas::Positive(64), SignedGas::Positive(63)),
			(SignedGas::Positive(65), SignedGas::Positive(63)),
			(SignedGas::Positive(63), SignedGas::Positive(62)),
			(SignedGas::Positive(1), SignedGas::Positive(0)),
			(SignedGas::Positive(0), SignedGas::Positive(0)),
			(SignedGas::Positive(123_456_789), SignedGas::Positive(121_527_776)),
			(SignedGas::Negative(100), SignedGas::Negative(100)),
			(SignedGas::Negative(0), SignedGas::Positive(0)),
		];

		for (input, expected) in test_cases {
			assert_eq!(input.apply_eip_150(), expected, "failed for input {input:?}");
		}
	});
}

#[test]
fn test_apply_eip_150_to_weight() {
	let test_cases: Vec<(Weight, Weight)> = vec![
		(Weight::from_parts(6400, 6400), Weight::from_parts(6300, 6300)),
		(Weight::from_parts(64, 64), Weight::from_parts(63, 63)),
		(Weight::from_parts(63, 63), Weight::from_parts(62, 62)),
		(Weight::from_parts(1, 1), Weight::from_parts(0, 0)),
		(Weight::from_parts(0, 0), Weight::from_parts(0, 0)),
		(Weight::from_parts(128, 64), Weight::from_parts(126, 63)),
		(
			Weight::from_parts(1_000_000_000, 500_000_000),
			Weight::from_parts(984_375_000, 492_187_500),
		),
		(Weight::from_parts(65, 100), Weight::from_parts(63, 98)),
		(Weight::from_parts(127, 129), Weight::from_parts(125, 126)),
	];

	for (input, expected) in test_cases {
		assert_eq!(eip_150::apply_weight(input), expected, "failed for input {input:?}");
	}
}

#[test]
fn test_compute_eip_150_overhead() {
	// Given consumed weight, verify: apply_eip_150(consumed + overhead) == consumed
	let input_weights: Vec<Weight> = vec![
		Weight::from_parts(0, 0),
		Weight::from_parts(1, 1),
		Weight::from_parts(62, 62),
		Weight::from_parts(63, 127),
		Weight::from_parts(128, 64),
		Weight::from_parts(138, 201),
		Weight::from_parts(6300, 3155),
		Weight::from_parts(847_293_651, 42),
		Weight::from_parts(5_183_492_761, 183_947),
		Weight::from_parts(12_345_678_901, 7_629_384),
	];

	for consumed in input_weights {
		let overhead = eip_150::overhead_weight(consumed);
		let required = consumed.saturating_add(overhead);
		let available_to_nested = eip_150::apply_weight(required);

		assert_eq!(
			available_to_nested, consumed,
			"failed for consumed={consumed:?}: overhead={overhead:?}, required={required:?}, available={available_to_nested:?}"
		);
	}
}

#[test]
fn test_compute_gas_ratio() {
	use super::math::compute_gas_ratio;

	ExtBuilder::default().build().execute_with(|| {
		// (gas_limit, remaining_gas, expected_numerator, expected_denominator)
		let ratio_cases: Vec<(u128, u128, u128, u128)> = vec![
			(100, 100, 1, 1),
			(200, 100, 1, 1),
			(100, 0, 1, 1),
			(0, 0, 1, 1),
			(50, 100, 1, 2),
			(25, 100, 1, 4),
			(1, 100, 1, 100),
			(0, 100, 0, 1),
		];

		for (gas_limit, remaining, num, denom) in ratio_cases {
			assert_eq!(
				compute_gas_ratio::<Test>(gas_limit, remaining),
				FixedU128::from_rational(num, denom),
				"failed for gas_limit={gas_limit}, remaining={remaining}"
			);
		}
	});
}

#[test]
fn test_apply_eip_150_to_balance() {
	ExtBuilder::default().build().execute_with(|| {
		// (input, expected)
		let test_cases: Vec<(u128, u128)> = vec![
			(6400, 6300),
			(64, 63),
			(65, 63),
			(63, 62),
			(1, 0),
			(0, 0),
			(128, 126),
			(127, 125),
			(1_847_293_651, 1_818_429_687),
			(123_456_789, 121_527_776),
		];

		for (input, expected) in test_cases {
			assert_eq!(eip_150::apply_balance::<Test>(input), expected, "failed for input {input}");
		}
	});
}

#[test]
fn test_scale_weight_by_ratio() {
	use super::math::scale_weight_by_ratio;

	// (input_weight, ratio, expected_weight)
	let test_cases: Vec<(Weight, FixedU128, Weight)> = vec![
		(
			Weight::from_parts(1000, 500),
			FixedU128::from_rational(1, 1),
			Weight::from_parts(1000, 500),
		),
		(Weight::from_parts(1000, 500), FixedU128::from_rational(0, 1), Weight::from_parts(0, 0)),
		(
			Weight::from_parts(1001, 503),
			FixedU128::from_rational(1, 3),
			Weight::from_parts(333, 167),
		),
		(
			Weight::from_parts(1001, 503),
			FixedU128::from_rational(2, 3),
			Weight::from_parts(667, 335),
		),
		(
			Weight::from_parts(1237, 891),
			FixedU128::from_rational(5, 7),
			Weight::from_parts(883, 636),
		),
		(
			Weight::from_parts(847_293_651, 123_456_789),
			FixedU128::from_rational(63, 64),
			Weight::from_parts(834_054_687, 121_527_776),
		),
	];

	for (input, ratio, expected) in test_cases {
		assert_eq!(
			scale_weight_by_ratio(input, ratio),
			expected,
			"failed for input {input:?}, ratio {ratio:?}"
		);
	}
}

#[test]
fn test_validate_and_get_stipend() {
	use super::math::validate_and_get_stipend;

	ExtBuilder::default().build().execute_with(|| {
		// With enough weight, should succeed and return stipend
		let stipend = validate_and_get_stipend::<Test>(Weight::MAX).unwrap();

		// With exactly stipend weight, should succeed
		let result = validate_and_get_stipend::<Test>(stipend);
		assert!(result.is_ok());

		// With less than stipend (in ref_time), should fail
		if stipend.ref_time() > 0 {
			let insufficient = Weight::from_parts(stipend.ref_time() - 1, stipend.proof_size());
			let result = validate_and_get_stipend::<Test>(insufficient);
			assert!(result.is_err());
		}

		// With less than stipend (in proof_size), should fail
		if stipend.proof_size() > 0 {
			let insufficient = Weight::from_parts(stipend.ref_time(), stipend.proof_size() - 1);
			let result = validate_and_get_stipend::<Test>(insufficient);
			assert!(result.is_err());
		}

		// With zero weight, should fail (assuming stipend > 0)
		if stipend.ref_time() > 0 || stipend.proof_size() > 0 {
			let result = validate_and_get_stipend::<Test>(Weight::zero());
			assert!(result.is_err());
		}
	});
}

/// Test that max_storage_deposit correctly tracks the peak storage allocation.
///
/// This test verifies that:
/// 1. `storage_deposit` reflects the net storage change after the call
/// 2. `max_storage_deposit` tracks the maximum storage allocation that occurred at any point during
///    execution (before any refunds)
///
/// The test contract sets two storage values (a=2, b=3) totaling 132 units of deposit,
/// then clears one value, leaving 66 units as the net deposit.
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

		// Test direct set and clear (no nested call)
		let direct_result = builder::bare_call(caller_addr)
			.data(DepositPrecompile::setAndClearCall {}.abi_encode())
			.build();

		// Net deposit: one storage slot remains (66 units)
		// Max deposit: peak allocation was two storage slots (132 units)
		assert_eq!(direct_result.storage_deposit, StorageDeposit::Charge(66));
		assert_eq!(direct_result.max_storage_deposit, StorageDeposit::Charge(132));
	});
}

/// Test that storage deposit refunds and persisted ContractInfo are correct when
/// parent allocates storage and a nested call clears it.
///
/// Compares `setAndClear()` (direct) vs `setAndCallClear()` (reentrant). Both have
/// the same net effect so deposits and persisted ContractInfo must be identical.
#[test_case(FixtureType::Solc   , "DepositPrecompile" ; "solc precompiles")]
#[test_case(FixtureType::Resolc , "DepositPrecompile" ; "resolc precompiles")]
#[test_case(FixtureType::Solc   , "DepositDirect" ; "solc direct")]
#[test_case(FixtureType::Resolc , "DepositDirect" ; "resolc direct")]
fn nested_call_storage_refund(fixture_type: FixtureType, fixture_name: &str) {
	let (code, _) = compile_module_with_type(fixture_name, fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		// Direct: set a=2, b=3, clear b in the same frame
		let direct_result = builder::bare_call(caller_addr)
			.data(DepositPrecompile::setAndClearCall {}.abi_encode())
			.build();
		let direct_info = AccountInfo::<Test>::load_contract(&caller_addr).unwrap();

		// Reset storage for a fair comparison
		builder::bare_call(caller_addr)
			.data(DepositPrecompile::clearAllCall {}.abi_encode())
			.build();

		// Reentrant: set a=2, b=3, then call this.clear() which clears b
		let nested_result = builder::bare_call(caller_addr)
			.data(DepositPrecompile::setAndCallClearCall {}.abi_encode())
			.build();
		let nested_info = AccountInfo::<Test>::load_contract(&caller_addr).unwrap();

		assert_eq!(
			direct_result.storage_deposit, nested_result.storage_deposit,
			"Nested call should produce same net storage deposit as direct call"
		);
		assert_eq!(
			direct_result.max_storage_deposit, nested_result.max_storage_deposit,
			"Nested call should produce same max storage deposit as direct call"
		);
		assert_eq!(
			direct_info.storage_items, nested_info.storage_items,
			"storage_items: direct={}, nested={} (should be equal)",
			direct_info.storage_items, nested_info.storage_items,
		);
		assert_eq!(
			direct_info.storage_item_deposit, nested_info.storage_item_deposit,
			"storage_item_deposit mismatch between direct and nested paths",
		);
		assert_eq!(
			direct_info.storage_bytes, nested_info.storage_bytes,
			"storage_bytes mismatch between direct and nested paths",
		);
		assert_eq!(
			direct_info.storage_byte_deposit, nested_info.storage_byte_deposit,
			"storage_byte_deposit mismatch between direct and nested paths",
		);
	});
}

/// Direct same-contract reentry (X -> X): a write, a self-reenter, then another write
/// must not double-count the pre-reentry write in the persisted `ContractInfo`. The
/// reentrant run must match a non-reentrant baseline exactly (both persisted accounting
/// and the net deposit charged to the origin). Regression repro for contract-issues#213.
#[test_case(FixtureType::Solc   ; "solc")]
#[test_case(FixtureType::Resolc ; "resolc")]
fn same_contract_reentry_does_not_double_count_storage(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("ReentryStorage", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Baseline: two writes, no reentry.
		let Contract { addr: baseline_addr, .. } =
			builder::bare_instantiate(Code::Upload(code.clone()))
				.salt(Some([1; 32]))
				.build_and_unwrap_contract();
		let baseline = builder::bare_call(baseline_addr)
			.data(ReentryStorage::writeTwiceCall {}.abi_encode())
			.build();
		let baseline_info = AccountInfo::<Test>::load_contract(&baseline_addr).unwrap();

		// Reentrant: write, reenter self (an empty frame), write. Same end state.
		let Contract { addr: reentrant_addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.salt(Some([2; 32]))
			.build_and_unwrap_contract();
		let reentrant = builder::bare_call(reentrant_addr)
			.data(ReentryStorage::writeReenterWriteCall {}.abi_encode())
			.build();
		let reentrant_info = AccountInfo::<Test>::load_contract(&reentrant_addr).unwrap();

		assert!(baseline.result.is_ok(), "baseline call failed: {:?}", baseline.result);
		assert!(reentrant.result.is_ok(), "reentrant call failed: {:?}", reentrant.result);

		// Without the bank-pending-changes fix the pre-reentry write is applied to the
		// persisted ContractInfo twice, inflating every storage field and over-charging
		// the origin. Assert the full set so a partial regression still fails.
		assert_eq!(
			reentrant_info.storage_items, baseline_info.storage_items,
			"storage_items inflated by double-applied pending diff under same-contract reentry",
		);
		assert_eq!(
			reentrant_info.storage_bytes, baseline_info.storage_bytes,
			"storage_bytes inflated under same-contract reentry",
		);
		assert_eq!(
			reentrant_info.storage_item_deposit, baseline_info.storage_item_deposit,
			"storage_item_deposit inflated under same-contract reentry",
		);
		assert_eq!(
			reentrant_info.storage_byte_deposit, baseline_info.storage_byte_deposit,
			"storage_byte_deposit inflated under same-contract reentry",
		);
		assert_eq!(
			reentrant.storage_deposit, baseline.storage_deposit,
			"net storage deposit charged to origin inflated under same-contract reentry",
		);
	});
}

/// Transitive same-contract reentry (X -> Y -> X): the invalidation matcher keys on
/// `account_id`, so an ancestor reentered through an intermediary is affected too. Full
/// solc/resolc matrix over (`ReentryStorage`, `ReentryProxy`) -> 4 cases. As above, the
/// reentrant run must match a non-reentrant baseline exactly.
#[test_case(FixtureType::Solc  , FixtureType::Solc   ; "solc storage, solc proxy")]
#[test_case(FixtureType::Solc  , FixtureType::Resolc ; "solc storage, resolc proxy")]
#[test_case(FixtureType::Resolc, FixtureType::Solc   ; "resolc storage, solc proxy")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc ; "resolc storage, resolc proxy")]
fn transitive_reentry_does_not_double_count_storage(
	storage_type: FixtureType,
	proxy_type: FixtureType,
) {
	let (storage_code, _) = compile_module_with_type("ReentryStorage", storage_type).unwrap();
	let (proxy_code, _) = compile_module_with_type("ReentryProxy", proxy_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: proxy_addr, .. } = builder::bare_instantiate(Code::Upload(proxy_code))
			.salt(Some([3; 32]))
			.build_and_unwrap_contract();

		// Baseline: two writes, no reentry.
		let Contract { addr: baseline_addr, .. } =
			builder::bare_instantiate(Code::Upload(storage_code.clone()))
				.salt(Some([1; 32]))
				.build_and_unwrap_contract();
		let baseline = builder::bare_call(baseline_addr)
			.data(ReentryStorage::writeTwiceCall {}.abi_encode())
			.build();
		let baseline_info = AccountInfo::<Test>::load_contract(&baseline_addr).unwrap();

		// Reentrant: write, reenter self via the proxy (X -> Y -> X), write.
		let Contract { addr: reentrant_addr, .. } =
			builder::bare_instantiate(Code::Upload(storage_code))
				.salt(Some([2; 32]))
				.build_and_unwrap_contract();
		let reentrant = builder::bare_call(reentrant_addr)
			.data(
				ReentryStorage::writeReenterWriteViaCall { proxy: proxy_addr.0.into() }
					.abi_encode(),
			)
			.build();
		let reentrant_info = AccountInfo::<Test>::load_contract(&reentrant_addr).unwrap();

		assert!(baseline.result.is_ok(), "baseline call failed: {:?}", baseline.result);
		assert!(reentrant.result.is_ok(), "reentrant call failed: {:?}", reentrant.result);

		assert_eq!(
			reentrant_info.storage_items, baseline_info.storage_items,
			"storage_items inflated by double-applied diff under transitive reentry",
		);
		assert_eq!(
			reentrant_info.storage_bytes, baseline_info.storage_bytes,
			"storage_bytes inflated under transitive reentry",
		);
		assert_eq!(
			reentrant_info.storage_item_deposit, baseline_info.storage_item_deposit,
			"storage_item_deposit inflated under transitive reentry",
		);
		assert_eq!(
			reentrant_info.storage_byte_deposit, baseline_info.storage_byte_deposit,
			"storage_byte_deposit inflated under transitive reentry",
		);
		assert_eq!(
			reentrant.storage_deposit, baseline.storage_deposit,
			"net storage deposit charged to origin inflated under transitive reentry",
		);
	});
}

/// A dry-run from an unfunded account should still report the `max_storage_deposit`
/// that a successful run would need, so that the caller can size the allowance
/// required to cover the storage deposit before submitting the real transaction.
#[test_case(FixtureType::Solc   , "DepositPrecompile" ; "solc precompiles")]
#[test_case(FixtureType::Resolc , "DepositPrecompile" ; "resolc precompiles")]
#[test_case(FixtureType::Solc   , "DepositDirect" ; "solc direct")]
#[test_case(FixtureType::Resolc , "DepositDirect" ; "resolc direct")]
fn max_storage_deposit_reported_for_unfunded_dry_run(
	fixture_type: FixtureType,
	fixture_name: &str,
) {
	let (code, _) = compile_module_with_type(fixture_name, fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		// Wrap each call in a rolled-back storage layer so state doesn't leak
		// between them. Mirrors how a runtime API dispatches the dry-run.
		let run_in_rollback = |build: &dyn Fn() -> _| {
			with_transaction(|| {
				TransactionOutcome::Rollback(Ok::<_, sp_runtime::DispatchError>(build()))
			})
			.unwrap()
		};

		// Reference run from a funded account.
		let funded = run_in_rollback(&|| {
			builder::bare_call(caller_addr)
				.data(DepositPrecompile::setAndClearCall {}.abi_encode())
				.build()
		});
		assert!(funded.result.is_ok(), "reference run must succeed, got {:?}", funded.result);
		assert!(
			funded.max_storage_deposit.charge_or_zero() > 0,
			"expected the funded reference run to require some storage deposit, got {:?}",
			funded.max_storage_deposit,
		);

		// Same call from CHARLIE, who has no balance, using the runtime-api dry-run
		// `ExecConfig`. Collecting the deposit fails because CHARLIE cannot fund it, but
		// the reported `max_storage_deposit` must still match the funded run so the
		// caller can size the allowance needed to cover the deposit.
		let unfunded = run_in_rollback(&|| {
			crate::Pallet::<Test>::prepare_dry_run(&CHARLIE);
			builder::bare_call(caller_addr)
				.origin(RawOrigin::Signed(CHARLIE).into())
				.data(DepositPrecompile::setAndClearCall {}.abi_encode())
				.transaction_limits(TransactionLimits::WeightAndDeposit {
					weight_limit: <Test as frame_system::Config>::BlockWeights::get().max_block,
					deposit_limit: u128::MAX,
				})
				.exec_config(ExecConfig::new_substrate_tx().with_dry_run(None))
				.build()
		});

		assert_eq!(
			unfunded.result.unwrap_err(),
			Error::<Test>::StorageDepositNotEnoughFunds.into()
		);
		assert_eq!(unfunded.max_storage_deposit, funded.max_storage_deposit);
	});
}

#[test]
fn substrate_metering_initialization_works() {
	let gas_scale = <Test as Config>::GasScale::get().into();

	let tests = vec![
		(
			5_000_000_000u128,
			1_000_000_000,
			2_000,
			Some((2999999500u128, 1499999750, 11107, 599999900)),
		),
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
						eth_gas_limit: eth_gas_limit.div_ceil(gas_scale),
						weight_limit: Weight::MAX,
						eth_tx_info,
					});

				if let Some((gas_left, ref_time_left, proof_size_left, deposit_left)) = remaining {
					let transaction_meter = transaction_meter.unwrap();
					assert_eq!(
						gas_left.div_ceil(gas_scale),
						transaction_meter.eth_gas_left().unwrap()
					);
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
						weight_limit: Weight::from_parts(ref_time_limit, proof_size_limit),
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

	let gas_scale = <Test as Config>::GasScale::get().into();
	let tests = vec![
		(
			(5_000_000_000u128, 1_000_000_000, 2_000),
			vec![(
				W(1000, 100),
				Some((2999997500u128, 1499998750, 11007, 599999500, 2000002500u128)),
			)],
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
						eth_gas_limit: eth_gas_limit.div_ceil(gas_scale),
						weight_limit: Weight::MAX,
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
									StorageDeposit::Charge(deposit_charge as u128)
								} else {
									StorageDeposit::Refund(-deposit_charge as u128)
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
						assert_eq!(
							gas_left.div_ceil(gas_scale),
							transaction_meter.eth_gas_left().unwrap()
						);
						assert_eq!(
							Weight::from_parts(ref_time_left, proof_size_left),
							transaction_meter.weight_left().unwrap()
						);
						assert_eq!(deposit_left, transaction_meter.deposit_left().unwrap());
						assert_eq!(
							gas_consumed.div_ceil(gas_scale),
							transaction_meter.total_consumed_gas()
						);
					} else {
						assert!(!is_ok);
					}
				}
			});
	}
}

fn run_nesting_tests(
	eip_150_rule: eip_150::Rule,
	tests: Vec<(
		((u128, u64, u64, u64, u64, i128), CallResources<Test>),
		Option<(u128, u64, u64, u128, u128)>,
	)>,
) {
	use CallResources::Ethereum;

	let gas_scale = <Test as Config>::GasScale::get().into();
	for (i, (input, remaining)) in tests.into_iter().enumerate() {
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
						eth_gas_limit: eth_gas_limit.div_ceil(gas_scale),
						weight_limit: Weight::MAX,
						eth_tx_info: eth_tx_info.clone(),
					})
					.unwrap();

				transaction_meter
					.charge_deposit(
						&(if deposit_charge >= 0 {
							StorageDeposit::Charge(deposit_charge as u128)
						} else {
							StorageDeposit::Refund(-deposit_charge as u128)
						}),
					)
					.unwrap();

				transaction_meter
					.charge_weight_token(TestToken(ref_time_charge, proof_size_charge))
					.unwrap();

				let scaled_call_resource = match call_resource {
					Ethereum { gas, add_stipend } => {
						Ethereum { gas: (gas as BalanceOf<Test>).div_ceil(gas_scale), add_stipend }
					},
					_ => call_resource,
				};
				let nested = transaction_meter.new_nested(&scaled_call_resource, eip_150_rule);

				if let Some((
					gas_left,
					ref_time_left,
					proof_size_left,
					deposit_left,
					gas_consumed,
				)) = remaining
				{
					let nested = nested.unwrap();
					assert_eq!(
						gas_left.div_ceil(gas_scale),
						nested.eth_gas_left().unwrap(),
						"gas_left mismatch in test case {i}"
					);
					assert_eq!(
						Weight::from_parts(ref_time_left, proof_size_left),
						nested.weight_left().unwrap(),
						"weight_left mismatch in test case {i}"
					);
					assert_eq!(
						deposit_left,
						nested.deposit_left().unwrap(),
						"deposit_left mismatch in test case {i}"
					);
					assert_eq!(
						gas_consumed.div_ceil(gas_scale),
						nested.total_consumed_gas(),
						"gas_consumed mismatch in test case {i}"
					);
				} else {
					assert!(nested.is_err(), "expected error in test case {i}");
				}
			});
	}
}

#[test]
fn substrate_nesting_works() {
	use CallResources::{Ethereum, NoLimits, WeightDeposit};

	run_nesting_tests(
		eip_150::Rule::Skip,
		vec![
			(
				((5_000_000_000u128, 1_000_000_000, 2_000, 1000, 1000, 1000i128), NoLimits),
				Some((2999992500u128, 1499996250, 10107, 599998500, 2000007500u128)),
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
		],
	);
}

/// Same inputs as `substrate_nesting_works` but with EIP-150 63/64 rule applied.
#[test]
fn substrate_nesting_works_with_eip_150() {
	use CallResources::{Ethereum, NoLimits, WeightDeposit};

	run_nesting_tests(
		eip_150::Rule::Apply,
		vec![
			// 0: NoLimits, low consumption.
			(
				((5_000_000_000u128, 1_000_000_000, 2_000, 1000, 1000, 1000i128), NoLimits),
				Some((2953117620u128, 1476558808, 9984, 590623523, 2000007500u128)),
			),
			// 1: NoLimits, high ref_time consumption.
			(
				((5_000_000_000, 1_000_000_000, 2_000, 1000000000, 10000, 50000), NoLimits),
				Some((415517270, 496576993, 1089, 83103453, 4577887220)),
			),
			// 2: NoLimits, large deposit refund.
			(
				((5_000_000_000, 1_000_000_000, 3000, 2000, 100000, -7000000000), NoLimits),
				Some((697545520, 18994461674, 1828, 139509102, 4291382340)),
			),
			// 3: NoLimits, very large deposit refund.
			(
				((5_000_000_000, 1_000_000_000, 3000, 2000, 100000, -70000000000), NoLimits),
				Some((310775670520, 174033524174, 814679, 62155134102, 0)),
			),
			// 4: WeightDeposit, limits larger than remaining budget.
			(
				(
					(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
					WeightDeposit {
						weight: Weight::from_parts(10000000000, 100000),
						deposit_limit: 1000000000,
					},
				),
				Some((2953117620, 1476558808, 9984, 590623523, 2000007500)),
			),
			// 5: WeightDeposit, ref_time limit (1B) binding.
			(
				(
					(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
					WeightDeposit {
						weight: Weight::from_parts(1000000000, 100000),
						deposit_limit: 1000000000,
					},
				),
				Some((2953117620, 1000000000, 9984, 590623523, 2000007500)),
			),
			// 6: WeightDeposit, proof_size limit (10000) not binding.
			(
				(
					(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
					WeightDeposit {
						weight: Weight::from_parts(10000000000, 10000),
						deposit_limit: 1000000000,
					},
				),
				Some((2953117620, 1476558808, 9984, 590623523, 2000007500)),
			),
			// 7: WeightDeposit, deposit limit (100M) binding.
			(
				(
					(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
					WeightDeposit {
						weight: Weight::from_parts(10000000000, 100000),
						deposit_limit: 100000000,
					},
				),
				Some((2953117620, 1476558808, 9984, 100000000, 2000007500)),
			),
			// 8: WeightDeposit, all explicit limits binding.
			(
				(
					(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
					WeightDeposit { weight: Weight::from_parts(40000, 200), deposit_limit: 300000 },
				),
				Some((1580000, 40000, 200, 300000, 2000007500)),
			),
			// 9: WeightDeposit, smaller tx gas, explicit limits binding.
			(
				(
					(4_000_000_000, 100_000_000, 3_000, 1000, 1000, 100),
					WeightDeposit { weight: Weight::from_parts(40000, 200), deposit_limit: 300000 },
				),
				Some((77793945, 40000, 200, 300000, 1525879906)),
			),
			// 10: WeightDeposit, high ref_time consumption, explicit limits binding.
			(
				(
					(4_000_000_000, 100_000_000, 3_000, 1800000000, 1000, 100),
					WeightDeposit { weight: Weight::from_parts(40000, 200), deposit_limit: 300000 },
				),
				Some((1580000, 40000, 200, 300000, 3800001000)),
			),
			// 11: Ethereum gas, gas_limit (2999992501) > 63/64 of remaining.
			(
				(
					(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
					Ethereum { gas: 2999992501, add_stipend: false },
				),
				Some((2953117620, 1476558808, 9984, 590623523, 2000007500)),
			),
			// 12: Ethereum gas, gas_limit (2999992490) > 63/64 of remaining.
			(
				(
					(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000),
					Ethereum { gas: 2999992490, add_stipend: false },
				),
				Some((2953117620, 1476558808, 9984, 590623523, 2000007500)),
			),
			// 13: Ethereum gas, very small gas_limit (10000) is binding over 63/64.
			(
				(
					(5_000_000_000, 1_000_000_000, 2_000, 1000000000, 10000, 50000),
					Ethereum { gas: 10000, add_stipend: false },
				),
				Some((10000, 288823359, 0, 2000, 4577887218)),
			),
			// 14: Ethereum gas, large refund, gas_limit (708617660) is binding.
			(
				(
					(5_000_000_000, 1_000_000_000, 3000, 2000, 100000, -7000000000),
					Ethereum { gas: 708617660, add_stipend: false },
				),
				Some((697545520, 18994461674, 1828, 139509102, 4291382340)),
			),
			// 15: Ethereum gas, large refund, gas_limit (3157000000) > 63/64.
			(
				(
					(5_000_000_000, 1_000_000_000, 3000, 2000, 100000, -7000000000),
					Ethereum { gas: 3157000000, add_stipend: false },
				),
				Some((697545520, 18994461674, 1828, 139509102, 4291382340)),
			),
			// 16: Ethereum gas, tiny gas_limit (500), high proof_size consumption.
			(
				(
					(5_000_000_000, 1_000_000_000, 3000, 2000, 10106, 91452),
					Ethereum { gas: 500, add_stipend: false },
				),
				Some((10, 1499769119, 0, 0, 5000000000)),
			),
			// 17: Ethereum gas, even smaller gas_limit (300).
			(
				(
					(5_000_000_000, 1_000_000_000, 3000, 2000, 10106, 91452),
					Ethereum { gas: 300, add_stipend: false },
				),
				Some((10, 1499769119, 0, 0, 5000000000)),
			),
			// 18: Ethereum gas, small gas_limit (300), moderate proof_size.
			(
				(
					(5_000_000_000, 1_000_000_000, 3000, 2000, 1010, 91452),
					Ethereum { gas: 300, add_stipend: false },
				),
				Some((300, 150, 1232, 60, 2000461760)),
			),
			// 19: Ethereum gas, gas_limit (600), proof_size near boundary.
			(
				(
					(5_000_000_000, 1_000_000_000, 3000, 2000, 2242, 91452),
					Ethereum { gas: 600, add_stipend: false },
				),
				Some((600, 300, 0, 120, 2000461760)),
			),
			// 20: Ethereum gas, gas_limit (600), proof_size just over boundary.
			(
				(
					(5_000_000_000, 1_000_000_000, 3000, 2000, 2243, 91452),
					Ethereum { gas: 600, add_stipend: false },
				),
				Some((600, 21188, 0, 120, 2000503536)),
			),
		],
	);
}

fn run_nesting_charges_tests(
	eip_150_rule: eip_150::Rule,
	tests: Vec<(
		(u128, u64, u64, u64, u64, i128, u128),
		Vec<(Charge, Option<(u128, u64, u64, u128, u128)>)>,
	)>,
) {
	let gas_scale = <Test as Config>::GasScale::get().into();

	for (i, (input, charges)) in tests.into_iter().enumerate() {
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
						eth_gas_limit: eth_gas_limit.div_ceil(gas_scale),
						weight_limit: Weight::MAX,
						eth_tx_info,
					})
					.unwrap();

				transaction_meter
					.charge_deposit(
						&(if deposit_charge >= 0 {
							StorageDeposit::Charge(deposit_charge as u128)
						} else {
							StorageDeposit::Refund((-deposit_charge) as u128)
						}),
					)
					.unwrap();

				transaction_meter
					.charge_weight_token(TestToken(ref_time_charge, proof_size_charge))
					.unwrap();

				let mut nested = transaction_meter
					.new_nested(
						&CallResources::Ethereum {
							gas: gas_limit.div_ceil(gas_scale),
							add_stipend: false,
						},
						eip_150_rule,
					)
					.unwrap();

				for (j, (charge, remaining)) in charges.into_iter().enumerate() {
					let is_ok = match charge {
						Charge::W(ref_time_charge, proof_size_charge) => nested
							.charge_weight_token(TestToken(ref_time_charge, proof_size_charge))
							.is_ok(),
						Charge::D(deposit_charge) => nested
							.charge_deposit(
								&(if deposit_charge >= 0 {
									StorageDeposit::Charge(deposit_charge as u128)
								} else {
									StorageDeposit::Refund(-deposit_charge as u128)
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
						assert!(is_ok, "charge failed in test case {i}, step {j}");
						assert_eq!(
							gas_left.div_ceil(gas_scale),
							nested.eth_gas_left().unwrap(),
							"gas_left mismatch in test case {i}, step {j}"
						);
						assert_eq!(
							Weight::from_parts(ref_time_left, proof_size_left),
							nested.weight_left().unwrap(),
							"weight_left mismatch in test case {i}, step {j}"
						);
						assert_eq!(
							deposit_left,
							nested.deposit_left().unwrap(),
							"deposit_left mismatch in test case {i}, step {j}"
						);
						assert_eq!(
							gas_consumed.div_ceil(gas_scale),
							nested.total_consumed_gas(),
							"gas_consumed mismatch in test case {i}, step {j}"
						);
					} else {
						assert!(!is_ok, "expected failure in test case {i}, step {j}");
					}
				}
			});
	}
}

#[test]
fn substrate_nesting_charges_works() {
	use Charge::{D, W};

	run_nesting_charges_tests(
		eip_150::Rule::Skip,
		vec![
			(
				(5_000_000_000u128, 1_000_000_000, 2_000, 1000, 100, 1000i128, 1000u128),
				vec![
					(W(100, 100), Some((800u128, 400, 3042, 160, 2000007700u128))),
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
		],
	);
}

/// Tests where EIP-150 63/64 gas rule is the binding constraint for nested meter charges.
#[test]
fn substrate_nesting_charges_works_with_eip_150() {
	use Charge::{D, W};

	run_nesting_charges_tests(
		eip_150::Rule::Apply,
		vec![
			// 0: proof_size starts at 9984 (vs 10107 without EIP-150).
			// W(0, 10000) fails after W(100,100) leaves only 9884.
			(
				(5_000_000_000u128, 1_000_000_000, 2_000, 1000, 1000, 1000i128, 5_000_000_000u128),
				vec![
					(
						W(100, 100),
						Some((2953117420u128, 1476558708, 9884, 590623483, 2000007700u128)),
					),
					(D(100), Some((2953116920, 1476558458, 9884, 590623383, 2000008200))),
					(W(0, 10000), None),
				],
			),
			// 1: deposit starts at ~590M (vs ~600M without EIP-150).
			// D(595M) fails; D(-595M) refund succeeds.
			(
				(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000, 5_000_000_000),
				vec![
					(D(595000000), None),
					(D(-595000000), Some((2953117620, 1476558808, 9984, 590623523, 2000007500))),
				],
			),
			// 2: All charges succeed within the 63/64-reduced budget.
			(
				(5_000_000_000, 1_000_000_000, 2_000, 1000, 1000, 1000, 5_000_000_000),
				vec![
					(W(100, 100), Some((2953117420, 1476558708, 9884, 590623483, 2000007700))),
					(D(100), Some((2953116920, 1476558458, 9884, 590623383, 2000008200))),
					(W(100000000, 0), Some((2753116920, 1376558458, 9884, 550623383, 2200008200))),
				],
			),
		],
	);
}

/// Proves that without ratio scaling, the deposit_limit set by a substrate call
/// (WeightDeposit) is correctly enforced as a total budget across ethereum children.
///
/// Call chain: ethereum root → substrate (WeightDeposit) → ethereum children.
#[test]
fn ethereum_execution_substrate_deposit_limit_respected() {
	use CallResources::{Ethereum, WeightDeposit};

	let gas_scale = <Test as Config>::GasScale::get().into();

	ExtBuilder::default()
		.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
		.build()
		.execute_with(|| {
			let eth_tx_info = EthTxInfo::<Test>::new(100, Weight::from_parts(1_000_000_000, 2_000));
			let root = TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
				eth_gas_limit: 5_000_000_000u128.div_ceil(gas_scale),
				weight_limit: Weight::MAX,
				eth_tx_info,
			})
			.unwrap();

			// 1. Substrate call sets weight = 10_000, deposit_limit = 1000.
			let deposit_limit = 1_000u128;
			let mut parent = root
				.new_nested(
					&WeightDeposit { weight: Weight::from_parts(10_000, 0), deposit_limit },
					eip_150::Rule::Skip,
				)
				.unwrap();

			assert_eq!(parent.deposit_left().unwrap(), deposit_limit);

			// 2. Ethereum contract makes two calls, each with half the gas.
			let remaining_gas = parent.eth_gas_left().unwrap();
			let child_gas = remaining_gas / 2;

			// 3. First child uses 700 deposit (more than the ~500 ratio scaling would allow, but
			//    within the total budget).
			let mut child1 = parent
				.new_nested(&Ethereum { gas: child_gas, add_stipend: false }, eip_150::Rule::Skip)
				.unwrap();

			child1.charge_deposit(&StorageDeposit::Charge(700)).unwrap();
			parent.absorb_all_meters(child1, &ALICE, None);

			assert_eq!(parent.deposit_left().unwrap(), 300);

			// 4. Second child uses the remaining 300 deposit.
			let mut child2 = parent
				.new_nested(&Ethereum { gas: child_gas, add_stipend: false }, eip_150::Rule::Skip)
				.unwrap();

			child2.charge_deposit(&StorageDeposit::Charge(300)).unwrap();
			parent.absorb_all_meters(child2, &ALICE, None);

			// Total deposit = 700 + 300 = 1000 = deposit_limit. Both calls succeeded.
			assert_eq!(parent.deposit_left().unwrap(), 0);
		});
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
			None,
			true,
			None,
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
			None,
			true,
			None,
		);

		assert_ok!(second_estimate);

		let make_call = |eth_gas_limit: u128| {
			builder::bare_call(test_address)
				.data(
					CatchConstructorTest::tryCatchNewContractCall { _owner: [0u8; 20].into() }
						.abi_encode(),
				)
				.transaction_limits(crate::TransactionLimits::EthereumGas {
					eth_gas_limit: eth_gas_limit.into(),
					weight_limit: Weight::MAX,
					eth_tx_info: crate::EthTxInfo::new(0, Default::default()),
				})
				.build()
		};

		let results = make_call(u128::MAX);

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

#[test]
fn dry_run_bounded_execution_runs_out_of_gas() {
	use crate::evm::*;
	use pallet_revive_fixtures::Fibonacci;

	let (code, _) = compile_module_with_type("Fibonacci", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = crate::Pallet::<Test>::dry_run_eth_transact(
			GenericTransaction {
				from: Some(ALICE_ADDR),
				to: Some(addr),
				input: Fibonacci::fibCall { n: 100u64 }.abi_encode().into(),
				..Default::default()
			},
			None,
			true,
			None,
		);

		let err = result.expect_err("fib(100) should run out of gas");
		assert!(
			matches!(&err, crate::EthTransactError::Message(msg) if msg.contains("OutOfGas")),
			"expected OutOfGas error, got: {err:?}"
		);
	});
}

/// Regression test for proxy contract delegatecall with large deposit limits.
///
/// When deposit_left is very large (u128::MAX in production), remaining_gas becomes huge,
/// causing ratio = gas_limit / remaining_gas ≈ 0. This resulted in nested calls receiving
/// almost no weight. The fix caps remaining_gas to u64::MAX since Ethereum gas is u64.
#[test]
fn substrate_nesting_with_large_deposit_and_max_gas_request() {
	use super::math::substrate_execution;

	ExtBuilder::default()
		.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
		.build()
		.execute_with(|| {
			let weight_limit = Weight::from_parts(1_000_000_000, 10_000);
			let deposit_limit: u128 = u64::MAX as _;

			let mut root_meter =
				substrate_execution::new_root::<Test>(weight_limit, deposit_limit).unwrap();

			root_meter.charge_weight_token(TestToken(1000, 100)).unwrap();
			root_meter.charge_deposit(&StorageDeposit::Charge(1000)).unwrap();

			let weight_left_before = root_meter.weight_left().unwrap();
			let nested = root_meter
				.new_nested(
					&CallResources::Ethereum { gas: u64::MAX as _, add_stipend: false },
					eip_150::Rule::Skip,
				)
				.unwrap();

			let nested_weight_left = nested.weight_left().unwrap();
			assert!(nested_weight_left.eq(&weight_left_before));
		});
}
