// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! # The Asset Para Runtime
//!
//! This is the main stage of the example, where we showcase how to configure and use a runtime
//! that has the capabilities of:
//! 1. Having foreign assets registered.
//! 2. Set up a liquidity pool of the native and foreign assets
//! 3. Configure the XCM stuff to use the foreign tokens and pay XCM fees with foreign tokens.

use frame::{deps::frame_system, runtime::prelude::*, traits::IdentityLookup};
use xcm_executor::XcmExecutor;
use xcm_simulator::mock_message_queue;
pub mod xcm_config;

pub(crate) mod assets;

use xcm_config::XcmConfig;

pub type Block = frame_system::mocking::MockBlock<Runtime>;
pub type AccountId = frame::deps::sp_runtime::AccountId32;
pub type Balance = u128;

construct_runtime! {
	pub struct Runtime {
		System: frame_system,
		ParachainInfo: parachain_info,
		MessageQueue: mock_message_queue,
		ForeignAssets: pallet_assets::<Instance1>,
		PoolAssets: pallet_assets::<Instance2>,
		AssetConversion: pallet_asset_conversion,
		Balances: pallet_balances,
		XcmPallet: pallet_xcm,
		CumulusXcm: cumulus_pallet_xcm,
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
}

impl mock_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

pub const UNITS: Balance = 10_000_000_000;

parameter_types! {
	pub const ExistentialDeposit: Balance = UNITS;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type AccountStore = System;
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
}

impl parachain_info::Config for Runtime {}
