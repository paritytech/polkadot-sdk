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
	BalanceOf, CallResources, Config, DispatchError, Error, EthTxInfo, FixedPointNumber, FixedU128,
	FrameMeter, InfoT, ResourceMeter, RootStorageMeter, SaturatedConversion, SignedGas, State,
	StorageDeposit, Token, TransactionLimits, TransactionMeter, Weight, WeightMeter,
};
use crate::vm::{RuntimeCosts, evm::EVMGas};
use core::{marker::PhantomData, num::NonZero};
use eip_150::{BalanceEip150, WeightEip150};
use num_traits::{One, Zero};
use revm::interpreter::gas::CALL_STIPEND;
use sp_runtime::Saturating;

/// EIP-150 63/64 gas rule helpers.
///
/// With `r` the parent's remaining resources and `s` the call stipend (zero without one), a
/// subcall receives at most `63/64 * (r - s) + s` and the parent keeps at least `1/64 * (r - s)`.
/// This module provides the [`Peak`] type for tracking the minimum starting
/// value a meter needs, as well as the [`Resource`] trait with the operations the rule needs.
pub mod eip_150 {
	use super::*;
	use core::fmt::Debug;
	use frame_support::{DebugNoBound, DefaultNoBound};

	/// Denominator of the 63/64 rule.
	pub const DENOMINATOR: NonZero<u64> = NonZero::new(64).expect("64 is not zero; qed");
	/// Numerator of the 63/64 rule; the caller keeps the one remaining part.
	pub const NUMERATOR: NonZero<u64> =
		NonZero::new(DENOMINATOR.get() - 1).expect("`DENOMINATOR` is greater than 1; qed");

	/// The minimum starting amount a call needs so that each of its subcalls still receives
	/// enough after the 63/64 split.
	#[derive(DebugNoBound, DefaultNoBound)]
	pub struct Peak<R: Resource> {
		/// The peak tracked from the children absorbed so far.
		value: R::Amount,
	}

	/// Whether the EIP-150 63/64 gas rule should be applied.
	#[derive(Copy, Clone, Debug)]
	pub enum Rule {
		Apply,
		Skip,
	}

	impl Rule {
		/// Returns the share of `amount` that a subcall can receive under this rule.
		pub fn apply<R: Resource>(self, amount: R::Amount) -> R::Amount {
			match self {
				Self::Apply => R::apply(amount),
				Self::Skip => amount,
			}
		}
	}

	impl<R: Resource> Peak<R> {
		/// Returns the tracked peak value.
		pub fn get(&self) -> R::Amount {
			self.value
		}

		/// Raises the peak to `new_value` when `new_value` is higher.
		pub fn update(&mut self, new_value: R::Amount) {
			self.value = R::max(self.value, new_value);
		}

		/// Returns the total EIP-150 63/64 overhead stored in this peak.
		///
		/// `required` is what the call and its subcalls needed, without the resources retained
		/// under the 63/64 rule, `rule` is the rule the call was created under, and `stipend` is
		/// what the call got on top of its 63/64 share.
		pub fn total_overhead(
			&self,
			required: R::Amount,
			rule: Rule,
			stipend: R::Amount,
		) -> R::Amount {
			let needed_at_boundary = R::max(self.value, required);
			let children_overhead = R::saturating_sub(needed_at_boundary, required);
			match rule {
				Rule::Skip => children_overhead,
				Rule::Apply => {
					// The stipend is set aside before the 63/64 rule, so it adds no overhead.
					let needed_beyond_stipend = R::saturating_sub(needed_at_boundary, stipend);
					R::saturating_add(children_overhead, R::overhead(needed_beyond_stipend))
				},
			}
		}
	}

	/// A resource that the 63/64 rule splits between a caller and its subcall.
	///
	/// Provides the operations the rule needs.
	pub trait Resource {
		/// The type the resource is measured in.
		type Amount: Copy + Default + Debug;

		/// Returns the maximum of `lhs` and `rhs`.
		fn max(lhs: Self::Amount, rhs: Self::Amount) -> Self::Amount;

		/// Returns `lhs + rhs`, saturating at the numeric bounds instead of overflowing.
		fn saturating_add(lhs: Self::Amount, rhs: Self::Amount) -> Self::Amount;

		/// Returns `lhs - rhs`, saturating at the numeric bounds instead of overflowing.
		fn saturating_sub(lhs: Self::Amount, rhs: Self::Amount) -> Self::Amount;

		/// Returns `amount / divisor`, rounded up.
		fn div_ceil(amount: Self::Amount, divisor: NonZero<u64>) -> Self::Amount;

		/// Returns the share of `amount` that a subcall can receive.
		fn apply(amount: Self::Amount) -> Self::Amount {
			Self::saturating_sub(amount, Self::div_ceil(amount, DENOMINATOR))
		}

		/// Returns what a caller needs on top of `amount` for its subcall to receive `amount`.
		fn overhead(amount: Self::Amount) -> Self::Amount {
			Self::div_ceil(amount, NUMERATOR)
		}
	}

	// Marker types, since Rust rejects implementing `Resource` for both `Weight` and
	// `BalanceOf<T>`.
	pub struct WeightEip150;
	pub struct BalanceEip150<T>(PhantomData<T>);

	/// Each operation works on `ref_time` and `proof_size` separately.
	impl Resource for WeightEip150 {
		type Amount = Weight;

		fn max(lhs: Weight, rhs: Weight) -> Weight {
			lhs.max(rhs)
		}

		fn saturating_add(lhs: Weight, rhs: Weight) -> Weight {
			lhs.saturating_add(rhs)
		}

		fn saturating_sub(lhs: Weight, rhs: Weight) -> Weight {
			lhs.saturating_sub(rhs)
		}

		fn div_ceil(amount: Weight, divisor: NonZero<u64>) -> Weight {
			Weight::from_parts(
				amount.ref_time().div_ceil(divisor.get()),
				amount.proof_size().div_ceil(divisor.get()),
			)
		}
	}

	impl<T: Config> Resource for BalanceEip150<T> {
		type Amount = BalanceOf<T>;

		fn max(lhs: BalanceOf<T>, rhs: BalanceOf<T>) -> BalanceOf<T> {
			lhs.max(rhs)
		}

		fn saturating_add(lhs: BalanceOf<T>, rhs: BalanceOf<T>) -> BalanceOf<T> {
			lhs.saturating_add(rhs)
		}

		fn saturating_sub(lhs: BalanceOf<T>, rhs: BalanceOf<T>) -> BalanceOf<T> {
			lhs.saturating_sub(rhs)
		}

		fn div_ceil(amount: BalanceOf<T>, divisor: NonZero<u64>) -> BalanceOf<T> {
			let divisor: BalanceOf<T> = divisor.get().saturated_into();
			let quotient = amount / divisor;
			if (amount % divisor).is_zero() {
				quotient
			} else {
				quotient.saturating_add(One::one())
			}
		}
	}
}

/// Maximum number of LOG topics a stipend frame is expected to emit.
const STIPEND_LOG_TOPICS: u32 = 4;
/// Maximum LOG data size (in bytes) a stipend frame is expected to emit.
const STIPEND_LOG_DATA_LEN: u32 = 64;

fn determine_call_stipend<T: Config>() -> Weight {
	let gas_weight = <EVMGas as Token<T>>::weight(&EVMGas(CALL_STIPEND));
	let event_weight = <RuntimeCosts as Token<T>>::weight(&RuntimeCosts::DepositEvent {
		num_topic: STIPEND_LOG_TOPICS,
		len: STIPEND_LOG_DATA_LEN,
	});
	gas_weight.saturating_add(event_weight)
}

/// Validate that there's enough weight for the stipend and return the stipend weight.
pub fn validate_and_get_stipend<T: Config>(weight_left: Weight) -> Result<Weight, DispatchError> {
	let weight_stipend = determine_call_stipend::<T>();
	if weight_left.any_lt(weight_stipend) {
		return Err(<Error<T>>::OutOfGas.into());
	}
	Ok(weight_stipend)
}

pub mod substrate_execution {
	use super::*;

	/// Create a transaction-level (root) meter for Substrate-style execution.
	///
	/// This constructs the root resource meter that enforces explicit weight and
	/// storage-deposit limits for the whole transaction. The returned `TransactionMeter`:
	/// - charges weight via `WeightMeter` with the provided `weight_limit`,
	/// - accounts storage deposit via `RootStorageMeter` with the provided `deposit_limit`,
	/// - records that the transaction's limit mode is `WeightAndDeposit`.
	pub fn new_root<T: Config>(
		weight_limit: Weight,
		deposit_limit: BalanceOf<T>,
	) -> Result<TransactionMeter<T>, DispatchError> {
		Ok(TransactionMeter {
			weight: WeightMeter::new(weight_limit, None),
			deposit: RootStorageMeter::new(Some(deposit_limit)),
			// ignore max total gas for Substrate executions
			max_total_gas: Default::default(),
			total_consumed_weight_before: Default::default(),
			total_consumed_deposit_before: Default::default(),
			transaction_limits: TransactionLimits::WeightAndDeposit { weight_limit, deposit_limit },
			_phantom: PhantomData,
		})
	}

	/// Create a nested (frame) meter derived from a parent `ResourceMeter`.
	///
	/// This produces a frame-local meter that enforces the resource limits for
	/// a nested call. It computes how much of the parent's remaining resources are available
	/// to the nested frame by:
	/// - collecting the parent's own consumed amounts (`self_consumed_*`),
	/// - deriving the total consumed amounts up to this point,
	/// - setting the call stipend aside, if `limit` asks for one,
	/// - applying the EIP-150 63/64 rule to the remaining weight and deposit, except for the
	///   transaction's first call,
	/// - applying the requested `CallResources` (no limits, ethereum gas conversion, or explicit
	///   weight+deposit) to derive per-frame limits,
	/// - adding the stipend to the nested frame's weight limit.
	///
	/// Returns `Err(Error::OutOfGas)` when weight is exhausted, or
	/// `Err(Error::StorageDepositLimitExhausted)` when deposit bookkeeping forbids further storage.
	pub fn new_nested_meter<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		limit: &CallResources<T>,
	) -> Result<FrameMeter<T>, DispatchError> {
		let (
			self_consumed_weight,
			self_consumed_deposit,
			total_consumed_weight,
			total_consumed_deposit,
		) = meter.consumed_resources();

		let weight_left_before_rule = meter
			.weight
			.weight_limit
			.checked_sub(&self_consumed_weight)
			.ok_or(<Error<T>>::OutOfGas)?;

		let deposit_limit = meter.deposit.limit.expect(
			"Deposit limits are always defined for `ResourceMeter` in Substrate \
				execution mode (i.e., when its `transaction_limits` are `WeightAndDeposit`); qed",
		);
		let deposit_left = self_consumed_deposit
			.available(&deposit_limit)
			.ok_or(<Error<T>>::StorageDepositLimitExhausted)?;

		let stipend_weight = match limit {
			CallResources::Ethereum { add_stipend: true, .. } => {
				validate_and_get_stipend::<T>(weight_left_before_rule)?
			},
			CallResources::Ethereum { add_stipend: false, .. } |
			CallResources::NoLimits |
			CallResources::WeightDeposit { .. } => Weight::zero(),
		};

		// Setting the stipend aside before the 63/64 rule leaves the caller 1/64 of the rest, as
		// on Ethereum.
		let weight_left = S::EIP_150_RULE
			.apply::<WeightEip150>(weight_left_before_rule.saturating_sub(stipend_weight));
		let deposit_left = S::EIP_150_RULE.apply::<BalanceEip150<T>>(deposit_left);

		let (nested_weight_limit, nested_deposit_limit) = {
			match limit {
				CallResources::NoLimits => (weight_left, deposit_left),

				CallResources::Ethereum { gas: requested_gas, .. } => {
					// Convert leftover weight and deposit to an ethereum-gas equivalent,
					// then cap that gas by `requested_gas`. Distribute the capped gas
					// back into weight and deposit portions using the same ratio so that
					// the nested frame receives proportional limits.
					let weight_gas_left = SignedGas::<T>::from_weight_fee(
						T::FeeInfo::weight_to_fee_average(&weight_left),
					);
					let deposit_gas_left = SignedGas::<T>::from_adjusted_deposit_charge(
						&StorageDeposit::Charge(deposit_left),
					);
					let Some(remaining_gas) =
						(weight_gas_left.saturating_add(&deposit_gas_left)).to_ethereum_gas()
					else {
						return Err(<Error<T>>::OutOfGas.into());
					};

					// Cap to u64::MAX since Ethereum gas is u64. Without this, large deposit_left
					// (e.g., u128::MAX) causes ratio ≈ 0, giving nested calls almost no weight.
					let remaining_gas = remaining_gas.min(u64::MAX.saturated_into());

					let callee_gas_limit = remaining_gas.min(*requested_gas);

					let ratio = if remaining_gas.is_zero() {
						FixedU128::one()
					} else {
						FixedU128::from_rational(
							callee_gas_limit.saturated_into(),
							remaining_gas.saturated_into(),
						)
					};

					let weight_limit = Weight::from_parts(
						ratio.saturating_mul_int(weight_left.ref_time()),
						ratio.saturating_mul_int(weight_left.proof_size()),
					);
					let deposit_limit = ratio.saturating_mul_int(deposit_left);

					(
						weight_limit.saturating_add(stipend_weight).min(weight_left_before_rule),
						deposit_left.min(deposit_limit),
					)
				},

				CallResources::WeightDeposit { weight, deposit_limit } =>
				// when explicit weight+deposit requested, take the minimum of parent's left
				// and the requested per-call limits.
				{
					(weight_left.min(*weight), deposit_left.min(*deposit_limit))
				},
			}
		};

		Ok(FrameMeter::<T> {
			weight: WeightMeter::new(nested_weight_limit, Some(stipend_weight)),
			deposit: meter.deposit.nested(Some(nested_deposit_limit)),
			max_total_gas: Default::default(),
			total_consumed_weight_before: total_consumed_weight,
			total_consumed_deposit_before: total_consumed_deposit,
			transaction_limits: meter.transaction_limits.clone(),
			_phantom: PhantomData,
		})
	}

	/// Compute the remaining ethereum-gas-equivalent for a Substrate-style transaction.
	///
	/// Converts the remaining weight and deposit into their gas-equivalents (via `FeeInfo`) and
	/// returns the sum. Returns `None` if either component does not have enough left.
	pub fn gas_left<T: Config, S: State>(meter: &ResourceMeter<T, S>) -> Option<SignedGas<T>> {
		match (weight_left(meter), deposit_left(meter)) {
			(Some(weight_left), Some(deposit_left)) => {
				let weight_gas_left = SignedGas::<T>::from_weight_fee(
					T::FeeInfo::weight_to_fee_average(&weight_left),
				);
				let deposit_gas_left = SignedGas::<T>::from_adjusted_deposit_charge(
					&StorageDeposit::Charge(deposit_left),
				);

				Some(weight_gas_left.saturating_add(&deposit_gas_left))
			},
			_ => None,
		}
	}

	/// Return remaining weight available in the given meter.
	///
	/// Subtracts the weight already consumed in the current frame from the configured limit.
	pub fn weight_left<T: Config, S: State>(meter: &ResourceMeter<T, S>) -> Option<Weight> {
		meter.weight.weight_limit.checked_sub(&meter.weight.weight_consumed())
	}

	/// Return remaining deposit available to the given meter.
	///
	/// Subtracts the storage deposit already consumed in the current frame from the configured
	/// limit.
	pub fn deposit_left<T: Config, S: State>(meter: &ResourceMeter<T, S>) -> Option<BalanceOf<T>> {
		let deposit_limit = meter.deposit.limit.expect(
			"Deposit limits are always defined for `ResourceMeter` in Substrate \
				execution mode (i.e., when its `transaction_limits` are `WeightAndDeposit`); qed",
		);
		meter.deposit.consumed().available(&deposit_limit)
	}

	/// Compute the total consumed gas (signed) for Substrate-style execution.
	///
	/// This returns a `SignedGas` as the consumed gas can be negative (when there are major storage
	/// deposit refunds)
	pub fn total_consumed_gas<T: Config, S: State>(meter: &ResourceMeter<T, S>) -> SignedGas<T> {
		let (_, _, total_consumed_weight, total_consumed_deposit) = meter.consumed_resources();

		let consumed_weight_gas =
			SignedGas::from_weight_fee(T::FeeInfo::weight_to_fee_average(&total_consumed_weight));
		let consumed_deposit_gas = SignedGas::from_adjusted_deposit_charge(&total_consumed_deposit);

		consumed_deposit_gas.saturating_add(&consumed_weight_gas)
	}

	/// Compute the gas (signed) during the lifetime of this meter for Substrate-style execution.
	pub fn eth_gas_consumed<T: Config, S: State>(meter: &ResourceMeter<T, S>) -> SignedGas<T> {
		let self_consumed_weight = meter.weight.weight_consumed();
		let self_consumed_deposit = meter.deposit.consumed();

		let total_consumed_weight =
			meter.total_consumed_weight_before.saturating_add(self_consumed_weight);

		let consumed_weight_gas_before = SignedGas::from_weight_fee(
			T::FeeInfo::weight_to_fee_average(&meter.total_consumed_weight_before),
		);
		let consumed_weight_gas =
			SignedGas::from_weight_fee(T::FeeInfo::weight_to_fee_average(&total_consumed_weight));

		let self_consumed_weight_gas =
			consumed_weight_gas.saturating_sub(&consumed_weight_gas_before);

		let self_consumed_deposit_gas =
			SignedGas::from_adjusted_deposit_charge(&self_consumed_deposit);

		self_consumed_deposit_gas.saturating_add(&self_consumed_weight_gas)
	}
}

pub mod ethereum_execution {
	use super::*;

	/// Create a transaction-level (root) meter for Ethereum-style execution.
	///
	/// This constructs a root `TransactionMeter` where the global limit is an
	/// ethereum-gas budget (`max_total_gas`). Weight and deposit meters are left unbounded
	/// (None). The function validates that there is positive gas left after initialization,
	/// otherwise it returns an error.
	pub fn new_root<T: Config>(
		eth_gas_limit: BalanceOf<T>,
		weight_limit: Weight,
		eth_tx_info: EthTxInfo<T>,
	) -> Result<TransactionMeter<T>, DispatchError> {
		let meter = TransactionMeter {
			weight: WeightMeter::new(weight_limit, None),
			deposit: RootStorageMeter::new(None),
			max_total_gas: SignedGas::from_ethereum_gas(eth_gas_limit),
			total_consumed_weight_before: Default::default(),
			total_consumed_deposit_before: Default::default(),
			transaction_limits: TransactionLimits::EthereumGas {
				eth_gas_limit,
				weight_limit,
				eth_tx_info,
				authorization_deposit: Default::default(),
			},
			_phantom: PhantomData,
		};

		if meter.eth_gas_left().is_some() {
			Ok(meter)
		} else {
			return Err(<Error<T>>::OutOfGas.into());
		}
	}

	/// Create a nested (frame) meter for an Ethereum-style execution.
	///
	/// - computes the gas already consumed by the transaction and determines how much gas is left,
	/// - sets the call stipend aside, if `limit` asks for one,
	/// - applies the EIP-150 63/64 rule to the remaining gas, weight and deposit, except for the
	///   transaction's first call,
	/// - adds the stipend to the nested frame's gas and weight limits,
	/// - if the parent is in a simple gas-only mode, returns a child meter that is limited only by
	///   gas (no per-frame weight/deposit limits),
	/// - otherwise computes concrete nested weight/deposit limits derived from the remaining
	///   ethereum gas
	///
	/// The function ensures the nested frame's derived gas+resources remain within the parent's
	/// remaining budget and returns `Err(Error::OutOfGas)` when the derived limits would exhaust
	/// available resources.
	pub fn new_nested_meter<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		limit: &CallResources<T>,
		eth_tx_info: &EthTxInfo<T>,
	) -> Result<FrameMeter<T>, DispatchError> {
		let (
			self_consumed_weight,
			self_consumed_deposit,
			total_consumed_weight,
			total_consumed_deposit,
		) = meter.consumed_resources();

		let total_gas_consumption =
			eth_tx_info.gas_consumption(&total_consumed_weight, &total_consumed_deposit);

		let remaining_gas_before_rule = meter.max_total_gas.saturating_sub(&total_gas_consumption);

		let weight_limit_left_before_rule = meter
			.weight
			.weight_limit
			.checked_sub(&self_consumed_weight)
			.ok_or(<Error<T>>::OutOfGas)?;

		let (stipend_weight, stipend_gas) = match limit {
			CallResources::Ethereum { add_stipend: true, .. } => {
				let weight_left_before_rule =
					self::weight_left(meter, eth_tx_info).ok_or(<Error<T>>::OutOfGas)?;
				let stipend_weight = validate_and_get_stipend::<T>(weight_left_before_rule)?;
				// The gas the stipend adds to the fee, which can be less than its own price because
				// the weight part of the fee is the larger of the ref_time and proof-size costs.
				let stipend_gas = eth_tx_info
					.weight_gas_consumption(&total_consumed_weight.saturating_add(stipend_weight))
					.saturating_sub(&eth_tx_info.weight_gas_consumption(&total_consumed_weight));
				(stipend_weight, stipend_gas)
			},
			CallResources::Ethereum { add_stipend: false, .. } |
			CallResources::NoLimits |
			CallResources::WeightDeposit { .. } => (Weight::zero(), SignedGas::default()),
		};

		// Setting the stipend aside before the 63/64 rule leaves the caller 1/64 of the rest, as
		// on Ethereum.
		let remaining_gas = remaining_gas_before_rule
			.saturating_sub(&stipend_gas)
			.apply_eip_150(S::EIP_150_RULE);
		let max_total_gas = total_gas_consumption.saturating_add(&remaining_gas);

		let weight_left = {
			let unbounded_weight_left = eth_tx_info
				.weight_remaining(&max_total_gas, &total_consumed_weight, &total_consumed_deposit)
				.ok_or(<Error<T>>::OutOfGas)?;
			let weight_limit_left = S::EIP_150_RULE.apply::<WeightEip150>(
				weight_limit_left_before_rule.saturating_sub(stipend_weight),
			);

			unbounded_weight_left.min(weight_limit_left)
		};

		let deposit_left = {
			// The stipend's gas can pay for storage deposit too.
			let Some(unbounded_deposit_left) =
				remaining_gas.saturating_add(&stipend_gas).to_adjusted_deposit_charge()
			else {
				return Err(<Error<T>>::OutOfGas.into());
			};

			match meter.deposit.limit {
				Some(deposit_limit) => {
					let deposit_limit_left = self_consumed_deposit
						.available(&deposit_limit)
						.map(|left| S::EIP_150_RULE.apply::<BalanceEip150<T>>(left))
						.ok_or(<Error<T>>::StorageDepositLimitExhausted)?;

					unbounded_deposit_left.min(deposit_limit_left)
				},
				None => unbounded_deposit_left,
			}
		};

		let (nested_gas_limit, nested_weight_limit, nested_deposit_limit) = {
			match limit {
				CallResources::NoLimits => (
					remaining_gas,
					weight_left,
					if meter.deposit.limit.is_none() { None } else { Some(deposit_left) },
				),

				CallResources::Ethereum { gas: requested_gas, .. } => (
					remaining_gas
						.min(&SignedGas::from_ethereum_gas(*requested_gas))
						.saturating_add(&stipend_gas)
						.min(&remaining_gas_before_rule),
					weight_left.saturating_add(stipend_weight).min(weight_limit_left_before_rule),
					if meter.deposit.limit.is_none() { None } else { Some(deposit_left) },
				),

				CallResources::WeightDeposit { weight, deposit_limit } => {
					let nested_weight_limit = weight_left.min(*weight);
					let nested_deposit_limit = deposit_left.min(*deposit_limit);

					let new_max_total_gas = eth_tx_info.gas_consumption(
						&total_consumed_weight.saturating_add(nested_weight_limit),
						&total_consumed_deposit
							.saturating_add(&StorageDeposit::Charge(nested_deposit_limit)),
					);

					let gas_limit = new_max_total_gas.saturating_sub(&total_gas_consumption);

					(remaining_gas.min(&gas_limit), nested_weight_limit, Some(nested_deposit_limit))
				},
			}
		};

		let nested_max_total_gas = total_gas_consumption.saturating_add(&nested_gas_limit);

		Ok(FrameMeter::<T> {
			weight: WeightMeter::new(nested_weight_limit, Some(stipend_weight)),
			deposit: meter.deposit.nested(nested_deposit_limit),
			max_total_gas: nested_max_total_gas,
			total_consumed_weight_before: total_consumed_weight,
			total_consumed_deposit_before: total_consumed_deposit,
			transaction_limits: meter.transaction_limits.clone(),
			_phantom: PhantomData,
		})
	}

	/// Compute remaining ethereum gas for an Ethereum-style execution.
	pub fn gas_left<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		eth_tx_info: &EthTxInfo<T>,
	) -> Option<SignedGas<T>> {
		let (_, _, total_consumed_weight, total_consumed_deposit) = meter.consumed_resources();

		let total_gas_consumption =
			eth_tx_info.gas_consumption(&total_consumed_weight, &total_consumed_deposit);

		Some(meter.max_total_gas.saturating_sub(&total_gas_consumption))
	}

	/// Return the remaining weight available to a nested frame under Ethereum-style execution.
	pub fn weight_left<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		eth_tx_info: &EthTxInfo<T>,
	) -> Option<Weight> {
		let (self_consumed_weight, _, total_consumed_weight, total_consumed_deposit) =
			meter.consumed_resources();

		let weight_left = eth_tx_info.weight_remaining(
			&meter.max_total_gas,
			&total_consumed_weight,
			&total_consumed_deposit,
		)?;

		Some(weight_left.min(meter.weight.weight_limit.checked_sub(&self_consumed_weight)?))
	}

	/// Return remaining deposit available to a nested frame under Ethereum-style execution.
	pub fn deposit_left<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		eth_tx_info: &EthTxInfo<T>,
	) -> Option<BalanceOf<T>> {
		let deposit_left = gas_left(meter, eth_tx_info)?.to_adjusted_deposit_charge()?;

		Some(match meter.deposit.limit {
			Some(deposit_limit) => {
				let deposit_available = meter.deposit.consumed().available(&deposit_limit)?;
				deposit_left.min(deposit_available)
			},
			None => deposit_left,
		})
	}

	/// Compute the total consumed gas (signed) for Ethereum-style execution.
	pub fn total_consumed_gas<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		eth_tx_info: &EthTxInfo<T>,
	) -> SignedGas<T> {
		let (_, _, total_consumed_weight, total_consumed_deposit) = meter.consumed_resources();

		eth_tx_info.gas_consumption(&total_consumed_weight, &total_consumed_deposit)
	}

	/// Compute the gas (signed) during the lifetime of this meter for Ethereum-style execution.
	pub fn eth_gas_consumed<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		eth_tx_info: &EthTxInfo<T>,
	) -> SignedGas<T> {
		let (_, _, total_consumed_weight, total_consumed_deposit) = meter.consumed_resources();

		let total_gas_consumed =
			eth_tx_info.gas_consumption(&total_consumed_weight, &total_consumed_deposit);
		let total_gas_consumed_before = eth_tx_info.gas_consumption(
			&meter.total_consumed_weight_before,
			&meter.total_consumed_deposit_before,
		);

		total_gas_consumed.saturating_sub(&total_gas_consumed_before)
	}
}
