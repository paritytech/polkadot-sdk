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

//! Parachain-specific RPCs implementation.

#![warn(missing_docs)]

use crate::common::{
	types::{AccountId, Balance, Nonce, ParachainBackend, ParachainClient},
	ConstructNodeRuntimeApi,
};
use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
use sc_rpc::dev::{Dev, DevApiServer};
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};
use substrate_frame_rpc_system::{System, SystemApiServer};
use substrate_state_trie_migration_rpc::{StateMigration, StateMigrationApiServer};

/// A type representing all RPC extensions.
pub type RpcExtension = jsonrpsee::RpcModule<()>;

pub(crate) trait BuildRpcExtensions<Client, Backend, Pool> {
	fn build_rpc_extensions(
		client: Arc<Client>,
		backend: Arc<Backend>,
		pool: Arc<Pool>,
	) -> sc_service::error::Result<RpcExtension>;
}

pub(crate) struct BuildParachainRpcExtensions<Block, RuntimeApi>(PhantomData<(Block, RuntimeApi)>);

impl<Block: BlockT, RuntimeApi>
	BuildRpcExtensions<
		ParachainClient<Block, RuntimeApi>,
		ParachainBackend<Block>,
		sc_transaction_pool::TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>,
	> for BuildParachainRpcExtensions<Block, RuntimeApi>
where
	RuntimeApi:
		ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
{
	fn build_rpc_extensions(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		backend: Arc<ParachainBackend<Block>>,
		pool: Arc<
			sc_transaction_pool::TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>,
		>,
	) -> sc_service::error::Result<RpcExtension> {
		let build = || -> Result<RpcExtension, Box<dyn std::error::Error + Send + Sync>> {
			let mut module = RpcExtension::new(());

			module.merge(System::new(client.clone(), pool).into_rpc())?;
			module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
			module.merge(StateMigration::new(client.clone(), backend).into_rpc())?;
			module.merge(Dev::new(client).into_rpc())?;

			Ok(module)
		};
		build().map_err(Into::into)
	}
}
