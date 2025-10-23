// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use crate::cli::Consensus;
use jsonrpsee::{core::RpcResult, proc_macros::rpc, RpcModule};
use polkadot_sdk::{
	parachains_common::Hash,
	sc_transaction_pool_api::TransactionPool,
	sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata},
	*,
};
use revive_dev_runtime::{AccountId, Nonce, OpaqueBlock};
use std::sync::{Arc, Mutex, atomic::AtomicU64};
use crate::service::FullBackend;
use crate::snapshot::{SnapshotManager, SnapshotRpcServer};

pub type SharedTimestampDelta = Arc<Mutex<Option<u64>>>;

/// Full client dependencies.
pub struct FullDeps<C, P> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// The backend instance to use.
	pub backend: Arc<FullBackend>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// Connection to allow RPC triggers for block production.
	pub manual_seal_sink:
		futures::channel::mpsc::Sender<sc_consensus_manual_seal::EngineCommand<Hash>>,
	/// Consensus
	pub consensus_type: Consensus,
	pub timestamp_delta: SharedTimestampDelta,
	pub next_timestamp: Arc<AtomicU64>,
}

#[rpc(server, client)]
pub trait HardhatRpc {
	#[method(name = "hardhat_getAutomine")]
	fn get_automine(&self) -> RpcResult<bool>;
	#[method(name = "evm_setAutomine", )]
	fn set_automine(&self, automine: bool) -> RpcResult<bool>;
}

pub struct HardhatRpcServerImpl {
	consensus_type: Consensus,
}

impl HardhatRpcServerImpl {
	pub fn new(consensus_type: Consensus) -> Self {
		Self { consensus_type }
	}
}

impl HardhatRpcServer for HardhatRpcServerImpl {
	fn get_automine(&self) -> RpcResult<bool> {
		Ok(match self.consensus_type {
			Consensus::InstantSeal => true,
			_ => false,
		})
	}

	fn set_automine(&self, automine: bool) -> RpcResult<bool> {
		// stub for backward compatibility
		// but we won't support dynamic switching yet
		Ok(match self.consensus_type {
			Consensus::InstantSeal => true,
			_ => false,
		})
	}
}

#[docify::export]
/// Instantiate all full RPC extensions.
pub fn create_full<C, P>(
	deps: FullDeps<C, P>,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
	C: Send
		+ Sync
		+ 'static
		+ sp_api::ProvideRuntimeApi<OpaqueBlock>
		+ HeaderBackend<OpaqueBlock>
		+ HeaderMetadata<OpaqueBlock, Error = BlockChainError>
		+ sc_client_api::BlockBackend<OpaqueBlock>,
	C::Api: sp_block_builder::BlockBuilder<OpaqueBlock>,
	C::Api: substrate_frame_rpc_system::AccountNonceApi<OpaqueBlock, AccountId, Nonce>,
	P: TransactionPool + 'static,
{
	use polkadot_sdk::sc_rpc::dev::{Dev, DevApiServer};
	use polkadot_sdk::substrate_frame_rpc_system::{System, SystemApiServer};
	use sc_consensus_manual_seal::rpc::{ManualSeal, ManualSealApiServer};

	let mut module = RpcModule::new(());
	let FullDeps { client, backend, pool, manual_seal_sink, consensus_type, timestamp_delta, next_timestamp } =
		deps;

	module.merge(System::new(client.clone(), pool.clone()).into_rpc())?;
	module.merge(Dev::new(client.clone()).into_rpc())?;
	module.merge(
		ManualSeal::<Hash>::new(manual_seal_sink.clone(), timestamp_delta.clone(), next_timestamp.clone()).into_rpc(),
	)?;
	module.merge(HardhatRpcServerImpl::new(consensus_type).into_rpc())?;
	module.merge(SnapshotManager::new(client, backend).into_rpc())?;

	Ok(module)
}
