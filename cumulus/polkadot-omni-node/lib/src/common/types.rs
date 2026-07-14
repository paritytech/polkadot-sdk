// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Common type definitions used throughout the parachain node.
//!
//! This module provides type aliases and re-exports for commonly used types
//! in parachain node implementations, reducing boilerplate and ensuring
//! consistency across the codebase.

use cumulus_client_consensus_common::ParachainBlockImport as TParachainBlockImport;
use cumulus_primitives_core::relay_chain::UncheckedExtrinsic;
use sc_consensus::DefaultImportQueue;
use sc_executor::WasmExecutor;
use sc_service::{PartialComponents, TFullBackend, TFullClient};
use sc_telemetry::{Telemetry, TelemetryWorkerHandle};
use sc_transaction_pool::TransactionPoolHandle;
use sp_runtime::{generic, traits::BlakeTwo256};

pub use parachains_common_types::{AccountId, Balance, Hash, Nonce};

/// Header type for parachain blocks.
///
/// Uses BlakeTwo256 hashing and is generic over the block number type.
type Header<BlockNumber> = generic::Header<BlockNumber, BlakeTwo256>;

/// Block type for parachain nodes.
///
/// Consists of a header and unchecked extrinsics from the relay chain.
pub type Block<BlockNumber> = generic::Block<Header<BlockNumber>, UncheckedExtrinsic>;

/// Host functions available in the parachain runtime when benchmarks are disabled.
///
/// Combines parachain host functions with statement store host functions.
#[cfg(not(feature = "runtime-benchmarks"))]
pub type ParachainHostFunctions = (
	cumulus_client_service::ParachainHostFunctions,
	sp_statement_store::runtime_api::HostFunctions,
);

/// Host functions available in the parachain runtime when benchmarks are enabled.
///
/// Extends the standard host functions with benchmarking capabilities.
#[cfg(feature = "runtime-benchmarks")]
pub type ParachainHostFunctions = (
	cumulus_client_service::ParachainHostFunctions,
	sp_statement_store::runtime_api::HostFunctions,
	frame_benchmarking::benchmarking::HostFunctions,
);

/// Full client type for parachain nodes.
///
/// Combines a block type, runtime API, and WASM executor with the appropriate
/// host functions.
pub type ParachainClient<Block, RuntimeApi> =
	TFullClient<Block, RuntimeApi, WasmExecutor<ParachainHostFunctions>>;

/// Backend type for parachain nodes.
///
/// Uses the full backend implementation from sc-service.
pub type ParachainBackend<Block> = TFullBackend<Block>;

/// Block import type for parachain nodes.
///
/// Wraps the underlying block import with parachain-specific functionality.
pub type ParachainBlockImport<Block, BI> =
	TParachainBlockImport<Block, BI, ParachainBackend<Block>>;

/// Complete assembly of partial components for parachain service construction.
///
/// This type alias represents all the components needed to build a parachain node,
/// including the client, backend, import queue, transaction pool, and additional
/// services like telemetry and block import.
pub type ParachainService<Block, RuntimeApi, BI, BIExtraReturnValue> = PartialComponents<
	ParachainClient<Block, RuntimeApi>,
	ParachainBackend<Block>,
	(),
	DefaultImportQueue<Block>,
	TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>,
	(
		ParachainBlockImport<Block, BI>,
		Option<Telemetry>,
		Option<TelemetryWorkerHandle>,
		BIExtraReturnValue,
	),
>;
