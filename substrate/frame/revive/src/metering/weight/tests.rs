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

#[test]
fn eip_150_overhead_root_meter() {
	// Fresh root meter with no consumption
	let meter = WeightMeter::<Test>::new(Weight::from_parts(10000, 5000), None);
	assert_eq!(meter.compute_eip_150_total_overhead(), Weight::zero());

	// Root meter with consumption
	let mut root_meter = WeightMeter::<Test>::new(Weight::from_parts(10000, 5000), None);
	root_meter.charge(SimpleToken(6300)).unwrap();
	// Root meter doesn't add overhead for its own consumption
	let overhead = root_meter.compute_eip_150_total_overhead();
	assert_eq!(overhead, Weight::zero());
}

#[test]
fn eip_150_overhead_nested_meter() {
	// Nested meter with consumption
	let mut nested_meter = WeightMeter::<Test>::new_nested(Weight::from_parts(10000, 5000), None);
	nested_meter.charge(SimpleToken(6300)).unwrap();
	// total_required = 6300 + 0 = 6300, overhead = ceil(6300/63) = 100
	let overhead = nested_meter.compute_eip_150_total_overhead();
	assert_eq!(overhead, Weight::from_parts(100, 0));
}

#[test]
fn eip_150_overhead_single_nested() {
	let mut parent = WeightMeter::<Test>::new(Weight::from_parts(100_000, 50_000), None);
	parent.charge(SimpleToken(1000)).unwrap();

	let mut child = WeightMeter::<Test>::new_nested(Weight::from_parts(50_000, 25_000), None);
	child.charge(SimpleToken(6300)).unwrap();

	// Before absorb: parent is root, so overhead = 0
	assert_eq!(parent.compute_eip_150_total_overhead(), Weight::zero());

	// Child is nested: overhead = ceil(6300/63) = 100
	assert_eq!(child.compute_eip_150_total_overhead(), Weight::from_parts(100, 0));

	parent.absorb_nested(child);

	// Parent is root, so doesn't add its own overhead
	let overhead = parent.compute_eip_150_total_overhead();
	assert_eq!(overhead, Weight::from_parts(100, 0));
}

#[test]
fn eip_150_overhead_nested_two_levels() {
	let mut root = WeightMeter::<Test>::new(Weight::from_parts(1_000_000, 500_000), None);
	root.charge(SimpleToken(1000)).unwrap();

	let mut level1 = WeightMeter::<Test>::new_nested(Weight::from_parts(500_000, 250_000), None);
	level1.charge(SimpleToken(2000)).unwrap();

	let mut level2 = WeightMeter::<Test>::new_nested(Weight::from_parts(250_000, 125_000), None);
	level2.charge(SimpleToken(6300)).unwrap();

	// level2 is nested: overhead = ceil(6300/63) = 100
	assert_eq!(level2.compute_eip_150_total_overhead(), Weight::from_parts(100, 0));

	level1.absorb_nested(level2);

	// level1.weight_overhead = 100 (from level2)
	// level1 is nested: uses "required" = 8300 + 100 = 8400
	// level1 total = 100 + ceil(8400/63) = 100 + 134 = 234
	assert_eq!(level1.compute_eip_150_total_overhead(), Weight::from_parts(234, 0));

	root.absorb_nested(level1);

	// root.weight_overhead = 234 (from level1)
	let overhead = root.compute_eip_150_total_overhead();
	assert_eq!(overhead, Weight::from_parts(234, 0));
}

#[test]
fn eip_150_overhead_deep_nesting() {
	use crate::metering::math::apply_eip_150_to_weight;

	const DEPTH: usize = 16;
	const GAS_PER_LEVEL: u64 = 1000;

	// Create root meter with enough gas for deep nesting
	let mut root = WeightMeter::<Test>::new(Weight::from_parts(100_000_000, 50_000_000), None);
	root.charge(SimpleToken(GAS_PER_LEVEL)).unwrap();

	// Build nested meters from level 1 to DEPTH
	let mut meters: Vec<WeightMeter<Test>> = Vec::with_capacity(DEPTH);
	let mut parent_weight_left = root.weight_left();

	for _ in 0..DEPTH {
		let mut nested = WeightMeter::<Test>::new_nested(parent_weight_left, None);
		nested.charge(SimpleToken(GAS_PER_LEVEL)).unwrap();
		parent_weight_left = nested.weight_left();
		meters.push(nested);
	}

	// Absorb meters from deepest to shallowest
	// meters[DEPTH-1] is the deepest, meters[0] is directly under root
	for i in (1..DEPTH).rev() {
		let child = meters.pop().unwrap();
		meters[i - 1].absorb_nested(child);
	}

	// Absorb the last one into root
	let level1 = meters.pop().unwrap();
	root.absorb_nested(level1);

	// Get the total overhead
	let overhead = root.compute_eip_150_total_overhead();

	// Total consumed = (DEPTH + 1) * GAS_PER_LEVEL (root + all nested levels)
	let total_consumed = (DEPTH as u64 + 1) * GAS_PER_LEVEL;
	assert_eq!(root.weight_required().ref_time(), total_consumed);

	// Verify the overhead is sufficient:
	// If user provides (consumed + overhead), the deepest level should get enough gas
	let user_provided = Weight::from_parts(total_consumed + overhead.ref_time(), 0);

	// Simulate the 63/64 reductions through all levels
	let mut available = user_provided;
	// Root consumes GAS_PER_LEVEL, then forwards the rest
	available = available.saturating_sub(Weight::from_parts(GAS_PER_LEVEL, 0));

	for level in 0..DEPTH {
		// Apply 63/64 rule
		available = apply_eip_150_to_weight(available);
		// Each nested level consumes GAS_PER_LEVEL
		let needed = Weight::from_parts(GAS_PER_LEVEL, 0);
		assert!(
			available.all_gte(needed),
			"Level {level} ran out of gas: available={available:?}, needed={needed:?}"
		);
		available = available.saturating_sub(needed);
	}

	// Verify overhead grows with depth (should be > 0 for deep nesting)
	assert!(overhead.ref_time() > 0, "Correction should be positive for deep nesting");
}
