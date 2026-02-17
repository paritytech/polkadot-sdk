// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Layered Substrate client implementation.

use crate::{Chain, ConnectionParams};

use caching::CachingClient;
use num_traits::Saturating;
use rpc::RpcClient;
use sp_version::RuntimeVersion;

pub mod caching;
pub mod rpc;

mod rpc_api;
mod subscription;
mod traits;

pub use subscription::{StreamDescription, Subscription, SubscriptionBroadcaster};
pub use traits::Client;

/// Type of RPC client with caching support.
pub type RpcWithCachingClient<C> = CachingClient<C, RpcClient<C>>;

/// Creates new RPC client with caching support.
pub async fn rpc_with_caching<C: Chain>(params: ConnectionParams) -> RpcWithCachingClient<C> {
	let rpc = rpc::RpcClient::<C>::new(params).await;
	caching::CachingClient::new(rpc).await
}

/// The difference between best block number and number of its ancestor, that is enough
/// for us to consider that ancestor an "ancient" block with dropped state.
///
/// The relay does not assume that it is connected to the archive node, so it always tries
/// to use the best available chain state. But sometimes it still may use state of some
/// old block. If the state of that block is already dropped, relay will see errors when
/// e.g. it tries to prove something.
///
/// By default Substrate-based nodes are storing state for last 256 blocks. We'll use
/// half of this value.
pub const ANCIENT_BLOCK_THRESHOLD: u32 = 128;

/// Returns `true` if we think that the state is already discarded for given block.
pub fn is_ancient_block<N: From<u32> + PartialOrd + Saturating>(block: N, best: N) -> bool {
	best.saturating_sub(block) >= N::from(ANCIENT_BLOCK_THRESHOLD)
}

/// Opaque GRANDPA authorities set.
pub type OpaqueGrandpaAuthoritiesSet = Vec<u8>;

/// A simple runtime version. It only includes the `spec_version` and `transaction_version`.
#[derive(Copy, Clone, Debug)]
pub struct SimpleRuntimeVersion {
	/// Version of the runtime specification.
	pub spec_version: u32,
	/// All existing dispatches are fully compatible when this number doesn't change.
	pub transaction_version: u32,
}

impl SimpleRuntimeVersion {
	/// Create a new instance of `SimpleRuntimeVersion` from a `RuntimeVersion`.
	pub const fn from_runtime_version(runtime_version: &RuntimeVersion) -> Self {
		Self {
			spec_version: runtime_version.spec_version,
			transaction_version: runtime_version.transaction_version,
		}
	}
}

/// Chain runtime version in client
#[derive(Copy, Clone, Debug)]
pub enum ChainRuntimeVersion {
	/// Auto query from chain.
	Auto,
	/// Custom runtime version, defined by user.
	Custom(SimpleRuntimeVersion),
}
