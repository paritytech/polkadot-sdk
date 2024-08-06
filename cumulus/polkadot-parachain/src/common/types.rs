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

use cumulus_client_consensus_common::ParachainBlockImport as TParachainBlockImport;
use sc_consensus::DefaultImportQueue;
use sc_executor::WasmExecutor;
use sc_service::{PartialComponents, TFullBackend, TFullClient};
use sc_telemetry::{Telemetry, TelemetryWorkerHandle};
use sc_transaction_pool::FullPool;
use std::sync::Arc;

pub use parachains_common::{AccountId, Balance, Block, Hash, Nonce};

#[cfg(not(feature = "runtime-benchmarks"))]
pub type ParachainHostFunctions = cumulus_client_service::ParachainHostFunctions;

#[cfg(feature = "runtime-benchmarks")]
pub type ParachainHostFunctions = (
	cumulus_client_service::ParachainHostFunctions,
	frame_benchmarking::benchmarking::HostFunctions,
);

pub type ParachainClient<Block, RuntimeApi> =
	TFullClient<Block, RuntimeApi, WasmExecutor<ParachainHostFunctions>>;

pub type ParachainBackend<Block> = TFullBackend<Block>;

pub type ParachainBlockImport<Block, RuntimeApi> =
	TParachainBlockImport<Block, Arc<ParachainClient<Block, RuntimeApi>>, ParachainBackend<Block>>;

/// Assembly of PartialComponents (enough to run chain ops subcommands)
pub type ParachainService<Block, RuntimeApi> = PartialComponents<
	ParachainClient<Block, RuntimeApi>,
	ParachainBackend<Block>,
	(),
	DefaultImportQueue<Block>,
	FullPool<Block, ParachainClient<Block, RuntimeApi>>,
	(ParachainBlockImport<Block, RuntimeApi>, Option<Telemetry>, Option<TelemetryWorkerHandle>),
>;
