// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use std::sync::Arc;
use cumulus_client_cli::CollatorOptions;
use cumulus_client_service::CollatorSybilResistance;
use cumulus_primitives_core::ParaId;
use parachains_common::{Block, Hash};
use sc_network::NetworkBackend;
use sc_service::{Configuration, TaskManager};
use crate::service::{build_aura_import_queue, build_parachain_rpc_extensions, ParachainClient, start_lookahead_aura_consensus, start_node_impl};

/// Start a rococo parachain node.
pub async fn start_rococo_parachain_node<Net: NetworkBackend<Block, Hash>>(
    parachain_config: Configuration,
    polkadot_config: Configuration,
    collator_options: CollatorOptions,
    para_id: ParaId,
    hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<crate::fake_runtime_api::aura::RuntimeApi>>)> {
    start_node_impl::<crate::fake_runtime_api::aura::RuntimeApi, _, _, _, Net>(
        parachain_config,
        polkadot_config,
        collator_options,
        CollatorSybilResistance::Resistant, // Aura
        para_id,
        build_parachain_rpc_extensions::<crate::fake_runtime_api::aura::RuntimeApi>,
        build_aura_import_queue,
        start_lookahead_aura_consensus,
        hwbench,
    )
        .await
}