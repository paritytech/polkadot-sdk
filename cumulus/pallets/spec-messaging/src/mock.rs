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

use crate as cumulus_pallet_spec_messaging;
use cumulus_primitives_spec_messaging::WindowGrant;
use frame_support::{derive_impl, parameter_types};
use frame_system::EnsureRoot;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		SpecMessaging: cumulus_pallet_spec_messaging,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

/// This chain's own id — consumed streams are addressed to it.
pub const SELF_PARA: u32 = 2000;

parameter_types! {
	pub const MaxMsgLen: u32 = 1024;
	pub const MaxMessagesPerBlock: u32 = 16;
	pub const MaxTouchedStreams: u32 = 8;
	pub const MaxContextGaps: u32 = 4;
	pub SelfParaId: ParaId = ParaId::from(SELF_PARA);
	/// Small, so tests reach the credit gate.
	pub const TestGrant: WindowGrant =
		WindowGrant { max_messages: 4, max_bytes: 4096, max_message_size: 1024 };
}

impl cumulus_pallet_spec_messaging::Config for Test {
	type SelfParaId = SelfParaId;
	type MaxMsgLen = MaxMsgLen;
	type MaxMessagesPerBlock = MaxMessagesPerBlock;
	type MaxTouchedStreams = MaxTouchedStreams;
	type MaxContextGaps = MaxContextGaps;
	type DataHandler = ();
	type OpenChannelOrigin = EnsureRoot<u64>;
	type AcceptChannelOrigin = EnsureRoot<u64>;
	type ChannelManagementOrigin = EnsureRoot<u64>;
	type DefaultWindowGrant = TestGrant;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into();
	ext.execute_with(|| System::set_block_number(1));
	ext
}
