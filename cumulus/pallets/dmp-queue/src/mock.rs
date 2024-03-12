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

#![cfg(test)]

use frame_support::{
	derive_impl, parameter_types,
	traits::{HandleMessage, QueueFootprint},
};
use sp_core::{bounded_vec::BoundedSlice, ConstU32};
use sp_runtime::traits::IdentityLookup;

type Block = frame_system::mocking::MockBlock<Runtime>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Runtime
	{
		System: frame_system,
		DmpQueue: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type PalletInfo = PalletInfo;
}

impl crate::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type DmpSink = RecordingDmpSink;
	type WeightInfo = ();
}

parameter_types! {
	/// All messages that came into the `DmpSink`.
	pub static RecordedMessages: Vec<Vec<u8>> = vec![];
}

/// Can be used as [`Config::DmpSink`] to record all messages that came in.
pub struct RecordingDmpSink;
impl HandleMessage for RecordingDmpSink {
	type MaxMessageLen = ConstU32<16>;

	fn handle_message(msg: BoundedSlice<u8, Self::MaxMessageLen>) {
		RecordedMessages::mutate(|n| n.push(msg.to_vec()));
	}

	fn handle_messages<'a>(_: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>) {
		unimplemented!()
	}

	fn sweep_queue() {
		unimplemented!()
	}

	fn footprint() -> QueueFootprint {
		unimplemented!()
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_io::TestExternalities::new(Default::default())
}
