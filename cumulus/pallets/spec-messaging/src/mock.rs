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
use core::ops::RangeInclusive;
use frame_support::{
	derive_impl, pallet_prelude::Weight, parameter_types,
	weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
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
	type BlockWeights = MockBlockWeights;
}

parameter_types! {
	// A finite proof limit, which `MaxBundleStreams` must cover.
	pub MockBlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::with_sensible_defaults(
			Weight::from_parts(2 * WEIGHT_REF_TIME_PER_SECOND, 1 << 20),
			sp_runtime::Perbill::from_percent(75),
		);
}

pub const SELF_PARA: u32 = 2000;

parameter_types! {
	pub const MaxMsgLen: u32 = 1024;
	pub const MaxMessagesPerBlock: u32 = 16;
	pub const MaxInFlight: u32 = 4;
	pub const MaxSendsPerBlock: u32 = 32;
	pub const MaxTouchedStreams: u32 = 8;
	pub const MaxBundleStreams: u32 = 512;
	pub const MaxTreeDepth: u32 = 4;
	pub const MaxInboundMessagesPerItem: u32 = 16;
	pub const MaxHelloPassPerBlock: u32 = 2;
	pub const MaxMessageSize: u32 = 1024;
	pub const MaxQueueFootprint: u32 = 4;
	pub SelfParaId: ParaId = ParaId::from(SELF_PARA);
}

impl cumulus_pallet_spec_messaging::Config for Test {
	type SelfParaId = SelfParaId;
	type MaxMsgLen = MaxMsgLen;
	type MaxMessagesPerBlock = MaxMessagesPerBlock;
	type MaxInFlight = MaxInFlight;
	type MaxSendsPerBlock = MaxSendsPerBlock;
	type MaxTouchedStreams = MaxTouchedStreams;
	type MaxBundleStreams = MaxBundleStreams;
	type MaxTreeDepth = MaxTreeDepth;
	type MaxInboundMessagesPerItem = MaxInboundMessagesPerItem;
	type MaxHelloPassPerBlock = MaxHelloPassPerBlock;
	type SupportedVersions = SupportedVersions;
	type MaxMessageSize = MaxMessageSize;
	type MaxQueueFootprint = MaxQueueFootprint;
	type QueueFootprint = MockQueueFootprint;
	type DataHandler = ();
	type WeightInfo = ();
	type OpenChannelOrigin = EnsureRoot<u64>;
	type AcceptChannelOrigin = EnsureRoot<u64>;
}

parameter_types! {
	/// The versions the pallet supports; tests change it to model a runtime upgrade.
	pub static SupportedVersions: RangeInclusive<u8> = 1..=1;
}

std::thread_local! {
	/// Ready pages every delivery queue reports.
	pub static READY_PAGES: core::cell::Cell<u32> = const { core::cell::Cell::new(0) };
}

pub struct MockQueueFootprint;

impl crate::QueueFootprintQuery<cumulus_primitives_spec_messaging::InChannelId>
	for MockQueueFootprint
{
	fn ready_pages(_: &cumulus_primitives_spec_messaging::InChannelId) -> u32 {
		READY_PAGES.with(|p| p.get())
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into();
	ext.execute_with(|| {
		System::initialize(&1, &Default::default(), &Default::default());
	});
	ext
}
