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
use frame_support::{derive_impl, parameter_types};
use sp_runtime::BuildStorage;

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

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.expect("mock genesis storage builds; qed")
		.into()
}
