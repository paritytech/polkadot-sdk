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
use xcm_emulator::{decl_test_parachains, decl_test_relay_chains, decl_test_networks};
use super::{asset_para, relay_chain, simple_para};

pub const ALICE: AccountId32 = AccountId32::new([0u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([1u8; 32]);
pub const UNITS: u64 = 10_000_000_000;
pub const CENTS: u64 = 100_000_000;
pub const INITIAL_BALANCE: u64 = UNITS;

decl_test_parachains! {
	pub struct SimplePara {
		runtime = parachain::Runtime,
		XcmpMessageHandler = simple_para::MessageQueue,
		DmpMessageHandler = simple_para::MessageQueue,
		new_ext = simple_para_ext(),
	}
}

decl_test_parachains! {
	pub struct AssetPara {
		Runtime = parachain::Runtime,
		XcmpMessageHandler = asset_para::MessageQueue,
		DmpMessageHandler = asset_para::MessageQueue,
		new_ext = asset_para_ext(),
	}
}

decl_test_relay_chains! {
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

decl_test_networks! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(SIMPLE_PARA_ID, SimplePara),
			(ASSET_PARA_ID, AssetPara),
		],
	}
}

pub fn simple_para_ext() -> TestExternalities {
	use simple_para::{MessageQueue, Runtime, System};

	let t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
	let mut ext = TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		MessageQueue::set_para_id(2222.into());
	});
	ext
}

pub fn asset_para_ext() -> TestExternalities {
	use asset_para::{MessageQueue, Runtime, System};

	let t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
	let mut ext = TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		MessageQueue::set_para_id(2222.into());
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
