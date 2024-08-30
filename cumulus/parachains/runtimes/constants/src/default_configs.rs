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

// This builds on the DefaultConfigs for the various pallets to maximise the usefulness of
// [`derive_impl`] for the system parachains runtimes for testnets.

use super::*;
use frame_support::pallet_prelude::*;
use parachains_common::{AccountId, Balance, Hash, Nonce};
use polkadot_runtime_common::BlockHashCount;

/// A struct for which DefaultConfigs can be defined for pallets common to system parachains.
pub struct SystemParachainDefaultConfig;

/// A marker type to indicate associated types that must be overridden.
pub struct MustBeOverridden;

/// [`frame_system::DefaultConfig`] for system parachains.
#[frame_support::register_default_impl(SystemParachainDefaultConfig)]
impl frame_system::DefaultConfig for SystemParachainDefaultConfig {
	/// The default type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The default type for hashing blocks and tries.
	type Hash = Hash;
	/// The default hashing algorithm used.
	type Hashing = sp_runtime::traits::BlakeTwo256;
	/// The default identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The lookup mechanism to get account ID from whatever is passed in dispatchers.
	type Lookup = sp_runtime::traits::AccountIdLookup<Self::AccountId, ()>;
	/// The maximum number of consumers allowed on a single account.
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	/// The default data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// What to do if a new account is created.
	type OnNewAccount = ();
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = ();
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = MustBeOverridden;
	/// This is used as an identifier of the chain.
	type SS58Prefix = MustBeOverridden;
	/// Version of the runtime.
	type Version = MustBeOverridden;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = MustBeOverridden;
	/// The maximum length of a block (in bytes).
	type BlockLength = RuntimeBlockLength;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = MustBeOverridden;
	/// The ubiquitous event type injected by `construct_runtime!`.
	#[inject_runtime_type]
	type RuntimeEvent = ();
	/// The ubiquitous origin type injected by `construct_runtime!`.
	#[inject_runtime_type]
	type RuntimeOrigin = ();
	/// The aggregated dispatch type available for extrinsics, injected by
	/// `construct_runtime!`.
	#[inject_runtime_type]
	type RuntimeCall = ();
	/// The aggregated Task type, injected by `construct_runtime!`.
	#[inject_runtime_type]
	type RuntimeTask = ();
	/// Converts a module to the index of the module, injected by `construct_runtime!`.
	#[inject_runtime_type]
	type PalletInfo = ();
	/// The basic call filter to use in dispatchable. Supports everything as the default.
	type BaseCallFilter = frame_support::traits::Everything;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The set code logic.
	type OnSetCode = MustBeOverridden;
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
}
