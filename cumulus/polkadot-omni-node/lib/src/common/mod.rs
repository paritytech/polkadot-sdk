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

/// Plug-in hook for downstream binaries to add service tasks and RPC handlers.
pub trait NodeExtension<Block, RuntimeApi>: Send + Sync + 'static
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	/// Called after the task manager is built, before RPC server construction.
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

	/// Returns an RPC module to be merged with the node's defaults.
	fn build_rpc_extension(
		&self,
		_client: Arc<ParachainClient<Block, RuntimeApi>>,
	) -> sc_service::error::Result<jsonrpsee::RpcModule<()>> {
		Ok(jsonrpsee::RpcModule::new(()))
	}
}

/// Block with `u32` block number.
pub type BlockU32 = crate::common::types::Block<u32>;

/// Block with `u64` block number.
pub type BlockU64 = crate::common::types::Block<u64>;

/// Names the four fake `RuntimeApi` types the lib's dispatch will use when
/// instantiating `AuraNode<...>` and the matching `NodeExtensions` slots.
///
/// The lib provides [`DefaultRuntimeApiBundle`] which wires its own
/// `fake_runtime_api` types into all four slots. Downstream binaries that
/// need a different fake (e.g. one that implements an extra runtime API
/// trait so the binary's `NodeExtension` impl can be parameterized over it)
/// supply their own bundle and pass it through `RunConfig` / `run_with_matches`.
pub trait RuntimeApiBundle: Send + Sync + 'static {
	/// Fake `RuntimeApi` for `(Block<u32>, sr25519)`.
	type AuraSr25519U32: ConstructNodeRuntimeApi<
			BlockU32,
			crate::common::types::ParachainClient<BlockU32, Self::AuraSr25519U32>,
		>;
	/// Fake `RuntimeApi` for `(Block<u32>, ed25519)`.
	type AuraEd25519U32: ConstructNodeRuntimeApi<
			BlockU32,
			crate::common::types::ParachainClient<BlockU32, Self::AuraEd25519U32>,
		>;
	/// Fake `RuntimeApi` for `(Block<u64>, sr25519)`.
	type AuraSr25519U64: ConstructNodeRuntimeApi<
			BlockU64,
			crate::common::types::ParachainClient<BlockU64, Self::AuraSr25519U64>,
		>;
	/// Fake `RuntimeApi` for `(Block<u64>, ed25519)`.
	type AuraEd25519U64: ConstructNodeRuntimeApi<
			BlockU64,
			crate::common::types::ParachainClient<BlockU64, Self::AuraEd25519U64>,
		>;
}

/// Default bundle used by `polkadot-omni-node`. Wires the lib's own
/// `fake_runtime_api` types into all four slots.
#[derive(Default)]
pub struct DefaultRuntimeApiBundle;

impl RuntimeApiBundle for DefaultRuntimeApiBundle {
	type AuraSr25519U32 = crate::fake_runtime_api::aura_sr25519::RuntimeApi;
	type AuraEd25519U32 = crate::fake_runtime_api::aura_ed25519::RuntimeApi;
	type AuraSr25519U64 = crate::fake_runtime_api::aura_sr25519::RuntimeApi;
	type AuraEd25519U64 = crate::fake_runtime_api::aura_ed25519::RuntimeApi;
}

/// Bundle of [`NodeExtension`]s for the two Aura `RuntimeApi` variants
/// (sr25519, ed25519) at a given `Block` type. Only the variant matching the
/// resolved `AuraConsensusId` is consumed; the other is ignored.
pub struct AuraExtensions<Block, Sr25519Api, Ed25519Api>
where
	Block: NodeBlock,
	Sr25519Api: ConstructNodeRuntimeApi<Block, ParachainClient<Block, Sr25519Api>>,
	Ed25519Api: ConstructNodeRuntimeApi<Block, ParachainClient<Block, Ed25519Api>>,
{
	/// Extensions for the sr25519 variant.
	pub sr25519: Vec<Box<dyn NodeExtension<Block, Sr25519Api>>>,
	/// Extensions for the ed25519 variant.
	pub ed25519: Vec<Box<dyn NodeExtension<Block, Ed25519Api>>>,
}

impl<Block, Sr25519Api, Ed25519Api> Default for AuraExtensions<Block, Sr25519Api, Ed25519Api>
where
	Block: NodeBlock,
	Sr25519Api: ConstructNodeRuntimeApi<Block, ParachainClient<Block, Sr25519Api>>,
	Ed25519Api: ConstructNodeRuntimeApi<Block, ParachainClient<Block, Ed25519Api>>,
{
	fn default() -> Self {
		Self { sr25519: Vec::new(), ed25519: Vec::new() }
	}
}

/// Container for [`NodeExtension`]s installed on a node, keyed by the
/// `(Block, RuntimeApi)` combinations the runtime resolver picks. Only the
/// combo matching the resolved runtime is consumed; the others are ignored.
///
/// Parameterized over a [`RuntimeApiBundle`] (default
/// [`DefaultRuntimeApiBundle`]) which names the four fake `RuntimeApi` types
/// the slots are typed against. Downstream binaries that need their fake to
/// implement extra runtime API traits supply their own bundle.
pub struct NodeExtensions<Bundle: RuntimeApiBundle = DefaultRuntimeApiBundle> {
	/// Aura extensions for `Block<u32>`.
	pub aura_u32: AuraExtensions<BlockU32, Bundle::AuraSr25519U32, Bundle::AuraEd25519U32>,
	/// Aura extensions for `Block<u64>`.
	pub aura_u64: AuraExtensions<BlockU64, Bundle::AuraSr25519U64, Bundle::AuraEd25519U64>,
}

impl<Bundle: RuntimeApiBundle> Default for NodeExtensions<Bundle> {
	fn default() -> Self {
		Self { aura_u32: AuraExtensions::default(), aura_u64: AuraExtensions::default() }
	}
}
