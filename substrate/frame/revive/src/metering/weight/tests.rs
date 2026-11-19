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

use super::{EngineMeter, Token, Weight, WeightMeter, slope};
use crate::{Config, Error, tests::Test, weights::WeightInfo};
use frame_support::assert_err;

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

/// Negative gas left in the executor means it used all the gas it was given and more.
///
/// The proof size is not affected.
#[test]
fn negative_executor_gas_consumes_all_gas() {
	let mut weight_meter =
		WeightMeter::<Test>::new(Weight::from_parts(1_000_000_000, 1_000_000), None);
	let gas = u64::try_from(weight_meter.sync_to_executor()).unwrap();
	assert!(gas > 0);

	assert_err!(weight_meter.sync_from_executor(-1), Error::<Test>::OutOfGas);
	let ref_time_per_fuel = EngineMeter::<Test>::ref_time_per_fuel();
	assert_eq!(weight_meter.weight_consumed(), Weight::from_parts(gas * ref_time_per_fuel, 0));
	assert!(weight_meter.weight_left().ref_time() < ref_time_per_fuel);
}

/// The compilation cost is converted to gas and thousandths of gas, rounding up and saturating.
#[test]
fn compilation_cost_rounds_up() {
	let convert = EngineMeter::<Test>::compilation_cost_from_ref_time;
	let cost = |per_block, per_instruction_millis, per_code_byte_millis| {
		polkavm::InterpreterCompilationCost {
			per_block,
			per_instruction_millis,
			per_code_byte_millis,
		}
	};

	assert_eq!(convert(0, 0, 0, 7), cost(0, 0, 0));
	assert_eq!(convert(2000, 3, 5, 1000), cost(2, 3, 5));
	assert_eq!(convert(2001, 3, 5, 1000), cost(3, 3, 5));
	assert_eq!(convert(1, 1, 1, 3000), cost(1, 1, 1));
	assert_eq!(convert(5999, 5999, 5999, 3000), cost(2, 2000, 2000));
	assert_eq!(convert(6001, 6001, 6001, 3000), cost(3, 2001, 2001));
	assert_eq!(convert(u64::MAX, u64::MAX, u64::MAX, 1), cost(u32::MAX, u32::MAX, u32::MAX));
}

/// The per block compilation cost leaves out the instruction of the benchmarked blocks.
///
/// `interpreter_compile_per_block` compiles and runs blocks of one instruction. `per_block` is
/// its slope minus the slope of compiling one instruction and minus one gas for running it,
/// rounded up to whole gas: at least that rest and less than one gas above it.
#[test]
fn per_block_compilation_cost_leaves_out_the_instruction() {
	let ref_time_slope = |weight: fn(u32) -> Weight| slope(weight, 1).ref_time();
	let r = EngineMeter::<Test>::ref_time_per_fuel();
	let rest = ref_time_slope(<Test as Config>::WeightInfo::interpreter_compile_per_block) -
		ref_time_slope(<Test as Config>::WeightInfo::interpreter_compile_per_instruction) -
		r;
	let per_block = u64::from(EngineMeter::<Test>::interpreter_compilation_cost().per_block) * r;

	assert!(per_block >= rest, "{per_block} < {rest}");
	assert!(per_block < rest + r, "{per_block} >= {rest} + {r}");
}
