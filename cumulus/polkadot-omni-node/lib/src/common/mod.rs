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
/// Chain specification utilities for parachain nodes.
pub mod chain_spec;
/// Command handling and execution for parachain CLI operations.
pub mod command;
/// RPC (Remote Procedure Call) extensions for parachain nodes.
pub mod rpc;
/// Runtime configuration and integration.
pub mod runtime;
/// Chain specification types and helpers.
pub mod spec;
pub(crate) mod statement_store;
/// Common type definitions used throughout the parachain node.
pub mod types;

use crate::cli::AuthoringPolicy;

use cumulus_primitives_core::{
	CollectCollationInfo, GetParachainInfo, RelayParentOffsetApi, SchedulingV3EnabledApi,
};
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
use std::{fmt::Debug, path::PathBuf, str::FromStr};

/// A trait that defines the requirements for a block type used in a parachain node.
///
/// This trait ensures that the block type:
/// * Uses opaque extrinsics
/// * Has a header with bounded number type
/// * Uses database hash type
/// * Can be deserialized from any format
pub trait NodeBlock:
	BlockT<Extrinsic = OpaqueExtrinsic, Header = Self::BoundedHeader, Hash = DbHash>
	+ DeserializeOwned
	+ Unpin
{
	/// The error type that occurs when parsing a block number from a string.
	type BoundedFromStrErr: Debug;

	/// The block number type that can be parsed from a string.
	type BoundedNumber: FromStr<Err = Self::BoundedFromStrErr> + BlockNumber;

	/// The header type that contains the bounded number.
	type BoundedHeader: HeaderT<Number = Self::BoundedNumber, Hash = DbHash> + Unpin;
}

impl<T> NodeBlock for T
where
	T: BlockT<Extrinsic = OpaqueExtrinsic, Hash = DbHash> + DeserializeOwned + Unpin,
	<T as BlockT>::Header: Unpin,
	<NumberFor<T> as FromStr>::Err: Debug,
{
	type BoundedFromStrErr = <NumberFor<T> as FromStr>::Err;
	type BoundedNumber = NumberFor<T>;
	type BoundedHeader = <T as BlockT>::Header;
}

/// Convenience trait that defines the basic bounds for the `RuntimeApi` of a parachain node.
///
/// This trait combines multiple runtime API requirements into a single bound,
/// ensuring that the runtime implements all necessary APIs for parachain operation.
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
	+ sp_authority_discovery::AuthorityDiscoveryApi<Block>
	+ SchedulingV3EnabledApi<Block>
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
		+ sp_authority_discovery::AuthorityDiscoveryApi<Block>
		+ SchedulingV3EnabledApi<Block>
{
}

/// Convenience trait that defines the basic bounds for the `ConstructRuntimeApi` of a parachain
/// node.
///
/// This trait ensures that the runtime API constructor can produce a runtime API
/// that satisfies the [`NodeRuntimeApi`] bounds.
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

/// Extra arguments that are passed when creating a new node specification.
///
/// These arguments provide additional configuration options that affect
/// node behavior, authoring, and performance characteristics.
pub struct NodeExtraArgs {
	/// The authoring policy to use.
	///
	/// Can be used to influence details of block production, such as
	/// whether to produce blocks immediately or wait for certain conditions.
	pub authoring_policy: AuthoringPolicy,

	/// If set, each Proof-of-Validity (PoV) built by the node will be exported to this folder.
	///
	/// This is useful for debugging and analysis of block production.
	pub export_pov: Option<PathBuf>,

	/// The maximum percentage of the maximum PoV size that the collator can use.
	///
	/// It will be removed once <https://github.com/paritytech/polkadot-sdk/issues/6020> is fixed.
	pub max_pov_percentage: Option<u32>,

	/// Statement store and network handler configuration.
	/// `None` disables the statement store.
	pub statement_store_config: Option<sc_statement_store::Config>,

	/// Parameters for storage monitoring.
	///
	/// Controls how the node monitors and reports on storage usage.
	pub storage_monitor: sc_storage_monitor::StorageMonitorParams,

	/// Upper bound on collator reserved-peer slots.
	pub collator_reserved_slots: usize,

	/// HOP (Hand-Off Protocol) configuration parameters.
	/// `None` disables HOP.
	pub hop: Option<sc_hop::HopParams>,
}
