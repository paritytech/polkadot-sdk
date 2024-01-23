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

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::derive_impl;

pub use pallet::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use frame_support::pallet_prelude::*;

	// The struct on which we build all of our Pallet logic.
	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	// Your Pallet's configuration trait, representing custom external types and interfaces.
	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {}
	
	#[pallet::composite_enum]
	pub enum HoldReason<I: 'static = ()> {
		SomeHoldReason
	}

	#[pallet::composite_enum]
	pub enum FreezeReason<I: 'static = ()> {
		SomeFreezeReason
	}

	#[pallet::composite_enum]
	pub enum SlashReason<I: 'static = ()> {
		SomeSlashReason
	}

	#[pallet::composite_enum]
	pub enum LockId<I: 'static = ()> {
		SomeLockId
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

pub type Header = sp_runtime::generic::Header<u64, sp_runtime::traits::BlakeTwo256>;
pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<u64, RuntimeCall, (), ()>;
pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;

frame_support::construct_runtime!(
	pub enum Runtime
	{
		// Exclude part `Storage` in order not to check its metadata in tests.
		System: frame_system,
		Pallet1: pallet,
		Pallet2: pallet::<Instance2>,
	}
);

impl pallet::Config for Runtime {}

impl pallet::Config<pallet::Instance2> for Runtime {}

fn main() {}
