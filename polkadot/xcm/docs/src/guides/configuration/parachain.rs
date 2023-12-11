//! # Mock parachain

use frame::prelude::*;
use frame::runtime::prelude::*;
use frame::traits::{Nothing, Everything};
use frame::deps::frame_system;
use xcm_executor::{XcmExecutor, Config};
use xcm::latest::prelude::*;

use super::mock_message_queue;

pub type Block = frame_system::mocking::MockBlock<Runtime>;

construct_runtime! {
    pub struct Runtime {
        System: frame_system,
        MessageQueue: mock_message_queue,
    }
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
    type Block = Block;
}

impl mock_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

#[docify::export(UniversalLocation)]
parameter_types! {
    pub UniversalLocation: InteriorMultiLocation = X2(
        GlobalConsensus(NetworkId::Polkadot), Parachain(2222)
    );
}

use xcm_config::XcmConfig;

mod xcm_config {
    use super::*;

    #[docify::export(Weigher)]
    mod weigher {
        use super::RuntimeCall;
        use frame::prelude::*;
        use frame::runtime::prelude::*;
        use xcm_builder::FixedWeightBounds;

        parameter_types! {
            pub const WeightPerInstruction: Weight = Weight::from_parts(1, 1);
            pub const MaxInstructions: u32 = 100;
        }

        pub type Weigher = FixedWeightBounds<WeightPerInstruction, RuntimeCall, MaxInstructions>;        
    }

    #[docify::export(asset_handling)]
    mod assets {
        pub type AssetTransactor = ;
    }

    #[docify::export]
    pub struct XcmConfig;

    #[docify::export(XcmConfigImpl)]
    impl Config for XcmConfig {
        type RuntimeCall = RuntimeCall;
        type XcmSender = ();
        type AssetTransactor = assets::AssetTransactor;
        type OriginConverter = ();
        type IsReserve = ();
        type IsTeleporter = ();
        type UniversalLocation = UniversalLocation;
        type Barrier = ();
        type Weigher = weigher::Weigher;
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
}
