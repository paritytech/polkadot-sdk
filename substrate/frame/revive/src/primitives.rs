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

//! A crate that hosts a common definitions that are relevant for the pallet-revive.

use crate::{BalanceOf, Config, H160, U256};
use alloc::{string::String, vec::Vec};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::weights::Weight;
use pallet_revive_uapi::ReturnFlags;
use scale_info::TypeInfo;
use sp_arithmetic::traits::Bounded;
use sp_core::Get;
use sp_runtime::{
	traits::{One, Saturating, Zero},
	DispatchError, RuntimeDebug,
};

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum DepositLimit<Balance> {
	/// Allows bypassing all balance transfer checks.
	UnsafeOnlyForDryRun,

	/// Specifies a maximum allowable balance for a deposit.
	Balance(Balance),
}

impl<T> DepositLimit<T> {
	pub fn is_unchecked(&self) -> bool {
		match self {
			Self::UnsafeOnlyForDryRun => true,
			_ => false,
		}
	}
}

impl<T> From<T> for DepositLimit<T> {
	fn from(value: T) -> Self {
		Self::Balance(value)
	}
}

impl<T: Bounded + Copy> DepositLimit<T> {
	pub fn limit(&self) -> T {
		match self {
			Self::UnsafeOnlyForDryRun => T::max_value(),
			Self::Balance(limit) => *limit,
		}
	}
}

/// Result type of a `bare_call` or `bare_instantiate` call as well as `ContractsApi::call` and
/// `ContractsApi::instantiate`.
///
/// It contains the execution result together with some auxiliary information.
///
/// #Note
///
/// It has been extended to include `events` at the end of the struct while not bumping the
/// `ContractsApi` version. Therefore when SCALE decoding a `ContractResult` its trailing data
/// should be ignored to avoid any potential compatibility issues.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct ContractResult<R, Balance> {
	/// How much weight was consumed during execution.
	pub gas_consumed: Weight,
	/// How much weight is required as gas limit in order to execute this call.
	///
	/// This value should be used to determine the weight limit for on-chain execution.
	///
	/// # Note
	///
	/// This can only be different from [`Self::gas_consumed`] when weight pre charging
	/// is used. Currently, only `seal_call_runtime` makes use of pre charging.
	/// Additionally, any `seal_call` or `seal_instantiate` makes use of pre-charging
	/// when a non-zero `gas_limit` argument is supplied.
	pub gas_required: Weight,
	/// How much balance was paid by the origin into the contract's deposit account in order to
	/// pay for storage.
	///
	/// The storage deposit is never actually charged from the origin in case of [`Self::result`]
	/// is `Err`. This is because on error all storage changes are rolled back including the
	/// payment of the deposit.
	pub storage_deposit: StorageDeposit<Balance>,
	/// The execution result of the vm binary code.
	pub result: Result<R, DispatchError>,
}

/// The result of the execution of a `eth_transact` call.
#[derive(Clone, Eq, PartialEq, Default, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct EthTransactInfo<Balance> {
	/// The amount of gas that was necessary to execute the transaction.
	pub gas_required: Weight,
	/// Storage deposit charged.
	pub storage_deposit: Balance,
	/// The weight and deposit equivalent in EVM Gas.
	pub eth_gas: U256,
	/// The execution return value.
	pub data: Vec<u8>,
}

/// Error type of a `eth_transact` call.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum EthTransactError {
	Data(Vec<u8>),
	Message(String),
}

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
/// Error encountered while creating a BalanceWithDust from a U256 balance.
pub enum BalanceConversionError {
	/// Error encountered while creating the main balance value.
	Value,
	/// Error encountered while creating the dust value.
	Dust,
}

/// A Balance amount along with some "dust" to represent the lowest decimals that can't be expressed
/// in the native currency
#[derive(Default, Clone, Copy, Eq, PartialEq, Debug)]
pub struct BalanceWithDust<Balance> {
	/// The value expressed in the native currency
	value: Balance,
	/// The dust, representing up to 1 unit of the native currency.
	/// The dust is bounded between 0 and `crate::Config::NativeToEthRatio`
	dust: u32,
}

impl<Balance> From<Balance> for BalanceWithDust<Balance> {
	fn from(value: Balance) -> Self {
		Self { value, dust: 0 }
	}
}

impl<Balance> BalanceWithDust<Balance> {
	/// Deconstructs the `BalanceWithDust` into its components.
	pub fn deconstruct(self) -> (Balance, u32) {
		(self.value, self.dust)
	}

	/// Creates a new `BalanceWithDust` with the given value and dust.
	pub fn new_unchecked<T: Config>(value: Balance, dust: u32) -> Self {
		debug_assert!(dust < T::NativeToEthRatio::get());
		Self { value, dust }
	}

	/// Creates a new `BalanceWithDust` from the given EVM value.
	pub fn from_value<T: Config>(
		value: U256,
	) -> Result<BalanceWithDust<BalanceOf<T>>, BalanceConversionError>
	where
		BalanceOf<T>: TryFrom<U256>,
	{
		if value.is_zero() {
			return Ok(Default::default())
		}

		let (quotient, remainder) = value.div_mod(T::NativeToEthRatio::get().into());
		let value = quotient.try_into().map_err(|_| BalanceConversionError::Value)?;
		let dust = remainder.try_into().map_err(|_| BalanceConversionError::Dust)?;

		Ok(BalanceWithDust { value, dust })
	}
}

impl<Balance: Zero + One + Saturating> BalanceWithDust<Balance> {
	/// Returns true if both the value and dust are zero.
	pub fn is_zero(&self) -> bool {
		self.value.is_zero() && self.dust == 0
	}

	/// Returns the Balance rounded to the nearest whole unit if the dust is non-zero.
	pub fn into_rounded_balance(self) -> Balance {
		if self.dust == 0 {
			self.value
		} else {
			self.value.saturating_add(Balance::one())
		}
	}
}

/// Result type of a `bare_code_upload` call.
pub type CodeUploadResult<Balance> = Result<CodeUploadReturnValue<Balance>, DispatchError>;

/// Result type of a `get_storage` call.
pub type GetStorageResult = Result<Option<Vec<u8>>, ContractAccessError>;

/// The possible errors that can happen querying the storage of a contract.
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, MaxEncodedLen, RuntimeDebug, TypeInfo)]
pub enum ContractAccessError {
	/// The given address doesn't point to a contract.
	DoesntExist,
	/// Storage key cannot be decoded from the provided input data.
	KeyDecodingFailed,
}

/// Output of a contract call or instantiation which ran to completion.
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo, Default)]
pub struct ExecReturnValue {
	/// Flags passed along by `seal_return`. Empty when `seal_return` was never called.
	pub flags: ReturnFlags,
	/// Buffer passed along by `seal_return`. Empty when `seal_return` was never called.
	pub data: Vec<u8>,
}

impl ExecReturnValue {
	/// The contract did revert all storage changes.
	pub fn did_revert(&self) -> bool {
		self.flags.contains(ReturnFlags::REVERT)
	}
}

/// The result of a successful contract instantiation.
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct InstantiateReturnValue {
	/// The output of the called constructor.
	pub result: ExecReturnValue,
	/// The address of the new contract.
	pub addr: H160,
}

/// The result of successfully uploading a contract.
#[derive(Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, RuntimeDebug, TypeInfo)]
pub struct CodeUploadReturnValue<Balance> {
	/// The key under which the new code is stored.
	pub code_hash: sp_core::H256,
	/// The deposit that was reserved at the caller. Is zero when the code already existed.
	pub deposit: Balance,
}

/// Reference to an existing code hash or a new vm module.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum Code {
	/// A vm module as raw bytes.
	Upload(Vec<u8>),
	/// The code hash of an on-chain vm binary blob.
	Existing(sp_core::H256),
}

/// The amount of balance that was either charged or refunded in order to pay for storage.
#[derive(
	Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, MaxEncodedLen, RuntimeDebug, TypeInfo,
)]
pub enum StorageDeposit<Balance> {
	/// The transaction reduced storage consumption.
	///
	/// This means that the specified amount of balance was transferred from the involved
	/// deposit accounts to the origin.
	Refund(Balance),
	/// The transaction increased storage consumption.
	///
	/// This means that the specified amount of balance was transferred from the origin
	/// to the involved deposit accounts.
	Charge(Balance),
}

impl<Balance: Zero> Default for StorageDeposit<Balance> {
	fn default() -> Self {
		Self::Charge(Zero::zero())
	}
}

impl<Balance: Zero + Copy> StorageDeposit<Balance> {
	/// Returns how much balance is charged or `0` in case of a refund.
	pub fn charge_or_zero(&self) -> Balance {
		match self {
			Self::Charge(amount) => *amount,
			Self::Refund(_) => Zero::zero(),
		}
	}

	pub fn is_zero(&self) -> bool {
		match self {
			Self::Charge(amount) => amount.is_zero(),
			Self::Refund(amount) => amount.is_zero(),
		}
	}
}

impl<Balance> StorageDeposit<Balance>
where
	Balance: Saturating + Ord + Copy,
{
	/// This is essentially a saturating signed add.
	pub fn saturating_add(&self, rhs: &Self) -> Self {
		use StorageDeposit::*;
		match (self, rhs) {
			(Charge(lhs), Charge(rhs)) => Charge(lhs.saturating_add(*rhs)),
			(Refund(lhs), Refund(rhs)) => Refund(lhs.saturating_add(*rhs)),
			(Charge(lhs), Refund(rhs)) =>
				if lhs >= rhs {
					Charge(lhs.saturating_sub(*rhs))
				} else {
					Refund(rhs.saturating_sub(*lhs))
				},
			(Refund(lhs), Charge(rhs)) =>
				if lhs > rhs {
					Refund(lhs.saturating_sub(*rhs))
				} else {
					Charge(rhs.saturating_sub(*lhs))
				},
		}
	}

	/// This is essentially a saturating signed sub.
	pub fn saturating_sub(&self, rhs: &Self) -> Self {
		use StorageDeposit::*;
		match (self, rhs) {
			(Charge(lhs), Refund(rhs)) => Charge(lhs.saturating_add(*rhs)),
			(Refund(lhs), Charge(rhs)) => Refund(lhs.saturating_add(*rhs)),
			(Charge(lhs), Charge(rhs)) =>
				if lhs >= rhs {
					Charge(lhs.saturating_sub(*rhs))
				} else {
					Refund(rhs.saturating_sub(*lhs))
				},
			(Refund(lhs), Refund(rhs)) =>
				if lhs > rhs {
					Refund(lhs.saturating_sub(*rhs))
				} else {
					Charge(rhs.saturating_sub(*lhs))
				},
		}
	}

	/// If the amount of deposit (this type) is constrained by a `limit` this calculates how
	/// much balance (if any) is still available from this limit.
	///
	/// # Note
	///
	/// In case of a refund the return value can be larger than `limit`.
	pub fn available(&self, limit: &Balance) -> Balance {
		use StorageDeposit::*;
		match self {
			Charge(amount) => limit.saturating_sub(*amount),
			Refund(amount) => limit.saturating_add(*amount),
		}
	}
}

/// Indicates whether the account nonce should be incremented after instantiating a new contract.
///
/// In Substrate, where transactions can be batched, the account's nonce should be incremented after
/// each instantiation, ensuring that each instantiation uses a unique nonce.
///
/// For transactions sent from Ethereum wallets, which cannot be batched, the nonce should only be
/// incremented once. In these cases, Use `BumpNonce::No` to suppress an extra nonce increment.
///
/// Note:
/// The origin's nonce is already incremented pre-dispatch by the `CheckNonce` transaction
/// extension.
pub enum BumpNonce {
	/// Do not increment the nonce after contract instantiation
	No,
	/// Increment the nonce after contract instantiation
	Yes,
}

/// Indicates whether the code was removed after the last refcount was decremented.
#[must_use = "You must handle whether the code was removed or not."]
pub enum CodeRemoved {
	/// The code was not removed. (refcount > 0)
	No,
	/// The code was removed. (refcount == 0)
	Yes,
}
