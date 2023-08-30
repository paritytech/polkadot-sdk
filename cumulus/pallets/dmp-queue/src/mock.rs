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

#![cfg(test)]

use super::migration::*;

use frame_support::{
	parameter_types,
	traits::{ConstU16, ConstU64, Footprint, HandleMessage},
};
use sp_core::{bounded_vec::BoundedSlice, ConstU32, H256};
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};

type Block = frame_system::mocking::MockBlock<Runtime>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Runtime
	{
		System: frame_system,
		DmpQueue: crate,
	}
);

impl frame_system::Config for Runtime {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type Block = Block;
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type Nonce = u64;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<42>;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
	pub const PalletName: &'static str = "DmpQueue";
}

impl crate::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type DmpSink = MockedDmpHandler;
}

parameter_types! {
	pub static RecordedMessages: u32 = 0;
}

pub struct MockedDmpHandler;
impl HandleMessage for MockedDmpHandler {
	type MaxMessageLen = ConstU32<16>;

	fn handle_message(_: BoundedSlice<u8, Self::MaxMessageLen>) {
		RecordedMessages::mutate(|n| *n += 1);
	}

	fn handle_messages<'a>(_: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>) {
		unimplemented!()
	}

	fn sweep_queue() {
		unimplemented!()
	}

	fn footprint() -> Footprint {
		unimplemented!()
	}
}

/// TESTING ONLY
#[cfg(feature = "try-runtime")]
pub(crate) fn pre_upgrade_checks<T: crate::Config>() {
	let index = PageIndex::<T>::get();

	// Check that all pages are present.
	assert!(index.begin_used <= index.end_used, "Invalid page index");
	for p in index.begin_used..index.end_used {
		assert!(Pages::<T>::contains_key(p), "Missing page");
		assert!(Pages::<T>::get(p).len() > 0, "Empty page");
	}

	// Check that all overweight messages are present.
	for i in 0..index.overweight_count {
		assert!(Overweight::<T>::contains_key(i), "Missing overweight message");
	}
}

/// TESTING ONLY
#[cfg(feature = "try-runtime")]
pub(crate) fn post_upgrade_checks<T: crate::Config>() {
	let index = PageIndex::<T>::get();

	// Check that all pages are removed.
	for p in index.begin_used..index.end_used {
		assert!(!Pages::<T>::contains_key(p), "Page should be gone");
	}
	assert!(Pages::<T>::iter_keys().next().is_none(), "Un-indexed pages");

	// Check that all overweight messages are removed.
	for i in 0..index.overweight_count {
		assert!(!Overweight::<T>::contains_key(i), "Overweight message should be gone");
	}
	assert!(Overweight::<T>::iter_keys().next().is_none(), "Un-indexed overweight messages");
}
