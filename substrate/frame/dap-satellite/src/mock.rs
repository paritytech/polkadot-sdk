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

//! Test mock for the DAP Satellite pallet.

use crate::{self as pallet_dap_satellite, Config};
use frame_support::{
	derive_impl, parameter_types,
	sp_runtime::traits::AccountIdConversion,
	traits::{
		fungible::Mutate,
		tokens::{Fortitude, Precision, Preservation},
	},
	weights::constants::RocksDbWeight,
	PalletId,
};
use sp_runtime::BuildStorage;
use std::cell::RefCell;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		DapSatellite: pallet_dap_satellite,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
	/// Use non-zero DB weights so that weight exhaustion can be tested.
	type DbWeight = RocksDbWeight;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type ExistentialDeposit = ExistentialDeposit;
	type DustRemoval = DapSatellite;
}

thread_local! {
	/// Counts successful `MockSendToDap::send_native` calls.
	pub static SEND_COUNT: RefCell<u32> = RefCell::new(0);
	/// Set to `true` to make `MockSendToDap::send_native` return an error.
	pub static SEND_FAIL: RefCell<bool> = RefCell::new(false);
	/// Records the amount from the most recent successful `MockSendToDap::send_native` call.
	pub static LAST_SENT_AMOUNT: RefCell<Option<u64>> = RefCell::new(None);
}

/// Mock implementation of [`pallet_dap_satellite::SendToDap`].
pub struct MockSendToDap;

impl sp_dap::SendToDap<u64, u64> for MockSendToDap {
	fn send_native(source: u64, amount: u64) -> Result<(), ()> {
		if SEND_FAIL.with(|f| *f.borrow()) {
			return Err(());
		}
		// Simulate the real implementation: burn funds from the source account.
		Balances::burn_from(
			&source,
			amount,
			Preservation::Preserve,
			Precision::Exact,
			Fortitude::Polite,
		)
		.map_err(|_| ())?;
		SEND_COUNT.with(|c| *c.borrow_mut() += 1);
		LAST_SENT_AMOUNT.with(|a| *a.borrow_mut() = Some(amount));
		Ok(())
	}
}

parameter_types! {
	pub const DapSatellitePalletId: PalletId = sp_dap::DAP_SATELLITE_PALLET_ID;
	pub const ExistentialDeposit: u64 = 10;
	/// The transfer period in blocks.
	pub const TransferPeriod: u64 = 5;
	/// The smallest transferable amount (above ED).
	pub const MinTransferAmount: u64 = 10;
}

impl Config for Test {
	type Currency = Balances;
	type PalletId = DapSatellitePalletId;
	type SendToDap = MockSendToDap;
	type TransferPeriod = TransferPeriod;
	type MinTransferAmount = MinTransferAmount;
	type BlockNumberProvider = System;
	type WeightInfo = ();
}

pub fn new_test_ext(fund_satellite: bool) -> sp_io::TestExternalities {
	let mut balances = vec![(1, 100), (2, 200), (3, 300)];

	if fund_satellite {
		let satellite: u64 = DapSatellitePalletId::get().into_account_truncating();
		balances.push((satellite, ExistentialDeposit::get()));
	}

	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();
	t.into()
}
