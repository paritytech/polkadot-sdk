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

		let mut nested0 = meter.nested(Some(BalanceOf::<Test>::MAX));
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
		let mut nested1 = nested0.nested(Some(BalanceOf::<Test>::MAX));
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

mod eip_150 {
	use super::*;
	use crate::metering::math::eip_150::Rule;
	use pretty_assertions::assert_eq;

	#[test]
	fn call_without_subcalls_has_no_peak() {
		let mut meter = TestMeter::new(Some(100_000)).nested(Some(50_000));
		meter.record_charge(&Deposit::Charge(6300));

		// Without subcalls, a meter has no peak and requires only what it charged.
		assert_eq!(meter.max_charged(), Deposit::Charge(6300));
		assert_eq!(meter.eip_150_peak(), 0);
		assert_eq!(meter.deposit_required_with_eip_150(), Deposit::Charge(6300));
		// As the transaction's first call, nothing is kept back for it. As a nested call, its
		// caller keeps ceil(6300/63) = 100.
		assert_eq!(meter.eip_150_total_overhead(Rule::Skip), 0);
		assert_eq!(meter.eip_150_total_overhead(Rule::Apply), 100);
	}

	#[test]
	fn peak_includes_the_part_retained_during_a_subcall() {
		struct Case {
			description: &'static str,
			parent_charge: u128,
			child_charge: u128,
			max_charged: u128,
			peak: u128,
			overhead: u128,
		}

		let cases = [
			Case {
				description: "subcall charges",
				parent_charge: 1000,
				child_charge: 6300,
				max_charged: 7300,
				peak: 7400, // 1000 + 6300 + ceil(6300/63) = 7400
				overhead: 100,
			},
			Case {
				description: "subcall charges nothing",
				parent_charge: 5000,
				child_charge: 0,
				max_charged: 5000,
				peak: 5000,
				overhead: 0,
			},
		];

		for case in &cases {
			for subcall_reverts in [false, true] {
				let mut parent_meter = TestMeter::new(Some(100_000)).nested(Some(100_000));
				parent_meter.record_charge(&Deposit::Charge(case.parent_charge));
				let mut child_meter = parent_meter.nested(Some(50_000));
				child_meter.record_charge(&Deposit::Charge(case.child_charge));
				if subcall_reverts {
					parent_meter.absorb_only_max_charged(child_meter);
				} else {
					parent_meter.absorb(child_meter, &ALICE, None);
				}

				let context = format!("{}, subcall reverts: {subcall_reverts}", case.description);
				assert_eq!(
					parent_meter.max_charged(),
					Deposit::Charge(case.max_charged),
					"{context}",
				);
				assert_eq!(parent_meter.eip_150_peak(), case.peak, "{context}");
				assert_eq!(
					parent_meter.deposit_required_with_eip_150(),
					Deposit::Charge(case.peak),
					"{context}",
				);
				assert_eq!(
					parent_meter.eip_150_total_overhead(Rule::Skip),
					case.overhead,
					"{context}",
				);
			}
		}
	}

	#[test]
	fn retained_parts_add_up_across_levels() {
		for first_level_reverts in [false, true] {
			let mut first_call_meter = TestMeter::new(Some(1_000_000)).nested(Some(1_000_000));
			first_call_meter.record_charge(&Deposit::Charge(1000));

			let mut first_level_meter = first_call_meter.nested(Some(500_000));
			first_level_meter.record_charge(&Deposit::Charge(2000));

			let mut second_level_meter = first_level_meter.nested(Some(250_000));
			second_level_meter.record_charge(&Deposit::Charge(6300));

			// second_level_meter has no subcalls, so no peak; overhead = ceil(6300/63) = 100
			assert_eq!(second_level_meter.max_charged(), Deposit::Charge(6300));
			assert_eq!(second_level_meter.eip_150_peak(), 0);
			assert_eq!(second_level_meter.deposit_required_with_eip_150(), Deposit::Charge(6300));
			assert_eq!(second_level_meter.eip_150_total_overhead(Rule::Apply), 100);

			first_level_meter.absorb(second_level_meter, &ALICE, None);

			// first_level_meter: peak = 2000 + 6300 + 100 = 8400, max_charged = 8300
			// children_overhead = 100, own = ceil(8400/63) = 134, total = 234
			assert_eq!(first_level_meter.max_charged(), Deposit::Charge(8300));
			assert_eq!(first_level_meter.eip_150_peak(), 8400);
			assert_eq!(first_level_meter.deposit_required_with_eip_150(), Deposit::Charge(8400));
			assert_eq!(first_level_meter.eip_150_total_overhead(Rule::Apply), 234);

			// A reverted call keeps the peak of everything it did, so both paths give the same.
			if first_level_reverts {
				first_call_meter.absorb_only_max_charged(first_level_meter);
			} else {
				first_call_meter.absorb(first_level_meter, &ALICE, None);
			}

			// first_call_meter: peak = 1000 + 8300 + 234 = 9534, max_charged = 9300, overhead = 234
			let context = format!("first level reverts: {first_level_reverts}");
			assert_eq!(first_call_meter.max_charged(), Deposit::Charge(9300), "{context}");
			assert_eq!(first_call_meter.eip_150_peak(), 9534, "{context}");
			assert_eq!(
				first_call_meter.deposit_required_with_eip_150(),
				Deposit::Charge(9534),
				"{context}",
			);
			assert_eq!(first_call_meter.eip_150_total_overhead(Rule::Skip), 234, "{context}");
		}
	}

	#[test]
	fn peak_rises_when_a_later_subcall_exceeds_it() {
		let mut parent_meter = TestMeter::new(Some(1_000_000)).nested(Some(1_000_000));
		parent_meter.record_charge(&Deposit::Charge(500));

		// First child: 3000 deposit, overhead = ceil(3000/63) = 48
		let mut first_child_meter = parent_meter.nested(Some(500_000));
		first_child_meter.record_charge(&Deposit::Charge(3000));
		parent_meter.absorb(first_child_meter, &ALICE, None);
		// peak = 500 + 3000 + 48 = 3548, consumed = 3500
		assert_eq!(parent_meter.max_charged(), Deposit::Charge(3500));
		assert_eq!(parent_meter.eip_150_peak(), 3548);
		assert_eq!(parent_meter.eip_150_total_overhead(Rule::Skip), 48);

		parent_meter.record_charge(&Deposit::Charge(700));
		// consumed = 4200, past the peak (3548), so the overhead drops to 0
		assert_eq!(parent_meter.max_charged(), Deposit::Charge(4200));
		assert_eq!(parent_meter.eip_150_peak(), 3548);
		assert_eq!(parent_meter.eip_150_total_overhead(Rule::Skip), 0);

		// Second child: 1000 deposit, overhead = ceil(1000/63) = 16
		let mut second_child_meter = parent_meter.nested(Some(500_000));
		second_child_meter.record_charge(&Deposit::Charge(1000));
		parent_meter.absorb(second_child_meter, &ALICE, None);

		// peak = max(3548, 4200 + 1000 + 16) = 5216, max_charged = max(4200, 5200) = 5200
		assert_eq!(parent_meter.max_charged(), Deposit::Charge(5200));
		assert_eq!(parent_meter.eip_150_peak(), 5216);
		assert_eq!(parent_meter.deposit_required_with_eip_150(), Deposit::Charge(5216));
		assert_eq!(parent_meter.eip_150_total_overhead(Rule::Skip), 16);
	}

	#[test]
	fn peak_stays_when_a_later_subcall_needs_less() {
		let mut parent_meter = TestMeter::new(Some(1_000_000)).nested(Some(1_000_000));
		parent_meter.record_charge(&Deposit::Charge(500));

		// First child: 6300 deposit, peak = 500 + 6300 + ceil(6300/63) = 6900
		let mut first_child_meter = parent_meter.nested(Some(500_000));
		first_child_meter.record_charge(&Deposit::Charge(6300));
		parent_meter.absorb(first_child_meter, &ALICE, None);

		// Second child: 10 deposit, 6800 + 10 + ceil(10/63) = 6811, below the earlier peak
		let mut second_child_meter = parent_meter.nested(Some(500_000));
		second_child_meter.record_charge(&Deposit::Charge(10));
		parent_meter.absorb(second_child_meter, &ALICE, None);

		assert_eq!(parent_meter.max_charged(), Deposit::Charge(6810));
		assert_eq!(parent_meter.eip_150_peak(), 6900);
		assert_eq!(parent_meter.deposit_required_with_eip_150(), Deposit::Charge(6900));
		assert_eq!(parent_meter.eip_150_total_overhead(Rule::Skip), 90);
	}

	#[test]
	fn earlier_deposit_refund_pays_for_later_charges() {
		enum DepositChange {
			Caller(DepositOf<Test>),
			Callee(&'static [DepositOf<Test>]),
		}
		use DepositChange::{Callee, Caller};

		struct Case {
			description: &'static str,
			top_level: bool,
			limit: Option<u128>,
			changes: &'static [DepositChange],
			expected: u128,
		}

		/// Applies each of `case.changes` to `caller` or to a callee it calls, then checks the
		/// deposit `caller` reports as required.
		fn run_and_assert_case<S: State>(mut caller: RawMeter<Test, TestExt, S>, case: &Case) {
			for change in case.changes {
				match change {
					Caller(deposit) => caller.record_charge(deposit),
					Callee(deposits) => {
						let mut callee = caller.nested(caller.limit);
						for deposit in *deposits {
							callee.record_charge(deposit);
						}
						caller.absorb(callee, &ALICE, None);
					},
				}
			}
			let required = caller.deposit_required_with_eip_150();
			assert_eq!(required, Deposit::Charge(case.expected), "{}", case.description);
		}

		let cases = [
			Case {
				description: "caller refund pays for a later call's charge",
				top_level: false,
				limit: None,
				changes: &[Caller(Deposit::Refund(6300)), Callee(&[Deposit::Charge(6300)])],
				expected: 100,
			},
			Case {
				description: "refund from an earlier call pays for a later call's charge",
				top_level: false,
				limit: None,
				changes: &[Callee(&[Deposit::Refund(6300)]), Callee(&[Deposit::Charge(6300)])],
				expected: 100,
			},
			Case {
				description: "refund does not pay for a later call when a deposit limit is set",
				top_level: false,
				limit: Some(100_000),
				changes: &[Caller(Deposit::Refund(6300)), Callee(&[Deposit::Charge(6300)])],
				expected: 6400,
			},
			Case {
				description: "refund larger than a later charge plus the part the caller keeps",
				top_level: false,
				limit: None,
				changes: &[Caller(Deposit::Refund(10_000)), Callee(&[Deposit::Charge(6300)])],
				expected: 0,
			},
			Case {
				description: "callee refund does not pay for its own earlier charge",
				top_level: false,
				limit: None,
				changes: &[Callee(&[Deposit::Charge(6300), Deposit::Refund(6300)])],
				expected: 6400,
			},
			Case {
				description: "caller refund after a call does not pay for that call",
				top_level: false,
				limit: None,
				changes: &[Callee(&[Deposit::Charge(6300)]), Caller(Deposit::Refund(6300))],
				expected: 6400,
			},
			Case {
				description: "top-level refund pays for all of the first call's charge",
				top_level: true,
				limit: None,
				changes: &[Caller(Deposit::Refund(6300)), Callee(&[Deposit::Charge(6300)])],
				expected: 0,
			},
		];

		for case in &cases {
			let top_level_meter = TestMeter::new(case.limit);
			if case.top_level {
				run_and_assert_case(top_level_meter, case);
			} else {
				run_and_assert_case(top_level_meter.nested(case.limit), case);
			}
		}
	}
}
