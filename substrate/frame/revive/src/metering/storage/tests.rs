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

use super::*;
use crate::{exec::AccountIdOf, test_utils::*, tests::Test};
use frame_support::parameter_types;
use pretty_assertions::assert_eq;

type TestMeter = RawMeter<Test, TestExt, Root>;

parameter_types! {
	static TestExtTestValue: TestExt = Default::default();
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct Charge {
	origin: AccountIdOf<Test>,
	contract: AccountIdOf<Test>,
	amount: DepositOf<Test>,
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct TestExt {
	charges: Vec<Charge>,
}

impl TestExt {
	fn clear(&mut self) {
		self.charges.clear();
	}
}

impl Ext<Test> for TestExt {
	fn charge(
		origin: &AccountIdOf<Test>,
		contract: &AccountIdOf<Test>,
		amount: &DepositOf<Test>,
		_exec_config: &ExecConfig<Test>,
	) -> Result<(), DispatchError> {
		TestExtTestValue::mutate(|ext| {
			ext.charges.push(Charge {
				origin: origin.clone(),
				contract: contract.clone(),
				amount: amount.clone(),
			})
		});
		Ok(())
	}
}

fn clear_ext() {
	TestExtTestValue::mutate(|ext| ext.clear())
}

struct ChargingTestCase {
	origin: Origin<Test>,
	deposit: DepositOf<Test>,
	expected: TestExt,
}

#[derive(Default)]
struct StorageInfo {
	bytes: u32,
	items: u32,
	bytes_deposit: BalanceOf<Test>,
	items_deposit: BalanceOf<Test>,
	immutable_data_len: u32,
}

fn new_info(info: StorageInfo) -> ContractInfo<Test> {
	ContractInfo::<Test> {
		trie_id: Default::default(),
		code_hash: Default::default(),
		storage_bytes: info.bytes,
		storage_items: info.items,
		storage_byte_deposit: info.bytes_deposit,
		storage_item_deposit: info.items_deposit,
		storage_base_deposit: Default::default(),
		immutable_data_len: info.immutable_data_len,
	}
}

#[test]
fn new_reserves_balance_works() {
	clear_ext();

	TestMeter::new(Some(1_000));

	assert_eq!(TestExtTestValue::get(), TestExt { ..Default::default() })
}

/// Previously, passing a limit of 0 meant unlimited storage for a nested call.
///
/// Now, a limit of 0 means the subcall will not be able to use any storage.
#[test]
fn nested_zero_limit_requested() {
	clear_ext();

	let meter = TestMeter::new(Some(1_000));
	assert_eq!(meter.available(), 1_000);
	let nested0 = meter.nested(Some(BalanceOf::<Test>::zero()));
	assert_eq!(nested0.available(), 0);
}

#[test]
fn nested_some_limit_requested() {
	clear_ext();

	let meter = TestMeter::new(Some(1_000));
	assert_eq!(meter.available(), 1_000);
	let nested0 = meter.nested(Some(500));
	assert_eq!(nested0.available(), 500);
}

#[test]
fn nested_all_limit_requested() {
	clear_ext();

	let meter = TestMeter::new(Some(1_000));
	assert_eq!(meter.available(), 1_000);
	let nested0 = meter.nested(Some(1_000));
	assert_eq!(nested0.available(), 1_000);
}

#[test]
fn nested_over_limit_requested() {
	clear_ext();

	let meter = TestMeter::new(Some(1_000));
	assert_eq!(meter.available(), 1_000);
	let nested0 = meter.nested(Some(2_000));
	assert_eq!(nested0.available(), 1_000);
}

#[test]
fn empty_charge_works() {
	clear_ext();

	let mut meter = TestMeter::new(Some(1_000));
	assert_eq!(meter.available(), 1_000);

	// an empty charge does not create a `Charge` entry
	let mut nested0 = meter.nested(Some(BalanceOf::<Test>::zero()));
	nested0.charge(&Default::default());
	meter.absorb(nested0, &BOB, None);
	assert_eq!(
		meter
			.execute_postponed_deposits(
				&Origin::<Test>::from_account_id(ALICE),
				&ExecConfig::new_substrate_tx(),
			)
			.unwrap(),
		Default::default()
	);
	assert_eq!(TestExtTestValue::get(), TestExt { ..Default::default() })
}

#[test]
fn charging_works() {
	let test_cases = vec![
		ChargingTestCase {
			origin: Origin::<Test>::from_account_id(ALICE),
			deposit: Deposit::Refund(28),
			expected: TestExt {
				charges: vec![
					Charge { origin: ALICE, contract: CHARLIE, amount: Deposit::Refund(30) },
					Charge { origin: ALICE, contract: BOB, amount: Deposit::Charge(2) },
				],
			},
		},
		ChargingTestCase {
			origin: Origin::<Test>::Root,
			deposit: Deposit::Charge(0),
			expected: TestExt { charges: vec![] },
		},
	];

	for test_case in test_cases {
		clear_ext();

		let mut meter = TestMeter::new(Some(100));
		assert_eq!(meter.consumed(), Default::default());
		assert_eq!(meter.available(), 100);

		let mut nested0_info = new_info(StorageInfo {
			bytes: 100,
			items: 5,
			bytes_deposit: 100,
			items_deposit: 10,
			immutable_data_len: 0,
		});
		let mut nested0 = meter.nested(Some(BalanceOf::<Test>::zero()));
		nested0.charge(&Diff {
			bytes_added: 108,
			bytes_removed: 5,
			items_added: 1,
			items_removed: 2,
		});
		assert_eq!(nested0.consumed(), Deposit::Charge(103));
		assert_eq!(nested0.available(), 0);
		nested0.charge(&Diff { bytes_removed: 99, ..Default::default() });
		assert_eq!(nested0.consumed(), Deposit::Charge(4));
		assert_eq!(nested0.available(), 0);

		let mut nested1_info = new_info(StorageInfo {
			bytes: 100,
			items: 10,
			bytes_deposit: 100,
			items_deposit: 20,
			immutable_data_len: 0,
		});
		let mut nested1 = nested0.nested(Some(BalanceOf::<Test>::zero()));
		nested1.charge(&Diff { items_removed: 5, ..Default::default() });
		assert_eq!(nested1.consumed(), Default::default());
		assert_eq!(nested1.available(), 0);
		nested1.finalize_own_contributions(Some(&mut nested1_info));
		assert_eq!(nested1.consumed(), Deposit::Refund(10));
		assert_eq!(nested1.available(), 10);

		nested0.absorb(nested1, &CHARLIE, Some(&mut nested1_info));
		assert_eq!(nested0.consumed(), Deposit::Refund(6));
		assert_eq!(nested0.available(), 6);

		let mut nested2_info = new_info(StorageInfo {
			bytes: 100,
			items: 7,
			bytes_deposit: 100,
			items_deposit: 20,
			immutable_data_len: 0,
		});
		let mut nested2 = nested0.nested(Some(BalanceOf::<Test>::zero()));
		nested2.charge(&Diff { items_removed: 7, ..Default::default() });
		assert_eq!(nested2.consumed(), Default::default());
		assert_eq!(nested2.available(), 0);
		nested2.finalize_own_contributions(Some(&mut nested2_info));
		assert_eq!(nested2.consumed(), Deposit::Refund(20));
		assert_eq!(nested2.available(), 20);

		nested0.absorb(nested2, &CHARLIE, Some(&mut nested2_info));
		assert_eq!(nested0.consumed(), Deposit::Refund(26));
		assert_eq!(nested0.available(), 26);

		nested0.finalize_own_contributions(Some(&mut nested0_info));
		assert_eq!(nested0.consumed(), Deposit::Refund(28));
		assert_eq!(nested0.available(), 28);

		meter.absorb(nested0, &BOB, Some(&mut nested0_info));
		assert_eq!(meter.consumed(), Deposit::Refund(28));
		assert_eq!(meter.available(), 128);

		assert_eq!(
			meter
				.execute_postponed_deposits(&test_case.origin, &ExecConfig::new_substrate_tx())
				.unwrap(),
			test_case.deposit
		);

		assert_eq!(nested0_info.extra_deposit(), 112);
		assert_eq!(nested1_info.extra_deposit(), 110);
		assert_eq!(nested2_info.extra_deposit(), 100);

		assert_eq!(TestExtTestValue::get(), test_case.expected)
	}
}

#[test]
fn termination_works() {
	let test_cases = vec![
		ChargingTestCase {
			origin: Origin::<Test>::from_account_id(ALICE),
			deposit: Deposit::Refund(108),
			expected: TestExt {
				charges: vec![Charge { origin: ALICE, contract: BOB, amount: Deposit::Charge(12) }],
			},
		},
		ChargingTestCase {
			origin: Origin::<Test>::Root,
			deposit: Deposit::Charge(0),
			expected: TestExt { charges: vec![] },
		},
	];

	for test_case in test_cases {
		clear_ext();

		let mut meter = TestMeter::new(Some(1_000));
		assert_eq!(meter.available(), 1_000);

		let mut nested0 = meter.nested(Some(BalanceOf::<Test>::max_value()));
		assert_eq!(nested0.available(), 1_000);

		nested0.charge(&Diff {
			bytes_added: 5,
			bytes_removed: 1,
			items_added: 3,
			items_removed: 1,
		});
		assert_eq!(nested0.consumed(), Deposit::Charge(8));

		nested0.charge(&Diff { items_added: 2, ..Default::default() });
		assert_eq!(nested0.consumed(), Deposit::Charge(12));

		let mut nested1_info = new_info(StorageInfo {
			bytes: 100,
			items: 10,
			bytes_deposit: 100,
			items_deposit: 20,
			immutable_data_len: 0,
		});
		let mut nested1 = nested0.nested(Some(BalanceOf::<Test>::max_value()));
		assert_eq!(nested1.consumed(), Default::default());
		let total_deposit = nested1_info.total_deposit();
		nested1.charge(&Diff { items_removed: 5, ..Default::default() });
		assert_eq!(nested1.consumed(), Default::default());
		nested1.charge(&Diff { bytes_added: 20, ..Default::default() });
		assert_eq!(nested1.consumed(), Deposit::Charge(20));
		nested1.finalize_own_contributions(Some(&mut nested1_info));
		assert_eq!(nested1.consumed(), Deposit::Charge(10));
		nested0.absorb(nested1, &CHARLIE, None);
		assert_eq!(nested0.consumed(), Deposit::Charge(22));

		meter.absorb(nested0, &BOB, None);
		assert_eq!(meter.consumed(), Deposit::Charge(22));

		meter.terminate(CHARLIE, total_deposit);
		assert_eq!(meter.consumed(), Deposit::Refund(98));
		assert_eq!(
			meter
				.execute_postponed_deposits(&test_case.origin, &ExecConfig::new_substrate_tx())
				.unwrap(),
			test_case.deposit
		);
		assert_eq!(TestExtTestValue::get(), test_case.expected)
	}
}

#[test]
fn max_deposits_work_with_charges() {
	clear_ext();
	let meter = TestMeter::new(None);
	let mut nested = meter.nested(None);

	assert_eq!(nested.consumed(), Default::default());
	assert_eq!(nested.max_charged(), Default::default());

	nested.record_charge(&Deposit::Charge(100));
	assert_eq!(nested.consumed(), Deposit::Charge(100));
	assert_eq!(nested.max_charged(), Deposit::Charge(100));

	nested.record_charge(&Deposit::Refund(50));
	assert_eq!(nested.consumed(), Deposit::Charge(50));
	assert_eq!(nested.max_charged(), Deposit::Charge(100));

	nested.record_charge(&Deposit::Charge(80));
	assert_eq!(nested.consumed(), Deposit::Charge(130));
	assert_eq!(nested.max_charged(), Deposit::Charge(130));

	nested.record_charge(&Deposit::Refund(200));
	assert_eq!(nested.consumed(), Deposit::Refund(70));
	assert_eq!(nested.max_charged(), Deposit::Charge(130));

	let meter = TestMeter::new(None);
	let mut nested = meter.nested(None);
	nested.record_charge(&Deposit::Refund(100));
	assert_eq!(nested.consumed(), Deposit::Refund(100));
	assert_eq!(nested.max_charged(), Default::default());

	nested.record_charge(&Deposit::Charge(100));
	assert_eq!(nested.consumed(), Default::default());
	assert_eq!(nested.max_charged(), Default::default());

	nested.record_charge(&Deposit::Charge(50));
	assert_eq!(nested.consumed(), Deposit::Charge(50));
	assert_eq!(nested.max_charged(), Deposit::Charge(50));

	nested.record_charge(&Deposit::Refund(20));
	assert_eq!(nested.consumed(), Deposit::Charge(30));
	assert_eq!(nested.max_charged(), Deposit::Charge(50));
}

#[test]
fn max_deposits_work_with_diffs() {
	clear_ext();
	let meter = TestMeter::new(None);
	let mut nested = meter.nested(None);

	nested.charge(&Diff { bytes_added: 2, ..Default::default() });

	assert_eq!(nested.consumed(), Deposit::Charge(2));
	assert_eq!(nested.max_charged(), Deposit::Charge(2));

	nested.charge(&Diff { bytes_removed: 1, ..Default::default() });
	assert_eq!(nested.consumed(), Deposit::Charge(1));
	assert_eq!(nested.max_charged(), Deposit::Charge(2));

	nested.charge(&Diff { items_added: 10, ..Default::default() });
	assert_eq!(nested.consumed(), Deposit::Charge(21));
	assert_eq!(nested.max_charged(), Deposit::Charge(21));

	nested.charge(&Diff { items_removed: 8, ..Default::default() });
	assert_eq!(nested.consumed(), Deposit::Charge(5));
	assert_eq!(nested.max_charged(), Deposit::Charge(21));

	nested.charge(&Diff { items_added: 10, bytes_added: 10, ..Default::default() });
	assert_eq!(nested.consumed(), Deposit::Charge(35));
	assert_eq!(nested.max_charged(), Deposit::Charge(35));

	nested.charge(&Diff { items_removed: 5, bytes_added: 10, ..Default::default() });
	assert_eq!(nested.consumed(), Deposit::Charge(35));
	assert_eq!(nested.max_charged(), Deposit::Charge(35));

	let meter = TestMeter::new(None);
	let mut nested = meter.nested(None);
	nested.charge(&Diff { bytes_removed: 10, items_added: 2, ..Default::default() });
	assert_eq!(nested.consumed(), Deposit::Charge(4));
	assert_eq!(nested.max_charged(), Deposit::Charge(4));

	nested.charge(&Diff { bytes_added: 5, items_removed: 3, ..Default::default() });
	assert_eq!(nested.consumed(), Default::default());
	assert_eq!(nested.max_charged(), Deposit::Charge(4));

	nested.charge(&Diff { bytes_added: 7, ..Default::default() });
	assert_eq!(nested.consumed(), Deposit::Charge(2));
	assert_eq!(nested.max_charged(), Deposit::Charge(4));

	nested.record_charge(&Deposit::Refund(10));
	assert_eq!(nested.consumed(), Deposit::Refund(8));
	assert_eq!(nested.max_charged(), Deposit::Charge(4));

	nested.charge(&Diff { bytes_removed: 4, items_added: 2, ..Default::default() });
	assert_eq!(nested.consumed(), Deposit::Refund(8));
	assert_eq!(nested.max_charged(), Deposit::Charge(4));

	nested.charge(&Diff { bytes_added: 20, ..Default::default() });
	assert_eq!(nested.consumed(), Deposit::Charge(10));
	assert_eq!(nested.max_charged(), Deposit::Charge(10));

	nested.record_charge(&Deposit::Refund(20));
	assert_eq!(nested.consumed(), Deposit::Refund(10));
	assert_eq!(nested.max_charged(), Deposit::Charge(10));
}

#[test]
fn max_deposits_work_nested() {
	clear_ext();
	let mut meter = TestMeter::new(None);
	let mut nested1 = meter.nested(None);
	nested1.record_charge(&Deposit::Charge(10));

	let mut nested2a = nested1.nested(None);
	nested2a.record_charge(&Deposit::Charge(20));
	nested2a.record_charge(&Deposit::Refund(10));
	assert_eq!(nested2a.consumed(), Deposit::Charge(10));
	assert_eq!(nested2a.max_charged(), Deposit::Charge(20));

	nested2a.charge(&Diff { bytes_removed: 20, items_removed: 10, ..Default::default() });
	assert_eq!(nested2a.consumed(), Deposit::Charge(10));
	assert_eq!(nested2a.max_charged(), Deposit::Charge(20));

	nested2a.charge(&Diff { bytes_added: 15, items_added: 16, ..Default::default() });
	assert_eq!(nested2a.consumed(), Deposit::Charge(22));
	assert_eq!(nested2a.max_charged(), Deposit::Charge(22));

	let mut nested2a_info = new_info(StorageInfo {
		bytes: 100,
		items: 100,
		bytes_deposit: 100,
		items_deposit: 100,
		immutable_data_len: 0,
	});
	nested1.absorb(nested2a, &BOB, Some(&mut nested2a_info));
	assert_eq!(nested1.consumed(), Deposit::Charge(27));
	assert_eq!(nested1.max_charged(), Deposit::Charge(32));

	nested1.charge(&Diff { bytes_added: 10, ..Default::default() });
	assert_eq!(nested1.consumed(), Deposit::Charge(37));
	assert_eq!(nested1.max_charged(), Deposit::Charge(37));

	nested1.record_charge(&Deposit::Refund(10));
	assert_eq!(nested1.consumed(), Deposit::Charge(27));
	assert_eq!(nested1.max_charged(), Deposit::Charge(37));

	let mut nested2b = nested1.nested(None);
	nested2b.record_charge(&Deposit::Refund(10));
	assert_eq!(nested2b.consumed(), Deposit::Refund(10));
	assert_eq!(nested2b.max_charged(), Default::default());

	nested2b.charge(&Diff { bytes_added: 10, items_added: 10, ..Default::default() });
	assert_eq!(nested2b.consumed(), Deposit::Charge(20));
	assert_eq!(nested2b.max_charged(), Deposit::Charge(20));

	nested2b.charge(&Diff { bytes_removed: 20, items_removed: 20, ..Default::default() });
	assert_eq!(nested2b.consumed(), Deposit::Refund(10));
	assert_eq!(nested2b.max_charged(), Deposit::Charge(20));

	let mut nested2b_info = new_info(StorageInfo {
		bytes: 100,
		items: 100,
		bytes_deposit: 100,
		items_deposit: 100,
		immutable_data_len: 0,
	});
	nested1.absorb(nested2b, &BOB, Some(&mut nested2b_info));
	assert_eq!(nested1.consumed(), Deposit::Refund(3));
	assert_eq!(nested1.max_charged(), Deposit::Charge(47));

	meter.absorb(nested1, &ALICE, None);
	assert_eq!(meter.consumed(), Deposit::Refund(3));
	assert_eq!(meter.max_charged(), Deposit::Charge(47));
}

#[test]
fn max_deposits_work_for_reverts() {
	clear_ext();
	let mut meter = TestMeter::new(None);
	let mut nested1 = meter.nested(None);
	nested1.record_charge(&Deposit::Charge(10));

	meter.absorb_only_max_charged(nested1);
	assert_eq!(meter.max_charged(), Deposit::Charge(10));
}
