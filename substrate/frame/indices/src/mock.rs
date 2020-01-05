// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Test utilities

#![cfg(test)]

use std::{cell::RefCell, collections::HashSet};
use sp_runtime::testing::Header;
use sp_runtime::Perbill;
use sp_core::H256;
use frame_support::{impl_outer_origin, parameter_types, weights::Weight};
use crate::{GenesisConfig, Module, Trait, IsDeadAccount, OnNewAccount, ResolveHint};

impl_outer_origin!{
	pub enum Origin for Runtime where system = frame_system {}
}

thread_local! {
	static ALIVE: RefCell<HashSet<u64>> = Default::default();
}

pub fn make_account(who: u64) {
	ALIVE.with(|a| a.borrow_mut().insert(who));
	Indices::on_new_account(&who);
}

pub fn kill_account(who: u64) {
	ALIVE.with(|a| a.borrow_mut().remove(&who));
}

pub struct TestIsDeadAccount {}
impl IsDeadAccount<u64> for TestIsDeadAccount {
	fn is_dead_account(who: &u64) -> bool {
		!ALIVE.with(|a| a.borrow_mut().contains(who))
	}
}

pub struct TestResolveHint;
impl ResolveHint<u64, u64> for TestResolveHint {
	fn resolve_hint(who: &u64) -> Option<u64> {
		if *who < 256 {
			None
		} else {
			Some(*who - 256)
		}
	}
}

// Workaround for https://github.com/rust-lang/rust/issues/26925 . Remove when sorted.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Runtime;
parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const MaximumBlockWeight: Weight = 1024;
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
}

impl frame_system::Trait for Runtime {
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = u64;
	type Call = ();
	type Hash = H256;
	type Hashing = ::sp_runtime::traits::BlakeTwo256;
	type AccountId = u64;
	type Lookup = Indices;
	type Header = Header;
	type Event = ();
	type BlockHashCount = BlockHashCount;
	type MaximumBlockWeight = MaximumBlockWeight;
	type MaximumBlockLength = MaximumBlockLength;
	type AvailableBlockRatio = AvailableBlockRatio;
	type Version = ();
	type ModuleToIndex = ();
}

impl Trait for Runtime {
	type AccountIndex = u64;
	type IsDeadAccount = TestIsDeadAccount;
	type ResolveHint = TestResolveHint;
	type Event = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	{
		ALIVE.with(|a| {
			let mut h = a.borrow_mut();
			h.clear();
			for i in 1..5 { h.insert(i); }
		});
	}

	let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();
	GenesisConfig::<Runtime> {
		ids: vec![1, 2, 3, 4]
	}.assimilate_storage(&mut t).unwrap();
	t.into()
}

pub type Indices = Module<Runtime>;
