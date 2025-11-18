// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
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

use frame_support::{
	derive_impl, ord_parameter_types, parameter_types,
	sp_runtime::traits::Hash,
	traits::{fungible::HoldConsideration, Contains, Footprint},
};
use frame_system::EnsureSignedBy;
use pallet_revive::H256;
use sp_runtime::{
	traits::{Convert, IdentityLookup},
	AccountId32, BuildStorage,
};

pub type AccountId = AccountId32;
pub type Balance = u128;
type Block = frame_system::mocking::MockBlock<Test>;

pub const ALICE: AccountId = AccountId::new([0u8; 32]);
pub const BOB: AccountId = AccountId::new([1u8; 32]);
pub const CHARLIE: AccountId = AccountId::new([2u8; 32]);

use crate::PreimagePrecompile;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Preimage: pallet_preimage,
		Revive: pallet_revive,
	}
);

// Test that a filtered call can be dispatched.
pub struct BaseFilter;
impl Contains<RuntimeCall> for BaseFilter {
	fn contains(call: &RuntimeCall) -> bool {
		!matches!(call, &RuntimeCall::Balances(pallet_balances::Call::force_set_balance { .. }))
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type BaseCallFilter = BaseFilter;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

parameter_types! {
	pub ExistentialDeposit: Balance = 1;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

#[derive_impl(pallet_revive::config_preludes::TestDefaultConfig)]
impl pallet_revive::Config for Test {
	type AddressMapper = pallet_revive::AccountId32Mapper<Self>;
	type Balance = Balance;
	type Currency = Balances;
	type Precompiles = (PreimagePrecompile<Self>,);
	type UploadOrigin = frame_system::EnsureSigned<AccountId>;
	type InstantiateOrigin = frame_system::EnsureSigned<AccountId>;
}

ord_parameter_types! {
	pub const Manager: AccountId = BOB;
}
parameter_types! {
	pub const PreimageHoldReason: RuntimeHoldReason = RuntimeHoldReason::Preimage(pallet_preimage::HoldReason::Preimage);
}

pub struct ConvertDeposit;
impl Convert<Footprint, u128> for ConvertDeposit {
	fn convert(a: Footprint) -> u128 {
		a.count.saturating_mul(2).saturating_add(a.size).into()
	}
}

impl pallet_preimage::Config for Test {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type ManagerOrigin = EnsureSignedBy<Manager, AccountId>;
	type Consideration = HoldConsideration<AccountId, Balances, PreimageHoldReason, ConvertDeposit>;
}

pub fn hashed(data: impl AsRef<[u8]>) -> H256 {
	<Test as frame_system::Config>::Hashing::hash(data.as_ref())
}

/// Declares a new test externality, funds ALICE and BOB accounts.
pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	let initial_balance = Balance::MAX.saturating_div(100000);
	let balances = vec![(ALICE, initial_balance), (BOB, initial_balance)];

	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();

	pallet_revive::GenesisConfig::<Test> {
		mapped_accounts: vec![ALICE, BOB],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
