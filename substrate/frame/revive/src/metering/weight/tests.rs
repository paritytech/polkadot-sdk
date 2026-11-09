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

use super::{Token, Weight, WeightMeter};
use crate::tests::Test;

/// A simple utility macro that helps to match against a
/// list of tokens.
macro_rules! match_tokens {
		($tokens_iter:ident,) => {
		};
		($tokens_iter:ident, $x:expr, $($rest:tt)*) => {
			{
				let next = ($tokens_iter).next().unwrap();
				let pattern = $x;

				// Note that we don't specify the type name directly in this macro,
				// we only have some expression $x of some type. At the same time, we
				// have an iterator of Box<dyn Any> and to downcast we need to specify
				// the type which we want downcast to.
				//
				// So what we do is we assign `_pattern_typed_next_ref` to a variable which has
				// the required type.
				//
				// Then we make `_pattern_typed_next_ref = token.downcast_ref()`. This makes
				// rustc infer the type `T` (in `downcast_ref<T: Any>`) to be the same as in $x.

				let mut _pattern_typed_next_ref = &pattern;
				_pattern_typed_next_ref = match next.token.downcast_ref() {
					Some(p) => {
						assert_eq!(p, &pattern);
						p
					}
					None => {
						panic!("expected type {} got {}", stringify!($x), next.description);
					}
				};
			}

			match_tokens!($tokens_iter, $($rest)*);
		};
	}

/// A trivial token that charges the specified number of weight units.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
struct SimpleToken(u64);
impl Token<Test> for SimpleToken {
	fn weight(&self) -> Weight {
		Weight::from_parts(self.0, 0)
	}
}

#[test]
fn it_works() {
	let weight_meter = WeightMeter::<Test>::new(Weight::from_parts(50000, 0), None);
	assert_eq!(weight_meter.weight_left(), Weight::from_parts(50000, 0));
}

#[test]
fn tracing() {
	let mut weight_meter = WeightMeter::<Test>::new(Weight::from_parts(50000, 0), None);
	assert!(!weight_meter.charge(SimpleToken(1)).is_err());

	let mut tokens = weight_meter.tokens().iter();
	match_tokens!(tokens, SimpleToken(1),);
}

// This test makes sure that nothing can be executed if there is no weight.
#[test]
fn refuse_to_execute_anything_if_zero() {
	let mut weight_meter = WeightMeter::<Test>::new(Weight::zero(), None);
	assert!(weight_meter.charge(SimpleToken(1)).is_err());
}

/// Previously, passing a `Weight` of 0 to `nested` would consume all of the meter's current
/// weight.
///
/// Now, a `Weight` of 0 means no weight for the nested call.
#[test]
fn nested_zero_weight_requested() {
	let test_weight = 50000.into();
	let mut weight_meter = WeightMeter::<Test>::new(test_weight, None);
	let weight_for_nested_call = weight_meter.nested(0.into());

	assert_eq!(weight_meter.weight_left(), 50000.into());
	assert_eq!(weight_for_nested_call.weight_left(), 0.into())
}

#[test]
fn nested_some_weight_requested() {
	let test_weight = 50000.into();
	let mut weight_meter = WeightMeter::<Test>::new(test_weight, None);
	let weight_for_nested_call = weight_meter.nested(10000.into());

	assert_eq!(weight_meter.weight_consumed(), 0.into());
	assert_eq!(weight_for_nested_call.weight_left(), 10000.into())
}

#[test]
fn nested_all_weight_requested() {
	let test_weight = Weight::from_parts(50000, 50000);
	let mut weight_meter = WeightMeter::<Test>::new(test_weight, None);
	let weight_for_nested_call = weight_meter.nested(test_weight);

	assert_eq!(weight_meter.weight_consumed(), Weight::from_parts(0, 0));
	assert_eq!(weight_for_nested_call.weight_left(), 50_000.into())
}

#[test]
fn nested_excess_weight_requested() {
	let test_weight = Weight::from_parts(50000, 50000);
	let mut weight_meter = WeightMeter::<Test>::new(test_weight, None);
	let weight_for_nested_call = weight_meter.nested(test_weight + 10000.into());

	assert_eq!(weight_meter.weight_consumed(), Weight::from_parts(0, 0));
	assert_eq!(weight_for_nested_call.weight_left(), 50_000.into())
}

// Make sure that the weight meter does not charge in case of overcharge
#[test]
fn overcharge_does_not_charge() {
	let mut weight_meter = WeightMeter::<Test>::new(Weight::from_parts(200, 0), None);

	// The first charge is should lead to OOG.
	assert!(weight_meter.charge(SimpleToken(300)).is_err());

	// The weight meter should still contain the full 200.
	assert!(weight_meter.charge(SimpleToken(200)).is_ok());
}

// Charging the exact amount that the user paid for should be
// possible.
#[test]
fn charge_exact_amount() {
	let mut weight_meter = WeightMeter::<Test>::new(Weight::from_parts(25, 0), None);
	assert!(!weight_meter.charge(SimpleToken(25)).is_err());
}

mod eip_150 {
	use super::*;
	use crate::metering::math::eip_150::Rule;

	#[test]
	fn call_without_subcalls_has_no_peak() {
		let mut meter = WeightMeter::<Test>::new(Weight::from_parts(10000, 5000), None);
		// Before any charge, the peak, the required weight and the overhead are all zero.
		assert_eq!(meter.eip_150_peak.get(), Weight::zero());
		assert_eq!(meter.weight_required(), Weight::zero());
		assert_eq!(meter.eip_150_total_overhead(Rule::Skip), Weight::zero());

		meter.charge(SimpleToken(6300)).unwrap();
		// Without subcalls, a meter has no peak and requires only what it consumed.
		assert_eq!(meter.eip_150_peak.get(), Weight::zero());
		assert_eq!(meter.weight_required(), Weight::from_parts(6300, 0));
		// As the transaction's first call, nothing is kept back for it. As a nested call, its
		// caller keeps ceil(6300/63) = 100.
		assert_eq!(meter.eip_150_total_overhead(Rule::Skip), Weight::zero());
		assert_eq!(meter.eip_150_total_overhead(Rule::Apply), Weight::from_parts(100, 0));
	}

	#[test]
	fn peak_includes_the_part_retained_during_a_subcall() {
		struct Case {
			description: &'static str,
			parent_consumption: u64,
			child_consumption: u64,
			weight_required: u64,
			peak: u64,
			overhead: u64,
		}

		let cases = [
			Case {
				description: "subcall consumes",
				parent_consumption: 1000,
				child_consumption: 6300,
				weight_required: 7300,
				peak: 7400, // 1000 + 6300 + ceil(6300/63) = 7400
				overhead: 100,
			},
			Case {
				description: "subcall consumes nothing",
				parent_consumption: 5000,
				child_consumption: 0,
				weight_required: 5000,
				peak: 5000,
				overhead: 0,
			},
		];

		for case in &cases {
			let mut parent_meter =
				WeightMeter::<Test>::new(Weight::from_parts(100_000, 50_000), None);
			parent_meter.charge(SimpleToken(case.parent_consumption)).unwrap();
			let mut child_meter =
				WeightMeter::<Test>::new(Weight::from_parts(50_000, 25_000), None);
			child_meter.charge(SimpleToken(case.child_consumption)).unwrap();
			parent_meter.absorb_nested(child_meter, Rule::Apply);

			let peak = Weight::from_parts(case.peak, 0);
			assert_eq!(
				parent_meter.weight_required(),
				Weight::from_parts(case.weight_required, 0),
				"{}",
				case.description,
			);
			assert_eq!(parent_meter.eip_150_peak.get(), peak, "{}", case.description);
			assert_eq!(parent_meter.weight_required_with_eip_150(), peak, "{}", case.description);
			assert_eq!(
				parent_meter.eip_150_total_overhead(Rule::Skip),
				Weight::from_parts(case.overhead, 0),
				"{}",
				case.description,
			);
		}
	}

	#[test]
	fn retained_parts_add_up_across_levels() {
		let mut first_call_meter =
			WeightMeter::<Test>::new(Weight::from_parts(1_000_000, 500_000), None);
		first_call_meter.charge(SimpleToken(1000)).unwrap();

		let mut first_level_meter =
			WeightMeter::<Test>::new(Weight::from_parts(500_000, 250_000), None);
		first_level_meter.charge(SimpleToken(2000)).unwrap();

		let mut second_level_meter =
			WeightMeter::<Test>::new(Weight::from_parts(250_000, 125_000), None);
		second_level_meter.charge(SimpleToken(6300)).unwrap();

		// second_level_meter is a leaf subcall: peak=0, overhead = ceil(6300/63) = 100
		assert_eq!(second_level_meter.eip_150_peak.get(), Weight::zero());
		assert_eq!(second_level_meter.weight_required(), Weight::from_parts(6300, 0));
		assert_eq!(
			second_level_meter.eip_150_total_overhead(Rule::Apply),
			Weight::from_parts(100, 0),
		);

		first_level_meter.absorb_nested(second_level_meter, Rule::Apply);

		// first_level_meter: peak_at_subcall = 2000 + 6300 + 100 = 8400
		assert_eq!(first_level_meter.eip_150_peak.get(), Weight::from_parts(8400, 0));
		assert_eq!(first_level_meter.weight_required(), Weight::from_parts(8300, 0));
		// children_overhead = 8400 - 8300 = 100, ceil(8400/63) = 134, total = 234
		assert_eq!(
			first_level_meter.eip_150_total_overhead(Rule::Apply),
			Weight::from_parts(234, 0),
		);

		first_call_meter.absorb_nested(first_level_meter, Rule::Apply);

		// first_call_meter: peak_at_subcall = 1000 + 8300 + 234 = 9534
		assert_eq!(first_call_meter.eip_150_peak.get(), Weight::from_parts(9534, 0));
		assert_eq!(first_call_meter.weight_required(), Weight::from_parts(9300, 0));
		assert_eq!(first_call_meter.weight_required_with_eip_150(), Weight::from_parts(9534, 0));
		// The transaction's first call: overhead = peak - weight_required = 234
		assert_eq!(first_call_meter.eip_150_total_overhead(Rule::Skip), Weight::from_parts(234, 0));
	}

	#[test]
	fn peak_rises_when_a_later_subcall_exceeds_it() {
		let mut parent_meter =
			WeightMeter::<Test>::new(Weight::from_parts(1_000_000, 500_000), None);
		parent_meter.charge(SimpleToken(500)).unwrap();

		// First child: 3000 weight, overhead = ceil(3000/63) = 48
		let mut first_child_meter =
			WeightMeter::<Test>::new(Weight::from_parts(500_000, 250_000), None);
		first_child_meter.charge(SimpleToken(3000)).unwrap();
		parent_meter.absorb_nested(first_child_meter, Rule::Apply);
		// peak = max(0, 500 + 3000 + ceil(3000/63)) = 3548, consumed = 3500
		assert_eq!(parent_meter.weight_required(), Weight::from_parts(3500, 0));
		assert_eq!(parent_meter.eip_150_peak.get(), Weight::from_parts(3548, 0));
		// The transaction's first call: overhead = peak - weight_required = 48
		assert_eq!(parent_meter.eip_150_total_overhead(Rule::Skip), Weight::from_parts(48, 0));

		parent_meter.charge(SimpleToken(700)).unwrap();
		// consumed = 4200, past the peak (3548), so the overhead drops to 0
		assert_eq!(parent_meter.weight_required(), Weight::from_parts(4200, 0));
		assert_eq!(parent_meter.eip_150_peak.get(), Weight::from_parts(3548, 0));
		assert_eq!(parent_meter.eip_150_total_overhead(Rule::Skip), Weight::zero());

		// Second child
		let mut second_child_meter =
			WeightMeter::<Test>::new(Weight::from_parts(500_000, 250_000), None);
		second_child_meter.charge(SimpleToken(1000)).unwrap();
		// Subcall leaf: no children, overhead = ceil(1000/63) = 16
		assert_eq!(second_child_meter.weight_required(), Weight::from_parts(1000, 0));
		assert_eq!(second_child_meter.eip_150_peak.get(), Weight::zero());
		assert_eq!(
			second_child_meter.eip_150_total_overhead(Rule::Apply),
			Weight::from_parts(16, 0),
		);

		parent_meter.absorb_nested(second_child_meter, Rule::Apply);

		// peak = max(3548, 4200 + 1000 + 16) = 5216
		assert_eq!(parent_meter.weight_required(), Weight::from_parts(5200, 0));
		assert_eq!(parent_meter.eip_150_peak.get(), Weight::from_parts(5216, 0));
		assert_eq!(parent_meter.weight_required_with_eip_150(), Weight::from_parts(5216, 0));
		// The transaction's first call: overhead = peak - weight_required = 16
		assert_eq!(parent_meter.eip_150_total_overhead(Rule::Skip), Weight::from_parts(16, 0));
	}

	#[test]
	fn peak_stays_when_a_later_subcall_needs_less() {
		let mut parent_meter =
			WeightMeter::<Test>::new(Weight::from_parts(1_000_000, 500_000), None);
		parent_meter.charge(SimpleToken(500)).unwrap();

		// First child: 6300 weight, peak = 500 + 6300 + ceil(6300/63) = 6900
		let mut first_child_meter =
			WeightMeter::<Test>::new(Weight::from_parts(500_000, 250_000), None);
		first_child_meter.charge(SimpleToken(6300)).unwrap();
		parent_meter.absorb_nested(first_child_meter, Rule::Apply);

		// Second child: 10 weight, 6800 + 10 + ceil(10/63) = 6811, below the earlier peak
		let mut second_child_meter =
			WeightMeter::<Test>::new(Weight::from_parts(500_000, 250_000), None);
		second_child_meter.charge(SimpleToken(10)).unwrap();
		parent_meter.absorb_nested(second_child_meter, Rule::Apply);

		assert_eq!(parent_meter.weight_required(), Weight::from_parts(6810, 0));
		assert_eq!(parent_meter.eip_150_peak.get(), Weight::from_parts(6900, 0));
		assert_eq!(parent_meter.weight_required_with_eip_150(), Weight::from_parts(6900, 0));
		assert_eq!(parent_meter.eip_150_total_overhead(Rule::Skip), Weight::from_parts(90, 0));
	}
}
