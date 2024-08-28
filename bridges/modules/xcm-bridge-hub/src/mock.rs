// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

#![cfg(test)]

use crate as pallet_xcm_bridge_hub;

use bp_messages::{
	target_chain::{DispatchMessage, MessageDispatch},
	ChainWithMessages, LaneId, MessageNonce,
};
use bp_runtime::{messages::MessageDispatchResult, Chain, ChainId, HashOf};
use bridge_runtime_common::messages_xcm_extension::{SenderAndLane, XcmBlobHauler};
use codec::Encode;
use frame_support::{
	assert_ok, derive_impl, parameter_types,
	traits::{Everything, NeverEnsureOrigin},
	weights::RuntimeDbWeight,
};
use sp_core::H256;
use sp_runtime::{
	testing::Header as SubstrateHeader,
	traits::{BlakeTwo256, ConstU128, ConstU32, IdentityLookup},
	AccountId32, BuildStorage, StateVersion,
};
use sp_std::cell::RefCell;
use xcm::prelude::*;
use xcm_builder::{
	AllowUnpaidExecutionFrom, FixedWeightBounds, InspectMessageQueues, NetworkExportTable,
	NetworkExportTableItem,
};
use xcm_executor::XcmExecutor;

pub type AccountId = AccountId32;
pub type Balance = u64;

type Block = frame_system::mocking::MockBlock<TestRuntime>;

pub const SIBLING_ASSET_HUB_ID: u32 = 2001;
pub const THIS_BRIDGE_HUB_ID: u32 = 2002;
pub const BRIDGED_ASSET_HUB_ID: u32 = 1001;
pub const TEST_LANE_ID: LaneId = LaneId([0, 0, 0, 1]);

frame_support::construct_runtime! {
	pub enum TestRuntime {
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Event<T>},
		Messages: pallet_bridge_messages::{Pallet, Call, Event<T>},
		XcmOverBridge: pallet_xcm_bridge_hub::{Pallet},
		XcmOverBridgeRouter: pallet_xcm_bridge_hub_router,
	}
}

parameter_types! {
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight { read: 1, write: 2 };
	pub const ExistentialDeposit: Balance = 1;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for TestRuntime {
	type AccountId = AccountId;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Block = Block;
	type Lookup = IdentityLookup<Self::AccountId>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for TestRuntime {
	type AccountStore = System;
}

parameter_types! {
	pub const ActiveOutboundLanes: &'static [LaneId] = &[TEST_LANE_ID];
}

impl pallet_bridge_messages::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = TestMessagesWeights;

	type ActiveOutboundLanes = ActiveOutboundLanes;
	type OutboundPayload = Vec<u8>;
	type InboundPayload = Vec<u8>;
	type DeliveryPayments = ();
	type DeliveryConfirmationPayments = ();
	type OnMessagesDelivered = ();
	type MessageDispatch = TestMessageDispatch;

	type ThisChain = ThisUnderlyingChain;
	type BridgedChain = BridgedUnderlyingChain;
	type BridgedHeaderChain = BridgedHeaderChain;
}

pub struct TestMessagesWeights;

impl pallet_bridge_messages::WeightInfo for TestMessagesWeights {
	fn receive_single_message_proof() -> Weight {
		Weight::zero()
	}
	fn receive_n_messages_proof(_: u32) -> Weight {
		Weight::zero()
	}
	fn receive_single_message_proof_with_outbound_lane_state() -> Weight {
		Weight::zero()
	}
	fn receive_single_n_bytes_message_proof(_: u32) -> Weight {
		Weight::zero()
	}
	fn receive_delivery_proof_for_single_message() -> Weight {
		Weight::zero()
	}
	fn receive_delivery_proof_for_two_messages_by_single_relayer() -> Weight {
		Weight::zero()
	}
	fn receive_delivery_proof_for_two_messages_by_two_relayers() -> Weight {
		Weight::zero()
	}
	fn receive_single_n_bytes_message_proof_with_dispatch(_: u32) -> Weight {
		Weight::zero()
	}
}

impl pallet_bridge_messages::WeightInfoExt for TestMessagesWeights {
	fn expected_extra_storage_proof_size() -> u32 {
		0
	}

	fn receive_messages_proof_overhead_from_runtime() -> Weight {
		Weight::zero()
	}

	fn receive_messages_delivery_proof_overhead_from_runtime() -> Weight {
		Weight::zero()
	}
}

parameter_types! {
	pub const RelayNetwork: NetworkId = NetworkId::Kusama;
	pub UniversalLocation: InteriorLocation = [
		GlobalConsensus(RelayNetwork::get()),
		Parachain(THIS_BRIDGE_HUB_ID),
	].into();
	pub SiblingLocation: Location = Location::new(1, [Parachain(SIBLING_ASSET_HUB_ID)]);

	pub const BridgedRelayNetwork: NetworkId = NetworkId::Polkadot;
	pub BridgedRelayNetworkLocation: Location = (Parent, GlobalConsensus(BridgedRelayNetwork::get())).into();
	pub BridgedRelativeDestination: InteriorLocation = [Parachain(BRIDGED_ASSET_HUB_ID)].into();
	pub BridgedUniversalDestination: InteriorLocation = [GlobalConsensus(BridgedRelayNetwork::get()), Parachain(BRIDGED_ASSET_HUB_ID)].into();
	pub const NonBridgedRelayNetwork: NetworkId = NetworkId::Rococo;

	pub const BridgeDeposit: Balance = 100_000;
	pub const Penalty: Balance = 1_000;

	// configuration for pallet_xcm_bridge_hub_router
	pub BridgeHubLocation: Location = Here.into();
	pub BridgeFeeAsset: AssetId = Location::here().into();
	pub BridgeTable: Vec<NetworkExportTableItem>
		= vec![
			NetworkExportTableItem::new(
				BridgedRelayNetwork::get(),
				None,
				BridgeHubLocation::get(),
				None
			)
		];
	pub UnitWeightCost: Weight = Weight::from_parts(10, 10);
}

impl pallet_xcm_bridge_hub::Config for TestRuntime {
	type UniversalLocation = UniversalLocation;
	type BridgedNetwork = BridgedRelayNetworkLocation;
	type BridgeMessagesPalletInstance = ();

	type MessageExportPrice = ();
	type DestinationVersion = AlwaysLatest;

	type Lanes = TestLanes;
	type LanesSupport = TestXcmBlobHauler;
}

impl pallet_xcm_bridge_hub_router::Config<()> for TestRuntime {
	type WeightInfo = ();

	type UniversalLocation = UniversalLocation;
	type BridgedNetworkId = BridgedRelayNetwork;
	type Bridges = NetworkExportTable<BridgeTable>;
	type DestinationVersion = AlwaysLatest;

	type BridgeHubOrigin = NeverEnsureOrigin<AccountId>;
	type ToBridgeHubSender = TestExportXcmWithXcmOverBridge;

	type ByteFee = ConstU128<0>;
	type FeeAsset = BridgeFeeAsset;

	type WithBridgeHubChannel = ();
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ();
	type AssetTransactor = ();
	type OriginConverter = ();
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = AllowUnpaidExecutionFrom<Everything>;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, ConstU32<100>>;
	type Trader = ();
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type MaxAssetsIntoHolding = ();
	type AssetLocker = ();
	type AssetExchanger = ();
	type FeeManager = ();
	// We just set `MessageExporter` as our `pallet_xcm_bridge_hub` instance.
	type MessageExporter = (XcmOverBridge,);
	type UniversalAliases = ();
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = ();
	type TransactionalProcessor = ();
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = ();
}

thread_local! {
	pub static EXECUTE_XCM_ORIGIN: RefCell<Option<Location>> = RefCell::new(None);
}

/// The `SendXcm` implementation directly executes XCM using `XcmExecutor`.
///
/// We ensure that the `ExportMessage` produced by `pallet_xcm_bridge_hub_router` is compatible with
/// the `ExportXcm` implementation of `pallet_xcm_bridge_hub`.
///
/// Note: The crucial part is that `ExportMessage` is processed by `XcmExecutor`, which calls the
/// `ExportXcm` implementation of `pallet_xcm_bridge_hub` as `MessageExporter`.
pub struct TestExportXcmWithXcmOverBridge;
impl SendXcm for TestExportXcmWithXcmOverBridge {
	type Ticket = Xcm<()>;

	fn validate(
		_: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		Ok((message.take().unwrap(), Assets::new()))
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		let xcm: Xcm<RuntimeCall> = ticket.into();

		let origin = EXECUTE_XCM_ORIGIN.with(|o| o.borrow().clone().unwrap());
		let mut hash = xcm.using_encoded(sp_io::hashing::blake2_256);
		let outcome = XcmExecutor::<XcmConfig>::prepare_and_execute(
			origin,
			xcm,
			&mut hash,
			Weight::MAX,
			Weight::zero(),
		);
		assert_ok!(outcome.ensure_complete());

		Ok(hash)
	}
}
impl InspectMessageQueues for TestExportXcmWithXcmOverBridge {
	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		todo!()
	}
}
impl TestExportXcmWithXcmOverBridge {
	pub fn set_origin_for_execute(origin: Location) {
		EXECUTE_XCM_ORIGIN.with(|o| *o.borrow_mut() = Some(origin));
	}
}

parameter_types! {
	pub TestSenderAndLane: SenderAndLane = SenderAndLane {
		location: SiblingLocation::get(),
		lane: TEST_LANE_ID,
	};
	pub TestLanes: sp_std::vec::Vec<(SenderAndLane, (NetworkId, InteriorLocation))> = sp_std::vec![
		(TestSenderAndLane::get(), (BridgedRelayNetwork::get(), BridgedRelativeDestination::get()))
	];
}

pub struct TestXcmBlobHauler;
impl XcmBlobHauler for TestXcmBlobHauler {
	type Runtime = TestRuntime;
	type MessagesInstance = ();
	type ToSourceChainSender = ();
	type CongestedMessage = ();
	type UncongestedMessage = ();
}

pub struct ThisUnderlyingChain;

impl Chain for ThisUnderlyingChain {
	const ID: ChainId = *b"tuch";
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

impl ChainWithMessages for ThisUnderlyingChain {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str = "";

	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 16;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 1000;
}

pub struct BridgedUnderlyingChain;
pub type BridgedHeaderHash = H256;
pub type BridgedChainHeader = SubstrateHeader;

impl Chain for BridgedUnderlyingChain {
	const ID: ChainId = *b"bgdc";
	type BlockNumber = u64;
	type Hash = BridgedHeaderHash;
	type Hasher = BlakeTwo256;
	type Header = BridgedChainHeader;
	type AccountId = AccountId;
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

impl ChainWithMessages for BridgedUnderlyingChain {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str = "";
	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 16;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 1000;
}

/// Test message dispatcher.
pub struct TestMessageDispatch;

impl TestMessageDispatch {
	pub fn deactivate(lane: LaneId) {
		frame_support::storage::unhashed::put(&(b"inactive", lane).encode()[..], &false);
	}
}

impl MessageDispatch for TestMessageDispatch {
	type DispatchPayload = Vec<u8>;
	type DispatchLevelResult = ();

	fn is_active() -> bool {
		frame_support::storage::unhashed::take::<bool>(&(b"inactive").encode()[..]) != Some(false)
	}

	fn dispatch_weight(_message: &mut DispatchMessage<Self::DispatchPayload>) -> Weight {
		Weight::zero()
	}

	fn dispatch(
		_: DispatchMessage<Self::DispatchPayload>,
	) -> MessageDispatchResult<Self::DispatchLevelResult> {
		MessageDispatchResult { unspent_weight: Weight::zero(), dispatch_level_result: () }
	}
}

pub struct BridgedHeaderChain;
impl bp_header_chain::HeaderChain<BridgedUnderlyingChain> for BridgedHeaderChain {
	fn finalized_header_state_root(
		_hash: HashOf<BridgedUnderlyingChain>,
	) -> Option<HashOf<BridgedUnderlyingChain>> {
		unreachable!()
	}
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	sp_io::TestExternalities::new(
		frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap(),
	)
	.execute_with(test)
}
