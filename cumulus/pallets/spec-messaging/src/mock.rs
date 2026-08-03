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
use crate::{EnqueueToXcmQueue, OnSpecMsgData};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use cumulus_primitives_core::{
	AggregateMessageOrigin, ChannelInfo, ChannelStatus, GetChannelInfo, ParaId,
};
use cumulus_primitives_spec_messaging::{MessagePosition, StreamId, WindowGrant};
use frame_support::{
	derive_impl, parameter_types,
	traits::{
		Consideration, ConstU32, ConstU64, EitherOfDiverse, Footprint, ProcessMessage,
		ProcessMessageError, TransformOrigin,
	},
	weights::{Weight, WeightMeter},
};
use frame_system::{EnsureRoot, EnsureSigned};
use polkadot_runtime_common::xcm_sender::NoPriceForMessageDelivery;
use scale_info::TypeInfo;
use sp_runtime::{traits::Convert, BuildStorage, DispatchError};
use xcm::{latest::prelude::Location, AlwaysLatest};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		MessageQueue: pallet_message_queue,
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
	pub const MaxTouchedStreams: u32 = 4;
	pub const MaxContextGaps: u32 = 2;
	/// This chain's own id: inbound streams in the tests are addressed to
	/// para 100. Settable (`SelfPara::set`) so the cross-half lifecycle
	/// test can play two chains against each other.
	pub static SelfPara: ParaId = ParaId::from(100);
	/// The grant published for live inbound channels. ¼-window publish
	/// trigger: 4 messages or 1024 bytes.
	pub const TestGrant: WindowGrant =
		WindowGrant { max_messages: 16, max_bytes: 4096, max_message_size: 64 };
}

parameter_types! {
	/// Deposits taken by [`TestConsideration`]: `(who, footprint bytes)`.
	pub static Deposits: Vec<(u64, u64)> = Vec::new();
}

/// Recording acceptance-deposit consideration: every `new` is logged so
/// tests can assert who was charged for what.
#[derive(
	Clone, Debug, Decode, DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo,
)]
pub struct TestConsideration;

impl Consideration<u64, Footprint> for TestConsideration {
	fn new(who: &u64, footprint: Footprint) -> Result<Self, DispatchError> {
		Deposits::mutate(|deposits| deposits.push((*who, footprint.size)));
		Ok(Self)
	}

	fn update(self, _: &u64, _: Footprint) -> Result<Self, DispatchError> {
		Ok(self)
	}

	fn drop(self, _: &u64) -> Result<(), DispatchError> {
		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_: &u64, _: Footprint) {}
}

parameter_types! {
	/// Every `Data` payload the receiver part handed over, in consumption
	/// order.
	pub static ConsumedData: Vec<(ParaId, StreamId, MessagePosition, Vec<u8>)> = Vec::new();
}

/// Convert the source `ParaId` to the spec-msg aggregate origin — what
/// `parachains_common::message_queue::ParaIdToSpecMsg` does, duplicated here
/// to keep the dev-dependencies light.
pub struct ParaIdToSpecMsg;
impl Convert<ParaId, AggregateMessageOrigin> for ParaIdToSpecMsg {
	fn convert(para_id: ParaId) -> AggregateMessageOrigin {
		AggregateMessageOrigin::SpecMsg(para_id)
	}
}

/// The XCM-queue forwarding exactly as a runtime wires it.
pub type ForwardToXcmQueue = EnqueueToXcmQueue<
	TransformOrigin<MessageQueue, AggregateMessageOrigin, ParaId, ParaIdToSpecMsg>,
>;

/// Records consumed `Data` payloads, then forwards them into the message
/// queue like a runtime would.
pub struct RecordingDataHandler;
impl OnSpecMsgData for RecordingDataHandler {
	fn on_data(source: ParaId, stream: StreamId, position: MessagePosition, data: Vec<u8>) {
		ConsumedData::mutate(|consumed| consumed.push((source, stream, position, data.clone())));
		ForwardToXcmQueue::on_data(source, stream, position, data);
	}
}

impl spec_messaging::Config for Test {
	type SelfParaId = SelfPara;
	type MaxMsgLen = MaxMsgLen;
	type MaxMessagesPerBlock = MaxMessagesPerBlock;
	type MaxTouchedStreams = MaxTouchedStreams;
	type MaxContextGaps = MaxContextGaps;
	type DataHandler = RecordingDataHandler;
	type ChannelManagementOrigin = EnsureRoot<u64>;
	type OpenChannelOrigin = EnsureRoot<u64>;
	// Root accepts for free; any signed account may accept against the
	// (recorded) acceptance deposit.
	type AcceptChannelOrigin = EitherOfDiverse<EnsureRoot<u64>, EnsureSigned<u64>>;
	type AcceptConsideration = TestConsideration;
	type DefaultWindowGrant = TestGrant;
	type RegisterPublishAge = ConstU64<8>;
}

parameter_types! {
	pub const ServiceWeight: Weight = Weight::MAX;
	/// Every message the queue processed: the computed origin `Location`
	/// (the dispatch key `ProcessXcmMessage` hands to the XCM executor) and
	/// the message bytes.
	pub static ProcessedXcm: Vec<(Location, Vec<u8>)> = Vec::new();
}

/// Stand-in for `ProcessXcmMessage`: records each processed message under
/// its computed origin `Location` instead of executing it.
pub struct RecordingMessageProcessor;
impl ProcessMessage for RecordingMessageProcessor {
	type Origin = AggregateMessageOrigin;

	fn process_message(
		message: &[u8],
		origin: Self::Origin,
		_meter: &mut WeightMeter,
		_id: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError> {
		ProcessedXcm::mutate(|xcms| xcms.push((origin.into(), message.to_vec())));
		Ok(true)
	}
}

impl pallet_message_queue::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MessageProcessor = RecordingMessageProcessor;
	type Size = u32;
	type QueueChangeHandler = ();
	type QueuePausedQuery = ();
	type HeapSize = ConstU32<{ 64 * 1024 }>;
	type MaxStale = ConstU32<8>;
	type ServiceWeight = ServiceWeight;
	type IdleMaxServiceWeight = ();
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
