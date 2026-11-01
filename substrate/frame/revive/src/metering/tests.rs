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

use super::{
	SignedGas,
	math::eip_150::{self, BalanceEip150, Resource, WeightEip150},
};
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
			assert_eq!(
				input.apply_eip_150(eip_150::Rule::Apply),
				expected,
				"failed for input {input:?}"
			);
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
		assert_eq!(WeightEip150::apply(input), expected, "failed for input {input:?}");
	}
}

#[test]
fn test_eip_150_overhead_of_weight() {
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
		let overhead = WeightEip150::overhead(consumed);
		let required = consumed.saturating_add(overhead);
		let available_to_nested = WeightEip150::apply(required);

		assert_eq!(
			available_to_nested, consumed,
			"failed for consumed={consumed:?}: overhead={overhead:?}, required={required:?}, \
			available={available_to_nested:?}"
		);
	}
}

#[test]
fn test_gas_ratio() {
	use super::math::gas_ratio;

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
				gas_ratio::<Test>(gas_limit, remaining),
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
			(u128::MAX, u128::MAX - (1 << 122)),
		];

		for (input, expected) in test_cases {
			assert_eq!(BalanceEip150::<Test>::apply(input), expected, "failed for input {input}");
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
						authorization_deposit: Default::default(),
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
						authorization_deposit: Default::default(),
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
						authorization_deposit: Default::default(),
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

/// Gas amounts in this module are `GasScale` times Ethereum gas, the unit the meter computes in.
mod nesting {
	use super::*;
	use crate::FrameMeter;
	use sp_runtime::DispatchError;
	use test_case::test_case;

	/// Creates a frame meter under `eip_150_rule`, nested below a first frame for `Rule::Apply`.
	fn new_frame_meter(
		transaction_meter: &TransactionMeter<Test>,
		call_resources: &CallResources<Test>,
		eip_150_rule: eip_150::Rule,
	) -> Result<FrameMeter<Test>, DispatchError> {
		match eip_150_rule {
			eip_150::Rule::Skip => transaction_meter.new_nested(call_resources),
			eip_150::Rule::Apply => transaction_meter
				.new_nested(&CallResources::NoLimits)?
				.new_nested(call_resources),
		}
	}

	/// The caller's meter when it makes the nested call.
	struct CallerMeter {
		gas_limit: u128,
		extra_weight: Weight,
		weight_charged: Weight,
		/// Negative for a refund.
		deposit_charged: i128,
		/// The gas left after `extra_weight` and the charges.
		gas_left: u128,
	}

	/// What the callee's meter reports.
	struct CalleeMeter {
		gas_left: u128,
		weight_left: Weight,
		deposit_left: u128,
		total_consumed_gas: u128,
	}

	/// A nested call from `caller` with `call_resources`, and what the callee's meter reports
	/// without and with the 63/64 rule.
	struct CallCase {
		caller: CallerMeter,
		call_resources: CallResources<Test>,
		expected_without_rule: CalleeMeter,
		expected_with_rule: CalleeMeter,
	}

	/// A nested call from `caller` with `requested_gas`, and `steps` against the callee's meter.
	struct ChargesCase {
		caller: CallerMeter,
		requested_gas: u128,
		steps: Vec<Step>,
	}

	/// A charge against the callee's meter, and what the meter reports after it without and with
	/// the 63/64 rule, or `None` when the charge must fail.
	struct Step {
		charge: Charge,
		expected_without_rule: Option<CalleeMeter>,
		expected_with_rule: Option<CalleeMeter>,
	}

	const SMALL_CHARGES: CallerMeter = CallerMeter {
		gas_limit: 5_000_000_000,
		extra_weight: Weight::from_parts(1_000_000_000, 2_000),
		weight_charged: Weight::from_parts(1000, 1000),
		deposit_charged: 1000,
		gas_left: 2999992500,
	};

	const HIGH_REF_TIME: CallerMeter = CallerMeter {
		gas_limit: 5_000_000_000,
		extra_weight: Weight::from_parts(1_000_000_000, 2_000),
		weight_charged: Weight::from_parts(1000000000, 10000),
		deposit_charged: 50000,
		gas_left: 422112782,
	};

	const LARGE_REFUND: CallerMeter = CallerMeter {
		gas_limit: 5_000_000_000,
		extra_weight: Weight::from_parts(1_000_000_000, 3000),
		weight_charged: Weight::from_parts(2000, 100000),
		deposit_charged: -7000000000,
		gas_left: 708617665,
	};

	const HIGH_PROOF_SIZE: CallerMeter = CallerMeter {
		gas_limit: 5_000_000_000,
		extra_weight: Weight::from_parts(1_000_000_000, 3000),
		weight_charged: Weight::from_parts(2000, 10106),
		deposit_charged: 91452,
		gas_left: 4,
	};

	#[test_case(eip_150::Rule::Skip  ; "without the 63/64 rule")]
	#[test_case(eip_150::Rule::Apply ; "with the 63/64 rule")]
	fn substrate_nesting_works(eip_150_rule: eip_150::Rule) {
		use CallResources::{Ethereum, NoLimits, WeightDeposit};

		let gas_scale = <Test as Config>::GasScale::get().into();
		let cases = vec![
			// 0: NoLimits, small charges before the call.
			CallCase {
				caller: SMALL_CHARGES,
				call_resources: NoLimits,
				expected_without_rule: CalleeMeter {
					gas_left: 2999992500,
					weight_left: Weight::from_parts(1499996250, 10107),
					deposit_left: 599998500,
					total_consumed_gas: 2000007500,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 2953117620,
					weight_left: Weight::from_parts(1476558808, 9949),
					deposit_left: 590623523,
					total_consumed_gas: 2000007500,
				},
			},
			// 1: NoLimits, large ref_time charge before the call.
			CallCase {
				caller: HIGH_REF_TIME,
				call_resources: NoLimits,
				expected_without_rule: CalleeMeter {
					gas_left: 422112782,
					weight_left: Weight::from_parts(499874750, 1106),
					deposit_left: 84422556,
					total_consumed_gas: 4577887218,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 415517270,
					weight_left: Weight::from_parts(492064207, 1088),
					deposit_left: 83103453,
					total_consumed_gas: 4577887220,
				},
			},
			// 2: NoLimits, large deposit refund.
			CallCase {
				caller: LARGE_REFUND,
				call_resources: NoLimits,
				expected_without_rule: CalleeMeter {
					gas_left: 708617665,
					weight_left: Weight::from_parts(18999997750, 1857),
					deposit_left: 141723533,
					total_consumed_gas: 4291382335,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 697545520,
					weight_left: Weight::from_parts(18703122785, 1827),
					deposit_left: 139509102,
					total_consumed_gas: 4291382340,
				},
			},
			// 3: NoLimits, very large deposit refund.
			CallCase {
				caller: CallerMeter {
					gas_limit: 5_000_000_000,
					extra_weight: Weight::from_parts(1_000_000_000, 3000),
					weight_charged: Weight::from_parts(2000, 100000),
					deposit_charged: -70000000000,
					gas_left: 315708617665,
				},
				call_resources: NoLimits,
				expected_without_rule: CalleeMeter {
					gas_left: 315708617665,
					weight_left: Weight::from_parts(176499997750, 827611),
					deposit_left: 63141723533,
					total_consumed_gas: 0,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 310775670520,
					weight_left: Weight::from_parts(173742185285, 814679),
					deposit_left: 62155134102,
					total_consumed_gas: 0,
				},
			},
			// 4: WeightDeposit, the call's limits are above remaining.
			CallCase {
				caller: SMALL_CHARGES,
				call_resources: WeightDeposit {
					weight: Weight::from_parts(10000000000, 100000),
					deposit_limit: 1000000000,
				},
				expected_without_rule: CalleeMeter {
					gas_left: 2999992500,
					weight_left: Weight::from_parts(1499996250, 10107),
					deposit_left: 599998500,
					total_consumed_gas: 2000007500,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 2953117620,
					weight_left: Weight::from_parts(1476558808, 9949),
					deposit_left: 590623523,
					total_consumed_gas: 2000007500,
				},
			},
			// 5: WeightDeposit, ref_time limit applies.
			CallCase {
				caller: SMALL_CHARGES,
				call_resources: WeightDeposit {
					weight: Weight::from_parts(1000000000, 100000),
					deposit_limit: 1000000000,
				},
				expected_without_rule: CalleeMeter {
					gas_left: 2999992500,
					weight_left: Weight::from_parts(1000000000, 10107),
					deposit_left: 599998500,
					total_consumed_gas: 2000007500,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 2953117620,
					weight_left: Weight::from_parts(1000000000, 9949),
					deposit_left: 590623523,
					total_consumed_gas: 2000007500,
				},
			},
			// 6: WeightDeposit, proof_size limit below remaining but above 63/64 of it.
			CallCase {
				caller: SMALL_CHARGES,
				call_resources: WeightDeposit {
					weight: Weight::from_parts(10000000000, 10000),
					deposit_limit: 1000000000,
				},
				expected_without_rule: CalleeMeter {
					gas_left: 2999992500,
					weight_left: Weight::from_parts(1499996250, 10000),
					deposit_left: 599998500,
					total_consumed_gas: 2000007500,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 2953117620,
					weight_left: Weight::from_parts(1476558808, 9949),
					deposit_left: 590623523,
					total_consumed_gas: 2000007500,
				},
			},
			// 7: WeightDeposit, deposit limit applies.
			CallCase {
				caller: SMALL_CHARGES,
				call_resources: WeightDeposit {
					weight: Weight::from_parts(10000000000, 100000),
					deposit_limit: 100000000,
				},
				expected_without_rule: CalleeMeter {
					gas_left: 2999992500,
					weight_left: Weight::from_parts(1499996250, 10107),
					deposit_left: 100000000,
					total_consumed_gas: 2000007500,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 2953117620,
					weight_left: Weight::from_parts(1476558808, 9949),
					deposit_left: 100000000,
					total_consumed_gas: 2000007500,
				},
			},
			// 8: WeightDeposit, all of the call's limits apply.
			CallCase {
				caller: SMALL_CHARGES,
				call_resources: WeightDeposit {
					weight: Weight::from_parts(40000, 200),
					deposit_limit: 300000,
				},
				expected_without_rule: CalleeMeter {
					gas_left: 1580000,
					weight_left: Weight::from_parts(40000, 200),
					deposit_left: 300000,
					total_consumed_gas: 2000007500,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 1580000,
					weight_left: Weight::from_parts(40000, 200),
					deposit_left: 300000,
					total_consumed_gas: 2000007500,
				},
			},
			// 9: WeightDeposit, proof-size fee above the ref_time fee, the call's limits apply.
			CallCase {
				caller: CallerMeter {
					gas_limit: 4_000_000_000,
					extra_weight: Weight::from_parts(100_000_000, 3_000),
					weight_charged: Weight::from_parts(1000, 1000),
					deposit_charged: 100,
					gas_left: 2474120094,
				},
				call_resources: WeightDeposit {
					weight: Weight::from_parts(40000, 200),
					deposit_limit: 300000,
				},
				expected_without_rule: CalleeMeter {
					gas_left: 77793945,
					weight_left: Weight::from_parts(40000, 200),
					deposit_left: 300000,
					total_consumed_gas: 1525879906,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 77793945,
					weight_left: Weight::from_parts(40000, 200),
					deposit_left: 300000,
					total_consumed_gas: 1525879906,
				},
			},
			// 10: WeightDeposit, ref_time fee above the proof-size fee, the call's limits apply.
			CallCase {
				caller: CallerMeter {
					gas_limit: 4_000_000_000,
					extra_weight: Weight::from_parts(100_000_000, 3_000),
					weight_charged: Weight::from_parts(1800000000, 1000),
					deposit_charged: 100,
					gas_left: 199999000,
				},
				call_resources: WeightDeposit {
					weight: Weight::from_parts(40000, 200),
					deposit_limit: 300000,
				},
				expected_without_rule: CalleeMeter {
					gas_left: 1580000,
					weight_left: Weight::from_parts(40000, 200),
					deposit_left: 300000,
					total_consumed_gas: 3800001000,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 1580000,
					weight_left: Weight::from_parts(40000, 200),
					deposit_left: 300000,
					total_consumed_gas: 3800001000,
				},
			},
			// 11: Ethereum gas, requested gas above remaining.
			CallCase {
				caller: SMALL_CHARGES,
				call_resources: Ethereum { gas: 2999992501, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 2999992500,
					weight_left: Weight::from_parts(1499996250, 10107),
					deposit_left: 599998500,
					total_consumed_gas: 2000007500,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 2953117620,
					weight_left: Weight::from_parts(1476558808, 9949),
					deposit_left: 590623523,
					total_consumed_gas: 2000007500,
				},
			},
			// 12: Ethereum gas, requested gas just below remaining.
			CallCase {
				caller: SMALL_CHARGES,
				call_resources: Ethereum { gas: 2999992490, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 2999992490,
					weight_left: Weight::from_parts(1499996245, 10107),
					deposit_left: 599998498,
					total_consumed_gas: 2000007500,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 2953117620,
					weight_left: Weight::from_parts(1476558808, 9949),
					deposit_left: 590623523,
					total_consumed_gas: 2000007500,
				},
			},
			// 13: Ethereum gas, requested gas just below 63/64 of remaining.
			CallCase {
				caller: SMALL_CHARGES,
				call_resources: Ethereum { gas: 2953117610, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 2953117610,
					weight_left: Weight::from_parts(1476558805, 9984),
					deposit_left: 590623522,
					total_consumed_gas: 2000007500,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 2953117610,
					weight_left: Weight::from_parts(1476558805, 9949),
					deposit_left: 590623522,
					total_consumed_gas: 2000007500,
				},
			},
			// 14: Ethereum gas, requested gas far below 63/64 of remaining.
			CallCase {
				caller: HIGH_REF_TIME,
				call_resources: Ethereum { gas: 10000, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 10000,
					weight_left: Weight::from_parts(288823359, 0),
					deposit_left: 2000,
					total_consumed_gas: 4577887218,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 10000,
					weight_left: Weight::from_parts(288823359, 0),
					deposit_left: 2000,
					total_consumed_gas: 4577887218,
				},
			},
			// 15: Ethereum gas, large refund, requested gas just below remaining.
			CallCase {
				caller: LARGE_REFUND,
				call_resources: Ethereum { gas: 708617660, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 708617660,
					weight_left: Weight::from_parts(18999997747, 1857),
					deposit_left: 141723532,
					total_consumed_gas: 4291382335,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 697545520,
					weight_left: Weight::from_parts(18703122785, 1827),
					deposit_left: 139509102,
					total_consumed_gas: 4291382340,
				},
			},
			// 16: Ethereum gas, large refund, requested gas above remaining.
			CallCase {
				caller: LARGE_REFUND,
				call_resources: Ethereum { gas: 3157000000, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 708617665,
					weight_left: Weight::from_parts(18999997750, 1857),
					deposit_left: 141723533,
					total_consumed_gas: 4291382335,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 697545520,
					weight_left: Weight::from_parts(18703122785, 1827),
					deposit_left: 139509102,
					total_consumed_gas: 4291382340,
				},
			},
			// 17: Ethereum gas, large proof_size charge, requested gas above remaining.
			CallCase {
				caller: HIGH_PROOF_SIZE,
				call_resources: Ethereum { gas: 500, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 4,
					weight_left: Weight::from_parts(1499769120, 0),
					deposit_left: 0,
					total_consumed_gas: 4999999996,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 10,
					weight_left: Weight::from_parts(1476335227, 0),
					deposit_left: 0,
					total_consumed_gas: 5000000000,
				},
			},
			// 18: Ethereum gas, large proof_size charge, small requested gas above remaining.
			CallCase {
				caller: HIGH_PROOF_SIZE,
				call_resources: Ethereum { gas: 300, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 4,
					weight_left: Weight::from_parts(1499769120, 0),
					deposit_left: 0,
					total_consumed_gas: 4999999996,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 10,
					weight_left: Weight::from_parts(1476335227, 0),
					deposit_left: 0,
					total_consumed_gas: 5000000000,
				},
			},
			// 19: Ethereum gas, small requested gas, moderate proof_size charge.
			CallCase {
				caller: CallerMeter {
					gas_limit: 5_000_000_000,
					extra_weight: Weight::from_parts(1_000_000_000, 3000),
					weight_charged: Weight::from_parts(2000, 1010),
					deposit_charged: 91452,
					gas_left: 2999538240,
				},
				call_resources: Ethereum { gas: 300, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 300,
					weight_left: Weight::from_parts(150, 1232),
					deposit_left: 60,
					total_consumed_gas: 2000461760,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 300,
					weight_left: Weight::from_parts(150, 1232),
					deposit_left: 60,
					total_consumed_gas: 2000461760,
				},
			},
			// 20: Ethereum gas, proof-size fee just below the ref_time fee.
			CallCase {
				caller: CallerMeter {
					gas_limit: 5_000_000_000,
					extra_weight: Weight::from_parts(1_000_000_000, 3000),
					weight_charged: Weight::from_parts(2000, 2242),
					deposit_charged: 91452,
					gas_left: 2999538240,
				},
				call_resources: Ethereum { gas: 600, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 600,
					weight_left: Weight::from_parts(300, 0),
					deposit_left: 120,
					total_consumed_gas: 2000461760,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 600,
					weight_left: Weight::from_parts(300, 0),
					deposit_left: 120,
					total_consumed_gas: 2000461760,
				},
			},
			// 21: Ethereum gas, proof-size fee just above the ref_time fee.
			CallCase {
				caller: CallerMeter {
					gas_limit: 5_000_000_000,
					extra_weight: Weight::from_parts(1_000_000_000, 3000),
					weight_charged: Weight::from_parts(2000, 2243),
					deposit_charged: 91452,
					gas_left: 2999496464,
				},
				call_resources: Ethereum { gas: 600, add_stipend: false },
				expected_without_rule: CalleeMeter {
					gas_left: 600,
					weight_left: Weight::from_parts(21188, 0),
					deposit_left: 120,
					total_consumed_gas: 2000503536,
				},
				expected_with_rule: CalleeMeter {
					gas_left: 600,
					weight_left: Weight::from_parts(21188, 0),
					deposit_left: 120,
					total_consumed_gas: 2000503536,
				},
			},
		];

		for (i, CallCase { caller, call_resources, expected_without_rule, expected_with_rule }) in
			cases.into_iter().enumerate()
		{
			let expected = match eip_150_rule {
				eip_150::Rule::Skip => expected_without_rule,
				eip_150::Rule::Apply => expected_with_rule,
			};
			ExtBuilder::default()
				.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
				.build()
				.execute_with(|| {
					#[cfg(test)]
					let eth_tx_info = EthTxInfo::<Test>::new(100, caller.extra_weight);
					let mut transaction_meter =
						TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
							eth_gas_limit: caller.gas_limit.div_ceil(gas_scale),
							weight_limit: Weight::MAX,
							eth_tx_info: eth_tx_info.clone(),
							authorization_deposit: Default::default(),
						})
						.unwrap();

					transaction_meter
						.charge_deposit(
							&(if caller.deposit_charged >= 0 {
								StorageDeposit::Charge(caller.deposit_charged as u128)
							} else {
								StorageDeposit::Refund(-caller.deposit_charged as u128)
							}),
						)
						.unwrap();

					transaction_meter
						.charge_weight_token(TestToken(
							caller.weight_charged.ref_time(),
							caller.weight_charged.proof_size(),
						))
						.unwrap();

					assert_eq!(
						caller.gas_left.div_ceil(gas_scale),
						transaction_meter.eth_gas_left().unwrap(),
						"caller gas_left mismatch in test case {i}"
					);

					let scaled_call_resources = match call_resources {
						Ethereum { gas, add_stipend } => Ethereum {
							gas: (gas as BalanceOf<Test>).div_ceil(gas_scale),
							add_stipend,
						},
						_ => call_resources,
					};
					let nested =
						new_frame_meter(&transaction_meter, &scaled_call_resources, eip_150_rule)
							.unwrap();

					assert_eq!(
						expected.gas_left.div_ceil(gas_scale),
						nested.eth_gas_left().unwrap(),
						"gas_left mismatch in test case {i}"
					);
					assert_eq!(
						expected.weight_left,
						nested.weight_left().unwrap(),
						"weight_left mismatch in test case {i}"
					);
					assert_eq!(
						expected.deposit_left,
						nested.deposit_left().unwrap(),
						"deposit_left mismatch in test case {i}"
					);
					assert_eq!(
						expected.total_consumed_gas.div_ceil(gas_scale),
						nested.total_consumed_gas(),
						"total_consumed_gas mismatch in test case {i}"
					);
				});
		}
	}

	#[test_case(eip_150::Rule::Skip  ; "without the 63/64 rule")]
	#[test_case(eip_150::Rule::Apply ; "with the 63/64 rule")]
	fn substrate_nesting_charges_works(eip_150_rule: eip_150::Rule) {
		use Charge::{D, W};

		let gas_scale = <Test as Config>::GasScale::get().into();
		let cases = vec![
			// 0: The ref_time fee is the larger one, so proof size costs no gas.
			ChargesCase {
				caller: CallerMeter {
					gas_limit: 5_000_000_000,
					extra_weight: Weight::from_parts(1_000_000_000, 2_000),
					weight_charged: Weight::from_parts(1000, 100),
					deposit_charged: 1000,
					gas_left: 2999992500,
				},
				requested_gas: 1000,
				steps: vec![
					Step {
						charge: W(100, 100),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 800,
							weight_left: Weight::from_parts(400, 3042),
							deposit_left: 160,
							total_consumed_gas: 2000007700,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 800,
							weight_left: Weight::from_parts(400, 3042),
							deposit_left: 160,
							total_consumed_gas: 2000007700,
						}),
					},
					Step {
						charge: D(100),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 300,
							weight_left: Weight::from_parts(150, 3042),
							deposit_left: 60,
							total_consumed_gas: 2000008200,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 300,
							weight_left: Weight::from_parts(150, 3042),
							deposit_left: 60,
							total_consumed_gas: 2000008200,
						}),
					},
				],
			},
			// 1: The proof-size fee becomes the larger one.
			// All gas is used, then a refund adds gas back.
			ChargesCase {
				caller: CallerMeter {
					gas_limit: 5_000_000_000,
					extra_weight: Weight::from_parts(419_615_482, 2_000),
					weight_charged: Weight::from_parts(1000, 100),
					deposit_charged: 100,
					gas_left: 4160766036,
				},
				requested_gas: 1000,
				steps: vec![
					Step {
						charge: W(100, 100),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 566,
							weight_left: Weight::from_parts(400, 0),
							deposit_left: 113,
							total_consumed_gas: 839234398,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 566,
							weight_left: Weight::from_parts(400, 0),
							deposit_left: 113,
							total_consumed_gas: 839234398,
						}),
					},
					Step {
						charge: W(100, 0),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 566,
							weight_left: Weight::from_parts(300, 0),
							deposit_left: 113,
							total_consumed_gas: 839234398,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 566,
							weight_left: Weight::from_parts(300, 0),
							deposit_left: 113,
							total_consumed_gas: 839234398,
						}),
					},
					Step {
						charge: D(100),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 66,
							weight_left: Weight::from_parts(50, 0),
							deposit_left: 13,
							total_consumed_gas: 839234898,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 66,
							weight_left: Weight::from_parts(50, 0),
							deposit_left: 13,
							total_consumed_gas: 839234898,
						}),
					},
					Step {
						charge: W(50, 0),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 0,
							weight_left: Weight::from_parts(0, 0),
							deposit_left: 0,
							total_consumed_gas: 839234964,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 0,
							weight_left: Weight::from_parts(0, 0),
							deposit_left: 0,
							total_consumed_gas: 839234964,
						}),
					},
					Step {
						charge: D(-300),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 1500,
							weight_left: Weight::from_parts(750, 0),
							deposit_left: 300,
							total_consumed_gas: 839233464,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 1500,
							weight_left: Weight::from_parts(750, 0),
							deposit_left: 300,
							total_consumed_gas: 839233464,
						}),
					},
					Step {
						charge: W(50, 0),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 1400,
							weight_left: Weight::from_parts(700, 0),
							deposit_left: 280,
							total_consumed_gas: 839233564,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 1400,
							weight_left: Weight::from_parts(700, 0),
							deposit_left: 280,
							total_consumed_gas: 839233564,
						}),
					},
					Step { charge: W(0, 1), expected_without_rule: None, expected_with_rule: None },
				],
			},
			// 2: The proof-size fee is already the larger one, so ref_time costs no gas.
			ChargesCase {
				caller: CallerMeter {
					gas_limit: 5_000_000_000,
					extra_weight: Weight::from_parts(100_000_000, 2_000),
					weight_charged: Weight::from_parts(1000, 100),
					deposit_charged: 100,
					gas_left: 4198912575,
				},
				requested_gas: 10000000,
				steps: vec![
					Step {
						charge: D(100),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 9999500,
							weight_left: Weight::from_parts(305541962, 26),
							deposit_left: 1999900,
							total_consumed_gas: 801087925,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 9999500,
							weight_left: Weight::from_parts(305541962, 26),
							deposit_left: 1999900,
							total_consumed_gas: 801087925,
						}),
					},
					Step {
						charge: W(100, 0),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 9999500,
							weight_left: Weight::from_parts(305541862, 26),
							deposit_left: 1999900,
							total_consumed_gas: 801087925,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 9999500,
							weight_left: Weight::from_parts(305541862, 26),
							deposit_left: 1999900,
							total_consumed_gas: 801087925,
						}),
					},
					Step {
						charge: W(0, 20),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 2370105,
							weight_left: Weight::from_parts(305541862, 6),
							deposit_left: 474021,
							total_consumed_gas: 808717320,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 2370105,
							weight_left: Weight::from_parts(305541862, 6),
							deposit_left: 474021,
							total_consumed_gas: 808717320,
						}),
					},
				],
			},
			// 3: Charges within the callee's 63/64 share succeed.
			// Then a proof-size charge that fits without the rule fails with it.
			ChargesCase {
				caller: SMALL_CHARGES,
				requested_gas: 5_000_000_000,
				steps: vec![
					Step {
						charge: W(100, 100),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 2999992300,
							weight_left: Weight::from_parts(1499996150, 10007),
							deposit_left: 599998460,
							total_consumed_gas: 2000007700,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 2953117420,
							weight_left: Weight::from_parts(1476558708, 9849),
							deposit_left: 590623483,
							total_consumed_gas: 2000007700,
						}),
					},
					Step {
						charge: D(100),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 2999991800,
							weight_left: Weight::from_parts(1499995900, 10007),
							deposit_left: 599998360,
							total_consumed_gas: 2000008200,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 2953116920,
							weight_left: Weight::from_parts(1476558458, 9849),
							deposit_left: 590623383,
							total_consumed_gas: 2000008200,
						}),
					},
					Step {
						charge: W(0, 10000),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 2740583,
							weight_left: Weight::from_parts(1499995900, 7),
							deposit_left: 548116,
							total_consumed_gas: 4997259417,
						}),
						expected_with_rule: None,
					},
				],
			},
			// 4: A deposit charge that fits without the rule fails with it.
			// Refunding the same amount brings back the starting values.
			ChargesCase {
				caller: SMALL_CHARGES,
				requested_gas: 5_000_000_000,
				steps: vec![
					Step {
						charge: D(595000000),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 24992500,
							weight_left: Weight::from_parts(12496250, 2308),
							deposit_left: 4998500,
							total_consumed_gas: 4975007500,
						}),
						expected_with_rule: None,
					},
					Step {
						charge: D(-595000000),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 2999992500,
							weight_left: Weight::from_parts(1499996250, 10107),
							deposit_left: 599998500,
							total_consumed_gas: 2000007500,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 2953117620,
							weight_left: Weight::from_parts(1476558808, 9949),
							deposit_left: 590623523,
							total_consumed_gas: 2000007500,
						}),
					},
				],
			},
			// 5: Charges within the callee's 63/64 share succeed.
			ChargesCase {
				caller: SMALL_CHARGES,
				requested_gas: 5_000_000_000,
				steps: vec![
					Step {
						charge: W(100, 100),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 2999992300,
							weight_left: Weight::from_parts(1499996150, 10007),
							deposit_left: 599998460,
							total_consumed_gas: 2000007700,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 2953117420,
							weight_left: Weight::from_parts(1476558708, 9849),
							deposit_left: 590623483,
							total_consumed_gas: 2000007700,
						}),
					},
					Step {
						charge: D(100),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 2999991800,
							weight_left: Weight::from_parts(1499995900, 10007),
							deposit_left: 599998360,
							total_consumed_gas: 2000008200,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 2953116920,
							weight_left: Weight::from_parts(1476558458, 9849),
							deposit_left: 590623383,
							total_consumed_gas: 2000008200,
						}),
					},
					Step {
						charge: W(100000000, 0),
						expected_without_rule: Some(CalleeMeter {
							gas_left: 2799991800,
							weight_left: Weight::from_parts(1399995900, 10007),
							deposit_left: 559998360,
							total_consumed_gas: 2200008200,
						}),
						expected_with_rule: Some(CalleeMeter {
							gas_left: 2753116920,
							weight_left: Weight::from_parts(1376558458, 9849),
							deposit_left: 550623383,
							total_consumed_gas: 2200008200,
						}),
					},
				],
			},
			// 6: A ref_time charge that fits without the rule fails with it.
			ChargesCase {
				caller: SMALL_CHARGES,
				requested_gas: 5_000_000_000,
				steps: vec![Step {
					charge: W(1490000000, 0),
					expected_without_rule: Some(CalleeMeter {
						gas_left: 19992500,
						weight_left: Weight::from_parts(9996250, 10107),
						deposit_left: 3998500,
						total_consumed_gas: 4980007500,
					}),
					expected_with_rule: None,
				}],
			},
		];

		for (i, ChargesCase { caller, requested_gas, steps }) in cases.into_iter().enumerate() {
			ExtBuilder::default()
				.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
				.build()
				.execute_with(|| {
					let eth_tx_info = EthTxInfo::<Test>::new(100, caller.extra_weight);
					let mut transaction_meter =
						TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
							eth_gas_limit: caller.gas_limit.div_ceil(gas_scale),
							weight_limit: Weight::MAX,
							eth_tx_info,
							authorization_deposit: Default::default(),
						})
						.unwrap();

					transaction_meter
						.charge_deposit(
							&(if caller.deposit_charged >= 0 {
								StorageDeposit::Charge(caller.deposit_charged as u128)
							} else {
								StorageDeposit::Refund((-caller.deposit_charged) as u128)
							}),
						)
						.unwrap();

					transaction_meter
						.charge_weight_token(TestToken(
							caller.weight_charged.ref_time(),
							caller.weight_charged.proof_size(),
						))
						.unwrap();

					assert_eq!(
						caller.gas_left.div_ceil(gas_scale),
						transaction_meter.eth_gas_left().unwrap(),
						"caller gas_left mismatch in test case {i}"
					);

					let mut nested = new_frame_meter(
						&transaction_meter,
						&CallResources::Ethereum {
							gas: requested_gas.div_ceil(gas_scale),
							add_stipend: false,
						},
						eip_150_rule,
					)
					.unwrap();

					for (j, Step { charge, expected_without_rule, expected_with_rule }) in
						steps.into_iter().enumerate()
					{
						let expected = match eip_150_rule {
							eip_150::Rule::Skip => expected_without_rule,
							eip_150::Rule::Apply => expected_with_rule,
						};
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

						if let Some(expected) = expected {
							assert!(is_ok, "charge failed in test case {i}, step {j}");
							assert_eq!(
								expected.gas_left.div_ceil(gas_scale),
								nested.eth_gas_left().unwrap(),
								"gas_left mismatch in test case {i}, step {j}"
							);
							assert_eq!(
								expected.weight_left,
								nested.weight_left().unwrap(),
								"weight_left mismatch in test case {i}, step {j}"
							);
							assert_eq!(
								expected.deposit_left,
								nested.deposit_left().unwrap(),
								"deposit_left mismatch in test case {i}, step {j}"
							);
							assert_eq!(
								expected.total_consumed_gas.div_ceil(gas_scale),
								nested.total_consumed_gas(),
								"total_consumed_gas mismatch in test case {i}, step {j}"
							);
						} else {
							assert!(!is_ok, "expected failure in test case {i}, step {j}");
						}
					}
				});
		}
	}
}

#[test]
fn ethereum_execution_failed_weight_charge_consumes_nothing() {
	use frame_support::assert_err;

	ExtBuilder::default().build().execute_with(|| {
		let root_meter = TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
			eth_gas_limit: u64::MAX.into(),
			weight_limit: crate::Pallet::<Test>::evm_max_extrinsic_weight(),
			eth_tx_info: EthTxInfo::new(0, Weight::zero()),
			authorization_deposit: Default::default(),
		})
		.unwrap();
		let mut meter = root_meter.new_nested(&CallResources::NoLimits).unwrap();
		let weight_left_before = meter.weight_left().unwrap();
		let gas_left_before = meter.eth_gas_left().unwrap();

		assert_err!(
			meter.charge_weight_token(TestToken(weight_left_before.ref_time() + 1, 0)),
			Error::<Test>::OutOfGas
		);
		assert_eq!(meter.weight_left().unwrap(), weight_left_before);
		assert_eq!(meter.eth_gas_left().unwrap(), gas_left_before);
	});
}

#[test]
fn ethereum_execution_failed_deposit_charge_stays_recorded() {
	use frame_support::assert_err;

	ExtBuilder::default().build().execute_with(|| {
		let root_meter = TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
			eth_gas_limit: u64::MAX.into(),
			weight_limit: Weight::MAX,
			eth_tx_info: EthTxInfo::new(0, Weight::zero()),
			authorization_deposit: Default::default(),
		})
		.unwrap();
		let mut meter = root_meter.new_nested(&CallResources::NoLimits).unwrap();
		let unaffordable_deposit = meter.deposit_left().unwrap() + 1;

		assert_err!(
			meter.charge_deposit(&StorageDeposit::Charge(unaffordable_deposit)),
			Error::<Test>::OutOfGas
		);
		assert_eq!(meter.deposit_consumed(), StorageDeposit::Charge(unaffordable_deposit));
		assert_eq!(meter.deposit_left(), None);
	});
}

/// Callees share their caller's deposit limit as one budget, not in proportion to their gas.
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
				authorization_deposit: Default::default(),
			})
			.unwrap();

			// 1. Substrate call sets weight = 10_000, deposit_limit = 1000.
			let deposit_limit = 1_000u128;
			let mut parent = root
				.new_nested(&WeightDeposit { weight: Weight::from_parts(10_000, 0), deposit_limit })
				.unwrap();

			assert_eq!(parent.deposit_left().unwrap(), deposit_limit);

			// 2. Ethereum contract makes two calls, each with half the gas.
			let remaining_gas = parent.eth_gas_left().unwrap();
			let child_gas = remaining_gas / 2;

			// 3. First child uses 680 deposit (more than the ~500 ratio scaling would allow, but
			//    within the total budget).
			let mut child1 =
				parent.new_nested(&Ethereum { gas: child_gas, add_stipend: false }).unwrap();

			child1.charge_deposit(&StorageDeposit::Charge(680)).unwrap();
			parent.absorb_all_meters(child1, &ALICE, None);

			assert_eq!(parent.deposit_left().unwrap(), 320);

			// 4. Second child can use 63/64 of the remaining 320 deposit.
			let mut child2 =
				parent.new_nested(&Ethereum { gas: child_gas, add_stipend: false }).unwrap();
			assert_eq!(child2.deposit_left().unwrap(), 315);

			child2.charge_deposit(&StorageDeposit::Charge(315)).unwrap();
			parent.absorb_all_meters(child2, &ALICE, None);

			// Total deposit = 680 + 315 = 995. The parent keeps 1/64 of what it had left.
			assert_eq!(parent.deposit_left().unwrap(), 5);
		});
}

/// The caller keeps 1/64 of its remaining weight when its weight limit is below what its gas buys.
#[test]
fn ethereum_execution_caller_keeps_weight_under_weight_limit() {
	ExtBuilder::default().build().execute_with(|| {
		let root_meter = TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
			eth_gas_limit: u64::MAX.into(),
			weight_limit: crate::Pallet::<Test>::evm_max_extrinsic_weight(),
			eth_tx_info: EthTxInfo::new(0, Weight::zero()),
			authorization_deposit: Default::default(),
		})
		.unwrap();
		let mut caller_meter = root_meter.new_nested(&CallResources::NoLimits).unwrap();
		let caller_charge = caller_meter.weight_left().unwrap() / 2;
		caller_meter
			.charge_weight_token(TestToken(caller_charge.ref_time(), caller_charge.proof_size()))
			.unwrap();
		let available_weight = caller_meter.weight_left().unwrap();
		let forwarded_weight = WeightEip150::apply(available_weight);
		let retained_weight = available_weight - forwarded_weight;

		let mut callee_meter = caller_meter.new_nested(&CallResources::NoLimits).unwrap();
		assert_eq!(callee_meter.weight_left().unwrap(), forwarded_weight);

		callee_meter.consume_all_weight();
		caller_meter.absorb_weight_meter_only(callee_meter);
		assert_eq!(caller_meter.weight_left().unwrap(), retained_weight);
	});
}

/// A callee of a call with a deposit limit can use at most 63/64 of the remaining deposit, so
/// its caller keeps 1/64.
#[test]
fn ethereum_execution_caller_keeps_deposit_under_deposit_limit() {
	ExtBuilder::default().build().execute_with(|| {
		let root_meter = TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
			eth_gas_limit: u64::MAX.into(),
			weight_limit: Weight::MAX,
			eth_tx_info: EthTxInfo::new(0, Weight::zero()),
			authorization_deposit: Default::default(),
		})
		.unwrap();
		let deposit_limit = 1_000;
		let mut caller_meter = root_meter
			.new_nested(&CallResources::WeightDeposit {
				weight: Weight::from_parts(10_000, 0),
				deposit_limit,
			})
			.unwrap();
		let available_deposit = caller_meter.deposit_left().unwrap();
		let forwarded_deposit = BalanceEip150::<Test>::apply(available_deposit);
		let retained_deposit = available_deposit - forwarded_deposit;

		let mut callee_meter = caller_meter.new_nested(&CallResources::NoLimits).unwrap();
		assert_eq!(callee_meter.deposit_left().unwrap(), forwarded_deposit);

		callee_meter.charge_deposit(&StorageDeposit::Charge(forwarded_deposit)).unwrap();
		caller_meter.absorb_all_meters(callee_meter, &ALICE, None);
		assert_eq!(caller_meter.deposit_left().unwrap(), retained_deposit);
	});
}

/// A callee's deposit limit stays within its caller's limit, even after the caller got a refund.
#[test]
fn substrate_execution_caller_refund_does_not_raise_callee_deposit_limit() {
	ExtBuilder::default().build().execute_with(|| {
		let deposit_limit = 1_000;
		let root_meter =
			TransactionMeter::<Test>::new_from_limits(Weight::MAX, deposit_limit).unwrap();
		let mut caller_meter = root_meter.new_nested(&CallResources::NoLimits).unwrap();
		caller_meter.charge_deposit(&StorageDeposit::Refund(100)).unwrap();

		let callee_meter = caller_meter.new_nested(&CallResources::NoLimits).unwrap();
		assert_eq!(callee_meter.deposit_left().unwrap(), deposit_limit);
	});
}

/// A callee's deposit limit stays within its caller's limit, even after the caller got a refund.
#[test]
fn ethereum_execution_caller_refund_does_not_raise_callee_deposit_limit() {
	ExtBuilder::default().build().execute_with(|| {
		let root_meter = TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
			eth_gas_limit: u64::MAX.into(),
			weight_limit: Weight::MAX,
			eth_tx_info: EthTxInfo::new(0, Weight::zero()),
			authorization_deposit: Default::default(),
		})
		.unwrap();
		let deposit_limit = 1_000;
		let mut caller_meter = root_meter
			.new_nested(&CallResources::WeightDeposit {
				weight: Weight::from_parts(10_000, 0),
				deposit_limit,
			})
			.unwrap();
		caller_meter.charge_deposit(&StorageDeposit::Refund(100)).unwrap();

		let callee_meter = caller_meter.new_nested(&CallResources::NoLimits).unwrap();
		assert_eq!(callee_meter.deposit_left().unwrap(), deposit_limit);
	});
}

/// The callee of a value call with no gas limit gets the stipend on top of its 63/64 share.
#[test]
fn ethereum_execution_stipend_comes_on_top() {
	use crate::evm::fees::InfoT;

	ExtBuilder::default().build().execute_with(|| {
		let stipend_weight = super::math::validate_and_get_stipend::<Test>(Weight::MAX).unwrap();
		let root_meter = TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
			eth_gas_limit: u64::MAX.into(),
			weight_limit: stipend_weight.saturating_mul(1000),
			eth_tx_info: EthTxInfo::new(0, Weight::zero()),
			authorization_deposit: Default::default(),
		})
		.unwrap();
		let caller_meter = root_meter.new_nested(&CallResources::NoLimits).unwrap();

		let plain_callee_meter = caller_meter
			.new_nested(&CallResources::Ethereum { gas: u64::MAX.into(), add_stipend: false })
			.unwrap();
		let value_callee_meter = caller_meter
			.new_nested(&CallResources::Ethereum { gas: u64::MAX.into(), add_stipend: true })
			.unwrap();

		assert_eq!(
			value_callee_meter.weight_left().unwrap(),
			plain_callee_meter.weight_left().unwrap() + stipend_weight,
		);
		let plain_callee_gas = plain_callee_meter.eth_gas_left().unwrap();
		let stipend_gas = SignedGas::<Test>::from_weight_fee(
			<Test as Config>::FeeInfo::weight_to_fee(&stipend_weight),
		)
		.to_ethereum_gas()
		.unwrap();
		let expected_gas = plain_callee_gas + stipend_gas;
		let value_callee_gas = value_callee_meter.eth_gas_left().unwrap();
		assert!(
			(expected_gas - 1..=expected_gas).contains(&value_callee_gas),
			"expected {expected_gas} or 1 less, got {value_callee_gas}",
		);
	});
}

/// The callee of a value call with no gas limit gets the stipend on top of its 63/64 share.
#[test]
fn substrate_execution_stipend_comes_on_top() {
	ExtBuilder::default().build().execute_with(|| {
		let stipend_weight = super::math::validate_and_get_stipend::<Test>(Weight::MAX).unwrap();
		let root_meter = TransactionMeter::<Test>::new_from_limits(
			stipend_weight.saturating_mul(1000),
			1_000_000,
		)
		.unwrap();
		let caller_meter = root_meter.new_nested(&CallResources::NoLimits).unwrap();

		let plain_callee_meter = caller_meter
			.new_nested(&CallResources::Ethereum { gas: u64::MAX.into(), add_stipend: false })
			.unwrap();
		let value_callee_meter = caller_meter
			.new_nested(&CallResources::Ethereum { gas: u64::MAX.into(), add_stipend: true })
			.unwrap();

		assert_eq!(
			value_callee_meter.weight_left().unwrap(),
			plain_callee_meter.weight_left().unwrap() + stipend_weight,
		);
	});
}

/// A value call succeeds when its caller has exactly one stipend left.
#[test]
fn ethereum_execution_one_stipend_left_is_enough() {
	ExtBuilder::default().build().execute_with(|| {
		let root_meter = TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
			eth_gas_limit: u64::MAX.into(),
			weight_limit: super::math::validate_and_get_stipend::<Test>(Weight::MAX).unwrap(),
			eth_tx_info: EthTxInfo::new(0, Weight::zero()),
			authorization_deposit: Default::default(),
		})
		.unwrap();
		let caller_meter = root_meter.new_nested(&CallResources::NoLimits).unwrap();

		let value_call =
			caller_meter.new_nested(&CallResources::Ethereum { gas: 0, add_stipend: true });
		assert!(value_call.is_ok());
	});
}

/// A value call succeeds when its caller has exactly one stipend left.
#[test]
fn substrate_execution_one_stipend_left_is_enough() {
	ExtBuilder::default().build().execute_with(|| {
		let stipend_weight = super::math::validate_and_get_stipend::<Test>(Weight::MAX).unwrap();
		let root_meter = TransactionMeter::<Test>::new_from_limits(stipend_weight, 1_000).unwrap();
		let caller_meter = root_meter.new_nested(&CallResources::NoLimits).unwrap();

		let value_callee_meter = caller_meter
			.new_nested(&CallResources::Ethereum { gas: 0, add_stipend: true })
			.unwrap();
		assert_eq!(value_callee_meter.weight_left().unwrap(), stipend_weight);
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
					authorization_deposit: Default::default(),
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
				.new_nested(&CallResources::Ethereum { gas: u64::MAX as _, add_stipend: false })
				.unwrap();

			let nested_weight_left = nested.weight_left().unwrap();
			assert!(nested_weight_left.eq(&weight_left_before));
		});
}

/// Regression test: a net-refund `authorization_deposit` (pure-revoke EIP-7702 tx) must increase
/// the available deposit budget, not be silently clamped to zero.
#[test]
fn authorization_deposit_refund_increases_budget() {
	let gas_scale: u128 = <Test as Config>::GasScale::get().into();
	let eth_gas_limit: BalanceOf<Test> = 5_000_000_000u128 / gas_scale;
	let eth_tx_info = EthTxInfo::<Test>::new(0, Weight::from_parts(1_000_000_000, 2_000));

	let mk = |auth: StorageDeposit<BalanceOf<Test>>| {
		ExtBuilder::default()
			.with_next_fee_multiplier(FixedU128::from_rational(1, 5))
			.build()
			.execute_with(|| {
				TransactionMeter::<Test>::new(TransactionLimits::EthereumGas {
					eth_gas_limit,
					weight_limit: Weight::MAX,
					eth_tx_info: eth_tx_info.clone(),
					authorization_deposit: auth,
				})
				.unwrap()
				.deposit_left()
				.unwrap()
			})
	};

	let baseline = mk(StorageDeposit::Charge(0));
	let charged = mk(StorageDeposit::Charge(1_000));
	let refunded = mk(StorageDeposit::Refund(1_000));

	assert_eq!(
		baseline.saturating_sub(charged),
		1_000,
		"charge must reduce budget by exactly the amount"
	);
	assert_eq!(
		refunded.saturating_sub(baseline),
		1_000,
		"refund must increase budget by exactly the amount"
	);
}
