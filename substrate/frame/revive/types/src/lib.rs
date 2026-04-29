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

//! Versioned wire types for the `pallet-revive` runtime API boundary.
//!
//! # Purpose
//!
//! This crate provides the stable, versioned types exchanged across the runtime/client
//! boundary (the "wire format"). It is intentionally kept lightweight so that off-chain
//! tools, light clients, and indexers can depend on it without pulling in the entire
//! `pallet-revive` crate.
//!
//! # Design principles
//!
//! ## Separate wire types from execution types
//!
//! `pallet-revive` maintains two distinct sets of types:
//!
//! * **Wire types** (this crate): stable, SCALE-encoded types exposed through the
//!   `ReviveApi` runtime API. They must remain backward-compatible across API versions.
//! * **Execution types** (inside `pallet-revive`): runtime-internal types used during
//!   contract execution. These may carry additional context not suitable for serialisation
//!   and are free to evolve independently.
//!
//! ## Versioning convention
//!
//! * Concrete versioned structs are named with a `VN` suffix: `ContractResultV1`,
//!   `ContractResultV2`, etc.
//! * The un-suffixed name (e.g. `ContractResult`) is always a type alias for the
//!   **latest** version.
//! * When a breaking change is needed, a new versioned struct is introduced and the
//!   alias is updated. The old version is kept so it can be referenced with
//!   `#[changed_in(N)]` in the `ReviveApi` runtime API trait.
//!
//! ## `define_versioned_type!` macro
//!
//! Use the [`define_versioned_type!`] macro to define a new versioned struct. The macro
//! automatically generates the `VN`-suffixed concrete type and the un-suffixed alias.
//! When upgrading to a new version, provide a `From` implementation to convert the
//! previous version into the new one; this enables backwards-compatible decoding on the
//! client side.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{string::String, vec::Vec};

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{H160, H256, U256};
use sp_runtime::{
	DispatchError,
	traits::{CheckedSub, One, Saturating, Zero},
};
use sp_weights::Weight;

pub use pallet_revive_uapi::ReturnFlags;

/// Define a versioned wire type.
///
/// This macro generates:
///
/// 1. A concrete struct named `<Name>V<N>` (e.g. `ContractResultV1`) that carries all
///    of the specified fields together with the requested `#[derive(…)]` attributes.
/// 2. A public type alias `<Name> = <Name>V<N>` that always points to the *latest*
///    defined version. When you introduce `V2`, re-run the macro with `version = 2` and
///    the alias will update automatically.
///
/// # Syntax
///
/// The `version = N,` clause **must come first**, before any doc comments or derive
/// attributes:
///
/// ```ignore
/// define_versioned_type! {
///     version = 1,
///     /// Optional doc comment.
///     #[derive(Clone, Encode, Decode, TypeInfo)]
///     pub struct MyType<A, B> {
///         pub field_a: A,
///         pub field_b: B,
///     }
/// }
/// ```
///
/// # Upgrading to a new version
///
/// ```ignore
/// // v2 — add a new field; provide a From conversion from v1.
/// define_versioned_type! {
///     version = 2,
///     #[derive(Clone, Encode, Decode, TypeInfo)]
///     pub struct MyType<A, B> {
///         pub field_a: A,
///         pub field_b: B,
///         pub new_field: u32,
///     }
/// }
///
/// impl<A, B> From<MyTypeV1<A, B>> for MyTypeV2<A, B> {
///     fn from(v1: MyTypeV1<A, B>) -> Self {
///         MyTypeV2 { field_a: v1.field_a, field_b: v1.field_b, new_field: 0 }
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_versioned_type {
	// The `version = N,` clause must appear first so the parser can unambiguously
	// distinguish it from the outer attribute list that follows.
	(
		version = $ver:literal,
		$(#[$outer:meta])*
		pub struct $name:ident $(<$($gen:tt),+>)? {
			$(
				$(#[$fattrs:meta])*
				pub $fname:ident: $fty:ty
			),* $(,)?
		}
	) => {
		::paste::paste! {
			$(#[$outer])*
			pub struct [<$name V $ver>] $(<$($gen),+>)? {
				$(
					$(#[$fattrs])*
					pub $fname: $fty,
				)*
			}

			/// Type alias pointing to the current (latest) version of this wire type.
			pub type $name $(<$($gen),+>)? = [<$name V $ver>] $(<$($gen),+>)?;
		}
	};
}

/// The amount of balance that was either charged or refunded in order to pay for storage.
#[derive(
	Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, MaxEncodedLen, Debug, TypeInfo,
)]
pub enum StorageDeposit<Balance> {
	/// The transaction reduced storage consumption.
	///
	/// The specified amount of balance was transferred *from* the involved deposit accounts
	/// back to the origin.
	Refund(Balance),
	/// The transaction increased storage consumption.
	///
	/// The specified amount of balance was transferred *from* the origin to the involved
	/// deposit accounts.
	Charge(Balance),
}

/// The possible errors when querying or writing a contract's storage.
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, MaxEncodedLen, Debug, TypeInfo)]
pub enum ContractAccessError {
	/// The given address does not point to a contract.
	DoesntExist,
	/// The storage key could not be decoded from the provided input data.
	KeyDecodingFailed,
	/// Writing to storage failed.
	StorageWriteFailed(DispatchError),
}

define_versioned_type! {
	version = 1,
	/// Result type returned across the runtime API boundary for `call` and `instantiate`.
	///
	/// Contains the execution result together with auxiliary information such as weight
	/// consumed and storage deposit charged.
	#[derive(Clone, Eq, PartialEq, Encode, Decode, Debug, TypeInfo)]
	pub struct ContractResult<R, Balance> {
		/// How much weight was consumed during execution.
		pub weight_consumed: Weight,
		/// How much weight is required as the weight limit for on-chain execution.
		///
		/// This value should be used as the `weight_limit` argument when submitting the
		/// corresponding extrinsic. It can differ from `weight_consumed` when weight
		/// pre-charging is applied.
		pub weight_required: Weight,
		/// How much balance was paid by the origin for storage.
		///
		/// Not charged when `result` is `Err`; all storage changes roll back on error.
		pub storage_deposit: StorageDeposit<Balance>,
		/// Maximal storage deposit at any point during execution.
		///
		/// Can exceed `storage_deposit` because intermediate allocations may be freed
		/// before the transaction completes. Always encoded as `StorageDeposit::Charge`.
		pub max_storage_deposit: StorageDeposit<Balance>,
		/// The amount of Ethereum gas consumed during execution.
		pub gas_consumed: Balance,
		/// The execution result of the contract.
		pub result: Result<R, DispatchError>
	}
}

define_versioned_type! {
	version = 1,
	/// Return value produced by a contract execution that ran to completion.
	#[derive(Clone, PartialEq, Eq, Encode, Decode, Debug, TypeInfo, Default)]
	pub struct ExecReturnValue {
		/// Flags passed along by `seal_return`. Empty when `seal_return` was never called.
		pub flags: ReturnFlags,
		/// Buffer passed along by `seal_return`. Empty when `seal_return` was never called.
		pub data: Vec<u8>
	}
}

define_versioned_type! {
	version = 1,
	/// Result of a successful contract instantiation.
	#[derive(Clone, PartialEq, Eq, Encode, Decode, Debug, TypeInfo, Default)]
	pub struct InstantiateReturnValue {
		/// The output of the called constructor.
		pub result: ExecReturnValueV1,
		/// The address of the newly deployed contract.
		pub addr: H160
	}
}

define_versioned_type! {
	version = 1,
	/// Result of a successful code upload via `bare_upload_code`.
	#[derive(Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, Debug, TypeInfo)]
	pub struct CodeUploadReturnValue<Balance> {
		/// The hash under which the uploaded code is stored on-chain.
		pub code_hash: H256,
		/// The storage deposit reserved at the caller.
		///
		/// Zero when the code already existed on-chain (de-duplicated upload).
		pub deposit: Balance
	}
}

define_versioned_type! {
	version = 1,
	/// Information returned by a successful `eth_transact` dry-run.
	#[derive(Clone, Eq, PartialEq, Default, Encode, Decode, Debug, TypeInfo)]
	pub struct EthTransactInfo<Balance> {
		/// The weight required to execute the transaction on-chain.
		pub weight_required: Weight,
		/// Final storage deposit charged.
		pub storage_deposit: Balance,
		/// Maximal storage deposit charged at any point during execution.
		pub max_storage_deposit: Balance,
		/// The weight and deposit equivalent expressed in EVM gas units.
		pub eth_gas: U256,
		/// The execution return data.
		pub data: Vec<u8>
	}
}

/// Error returned by a failed `eth_transact` dry-run.
#[derive(Clone, Eq, PartialEq, Encode, Decode, Debug, TypeInfo)]
pub enum EthTransactError {
	/// Execution reverted and returned ABI-encoded revert data.
	Data(Vec<u8>),
	/// Execution failed with a human-readable error message.
	Message(String),
}

pub type CodeUploadResult<Balance> = Result<CodeUploadReturnValue<Balance>, DispatchError>;

pub type GetStorageResult = Result<Option<Vec<u8>>, ContractAccessError>;

pub use pallet_revive_uapi::ReturnErrorCode;

impl ExecReturnValueV1 {
	/// Returns `true` if the contract reverted all storage changes.
	pub fn did_revert(&self) -> bool {
		self.flags.contains(ReturnFlags::REVERT)
	}
}

impl From<&ExecReturnValueV1> for ReturnErrorCode {
	fn from(from: &ExecReturnValueV1) -> Self {
		if from.flags.contains(ReturnFlags::REVERT) {
			Self::CalleeReverted
		} else {
			Self::Success
		}
	}
}

impl<T, Balance> ContractResultV1<T, Balance> {
	/// Map the inner `result` value while keeping all other fields unchanged.
	pub fn map_result<V>(self, map_fn: impl FnOnce(T) -> V) -> ContractResultV1<V, Balance> {
		ContractResultV1 {
			weight_consumed: self.weight_consumed,
			weight_required: self.weight_required,
			storage_deposit: self.storage_deposit,
			max_storage_deposit: self.max_storage_deposit,
			gas_consumed: self.gas_consumed,
			result: self.result.map(map_fn),
		}
	}
}

impl<R: Default, B: Zero + Default> Default for ContractResultV1<R, B> {
	fn default() -> Self {
		Self {
			weight_consumed: Default::default(),
			weight_required: Default::default(),
			storage_deposit: Default::default(),
			max_storage_deposit: Default::default(),
			gas_consumed: Default::default(),
			result: Ok(Default::default()),
		}
	}
}

impl<Balance: Zero> Default for StorageDeposit<Balance> {
	fn default() -> Self {
		Self::Charge(Zero::zero())
	}
}

impl<Balance: Zero + Copy> StorageDeposit<Balance> {
	/// Returns how much balance is charged, or `0` when this is a refund.
	pub fn charge_or_zero(&self) -> Balance {
		match self {
			Self::Charge(amount) => *amount,
			Self::Refund(_) => Zero::zero(),
		}
	}

	/// Returns `true` if the deposit amount is zero.
	pub fn is_zero(&self) -> bool {
		match self {
			Self::Charge(amount) => amount.is_zero(),
			Self::Refund(amount) => amount.is_zero(),
		}
	}
}

impl<Balance> StorageDeposit<Balance>
where
	Balance: Saturating + Ord + Copy + Zero + One + CheckedSub,
{
	/// Saturating signed addition of two `StorageDeposit` values.
	pub fn saturating_add(&self, rhs: &Self) -> Self {
		use StorageDeposit::*;
		match (self, rhs) {
			(Charge(lhs), Charge(rhs)) => Charge(lhs.saturating_add(*rhs)),
			(Refund(lhs), Refund(rhs)) => Refund(lhs.saturating_add(*rhs)),
			(Charge(lhs), Refund(rhs)) => {
				if lhs >= rhs {
					Charge(lhs.saturating_sub(*rhs))
				} else {
					Refund(rhs.saturating_sub(*lhs))
				}
			},
			(Refund(lhs), Charge(rhs)) => {
				if lhs > rhs {
					Refund(lhs.saturating_sub(*rhs))
				} else {
					Charge(rhs.saturating_sub(*lhs))
				}
			},
		}
	}

	/// Saturating signed subtraction of two `StorageDeposit` values.
	pub fn saturating_sub(&self, rhs: &Self) -> Self {
		use StorageDeposit::*;
		match (self, rhs) {
			(Charge(lhs), Refund(rhs)) => Charge(lhs.saturating_add(*rhs)),
			(Refund(lhs), Charge(rhs)) => Refund(lhs.saturating_add(*rhs)),
			(Charge(lhs), Charge(rhs)) => {
				if lhs >= rhs {
					Charge(lhs.saturating_sub(*rhs))
				} else {
					Refund(rhs.saturating_sub(*lhs))
				}
			},
			(Refund(lhs), Refund(rhs)) => {
				if lhs > rhs {
					Refund(lhs.saturating_sub(*rhs))
				} else {
					Charge(rhs.saturating_sub(*lhs))
				}
			},
		}
	}

	/// How much balance remains available from `limit` after applying this deposit.
	///
	/// Returns `None` if a charge exceeds the limit.
	/// Returns a value larger than `limit` in the case of a refund.
	pub fn available(&self, limit: &Balance) -> Option<Balance> {
		use StorageDeposit::*;
		match self {
			Charge(amount) => limit.checked_sub(amount),
			Refund(amount) => Some(limit.saturating_add(*amount)),
		}
	}
}
