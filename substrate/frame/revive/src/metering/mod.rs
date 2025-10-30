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

pub mod storage;
pub mod weight;

use crate::{
	evm::fees::InfoT, exec::CallResources, storage::ContractInfo, vm::evm::Halt, BalanceOf, Config,
	Error, ExecConfig, ExecOrigin as Origin, StorageDeposit, LOG_TARGET,
};
use frame_support::DefaultNoBound;
use num_traits::Zero;

use core::{fmt::Debug, marker::PhantomData, ops::ControlFlow};
use sp_runtime::{FixedPointNumber, Saturating, Weight};
use storage::{DepositOf, Diff, GenericMeter as GenericStorageMeter, Meter as RootStorageMeter};
use weight::{ChargedAmount, Token, WeightMeter};

use sp_runtime::{DispatchError, DispatchResult, FixedU128, SaturatedConversion};

/// Used to implement a type state pattern for the meter.
///
/// It is sealed and cannot be implemented outside of this module.
pub trait State: private::Sealed + Default + Debug {}

/// State parameter that constitutes a meter that is in its root state.
#[derive(Default, Debug)]
pub struct Root;

/// State parameter that constitutes a meter that is in its nested state.
/// Its value indicates whether the nested meter has its own limit.
#[derive(Default, Debug)]
pub struct Nested;

impl State for Root {}
impl State for Nested {}

mod private {
	pub trait Sealed {}
	impl Sealed for super::Root {}
	impl Sealed for super::Nested {}
}

pub type TransactionMeter<T> = ResourceMeter<T, Root>;
pub type FrameMeter<T> = ResourceMeter<T, Nested>;

#[derive(DefaultNoBound)]
pub struct ResourceMeter<T: Config, S: State> {
	weight: WeightMeter<T>,
	deposit: GenericStorageMeter<T, S>,

	eth_gas_limit: BalanceOf<T>,
	max_total_gas: DepositOf<T>,
	total_consumed_weight_before: Weight,
	total_consumed_deposit_before: DepositOf<T>,

	transaction_limits: TransactionLimits<T>,

	_phantom: PhantomData<S>,
}

#[derive(Debug, Clone)]
pub struct EthTxInfo<T: Config> {
	pub encoded_len: u32,
	pub extra_weight: Weight,
	_phantom: PhantomData<T>,
}

#[derive(Debug, Clone)]
pub enum TransactionLimits<T: Config> {
	EthereumGas { eth_gas_limit: BalanceOf<T>, eth_tx_info: EthTxInfo<T> },
	WeightAndDeposit { weight_limit: Weight, deposit_limit: BalanceOf<T> },
}

impl<T: Config> Default for TransactionLimits<T> {
	fn default() -> Self {
		Self::WeightAndDeposit {
			weight_limit: Default::default(),
			deposit_limit: Default::default(),
		}
	}
}

impl<T: Config, S: State> ResourceMeter<T, S> {
	pub fn charge_weight_token<Tok: Token<T>>(
		&mut self,
		token: Tok,
	) -> Result<ChargedAmount, DispatchError> {
		// TODO: optimize
		let weight_left = self.weight_left().ok_or(<Error<T>>::OutOfGas)?;

		self.weight.charge(token, weight_left)
	}

	pub fn charge_or_halt<Tok: Token<T>>(
		&mut self,
		token: Tok,
	) -> ControlFlow<Halt, ChargedAmount> {
		// TODO: optimize
		let weight_left = self.weight_left().unwrap_or_default();

		self.weight.charge_or_halt(token, weight_left)
	}

	pub fn adjust_weight<Tok: Token<T>>(&mut self, charged_amount: ChargedAmount, token: Tok) {
		self.weight.adjust_weight(charged_amount, token);
	}

	pub fn sync_from_executor(&mut self, engine_fuel: polkavm::Gas) -> Result<(), DispatchError> {
		// TODO: optimize
		let weight_left = self.weight_left().ok_or(<Error<T>>::OutOfGas)?;
		let weight_consumed = self.weight.weight_consumed();

		self.weight
			.sync_from_executor(engine_fuel, weight_left.saturating_add(weight_consumed))
	}

	pub fn consume_all_weight(&mut self) {
		// TODO: optimize
		let weight_left = self.weight_left().unwrap_or_default();
		let weight_consumed = self.weight.weight_consumed();

		self.weight.consume_all(weight_left.saturating_add(weight_consumed));
	}

	pub fn sync_to_executor(&mut self) -> polkavm::Gas {
		// TODO: optimize
		let weight_left = self.weight_left().unwrap_or_default();

		self.weight.sync_to_executor(weight_left)
	}

	pub fn charge_deposit(&mut self, deposit: &DepositOf<T>) -> DispatchResult {
		self.deposit.record_charge(deposit);

		if self.deposit.is_root {
			if self.deposit_left().is_none() {
				return Err(<Error<T>>::StorageDepositLimitExhausted.into());
			}
		}

		Ok(())
	}

	pub fn new_nested(&self, limit: &CallResources<T>) -> Result<FrameMeter<T>, DispatchError> {
		let self_consumed_weight = self.weight.weight_consumed();
		let self_consumed_deposit = self.deposit.consumed();

		let total_consumed_weight =
			self.total_consumed_weight_before.saturating_add(self_consumed_weight);
		let total_consumed_deposit =
			self.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

		let (nested_gas_limit, nested_weight_limit, nested_deposit_limit, max_total_gas) =
			match &self.transaction_limits {
				TransactionLimits::EthereumGas { eth_tx_info, .. } => {
					let max_total_gas = eth_tx_info.max_total_gas(
						self.eth_gas_limit,
						self.total_consumed_weight_before,
						&self.total_consumed_deposit_before,
					);

					let total_gas_consumption =
						eth_tx_info.gas_consumption(total_consumed_weight, &total_consumed_deposit);

					let StorageDeposit::Refund(gas_left) =
						max_total_gas.saturating_sub(&total_gas_consumption)
					else {
						return Err(<Error<T>>::OutOfGas.into());
					};

					if self.weight.weight_limit.is_none() &&
						matches!(limit, CallResources::NoLimits | CallResources::Ethereum(..))
					{
						let nested_gas_limit = if let CallResources::Ethereum(gas) = limit {
							gas_left.min(*gas)
						} else {
							gas_left
						};
						(nested_gas_limit, None, None, max_total_gas)
					} else {
						let weight_left = eth_tx_info
							.weight_remaining(
								&max_total_gas,
								total_consumed_weight,
								&total_consumed_deposit,
							)
							.ok_or(<Error<T>>::OutOfGas)?;

						let weight_left = match self.weight.weight_limit {
							Some(weight_limit) => weight_left.min(
								weight_limit
									.checked_sub(&self_consumed_weight)
									.ok_or(<Error<T>>::OutOfGas)?,
							),
							None => weight_left,
						};

						let deposit_left: BalanceOf<T> =
							EthTxInfo::<T>::deposit_remaining(gas_left);
						let deposit_left = match self.deposit.limit {
							Some(deposit_limit) => deposit_left.min(
								self_consumed_deposit
									.available(&deposit_limit)
									.ok_or(<Error<T>>::StorageDepositLimitExhausted)?,
							),
							None => deposit_left,
						};

						match limit {
							CallResources::NoLimits =>
								(gas_left, Some(weight_left), Some(deposit_left), max_total_gas),
							CallResources::Ethereum(gas) => (
								gas_left.min(*gas),
								Some(weight_left),
								Some(deposit_left),
								max_total_gas,
							),
							CallResources::Precise { weight, deposit_limit } => {
								let nested_weight_limit = weight_left.min(*weight);
								let nested_deposit_limit = deposit_left.min(*deposit_limit);

								let new_max_total_gas = eth_tx_info.gas_consumption(
									total_consumed_weight.saturating_add(nested_weight_limit),
									&total_consumed_deposit.saturating_add(
										&StorageDeposit::Charge(nested_deposit_limit),
									),
								);

								let gas_limit =
									new_max_total_gas.saturating_sub(&total_gas_consumption);
								let DepositOf::<T>::Refund(gas_limit) = gas_limit else {
									return Err(<Error<T>>::OutOfGas.into());
								};

								(
									gas_left.min(gas_limit),
									Some(nested_weight_limit),
									Some(nested_deposit_limit),
									max_total_gas,
								)
							},
						}
					}
				},

				TransactionLimits::WeightAndDeposit { .. } => {
					let weight_left = self
						.weight
						.weight_limit
						.expect("Weight limits all always defined for WeightAndDeposit; qed")
						.checked_sub(&self_consumed_weight)
						.ok_or(<Error<T>>::OutOfGas)?;

					let deposit_limit = self
						.deposit
						.limit
						.expect("Deposit limits all always defined for WeightAndDeposit; qed");
					let deposit_left = self_consumed_deposit
						.available(&deposit_limit)
						.ok_or(<Error<T>>::StorageDepositLimitExhausted)?;

					match limit {
						CallResources::NoLimits => (
							Default::default(),
							Some(weight_left),
							Some(deposit_left),
							Default::default(),
						),

						CallResources::Ethereum(gas) => {
							let weight_gas = T::FeeInfo::weight_to_fee(&weight_left);
							let deposit_gas = T::FeeInfo::next_fee_multiplier_reciprocal()
								.saturating_mul_int(deposit_left);
							let gas_left = weight_gas.saturating_add(deposit_gas);
							if (gas_left).is_zero() {
								Err(<Error<T>>::OutOfGas)?;
							}

							let ratio = FixedU128::from_rational(
								gas_left.min(*gas).saturated_into(),
								gas_left.saturated_into(),
							);

							let weight_limit = Weight::from_parts(
								ratio.saturating_mul_int(weight_left.ref_time()),
								ratio.saturating_mul_int(weight_left.proof_size()),
							);
							let deposit_limit = ratio.saturating_mul_int(deposit_left);

							(
								Default::default(),
								Some(weight_limit),
								Some(deposit_limit),
								Default::default(),
							)
						},

						CallResources::Precise { weight, deposit_limit } => (
							Default::default(),
							Some(weight_left.min(*weight)),
							Some(deposit_left.min(*deposit_limit)),
							Default::default(),
						),
					}
				},
			};

		Ok(FrameMeter::<T> {
			weight: WeightMeter::new(nested_weight_limit),
			deposit: self.deposit.nested(nested_deposit_limit),
			eth_gas_limit: nested_gas_limit,
			max_total_gas,
			total_consumed_weight_before: total_consumed_weight,
			total_consumed_deposit_before: total_consumed_deposit,
			transaction_limits: self.transaction_limits.clone(),
			_phantom: PhantomData,
		})
	}

	pub fn absorb_weight_meter_only(&mut self, other: FrameMeter<T>) {
		self.weight.absorb_nested(other.weight);
	}

	pub fn absorb_all_meters(
		&mut self,
		other: FrameMeter<T>,
		contract: &T::AccountId,
		info: Option<&mut ContractInfo<T>>,
	) {
		self.weight.absorb_nested(other.weight);
		self.deposit.absorb(other.deposit, contract, info);
	}

	pub fn eth_gas_left(&self) -> Option<BalanceOf<T>> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } => {
				let self_consumed_weight = self.weight.weight_consumed();
				let self_consumed_deposit = self.deposit.consumed();

				let total_consumed_weight =
					self.total_consumed_weight_before.saturating_add(self_consumed_weight);
				let total_consumed_deposit =
					self.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

				let total_gas_consumption =
					eth_tx_info.gas_consumption(total_consumed_weight, &total_consumed_deposit);

				match self.max_total_gas.saturating_sub(&total_gas_consumption) {
					StorageDeposit::Refund(gas_left) => Some(gas_left),
					StorageDeposit::Charge(_) => {
						log::debug!( target: LOG_TARGET, "Eth gas limit exhausted: {:?} > {:?}", total_gas_consumption, self.max_total_gas);
						None
					},
				}
			},

			TransactionLimits::WeightAndDeposit { .. } => {
				match (self.weight_left(), self.deposit_left()) {
					(Some(weight_left), Some(deposit_left)) => {
						let weight_gas = T::FeeInfo::weight_to_fee(&weight_left);
						let deposit_gas = T::FeeInfo::next_fee_multiplier_reciprocal()
							.saturating_mul_int(deposit_left);

						Some(weight_gas.saturating_add(deposit_gas))
					},
					_ => None,
				}
			},
		}
	}

	pub fn weight_left(&self) -> Option<Weight> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } => {
				let self_consumed_weight = self.weight.weight_consumed();
				let self_consumed_deposit = self.deposit.consumed();

				let total_consumed_weight =
					self.total_consumed_weight_before.saturating_add(self_consumed_weight);
				let total_consumed_deposit =
					self.total_consumed_deposit_before.saturating_add(&self_consumed_deposit);

				let weight_left = eth_tx_info.weight_remaining(
					&self.max_total_gas,
					total_consumed_weight,
					&total_consumed_deposit,
				)?;

				Some(match self.weight.weight_limit {
					Some(weight_limit) =>
						weight_left.min(weight_limit.checked_sub(&self_consumed_weight)?),
					None => weight_left,
				})
			},

			TransactionLimits::WeightAndDeposit { .. } => {
				let weight_limit = self
					.weight
					.weight_limit
					.expect("Weight limits all always defined for WeightAndDeposit; qed");
				weight_limit.checked_sub(&self.weight.weight_consumed())
			},
		}
	}

	pub fn deposit_left(&self) -> Option<BalanceOf<T>> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { .. } => {
				let eth_gas_left = self.eth_gas_left()?;
				let deposit_left = EthTxInfo::<T>::deposit_remaining(eth_gas_left);

				match self.deposit.limit {
					Some(deposit_limit) => {
						let deposit_available = self.deposit.consumed().available(&deposit_limit);
						let Some(deposit_available) = deposit_available else {
							log::debug!( target: LOG_TARGET, "Storage deposit limit exhausted: {:?} > {:?}", self.deposit.consumed(), deposit_limit);
							return None;
						};

						Some(deposit_left.min(deposit_available))
					},
					None => Some(deposit_left),
				}
			},

			TransactionLimits::WeightAndDeposit { .. } => {
				let deposit_limit = self
					.deposit
					.limit
					.expect("Deposit limits all always defined for WeightAndDeposit; qed");
				let deposit_available = self.deposit.consumed().available(&deposit_limit);

				if deposit_available.is_none() {
					log::debug!( target: LOG_TARGET, "Storage deposit limit exhausted: {:?} > {:?}", self.deposit.consumed(), deposit_limit);
					return None;
				}

				deposit_available
			},
		}
	}

	pub fn weight_consumed(&self) -> Weight {
		self.weight.weight_consumed()
	}

	pub fn weight_required(&self) -> Weight {
		self.weight.weight_required()
	}
}

impl<T: Config> EthTxInfo<T> {
	pub fn new(encoded_len: u32, extra_weight: Weight) -> Self {
		Self { encoded_len, extra_weight, _phantom: PhantomData }
	}

	pub fn gas_consumption(
		&self,
		consumed_weight: Weight,
		consumed_deposit: &DepositOf<T>,
	) -> DepositOf<T> {
		let fee_a = StorageDeposit::Refund(T::FeeInfo::fixed_fee(self.encoded_len))
			.saturating_sub(consumed_deposit)
			.scale_by_factor(&T::FeeInfo::next_fee_multiplier_reciprocal());

		let fee_b = T::FeeInfo::weight_to_fee(&consumed_weight.saturating_add(self.extra_weight));

		fee_a.saturating_add(&StorageDeposit::Refund(fee_b))
	}

	pub fn gas_remaining(
		max_total_gas: &DepositOf<T>,
		total_gas_consumption: &DepositOf<T>,
	) -> Option<BalanceOf<T>> {
		match max_total_gas.saturating_sub(total_gas_consumption) {
			StorageDeposit::Refund(amount) => Some(amount),
			StorageDeposit::Charge(_) => None,
		}
	}

	pub fn max_total_gas(
		&self,
		eth_gas_limit: BalanceOf<T>,
		total_consumed_weight_before: Weight,
		total_consumed_deposit_before: &DepositOf<T>,
	) -> DepositOf<T> {
		self.gas_consumption(total_consumed_weight_before, total_consumed_deposit_before)
			.saturating_add(&StorageDeposit::Refund(eth_gas_limit))
	}

	pub fn weight_remaining(
		&self,
		max_total_gas: &DepositOf<T>,
		total_weight_consumption: Weight,
		total_deposit_consumption: &DepositOf<T>,
	) -> Option<Weight> {
		let consumable_fee = max_total_gas.saturating_add(
			&total_deposit_consumption
				.saturating_add(&StorageDeposit::Charge(T::FeeInfo::fixed_fee(self.encoded_len)))
				.scale_by_factor(&T::FeeInfo::next_fee_multiplier_reciprocal()),
		);

		let StorageDeposit::Refund(consumable_fee) = consumable_fee else {
			return None;
		};

		T::FeeInfo::fee_to_weight(consumable_fee)
			.checked_sub(&total_weight_consumption.saturating_add(self.extra_weight))
	}

	pub fn deposit_remaining(gas_remaining: BalanceOf<T>) -> BalanceOf<T> {
		T::FeeInfo::next_fee_multiplier().saturating_mul_int(gas_remaining)
	}
}

impl<T: Config> TransactionMeter<T> {
	pub fn new(transaction_limits: TransactionLimits<T>) -> Result<Self, DispatchError> {
		match &transaction_limits {
			TransactionLimits::EthereumGas { eth_gas_limit, eth_tx_info } => {
				let base_gas =
					eth_tx_info.gas_consumption(Weight::default(), &DepositOf::<T>::default());

				let Some(gas_limit) = base_gas.available(&eth_gas_limit) else {
					return Err(<Error<T>>::StorageDepositNotEnoughFunds.into());
				};

				Ok(Self {
					weight: WeightMeter::new(None),
					deposit: RootStorageMeter::new(None),
					eth_gas_limit: gas_limit,
					max_total_gas: StorageDeposit::Refund(*eth_gas_limit),
					total_consumed_weight_before: Weight::default(),
					total_consumed_deposit_before: DepositOf::<T>::default(),
					transaction_limits,
					_phantom: PhantomData,
				})
			},

			TransactionLimits::WeightAndDeposit { weight_limit, deposit_limit } => Ok(Self {
				weight: WeightMeter::new(Some(*weight_limit)),
				deposit: RootStorageMeter::new(Some(*deposit_limit)),
				eth_gas_limit: Default::default(), // ignore eth gas limit for Substrate executions
				max_total_gas: Default::default(), // ignore eth gas limit for Substrate executions
				total_consumed_weight_before: Weight::default(),
				total_consumed_deposit_before: DepositOf::<T>::default(),
				transaction_limits,
				_phantom: PhantomData,
			}),
		}
	}

	pub fn execute_postponed_deposits(
		&self,
		origin: &Origin<T>,
		exec_config: &ExecConfig,
	) -> Result<DepositOf<T>, DispatchError> {
		if self.deposit_left().is_none() {
			// Deposit limit exceeded
			return Err(<Error<T>>::StorageDepositNotEnoughFunds.into());
		}

		self.deposit.execute_postponed_deposits(origin, exec_config)
	}
}

impl<T: Config> FrameMeter<T> {
	pub fn charge_contract_deposit_and_transfer(
		&mut self,
		contract: T::AccountId,
		amount: DepositOf<T>,
	) {
		self.deposit.charge_deposit(contract, amount)
	}

	pub fn terminate(&mut self, info: &ContractInfo<T>, beneficiary: T::AccountId) {
		self.deposit.terminate(info, beneficiary);
	}

	pub fn record_contract_storage_changes(&mut self, diff: &Diff) {
		self.deposit.charge(diff);
	}

	/// [`Self::charge_contract_deposit_and_transfer`] and [`Self::record_contract_storage_changes`]
	/// does not enforce the storage limit since we want to do this check as late as possible to
	/// allow later refunds to offset earlier charges.
	pub fn finalize(&mut self, info: Option<&mut ContractInfo<T>>) -> Result<(), DispatchError> {
		self.deposit.finalize_own_contributions(info);

		if self.deposit_left().is_none() {
			return Err(<Error<T>>::StorageDepositLimitExhausted.into());
		}

		Ok(())
	}
}
