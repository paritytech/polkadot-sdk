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
//! [`MockRegistry`] stands in for the relay chain's `hrmp` pallet. Keeping the real one out of
//! this crate is what lets it stay free of any Polkadot dependency; the two meeting for real is
//! the job of the `pallet-hrmp-test` crate.

// Helpers the per-flow tests will reach for as the handler bodies land.
#![allow(dead_code)]

use crate::{self as pallet_hrmp_relay, ForwardToPara, SendToPara};
use frame_support::{derive_impl, parameter_types, traits::EnsureOrigin};
use hrmp_primitives::{ChannelId, FailureReason, HrmpRegistry, MessageToPara, ParaId, ParaRequest};
use sp_runtime::BuildStorage;

pub type AccountId = u64;
pub type BlockNumber = u32;

pub const ALICE: AccountId = 1;

/// Sizes the registry opens system channels at, as `(max_capacity, max_message_size)`.
pub const SYSTEM_CHANNEL_SIZES: (u32, u32) = (1_000, 1_024);

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
	pub type Hrmp = pallet_hrmp_relay::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlockU32<Test>;
	type AccountId = AccountId;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
}

parameter_types! {
	/// Channels the registry holds.
	pub static RegistryChannels: Vec<ChannelId> = Vec::new();
	/// When set, the registry refuses everything with this reason.
	pub static RegistryRefuses: Option<FailureReason> = None;
	/// Reports the pallet handed to the transport, oldest first.
	pub static SentMessages: Vec<MessageToPara> = Vec::new();
	/// When true, the transport refuses everything.
	pub static SendFails: bool = false;
	/// Requests the pallet forwarded, oldest first.
	pub static ForwardedRequests: Vec<(ParaId, ParaRequest)> = Vec::new();
	/// When true, the forwarding transport refuses everything.
	pub static ForwardFails: bool = false;
	/// Signed accounts allowed to act as a para, as `(account, para id)`.
	pub static ParaOriginAccounts: Vec<(AccountId, ParaId)> = Vec::new();
}

/// An [`HrmpRegistry`] backed by [`RegistryChannels`], refusable through [`RegistryRefuses`].
pub struct MockRegistry;

impl MockRegistry {
	fn guard() -> Result<(), FailureReason> {
		RegistryRefuses::get().map_or(Ok(()), Err)
	}

	fn insert(channel: ChannelId) {
		RegistryChannels::mutate(|channels| {
			channels.retain(|existing| *existing != channel);
			channels.push(channel);
		});
	}

	fn remove(channel: ChannelId) {
		RegistryChannels::mutate(|channels| channels.retain(|existing| *existing != channel));
	}
}

impl HrmpRegistry for MockRegistry {
	fn open_channel(
		channel: ChannelId,
		_max_capacity: u32,
		_max_message_size: u32,
	) -> Result<(), FailureReason> {
		Self::guard()?;
		Self::insert(channel);
		Ok(())
	}

	fn open_system_channel(channel: ChannelId) -> Result<(u32, u32), FailureReason> {
		Self::guard()?;
		Self::insert(channel);
		Ok(SYSTEM_CHANNEL_SIZES)
	}

	fn open_system_pair(
		channel: ChannelId,
		_max_capacity: u32,
		_max_message_size: u32,
	) -> Result<(), FailureReason> {
		Self::guard()?;
		Self::insert(channel);
		Self::insert(channel.reversed());
		Ok(())
	}

	fn close_channel(channel: ChannelId, _initiator: ParaId) -> Result<(), FailureReason> {
		Self::guard()?;
		Self::remove(channel);
		Ok(())
	}

	fn force_clean(para_id: ParaId) -> Result<(), FailureReason> {
		Self::guard()?;
		RegistryChannels::mutate(|channels| {
			channels.retain(|channel| !channel.is_participant(para_id))
		});
		Ok(())
	}

	fn exists(channel: ChannelId) -> bool {
		RegistryChannels::get().contains(&channel)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_openable(_channel: ChannelId) {}
}

/// A [`SendToPara`] that records instead of sending, and can be made to fail.
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

/// Take everything recorded so far, clearing the log.
pub fn take_sent() -> Vec<MessageToPara> {
	SentMessages::mutate(core::mem::take)
}

/// A [`ForwardToPara`] that records instead of sending, and can be made to fail.
pub struct RecordingForwarder;

impl ForwardToPara for RecordingForwarder {
	fn forward(para_id: ParaId, request: ParaRequest) -> Result<(), ()> {
		if ForwardFails::get() {
			return Err(());
		}
		ForwardedRequests::mutate(|forwarded| forwarded.push((para_id, request)));
		Ok(())
	}
}

/// Take everything forwarded so far, clearing the log.
pub fn take_forwarded() -> Vec<(ParaId, ParaRequest)> {
	ForwardedRequests::mutate(core::mem::take)
}

/// Lets the accounts listed in [`ParaOriginAccounts`] act as their para, standing in for the
/// relay chain's native parachain origin. An explicit list, not an account range, so no other
/// account can resolve as a para by accident.
pub struct ParaAccounts;

impl EnsureOrigin<RuntimeOrigin> for ParaAccounts {
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
	let account = 1_000_000 + para_id as AccountId;
	ParaOriginAccounts::mutate(|paras| {
		if !paras.contains(&(account, para_id)) {
			paras.push((account, para_id));
		}
	});
	RuntimeOrigin::signed(account)
}

impl pallet_hrmp_relay::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ParaOrigin = frame_system::EnsureRoot<AccountId>;
	type SendToPara = RecordingSender;
	type ParachainOrigin = ParaAccounts;
	type ForwardToPara = RecordingForwarder;
	type Registry = MockRegistry;
	type WeightInfo = ();
}

/// Externalities with the registry and message log cleared.
pub fn new_test_ext() -> sp_io::TestExternalities {
	RegistryChannels::set(Vec::new());
	RegistryRefuses::set(None);
	SentMessages::set(Vec::new());
	SendFails::set(false);
	ForwardedRequests::set(Vec::new());
	ForwardFails::set(false);
	ParaOriginAccounts::set(Vec::new());

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
