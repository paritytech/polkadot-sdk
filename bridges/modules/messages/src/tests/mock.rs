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

use crate::{
	tests::messages_generation::{
		encode_all_messages, encode_lane_data, prepare_message_delivery_storage_proof,
		prepare_messages_storage_proof,
	},
	Config, StoredMessagePayload,
};

use bp_header_chain::{ChainWithGrandpa, StoredHeaderData};
use bp_messages::{
	calc_relayers_rewards,
	source_chain::{
		DeliveryConfirmationPayments, FromBridgedChainMessagesDeliveryProof, OnMessagesDelivered,
	},
	target_chain::{
		DeliveryPayments, DispatchMessage, DispatchMessageData, FromBridgedChainMessagesProof,
		MessageDispatch,
	},
	ChainWithMessages, DeliveredMessages, HashedLaneId, InboundLaneData, LaneIdType, LaneState,
	Message, MessageKey, MessageNonce, OutboundLaneData, UnrewardedRelayer,
	UnrewardedRelayersState,
};
use bp_runtime::{
	messages::MessageDispatchResult, Chain, ChainId, Size, UnverifiedStorageProofParams,
};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	derive_impl,
	weights::{constants::RocksDbWeight, Weight},
};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
	testing::Header as SubstrateHeader,
	traits::{BlakeTwo256, ConstU32},
	BuildStorage, StateVersion,
};
use std::{collections::VecDeque, ops::RangeInclusive};

pub type AccountId = u64;
pub type Balance = u64;
#[derive(Decode, DecodeWithMemTracking, Encode, Clone, Debug, PartialEq, Eq, TypeInfo)]
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

pub struct ThisChain;

impl Chain for ThisChain {
	const ID: ChainId = *b"ttch";

	type BlockNumber = u64;
	type Hash = H256;
	type Hasher = BlakeTwo256;
	type Header = SubstrateHeader;
	type AccountId = AccountId;
	type Balance = Balance;
	type Nonce = u64;
	type Signature = sp_runtime::MultiSignature;
	const STATE_VERSION: StateVersion = StateVersion::V1;

	fn max_extrinsic_size() -> u32 {
		u32::MAX
	}

	fn max_extrinsic_weight() -> Weight {
		Weight::MAX
	}
}

impl ChainWithMessages for ThisChain {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str = "WithThisChainBridgeMessages";
	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 16;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 128;
}

pub struct BridgedChain;

pub type BridgedHeaderHash = H256;
pub type BridgedChainHeader = SubstrateHeader;

impl Chain for BridgedChain {
	const ID: ChainId = *b"tbch";

	type BlockNumber = u64;
	type Hash = BridgedHeaderHash;
	type Hasher = BlakeTwo256;
	type Header = BridgedChainHeader;
	type AccountId = TestRelayer;
	type Balance = Balance;
	type Nonce = u64;
	type Signature = sp_runtime::MultiSignature;
	const STATE_VERSION: StateVersion = StateVersion::V1;

	fn max_extrinsic_size() -> u32 {
		4096
	}

	fn max_extrinsic_weight() -> Weight {
		Weight::MAX
	}
}

impl ChainWithGrandpa for BridgedChain {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = "WithBridgedChainBridgeGrandpa";
	const MAX_AUTHORITIES_COUNT: u32 = 16;
	const REASONABLE_HEADERS_IN_JUSTIFICATION_ANCESTRY: u32 = 4;
	const MAX_MANDATORY_HEADER_SIZE: u32 = 4096;
	const AVERAGE_HEADER_SIZE: u32 = 4096;
}

impl ChainWithMessages for BridgedChain {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str = "WithBridgedChainBridgeMessages";
	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 16;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 128;
}

type Block = frame_system::mocking::MockBlock<TestRuntime>;

use crate as pallet_bridge_messages;

frame_support::construct_runtime! {
	pub enum TestRuntime
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Event<T>},
		BridgedChainGrandpa: pallet_bridge_grandpa::{Pallet, Call, Event<T>},
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
	type AccountStore = System;
}

impl pallet_bridge_grandpa::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = BridgedChain;
	type MaxFreeHeadersPerBlock = ConstU32<4>;
	type FreeHeadersInterval = ConstU32<1_024>;
	type HeadersToKeep = ConstU32<8>;
	type WeightInfo = pallet_bridge_grandpa::weights::BridgeWeight<TestRuntime>;
}

/// weights of messages pallet calls we use in tests.
pub type TestWeightInfo = ();

impl Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = TestWeightInfo;

	type ThisChain = ThisChain;
	type BridgedChain = BridgedChain;
	type BridgedHeaderChain = BridgedChainGrandpa;

	type OutboundPayload = TestPayload;
	type InboundPayload = TestPayload;
	type LaneId = TestLaneIdType;

	type DeliveryPayments = TestDeliveryPayments;
	type DeliveryConfirmationPayments = TestDeliveryConfirmationPayments;
	type OnMessagesDelivered = TestOnMessagesDelivered;

	type MessageDispatch = TestMessageDispatch;
}

#[cfg(feature = "runtime-benchmarks")]
impl crate::benchmarking::Config<()> for TestRuntime {
	fn bench_lane_id() -> Self::LaneId {
		test_lane_id()
	}

	fn prepare_message_proof(
		params: crate::benchmarking::MessageProofParams<Self::LaneId>,
	) -> (FromBridgedChainMessagesProof<BridgedHeaderHash, Self::LaneId>, Weight) {
		use bp_runtime::RangeInclusiveExt;

		let dispatch_weight =
			REGULAR_PAYLOAD.declared_weight * params.message_nonces.saturating_len();
		(
			*prepare_messages_proof(
				params.message_nonces.into_iter().map(|n| message(n, REGULAR_PAYLOAD)).collect(),
				params.outbound_lane_data,
			),
			dispatch_weight,
		)
	}

	fn prepare_message_delivery_proof(
		params: crate::benchmarking::MessageDeliveryProofParams<AccountId, Self::LaneId>,
	) -> FromBridgedChainMessagesDeliveryProof<BridgedHeaderHash, Self::LaneId> {
		// in mock run we only care about benchmarks correctness, not the benchmark results
		// => ignore size related arguments
		prepare_messages_delivery_proof(params.lane, params.inbound_lane_data)
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

/// Account that has balance to use in tests.
pub const ENDOWED_ACCOUNT: AccountId = 0xDEAD;

/// Account id of test relayer.
pub const TEST_RELAYER_A: AccountId = 100;

/// Account id of additional test relayer - B.
pub const TEST_RELAYER_B: AccountId = 101;

/// Account id of additional test relayer - C.
pub const TEST_RELAYER_C: AccountId = 102;

/// Lane identifier type used for tests.
pub type TestLaneIdType = HashedLaneId;
/// Lane that we're using in tests.
pub fn test_lane_id() -> TestLaneIdType {
	TestLaneIdType::try_new(1, 2).unwrap()
}

/// Lane that is completely unknown to our runtime.
pub fn unknown_lane_id() -> TestLaneIdType {
	TestLaneIdType::try_new(1, 3).unwrap()
}

/// Lane that is registered, but it is closed.
pub fn closed_lane_id() -> TestLaneIdType {
	TestLaneIdType::try_new(1, 4).unwrap()
}

/// Regular message payload.
pub const REGULAR_PAYLOAD: TestPayload = message_payload(0, 50);

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

impl DeliveryConfirmationPayments<AccountId, TestLaneIdType> for TestDeliveryConfirmationPayments {
	type Error = &'static str;

	fn pay_reward(
		_lane_id: TestLaneIdType,
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

/// Test message dispatcher.
#[derive(Debug)]
pub struct TestMessageDispatch;

impl TestMessageDispatch {
	pub fn deactivate(lane: TestLaneIdType) {
		// "enqueue" enough (to deactivate dispatcher) messages at dispatcher
		let latest_received_nonce = BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX + 1;
		for _ in 1..=latest_received_nonce {
			Self::emulate_enqueued_message(lane);
		}
	}

	pub fn emulate_enqueued_message(lane: TestLaneIdType) {
		let key = (b"dispatched", lane).encode();
		let dispatched = frame_support::storage::unhashed::get_or_default::<MessageNonce>(&key[..]);
		frame_support::storage::unhashed::put(&key[..], &(dispatched + 1));
	}
}

impl MessageDispatch for TestMessageDispatch {
	type DispatchPayload = TestPayload;
	type DispatchLevelResult = TestDispatchLevelResult;
	type LaneId = TestLaneIdType;

	fn is_active(lane: Self::LaneId) -> bool {
		frame_support::storage::unhashed::get_or_default::<MessageNonce>(
			&(b"dispatched", lane).encode()[..],
		) <= BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX
	}

	fn dispatch_weight(message: &mut DispatchMessage<TestPayload, Self::LaneId>) -> Weight {
		match message.data.payload.as_ref() {
			Ok(payload) => payload.declared_weight,
			Err(_) => Weight::zero(),
		}
	}

	fn dispatch(
		message: DispatchMessage<TestPayload, Self::LaneId>,
	) -> MessageDispatchResult<TestDispatchLevelResult> {
		match message.data.payload.as_ref() {
			Ok(payload) => {
				Self::emulate_enqueued_message(message.key.lane_id);
				payload.dispatch_result.clone()
			},
			Err(_) => dispatch_result(0),
		}
	}
}

/// Test callback, called during message delivery confirmation transaction.
pub struct TestOnMessagesDelivered;

impl TestOnMessagesDelivered {
	pub fn call_arguments() -> Option<(TestLaneIdType, MessageNonce)> {
		frame_support::storage::unhashed::get(b"TestOnMessagesDelivered.OnMessagesDelivered")
	}
}

impl OnMessagesDelivered<TestLaneIdType> for TestOnMessagesDelivered {
	fn on_messages_delivered(lane: TestLaneIdType, enqueued_messages: MessageNonce) {
		frame_support::storage::unhashed::put(
			b"TestOnMessagesDelivered.OnMessagesDelivered",
			&(lane, enqueued_messages),
		);
	}
}

/// Return test lane message with given nonce and payload.
pub fn message(nonce: MessageNonce, payload: TestPayload) -> Message<TestLaneIdType> {
	Message { key: MessageKey { lane_id: test_lane_id(), nonce }, payload: payload.encode() }
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
pub fn inbound_unrewarded_relayers_state(lane: TestLaneIdType) -> UnrewardedRelayersState {
	let inbound_lane_data = crate::InboundLanes::<TestRuntime, ()>::get(lane).unwrap().0;
	UnrewardedRelayersState::from(&inbound_lane_data)
}

/// Return test externalities to use in tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<TestRuntime> {
		balances: vec![(ENDOWED_ACCOUNT, 1_000_000)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	sp_io::TestExternalities::new(t)
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	new_test_ext().execute_with(|| {
		crate::InboundLanes::<TestRuntime, ()>::insert(test_lane_id(), InboundLaneData::opened());
		crate::OutboundLanes::<TestRuntime, ()>::insert(test_lane_id(), OutboundLaneData::opened());
		crate::InboundLanes::<TestRuntime, ()>::insert(
			closed_lane_id(),
			InboundLaneData { state: LaneState::Closed, ..Default::default() },
		);
		crate::OutboundLanes::<TestRuntime, ()>::insert(
			closed_lane_id(),
			OutboundLaneData { state: LaneState::Closed, ..Default::default() },
		);
		test()
	})
}

/// Prepare valid storage proof for given messages and insert appropriate header to the
/// bridged header chain.
///
/// Since this function changes the runtime storage, you can't "inline" it in the
/// `asset_noop` macro calls.
pub fn prepare_messages_proof(
	messages: Vec<Message<TestLaneIdType>>,
	outbound_lane_data: Option<OutboundLaneData>,
) -> Box<FromBridgedChainMessagesProof<BridgedHeaderHash, TestLaneIdType>> {
	// first - let's generate storage proof
	let lane = messages.first().unwrap().key.lane_id;
	let nonces_start = messages.first().unwrap().key.nonce;
	let nonces_end = messages.last().unwrap().key.nonce;
	let (storage_root, storage_proof) =
		prepare_messages_storage_proof::<BridgedChain, ThisChain, TestLaneIdType>(
			lane,
			nonces_start..=nonces_end,
			outbound_lane_data,
			UnverifiedStorageProofParams::default(),
			|nonce| messages[(nonce - nonces_start) as usize].payload.clone(),
			encode_all_messages,
			encode_lane_data,
			false,
			false,
		);

	// let's now insert bridged chain header into the storage
	let bridged_header_hash = Default::default();
	pallet_bridge_grandpa::ImportedHeaders::<TestRuntime>::insert(
		bridged_header_hash,
		StoredHeaderData { number: 0, state_root: storage_root },
	);

	Box::new(FromBridgedChainMessagesProof::<BridgedHeaderHash, TestLaneIdType> {
		bridged_header_hash,
		storage_proof,
		lane,
		nonces_start,
		nonces_end,
	})
}

/// Prepare valid storage proof for given messages and insert appropriate header to the
/// bridged header chain.
///
/// Since this function changes the runtime storage, you can't "inline" it in the
/// `asset_noop` macro calls.
pub fn prepare_messages_delivery_proof(
	lane: TestLaneIdType,
	inbound_lane_data: InboundLaneData<AccountId>,
) -> FromBridgedChainMessagesDeliveryProof<BridgedHeaderHash, TestLaneIdType> {
	// first - let's generate storage proof
	let (storage_root, storage_proof) =
		prepare_message_delivery_storage_proof::<BridgedChain, ThisChain, TestLaneIdType>(
			lane,
			inbound_lane_data,
			UnverifiedStorageProofParams::default(),
		);

	// let's now insert bridged chain header into the storage
	let bridged_header_hash = Default::default();
	pallet_bridge_grandpa::ImportedHeaders::<TestRuntime>::insert(
		bridged_header_hash,
		StoredHeaderData { number: 0, state_root: storage_root },
	);

	FromBridgedChainMessagesDeliveryProof::<BridgedHeaderHash, TestLaneIdType> {
		bridged_header_hash,
		storage_proof,
		lane,
	}
}
