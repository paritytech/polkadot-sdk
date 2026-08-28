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
//!
//! [`MockRegistry`] stands in for the relay chain's own HRMP pallet. It has opinions of its own —
//! it refuses what the real registry refuses — so this pallet is tested against a registry that
//! can say no, not a rubber stamp.

use crate::{self as pallet_hrmp_relay, SendToPara};
use frame_support::derive_impl;
use hrmp_primitives::{ChannelId, FailureReason, HrmpRegistry, MessageToPara, ParaId};
use sp_runtime::BuildStorage;

pub type AccountId = u64;

pub const ALICE: AccountId = 1;

pub const PARA_A: ParaId = 2000;
pub const PARA_B: ParaId = 2001;
/// A para the registry has never heard of.
pub const PARA_UNKNOWN: ParaId = 4999;

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
	pub type Hrmp = pallet_hrmp_relay::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlockU32<Test>;
	type AccountId = AccountId;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
}

frame_support::parameter_types! {
	/// Open-channel requests the registry is holding, and whether each is confirmed.
	pub static Requests: Vec<(ChannelId, bool)> = Vec::new();
	/// Channels the registry has.
	pub static OpenChannels: Vec<ChannelId> = Vec::new();
	/// Paras the registry will accept a channel for.
	pub static KnownParas: Vec<ParaId> = vec![PARA_A, PARA_B];
	/// When set, the next registry call fails with this reason.
	pub static NextFailure: Option<FailureReason> = None;
	/// Reports handed to the transport, oldest first.
	pub static SentMessages: Vec<MessageToPara> = Vec::new();
	/// When true, the transport refuses everything.
	pub static SendFails: bool = false;
}

/// A raw storage key [`MockRegistry`] writes before it can fail.
///
/// The recorders above are thread locals, which a storage layer cannot unwind. This is a real
/// storage write, so a test can prove the pallet's contract that a refusal leaves nothing behind.
pub const PARTIAL_WRITE_KEY: &[u8] = b":mock_partial_hrmp:";

/// Stands in for the relay chain's `hrmp` pallet.
pub struct MockRegistry;

impl MockRegistry {
	/// Fail if a test asked for it, having first written to storage — exactly as a real registry
	/// that validates late would.
	fn maybe_fail() -> Result<(), FailureReason> {
		if let Some(reason) = NextFailure::take() {
			frame_support::storage::unhashed::put(PARTIAL_WRITE_KEY, &1u32);
			return Err(reason);
		}
		Ok(())
	}
}

impl HrmpRegistry for MockRegistry {
	fn init_open_channel(
		channel: ChannelId,
		max_capacity: u32,
		max_message_size: u32,
	) -> Result<(), FailureReason> {
		Self::maybe_fail()?;
		if channel.sender == channel.recipient {
			return Err(FailureReason::InvalidPara);
		}
		if !KnownParas::get().contains(&channel.recipient) {
			return Err(FailureReason::InvalidPara);
		}
		if max_capacity == 0 || max_message_size == 0 {
			return Err(FailureReason::InvalidParameters);
		}
		if Requests::get().iter().any(|(c, _)| *c == channel) ||
			OpenChannels::get().contains(&channel)
		{
			return Err(FailureReason::AlreadyExists);
		}
		Requests::mutate(|r| r.push((channel, false)));
		Ok(())
	}

	fn accept_open_channel(channel: ChannelId) -> Result<(), FailureReason> {
		Self::maybe_fail()?;
		let mut requests = Requests::get();
		let entry = requests
			.iter_mut()
			.find(|(c, _)| *c == channel)
			.ok_or(FailureReason::NotFound)?;
		if entry.1 {
			return Err(FailureReason::AlreadyExists);
		}
		entry.1 = true;
		Requests::set(requests);
		OpenChannels::mutate(|c| c.push(channel));
		Ok(())
	}

	fn close_channel(channel: ChannelId, initiator: ParaId) -> Result<(), FailureReason> {
		Self::maybe_fail()?;
		if !channel.is_participant(initiator) {
			return Err(FailureReason::InvalidPara);
		}
		if !OpenChannels::get().contains(&channel) {
			return Err(FailureReason::NotFound);
		}
		OpenChannels::mutate(|c| c.retain(|x| *x != channel));
		Requests::mutate(|r| r.retain(|(c, _)| *c != channel));
		Ok(())
	}

	fn cancel_open_request(channel: ChannelId) -> Result<(), FailureReason> {
		Self::maybe_fail()?;
		let requests = Requests::get();
		match requests.iter().find(|(c, _)| *c == channel) {
			None => Err(FailureReason::NotFound),
			// The real registry refuses to cancel something already confirmed.
			Some((_, true)) => Err(FailureReason::AlreadyExists),
			Some(_) => {
				Requests::mutate(|r| r.retain(|(c, _)| *c != channel));
				Ok(())
			},
		}
	}

	fn establish_system_channel(channel: ChannelId) -> Result<(), FailureReason> {
		Self::maybe_fail()?;
		let back = ChannelId { sender: channel.recipient, recipient: channel.sender };
		for id in [channel, back] {
			if OpenChannels::get().contains(&id) {
				return Err(FailureReason::AlreadyExists);
			}
			OpenChannels::mutate(|c| c.push(id));
		}
		Ok(())
	}

	fn exists(channel: ChannelId) -> bool {
		OpenChannels::get().contains(&channel) ||
			Requests::get().iter().any(|(c, _)| *c == channel)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_openable(channel: ChannelId) {
		KnownParas::mutate(|k| {
			for p in [channel.sender, channel.recipient] {
				if !k.contains(&p) {
					k.push(p);
				}
			}
		});
	}
}

/// Records what would have gone back to the parachain.
pub struct RecordingSender;

impl SendToPara for RecordingSender {
	fn send(message: MessageToPara) -> Result<(), ()> {
		if SendFails::get() {
			return Err(());
		}
		SentMessages::mutate(|sent| sent.push(message));
		Ok(())
	}
}

/// Every report handed to the transport since the last call, oldest first.
pub fn take_sent() -> Vec<MessageToPara> {
	SentMessages::take()
}

impl pallet_hrmp_relay::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ParaOrigin = frame_system::EnsureRoot<AccountId>;
	type SendToPara = RecordingSender;
	type Registry = MockRegistry;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	Requests::set(Vec::new());
	OpenChannels::set(Vec::new());
	KnownParas::set(vec![PARA_A, PARA_B]);
	NextFailure::set(None);
	SentMessages::set(Vec::new());
	SendFails::set(false);

	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Every event this pallet emitted, oldest first, clearing the log.
pub fn hrmp_events() -> Vec<pallet_hrmp_relay::Event<Test>> {
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
