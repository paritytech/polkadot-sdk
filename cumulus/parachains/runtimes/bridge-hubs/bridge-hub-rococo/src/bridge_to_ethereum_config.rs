use crate::{
	xcm_config::{AgentIdOf, UniversalLocation},
	Runtime,
};
use snowbridge_rococo_common::EthereumNetwork;
use snowbridge_router_primitives::outbound::EthereumBlobExporter;

pub type SnowbridgeExporter = EthereumBlobExporter<
	UniversalLocation,
	EthereumNetwork,
	snowbridge_outbound_queue::Pallet<Runtime>,
	AgentIdOf,
>;
