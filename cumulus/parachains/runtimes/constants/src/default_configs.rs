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

#![cfg_attr(not(feature = "std"), no_std)]

use super::*;
use frame_support::{derive_impl, pallet_prelude::*};
use parachains_common::{AccountId, Balance, Hash, Nonce};
use polkadot_runtime_common::BlockHashCount;

/// A struct for which DefaultConfigs can be defined for pallets common to system parachains.
pub struct SystemParachainDefaultConfig;

/// [`frame_system::DefaultConfig`] for system parachains.
#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig, no_aggregated_types)]
#[frame_support::register_default_impl(SystemParachainDefaultConfig)]
impl frame_system::DefaultConfig for SystemParachainDefaultConfig {
	/// The default type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The default type for hashing blocks and tries.
	type Hash = Hash;
	/// The default identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The lookup mechanism to get account ID from whatever is passed in dispatchers.
	type Lookup = sp_runtime::traits::AccountIdLookup<AccountId, ()>;
	/// The maximum number of consumers allowed on a single account.
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	/// The default data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = RuntimeBlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = RuntimeBlockLength;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
}
