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

//! Helpers for tests.

#![cfg(feature = "std")]

use crate::{paged_list::StoragePagedListMeta, Config, ListPrefix};
use frame_support::derive_impl;
use sp_runtime::{traits::IdentityLookup, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		PagedList: crate,
		PagedList2: crate::<Instance2>,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Nonce = u64;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
}

frame_support::parameter_types! {
	pub storage ValuesPerNewPage: u32 = 5;
	pub const MaxPages: Option<u32> = Some(20);
}

impl crate::Config for Test {
	type Value = u32;
	type ValuesPerNewPage = ValuesPerNewPage;
}

impl crate::Config<crate::Instance2> for Test {
	type Value = u32;
	type ValuesPerNewPage = ValuesPerNewPage;
}

pub type MetaOf<T, I> =
	StoragePagedListMeta<ListPrefix<T, I>, <T as Config>::Value, <T as Config>::ValuesPerNewPage>;

/// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}

/// Run this closure in test externalities.
pub fn test_closure<R>(f: impl FnOnce() -> R) -> R {
	let mut ext = new_test_ext();
	ext.execute_with(f)
}
