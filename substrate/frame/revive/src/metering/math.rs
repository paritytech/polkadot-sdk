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
	FrameMeter, InfoT, ResourceMeter, RootStorageMeter, SaturatedConversion, Saturating, SignedGas,
	State, StorageDeposit, TransactionLimits, TransactionMeter, Weight, WeightMeter, Zero,
};
use core::marker::PhantomData;

pub mod substrate_execution {
	use super::*;

	pub fn new_root<T: Config>(
		weight_limit: Weight,
		deposit_limit: BalanceOf<T>,
	) -> Result<TransactionMeter<T>, DispatchError> {
		Ok(TransactionMeter {
			weight: WeightMeter::new(Some(weight_limit)),
			deposit: RootStorageMeter::new(Some(deposit_limit)),
			// ignore max total gas for Substrate executions
			max_total_gas: Default::default(),
			total_consumed_weight_before: Default::default(),
			total_consumed_deposit_before: Default::default(),
			transaction_limits: TransactionLimits::WeightAndDeposit { weight_limit, deposit_limit },
			_phantom: PhantomData,
		})
	}

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

		let (nested_weight_limit, nested_deposit_limit) = {
			let weight_left = meter
				.weight
				.weight_limit
				.expect("Weight limits all always defined for WeightAndDeposit; qed")
				.checked_sub(&self_consumed_weight)
				.ok_or(<Error<T>>::OutOfGas)?;

			let deposit_limit = meter
				.deposit
				.limit
				.expect("Deposit limits all always defined for WeightAndDeposit; qed");
			let deposit_left = self_consumed_deposit
				.available(&deposit_limit)
				.ok_or(<Error<T>>::StorageDepositLimitExhausted)?;

			match limit {
				CallResources::NoLimits => (weight_left, deposit_left),

				CallResources::Ethereum(gas) => {
					let weight_gas = T::FeeInfo::weight_to_fee_average(&weight_left);
					let deposit_gas = T::FeeInfo::next_fee_multiplier_reciprocal()
						.saturating_mul_int(deposit_left);
					let gas_left = weight_gas.saturating_add(deposit_gas);
					let gas_limit = gas_left.min(*gas);

					if (gas_left).is_zero() {
						(weight_left, deposit_left)
					} else {
						let ratio = FixedU128::from_rational(
							gas_limit.saturated_into(),
							gas_left.saturated_into(),
						);

						let weight_limit = Weight::from_parts(
							ratio.saturating_mul_int(weight_left.ref_time()),
							ratio.saturating_mul_int(weight_left.proof_size()),
						);
						let deposit_limit = ratio.saturating_mul_int(deposit_left);

						(weight_limit, deposit_limit)
					}
				},

				CallResources::Precise { weight, deposit_limit } =>
					(weight_left.min(*weight), deposit_left.min(*deposit_limit)),
			}
		};

		Ok(FrameMeter::<T> {
			weight: WeightMeter::new(Some(nested_weight_limit)),
			deposit: meter.deposit.nested(Some(nested_deposit_limit)),
			max_total_gas: Default::default(),
			total_consumed_weight_before: total_consumed_weight,
			total_consumed_deposit_before: total_consumed_deposit,
			transaction_limits: meter.transaction_limits.clone(),
			_phantom: PhantomData,
		})
	}

	pub fn eth_gas_left<T: Config, S: State>(meter: &ResourceMeter<T, S>) -> Option<BalanceOf<T>> {
		match (meter.weight_left(), meter.deposit_left()) {
			(Some(weight_left), Some(deposit_left)) => {
				let weight_gas = T::FeeInfo::weight_to_fee_average(&weight_left);
				let deposit_gas =
					T::FeeInfo::next_fee_multiplier_reciprocal().saturating_mul_int(deposit_left);

				Some(weight_gas.saturating_add(deposit_gas))
			},
			_ => None,
		}
	}

	pub fn weight_left<T: Config, S: State>(meter: &ResourceMeter<T, S>) -> Option<Weight> {
		let weight_limit = meter
			.weight
			.weight_limit
			.expect("Weight limits all always defined for WeightAndDeposit; qed");
		weight_limit.checked_sub(&meter.weight.weight_consumed())
	}

	pub fn deposit_left<T: Config, S: State>(meter: &ResourceMeter<T, S>) -> Option<BalanceOf<T>> {
		let deposit_limit = meter
			.deposit
			.limit
			.expect("Deposit limits all always defined for WeightAndDeposit; qed");
		meter.deposit.consumed().available(&deposit_limit)
	}

	pub fn total_consumed_gas<T: Config, S: State>(meter: &ResourceMeter<T, S>) -> SignedGas<T> {
		let self_consumed_weight = meter.weight.weight_consumed();
		let self_consumed_deposit = meter.deposit.consumed();

		let total_consumed_weight =
			meter.total_consumed_weight_before.saturating_add(self_consumed_weight);
		let total_consumed_deposit =
			meter.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

		let consumed_weight_gas = T::FeeInfo::weight_to_fee_average(&total_consumed_weight);

		let multiplier = T::FeeInfo::next_fee_multiplier_reciprocal();
		let consumed_deposit_gas = match total_consumed_deposit {
			StorageDeposit::Charge(amount) =>
				SignedGas::Positive(multiplier.saturating_mul_int(amount)),
			StorageDeposit::Refund(amount) =>
				SignedGas::Negative(multiplier.saturating_mul_int(amount)),
		};

		consumed_deposit_gas.saturating_add(&SignedGas::Positive(consumed_weight_gas))
	}
}

pub mod ethereum_execution {
	use super::*;

	pub fn new_root<T: Config>(
		eth_gas_limit: BalanceOf<T>,
		eth_tx_info: EthTxInfo<T>,
	) -> Result<TransactionMeter<T>, DispatchError> {
		let meter = TransactionMeter {
			weight: WeightMeter::new(None),
			deposit: RootStorageMeter::new(None),
			max_total_gas: SignedGas::Positive(eth_gas_limit),
			total_consumed_weight_before: Default::default(),
			total_consumed_deposit_before: Default::default(),
			transaction_limits: TransactionLimits::EthereumGas { eth_gas_limit, eth_tx_info },
			_phantom: PhantomData,
		};

		if meter.eth_gas_left().is_some() {
			Ok(meter)
		} else {
			return Err(<Error<T>>::OutOfGas.into());
		}
	}

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

		let Some(gas_left) =
			meter.max_total_gas.saturating_sub(&total_gas_consumption).as_positive()
		else {
			return Err(<Error<T>>::OutOfGas.into());
		};

		let (nested_gas_limit, nested_weight_limit, nested_deposit_limit) = {
			let is_simple = meter.weight.weight_limit.is_none() &&
				matches!(limit, CallResources::NoLimits | CallResources::Ethereum(..));

			if is_simple {
				let nested_gas_limit = if let CallResources::Ethereum(gas) = limit {
					gas_left.min(*gas)
				} else {
					gas_left
				};
				(nested_gas_limit, None, None)
			} else {
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
							weight_limit
								.checked_sub(&self_consumed_weight)
								.ok_or(<Error<T>>::OutOfGas)?,
						),
						None => unbounded_weight_left,
					}
				};

				let deposit_left = {
					let unbounded_deposit_left: BalanceOf<T> =
						T::FeeInfo::next_fee_multiplier().saturating_mul_int(gas_left);
					match meter.deposit.limit {
						Some(deposit_limit) => unbounded_deposit_left.min(
							self_consumed_deposit
								.available(&deposit_limit)
								.ok_or(<Error<T>>::StorageDepositLimitExhausted)?,
						),
						None => unbounded_deposit_left,
					}
				};

				match limit {
					CallResources::NoLimits => (gas_left, Some(weight_left), Some(deposit_left)),
					CallResources::Ethereum(gas) =>
						(gas_left.min(*gas), Some(weight_left), Some(deposit_left)),
					CallResources::Precise { weight, deposit_limit } => {
						let nested_weight_limit = weight_left.min(*weight);
						let nested_deposit_limit = deposit_left.min(*deposit_limit);

						let new_max_total_gas = eth_tx_info.gas_consumption(
							&total_consumed_weight.saturating_add(nested_weight_limit),
							&total_consumed_deposit
								.saturating_add(&StorageDeposit::Charge(nested_deposit_limit)),
						);

						let Some(gas_limit) =
							new_max_total_gas.saturating_sub(&total_gas_consumption).as_positive()
						else {
							return Err(<Error<T>>::OutOfGas.into());
						};

						(
							gas_left.min(gas_limit),
							Some(nested_weight_limit),
							Some(nested_deposit_limit),
						)
					},
				}
			}
		};

		let nested_max_total_gas =
			total_gas_consumption.saturating_add(&SignedGas::Positive(nested_gas_limit));

		Ok(FrameMeter::<T> {
			weight: WeightMeter::new(nested_weight_limit),
			deposit: meter.deposit.nested(nested_deposit_limit),
			max_total_gas: nested_max_total_gas,
			total_consumed_weight_before: total_consumed_weight,
			total_consumed_deposit_before: total_consumed_deposit,
			transaction_limits: meter.transaction_limits.clone(),
			_phantom: PhantomData,
		})
	}

	pub fn eth_gas_left<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		eth_tx_info: &EthTxInfo<T>,
	) -> Option<BalanceOf<T>> {
		let self_consumed_weight = meter.weight.weight_consumed();
		let self_consumed_deposit = meter.deposit.consumed();

		let total_consumed_weight =
			meter.total_consumed_weight_before.saturating_add(self_consumed_weight);
		let total_consumed_deposit =
			meter.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

		let total_gas_consumption =
			eth_tx_info.gas_consumption(&total_consumed_weight, &total_consumed_deposit);

		meter.max_total_gas.saturating_sub(&total_gas_consumption).as_positive()
	}

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

	pub fn deposit_left<T: Config, S: State>(
		meter: &ResourceMeter<T, S>,
		eth_tx_info: &EthTxInfo<T>,
	) -> Option<BalanceOf<T>> {
		let eth_gas_left = eth_gas_left(meter, eth_tx_info)?;
		let deposit_left = T::FeeInfo::next_fee_multiplier().saturating_mul_int(eth_gas_left);

		Some(match meter.deposit.limit {
			Some(deposit_limit) => {
				let deposit_available = meter.deposit.consumed().available(&deposit_limit)?;
				deposit_left.min(deposit_available)
			},
			None => deposit_left,
		})
	}

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
}
