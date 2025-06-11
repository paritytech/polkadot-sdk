// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
	configuration::TestAuthorities,
	dummy_builder,
	environment::{TestEnvironment, TestEnvironmentDependencies, GENESIS_HASH},
	mock::{
		candidate_backing::MockCandidateBacking,
		chain_api::{ChainApiState, MockChainApi},
		network_bridge::{MockNetworkBridgeRx, MockNetworkBridgeTx},
		prospective_parachains::MockProspectiveParachains,
		runtime_api::{MockRuntimeApi, MockRuntimeApiCoreState},
		AlwaysSupportsParachains,
	},
	network::{new_network, NetworkEmulatorHandle, NetworkInterface, NetworkInterfaceReceiver},
	usage::BenchmarkUsage,
	NODE_UNDER_TEST,
};
use bitvec::vec::BitVec;
use colored::Colorize;
use itertools::Itertools;
use polkadot_node_core_dispute_coordinator::DisputeCoordinatorSubsystem;
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_network_protocol::{
	grid_topology::{SessionGridTopology, TopologyPeerInfo},
	request_response::{IncomingRequest, ReqProtocolNames},
	v3::{self, BackedCandidateManifest, StatementFilter},
	view, ValidationProtocols, View,
};
use polkadot_node_subsystem::messages::{
	network_bridge_event::NewGossipTopology, AllMessages, NetworkBridgeEvent,
	StatementDistributionMessage,
};
use polkadot_overseer::{
	Handle as OverseerHandle, Overseer, OverseerConnector, OverseerMetrics, SpawnGlue,
};
use polkadot_primitives::{
	AuthorityDiscoveryId, Block, GroupIndex, Hash, Id, ValidatorId, ValidatorIndex,
};
use polkadot_statement_distribution::StatementDistributionSubsystem;
use sc_keystore::LocalKeystore;
use sc_network_types::PeerId;
use sc_service::SpawnTaskHandle;
use sp_keystore::{Keystore, KeystorePtr};
use sp_runtime::RuntimeAppPublic;
use std::{
	sync::{atomic::Ordering, Arc},
	time::{Duration, Instant},
};
pub use test_state::TestState;

mod test_state;

const LOG_TARGET: &str = "subsystem-bench::disputes";

pub fn make_keystore() -> KeystorePtr {
	let keystore: KeystorePtr = Arc::new(LocalKeystore::in_memory());
	Keystore::sr25519_generate_new(&*keystore, ValidatorId::ID, Some("//Node0"))
		.expect("Insert key into keystore");
	Keystore::sr25519_generate_new(&*keystore, AuthorityDiscoveryId::ID, Some("//Node0"))
		.expect("Insert key into keystore");
	keystore
}

fn build_overseer(
	state: &TestState,
	network: NetworkEmulatorHandle,
	network_interface: NetworkInterface,
	network_receiver: NetworkInterfaceReceiver,
	dependencies: &TestEnvironmentDependencies,
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandle) {
	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let overseer_metrics = OverseerMetrics::try_register(&dependencies.registry).unwrap();
	let spawn_task_handle = dependencies.task_manager.spawn_handle();

	let store = todo!();
	let config = todo!();
	let keystore = make_keystore();
	let approval_voting_parallel_enabled = true;
	let dispute_coordinator = DisputeCoordinatorSubsystem::new(
		store,
		config,
		keystore,
		Metrics::try_register(&dependencies.registry).unwrap(),
		approval_voting_parallel_enabled,
	);

	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics)
		.replace_dispute_coordinator(|_| dispute_coordinator);

	let (overseer, raw_handle) = dummy.build_with_connector(overseer_connector).unwrap();
	let overseer_handle = OverseerHandle::new(raw_handle);

	(overseer, overseer_handle)
}

pub fn prepare_test(state: &TestState, with_prometheus_endpoint: bool) -> TestEnvironment {
	let dependencies = TestEnvironmentDependencies::default();
	let (network, network_interface, network_receiver) = new_network(
		&state.config,
		&dependencies,
		&state.test_authorities,
		vec![Arc::new(state.clone())],
	);
	let (overseer, overseer_handle) =
		build_overseer(state, network.clone(), network_interface, network_receiver, &dependencies);

	TestEnvironment::new(
		dependencies,
		state.config.clone(),
		network,
		overseer,
		overseer_handle,
		state.test_authorities.clone(),
		with_prometheus_endpoint,
	)
}

pub async fn benchmark_dispute_coordinator(
	env: &mut TestEnvironment,
	state: &TestState,
) -> BenchmarkUsage {
	todo!()
}
