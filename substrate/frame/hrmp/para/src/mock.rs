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
//! and can be told to fail, which is all this side needs to be tested on its own.

use crate::{self as pallet_hrmp_para, HoldReason, SendToRelay};
use frame_support::{
	derive_impl, parameter_types,
	traits::{fungible::HoldConsideration, ConstU32, ConstantStoragePrice},
};
use hrmp_primitives::{MessageToRelay, ParaId, ParaManager};
use sp_runtime::{traits::Convert, BuildStorage};

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u32;

/// The Coretime chain itself, in the mock's numbering.
pub const SELF_PARA: ParaId = 1005;
/// A system chain that is not us, for testing the deposit-free rule from the other side.
pub const SYSTEM_PARA: ParaId = 1000;

pub const PARA_A: ParaId = 2000;
pub const PARA_B: ParaId = 2001;
pub const PARA_C: ParaId = 2002;

/// Managers, so the signed-account path has something to resolve.
pub const ALICE: AccountId = 1; // manages PARA_A
pub const BOB: AccountId = 2; // manages PARA_B
pub const CHARLIE: AccountId = 3; // manages nothing

pub const FIRST_PUBLIC_PARA_ID: ParaId = 2000;
pub const CHANNEL_DEPOSIT: Balance = 500;
pub const MAX_CAPACITY: u32 = 8;
pub const MAX_MESSAGE_SIZE: u32 = 1_024;
pub const PENDING_DEADLINE: BlockNumber = 50;

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
	type RuntimeHoldReason = RuntimeHoldReason;
}

parameter_types! {
	/// Messages handed to the transport, oldest first.
	pub static SentMessages: Vec<MessageToRelay> = Vec::new();
	/// When true, the transport refuses everything.
	pub static SendFails: bool = false;
	/// Accounts allowed to act as a para, as `(account, para_id)`.
	pub static ParaOriginAccounts: Vec<(AccountId, ParaId)> = Vec::new();
}

/// Records what would have gone to the relay chain.
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

/// Every message handed to the transport since the last call, oldest first.
pub fn take_sent() -> Vec<MessageToRelay> {
	SentMessages::take()
}

/// The origin para `para_id` itself calls with, backed by a fresh stand-in account.
///
/// An explicit list rather than an account range, so no other account can resolve as a para by
/// accident — in particular the manager accounts, which must go down the signed path.
pub fn para_origin(para_id: ParaId) -> RuntimeOrigin {
	let account = 1_000_000 + para_id as AccountId;
	ParaOriginAccounts::mutate(|paras| {
		if !paras.contains(&(account, para_id)) {
			paras.push((account, para_id));
		}
	});
	RuntimeOrigin::signed(account)
}

/// Lets the accounts in [`ParaOriginAccounts`] act as their para, standing in for a real XCM
/// origin.
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

/// Stands in for `pallet-registrar-para`'s view of who manages what.
pub struct MockManagers;

impl ParaManager for MockManagers {
	type AccountId = AccountId;

	fn manager_of(para_id: ParaId) -> Option<AccountId> {
		match para_id {
			PARA_A => Some(ALICE),
			PARA_B => Some(BOB),
			_ => None,
		}
	}
}

/// A para's sovereign account here. Kept far from the manager accounts so a test that means one
/// cannot accidentally assert the other.
pub struct SovereignOf;

impl Convert<ParaId, AccountId> for SovereignOf {
	fn convert(para_id: ParaId) -> AccountId {
		2_000_000 + para_id as AccountId
	}
}

parameter_types! {
	pub const ChannelDeposit: Balance = CHANNEL_DEPOSIT;
	pub const ChannelHoldReason: RuntimeHoldReason = RuntimeHoldReason::Hrmp(HoldReason::Channel);
}

impl pallet_hrmp_para::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ChannelConsideration = HoldConsideration<
		AccountId,
		Balances,
		ChannelHoldReason,
		ConstantStoragePrice<ChannelDeposit, Balance>,
	>;
	type SendToRelay = RecordingSender;
	type RelayOrigin = frame_system::EnsureRoot<AccountId>;
	type ParachainOrigin = ParaAccounts;
	type ParaManager = MockManagers;
	type SovereignAccountOf = SovereignOf;
	type SelfParaId = ConstU32<SELF_PARA>;
	type FirstPublicParaId = ConstU32<FIRST_PUBLIC_PARA_ID>;
	type MaxCapacity = ConstU32<MAX_CAPACITY>;
	type MaxMessageSize = ConstU32<MAX_MESSAGE_SIZE>;
	type PendingDeadline = ConstU32<PENDING_DEADLINE>;
	type BlockNumberProvider = System;
	type WeightInfo = ();
}

/// Externalities with every para's sovereign account funded, and the message log cleared.
pub fn new_test_ext() -> sp_io::TestExternalities {
	SentMessages::set(Vec::new());
	SendFails::set(false);
	ParaOriginAccounts::set(Vec::new());

	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: [PARA_A, PARA_B, PARA_C, SYSTEM_PARA, SELF_PARA]
			.into_iter()
			.map(|p| (SovereignOf::convert(p), 1_000_000))
			.chain([(ALICE, 1_000_000), (BOB, 1_000_000), (CHARLIE, 1_000_000)])
			.collect(),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Advance to block `n`, so deadline-dependent logic can be exercised.
pub fn run_to_block(n: BlockNumber) {
	while System::block_number() < n {
		System::set_block_number(System::block_number() + 1);
	}
}

/// How much is held on a para's sovereign account for channels.
pub fn held(para_id: ParaId) -> Balance {
	use frame_support::traits::fungible::InspectHold;
	Balances::balance_on_hold(&ChannelHoldReason::get(), &SovereignOf::convert(para_id))
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
