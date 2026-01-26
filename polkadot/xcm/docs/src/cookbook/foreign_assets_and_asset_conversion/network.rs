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

//! Mock network

use frame::deps::{
	frame_system,
	sp_io::TestExternalities,
	sp_runtime::{AccountId32, BuildStorage},
};
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, TestExt};

use super::{asset_para, relay_chain, simple_para};

pub const ALICE: AccountId32 = AccountId32::new([0u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([1u8; 32]);
pub const UNITS: u128 = 10_000_000_000;
pub const FOREIGN_UNITS: u128 = 10_000_000_000;
pub const CENTS: u128 = 100_000_000;
pub const INITIAL_BALANCE: u128 = 100 * UNITS;

decl_test_parachain! {
	pub struct SimplePara {
		Runtime = simple_para::Runtime,
		XcmpMessageHandler = simple_para::MessageQueue,
		DmpMessageHandler = simple_para::MessageQueue,
		new_ext = simple_para_ext(),
	}
}

decl_test_parachain! {
	pub struct AssetPara {
		Runtime = asset_para::Runtime,
		XcmpMessageHandler = asset_para::MessageQueue,
		DmpMessageHandler = asset_para::MessageQueue,
		new_ext = asset_para_ext(),
	}
}

decl_test_relay_chain! {
	pub struct Relay {
		Runtime = relay_chain::Runtime,
		RuntimeCall = relay_chain::RuntimeCall,
		RuntimeEvent = relay_chain::RuntimeEvent,
		XcmConfig = relay_chain::XcmConfig,
		MessageQueue = relay_chain::MessageQueue,
		System = relay_chain::System,
		new_ext = relay_ext(),
	}
}

pub const SIMPLE_PARA_ID: u32 = 2222;
pub const ASSET_PARA_ID: u32 = 3333;

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(2222, SimplePara),
			(3333, AssetPara),
		],
	}
}

pub fn simple_para_ext() -> TestExternalities {
	use simple_para::{MessageQueue, Runtime, System};

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(ALICE, INITIAL_BALANCE)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	parachain_info::GenesisConfig::<Runtime> {
		parachain_id: SIMPLE_PARA_ID.into(),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		MessageQueue::set_para_id(SIMPLE_PARA_ID.into());
	});
	ext
}

pub fn asset_para_ext() -> TestExternalities {
	use asset_para::{MessageQueue, Runtime, System};

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(ALICE, INITIAL_BALANCE)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	parachain_info::GenesisConfig::<Runtime> {
		parachain_id: ASSET_PARA_ID.into(),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		MessageQueue::set_para_id(ASSET_PARA_ID.into());
	});
	ext
}

pub fn relay_ext() -> TestExternalities {
	use relay_chain::{Runtime, System};

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(ALICE, INITIAL_BALANCE)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}
