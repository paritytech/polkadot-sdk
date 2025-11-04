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

pub mod math;
pub mod storage;
pub mod weight;

#[cfg(test)]
mod tests;

use crate::{
	evm::fees::InfoT, exec::CallResources, storage::ContractInfo, vm::evm::Halt, BalanceOf, Config,
	Error, ExecConfig, ExecOrigin as Origin, SignedGas, StorageDeposit,
};
use frame_support::{DebugNoBound, DefaultNoBound};
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

/// invariant: either the limits in both meters are both None or both Some(..)
/// they will always be defined if `transaction_limits` is `TransactionLimits::WeightAndDeposit`
#[derive(DefaultNoBound)]
pub struct ResourceMeter<T: Config, S: State> {
	weight: WeightMeter<T>,
	deposit: GenericStorageMeter<T, S>,

	// this is always zero for Substrate executions
	max_total_gas: SignedGas<T>,
	total_consumed_weight_before: Weight,
	total_consumed_deposit_before: DepositOf<T>,

	transaction_limits: TransactionLimits<T>,

	_phantom: PhantomData<S>,
}

#[derive(DebugNoBound, Clone)]
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
				self.deposit.reset();
				return Err(<Error<T>>::StorageDepositLimitExhausted.into());
			}
		}

		Ok(())
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

	pub fn new_nested(&self, limit: &CallResources<T>) -> Result<FrameMeter<T>, DispatchError> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::new_nested_meter(self, limit, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::new_nested_meter(self, limit),
		}
	}

	pub fn eth_gas_left(&self) -> Option<BalanceOf<T>> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::eth_gas_left(self, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::eth_gas_left(self),
		}
	}

	pub fn weight_left(&self) -> Option<Weight> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::weight_left(self, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::weight_left(self),
		}
	}

	pub fn deposit_left(&self) -> Option<BalanceOf<T>> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::deposit_left(self, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::deposit_left(self),
		}
	}

	pub fn total_consumed_gas(&self) -> BalanceOf<T> {
		let signed_gas = match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::total_consumed_gas(self, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::total_consumed_gas(self),
		};

		signed_gas.as_positive().unwrap_or_default()
	}

	pub fn weight_consumed(&self) -> Weight {
		self.weight.weight_consumed()
	}

	pub fn weight_required(&self) -> Weight {
		self.weight.weight_required()
	}

	pub fn deposit_consumed(&self) -> DepositOf<T> {
		self.deposit.consumed()
	}

	pub fn deposit_required(&self) -> DepositOf<T> {
		self.deposit.max_charged()
	}
}

impl<T: Config> TransactionMeter<T> {
	pub fn new(transaction_limits: TransactionLimits<T>) -> Result<Self, DispatchError> {
		match transaction_limits {
			TransactionLimits::EthereumGas { eth_gas_limit, eth_tx_info } =>
				math::ethereum_execution::new_root(eth_gas_limit, eth_tx_info),
			TransactionLimits::WeightAndDeposit { weight_limit, deposit_limit } =>
				math::substrate_execution::new_root(weight_limit, deposit_limit),
		}
	}

	pub fn new_from_limits(
		weight_limit: Weight,
		deposit_limit: BalanceOf<T>,
	) -> Result<Self, DispatchError> {
		Self::new(TransactionLimits::WeightAndDeposit { weight_limit, deposit_limit })
	}

	pub fn execute_postponed_deposits(
		&mut self,
		origin: &Origin<T>,
		exec_config: &ExecConfig<T>,
	) -> Result<DepositOf<T>, DispatchError> {
		if self.deposit_left().is_none() {
			// Deposit limit exceeded
			return Err(<Error<T>>::StorageDepositNotEnoughFunds.into());
		}

		self.deposit.execute_postponed_deposits(origin, exec_config)
	}

	pub fn terminate_absorb(
		&mut self,
		contract_account: T::AccountId,
		contract_info: &mut ContractInfo<T>,
		beneficiary: T::AccountId,
		delete_code: bool,
	) {
		self.deposit
			.terminate_absorb(contract_account, contract_info, beneficiary, delete_code);
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

#[derive(DebugNoBound, Clone)]
pub struct EthTxInfo<T: Config> {
	pub encoded_len: u32,
	pub extra_weight: Weight,
	_phantom: PhantomData<T>,
}

impl<T: Config> EthTxInfo<T> {
	pub fn new(encoded_len: u32, extra_weight: Weight) -> Self {
		Self { encoded_len, extra_weight, _phantom: PhantomData }
	}

	pub fn gas_consumption(
		&self,
		consumed_weight: &Weight,
		consumed_deposit: &DepositOf<T>,
	) -> SignedGas<T> {
		let deposit_gas = SignedGas::from_deposit_charge(consumed_deposit);
		let fixed_fee_gas = SignedGas::Positive(T::FeeInfo::fixed_fee(self.encoded_len));
		let scaled_gas = (deposit_gas.saturating_add(&fixed_fee_gas))
			.scale_by_factor(&T::FeeInfo::next_fee_multiplier_reciprocal());

		let weight_fee = SignedGas::Positive(T::FeeInfo::weight_to_fee(
			&consumed_weight.saturating_add(self.extra_weight),
		));

		scaled_gas.saturating_add(&weight_fee)
	}

	pub fn gas_remaining(
		max_total_gas: &SignedGas<T>,
		total_gas_consumption: &SignedGas<T>,
	) -> Option<BalanceOf<T>> {
		max_total_gas.saturating_sub(total_gas_consumption).as_positive()
	}

	pub fn weight_remaining(
		&self,
		max_total_gas: &SignedGas<T>,
		total_weight_consumption: &Weight,
		total_deposit_consumption: &DepositOf<T>,
	) -> Option<Weight> {
		let numerator = SignedGas::from_deposit_charge(total_deposit_consumption)
			.saturating_add(&SignedGas::Positive(T::FeeInfo::fixed_fee(self.encoded_len)));
		let consumable_fee = max_total_gas.saturating_sub(
			&numerator.scale_by_factor(&T::FeeInfo::next_fee_multiplier_reciprocal()),
		);

		let SignedGas::Positive(consumable_fee) = consumable_fee else {
			return None;
		};

		T::FeeInfo::fee_to_weight(consumable_fee)
			.checked_sub(&total_weight_consumption.saturating_add(self.extra_weight))
	}
}
