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

//! Cumulus parachain collator primitives.

#![warn(missing_docs)]

pub(crate) mod aura;
pub mod chain_spec;
pub mod command;
pub mod rpc;
pub mod runtime;
pub mod spec;
pub mod types;

use cumulus_primitives_core::{CollectCollationInfo, GetCoreSelectorApi};
use sc_client_db::DbHash;
use serde::de::DeserializeOwned;
use sp_api::{ApiExt, CallApiAt, ConstructRuntimeApi, Metadata};
use sp_block_builder::BlockBuilder;
use sp_runtime::{
	traits::{Block as BlockT, BlockNumber, Header as HeaderT, NumberFor},
	OpaqueExtrinsic,
};
use sp_session::SessionKeys;
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use std::{fmt::Debug, path::PathBuf, str::FromStr};

pub trait NodeBlock:
	BlockT<Extrinsic = OpaqueExtrinsic, Header = Self::BoundedHeader, Hash = DbHash> + DeserializeOwned
{
	type BoundedFromStrErr: Debug;
	type BoundedNumber: FromStr<Err = Self::BoundedFromStrErr> + BlockNumber;
	type BoundedHeader: HeaderT<Number = Self::BoundedNumber> + Unpin;
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
	+ CollectCollationInfo<Block>
	+ GetCoreSelectorApi<Block>
	+ Sized
{
}

impl<T, Block: BlockT> NodeRuntimeApi<Block> for T where
	T: ApiExt<Block>
		+ Metadata<Block>
		+ SessionKeys<Block>
		+ BlockBuilder<Block>
		+ TaggedTransactionQueue<Block>
		+ GetCoreSelectorApi<Block>
		+ CollectCollationInfo<Block>
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
	pub use_slot_based_consensus: bool,

	/// If set, each `PoV` build by the node will be exported to this folder.
	pub export_pov: Option<PathBuf>,
}
