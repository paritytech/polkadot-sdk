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

//! Mock runtime for footprint pallet tests and benchmarks.

#![cfg(test)]

use crate as pallet_footprint;
use crate::{BaseAllowanceProvider, Config, HoldReason};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::cell::RefCell;
use frame_support::{
	derive_impl, parameter_types,
	traits::{tokens::fungible::InspectHold, EnsureOrigin, Get},
};
use scale_info::TypeInfo;
use sp_runtime::{traits::IdentityLookup, BuildStorage};
use std::collections::BTreeMap;

type Block = frame_system::mocking::MockBlock<Test>;
type Balance = u64;

thread_local! {
	static BASE_ALLOWANCES: RefCell<BTreeMap<u8, Option<u64>>> = RefCell::new(BTreeMap::new());
}

/// Reasons used by the mock runtime to attribute footprint usage.
#[derive(
	Clone,
	Copy,
	Debug,
	Eq,
	PartialEq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub enum TestReason {
	/// The first independently tracked feature.
	First,
	/// The second independently tracked feature.
	Second,
}

/// `Get` adapter selecting [`TestReason::First`].
pub struct FirstReason;
impl Get<TestReason> for FirstReason {
	fn get() -> TestReason {
		TestReason::First
	}
}

/// `Get` adapter selecting [`TestReason::Second`].
pub struct SecondReason;
impl Get<TestReason> for SecondReason {
	fn get() -> TestReason {
		TestReason::Second
	}
}

/// Set the current allowance returned for a mock personhood token.
pub fn set_base_allowance(token: u8, allowance: Option<u64>) {
	BASE_ALLOWANCES.with(|allowances| {
		allowances.borrow_mut().insert(token, allowance);
	});
}

/// Test provider whose mutable thread-local map can model grant, demotion, and revocation.
pub struct TestBaseAllowance;
impl BaseAllowanceProvider for TestBaseAllowance {
	type Token = u8;

	fn base_allowance(token: &Self::Token) -> Option<u64> {
		BASE_ALLOWANCES.with(|allowances| allowances.borrow().get(token).copied().flatten())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn create_token() -> Option<Self::Token> {
		set_base_allowance(1, Some(1_024));
		Some(1)
	}
}

/// Mock claim origin: a signed account proves the token equal to its low byte.
pub struct TestClaimOrigin;
impl EnsureOrigin<RuntimeOrigin> for TestClaimOrigin {
	type Success = u8;

	fn try_origin(origin: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		Result::<frame_system::RawOrigin<u64>, RuntimeOrigin>::from(origin).and_then(|origin| {
			match origin {
				frame_system::RawOrigin::Signed(who) => Ok(who as u8),
				origin => Err(origin.into()),
			}
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		Ok(frame_system::RawOrigin::Signed(1).into())
	}
}

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Footprint: pallet_footprint,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type RuntimeHoldReason = RuntimeHoldReason;
}

parameter_types! {
	pub const ItemByteWeight: u64 = 32;
	pub const PricePerByte: Balance = 5;
	pub const MaxPurchased: u64 = 1 << 20;
}

impl Config for Test {
	type RuntimeHoldReason = RuntimeHoldReason;
	type Currency = Balances;
	type Reason = TestReason;
	type ItemByteWeight = ItemByteWeight;
	type PricePerByte = PricePerByte;
	type MaxPurchased = MaxPurchased;
	type ClaimOrigin = TestClaimOrigin;
	type BaseAllowance = TestBaseAllowance;
	type WeightInfo = ();
}

/// Construct externalities with funded test accounts and no personhood grants.
#[derive(Default)]
pub struct ExtBuilder;

impl ExtBuilder {
	/// Build test externalities.
	pub fn build(self) -> sp_io::TestExternalities {
		BASE_ALLOWANCES.with(|allowances| allowances.borrow_mut().clear());
		let storage = match (RuntimeGenesisConfig {
			system: frame_system::GenesisConfig::default(),
			balances: pallet_balances::GenesisConfig {
				balances: (1..=20).map(|who| (who, 10_000_000)).collect(),
				..Default::default()
			},
		})
		.build_storage()
		{
			Ok(storage) => storage,
			Err(error) => panic!("mock runtime genesis configuration failed: {error:?}"),
		};
		let mut ext: sp_io::TestExternalities = storage.into();
		ext.execute_with(|| System::set_block_number(1));
		ext
	}

	/// Build externalities and execute one test closure within them.
	pub fn build_and_execute(self, test: impl FnOnce()) {
		self.build().execute_with(test);
	}
}

/// Return the exact fungible hold that backs an account's purchased allowance.
pub fn purchased_hold(who: u64) -> Balance {
	let reason: RuntimeHoldReason = HoldReason::PurchasedAllowance.into();
	Balances::balance_on_hold(&reason, &who)
}
