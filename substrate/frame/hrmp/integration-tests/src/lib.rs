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

//! End-to-end tests for HRMP channel management, across a real relay chain and a real parachain.
//!
//! `pallet-hrmp-para` and `pallet-hrmp-relay` each have their own unit tests with the other side
//! stubbed out. What those cannot cover is everything in between: the hand-written call-index
//! enums, the `UnpaidExecution + Transact` programs, the origin conversion on both ends, and the
//! real `parachains_hrmp` routing table. That is what this crate is for.
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

/// The para that asks for a channel in the flow tests.
pub const SENDER: u32 = 2000;
/// The para the channel is asked of.
pub const RECIPIENT: u32 = 2001;
/// A system chain, which an id at or below 1999 makes it. Channels with it are deposit-free.
pub const SYSTEM_PARA: u32 = 1001;

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

/// Channel bounds shared by both chains.
///
/// In production the parachain's copies are a mirror of the relay chain's live `configuration`,
/// kept in step by governance. Here they are the same constants, and the relay chain's
/// `configuration` genesis is built from them.
pub const MAX_CAPACITY: u32 = 8;
pub const MAX_MESSAGE_SIZE: u32 = 1_024;
pub const MAX_TOTAL_SIZE: u32 = MAX_CAPACITY * MAX_MESSAGE_SIZE;
pub const MAX_INBOUND_CHANNELS: u32 = 4;
pub const MAX_OUTBOUND_CHANNELS: u32 = 4;

decl_test_parachain! {
	pub struct HrmpPara {
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
			(PARA_ID, HrmpPara),
		],
	}
}

pub fn para_ext() -> sp_io::TestExternalities {
	use para::{MsgQueue, Runtime, SovereignAccountOf, System};
	use sp_runtime::traits::Convert;

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
	// The two ends of the flow tests put up their deposits out of these.
	//
	// [`SYSTEM_PARA`] is deliberately left out: a channel with the system takes no deposit, so it
	// must work without a sovereign account here at all.
	let sovereign = |id: u32| (SovereignAccountOf::convert(id), INITIAL_BALANCE);
	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(ALICE, INITIAL_BALANCE),
			(BOB, INITIAL_BALANCE),
			sovereign(SENDER),
			sovereign(RECIPIENT),
		],
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
	use polkadot_primitives::{HeadData, ValidationCode};
	use polkadot_runtime_parachains::{
		configuration,
		paras::{self, ParaGenesisArgs, ParaKind},
	};
	use relay::{Runtime, System};

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	// `hrmp` refuses a channel unless both ends are valid paras, so the two ends of the flow
	// tests are onboarded here rather than through the registrar.
	let para = |id: u32| {
		(
			id.into(),
			ParaGenesisArgs {
				genesis_head: HeadData(vec![id as u8]),
				validation_code: ValidationCode(vec![id as u8]),
				para_kind: ParaKind::Parachain,
			},
		)
	};
	paras::GenesisConfig::<Runtime> {
		paras: vec![para(PARA_ID), para(SENDER), para(RECIPIENT), para(SYSTEM_PARA)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	configuration::GenesisConfig::<Runtime> {
		config: configuration::HostConfiguration {
			// Channel notifications to the two ends travel down this queue.
			max_downward_message_size: 1_024,
			hrmp_channel_max_capacity: MAX_CAPACITY,
			hrmp_channel_max_message_size: MAX_MESSAGE_SIZE,
			hrmp_channel_max_total_size: MAX_TOTAL_SIZE,
			hrmp_max_parachain_inbound_channels: MAX_INBOUND_CHANNELS,
			hrmp_max_parachain_outbound_channels: MAX_OUTBOUND_CHANNELS,
			..Default::default()
		},
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
