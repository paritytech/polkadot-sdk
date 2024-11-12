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

//! Mock network

use frame::{deps::sp_runtime::AccountId32, testing_prelude::*};
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, TestExt};

use super::{parachain, relay_chain};

pub const ALICE: AccountId32 = AccountId32::new([0u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([1u8; 32]);
pub const UNITS: u128 = 10_000_000_000;
pub const CENTS: u128 = 100_000_000;
pub const INITIAL_BALANCE: u128 = UNITS;

decl_test_parachain! {
	pub struct ParaA {
		Runtime = parachain::Runtime,
		XcmpMessageHandler = parachain::MessageQueue,
		DmpMessageHandler = parachain::MessageQueue,
		new_ext = para_ext(1),
	}
}

decl_test_parachain! {
	pub struct ParaB {
		Runtime = parachain::Runtime,
		XcmpMessageHandler = parachain::MessageQueue,
		DmpMessageHandler = parachain::MessageQueue,
		new_ext = para_ext(2),
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

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(1, ParaA),
			(2, ParaB),
		],
	}
}

pub fn para_ext(para_id: u32) -> TestState {
	use parachain::{MessageQueue, Runtime, System};
	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	let account_with_starting_balance = match para_id {
		1 => ALICE,
		2 => BOB,
		_ => panic!("Not a valid para id"),
	};

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(account_with_starting_balance, INITIAL_BALANCE),
			(sibling_account_id(1), INITIAL_BALANCE),
			(sibling_account_id(2), INITIAL_BALANCE),
			(parent_account_id(), INITIAL_BALANCE),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = TestState::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		MessageQueue::set_para_id(para_id.into());
		force_create_foreign_asset(para_id);
	});
	ext
}

/// Will create a foreign asset on one parachain representing the asset
/// of another.
/// If para_id is 1, then it will create the asset in 1, referencing the asset in 2.
/// If para_id is 2, then it will create the asset in 2, referencing the asset in 1.
fn force_create_foreign_asset(para_id: u32) {
	use frame::testing_prelude::*;
	use parachain::{ForeignAssets, RuntimeOrigin};
	let other_para_id = if para_id == 1 { 2 } else { 1 };
	// We mark the asset as sufficient so tests are easier.
	// Being sufficient means an account with only this asset can exist.
	// In general, we should be careful with what is sufficient, as it can become an attack vector.
	assert_ok!(ForeignAssets::force_create(
		RuntimeOrigin::root(),
		(Parent, Parachain(other_para_id)).into(),
		ALICE, // Owner. You probably don't want this to be just an account.
		true,  // Sufficient.
		1,     // Minimum balance, this is the ED.
	));
}

pub fn relay_ext() -> TestState {
	use relay_chain::{Runtime, System};

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Runtime> { balances: vec![(ALICE, INITIAL_BALANCE)] }
		.assimilate_storage(&mut t)
		.unwrap();

	let mut ext = TestState::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}

pub fn parent_account_id() -> parachain::AccountId {
	let location = (Parent,);
	parachain::LocationToAccountId::convert_location(&location.into()).unwrap()
}

pub fn sibling_account_id(para: u32) -> parachain::AccountId {
	let location = (Parent, Parachain(para));
	parachain::LocationToAccountId::convert_location(&location.into()).unwrap()
}
