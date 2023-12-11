//! Mock relaychain

use frame::prelude::*;
use frame::runtime::prelude::*;
use frame::traits::{Nothing, Everything, ProcessMessage, ProcessMessageError};
use frame::deps::frame_system;
use xcm_simulator::{WeightMeter, AggregateMessageOrigin, UmpQueueId};
use xcm_executor::{XcmExecutor, Config};
use xcm::latest::prelude::*;
use xcm_builder::{ProcessXcmMessage, FixedWeightBounds};

pub type Block = frame_system::mocking::MockBlock<Runtime>;

construct_runtime! {
    pub struct Runtime {
        System: frame_system,
        MessageQueue: pallet_message_queue,
    }
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
    type Block = Block;
}

parameter_types! {
	/// Amount of weight that can be spent per block to service messages.
	pub MessageQueueServiceWeight: frame::prelude::Weight = frame::prelude::Weight::from_parts(1_000_000_000, 1_000_000);
	pub const MessageQueueHeapSize: u32 = 65_536;
	pub const MessageQueueMaxStale: u32 = 16;
}

/// Message processor to handle any messages that were enqueued into the `MessageQueue` pallet.
pub struct MessageProcessor;
impl ProcessMessage for MessageProcessor {
	type Origin = AggregateMessageOrigin;

	fn process_message(
		message: &[u8],
		origin: Self::Origin,
		meter: &mut WeightMeter,
		id: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError> {
		let para = match origin {
			AggregateMessageOrigin::Ump(UmpQueueId::Para(para)) => para,
		};
		ProcessXcmMessage::<
			Junction,
			XcmExecutor<XcmConfig>,
			RuntimeCall,
		>::process_message(message, Junction::Parachain(para.into()), meter, id)
	}
}

impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Size = u32;
	type HeapSize = MessageQueueHeapSize;
	type MaxStale = MessageQueueMaxStale;
	type ServiceWeight = MessageQueueServiceWeight;
	type MessageProcessor = MessageProcessor;
	type QueueChangeHandler = ();
	type QueuePausedQuery = ();
	type WeightInfo = ();
}

parameter_types! {
    pub UniversalLocation: InteriorMultiLocation = X1(GlobalConsensus(NetworkId::ByGenesis([0u8; 32])));
    pub const WeightPerInstruction: frame::prelude::Weight = frame::prelude::Weight::from_parts(1, 1);
    pub const MaxInstructions: u32 = 100;
}

pub struct XcmConfig;
impl Config for XcmConfig {
    type RuntimeCall = RuntimeCall;
    type XcmSender = ();
    type AssetTransactor = ();
    type OriginConverter = ();
    type IsReserve = ();
    type IsTeleporter = ();
    type UniversalLocation = UniversalLocation;
    type Barrier = ();
    type Weigher = FixedWeightBounds<WeightPerInstruction, RuntimeCall, MaxInstructions>;
    type Trader = ();
    type ResponseHandler = ();
    type AssetTrap = ();
    type AssetLocker = ();
    type AssetExchanger = ();
    type AssetClaims = ();
    type SubscriptionService = ();
    type PalletInstancesInfo = ();
    type FeeManager = ();
    type MaxAssetsIntoHolding = ();
    type MessageExporter = ();
    type UniversalAliases = Nothing;
    type CallDispatcher = RuntimeCall;
    type SafeCallFilter = Everything;
    type Aliasers = Nothing;
}
