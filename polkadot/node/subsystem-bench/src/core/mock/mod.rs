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

use polkadot_node_subsystem::{
	overseer, AllMessages, FromOrchestra, HeadSupportsParachains, Overseer, OverseerConnector,
	OverseerHandle, SpawnGlue, SpawnedSubsystem, Subsystem,
};
use polkadot_node_subsystem_types::Hash;

pub mod dummy;
mod temp;

use dummy::*;
use sc_service::SpawnTaskHandle;

struct AlwaysSupportsParachains {}
#[async_trait::async_trait]
impl HeadSupportsParachains for AlwaysSupportsParachains {
	async fn head_supports_parachains(&self, _head: &Hash) -> bool {
		true
	}
}

pub fn new_overseer_with_dummy_subsystems(spawn_task_handle: SpawnTaskHandle) {
    // Initialize a mock overseer.
    // All subsystem except approval_voting and approval_distribution are mock subsystems.
    let spawner_glue = SpawnGlue(spawn_task_handle);
    let overseer_connector = OverseerConnector::with_event_capacity(64000);
    let builder = Overseer::builder()
        .approval_voting(MockApprovalVoting {})
        .approval_distribution(MockApprovalDistribution {})
        .availability_recovery(MockAvailabilityRecovery {})
        .candidate_validation(MockCandidateValidation {})
        .chain_api(MockChainApi { })
        .chain_selection(MockChainSelection {})
        .dispute_coordinator(MockDisputeCoordinator {})
        .runtime_api(MockRuntimeApi { })
        .network_bridge_tx(MockNetworkBridgeTx {})
        .availability_distribution(MockAvailabilityDistribution {})
        .availability_store(MockAvailabilityStore {})
        .pvf_checker(MockPvfChecker {})
        .candidate_backing(MockCandidateBacking {})
        .statement_distribution(MockStatementDistribution {})
        .bitfield_signing(MockBitfieldSigning {})
        .bitfield_distribution(MockBitfieldDistribution {})
        .provisioner(MockProvisioner {})
        .network_bridge_rx(MockNetworkBridgeRx {})
        .collation_generation(MockCollationGeneration {})
        .collator_protocol(MockCollatorProtocol {})
        .gossip_support(MockGossipSupport {})
        .dispute_distribution(MockDisputeDistribution {})
        .prospective_parachains(MockProspectiveParachains {})
        .activation_external_listeners(Default::default())
        .span_per_active_leaf(Default::default())
        .active_leaves(Default::default())
        .metrics(Default::default())
        .supports_parachains(AlwaysSupportsParachains {})
        .spawner(spawner_glue);

    let (mock_overseer, mock_overseer_handle) =
        builder.build_with_connector(overseer_connector).expect("Should not fail");

}