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

/// A type-state pattern ensuring that meters can only be used in valid states (root vs nested).
///
/// It is sealed and cannot be implemented outside of this module.
pub trait State: private::Sealed + Default + Debug {}

/// Root state for transaction-level resource metering.
///
/// Represents the top-level accounting of a transaction's resource usage.
#[derive(Default, Debug)]
pub struct Root;

/// Nested state for frame-level resource metering.
///
/// Represents resource accounting for a single call frame.
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

/// Resource meter tracking weight and storage deposit consumption.
///
/// This type maintains the core invariant that either:
/// - Both weight and deposit limits are None, or
/// - Both limits are Some(value)
///
/// A resource meter tracks:
/// - Current frame's weight consumption via WeightMeter
/// - Current frame's storage deposit changes via GenericStorageMeter
/// - Total resources consumed before this frame started
/// - Transaction-wide resource limits and execution mode
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

/// Transaction-wide resource limit configuration.
///
/// Represents the two supported resource accounting modes:
/// - EthereumGas: Single gas limit
/// - WeightAndDeposit: Explicit limits for both computational weight and storage deposit
#[derive(DebugNoBound, Clone)]
pub enum TransactionLimits<T: Config> {
	EthereumGas {
		eth_gas_limit: BalanceOf<T>,
		// if this is provided, we will additionally ensure that execution will not exhaust this
		// weight limit
		maybe_weight_limit: Option<Weight>,
		eth_tx_info: EthTxInfo<T>,
	},
	WeightAndDeposit {
		weight_limit: Weight,
		deposit_limit: BalanceOf<T>,
	},
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
	/// Create a new nested meter with derived resource limits.
	pub fn new_nested(&self, limit: &CallResources<T>) -> Result<FrameMeter<T>, DispatchError> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::new_nested_meter(self, limit, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::new_nested_meter(self, limit),
		}
	}

	/// Absorb only the weight consumption from a nested frame meter.
	pub fn absorb_weight_meter_only(&mut self, other: FrameMeter<T>) {
		self.weight.absorb_nested(other.weight);
	}

	/// Absorb all resource consumption from a nested frame meter.
	pub fn absorb_all_meters(
		&mut self,
		other: FrameMeter<T>,
		contract: &T::AccountId,
		info: Option<&mut ContractInfo<T>>,
	) {
		self.weight.absorb_nested(other.weight);
		self.deposit.absorb(other.deposit, contract, info);
	}

	/// Charge a weight token against this meter's remaining weight limit.
	///
	/// Returns `Err(Error::OutOfGas)` if the weight limit would be exceeded.
	pub fn charge_weight_token<Tok: Token<T>>(
		&mut self,
		token: Tok,
	) -> Result<ChargedAmount, DispatchError> {
		// TODO: optimize
		let weight_left = self.weight_left().ok_or(<Error<T>>::OutOfGas)?;

		self.weight.charge(token, weight_left)
	}

	/// Try to charge a weight token or halt if not enough weight is left.
	pub fn charge_or_halt<Tok: Token<T>>(
		&mut self,
		token: Tok,
	) -> ControlFlow<Halt, ChargedAmount> {
		// TODO: optimize
		let weight_left = self.weight_left().unwrap_or_default();

		self.weight.charge_or_halt(token, weight_left)
	}

	/// Adjust an earlier weight charge with the actual weight consumed.
	pub fn adjust_weight<Tok: Token<T>>(&mut self, charged_amount: ChargedAmount, token: Tok) {
		self.weight.adjust_weight(charged_amount, token);
	}

	/// Synchronize meter state with PolkaVM executor's fuel consumption.
	///
	/// Maps the VM's internal fuel accounting to weight consumption:
	/// - Converts engine fuel units to weight units
	/// - Updates meter state to match actual VM resource usage
	pub fn sync_from_executor(&mut self, engine_fuel: polkavm::Gas) -> Result<(), DispatchError> {
		// TODO: optimize
		let weight_left = self.weight_left().ok_or(<Error<T>>::OutOfGas)?;
		let weight_consumed = self.weight.weight_consumed();

		self.weight
			.sync_from_executor(engine_fuel, weight_left.saturating_add(weight_consumed))
	}

	/// Convert meter state to PolkaVM executor fuel units.
	///
	/// Prepares for VM execution by:
	/// - Computing remaining available weight
	/// - Converting weight units to VM fuel units and return
	pub fn sync_to_executor(&mut self) -> polkavm::Gas {
		// TODO: optimize
		let weight_left = self.weight_left().unwrap_or_default();

		self.weight.sync_to_executor(weight_left)
	}

	/// Consume all remaining weight in the meter.
	pub fn consume_all_weight(&mut self) {
		// TODO: optimize
		let weight_left = self.weight_left().unwrap_or_default();
		let weight_consumed = self.weight.weight_consumed();

		self.weight.consume_all(weight_left.saturating_add(weight_consumed));
	}

	/// Record a storage deposit charge against this meter.
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

	/// Get remaining ethereum gas equivalent.
	///
	/// Converts remaining resources to ethereum gas units:
	/// - For ethereum mode: computes directly from gas accounting
	/// - For substrate mode: converts weight+deposit to gas equivalent
	/// Returns None if resources are exhausted or conversion fails.
	pub fn eth_gas_left(&self) -> Option<BalanceOf<T>> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::eth_gas_left(self, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::eth_gas_left(self),
		}
	}

	/// Get remaining weight available.
	///
	/// Computes remaining computational capacity:
	/// - For ethereum mode: converts from gas to weight units
	/// - For substrate mode: subtracts consumed from weight limit
	/// Returns None if resources are exhausted.
	pub fn weight_left(&self) -> Option<Weight> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::weight_left(self, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::weight_left(self),
		}
	}

	/// Get remaining deposit available.
	///
	/// Computes remaining storage deposit allowance:
	/// - For ethereum mode: converts from gas to deposit units
	/// - For substrate mode: subtracts consumed from deposit limit
	/// Returns None if resources are exhausted.
	pub fn deposit_left(&self) -> Option<BalanceOf<T>> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::deposit_left(self, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::deposit_left(self),
		}
	}

	/// Calculate total gas consumed so far.
	///
	/// Computes the ethereum-gas equivalent of all resource usage:
	/// - Converts weight and deposit consumption to gas units
	/// - For ethereum mode: uses direct gas accounting
	/// - For substrate mode: synthesizes from weight+deposit usage
	pub fn total_consumed_gas(&self) -> BalanceOf<T> {
		let signed_gas = match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::total_consumed_gas(self, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::total_consumed_gas(self),
		};

		signed_gas.as_positive().unwrap_or_default()
	}

	/// Get total weight consumed
	pub fn weight_consumed(&self) -> Weight {
		self.weight.weight_consumed()
	}

	/// Get total weight required
	/// This is the maxmimum amount of weight consumption that occurred during execution so far
	/// This is relevant because consumed weight can decrease in case it is asjusted a posteriori
	/// for some operations
	pub fn weight_required(&self) -> Weight {
		self.weight.weight_required()
	}

	/// Get total storage deposit consumed in the current frame.
	///
	/// Returns the net storage deposit change from this frame,
	pub fn deposit_consumed(&self) -> DepositOf<T> {
		self.deposit.consumed()
	}

	/// Get maximum storage deposit required at any point.
	///
	/// Returns the highest deposit amount needed during execution,
	/// accounting for temporary storage spikes before later refunds.
	pub fn deposit_required(&self) -> DepositOf<T> {
		self.deposit.max_charged()
	}

	/// Get the Ethereum gas that has been consumed during the lifetime of this meter
	pub fn eth_gas_consumed(&self) -> SignedGas<T> {
		match &self.transaction_limits {
			TransactionLimits::EthereumGas { eth_tx_info, .. } =>
				math::ethereum_execution::eth_gas_consumed(self, eth_tx_info),
			TransactionLimits::WeightAndDeposit { .. } =>
				math::substrate_execution::eth_gas_consumed(self),
		}
	}
}

impl<T: Config> TransactionMeter<T> {
	/// Create a new transaction-level meter with the specified resource limits.
	///
	/// Initializes either:
	/// - An ethereum-style gas-based meter or
	/// - A substrate-style meter with explicit weight and deposit limits
	pub fn new(transaction_limits: TransactionLimits<T>) -> Result<Self, DispatchError> {
		match transaction_limits {
			TransactionLimits::EthereumGas { eth_gas_limit, maybe_weight_limit, eth_tx_info } =>
				math::ethereum_execution::new_root(eth_gas_limit, maybe_weight_limit, eth_tx_info),
			TransactionLimits::WeightAndDeposit { weight_limit, deposit_limit } =>
				math::substrate_execution::new_root(weight_limit, deposit_limit),
		}
	}

	/// Convenience constructor for substrate-style weight+deposit limits.
	pub fn new_from_limits(
		weight_limit: Weight,
		deposit_limit: BalanceOf<T>,
	) -> Result<Self, DispatchError> {
		Self::new(TransactionLimits::WeightAndDeposit { weight_limit, deposit_limit })
	}

	/// Execute all postponed storage deposit operations.
	///
	/// Returns `Err(Error::StorageDepositNotEnoughFunds)` if deposit limit would be exceeded.
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

	/// Absorb resources from a terminated contract.
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
	/// Record a contract's storage deposit and schedule the transfer.
	///
	/// Updates the frame's deposit accounting and schedules the actual token transfer
	/// for later execution – at the end of the transaction execution.
	pub fn charge_contract_deposit_and_transfer(
		&mut self,
		contract: T::AccountId,
		amount: DepositOf<T>,
	) {
		self.deposit.charge_deposit(contract, amount)
	}

	/// Record storage changes of a contract.
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

/// Ethereum transaction context for gas conversions.
///
/// Contains the parameters needed to convert between ethereum gas and substrate resources
/// (weight/deposit)
#[derive(DebugNoBound, Clone)]
pub struct EthTxInfo<T: Config> {
	/// The encoding length of the extrinsic
	pub encoded_len: u32,
	/// The extra weight of the transaction. The total weight of the extrinsic is `extra_weight` +
	/// the weight consumed during smart contract execution.
	pub extra_weight: Weight,
	_phantom: PhantomData<T>,
}

impl<T: Config> EthTxInfo<T> {
	/// Create a new ethereum transaction context with the given parameters.
	pub fn new(encoded_len: u32, extra_weight: Weight) -> Self {
		Self { encoded_len, extra_weight, _phantom: PhantomData }
	}

	/// Calculate total gas consumed by weight and storage operations.
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

	/// Calculate maximal possible remaining weight that can be consumed given a particular gas
	/// limit.
	///
	/// Returns None if remaining gas would not allow any more weight consumption.
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
