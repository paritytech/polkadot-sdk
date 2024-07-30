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
use frame_support::{derive_impl, traits::VariantCount};
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

	#[pallet::composite_enum]
	pub enum FreezeReason {
		ModuleSingleInstanceReason1,
		ModuleSingleInstanceReason2,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeHoldReason: From<HoldReason>;
		type RuntimeFreezeReason: From<FreezeReason>;
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

	#[pallet::composite_enum]
	pub enum FreezeReason<I: 'static = ()> {
		ModuleMultiInstanceReason1,
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		type RuntimeHoldReason: From<HoldReason<I>>;
		type RuntimeFreezeReason: From<FreezeReason<I>>;
	}
}

#[frame_support::pallet(dev_mode)]
mod module_composite_enum_consumer {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		// consume `HoldReason` `composite_enum`
		type RuntimeHoldReason: VariantCount;
		// consume `FreezeReason` `composite_enum`
		type RuntimeFreezeReason: VariantCount;
	}
}

pub type BlockNumber = u64;
pub type Signature = sr25519::Signature;
pub type AccountId = <Signature as Verify>::Signer;
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<u32, RuntimeCall, Signature, ()>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

frame_support::construct_runtime!(
	pub enum Runtime
	{
		System: frame_system,
		ModuleSingleInstance: module_single_instance,
		ModuleMultiInstance0: module_multi_instance,
		ModuleMultiInstance1: module_multi_instance::<Instance1>,
		ModuleMultiInstance2: module_multi_instance::<Instance2>,
		ModuleMultiInstance3: module_multi_instance::<Instance3>,
		ModuleCompositeEnumConsumer: module_composite_enum_consumer,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

impl module_single_instance::Config for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}

impl module_multi_instance::Config for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}
impl module_multi_instance::Config<module_multi_instance::Instance1> for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}
impl module_multi_instance::Config<module_multi_instance::Instance2> for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}
impl module_multi_instance::Config<module_multi_instance::Instance3> for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}

impl module_composite_enum_consumer::Config for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}

fn list_all_hold_reason_variants() -> Vec<RuntimeHoldReason> {
	let variants = vec![
		RuntimeHoldReason::ModuleSingleInstance(module_single_instance::HoldReason::ModuleSingleInstanceReason1),
		RuntimeHoldReason::ModuleSingleInstance(module_single_instance::HoldReason::ModuleSingleInstanceReason2),
		RuntimeHoldReason::ModuleMultiInstance0(<module_multi_instance::HoldReason>::ModuleMultiInstanceReason1),
		RuntimeHoldReason::ModuleMultiInstance0(<module_multi_instance::HoldReason>::ModuleMultiInstanceReason2),
		RuntimeHoldReason::ModuleMultiInstance0(<module_multi_instance::HoldReason>::ModuleMultiInstanceReason3),
		RuntimeHoldReason::ModuleMultiInstance0(<module_multi_instance::HoldReason>::__Ignore(Default::default())),
		RuntimeHoldReason::ModuleMultiInstance1(module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason1),
		RuntimeHoldReason::ModuleMultiInstance1(module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason2),
		RuntimeHoldReason::ModuleMultiInstance1(module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason3),
		RuntimeHoldReason::ModuleMultiInstance1(module_multi_instance::HoldReason::<module_multi_instance::Instance1>::__Ignore(Default::default())),
		RuntimeHoldReason::ModuleMultiInstance2(module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason1),
		RuntimeHoldReason::ModuleMultiInstance2(module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason2),
		RuntimeHoldReason::ModuleMultiInstance2(module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason3),
		RuntimeHoldReason::ModuleMultiInstance2(module_multi_instance::HoldReason::<module_multi_instance::Instance2>::__Ignore(Default::default())),
		RuntimeHoldReason::ModuleMultiInstance3(module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason1),
		RuntimeHoldReason::ModuleMultiInstance3(module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason2),
		RuntimeHoldReason::ModuleMultiInstance3(module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason3),
		RuntimeHoldReason::ModuleMultiInstance3(module_multi_instance::HoldReason::<module_multi_instance::Instance3>::__Ignore(Default::default())),
	];
	// check that we didn't miss any value
	for v in &variants {
		match v {
			RuntimeHoldReason::ModuleSingleInstance(inner) => match inner {
				module_single_instance::HoldReason::ModuleSingleInstanceReason1
				| module_single_instance::HoldReason::ModuleSingleInstanceReason2 => (),
			}
			RuntimeHoldReason::ModuleMultiInstance0(inner) => match inner {
				<module_multi_instance::HoldReason>::ModuleMultiInstanceReason1
				| <module_multi_instance::HoldReason>::ModuleMultiInstanceReason2
				| <module_multi_instance::HoldReason>::ModuleMultiInstanceReason3
				| module_multi_instance::HoldReason::<()>::__Ignore(_) => (),
			}
			RuntimeHoldReason::ModuleMultiInstance1(inner) => match inner {
				module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason1
				| module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason2
				| module_multi_instance::HoldReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason3
				| module_multi_instance::HoldReason::<module_multi_instance::Instance1>::__Ignore(_) => (),
			}
			RuntimeHoldReason::ModuleMultiInstance2(inner) => match inner {
				module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason1
				| module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason2
				| module_multi_instance::HoldReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason3
				| module_multi_instance::HoldReason::<module_multi_instance::Instance2>::__Ignore(_) => (),
			}
			RuntimeHoldReason::ModuleMultiInstance3(inner) => match inner {
				module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason1
				| module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason2
				| module_multi_instance::HoldReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason3
				| module_multi_instance::HoldReason::<module_multi_instance::Instance3>::__Ignore(_) => (),
			}
		}
	}
	variants
}

fn list_all_freeze_reason_variants() -> Vec<RuntimeFreezeReason> {
	let variants = vec![
		RuntimeFreezeReason::ModuleSingleInstance(module_single_instance::FreezeReason::ModuleSingleInstanceReason1),
		RuntimeFreezeReason::ModuleSingleInstance(module_single_instance::FreezeReason::ModuleSingleInstanceReason2),
		RuntimeFreezeReason::ModuleMultiInstance0(<module_multi_instance::FreezeReason>::ModuleMultiInstanceReason1),
		RuntimeFreezeReason::ModuleMultiInstance0(<module_multi_instance::FreezeReason>::__Ignore(Default::default())),
		RuntimeFreezeReason::ModuleMultiInstance1(module_multi_instance::FreezeReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason1),
		RuntimeFreezeReason::ModuleMultiInstance1(module_multi_instance::FreezeReason::<module_multi_instance::Instance1>::__Ignore(Default::default())),
		RuntimeFreezeReason::ModuleMultiInstance2(module_multi_instance::FreezeReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason1),
		RuntimeFreezeReason::ModuleMultiInstance2(module_multi_instance::FreezeReason::<module_multi_instance::Instance2>::__Ignore(Default::default())),
		RuntimeFreezeReason::ModuleMultiInstance3(module_multi_instance::FreezeReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason1),
		RuntimeFreezeReason::ModuleMultiInstance3(module_multi_instance::FreezeReason::<module_multi_instance::Instance3>::__Ignore(Default::default())),
	];
	// check that we didn't miss any value
	for v in &variants {
		match v {
			RuntimeFreezeReason::ModuleSingleInstance(inner) => match inner {
				module_single_instance::FreezeReason::ModuleSingleInstanceReason1
				| module_single_instance::FreezeReason::ModuleSingleInstanceReason2 => (),
			}
			RuntimeFreezeReason::ModuleMultiInstance0(inner) => match inner {
				<module_multi_instance::FreezeReason>::ModuleMultiInstanceReason1
				| module_multi_instance::FreezeReason::<()>::__Ignore(_) => (),
			}
			RuntimeFreezeReason::ModuleMultiInstance1(inner) => match inner {
				module_multi_instance::FreezeReason::<module_multi_instance::Instance1>::ModuleMultiInstanceReason1
				| module_multi_instance::FreezeReason::<module_multi_instance::Instance1>::__Ignore(_) => (),
			}
			RuntimeFreezeReason::ModuleMultiInstance2(inner) => match inner {
				module_multi_instance::FreezeReason::<module_multi_instance::Instance2>::ModuleMultiInstanceReason1
				| module_multi_instance::FreezeReason::<module_multi_instance::Instance2>::__Ignore(_) => (),
			}
			RuntimeFreezeReason::ModuleMultiInstance3(inner) => match inner {
				module_multi_instance::FreezeReason::<module_multi_instance::Instance3>::ModuleMultiInstanceReason1
				| module_multi_instance::FreezeReason::<module_multi_instance::Instance3>::__Ignore(_) => (),
			}
		}
	}
	variants
}

#[test]
fn runtime_hold_reason_variant_count_works() {
	assert_eq!(RuntimeHoldReason::VARIANT_COUNT as usize, list_all_hold_reason_variants().len());
}

#[test]
fn runtime_freeze_reason_variant_count_works() {
	assert_eq!(
		RuntimeFreezeReason::VARIANT_COUNT as usize,
		list_all_freeze_reason_variants().len()
	);
}

#[test]
fn check_unique_encodings_for_hold_reason() {
	let variants = list_all_hold_reason_variants();
	let unique_encoded_variants =
		variants.iter().map(|v| v.encode()).collect::<std::collections::HashSet<_>>();
	assert_eq!(unique_encoded_variants.len(), variants.len());
}

#[test]
fn check_unique_encodings_for_freeze_reason() {
	let variants = list_all_freeze_reason_variants();
	let unique_encoded_variants =
		variants.iter().map(|v| v.encode()).collect::<std::collections::HashSet<_>>();
	assert_eq!(unique_encoded_variants.len(), variants.len());
}
