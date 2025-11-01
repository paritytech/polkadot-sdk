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
use crate::{vm::evm::Halt, weights::WeightInfo, Config, Error};
use core::{marker::PhantomData, ops::ControlFlow};
use frame_support::{weights::Weight, DefaultNoBound};
use sp_runtime::DispatchError;

#[cfg(test)]
use std::{any::Any, fmt::Debug};

#[derive(Debug, PartialEq, Eq)]
pub struct ChargedAmount(Weight);

impl ChargedAmount {
	pub fn amount(&self) -> Weight {
		self.0
	}
}

/// Meter for syncing the gas between the executor and the weight meter.
#[derive(DefaultNoBound)]
struct EngineMeter<T: Config> {
	fuel: u64,
	_phantom: PhantomData<T>,
}

impl<T: Config> EngineMeter<T> {
	/// Create a meter with the given fuel limit.
	fn new() -> Self {
		Self { fuel: 0, _phantom: PhantomData }
	}

	/// Set the fuel left to the given value.
	/// Returns the amount of Weight consumed since the last update.
	fn set_fuel(&mut self, fuel: u64) -> Weight {
		let consumed = self.fuel.saturating_sub(fuel).saturating_mul(Self::ref_time_per_fuel());
		self.fuel = fuel;
		Weight::from_parts(consumed, 0)
	}

	/// Charge the given amount of ref time.
	/// Returns the amount of fuel left.
	fn sync_remaining_ref_time(&mut self, remaining_ref_time: u64) -> polkavm::Gas {
		self.fuel = remaining_ref_time.saturating_div(Self::ref_time_per_fuel());
		self.fuel.try_into().unwrap_or(polkavm::Gas::MAX)
	}

	/// How much ref time does each PolkaVM gas correspond to.
	fn ref_time_per_fuel() -> u64 {
		let loop_iteration =
			T::WeightInfo::instr(1).saturating_sub(T::WeightInfo::instr(0)).ref_time();
		let empty_loop_iteration = T::WeightInfo::instr_empty_loop(1)
			.saturating_sub(T::WeightInfo::instr_empty_loop(0))
			.ref_time();
		loop_iteration.saturating_sub(empty_loop_iteration)
	}
}

/// Resource that needs to be synced to the executor.
///
/// Wrapped to make sure that the resource will be synced back to the executor.
#[must_use]
pub struct Syncable(polkavm::Gas);

impl From<Syncable> for polkavm::Gas {
	fn from(from: Syncable) -> Self {
		from.0
	}
}

#[cfg(not(test))]
pub trait TestAuxiliaries {}
#[cfg(not(test))]
impl<T> TestAuxiliaries for T {}

#[cfg(test)]
pub trait TestAuxiliaries: Any + Debug + PartialEq + Eq {}
#[cfg(test)]
impl<T: Any + Debug + PartialEq + Eq> TestAuxiliaries for T {}

/// This trait represents a token that can be used for charging `WeightMeter`.
/// There is no other way of charging it.
///
/// Implementing type is expected to be super lightweight hence `Copy` (`Clone` is added
/// for consistency). If inlined there should be no observable difference compared
/// to a hand-written code.
pub trait Token<T: Config>: Copy + Clone + TestAuxiliaries {
	/// Return the amount of weight that should be taken by this token.
	///
	/// This function should be really lightweight and must not fail. It is not
	/// expected that implementors will query the storage or do any kinds of heavy operations.
	///
	/// That said, implementors of this function still can run into overflows
	/// while calculating the amount. In this case it is ok to use saturating operations
	/// since on overflow they will return `max_value` which should consume all weight.
	fn weight(&self) -> Weight;

	/// Returns true if this token is expected to influence the lowest weight limit.
	fn influence_lowest_weight_limit(&self) -> bool {
		true
	}
}

/// A wrapper around a type-erased trait object of what used to be a `Token`.
#[cfg(test)]
pub struct ErasedToken {
	pub description: String,
	pub token: Box<dyn Any>,
}

#[derive(DefaultNoBound)]
pub struct WeightMeter<T: Config> {
	pub weight_limit: Option<Weight>,
	/// Amount of weight already consumed. Must be < `weight_limit`.
	weight_consumed: Weight,
	/// Due to `adjust_weight` and `nested` the `weight_consumed` can temporarily peak above its
	/// final value.
	weight_consumed_highest: Weight,
	/// The amount of resources that was consumed by the execution engine.
	/// We have to track it separately in order to avoid the loss of precision that happens when
	/// converting from ref_time to the execution engine unit.
	engine_meter: EngineMeter<T>,
	_phantom: PhantomData<T>,
	#[cfg(test)]
	tokens: Vec<ErasedToken>,
}

impl<T: Config> WeightMeter<T> {
	pub fn new(weight_limit: Option<Weight>) -> Self {
		WeightMeter {
			weight_limit,
			weight_consumed: Default::default(),
			weight_consumed_highest: Default::default(),
			engine_meter: EngineMeter::new(),
			_phantom: PhantomData,
			#[cfg(test)]
			tokens: Vec::new(),
		}
	}

	/// Absorb the remaining weight of a nested meter after we are done using it.
	pub fn absorb_nested(&mut self, nested: Self) {
		self.weight_consumed_highest = self
			.weight_consumed
			.saturating_add(nested.weight_required())
			.max(self.weight_consumed_highest);
		self.weight_consumed += nested.weight_consumed;
	}

	/// Account for used weight.
	///
	/// Amount is calculated by the given `token`.
	///
	/// Returns `OutOfGas` if there is not enough weight or addition of the specified
	/// amount of weight has lead to overflow.
	///
	/// NOTE that amount isn't consumed if there is not enough weight. This is considered
	/// safe because we always charge weight before performing any resource-spending action.
	#[inline]
	pub fn charge<Tok: Token<T>>(
		&mut self,
		token: Tok,
		weight_left: Weight,
	) -> Result<ChargedAmount, DispatchError> {
		#[cfg(test)]
		{
			// Unconditionally add the token to the storage.
			let erased_tok =
				ErasedToken { description: format!("{:?}", token), token: Box::new(token) };
			self.tokens.push(erased_tok);
		}
		let amount = token.weight();
		// It is OK to not charge anything on failure because we always charge _before_ we perform
		// any action
		if amount.any_gt(weight_left) {
			Err(<Error<T>>::OutOfGas)?;
		}

		self.weight_consumed = self.weight_consumed.saturating_add(amount);
		Ok(ChargedAmount(amount))
	}

	/// Charge the specified token amount of weight or halt if not enough weight is left.
	pub fn charge_or_halt<Tok: Token<T>>(
		&mut self,
		token: Tok,
		weight_left: Weight,
	) -> ControlFlow<Halt, ChargedAmount> {
		self.charge(token, weight_left)
			.map_or_else(|_| ControlFlow::Break(Error::<T>::OutOfGas.into()), ControlFlow::Continue)
	}

	/// Adjust a previously charged amount down to its actual amount.
	///
	/// This is when a maximum a priori amount was charged and then should be partially
	/// refunded to match the actual amount.
	pub fn adjust_weight<Tok: Token<T>>(&mut self, charged_amount: ChargedAmount, token: Tok) {
		if token.influence_lowest_weight_limit() {
			self.weight_consumed_highest = self.weight_required();
		}
		let adjustment = charged_amount.0.saturating_sub(token.weight());
		self.weight_consumed = self.weight_consumed.saturating_sub(adjustment);
	}

	/// Hand over the gas metering responsibility from the executor to this meter.
	///
	/// Needs to be called when entering a host function to update this meter with the
	/// gas that was tracked by the executor. It tracks the latest seen total value
	/// in order to compute the delta that needs to be charged.
	pub fn sync_from_executor(
		&mut self,
		engine_fuel: polkavm::Gas,
		weight_limit: Weight,
	) -> Result<(), DispatchError> {
		let weight_consumed = self
			.engine_meter
			.set_fuel(engine_fuel.try_into().map_err(|_| Error::<T>::OutOfGas)?);

		self.weight_consumed.saturating_accrue(weight_consumed);
		if self.weight_consumed.any_gt(weight_limit) {
			self.weight_consumed = weight_limit;
			Err(<Error<T>>::OutOfGas)?;
		}

		Ok(())
	}

	/// Hand over the gas metering responsibility from this meter to the executor.
	///
	/// Needs to be called when leaving a host function in order to calculate how much
	/// gas needs to be charged from the **executor**. It updates the last seen executor
	/// total value so that it is correct when `sync_from_executor` is called the next time.
	///
	/// It is important that this does **not** actually sync with the executor. That has
	/// to be done by the caller.
	pub fn sync_to_executor(&mut self, weight_left: Weight) -> polkavm::Gas {
		self.engine_meter.sync_remaining_ref_time(weight_left.ref_time())
	}

	/// Returns the amount of weight that is required to run the same call.
	///
	/// This can be different from `weight_spent` because due to `adjust_weight` the amount of
	/// spent weight can temporarily drop and be refunded later.
	pub fn weight_required(&self) -> Weight {
		self.weight_consumed_highest.max(self.weight_consumed)
	}

	/// Returns how much weight was spent
	pub fn weight_consumed(&self) -> Weight {
		self.weight_consumed
	}

	pub fn consume_all(&mut self, weight_limit: Weight) {
		self.weight_consumed = weight_limit;
	}

	/// Returns how much weight left from the initial budget.
	#[cfg(test)]
	pub fn weight_left(&self) -> Weight {
		self.weight_limit.unwrap().saturating_sub(self.weight_consumed)
	}

	#[cfg(test)]
	pub fn tokens(&self) -> &[ErasedToken] {
		&self.tokens
	}

	#[cfg(test)]
	pub fn nested(&mut self, amount: Weight) -> Self {
		Self::new(Some(self.weight_left().min(amount)))
	}
}

#[cfg(test)]
mod tests {
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
		let weight_meter = WeightMeter::<Test>::new(Some(Weight::from_parts(50000, 0)));
		assert_eq!(weight_meter.weight_left(), Weight::from_parts(50000, 0));
	}

	#[test]
	fn tracing() {
		let mut weight_meter = WeightMeter::<Test>::new(Some(Weight::from_parts(50000, 0)));
		assert!(!weight_meter.charge(SimpleToken(1), weight_meter.weight_left()).is_err());

		let mut tokens = weight_meter.tokens().iter();
		match_tokens!(tokens, SimpleToken(1),);
	}

	// This test makes sure that nothing can be executed if there is no weight.
	#[test]
	fn refuse_to_execute_anything_if_zero() {
		let mut weight_meter = WeightMeter::<Test>::new(Some(Weight::zero()));
		assert!(weight_meter.charge(SimpleToken(1), weight_meter.weight_left()).is_err());
	}

	/// Previously, passing a `Weight` of 0 to `nested` would consume all of the meter's current
	/// weight.
	///
	/// Now, a `Weight` of 0 means no weight for the nested call.
	#[test]
	fn nested_zero_weight_requested() {
		let test_weight = 50000.into();
		let mut weight_meter = WeightMeter::<Test>::new(Some(test_weight));
		let weight_for_nested_call = weight_meter.nested(0.into());

		assert_eq!(weight_meter.weight_left(), 50000.into());
		assert_eq!(weight_for_nested_call.weight_left(), 0.into())
	}

	#[test]
	fn nested_some_weight_requested() {
		let test_weight = 50000.into();
		let mut weight_meter = WeightMeter::<Test>::new(Some(test_weight));
		let weight_for_nested_call = weight_meter.nested(10000.into());

		assert_eq!(weight_meter.weight_consumed(), 0.into());
		assert_eq!(weight_for_nested_call.weight_left(), 10000.into())
	}

	#[test]
	fn nested_all_weight_requested() {
		let test_weight = Weight::from_parts(50000, 50000);
		let mut weight_meter = WeightMeter::<Test>::new(Some(test_weight));
		let weight_for_nested_call = weight_meter.nested(test_weight);

		assert_eq!(weight_meter.weight_consumed(), Weight::from_parts(0, 0));
		assert_eq!(weight_for_nested_call.weight_left(), 50_000.into())
	}

	#[test]
	fn nested_excess_weight_requested() {
		let test_weight = Weight::from_parts(50000, 50000);
		let mut weight_meter = WeightMeter::<Test>::new(Some(test_weight));
		let weight_for_nested_call = weight_meter.nested(test_weight + 10000.into());

		assert_eq!(weight_meter.weight_consumed(), Weight::from_parts(0, 0));
		assert_eq!(weight_for_nested_call.weight_left(), 50_000.into())
	}

	// Make sure that the weight meter does not charge in case of overcharge
	#[test]
	fn overcharge_does_not_charge() {
		let mut weight_meter = WeightMeter::<Test>::new(Some(Weight::from_parts(200, 0)));

		// The first charge is should lead to OOG.
		assert!(weight_meter.charge(SimpleToken(300), weight_meter.weight_left()).is_err());

		// The weight meter should still contain the full 200.
		assert!(weight_meter.charge(SimpleToken(200), weight_meter.weight_left()).is_ok());
	}

	// Charging the exact amount that the user paid for should be
	// possible.
	#[test]
	fn charge_exact_amount() {
		let mut weight_meter = WeightMeter::<Test>::new(Some(Weight::from_parts(25, 0)));
		assert!(!weight_meter.charge(SimpleToken(25), weight_meter.weight_left()).is_err());
	}
}
