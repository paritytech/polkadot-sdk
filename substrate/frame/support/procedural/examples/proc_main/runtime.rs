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
#![allow(deprecated, clippy::deprecated_semver)]

use super::{frame_system, Block};
use crate::derive_impl;

#[crate::pallet(dev_mode)]
mod pallet_basic {
	use super::frame_system;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}
}

impl pallet_basic::Config for Runtime {}

#[crate::pallet(dev_mode)]
mod pallet_with_disabled_call {
	use super::frame_system;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}
}

impl pallet_with_disabled_call::Config for Runtime {}

#[crate::pallet(dev_mode)]
mod pallet_with_disabled_unsigned {
	use super::frame_system;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}
}

impl pallet_with_disabled_unsigned::Config for Runtime {}

#[crate::pallet]
mod pallet_with_instance {
	use super::frame_system;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {}
}

#[allow(unused)]
type Instance1 = pallet_with_instance::Pallet<pallet_with_instance::Instance1>;

impl pallet_with_instance::Config<pallet_with_instance::Instance1> for Runtime {}

#[allow(unused)]
type Instance2 = pallet_with_instance::Pallet<pallet_with_instance::Instance2>;

impl pallet_with_instance::Config<pallet_with_instance::Instance2> for Runtime {}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

#[docify::export(runtime_macro)]
#[crate::runtime]
mod runtime {
	// The main runtime
	#[runtime::runtime]
	// Runtime Types to be generated
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Runtime;

	// Use the concrete pallet type
	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Runtime>;

	// Use path to the pallet
	#[runtime::pallet_index(1)]
	pub type Basic = pallet_basic;

	// Use the concrete pallet type with instance
	#[runtime::pallet_index(2)]
	pub type PalletWithInstance1 = pallet_with_instance::Pallet<Runtime, Instance1>;

	// Use path to the pallet with instance
	#[runtime::pallet_index(3)]
	pub type PalletWithInstance2 = pallet_with_instance<Instance2>;

	// Ensure that the runtime does not export the calls from the pallet
	#[runtime::pallet_index(4)]
	#[runtime::disable_call]
	#[deprecated = "example"]
	pub type PalletWithDisabledCall = pallet_with_disabled_call::Pallet<Runtime>;

	// Ensure that the runtime does not export the unsigned calls from the pallet
	#[runtime::pallet_index(5)]
	#[runtime::disable_unsigned]
	pub type PalletWithDisabledUnsigned = pallet_with_disabled_unsigned::Pallet<Runtime>;
}
