// Copyright (C) Parity Technologies (UK) Ltd.
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

// From construct_runtime macro
#![allow(clippy::from_over_into)]

use crate::{Config, StoredMessagePayload};

use bp_messages::{
	calc_relayers_rewards,
	source_chain::{DeliveryConfirmationPayments, OnMessagesDelivered, TargetHeaderChain},
	target_chain::{
		DeliveryPayments, DispatchMessage, DispatchMessageData, MessageDispatch,
		ProvedLaneMessages, ProvedMessages, SourceHeaderChain,
	},
	DeliveredMessages, InboundLaneData, LaneId, Message, MessageKey, MessageNonce,
	UnrewardedRelayer, UnrewardedRelayersState, VerificationError,
};
use bp_runtime::{messages::MessageDispatchResult, Size};
use codec::{Decode, Encode};
use frame_support::{
	derive_impl, parameter_types,
	weights::{constants::RocksDbWeight, Weight},
};
use scale_info::TypeInfo;
use sp_runtime::BuildStorage;
use std::{
	collections::{BTreeMap, VecDeque},
	ops::RangeInclusive,
};

pub type AccountId = u64;
pub type Balance = u64;
#[derive(Decode, Encode, Clone, Debug, PartialEq, Eq, TypeInfo)]
pub struct TestPayload {
	/// Field that may be used to identify messages.
	pub id: u64,
	/// Dispatch weight that is declared by the message sender.
	pub declared_weight: Weight,
	/// Message dispatch result.
	///
	/// Note: in correct code `dispatch_result.unspent_weight` will always be <= `declared_weight`,
	/// but for test purposes we'll be making it larger than `declared_weight` sometimes.
	pub dispatch_result: MessageDispatchResult<TestDispatchLevelResult>,
	/// Extra bytes that affect payload size.
	pub extra: Vec<u8>,
}
pub type TestMessageFee = u64;
pub type TestRelayer = u64;
pub type TestDispatchLevelResult = ();

type Block = frame_system::mocking::MockBlock<TestRuntime>;

use crate as pallet_bridge_messages;

frame_support::construct_runtime! {
	pub enum TestRuntime
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Event<T>},
		Messages: pallet_bridge_messages::{Pallet, Call, Event<T>},
	}
}

pub type DbWeight = RocksDbWeight;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
	type DbWeight = DbWeight;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for TestRuntime {
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
}

parameter_types! {
	pub const MaxMessagesToPruneAtOnce: u64 = 10;
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: u64 = 16;
	pub const MaxUnconfirmedMessagesAtInboundLane: u64 = 128;
	pub const TestBridgedChainId: bp_runtime::ChainId = *b"test";
	pub const ActiveOutboundLanes: &'static [LaneId] = &[TEST_LANE_ID, TEST_LANE_ID_2];
}

/// weights of messages pallet calls we use in tests.
pub type TestWeightInfo = ();

impl Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = TestWeightInfo;
	type ActiveOutboundLanes = ActiveOutboundLanes;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize = frame_support::traits::ConstU32<MAX_OUTBOUND_PAYLOAD_SIZE>;
	type OutboundPayload = TestPayload;

	type InboundPayload = TestPayload;
	type InboundRelayer = TestRelayer;
	type DeliveryPayments = TestDeliveryPayments;

	type TargetHeaderChain = TestTargetHeaderChain;
	type DeliveryConfirmationPayments = TestDeliveryConfirmationPayments;
	type OnMessagesDelivered = TestOnMessagesDelivered;

	type SourceHeaderChain = TestSourceHeaderChain;
	type MessageDispatch = TestMessageDispatch;
	type BridgedChainId = TestBridgedChainId;
}

#[cfg(feature = "runtime-benchmarks")]
impl crate::benchmarking::Config<()> for TestRuntime {
	fn bench_lane_id() -> LaneId {
		TEST_LANE_ID
	}

	fn prepare_message_proof(
		params: crate::benchmarking::MessageProofParams,
	) -> (TestMessagesProof, Weight) {
		// in mock run we only care about benchmarks correctness, not the benchmark results
		// => ignore size related arguments
		let (messages, total_dispatch_weight) =
			params.message_nonces.into_iter().map(|n| message(n, REGULAR_PAYLOAD)).fold(
				(Vec::new(), Weight::zero()),
				|(mut messages, total_dispatch_weight), message| {
					let weight = REGULAR_PAYLOAD.declared_weight;
					messages.push(message);
					(messages, total_dispatch_weight.saturating_add(weight))
				},
			);
		let mut proof: TestMessagesProof = Ok(messages).into();
		proof.result.as_mut().unwrap().get_mut(0).unwrap().1.lane_state = params.outbound_lane_data;
		(proof, total_dispatch_weight)
	}

	fn prepare_message_delivery_proof(
		params: crate::benchmarking::MessageDeliveryProofParams<AccountId>,
	) -> TestMessagesDeliveryProof {
		// in mock run we only care about benchmarks correctness, not the benchmark results
		// => ignore size related arguments
		TestMessagesDeliveryProof(Ok((params.lane, params.inbound_lane_data)))
	}

	fn is_relayer_rewarded(_relayer: &AccountId) -> bool {
		true
	}
}

impl Size for TestPayload {
	fn size(&self) -> u32 {
		16 + self.extra.len() as u32
	}
}

/// Maximal outbound payload size.
pub const MAX_OUTBOUND_PAYLOAD_SIZE: u32 = 4096;

/// Account that has balance to use in tests.
pub const ENDOWED_ACCOUNT: AccountId = 0xDEAD;

/// Account id of test relayer.
pub const TEST_RELAYER_A: AccountId = 100;

/// Account id of additional test relayer - B.
pub const TEST_RELAYER_B: AccountId = 101;

/// Account id of additional test relayer - C.
pub const TEST_RELAYER_C: AccountId = 102;

/// Error that is returned by all test implementations.
pub const TEST_ERROR: &str = "Test error";

/// Lane that we're using in tests.
pub const TEST_LANE_ID: LaneId = LaneId([0, 0, 0, 1]);

/// Secondary lane that we're using in tests.
pub const TEST_LANE_ID_2: LaneId = LaneId([0, 0, 0, 2]);

/// Inactive outbound lane.
pub const TEST_LANE_ID_3: LaneId = LaneId([0, 0, 0, 3]);

/// Regular message payload.
pub const REGULAR_PAYLOAD: TestPayload = message_payload(0, 50);

/// Payload that is rejected by `TestTargetHeaderChain`.
pub const PAYLOAD_REJECTED_BY_TARGET_CHAIN: TestPayload = message_payload(1, 50);

/// Vec of proved messages, grouped by lane.
pub type MessagesByLaneVec = Vec<(LaneId, ProvedLaneMessages<Message>)>;

/// Test messages proof.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub struct TestMessagesProof {
	pub result: Result<MessagesByLaneVec, ()>,
}

impl Size for TestMessagesProof {
	fn size(&self) -> u32 {
		0
	}
}

impl From<Result<Vec<Message>, ()>> for TestMessagesProof {
	fn from(result: Result<Vec<Message>, ()>) -> Self {
		Self {
			result: result.map(|messages| {
				let mut messages_by_lane: BTreeMap<LaneId, ProvedLaneMessages<Message>> =
					BTreeMap::new();
				for message in messages {
					messages_by_lane.entry(message.key.lane_id).or_default().messages.push(message);
				}
				messages_by_lane.into_iter().collect()
			}),
		}
	}
}

/// Messages delivery proof used in tests.
#[derive(Debug, Encode, Decode, Eq, Clone, PartialEq, TypeInfo)]
pub struct TestMessagesDeliveryProof(pub Result<(LaneId, InboundLaneData<TestRelayer>), ()>);

impl Size for TestMessagesDeliveryProof {
	fn size(&self) -> u32 {
		0
	}
}

/// Target header chain that is used in tests.
#[derive(Debug, Default)]
pub struct TestTargetHeaderChain;

impl TargetHeaderChain<TestPayload, TestRelayer> for TestTargetHeaderChain {
	type MessagesDeliveryProof = TestMessagesDeliveryProof;

	fn verify_message(payload: &TestPayload) -> Result<(), VerificationError> {
		if *payload == PAYLOAD_REJECTED_BY_TARGET_CHAIN {
			Err(VerificationError::Other(TEST_ERROR))
		} else {
			Ok(())
		}
	}

	fn verify_messages_delivery_proof(
		proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<TestRelayer>), VerificationError> {
		proof.0.map_err(|_| VerificationError::Other(TEST_ERROR))
	}
}

/// Reward payments at the target chain during delivery transaction.
#[derive(Debug, Default)]
pub struct TestDeliveryPayments;

impl TestDeliveryPayments {
	/// Returns true if given relayer has been rewarded with given balance. The reward-paid flag is
	/// cleared after the call.
	pub fn is_reward_paid(relayer: AccountId) -> bool {
		let key = (b":delivery-relayer-reward:", relayer).encode();
		frame_support::storage::unhashed::take::<bool>(&key).is_some()
	}
}

impl DeliveryPayments<AccountId> for TestDeliveryPayments {
	type Error = &'static str;

	fn pay_reward(
		relayer: AccountId,
		_total_messages: MessageNonce,
		_valid_messages: MessageNonce,
		_actual_weight: Weight,
	) {
		let key = (b":delivery-relayer-reward:", relayer).encode();
		frame_support::storage::unhashed::put(&key, &true);
	}
}

/// Reward payments at the source chain during delivery confirmation transaction.
#[derive(Debug, Default)]
pub struct TestDeliveryConfirmationPayments;

impl TestDeliveryConfirmationPayments {
	/// Returns true if given relayer has been rewarded with given balance. The reward-paid flag is
	/// cleared after the call.
	pub fn is_reward_paid(relayer: AccountId, fee: TestMessageFee) -> bool {
		let key = (b":relayer-reward:", relayer, fee).encode();
		frame_support::storage::unhashed::take::<bool>(&key).is_some()
	}
}

impl DeliveryConfirmationPayments<AccountId> for TestDeliveryConfirmationPayments {
	type Error = &'static str;

	fn pay_reward(
		_lane_id: LaneId,
		messages_relayers: VecDeque<UnrewardedRelayer<AccountId>>,
		_confirmation_relayer: &AccountId,
		received_range: &RangeInclusive<MessageNonce>,
	) -> MessageNonce {
		let relayers_rewards = calc_relayers_rewards(messages_relayers, received_range);
		let rewarded_relayers = relayers_rewards.len();
		for (relayer, reward) in &relayers_rewards {
			let key = (b":relayer-reward:", relayer, reward).encode();
			frame_support::storage::unhashed::put(&key, &true);
		}

		rewarded_relayers as _
	}
}

/// Source header chain that is used in tests.
#[derive(Debug)]
pub struct TestSourceHeaderChain;

impl SourceHeaderChain for TestSourceHeaderChain {
	type MessagesProof = TestMessagesProof;

	fn verify_messages_proof(
		proof: Self::MessagesProof,
		_messages_count: u32,
	) -> Result<ProvedMessages<Message>, VerificationError> {
		proof
			.result
			.map(|proof| proof.into_iter().collect())
			.map_err(|_| VerificationError::Other(TEST_ERROR))
	}
}

/// Test message dispatcher.
#[derive(Debug)]
pub struct TestMessageDispatch;

impl TestMessageDispatch {
	pub fn deactivate() {
		frame_support::storage::unhashed::put(b"TestMessageDispatch.IsCongested", &true)
	}
}

impl MessageDispatch for TestMessageDispatch {
	type DispatchPayload = TestPayload;
	type DispatchLevelResult = TestDispatchLevelResult;

	fn is_active() -> bool {
		!frame_support::storage::unhashed::get_or_default::<bool>(
			b"TestMessageDispatch.IsCongested",
		)
	}

	fn dispatch_weight(message: &mut DispatchMessage<TestPayload>) -> Weight {
		match message.data.payload.as_ref() {
			Ok(payload) => payload.declared_weight,
			Err(_) => Weight::zero(),
		}
	}

	fn dispatch(
		message: DispatchMessage<TestPayload>,
	) -> MessageDispatchResult<TestDispatchLevelResult> {
		match message.data.payload.as_ref() {
			Ok(payload) => payload.dispatch_result.clone(),
			Err(_) => dispatch_result(0),
		}
	}
}

/// Test callback, called during message delivery confirmation transaction.
pub struct TestOnMessagesDelivered;

impl TestOnMessagesDelivered {
	pub fn call_arguments() -> Option<(LaneId, MessageNonce)> {
		frame_support::storage::unhashed::get(b"TestOnMessagesDelivered.OnMessagesDelivered")
	}
}

impl OnMessagesDelivered for TestOnMessagesDelivered {
	fn on_messages_delivered(lane: LaneId, enqueued_messages: MessageNonce) {
		frame_support::storage::unhashed::put(
			b"TestOnMessagesDelivered.OnMessagesDelivered",
			&(lane, enqueued_messages),
		);
	}
}

/// Return test lane message with given nonce and payload.
pub fn message(nonce: MessageNonce, payload: TestPayload) -> Message {
	Message { key: MessageKey { lane_id: TEST_LANE_ID, nonce }, payload: payload.encode() }
}

/// Return valid outbound message data, constructed from given payload.
pub fn outbound_message_data(payload: TestPayload) -> StoredMessagePayload<TestRuntime, ()> {
	StoredMessagePayload::<TestRuntime, ()>::try_from(payload.encode()).expect("payload too large")
}

/// Return valid inbound (dispatch) message data, constructed from given payload.
pub fn inbound_message_data(payload: TestPayload) -> DispatchMessageData<TestPayload> {
	DispatchMessageData { payload: Ok(payload) }
}

/// Constructs message payload using given arguments and zero unspent weight.
pub const fn message_payload(id: u64, declared_weight: u64) -> TestPayload {
	TestPayload {
		id,
		declared_weight: Weight::from_parts(declared_weight, 0),
		dispatch_result: dispatch_result(0),
		extra: Vec::new(),
	}
}

/// Returns message dispatch result with given unspent weight.
pub const fn dispatch_result(
	unspent_weight: u64,
) -> MessageDispatchResult<TestDispatchLevelResult> {
	MessageDispatchResult {
		unspent_weight: Weight::from_parts(unspent_weight, 0),
		dispatch_level_result: (),
	}
}

/// Constructs unrewarded relayer entry from nonces range and relayer id.
pub fn unrewarded_relayer(
	begin: MessageNonce,
	end: MessageNonce,
	relayer: TestRelayer,
) -> UnrewardedRelayer<TestRelayer> {
	UnrewardedRelayer { relayer, messages: DeliveredMessages { begin, end } }
}

/// Returns unrewarded relayers state at given lane.
pub fn inbound_unrewarded_relayers_state(lane: bp_messages::LaneId) -> UnrewardedRelayersState {
	let inbound_lane_data = crate::InboundLanes::<TestRuntime, ()>::get(lane).0;
	UnrewardedRelayersState::from(&inbound_lane_data)
}

/// Return test externalities to use in tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<TestRuntime> { balances: vec![(ENDOWED_ACCOUNT, 1_000_000)] }
		.assimilate_storage(&mut t)
		.unwrap();
	sp_io::TestExternalities::new(t)
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	new_test_ext().execute_with(test)
}
