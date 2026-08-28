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

//! End-to-end tests for the parachain registrar, across a real relay chain and a real parachain.
//!
//! `pallet-registrar-para` and `pallet-registrar-relay` each have their own unit tests with the
//! other side stubbed out. What those cannot cover is everything in between: the hand-written
//! call-index enums, the `UnpaidExecution + Transact` programs, the origin conversion on both
//! ends, and the real `paras_registrar`/`paras` onboarding. That is what this crate is for.
//!
//! The two runtimes are wired through `xcm-simulator`, so messages really are encoded, routed and
//! executed, rather than short-circuited into direct function calls.

extern crate alloc;

// `construct_runtime!` refers to features these mocks do not declare.
#[allow(unexpected_cfgs)]
pub mod para;
#[allow(unexpected_cfgs)]
pub mod relay;
pub mod senders;

#[cfg(test)]
mod tests;

use sp_keyring::Sr25519Keyring;
use sp_runtime::{AccountId32, BuildStorage};
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, TestExt};

pub use senders::PARA_ID;

pub const ALICE: AccountId32 = AccountId32::new([1u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([2u8; 32]);
pub const INITIAL_BALANCE: u128 = 1_000_000_000;

/// Validators that approve PVFs on the relay chain.
pub const VALIDATORS: &[Sr25519Keyring] = &[
	Sr25519Keyring::Alice,
	Sr25519Keyring::Bob,
	Sr25519Keyring::Charlie,
	Sr25519Keyring::Dave,
	Sr25519Keyring::Ferdie,
];

/// Size bounds shared by both chains.
///
/// In production the parachain's copies are a mirror of the relay chain's live `configuration`,
/// kept in step by governance. Here they are simply the same constants, and the relay chain's
/// `configuration` genesis is built from them.
pub const MIN_CODE_SIZE: u32 = 9;
pub const MAX_CODE_SIZE: u32 = 3 * 1024;

/// HRMP channel bounds, mirrored on both chains so a request that passes locally passes remotely.
pub const MAX_CAPACITY: u32 = 8;
pub const MAX_MESSAGE_SIZE: u32 = 1024;
/// What one end of a channel costs on the parachain.
pub const CHANNEL_DEPOSIT: u128 = 500;
/// How long the parachain waits before a channel counts as stuck.
pub const HRMP_DEADLINE: u64 = 50;
/// What it costs on the parachain to buy out an upgrade cooldown.
pub const COOLDOWN_COST: u128 = 250;
pub const MAX_HEAD_SIZE: u32 = 1024;

decl_test_parachain! {
	pub struct RegistrarPara {
		Runtime = para::Runtime,
		XcmpMessageHandler = para::MsgQueue,
		DmpMessageHandler = para::MsgQueue,
		new_ext = para_ext(),
	}
}

decl_test_relay_chain! {
	pub struct Relay {
		Runtime = relay::Runtime,
		RuntimeCall = relay::RuntimeCall,
		RuntimeEvent = relay::RuntimeEvent,
		XcmConfig = relay::XcmConfig,
		MessageQueue = relay::MessageQueue,
		System = relay::System,
		new_ext = relay_ext(),
	}
}

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(PARA_ID, RegistrarPara),
		],
	}
}

pub fn para_ext() -> sp_io::TestExternalities {
	use para::{MsgQueue, Runtime, System};
	use sp_runtime::traits::Convert;

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
	// HRMP deposits are held on a para's sovereign account here, not on whoever calls, so those
	// accounts have to exist and be funded for a channel to be openable at all.
	let sovereigns = (PARA_ID..PARA_ID + 1)
		.chain(2000..2006)
		.map(|id| (para::SovereignOf::convert(id), INITIAL_BALANCE));

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(ALICE, INITIAL_BALANCE), (BOB, INITIAL_BALANCE)]
			.into_iter()
			.chain(sovereigns)
			.collect(),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		MsgQueue::set_para_id(PARA_ID.into());
	});
	ext
}

pub fn relay_ext() -> sp_io::TestExternalities {
	use polkadot_runtime_parachains::{configuration, paras};
	use relay::{Runtime, System};

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	configuration::GenesisConfig::<Runtime> {
		config: configuration::HostConfiguration {
			max_code_size: MAX_CODE_SIZE,
			max_head_data_size: MAX_HEAD_SIZE,
			// The HRMP defaults are all zero, which would make every channel request invalid
			// before this pallet's own bounds were ever exercised.
			hrmp_channel_max_capacity: MAX_CAPACITY,
			hrmp_channel_max_message_size: MAX_MESSAGE_SIZE,
			hrmp_channel_max_total_size: MAX_CAPACITY * MAX_MESSAGE_SIZE,
			hrmp_max_parachain_outbound_channels: 8,
			hrmp_max_parachain_inbound_channels: 8,
			// Deposits are held on the parachain now, so the relay chain's own figures are only
			// here to prove nothing is ever reserved against them.
			hrmp_sender_deposit: 1_000,
			hrmp_recipient_deposit: 1_000,
			// `hrmp` sends its own open/accept/close notifications down to the paras, and the
			// default of zero makes every one of them undeliverable.
			max_downward_message_size: 1024,
			..Default::default()
		},
	}
	.assimilate_storage(&mut t)
	.unwrap();

	// The control-plane parachain is a real, registered para here. It has to be: a channel is
	// only openable between two paras the relay chain knows, and every para it registers gets one
	// with it.
	paras::GenesisConfig::<Runtime> {
		paras: vec![(
			PARA_ID.into(),
			paras::ParaGenesisArgs {
				genesis_head: vec![1].into(),
				validation_code: vec![1].into(),
				para_kind: paras::ParaKind::Parachain,
			},
		)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(ALICE, INITIAL_BALANCE), (BOB, INITIAL_BALANCE)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}
