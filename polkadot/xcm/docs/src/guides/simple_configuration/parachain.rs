//! # Mock parachain

use frame::prelude::*;
use frame::runtime::prelude::*;
use xcm_executor::XcmExecutor;

use super::mock_message_queue;

construct_runtime! {
    pub struct Runtime {
        System: frame_system,
        MessageQueue: mock_message_queue,
    }
}

pub type Block = frame_system::mocking::MockBlock<Runtime>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
    type Block = Block;
}

impl mock_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

// TODO: Implement config trait
pub struct XcmConfig;
