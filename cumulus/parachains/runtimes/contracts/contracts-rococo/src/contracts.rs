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

use crate::{
	Balance, Balances, RandomnessCollectiveFlip, Runtime, RuntimeCall, RuntimeEvent,
	RuntimeHoldReason, Timestamp,
};
use frame_support::{
	parameter_types,
	traits::{ConstBool, ConstU32, Nothing},
};
use pallet_contracts::{
	weights::SubstrateWeight, Config, DebugInfo, DefaultAddressGenerator, Frame, Schedule,
};
use sp_runtime::Perbill;

use testnet_parachains_constants::rococo::currency::deposit;

// Prints debug output of the `contracts` pallet to stdout if the node is
// started with `-lruntime::contracts=debug`.
pub const CONTRACTS_DEBUG_OUTPUT: DebugInfo = DebugInfo::UnsafeDebug;

parameter_types! {
	pub const DepositPerItem: Balance = deposit(1, 0);
	pub const DepositPerByte: Balance = deposit(0, 1);
	pub const DefaultDepositLimit: Balance = deposit(1024, 1024 * 1024);
	pub MySchedule: Schedule<Runtime> = Default::default();
	pub CodeHashLockupDepositPercent: Perbill = Perbill::from_percent(30);
}

impl Config for Runtime {
	type Time = Timestamp;
	type Randomness = RandomnessCollectiveFlip;
	type Currency = Balances;
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	/// The safest default is to allow no calls at all.
	///
	/// Runtimes should whitelist dispatchables that are allowed to be called from contracts
	/// and make sure they are stable. Dispatchables exposed to contracts are not allowed to
	/// change because that would break already deployed contracts. The `Call` structure itself
	/// is not allowed to change the indices of existing pallets, too.
	type CallFilter = Nothing;
	type DepositPerItem = DepositPerItem;
	type DepositPerByte = DepositPerByte;
	type DefaultDepositLimit = DefaultDepositLimit;
	type WeightPrice = pallet_transaction_payment::Pallet<Self>;
	type WeightInfo = SubstrateWeight<Self>;
	type ChainExtension = ();
	type Schedule = MySchedule;
	type CallStack = [Frame<Self>; 5];
	type AddressGenerator = DefaultAddressGenerator;
	type MaxCodeLen = ConstU32<{ 123 * 1024 }>;
	type MaxStorageKeyLen = ConstU32<128>;
	type UnsafeUnstableInterface = ConstBool<true>;
	type MaxDebugBufferLen = ConstU32<{ 2 * 1024 * 1024 }>;
	type MaxDelegateDependencies = ConstU32<32>;
	type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
	type Migrations = ();
	type RuntimeHoldReason = RuntimeHoldReason;
	type Debug = ();
	type Environment = ();
	type Xcm = pallet_xcm::Pallet<Self>;
}
