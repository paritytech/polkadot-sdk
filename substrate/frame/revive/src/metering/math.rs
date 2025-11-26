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
	FrameMeter, InfoT, ResourceMeter, RootStorageMeter, SaturatedConversion, State, StorageDeposit,
	TransactionLimits, TransactionMeter, Weight, WeightMeter, Zero,
};
use crate::{metering::weight::Token, vm::evm::EVMGas, SignedGas};
use core::marker::PhantomData;
use revm::interpreter::gas::CALL_STIPEND;

fn determine_call_stipend<T: Config>() -> Weight {
	let gas = EVMGas(CALL_STIPEND);
	<EVMGas as Token<T>>::weight(&gas)
}

pub mod substrate_execution {
	use num_traits::One;

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
			weight: WeightMeter::new(Some(weight_limit), None),
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
	/// - applying the requested `CallResources` (no limits, ethereum gas conversion, or explicit
	///   weight+deposit) to derive per-frame limits.
	///
	/// Returns `Err(Error::OutOfGas)` when weight is exhausted, or
	/// `Err(Error::StorageDepositLimitExhausted)` when deposit bookkeeping forbids further storage.
	pub fn new_nested_meter<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		limit: &CallResources<T>,
	) -> Result<FrameMeter<T>, DispatchError> {
		let self_consumed_weight = meter.weight.weight_consumed();
		let self_consumed_deposit = meter.deposit.consumed();

		let total_consumed_weight =
			meter.total_consumed_weight_before.saturating_add(self_consumed_weight);
		let total_consumed_deposit =
			meter.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

		let weight_left = meter
			.weight
			.weight_limit
			.expect(
				"Weight limits are always defined for `ResourceMeter` in Substrate \
				execution mode (i.e., when its `transaction_limits` are `WeightAndDeposit`); qed",
			)
			.checked_sub(&self_consumed_weight)
			.ok_or(<Error<T>>::OutOfGas)?;

		let deposit_limit = meter.deposit.limit.expect(
			"Deposit limits are always defined for `ResourceMeter` in Substrate \
				execution mode (i.e., when its `transaction_limits` are `WeightAndDeposit`); qed",
		);
		let deposit_left = self_consumed_deposit
			.available(&deposit_limit)
			.ok_or(<Error<T>>::StorageDepositLimitExhausted)?;

		let (nested_weight_limit, nested_deposit_limit, stipend) = {
			match limit {
				CallResources::NoLimits => (weight_left, deposit_left, None),

				CallResources::Ethereum { gas, add_stipend } => {
					// Convert leftover weight and deposit to an ethereum-gas equivalent,
					// then cap that gas by the requested `gas`. Distribute the capped gas
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

					let gas_limit = remaining_gas.min(*gas);

					let ratio = if remaining_gas.is_zero() {
						FixedU128::one()
					} else {
						FixedU128::from_rational(
							gas_limit.saturated_into(),
							remaining_gas.saturated_into(),
						)
					};

					let mut weight_limit = Weight::from_parts(
						ratio.saturating_mul_int(weight_left.ref_time()),
						ratio.saturating_mul_int(weight_left.proof_size()),
					);
					let deposit_limit = ratio.saturating_mul_int(deposit_left);

					let stipend = if *add_stipend {
						let weight_stipend = determine_call_stipend::<T>();
						if weight_left.any_lt(weight_stipend) {
							return Err(<Error<T>>::OutOfGas.into())
						}

						weight_limit.saturating_accrue(weight_stipend);

						Some(weight_stipend)
					} else {
						None
					};

					(weight_left.min(weight_limit), deposit_left.min(deposit_limit), stipend)
				},

				CallResources::WeightDeposit { weight, deposit_limit } =>
				// when explicit weight+deposit requested, take the minimum of parent's left
				// and the requested per-call limits.
					(weight_left.min(*weight), deposit_left.min(*deposit_limit), None),
			}
		};

		Ok(FrameMeter::<T> {
			weight: WeightMeter::new(Some(nested_weight_limit), stipend),
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
		let weight_limit = meter.weight.weight_limit.expect(
			"Weight limits are always defined for `ResourceMeter` in Substrate \
				execution mode (i.e., when its `transaction_limits` are `WeightAndDeposit`); qed",
		);
		weight_limit.checked_sub(&meter.weight.weight_consumed())
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
		let self_consumed_weight = meter.weight.weight_consumed();
		let self_consumed_deposit = meter.deposit.consumed();

		let total_consumed_weight =
			meter.total_consumed_weight_before.saturating_add(self_consumed_weight);
		let total_consumed_deposit =
			meter.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

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
		maybe_weight_limit: Option<Weight>,
		eth_tx_info: EthTxInfo<T>,
	) -> Result<TransactionMeter<T>, DispatchError> {
		let meter = TransactionMeter {
			weight: WeightMeter::new(maybe_weight_limit, None),
			deposit: RootStorageMeter::new(None),
			max_total_gas: SignedGas::from_ethereum_gas(eth_gas_limit),
			total_consumed_weight_before: Default::default(),
			total_consumed_deposit_before: Default::default(),
			transaction_limits: TransactionLimits::EthereumGas {
				eth_gas_limit,
				maybe_weight_limit,
				eth_tx_info,
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
		let self_consumed_weight = meter.weight.weight_consumed();
		let self_consumed_deposit = meter.deposit.consumed();

		let total_consumed_weight =
			meter.total_consumed_weight_before.saturating_add(self_consumed_weight);
		let total_consumed_deposit =
			meter.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

		let total_gas_consumption =
			eth_tx_info.gas_consumption(&total_consumed_weight, &total_consumed_deposit);

		let remaining_gas = meter.max_total_gas.saturating_sub(&total_gas_consumption);

		let weight_left = {
			let unbounded_weight_left = eth_tx_info
				.weight_remaining(
					&meter.max_total_gas,
					&total_consumed_weight,
					&total_consumed_deposit,
				)
				.ok_or(<Error<T>>::OutOfGas)?;

			match meter.weight.weight_limit {
				Some(weight_limit) => unbounded_weight_left.min(
					weight_limit.checked_sub(&self_consumed_weight).ok_or(<Error<T>>::OutOfGas)?,
				),
				None => unbounded_weight_left,
			}
		};

		let deposit_left = {
			let Some(unbounded_deposit_left) = remaining_gas.to_adjusted_deposit_charge() else {
				return Err(<Error<T>>::OutOfGas.into());
			};

			match meter.deposit.limit {
				Some(deposit_limit) => unbounded_deposit_left.min(
					self_consumed_deposit
						.available(&deposit_limit)
						.ok_or(<Error<T>>::StorageDepositLimitExhausted)?,
				),
				None => unbounded_deposit_left,
			}
		};

		let (nested_gas_limit, nested_weight_limit, nested_deposit_limit, stipend) = {
			match limit {
				CallResources::NoLimits => (
					remaining_gas,
					if meter.weight.weight_limit.is_none() { None } else { Some(weight_left) },
					if meter.deposit.limit.is_none() { None } else { Some(deposit_left) },
					None,
				),

				CallResources::Ethereum { gas, add_stipend } => {
					let gas_limit = SignedGas::from_ethereum_gas(*gas);

					let (gas_limit, stipend) = if *add_stipend {
						let weight_stipend = determine_call_stipend::<T>();
						if weight_left.any_lt(weight_stipend) {
							return Err(<Error<T>>::OutOfGas.into())
						}

						(
							gas_limit.saturating_add(&SignedGas::<T>::from_weight_fee(
								T::FeeInfo::weight_to_fee(&weight_stipend),
							)),
							Some(weight_stipend),
						)
					} else {
						(gas_limit, None)
					};

					(
						remaining_gas.min(&gas_limit),
						if meter.weight.weight_limit.is_none() { None } else { Some(weight_left) },
						if meter.deposit.limit.is_none() { None } else { Some(deposit_left) },
						stipend,
					)
				},

				CallResources::WeightDeposit { weight, deposit_limit } => {
					let nested_weight_limit = weight_left.min(*weight);
					let nested_deposit_limit = deposit_left.min(*deposit_limit);

					let new_max_total_gas = eth_tx_info.gas_consumption(
						&total_consumed_weight.saturating_add(nested_weight_limit),
						&total_consumed_deposit
							.saturating_add(&StorageDeposit::Charge(nested_deposit_limit)),
					);

					let gas_limit = new_max_total_gas.saturating_sub(&total_gas_consumption);

					(
						remaining_gas.min(&gas_limit),
						Some(nested_weight_limit),
						Some(nested_deposit_limit),
						None,
					)
				},
			}
		};

		let nested_max_total_gas = total_gas_consumption.saturating_add(&nested_gas_limit);

		Ok(FrameMeter::<T> {
			weight: WeightMeter::new(nested_weight_limit, stipend),
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
		let self_consumed_weight = meter.weight.weight_consumed();
		let self_consumed_deposit = meter.deposit.consumed();

		let total_consumed_weight =
			meter.total_consumed_weight_before.saturating_add(self_consumed_weight);
		let total_consumed_deposit =
			meter.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

		let total_gas_consumption =
			eth_tx_info.gas_consumption(&total_consumed_weight, &total_consumed_deposit);

		Some(meter.max_total_gas.saturating_sub(&total_gas_consumption))
	}

	/// Return the remaining weight available to a nested frame under Ethereum-style execution.
	pub fn weight_left<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		eth_tx_info: &EthTxInfo<T>,
	) -> Option<Weight> {
		let self_consumed_weight = meter.weight.weight_consumed();
		let self_consumed_deposit = meter.deposit.consumed();

		let total_consumed_weight =
			meter.total_consumed_weight_before.saturating_add(self_consumed_weight);
		let total_consumed_deposit =
			meter.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

		let weight_left = eth_tx_info.weight_remaining(
			&meter.max_total_gas,
			&total_consumed_weight,
			&total_consumed_deposit,
		)?;

		Some(match meter.weight.weight_limit {
			Some(weight_limit) => weight_left.min(weight_limit.checked_sub(&self_consumed_weight)?),
			None => weight_left,
		})
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
		let self_consumed_weight = meter.weight.weight_consumed();
		let self_consumed_deposit = meter.deposit.consumed();

		let total_consumed_weight =
			meter.total_consumed_weight_before.saturating_add(self_consumed_weight);
		let total_consumed_deposit =
			meter.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

		eth_tx_info.gas_consumption(&total_consumed_weight, &total_consumed_deposit)
	}

	/// Compute the gas (signed) during the lifetime of this meter for Ethereum-style execution.
	pub fn eth_gas_consumed<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		eth_tx_info: &EthTxInfo<T>,
	) -> SignedGas<T> {
		let self_consumed_weight = meter.weight.weight_consumed();
		let self_consumed_deposit = meter.deposit.consumed();

		let total_consumed_weight =
			meter.total_consumed_weight_before.saturating_add(self_consumed_weight);
		let total_consumed_deposit =
			meter.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

		let total_gas_consumed =
			eth_tx_info.gas_consumption(&total_consumed_weight, &total_consumed_deposit);
		let total_gas_consumed_before = eth_tx_info.gas_consumption(
			&meter.total_consumed_weight_before,
			&meter.total_consumed_deposit_before,
		);

		total_gas_consumed.saturating_sub(&total_gas_consumed_before)
	}
}
