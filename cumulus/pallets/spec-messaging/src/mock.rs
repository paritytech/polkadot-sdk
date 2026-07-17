// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use crate as spec_messaging;
use cumulus_primitives_core::{ChannelInfo, ChannelStatus, GetChannelInfo, ParaId};
use frame_support::{derive_impl, parameter_types};
use polkadot_runtime_common::xcm_sender::NoPriceForMessageDelivery;
use sp_runtime::BuildStorage;
use xcm::AlwaysLatest;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		SpecMessaging: spec_messaging,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

parameter_types! {
	pub const MaxMsgLen: u32 = 64;
	pub const MaxMessagesPerBlock: u32 = 8;
}

impl spec_messaging::Config for Test {
	type MaxMsgLen = MaxMsgLen;
	type MaxMessagesPerBlock = MaxMessagesPerBlock;
}

parameter_types! {
	/// Siblings whose HRMP channel reports `Ready`.
	pub static HrmpReady: Vec<u32> = Vec::new();
	/// Siblings whose HRMP channel reports `Full`.
	pub static HrmpFull: Vec<u32> = Vec::new();
}

/// HRMP channel state as `ParachainSystem` would report it: `Ready`/`Full`
/// for the configured siblings, `Closed` for everyone else.
pub struct MockChannelInfo;
impl GetChannelInfo for MockChannelInfo {
	fn get_channel_status(id: ParaId) -> ChannelStatus {
		let id: u32 = id.into();
		if HrmpReady::get().contains(&id) {
			ChannelStatus::Ready(usize::MAX, usize::MAX)
		} else if HrmpFull::get().contains(&id) {
			ChannelStatus::Full
		} else {
			ChannelStatus::Closed
		}
	}

	fn get_channel_info(_id: ParaId) -> Option<ChannelInfo> {
		None
	}
}

/// The router under test, wired as a runtime would wire it.
pub type Router = spec_messaging::SpecMsgRouter<
	Test,
	MockChannelInfo,
	AlwaysLatest,
	NoPriceForMessageDelivery<ParaId>,
>;

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.expect("mock genesis storage builds; qed")
		.into()
}
