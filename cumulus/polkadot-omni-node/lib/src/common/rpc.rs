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
