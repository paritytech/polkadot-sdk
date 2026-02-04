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
	sc_transaction_pool_api::TransactionPool,
	sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata},
	*,
};
use revive_dev_runtime::{AccountId, Nonce, OpaqueBlock};
use std::sync::Arc;

/// Full client dependencies.
pub struct FullDeps<C, P> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// The consensus type of the node.
	pub consensus: Consensus,
}

/// AutoMine JSON-RPC api.
/// Automine is a feature of the Hardhat Network where a new block is automatically mined after each
/// transaction.
#[rpc(server, client)]
pub trait AutoMineRpc {
	/// API to get the automine status.
	#[method(name = "getAutomine")]
	fn get_automine(&self) -> RpcResult<bool>;
}

/// Implementation of the AutoMine RPC api.
pub struct AutoMineRpcImpl {
	/// Whether the node is running in auto-mine mode.
	is_auto_mine: bool,
}

impl AutoMineRpcImpl {
	/// Create new `AutoMineRpcImpl` instance.
	pub fn new(consensus: Consensus) -> Self {
		Self { is_auto_mine: matches!(consensus, Consensus::InstantSeal) }
	}
}

impl AutoMineRpcServer for AutoMineRpcImpl {
	/// Returns `true` if block production is set to `instant`.
	fn get_automine(&self) -> RpcResult<bool> {
		Ok(self.is_auto_mine)
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
		+ 'static,
	C::Api: sp_block_builder::BlockBuilder<OpaqueBlock>,
	C::Api: substrate_frame_rpc_system::AccountNonceApi<OpaqueBlock, AccountId, Nonce>,
	P: TransactionPool + 'static,
{
	use polkadot_sdk::substrate_frame_rpc_system::{System, SystemApiServer};
	let mut module = RpcModule::new(());
	let FullDeps { client, pool, consensus } = deps;

	module.merge(AutoMineRpcImpl::new(consensus).into_rpc())?;
	module.merge(System::new(client.clone(), pool.clone()).into_rpc())?;

	Ok(module)
}
