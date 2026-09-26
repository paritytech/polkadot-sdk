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

//! Mock runtime for `pallet-hrmp-relay`.

use crate::{self as pallet_hrmp_relay, AdmitRequest, SendToPara};
use frame_support::{derive_impl, traits::EnsureOrigin};
use hrmp_primitives::{
	ChannelId, DepositKey, DepositSide, MessageToPara, OnDepositHeld, ParaId, RelayHrmp,
};
use sp_runtime::{BuildStorage, DispatchError, DispatchResult};

pub type AccountId = u64;

pub const ALICE: AccountId = 1;

pub const PARA_A: ParaId = 2000;
pub const PARA_B: ParaId = 2001;

/// Signed accounts that stand in for a para acting as itself.
pub const PARA_A_ACCOUNT: AccountId = 1_000_000 + PARA_A as AccountId;
pub const PARA_B_ACCOUNT: AccountId = 1_000_000 + PARA_B as AccountId;

/// The channel from `PARA_A` to `PARA_B`.
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
	pub type HrmpRelay = pallet_hrmp_relay::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlockU32<Test>;
	type AccountId = AccountId;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
}

/// A call into the relay chain's HRMP, as the mock records it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum HrmpCall {
	Init { para: ParaId, recipient: ParaId, capacity: u32, size: u32 },
	Accept { para: ParaId, sender: ParaId },
	Close { para: ParaId, channel: ChannelId },
	Cancel { para: ParaId, channel: ChannelId, open_requests: u32 },
	WithSystem { para: ParaId, target: ParaId },
	Poke { channel: ChannelId },
	SystemChannel { channel: ChannelId },
}

frame_support::parameter_types! {
	/// Calls dispatched into HRMP, oldest first.
	pub static HrmpCalls: Vec<HrmpCall> = Vec::new();
	/// When set, the next HRMP call fails with this error.
	pub static HrmpFails: Option<DispatchError> = None;
	/// Messages handed to the transport, oldest first.
	pub static Sent: Vec<MessageToPara> = Vec::new();
	/// When true, the transport refuses everything.
	pub static SendFails: bool = false;
	/// Hold answers passed on, oldest first.
	pub static Answers: Vec<(DepositKey, bool)> = Vec::new();
	/// Paras whose requests are refused.
	pub static Rationed: Vec<ParaId> = Vec::new();
}

pub struct MockHrmp;

impl MockHrmp {
	fn record(call: HrmpCall) -> DispatchResult {
		if let Some(error) = HrmpFails::take() {
			return Err(error);
		}
		HrmpCalls::mutate(|calls| calls.push(call));
		Ok(())
	}
}

impl RelayHrmp for MockHrmp {
	fn init_open_channel(
		para: ParaId,
		recipient: ParaId,
		capacity: u32,
		size: u32,
	) -> DispatchResult {
		Self::record(HrmpCall::Init { para, recipient, capacity, size })
	}
	fn accept_open_channel(para: ParaId, sender: ParaId) -> DispatchResult {
		Self::record(HrmpCall::Accept { para, sender })
	}
	fn close_channel(para: ParaId, channel: ChannelId) -> DispatchResult {
		Self::record(HrmpCall::Close { para, channel })
	}
	fn cancel_open_request(para: ParaId, channel: ChannelId, open_requests: u32) -> DispatchResult {
		Self::record(HrmpCall::Cancel { para, channel, open_requests })
	}
	fn establish_channel_with_system(para: ParaId, target: ParaId) -> DispatchResult {
		Self::record(HrmpCall::WithSystem { para, target })
	}
	fn poke_channel_deposits(channel: ChannelId) -> DispatchResult {
		Self::record(HrmpCall::Poke { channel })
	}
	fn establish_system_channel(channel: ChannelId) -> DispatchResult {
		Self::record(HrmpCall::SystemChannel { channel })
	}
}

pub struct RecordingSender;

impl SendToPara for RecordingSender {
	fn send(message: MessageToPara) -> Result<(), ()> {
		if SendFails::get() {
			return Err(());
		}
		Sent::mutate(|sent| sent.push(message));
		Ok(())
	}
}

pub struct RecordingAnswers;

impl OnDepositHeld for RecordingAnswers {
	fn on_deposit_held(key: DepositKey, held: bool) {
		Answers::mutate(|answers| answers.push((key, held)));
	}
}

pub struct MockRation;

impl AdmitRequest for MockRation {
	fn admit(para: ParaId) -> bool {
		!Rationed::get().contains(&para)
	}
}

/// A signed account in `1_000_000 + para` acts as that para.
pub struct ParaAccounts;

impl EnsureOrigin<RuntimeOrigin> for ParaAccounts {
	type Success = ParaId;

	fn try_origin(o: RuntimeOrigin) -> Result<ParaId, RuntimeOrigin> {
		match frame_system::ensure_signed(o.clone()) {
			Ok(who) if who >= 1_000_000 => Ok((who - 1_000_000) as ParaId),
			_ => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		Ok(RuntimeOrigin::signed(PARA_A_ACCOUNT))
	}
}

impl pallet_hrmp_relay::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ParaOrigin = frame_system::EnsureSignedBy<ParaOriginAccount, AccountId>;
	type ParachainOrigin = ParaAccounts;
	type SendToPara = RecordingSender;
	type Hrmp = MockHrmp;
	type OnDepositHeld = RecordingAnswers;
	type AdmitRequest = MockRation;
	type WeightInfo = ();
}

frame_support::parameter_types! {
	/// The signed account that stands in for the deposit-holding parachain.
	pub static ParaOriginMembers: Vec<AccountId> = vec![CORETIME];
}

/// The deposit-holding parachain, as a signed account.
pub const CORETIME: AccountId = 7;

pub struct ParaOriginAccount;

impl frame_support::traits::SortedMembers<AccountId> for ParaOriginAccount {
	fn sorted_members() -> Vec<AccountId> {
		ParaOriginMembers::get()
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	HrmpCalls::set(vec![]);
	HrmpFails::set(None);
	Sent::set(vec![]);
	SendFails::set(false);
	Answers::set(vec![]);
	Rationed::set(vec![]);

	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Every event this pallet emitted, oldest first, clearing the log.
pub fn events() -> Vec<pallet_hrmp_relay::Event<Test>> {
	let events = System::events()
		.into_iter()
		.filter_map(|e| match e.event {
			RuntimeEvent::HrmpRelay(inner) => Some(inner),
			_ => None,
		})
		.collect();
	System::reset_events();
	events
}
