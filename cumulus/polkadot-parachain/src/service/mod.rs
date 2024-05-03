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

mod start_nodes;
mod consensus;
mod rpc_extensions;
mod start_node_impl;
mod new_partial;

use cumulus_client_consensus_aura::collators::lookahead::{self as aura, Params as AuraParams};
use cumulus_client_consensus_common::ParachainBlockImport as TParachainBlockImport;
use sc_executor::{ WasmExecutor};
use sc_service::{PartialComponents, TFullBackend, TFullClient};
use sc_telemetry::{Telemetry, TelemetryWorkerHandle};
use std::{sync::Arc};

pub use parachains_common::{AccountId, Balance, Block, Hash, Nonce};

// Exports from the service module

pub use start_nodes::{
	asset_hub_lookahead::start_asset_hub_lookahead_node,
	basic_lookahead::start_basic_lookahead_node,
	generic_aura_lookahead::start_generic_aura_lookahead_node,
	rococo_contracts::start_contracts_rococo_node, rococo_parachain::start_rococo_parachain_node,
	shell::{start_shell_node, build_shell_import_queue},
};
pub use start_node_impl::start_node_impl;
pub use consensus::{
	lookahead_aura_consensus::{build_aura_import_queue, start_lookahead_aura_consensus},
	relay_chain_consensus::build_relay_to_aura_import_queue,
};
pub use new_partial::new_partial;
pub use rpc_extensions::{build_contracts_rpc_extensions, build_parachain_rpc_extensions};

#[cfg(not(feature = "runtime-benchmarks"))]
type HostFunctions = cumulus_client_service::ParachainHostFunctions;

#[cfg(feature = "runtime-benchmarks")]
type HostFunctions = (
	cumulus_client_service::ParachainHostFunctions,
	frame_benchmarking::benchmarking::HostFunctions,
);

type ParachainClient<RuntimeApi> = TFullClient<Block, RuntimeApi, WasmExecutor<HostFunctions>>;

type ParachainBackend = TFullBackend<Block>;

type ParachainBlockImport<RuntimeApi> =
	TParachainBlockImport<Block, Arc<ParachainClient<RuntimeApi>>, ParachainBackend>;

/// Assembly of PartialComponents (enough to run chain ops subcommands)
pub type Service<RuntimeApi> = PartialComponents<
	ParachainClient<RuntimeApi>,
	ParachainBackend,
	(),
	sc_consensus::DefaultImportQueue<Block>,
	sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>,
	(ParachainBlockImport<RuntimeApi>, Option<Telemetry>, Option<TelemetryWorkerHandle>),
>;
