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
//!
//! The relay chain is not modelled here: [`RecordingSender`] captures what would have been sent
//! and can be told to fail. The two halves meeting for real is the job of the `pallet-hrmp-test`
//! crate.

// Helpers the per-flow tests will reach for as the extrinsic bodies land.
#![allow(dead_code)]

use crate::{self as pallet_hrmp_para, HoldReason, SendToRelay};
use frame_support::{
	derive_impl, parameter_types,
	traits::{fungible::HoldConsideration, ConstU128, ConstU32, LinearStoragePrice},
};
use hrmp_primitives::{MessageToRelay, ParaId};
use sp_runtime::BuildStorage;

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u32;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;

pub const PER_MESSAGE: Balance = 10;
pub const MAX_CAPACITY: u32 = 1_000;
pub const MAX_MESSAGE_SIZE: u32 = 1_024;
pub const MAX_INBOUND_CHANNELS: u32 = 8;
pub const MAX_OUTBOUND_CHANNELS: u32 = 8;

/// Sizes used for channels involving a system chain, as `(max_message_size, max_capacity)`.
pub const SYSTEM_CHANNEL_SIZE_AND_CAPACITY: (u32, u32) = (MAX_MESSAGE_SIZE, MAX_CAPACITY);

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
		RuntimeTask
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type Balances = pallet_balances::Pallet<Test>;

	#[runtime::pallet_index(2)]
	pub type Hrmp = pallet_hrmp_para::Pallet<Test>;
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
	type ExistentialDeposit = ConstU128<1>;
	type RuntimeHoldReason = RuntimeHoldReason;
}

parameter_types! {
	/// Messages the pallet handed to the transport, oldest first.
	pub static SentMessages: Vec<MessageToRelay> = Vec::new();
	/// When true, the transport refuses everything.
	pub static SendFails: bool = false;
}

/// A [`SendToRelay`] that records instead of sending, and can be made to fail.
pub struct RecordingSender;

impl SendToRelay for RecordingSender {
	fn send(message: MessageToRelay) -> Result<(), ()> {
		if SendFails::get() {
			return Err(());
		}
		SentMessages::mutate(|sent| sent.push(message));
		Ok(())
	}
}

/// Take everything recorded so far, clearing the log.
pub fn take_sent() -> Vec<MessageToRelay> {
	SentMessages::mutate(core::mem::take)
}

parameter_types! {
	/// Signed accounts allowed to act as a para, as `(account, para id)`.
	pub static ParaOriginAccounts: Vec<(AccountId, ParaId)> = Vec::new();
	/// Paras the pallet treats as system chains.
	pub static SystemParas: Vec<ParaId> = Vec::new();
}

/// The paras listed in [`SystemParas`].
pub struct IsSystemPara;

impl frame_support::traits::Contains<ParaId> for IsSystemPara {
	fn contains(para_id: &ParaId) -> bool {
		SystemParas::get().contains(para_id)
	}
}

/// Lets the accounts listed in [`ParaOriginAccounts`] act as their para, standing in for a real
/// XCM origin. An explicit list, not an account range, so no other account can resolve as a para
/// by accident.
pub struct ParaAccounts;

impl frame_support::traits::EnsureOrigin<RuntimeOrigin> for ParaAccounts {
	type Success = ParaId;

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		let signed: Result<frame_system::RawOrigin<AccountId>, _> = o.clone().into();
		match signed {
			Ok(frame_system::RawOrigin::Signed(who)) => ParaOriginAccounts::get()
				.iter()
				.find(|(account, _)| *account == who)
				.map(|(_, para_id)| *para_id)
				.ok_or(o),
			_ => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		Err(())
	}
}

/// The origin para `para_id` itself calls with, backed by a fresh stand-in account.
pub fn para_origin(para_id: ParaId) -> RuntimeOrigin {
	let account = para_account(para_id);
	ParaOriginAccounts::mutate(|paras| {
		if !paras.contains(&(account, para_id)) {
			paras.push((account, para_id));
		}
	});
	RuntimeOrigin::signed(account)
}

/// The sovereign account a para's deposits come out of.
pub fn para_account(para_id: ParaId) -> AccountId {
	1_000_000 + para_id as AccountId
}

/// Resolves a para to the account its deposits are taken from.
pub struct SovereignAccountOf;

impl sp_runtime::traits::Convert<ParaId, AccountId> for SovereignAccountOf {
	fn convert(para_id: ParaId) -> AccountId {
		para_account(para_id)
	}
}

parameter_types! {
	pub const DepositPerMessage: Balance = PER_MESSAGE;
	pub const SenderHoldReason: RuntimeHoldReason =
		RuntimeHoldReason::Hrmp(HoldReason::SenderDeposit);
	pub const RecipientHoldReason: RuntimeHoldReason =
		RuntimeHoldReason::Hrmp(HoldReason::RecipientDeposit);
	pub const SystemChannelSizes: (u32, u32) = SYSTEM_CHANNEL_SIZE_AND_CAPACITY;
}

impl pallet_hrmp_para::Config for Test {
	type SenderConsideration = HoldConsideration<
		AccountId,
		Balances,
		SenderHoldReason,
		LinearStoragePrice<ConstU128<0>, DepositPerMessage, Balance>,
	>;
	type RecipientConsideration = HoldConsideration<
		AccountId,
		Balances,
		RecipientHoldReason,
		LinearStoragePrice<ConstU128<0>, DepositPerMessage, Balance>,
	>;
	type SendToRelay = RecordingSender;
	type RelayOrigin = frame_system::EnsureRoot<AccountId>;
	type ParachainOrigin = ParaAccounts;
	type ChannelManager = frame_system::EnsureRoot<AccountId>;
	type SovereignAccountOf = SovereignAccountOf;
	type MaxCapacity = ConstU32<MAX_CAPACITY>;
	type MaxMessageSize = ConstU32<MAX_MESSAGE_SIZE>;
	type MaxInboundChannels = ConstU32<MAX_INBOUND_CHANNELS>;
	type MaxOutboundChannels = ConstU32<MAX_OUTBOUND_CHANNELS>;
	type DefaultChannelSizeAndCapacityWithSystem = SystemChannelSizes;
	type IsSystemPara = IsSystemPara;
	type WeightInfo = ();
}

/// Externalities with Alice and Bob funded, and the message log cleared.
pub fn new_test_ext() -> sp_io::TestExternalities {
	SentMessages::set(Vec::new());
	SendFails::set(false);
	ParaOriginAccounts::set(Vec::new());

	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(ALICE, 1_000_000), (BOB, 1_000_000)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Advance to block `n`.
pub fn run_to_block(n: BlockNumber) {
	while System::block_number() < n {
		System::set_block_number(System::block_number() + 1);
	}
}

/// Every event this pallet emitted, oldest first, clearing the log.
pub fn hrmp_events() -> Vec<pallet_hrmp_para::Event<Test>> {
	let events = System::events()
		.into_iter()
		.filter_map(|e| match e.event {
			RuntimeEvent::Hrmp(inner) => Some(inner),
			_ => None,
		})
		.collect();
	System::reset_events();
	events
}
