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

//! Types used inside `pallet-revive`.
//!
//! # Wire types vs execution types
//!
//! This module maintains a clear distinction between two categories of types:
//!
//! ## Wire types (re-exported from `pallet-revive-types`)
//!
//! Stable, SCALE-encoded types exchanged at the `ReviveApi` runtime API boundary.
//! They are defined in the lightweight [`pallet_revive_types`] crate so that
//! off-chain tooling, light clients, and indexers can depend on them without
//! pulling in the full `pallet-revive` crate.
//!
//! * [`ContractResult`] / [`ContractResultV1`]
//! * [`ExecReturnValue`] / [`ExecReturnValueV1`]
//! * [`InstantiateReturnValue`] / [`InstantiateReturnValueV1`]
//! * [`CodeUploadReturnValue`] / [`CodeUploadReturnValueV1`]
//! * [`EthTransactInfo`] / [`EthTransactInfoV1`]
//! * [`EthTransactError`]
//! * [`StorageDeposit`]
//! * [`ContractAccessError`]
//! * [`CodeUploadResult`] / [`GetStorageResult`]
//!
//! ## Execution types (defined here, pallet-internal)
//!
//! Runtime-internal types used during contract execution that may carry additional
//! context unsuitable for SCALE serialisation or that need to evolve independently.
//!
//! * [`ExecConfig`]
//! * [`BalanceWithDust`] / [`BalanceConversionError`]
//! * [`Code`]
//! * [`CodeRemoved`]
//! * [`SetStorageResult`]

use crate::{
	BalanceOf, Config, Time, U256, evm::DryRunConfig, mock::MockHandler, storage::WriteOutcome,
	transient_storage::TransientStorage,
};
use alloc::{boxed::Box, fmt::Debug};
use core::cell::RefCell;
use frame_support::{DefaultNoBound, weights::Weight};
use sp_core::Get;
use sp_runtime::traits::{One, Saturating, Zero};

pub use pallet_revive_types::{
	// unversioned aliases (always point to the latest version)
	CodeUploadResult,
	CodeUploadReturnValue,
	// versioned concrete types
	CodeUploadReturnValueV1,
	ContractAccessError,
	ContractResult,
	ContractResultV1,
	EthTransactError,
	EthTransactInfo,
	EthTransactInfoV1,
	ExecReturnValue,
	ExecReturnValueV1,
	GetStorageResult,
	InstantiateReturnValue,
	InstantiateReturnValueV1,
	StorageDeposit,
};

/// Result type of a `set_storage` call.
pub type SetStorageResult = Result<WriteOutcome, ContractAccessError>;

/// Error encountered while creating a [`BalanceWithDust`] from a `U256` balance.
#[derive(Clone, Eq, PartialEq, codec::Encode, codec::Decode, Debug, scale_info::TypeInfo)]
pub enum BalanceConversionError {
	/// Error converting the main balance value.
	Value,
	/// Error converting the dust component.
	Dust,
}

/// A balance amount together with "dust" — the sub-unit remainder that cannot be
/// expressed in the native currency denomination.
#[derive(Default, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct BalanceWithDust<Balance> {
	/// The value expressed in the native currency.
	value: Balance,
	/// Fractional part bounded between `0` and `crate::Config::NativeToEthRatio`.
	dust: u32,
}

impl<Balance> From<Balance> for BalanceWithDust<Balance> {
	fn from(value: Balance) -> Self {
		Self { value, dust: 0 }
	}
}

impl<Balance> BalanceWithDust<Balance> {
	/// Deconstructs into `(value, dust)`.
	pub fn deconstruct(self) -> (Balance, u32) {
		(self.value, self.dust)
	}

	/// Creates a `BalanceWithDust` without checking the dust bound.
	pub fn new_unchecked<T: Config>(value: Balance, dust: u32) -> Self {
		debug_assert!(dust < T::NativeToEthRatio::get());
		Self { value, dust }
	}

	/// Creates a `BalanceWithDust` from the given EVM `U256` value.
	pub fn from_value<T: Config>(
		value: U256,
	) -> Result<BalanceWithDust<BalanceOf<T>>, BalanceConversionError> {
		if value.is_zero() {
			return Ok(Default::default());
		}

		let (quotient, remainder) = value.div_mod(T::NativeToEthRatio::get().into());
		let value = quotient.try_into().map_err(|_| BalanceConversionError::Value)?;
		let dust = remainder.try_into().map_err(|_| BalanceConversionError::Dust)?;

		Ok(BalanceWithDust { value, dust })
	}
}

impl<Balance: Zero + One + Saturating> BalanceWithDust<Balance> {
	/// Returns `true` if both the value and dust are zero.
	pub fn is_zero(&self) -> bool {
		self.value.is_zero() && self.dust == 0
	}

	/// Returns the balance rounded up to the nearest whole unit when dust is non-zero.
	pub fn into_rounded_balance(self) -> Balance {
		if self.dust == 0 { self.value } else { self.value.saturating_add(Balance::one()) }
	}
}

/// Reference to existing on-chain code or a new bytecode upload.
#[derive(Clone, Eq, PartialEq, codec::Encode, codec::Decode, Debug, scale_info::TypeInfo)]
pub enum Code {
	/// Raw bytecode to be uploaded and stored.
	Upload(alloc::vec::Vec<u8>),
	/// The hash of code that is already stored on-chain.
	Existing(sp_core::H256),
}

/// `Stack`-wide configuration options that govern a single execution.
#[derive(DefaultNoBound)]
pub struct ExecConfig<T: Config> {
	/// Whether the account nonce should be incremented after a contract instantiation.
	///
	/// Set to `true` for Substrate transactions (which can be batched) and `false` for
	/// Ethereum transactions (which cannot be batched; the nonce is already bumped by
	/// `CheckNonce` pre-dispatch).
	pub bump_nonce: bool,
	/// Whether deposits should be collected from a `pallet-transaction-payment` credit
	/// hold rather than free balance.
	///
	/// `Some((encoded_len, base_weight))` when collecting from hold; `None` otherwise.
	pub collect_deposit_from_hold: Option<(u32, Weight)>,
	/// The effective gas price used for this transaction.
	///
	/// Populated when the execution originates from an Ethereum transaction.
	pub effective_gas_price: Option<U256>,
	/// When set the execution runs as a dry-run.
	pub is_dry_run: Option<DryRunConfig<<<T as Config>::Time as Time>::Moment>>,
	/// An optional mock handler for testing purposes (`None` in production).
	pub mock_handler: Option<Box<dyn MockHandler<T>>>,
	/// Optional externally-supplied transient storage for test environments.
	pub test_env_transient_storage: Option<RefCell<TransientStorage<T>>>,
}

impl<T: Config> ExecConfig<T> {
	/// Default config for calls originating from a Substrate extrinsic.
	pub fn new_substrate_tx() -> Self {
		Self {
			bump_nonce: true,
			collect_deposit_from_hold: None,
			effective_gas_price: None,
			is_dry_run: None,
			mock_handler: None,
			test_env_transient_storage: None,
		}
	}

	/// Like [`Self::new_substrate_tx`] but suppresses the extra nonce bump.
	pub fn new_substrate_tx_without_bump() -> Self {
		Self {
			bump_nonce: false,
			collect_deposit_from_hold: None,
			effective_gas_price: None,
			mock_handler: None,
			is_dry_run: None,
			test_env_transient_storage: None,
		}
	}

	/// Default config for calls originating from an Ethereum transaction.
	pub fn new_eth_tx(effective_gas_price: U256, encoded_len: u32, base_weight: Weight) -> Self {
		Self {
			bump_nonce: false,
			collect_deposit_from_hold: Some((encoded_len, base_weight)),
			effective_gas_price: Some(effective_gas_price),
			mock_handler: None,
			is_dry_run: None,
			test_env_transient_storage: None,
		}
	}

	/// Mark this config as a dry-run.
	pub fn with_dry_run(
		mut self,
		dry_run_config: DryRunConfig<<<T as Config>::Time as Time>::Moment>,
	) -> Self {
		self.is_dry_run = Some(dry_run_config);
		self
	}

	/// Shallow clone for testing (does not clone `mock_handler`).
	#[cfg(test)]
	pub fn clone(&self) -> Self {
		Self {
			bump_nonce: self.bump_nonce,
			collect_deposit_from_hold: self.collect_deposit_from_hold,
			effective_gas_price: self.effective_gas_price,
			is_dry_run: self.is_dry_run.clone(),
			mock_handler: None,
			test_env_transient_storage: None,
		}
	}
}

/// Indicates whether code was removed after the last reference was dropped.
#[must_use = "You must handle whether the code was removed or not."]
pub enum CodeRemoved {
	/// Code was not removed (refcount > 0).
	No,
	/// Code was removed (refcount reached 0).
	Yes,
}
