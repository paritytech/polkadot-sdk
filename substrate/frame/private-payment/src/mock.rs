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

//! Mock runtime for private payment pallet tests.

use crate as pallet_private_payment;
use frame_support::{
	derive_impl,
	parameter_types,
	traits::{AsEnsureOriginWithArg, ConstU16, ConstU8},
	PalletId,
};
use sp_runtime::{testing::UintAuthorityId, traits::AccountIdConversion, BuildStorage};

pub type TransactionExtension = frame_system::AuthorizeCall<Test>;
pub type Header = sp_runtime::generic::Header<u64, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic =
	sp_runtime::generic::UncheckedExtrinsic<u64, RuntimeCall, UintAuthorityId, TransactionExtension>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Assets: pallet_assets,
		PrivatePayment: pallet_private_payment,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u128>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type Balance = u128;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config for Test {
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<u64>>;
	type ForceOrigin = frame_system::EnsureRoot<u64>;
	type Currency = Balances;
	type Balance = u128;
}

parameter_types! {
	pub const PrivatePaymentPalletId: PalletId = PalletId(*b"privpay!");
	pub const BackingAssetId: u32 = 1;
	pub const BaseValue: u128 = 1_000_000; // $0.01 in 6-decimal asset units
}

impl pallet_private_payment::Config for Test {
	type MinimumAgeForRecycling = ConstU16<3>;
	type MaximumAge = ConstU16<10>;
	type MaxCoinExponent = ConstU8<14>;
	type Assets = Assets;
	type AssetId = u32;
	type Balance = u128;
	type Signature = UintAuthorityId;
	type BackingAssetId = BackingAssetId;
	type BaseValue = BaseValue;
	type PalletId = PrivatePaymentPalletId;
	type WeightInfo = ();
}

/// Build genesis storage for tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 1_000_000_000_000), (2, 1_000_000_000_000), (3, 1_000_000_000_000)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		// Create the backing stablecoin asset
		Assets::force_create(RuntimeOrigin::root(), 1, 1, true, 1).unwrap();
		// Mint some stablecoin to test accounts
		Assets::mint(RuntimeOrigin::signed(1), 1, 1, 1_000_000_000_000).unwrap();
		Assets::mint(RuntimeOrigin::signed(1), 1, 2, 1_000_000_000_000).unwrap();
		Assets::mint(RuntimeOrigin::signed(1), 1, 3, 1_000_000_000_000).unwrap();
		// Mint to pallet account for recycler operations
		let pallet_account: u64 = PrivatePaymentPalletId::get().into_account_truncating();
		Assets::mint(RuntimeOrigin::signed(1), 1, pallet_account, 1_000_000_000_000).unwrap();
	});
	ext
}
