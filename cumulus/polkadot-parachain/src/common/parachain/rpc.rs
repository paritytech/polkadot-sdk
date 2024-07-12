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
	parachain::{ParachainBackend, ParachainClient},
	BuildRpcExtensions, ConstructNodeRuntimeApi, RpcModule,
};
use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
use parachains_common::{AccountId, Balance, Block, Nonce};
use sc_rpc::{
	dev::{Dev, DevApiServer},
	DenyUnsafe,
};
use sc_transaction_pool::FullPool;
use std::{marker::PhantomData, sync::Arc};
use substrate_frame_rpc_system::{System, SystemApiServer};
use substrate_state_trie_migration_rpc::{StateMigration, StateMigrationApiServer};

pub(crate) struct BuildEmptyRpcExtensions<RuntimeApi>(PhantomData<RuntimeApi>);

impl<RuntimeApi> BuildRpcExtensions<Block, ParachainClient<RuntimeApi>, ParachainBackend>
	for BuildEmptyRpcExtensions<RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
{
	fn build_rpc_extensions(
		_deny_unsafe: DenyUnsafe,
		_client: Arc<ParachainClient<RuntimeApi>>,
		_backend: Arc<ParachainBackend>,
		_pool: Arc<FullPool<Block, ParachainClient<RuntimeApi>>>,
	) -> sc_service::error::Result<RpcModule> {
		Ok(RpcModule::new(()))
	}
}

pub(crate) struct BuildParachainRpcExtensions<RuntimeApi>(PhantomData<RuntimeApi>);

impl<RuntimeApi> BuildRpcExtensions<Block, ParachainClient<RuntimeApi>, ParachainBackend>
	for BuildParachainRpcExtensions<RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
{
	fn build_rpc_extensions(
		deny_unsafe: DenyUnsafe,
		client: Arc<ParachainClient<RuntimeApi>>,
		backend: Arc<ParachainBackend>,
		pool: Arc<FullPool<Block, ParachainClient<RuntimeApi>>>,
	) -> sc_service::error::Result<RpcModule> {
		let build = || -> Result<RpcModule, Box<dyn std::error::Error + Send + Sync>> {
			let mut module = RpcModule::new(());

			module.merge(System::new(client.clone(), pool, deny_unsafe).into_rpc())?;
			module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
			module.merge(StateMigration::new(client.clone(), backend, deny_unsafe).into_rpc())?;
			module.merge(Dev::new(client, deny_unsafe).into_rpc())?;

			Ok(module)
		};
		build().map_err(Into::into)
	}
}
