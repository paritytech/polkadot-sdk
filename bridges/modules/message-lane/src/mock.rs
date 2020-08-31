// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use bp_message_lane::{LaneId, Message, MessageResult, OnMessageReceived};
use frame_support::{impl_outer_event, impl_outer_origin, parameter_types, weights::Weight};
use sp_core::H256;
use sp_runtime::{
	testing::Header as SubstrateHeader,
	traits::{BlakeTwo256, IdentityLookup},
	Perbill,
};

use crate::Trait;

pub type AccountId = u64;
pub type TestPayload = u64;

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct TestRuntime;

mod message_lane {
	pub use crate::Event;
}

impl_outer_event! {
	pub enum TestEvent for TestRuntime {
		frame_system<T>,
		message_lane,
	}
}

impl_outer_origin! {
	pub enum Origin for TestRuntime where system = frame_system {}
}

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const MaximumBlockWeight: Weight = 1024;
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
}

impl frame_system::Trait for TestRuntime {
	type Origin = Origin;
	type Index = u64;
	type Call = ();
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = SubstrateHeader;
	type Event = TestEvent;
	type BlockHashCount = BlockHashCount;
	type MaximumBlockWeight = MaximumBlockWeight;
	type DbWeight = ();
	type BlockExecutionWeight = ();
	type ExtrinsicBaseWeight = ();
	type MaximumExtrinsicWeight = ();
	type AvailableBlockRatio = AvailableBlockRatio;
	type MaximumBlockLength = MaximumBlockLength;
	type Version = ();
	type ModuleToIndex = ();
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type BaseCallFilter = ();
	type SystemWeightInfo = ();
}

parameter_types! {
	pub const MaxMessagesToPruneAtOnce: u64 = 10;
}

impl Trait for TestRuntime {
	type Event = TestEvent;
	type Payload = TestPayload;
	type MaxMessagesToPruneAtOnce = MaxMessagesToPruneAtOnce;
	type OnMessageReceived = TestMessageProcessor;
}

/// Lane that we're using in tests.
pub const TEST_LANE_ID: LaneId = [0, 0, 0, 1];

/// Regular message payload that is not PAYLOAD_TO_QUEUE.
pub const REGULAR_PAYLOAD: TestPayload = 0;

/// All messages with this payload are queued by TestMessageProcessor.
pub const PAYLOAD_TO_QUEUE: TestPayload = 42;

/// Message processor that immediately handles all messages except messages with PAYLOAD_TO_QUEUE payload.
#[derive(Debug, Default)]
pub struct TestMessageProcessor;

impl OnMessageReceived<TestPayload> for TestMessageProcessor {
	fn on_message_received(&mut self, message: Message<TestPayload>) -> MessageResult<TestPayload> {
		if message.payload == PAYLOAD_TO_QUEUE {
			MessageResult::NotProcessed(message)
		} else {
			MessageResult::Processed
		}
	}
}

/// Run message lane test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	sp_io::TestExternalities::new(Default::default()).execute_with(test)
}
