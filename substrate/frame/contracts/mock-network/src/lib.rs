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

pub mod mocks;
pub mod parachain;
pub mod primitives;
pub mod relay_chain;

#[cfg(test)]
mod tests;

use crate::primitives::{AccountId, UNITS};
use sp_runtime::BuildStorage;
use xcm::latest::prelude::*;
use xcm_executor::traits::ConvertLocation;
pub use xcm_simulator::TestExt;
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain};

// Accounts
pub const ADMIN: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([0u8; 32]);
pub const ALICE: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([1u8; 32]);
pub const BOB: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([2u8; 32]);

// Balances
pub const INITIAL_BALANCE: u128 = 1_000_000_000 * UNITS;

decl_test_parachain! {
	pub struct ParaA {
		Runtime = parachain::Runtime,
		XcmpMessageHandler = parachain::MsgQueue,
		DmpMessageHandler = parachain::MsgQueue,
		new_ext = para_ext(1),
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
		],
	}
}

pub fn relay_sovereign_account_id() -> AccountId {
	let location: Location = (Parent,).into();
	parachain::SovereignAccountOf::convert_location(&location).unwrap()
}

pub fn parachain_sovereign_account_id(para: u32) -> AccountId {
	let location: Location = (Parachain(para),).into();
	relay_chain::SovereignAccountOf::convert_location(&location).unwrap()
}

pub fn parachain_account_sovereign_account_id(
	para: u32,
	who: sp_runtime::AccountId32,
) -> AccountId {
	let location: Location = (
		Parachain(para),
		AccountId32 { network: Some(relay_chain::RelayNetwork::get()), id: who.into() },
	)
		.into();
	relay_chain::SovereignAccountOf::convert_location(&location).unwrap()
}

pub fn para_ext(para_id: u32) -> sp_io::TestExternalities {
	use parachain::{MsgQueue, Runtime, System};

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(ALICE, INITIAL_BALANCE),
			(relay_sovereign_account_id(), INITIAL_BALANCE),
			(BOB, INITIAL_BALANCE),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_assets::GenesisConfig::<Runtime> {
		assets: vec![
			(0u128, ADMIN, false, 1u128), // Create derivative asset for relay's native token
		],
		metadata: Default::default(),
		accounts: vec![
			(0u128, ALICE, INITIAL_BALANCE),
			(0u128, relay_sovereign_account_id(), INITIAL_BALANCE),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		sp_tracing::try_init_simple();
		System::set_block_number(1);
		MsgQueue::set_para_id(para_id.into());
	});
	ext
}

pub fn relay_ext() -> sp_io::TestExternalities {
	use relay_chain::{Runtime, System};

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(ALICE, INITIAL_BALANCE),
			(parachain_sovereign_account_id(1), INITIAL_BALANCE),
			(parachain_account_sovereign_account_id(1, ALICE), INITIAL_BALANCE),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}

pub type ParachainPalletXcm = pallet_xcm::Pallet<parachain::Runtime>;
pub type ParachainBalances = pallet_balances::Pallet<parachain::Runtime>;
