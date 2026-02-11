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
use crate::{metering::math::EIP_150_NUMERATOR, tests::Test};

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

#[test]
fn eip_150_overhead_top_call() {
	// Fresh top-call meter with no consumption
	let meter = WeightMeter::<Test>::new(Weight::from_parts(10000, 5000), None);
	assert_eq!(meter.eip_150_peak(), Weight::zero());
	assert_eq!(meter.weight_required(), Weight::zero());
	assert_eq!(meter.compute_eip_150_total_overhead(), Weight::zero());

	// Top-call meter with consumption
	let mut top_call_meter = WeightMeter::<Test>::new(Weight::from_parts(10000, 5000), None);
	top_call_meter.charge(SimpleToken(6300)).unwrap();
	assert_eq!(top_call_meter.eip_150_peak(), Weight::zero());
	assert_eq!(top_call_meter.weight_required(), Weight::from_parts(6300, 0));
	// Top-call meter doesn't add overhead for its own consumption
	assert_eq!(top_call_meter.compute_eip_150_total_overhead(), Weight::zero());
}

#[test]
fn eip_150_overhead_subcall() {
	// Subcall leaf meter (no children): peak stays at zero
	let mut subcall_meter =
		WeightMeter::<Test>::new_with_eip_150(Weight::from_parts(10000, 5000), None);
	subcall_meter.charge(SimpleToken(6300)).unwrap();
	assert_eq!(subcall_meter.eip_150_peak(), Weight::zero());
	assert_eq!(subcall_meter.weight_required(), Weight::from_parts(6300, 0));
	// overhead = ceil(6300/63) = 100
	assert_eq!(subcall_meter.compute_eip_150_total_overhead(), Weight::from_parts(100, 0));
}

#[test]
fn eip_150_overhead_single_subcall() {
	let parent_consumption = 1000u64;
	let child_consumption = 6300u64;
	// Child is a leaf subcall (no children), so its overhead is ceil(6300/63) = 100.
	let child_overhead = child_consumption.div_ceil(EIP_150_NUMERATOR);
	// Parent consumed + child's weight_required = 1000 + 6300 = 7300.
	let parent_weight_required = parent_consumption + child_consumption;
	// Peak = parent consumed + child weight_required + child overhead = 1000 + 6300 + 100 = 7400.
	let expected_peak = parent_consumption + child_consumption + child_overhead;

	let mut parent = WeightMeter::<Test>::new(Weight::from_parts(100_000, 50_000), None);
	parent.charge(SimpleToken(parent_consumption)).unwrap();

	let mut child = WeightMeter::<Test>::new_with_eip_150(Weight::from_parts(50_000, 25_000), None);
	child.charge(SimpleToken(child_consumption)).unwrap();

	// Before absorb: parent is top-call with no children yet, peak and overhead are zero.
	assert_eq!(parent.eip_150_peak(), Weight::zero());
	assert_eq!(parent.weight_required(), Weight::from_parts(parent_consumption, 0));
	assert_eq!(parent.compute_eip_150_total_overhead(), Weight::zero());

	// Child is a leaf subcall: peak is zero (no children), overhead = ceil(6300/63) = 100.
	assert_eq!(child.eip_150_peak(), Weight::zero());
	assert_eq!(child.weight_required(), Weight::from_parts(child_consumption, 0));
	assert_eq!(child.compute_eip_150_total_overhead(), Weight::from_parts(child_overhead, 0));

	parent.absorb_nested(child);

	assert_eq!(parent.eip_150_peak(), Weight::from_parts(expected_peak, 0));
	assert_eq!(parent.weight_required(), Weight::from_parts(parent_weight_required, 0));
	assert_eq!(parent.weight_required_with_eip_150(), Weight::from_parts(expected_peak, 0));
	// Parent is top-call: overhead = peak - weight_required.
	assert_eq!(
		parent.compute_eip_150_total_overhead(),
		Weight::from_parts(expected_peak - parent_weight_required, 0)
	);
}

#[test]
fn eip_150_overhead_nested_two_levels() {
	let mut top_call_meter = WeightMeter::<Test>::new(Weight::from_parts(1_000_000, 500_000), None);
	top_call_meter.charge(SimpleToken(1000)).unwrap();

	let mut level1 =
		WeightMeter::<Test>::new_with_eip_150(Weight::from_parts(500_000, 250_000), None);
	level1.charge(SimpleToken(2000)).unwrap();

	let mut level2 =
		WeightMeter::<Test>::new_with_eip_150(Weight::from_parts(250_000, 125_000), None);
	level2.charge(SimpleToken(6300)).unwrap();

	// level2 is a leaf subcall: peak=0, overhead = ceil(6300/63) = 100
	assert_eq!(level2.eip_150_peak(), Weight::zero());
	assert_eq!(level2.weight_required(), Weight::from_parts(6300, 0));
	assert_eq!(level2.compute_eip_150_total_overhead(), Weight::from_parts(100, 0));

	level1.absorb_nested(level2);

	// level1: peak_at_subcall = 2000 + 6300 + 100 = 8400
	assert_eq!(level1.eip_150_peak(), Weight::from_parts(8400, 0));
	assert_eq!(level1.weight_required(), Weight::from_parts(8300, 0));
	// children_overhead = 8400 - 8300 = 100, ceil(8400/63) = 134, total = 234
	assert_eq!(level1.compute_eip_150_total_overhead(), Weight::from_parts(234, 0));

	top_call_meter.absorb_nested(level1);

	// top_call: peak_at_subcall = 1000 + 8300 + 234 = 9534
	assert_eq!(top_call_meter.eip_150_peak(), Weight::from_parts(9534, 0));
	assert_eq!(top_call_meter.weight_required(), Weight::from_parts(9300, 0));
	assert_eq!(top_call_meter.weight_required_with_eip_150(), Weight::from_parts(9534, 0));
	// TopCall: overhead = peak - weight_required = 234
	assert_eq!(top_call_meter.compute_eip_150_total_overhead(), Weight::from_parts(234, 0));
}
