// Copyright Parity Technologies (UK) Ltd.
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

//! # Runtime

use frame::{deps::frame_system, prelude::*, runtime::prelude::*, traits::IdentityLookup};
use xcm_executor::XcmExecutor;
use xcm_simulator::mock_message_queue;

mod xcm_config;
use xcm_config::XcmConfig;

pub type Block = frame_system::mocking::MockBlock<Runtime>;
pub type AccountId = frame::deps::sp_runtime::AccountId32;
pub type Balance = u64;

construct_runtime! {
	pub struct Runtime {
		System: frame_system,
		MessageQueue: mock_message_queue,
		Balances: pallet_balances,
		XcmPallet: pallet_xcm,
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
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

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type AccountStore = System;
}
