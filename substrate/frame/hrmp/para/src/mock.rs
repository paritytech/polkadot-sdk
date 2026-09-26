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

//! Mock runtime for `pallet-hrmp-para`.

use crate::{self as pallet_hrmp_para, HoldReason, SendToRelay};
use frame_support::{derive_impl, parameter_types, traits::fungible::InspectHold};
use hrmp_primitives::{ChannelId, DepositKey, DepositSide, MessageToRelay, ParaId};
use sp_runtime::{traits::Convert, BuildStorage};

pub type AccountId = u64;
pub type Balance = u128;

pub const ALICE: AccountId = 1;
/// The signed account that stands in for the relay chain.
pub const RELAY: AccountId = 9;

pub const PARA_A: ParaId = 2000; // sender
pub const PARA_B: ParaId = 2001; // recipient
pub const PARA_POOR: ParaId = 2002; // sovereign account holds 30

pub const CHANNEL: ChannelId = ChannelId { sender: PARA_A, recipient: PARA_B };

pub fn sender_key() -> DepositKey {
	DepositKey { channel: CHANNEL, side: DepositSide::Sender }
}

pub fn recipient_key() -> DepositKey {
	DepositKey { channel: CHANNEL, side: DepositSide::Recipient }
}

#[frame_support::runtime]
mod test_runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type Balances = pallet_balances::Pallet<Test>;

	#[runtime::pallet_index(2)]
	pub type HrmpPara = pallet_hrmp_para::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlockU32<Test>;
	type AccountId = AccountId;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type AccountStore = System;
	type RuntimeHoldReason = RuntimeHoldReason;
}

parameter_types! {
	/// Messages handed to the transport, oldest first.
	pub static Sent: Vec<MessageToRelay> = Vec::new();
	/// When true, the transport refuses everything.
	pub static SendFails: bool = false;
	pub static RelayMembers: Vec<AccountId> = vec![RELAY];
}

pub struct RecordingSender;

impl SendToRelay for RecordingSender {
	fn send(message: MessageToRelay) -> Result<(), ()> {
		if SendFails::get() {
			return Err(());
		}
		Sent::mutate(|sent| sent.push(message));
		Ok(())
	}
}

/// A para's sovereign account is `100_000 + para`.
pub struct SovereignAccountOf;

impl Convert<ParaId, AccountId> for SovereignAccountOf {
	fn convert(para: ParaId) -> AccountId {
		100_000 + para as AccountId
	}
}

pub struct RelayAccount;

impl frame_support::traits::SortedMembers<AccountId> for RelayAccount {
	fn sorted_members() -> Vec<AccountId> {
		RelayMembers::get()
	}
}

impl pallet_hrmp_para::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;
	type Currency = Balances;
	type RelayOrigin = frame_system::EnsureSignedBy<RelayAccount, AccountId>;
	type SendToRelay = RecordingSender;
	type SovereignAccountOf = SovereignAccountOf;
	type WeightInfo = ();
}

pub fn sovereign(para: ParaId) -> AccountId {
	SovereignAccountOf::convert(para)
}

/// What `para` has on hold for channel deposits.
pub fn on_hold(para: ParaId) -> Balance {
	Balances::balance_on_hold(
		&RuntimeHoldReason::HrmpPara(HoldReason::ChannelDeposit),
		&sovereign(para),
	)
}

/// `PARA_A` and `PARA_B` hold 1_000 each, `PARA_POOR` holds 30.
pub fn new_test_ext() -> sp_io::TestExternalities {
	Sent::set(vec![]);
	SendFails::set(false);

	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(ALICE, 1_000),
			(sovereign(PARA_A), 1_000),
			(sovereign(PARA_B), 1_000),
			(sovereign(PARA_POOR), 30),
		],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Every event this pallet emitted, oldest first, clearing the log.
pub fn events() -> Vec<pallet_hrmp_para::Event<Test>> {
	let events = System::events()
		.into_iter()
		.filter_map(|e| match e.event {
			RuntimeEvent::HrmpPara(inner) => Some(inner),
			_ => None,
		})
		.collect();
	System::reset_events();
	events
}
