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

//! General tests for composite_enum macro and its handling, test for:
//! * variant_count works

#![recursion_limit = "128"]

use codec::Encode;
use frame_support::derive_impl;
use frame_support::traits::VariantCount;
use sp_core::sr25519;
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, Verify},
};

#[frame_support::pallet(dev_mode)]
mod module_single_instance {

	#[pallet::composite_enum]
	pub enum HoldReason {
		ModuleSingleInstanceReason1,
		ModuleSingleInstanceReason2,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeHoldReason: From<HoldReason>;
	}
}

#[frame_support::pallet(dev_mode)]
mod module_multi_instance {

	#[pallet::composite_enum]
	pub enum HoldReason<I: 'static = ()> {
		ModuleMultiInstanceReason1,
		ModuleMultiInstanceReason2,
		ModuleMultiInstanceReason3,
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		type RuntimeHoldReason: From<HoldReason<I>>;
	}
}

#[frame_support::pallet(dev_mode)]
mod module_consumer {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		// consume `composite_enum`
		type RuntimeHoldReason: VariantCount;
	}
}

pub type BlockNumber = u64;
pub type Signature = sr25519::Signature;
pub type AccountId = <Signature as Verify>::Signer;
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<u32, RuntimeCall, Signature, ()>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

frame_support::construct_runtime!(
	pub struct Runtime
	{
		System: frame_system::{Pallet, Call, Event<T>, Origin<T>} = 30,
		ModuleSingleInstance: module_single_instance::{HoldReason},
		ModuleMultiInstance1: module_multi_instance::<Instance1>::{HoldReason} = 51,
		ModuleMultiInstance2: module_multi_instance::<Instance2>::{HoldReason} = 52,
		ModuleMultiInstance3: module_multi_instance::<Instance3>::{HoldReason} = 53,
		ModuleConsumer: module_consumer,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

impl module_single_instance::Config for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
}

impl module_multi_instance::Config<module_multi_instance::Instance1> for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
}
impl module_multi_instance::Config<module_multi_instance::Instance2> for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
}
impl module_multi_instance::Config<module_multi_instance::Instance3> for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
}

impl module_consumer::Config for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
}

fn list_all_variants() -> Vec<RuntimeHoldReason> {
	let variants = vec![
		RuntimeHoldReason::ModuleSingleInstance(module_single_instance::HoldReason::ModuleSingleInstanceReason1),
		RuntimeHoldReason::ModuleSingleInstance(module_single_instance::HoldReason::ModuleSingleInstanceReason2),
		RuntimeHoldReason::ModuleMultiInstance1(module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason1),
		RuntimeHoldReason::ModuleMultiInstance1(module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason2),
		RuntimeHoldReason::ModuleMultiInstance1(module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason3),
		RuntimeHoldReason::ModuleMultiInstance2(module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason1),
		RuntimeHoldReason::ModuleMultiInstance2(module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason2),
		RuntimeHoldReason::ModuleMultiInstance2(module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason3),
		RuntimeHoldReason::ModuleMultiInstance3(module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason1),
		RuntimeHoldReason::ModuleMultiInstance3(module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason2),
		RuntimeHoldReason::ModuleMultiInstance3(module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason3),
	];
	// check - just in case
	for v in &variants {
		assert!(match v {
			RuntimeHoldReason::ModuleSingleInstance(inner) => match inner {
				module_single_instance::HoldReason::ModuleSingleInstanceReason1 => true,
				module_single_instance::HoldReason::ModuleSingleInstanceReason2 => true,
			}
			RuntimeHoldReason::ModuleMultiInstance1(inner) => match inner {
				module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason1
				| module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason2
				| module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason3 => true,
				module_multi_instance::HoldReason::<module_multi_instance::Instance1>::__Ignore(_) => false,
			}
			RuntimeHoldReason::ModuleMultiInstance2(inner) => match inner {
				module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason1
				| module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason2
				| module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason3=> true,
				module_multi_instance::HoldReason::<module_multi_instance::Instance2>::__Ignore(_)=> false,
			}
			RuntimeHoldReason::ModuleMultiInstance3(inner) => match inner {
				module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason1
				| module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason2
				| module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason3=> true,
				module_multi_instance::HoldReason::<module_multi_instance::Instance3>::__Ignore(_)=> false,
			}
		});
	}
	variants
}

#[test]
fn runtime_hold_reason_variant_count_works() {
	assert_eq!(RuntimeHoldReason::VARIANT_COUNT as usize, list_all_variants().len());
}

#[test]
fn check_unique_encodings_for_composite_enums() {
	let variants = list_all_variants();
	let encoded_variants =
		variants.iter().map(|v| v.encode()).collect::<std::collections::HashSet<_>>();
	assert_eq!(encoded_variants.len(), variants.len());
}
