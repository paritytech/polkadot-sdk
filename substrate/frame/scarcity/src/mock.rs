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

//! Mock runtime for scarcity pallet tests.

#![cfg(test)]

use crate as pallet_scarcity;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
	derive_impl, parameter_types,
	traits::{Consideration, ConstU32, ConstU64, Footprint, UnixTime},
	weights::constants::RocksDbWeight,
};
use scale_info::TypeInfo;
use sp_runtime::{traits::IdentityLookup, BuildStorage, DispatchError};
use std::{cell::RefCell, thread_local};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system = 0,
		Scarcity: pallet_scarcity = 1,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Nonce = u64;
	type Block = Block;
	type BlockHashCount = ConstU64<250>;
	type DbWeight = RocksDbWeight;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
}

parameter_types! {
	pub static MockNow: u64 = 0;
	pub static MockDropFails: bool = false;
}

/// A recorded action performed by the test consideration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsiderationEvent {
	New { who: u64, footprint: Footprint },
	Update { who: u64, footprint: Footprint },
	Drop { who: u64 },
}

thread_local! {
	static CONSIDERATION_EVENTS: RefCell<Vec<ConsiderationEvent>> = const { RefCell::new(Vec::new()) };
}

/// Zero-sized deposit ticket that records charges and releases in thread-local test state.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct TestConsideration;

impl Consideration<u64, Footprint> for TestConsideration {
	fn new(who: &u64, footprint: Footprint) -> Result<Self, DispatchError> {
		CONSIDERATION_EVENTS.with(|events| {
			events.borrow_mut().push(ConsiderationEvent::New { who: *who, footprint })
		});
		Ok(Self)
	}

	fn update(self, who: &u64, footprint: Footprint) -> Result<Self, DispatchError> {
		CONSIDERATION_EVENTS.with(|events| {
			events.borrow_mut().push(ConsiderationEvent::Update { who: *who, footprint })
		});
		Ok(self)
	}

	fn drop(self, who: &u64) -> Result<(), DispatchError> {
		CONSIDERATION_EVENTS
			.with(|events| events.borrow_mut().push(ConsiderationEvent::Drop { who: *who }));
		if MockDropFails::get() {
			return Err(DispatchError::Other("test consideration drop failed"));
		}
		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_who: &u64, _footprint: Footprint) {}
}

pub fn consideration_events() -> Vec<ConsiderationEvent> {
	CONSIDERATION_EVENTS.with(|events| events.borrow().clone())
}

pub fn clear_consideration_events() {
	CONSIDERATION_EVENTS.with(|events| events.borrow_mut().clear());
}

/// Test-controlled Unix time source.
pub struct MockUnixTime;
impl UnixTime for MockUnixTime {
	fn now() -> core::time::Duration {
		core::time::Duration::from_secs(MockNow::get())
	}
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type UnixTime = MockUnixTime;
	type CollectionConsideration = TestConsideration;
	type ItemDefConsideration = TestConsideration;
	type InstanceConsideration = TestConsideration;
	type MetadataConsideration = TestConsideration;
	type MaxKeyLen = ConstU32<32>;
	type MaxValueLen = ConstU32<256>;
	type LockPeriod = ConstU64<60>;
	type MaxTransferPriority = ConstU64<1_000_000>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into();
	ext.execute_with(|| {
		MockNow::set(0);
		MockDropFails::set(false);
		clear_consideration_events();
		System::set_block_number(1);
	});
	ext
}
