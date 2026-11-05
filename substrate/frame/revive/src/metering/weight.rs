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

#[cfg(test)]
mod tests;

use crate::{Config, Error, limits::code::BASIC_BLOCK_SIZE, vm::evm::Halt, weights::WeightInfo};
use core::{marker::PhantomData, ops::ControlFlow};
use frame_support::{DefaultNoBound, weights::Weight};
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
pub struct EngineMeter<T: Config> {
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

	/// How much ref time each PolkaVM gas corresponds to.
	///
	/// It is the largest ref time per gas of nine benchmarks, each rounded up. All run only
	/// instructions that cost one gas each, so the slope of one iteration divided by its gas is
	/// the time of one gas:
	///
	/// - `instr_load_chain_<m>` for `m` in `0..8`: dependent 8 byte loads from memory that is not
	///   cached, over all memory a contract can address. The variants place the values at the 8
	///   offsets modulo 64 an 8 byte aligned host buffer can give them, so in one variant every
	///   load spans two cache lines. The slope of one iteration is divided by [`LOAD_CHAIN_GAS`].
	/// - `instr_indirect_jump`: indirect jumps through a random cycle of compiled basic blocks. The
	///   slope of one lap is divided by [`INDIRECT_JUMP_GAS`].
	pub fn ref_time_per_fuel() -> u64 {
		let per_gas = |weight: fn(u32) -> Weight, gas: u32| {
			weight(1).saturating_sub(weight(0)).ref_time().div_ceil(u64::from(gas))
		};
		let load_chains: [fn(u32) -> Weight; 8] = [
			T::WeightInfo::instr_load_chain_0,
			T::WeightInfo::instr_load_chain_1,
			T::WeightInfo::instr_load_chain_2,
			T::WeightInfo::instr_load_chain_3,
			T::WeightInfo::instr_load_chain_4,
			T::WeightInfo::instr_load_chain_5,
			T::WeightInfo::instr_load_chain_6,
			T::WeightInfo::instr_load_chain_7,
		];
		load_chains
			.into_iter()
			.map(|weight| per_gas(weight, LOAD_CHAIN_GAS))
			.fold(per_gas(T::WeightInfo::instr_indirect_jump, INDIRECT_JUMP_GAS), u64::max)
	}
}

/// Gas of one iteration of the `instr_load_chain_<m>` benchmarks.
///
/// An iteration is a basic block holding the loop condition, and a basic block holding
/// `LOAD_CHAIN_GAS - 3` dependent loads, the counter increment and the jump back. Every
/// instruction costs one gas. The second block has `BASIC_BLOCK_SIZE` instructions.
pub const LOAD_CHAIN_GAS: u32 = BASIC_BLOCK_SIZE + 1;

/// Gas of one lap of the `instr_indirect_jump` benchmark.
///
/// A lap is the loop condition, the counter increment, and `INDIRECT_JUMP_GAS - 2` indirect
/// jumps: one into a cycle of `INDIRECT_JUMP_GAS - 3` basic blocks and one out of each of them.
/// Every instruction costs one gas.
pub const INDIRECT_JUMP_GAS: u32 = 20_003;

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
	/// The overall weight limit of this weight meter.
	pub weight_limit: Weight,
	/// The current actual effective weight limit. Used to check whether the weight meter ran out
	/// of resources. This weight limit needs to be adapted whenever the metering runs in ethereum
	/// mode and there is a charge on the deposit meter.
	effective_weight_limit: Weight,
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
	pub fn new(weight_limit: Weight, stipend: Option<Weight>) -> Self {
		WeightMeter {
			weight_limit,
			effective_weight_limit: weight_limit,
			weight_consumed: Default::default(),
			weight_consumed_highest: stipend.unwrap_or_default(),
			engine_meter: EngineMeter::new(),
			_phantom: PhantomData,
			#[cfg(test)]
			tokens: Vec::new(),
		}
	}

	pub fn set_effective_weight_limit(&mut self, limit: Weight) {
		self.effective_weight_limit = limit;
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
	pub fn charge<Tok: Token<T>>(&mut self, token: Tok) -> Result<ChargedAmount, DispatchError> {
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
		let new_consumed = self.weight_consumed.saturating_add(amount);
		if new_consumed.any_gt(self.effective_weight_limit) {
			return Err(<Error<T>>::OutOfGas.into());
		}

		self.weight_consumed = new_consumed;
		Ok(ChargedAmount(amount))
	}

	/// Charge the specified token amount of weight or halt if not enough weight is left.
	#[inline]
	pub fn charge_or_halt<Tok: Token<T>>(
		&mut self,
		token: Tok,
	) -> ControlFlow<Halt, ChargedAmount> {
		self.charge(token)
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
	///
	/// The executor can leave its gas negative when it charges more than is left. All of the
	/// gas it was given is then consumed and `OutOfGas` is returned.
	pub fn sync_from_executor(&mut self, engine_fuel: polkavm::Gas) -> Result<(), DispatchError> {
		let fuel_left = u64::try_from(engine_fuel).ok();
		let weight_consumed = self.engine_meter.set_fuel(fuel_left.unwrap_or(0));

		self.weight_consumed.saturating_accrue(weight_consumed);
		if self.weight_consumed.any_gt(self.effective_weight_limit) {
			self.weight_consumed = self.effective_weight_limit;
			return Err(<Error<T>>::OutOfGas.into());
		}

		match fuel_left {
			Some(_) => Ok(()),
			None => Err(<Error<T>>::OutOfGas.into()),
		}
	}

	/// Hand over the gas metering responsibility from this meter to the executor.
	///
	/// Needs to be called when leaving a host function in order to calculate how much
	/// gas needs to be charged from the **executor**. It updates the last seen executor
	/// total value so that it is correct when `sync_from_executor` is called the next time.
	///
	/// It is important that this does **not** actually sync with the executor. That has
	/// to be done by the caller.
	pub fn sync_to_executor(&mut self) -> polkavm::Gas {
		self.engine_meter.sync_remaining_ref_time(self.weight_left().ref_time())
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

	pub fn consume_all(&mut self) {
		self.weight_consumed = self.effective_weight_limit;
	}

	/// Returns how much weight left from the initial budget.
	pub fn weight_left(&self) -> Weight {
		self.effective_weight_limit.saturating_sub(self.weight_consumed)
	}

	#[cfg(test)]
	pub fn tokens(&self) -> &[ErasedToken] {
		&self.tokens
	}

	#[cfg(test)]
	pub fn nested(&mut self, amount: Weight) -> Self {
		Self::new(self.weight_left().min(amount), None)
	}
}
