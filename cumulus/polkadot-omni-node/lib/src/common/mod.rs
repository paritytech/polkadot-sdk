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

//! Cumulus parachain collator primitives.

#![warn(missing_docs)]

pub(crate) mod aura;
pub mod chain_spec;
pub mod command;
pub mod rpc;
pub mod runtime;
pub mod spec;
pub(crate) mod statement_store;
pub mod types;

use crate::cli::AuthoringPolicy;

use cumulus_primitives_core::{CollectCollationInfo, GetParachainInfo, RelayParentOffsetApi};
use sc_client_db::DbHash;
use sc_offchain::OffchainWorkerApi;
use serde::de::DeserializeOwned;
use sp_api::{ApiExt, CallApiAt, ConstructRuntimeApi, Metadata};
use sp_block_builder::BlockBuilder;
use sp_runtime::{
	traits::{Block as BlockT, BlockNumber, Header as HeaderT, NumberFor},
	OpaqueExtrinsic,
};
use sp_session::SessionKeys;
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use sp_transaction_storage_proof::runtime_api::TransactionStorageApi;
use std::{fmt::Debug, path::{Path, PathBuf}, str::FromStr, sync::Arc};

use crate::common::types::ParachainClient;

pub trait NodeBlock:
	BlockT<Extrinsic = OpaqueExtrinsic, Header = Self::BoundedHeader, Hash = DbHash> + DeserializeOwned
{
	type BoundedFromStrErr: Debug;
	type BoundedNumber: FromStr<Err = Self::BoundedFromStrErr> + BlockNumber;
	type BoundedHeader: HeaderT<Number = Self::BoundedNumber, Hash = DbHash> + Unpin;
}

impl<T> NodeBlock for T
where
	T: BlockT<Extrinsic = OpaqueExtrinsic, Hash = DbHash> + DeserializeOwned,
	<T as BlockT>::Header: Unpin,
	<NumberFor<T> as FromStr>::Err: Debug,
{
	type BoundedFromStrErr = <NumberFor<T> as FromStr>::Err;
	type BoundedNumber = NumberFor<T>;
	type BoundedHeader = <T as BlockT>::Header;
}

/// Convenience trait that defines the basic bounds for the `RuntimeApi` of a parachain node.
pub trait NodeRuntimeApi<Block: BlockT>:
	ApiExt<Block>
	+ Metadata<Block>
	+ SessionKeys<Block>
	+ BlockBuilder<Block>
	+ TaggedTransactionQueue<Block>
	+ OffchainWorkerApi<Block>
	+ CollectCollationInfo<Block>
	+ GetParachainInfo<Block>
	+ TransactionStorageApi<Block>
	+ RelayParentOffsetApi<Block>
	+ Sized
{
}

impl<T, Block: BlockT> NodeRuntimeApi<Block> for T where
	T: ApiExt<Block>
		+ Metadata<Block>
		+ SessionKeys<Block>
		+ BlockBuilder<Block>
		+ TaggedTransactionQueue<Block>
		+ OffchainWorkerApi<Block>
		+ RelayParentOffsetApi<Block>
		+ CollectCollationInfo<Block>
		+ GetParachainInfo<Block>
		+ TransactionStorageApi<Block>
{
}

/// Convenience trait that defines the basic bounds for the `ConstructRuntimeApi` of a parachain
/// node.
pub trait ConstructNodeRuntimeApi<Block: BlockT, C: CallApiAt<Block>>:
	ConstructRuntimeApi<Block, C, RuntimeApi = Self::BoundedRuntimeApi> + Send + Sync + 'static
{
	/// Basic bounds for the `RuntimeApi` of a parachain node.
	type BoundedRuntimeApi: NodeRuntimeApi<Block>;
}

impl<T, Block: BlockT, C: CallApiAt<Block>> ConstructNodeRuntimeApi<Block, C> for T
where
	T: ConstructRuntimeApi<Block, C> + Send + Sync + 'static,
	T::RuntimeApi: NodeRuntimeApi<Block>,
{
	type BoundedRuntimeApi = T::RuntimeApi;
}

/// Extra args that are passed when creating a new node spec.
pub struct NodeExtraArgs {
	/// The authoring policy to use.
	///
	/// Can be used to influence details of block production.
	pub authoring_policy: AuthoringPolicy,

	/// If set, each `PoV` build by the node will be exported to this folder.
	pub export_pov: Option<PathBuf>,

	/// The maximum percentage of the maximum PoV size that the collator can use.
	/// It will be removed once <https://github.com/paritytech/polkadot-sdk/issues/6020> is fixed.
	pub max_pov_percentage: Option<u32>,

	/// Statement store and network handler configuration.
	/// `None` disables the statement store.
	pub statement_store_config: Option<sc_statement_store::Config>,

	/// Parameters for storage monitoring.
	pub storage_monitor: sc_storage_monitor::StorageMonitorParams,
}

/// Hook called by the node startup machinery to let downstream binaries plug
/// in their own service tasks and RPC handlers without modifying this lib.
///
/// The default `polkadot-omni-node` binary uses [`NoNodeExtension`]. Custom
/// binaries (e.g. `polkadot-bulletin-parachain`) provide their own impl that
/// wires their protocol(s) into the node.
pub trait NodeExtension<Block, RuntimeApi>: Send + Sync + 'static
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	/// Called once after the task manager is built and the network has started,
	/// just before RPC server construction. Implementations can spawn extra
	/// service tasks tied to the typed `client`/`transaction_pool`.
	fn on_start(
		&self,
		_client: Arc<ParachainClient<Block, RuntimeApi>>,
		_transaction_pool: Arc<
			sc_transaction_pool::TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>,
		>,
		_task_manager: &sc_service::TaskManager,
		_database_path: Option<&Path>,
	) -> sc_service::error::Result<()> {
		Ok(())
	}

	/// Called once during RPC server construction. Returns a module to be
	/// merged into the node's RPC server alongside the lib's defaults.
	fn build_rpc_extension(
		&self,
		_client: Arc<ParachainClient<Block, RuntimeApi>>,
	) -> sc_service::error::Result<jsonrpsee::RpcModule<()>> {
		Ok(jsonrpsee::RpcModule::new(()))
	}
}

/// No-op extension used by `polkadot-omni-node` and any other consumer that
/// does not need to plug in additional service tasks or RPC modules.
#[derive(Clone, Default)]
pub struct NoNodeExtension;

impl<Block, RuntimeApi> NodeExtension<Block, RuntimeApi> for NoNodeExtension
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
}

/// Block type with a `u32` block number used by the lib's runtime resolver.
pub type BlockU32 = crate::common::types::Block<u32>;

/// Block type with a `u64` block number used by the lib's runtime resolver.
pub type BlockU64 = crate::common::types::Block<u64>;

/// Object-safe factory that yields a [`NodeExtension`] for whichever
/// `(Block, RuntimeApi)` combination the runtime resolver picks. Each method
/// returns `None` if no extension is wired for that combination, in which case
/// the lib treats the slot as a no-op (same behaviour as if [`NoNodeExtension`]
/// were used).
///
/// The lib enumerates the four combinations its `RuntimeResolver` produces
/// today (`u32`/`u64` block number × Aura `sr25519`/`ed25519` consensus). Each
/// factory method is called at most once during a single node startup. The
/// methods take `&self`; impls that need to hand out a unique extension value
/// should use interior mutability (e.g. `Mutex<Option<Box<dyn NodeExtension>>>`).
pub trait NodeExtensionFactory: Send + Sync + 'static {
	/// Extension for `(Block<u32>, aura_sr25519::RuntimeApi)`.
	fn create_aura_sr25519_u32(
		&self,
	) -> Option<
		Box<
			dyn NodeExtension<BlockU32, crate::fake_runtime_api::aura_sr25519::RuntimeApi>,
		>,
	> {
		None
	}

	/// Extension for `(Block<u32>, aura_ed25519::RuntimeApi)`.
	fn create_aura_ed25519_u32(
		&self,
	) -> Option<
		Box<
			dyn NodeExtension<BlockU32, crate::fake_runtime_api::aura_ed25519::RuntimeApi>,
		>,
	> {
		None
	}

	/// Extension for `(Block<u64>, aura_sr25519::RuntimeApi)`.
	fn create_aura_sr25519_u64(
		&self,
	) -> Option<
		Box<
			dyn NodeExtension<BlockU64, crate::fake_runtime_api::aura_sr25519::RuntimeApi>,
		>,
	> {
		None
	}

	/// Extension for `(Block<u64>, aura_ed25519::RuntimeApi)`.
	fn create_aura_ed25519_u64(
		&self,
	) -> Option<
		Box<
			dyn NodeExtension<BlockU64, crate::fake_runtime_api::aura_ed25519::RuntimeApi>,
		>,
	> {
		None
	}
}

/// Default factory that produces no extension for any of the supported
/// `(Block, RuntimeApi)` combinations. Used by `polkadot-omni-node`.
#[derive(Default)]
pub struct NoNodeExtensionFactory;

impl NodeExtensionFactory for NoNodeExtensionFactory {}
