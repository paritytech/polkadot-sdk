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

//! Tools to interact with Substrate node using RPC methods.

#![warn(missing_docs)]

mod chain;
mod client;
mod error;
mod sync_header;
mod transaction_tracker;

pub mod calls;
pub mod guard;
pub mod metrics;
pub mod test_chain;

use std::time::Duration;

pub use crate::{
	chain::{
		AccountKeyPairOf, BlockWithJustification, CallOf, Chain, ChainWithBalances,
		ChainWithGrandpa, ChainWithMessages, ChainWithRewards, ChainWithRuntimeVersion,
		ChainWithTransactions, ChainWithUtilityPallet, FullRuntimeUtilityPallet,
		MockedRuntimeUtilityPallet, Parachain, RelayChain, SignParam, SignedBlockOf,
		TransactionStatusOf, UnsignedTransaction, UtilityPallet,
	},
	client::{
		is_ancient_block, rpc_with_caching as new, ChainRuntimeVersion, Client,
		OpaqueGrandpaAuthoritiesSet, RpcWithCachingClient, SimpleRuntimeVersion, StreamDescription,
		Subscription, ANCIENT_BLOCK_THRESHOLD,
	},
	error::{Error, Result},
	sync_header::SyncHeader,
	transaction_tracker::TransactionTracker,
};
pub use bp_runtime::{
	AccountIdOf, AccountPublicOf, BalanceOf, BlockNumberOf, Chain as ChainBase, HashOf, HeaderIdOf,
	HeaderOf, NonceOf, Parachain as ParachainBase, SignatureOf, TransactionEra, TransactionEraOf,
	UnderlyingChainProvider,
};

/// Where the client gets chain data from.
#[derive(Clone, Debug)]
pub enum Transport {
	/// WebSocket connection to a full node's RPC endpoint.
	Ws {
		/// Websocket endpoint URL.
		uri: String,
	},
	/// Embedded smoldot light client.
	LightClient {
		/// Path to the chain specification of the chain.
		chain_spec: std::path::PathBuf,
		/// Path to the chain specification of the relay chain, if the chain is a parachain.
		relay_chain_spec: Option<std::path::PathBuf>,
	},
}

impl std::fmt::Display for Transport {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			Transport::Ws { uri } => write!(f, "{uri}"),
			Transport::LightClient { chain_spec, relay_chain_spec: None } => {
				write!(f, "light client ({})", chain_spec.display())
			},
			Transport::LightClient { chain_spec, relay_chain_spec: Some(relay_chain_spec) } => {
				write!(
					f,
					"light client ({}, relay chain {})",
					chain_spec.display(),
					relay_chain_spec.display()
				)
			},
		}
	}
}

/// Substrate client connection params.
#[derive(Debug, Clone)]
pub struct ConnectionParams {
	/// Where to get chain data from.
	pub transport: Transport,
	/// Defined chain runtime version
	pub chain_runtime_version: ChainRuntimeVersion,
}

impl Default for ConnectionParams {
	fn default() -> Self {
		ConnectionParams {
			transport: Transport::Ws { uri: "ws://localhost:9944".into() },
			chain_runtime_version: ChainRuntimeVersion::Auto,
		}
	}
}

/// Returns stall timeout for relay loop.
///
/// Relay considers himself stalled if he has submitted transaction to the node, but it has not
/// been mined for this period.
pub fn transaction_stall_timeout(
	mortality_period: Option<u32>,
	average_block_interval: Duration,
	default_stall_timeout: Duration,
) -> Duration {
	// 1 extra block for transaction to reach the pool && 1 for relayer to awake after it is mined
	mortality_period
		.map(|mortality_period| average_block_interval.saturating_mul(mortality_period + 1 + 1))
		.unwrap_or(default_stall_timeout)
}
