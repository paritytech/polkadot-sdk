// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Ethereum PoA -> Substrate synchronization.

use crate::ethereum_client::{EthereumConnectionParams, EthereumHighLevelRpc, EthereumRpcClient};
use crate::ethereum_types::{EthereumHeaderId, EthereumHeadersSyncPipeline, Header, QueuedEthereumHeader, Receipt};
use crate::rpc::{EthereumRpc, SubstrateRpc};
use crate::rpc_errors::RpcError;
use crate::substrate_client::{
	SubmitEthereumHeaders, SubstrateConnectionParams, SubstrateRpcClient, SubstrateSigningParams,
};
use crate::substrate_types::into_substrate_ethereum_header;
use crate::sync::{HeadersSyncParams, TargetTransactionMode};
use crate::sync_loop::{SourceClient, TargetClient};
use crate::sync_types::{SourceHeader, SubmittedHeaders};

use async_trait::async_trait;
use web3::types::H256;

use std::{collections::HashSet, time::Duration};

/// Interval at which we check new Ethereum headers when we are synced/almost synced.
const ETHEREUM_TICK_INTERVAL: Duration = Duration::from_secs(10);
/// Interval at which we check new Substrate blocks.
const SUBSTRATE_TICK_INTERVAL: Duration = Duration::from_secs(5);
/// Max number of headers in single submit transaction.
const MAX_HEADERS_IN_SINGLE_SUBMIT: usize = 32;
/// Max total size of headers in single submit transaction. This only affects signed
/// submissions, when several headers are submitted at once. 4096 is the maximal **expected**
/// size of the Ethereum header + transactions receipts (if they're required).
const MAX_HEADERS_SIZE_IN_SINGLE_SUBMIT: usize = MAX_HEADERS_IN_SINGLE_SUBMIT * 4096;
/// Max Ethereum headers we want to have in all 'before-submitted' states.
const MAX_FUTURE_HEADERS_TO_DOWNLOAD: usize = 128;
/// Max Ethereum headers count we want to have in 'submitted' state.
const MAX_SUBMITTED_HEADERS: usize = 128;
/// Max depth of in-memory headers in all states. Past this depth they will be forgotten (pruned).
const PRUNE_DEPTH: u32 = 4096;

/// Ethereum synchronization parameters.
#[derive(Clone, Debug)]
pub struct EthereumSyncParams {
	/// Ethereum connection params.
	pub eth: EthereumConnectionParams,
	/// Substrate connection params.
	pub sub: SubstrateConnectionParams,
	/// Substrate signing params.
	pub sub_sign: SubstrateSigningParams,
	/// Synchronization parameters.
	pub sync_params: HeadersSyncParams,
}

impl Default for EthereumSyncParams {
	fn default() -> Self {
		EthereumSyncParams {
			eth: Default::default(),
			sub: Default::default(),
			sub_sign: Default::default(),
			sync_params: HeadersSyncParams {
				max_future_headers_to_download: MAX_FUTURE_HEADERS_TO_DOWNLOAD,
				max_headers_in_submitted_status: MAX_SUBMITTED_HEADERS,
				max_headers_in_single_submit: MAX_HEADERS_IN_SINGLE_SUBMIT,
				max_headers_size_in_single_submit: MAX_HEADERS_SIZE_IN_SINGLE_SUBMIT,
				prune_depth: PRUNE_DEPTH,
				target_tx_mode: TargetTransactionMode::Signed,
			},
		}
	}
}

/// Ethereum client as headers source.
struct EthereumHeadersSource {
	/// Ethereum node client.
	client: EthereumRpcClient,
}

impl EthereumHeadersSource {
	fn new(client: EthereumRpcClient) -> Self {
		Self { client }
	}
}

#[async_trait]
impl SourceClient<EthereumHeadersSyncPipeline> for EthereumHeadersSource {
	type Error = RpcError;

	async fn best_block_number(&self) -> Result<u64, Self::Error> {
		self.client.best_block_number().await
	}

	async fn header_by_hash(&self, hash: H256) -> Result<Header, Self::Error> {
		self.client.header_by_hash(hash).await
	}

	async fn header_by_number(&self, number: u64) -> Result<Header, Self::Error> {
		self.client.header_by_number(number).await
	}

	async fn header_completion(&self, id: EthereumHeaderId) -> Result<(EthereumHeaderId, Option<()>), Self::Error> {
		Ok((id, None))
	}

	async fn header_extra(
		&self,
		id: EthereumHeaderId,
		header: QueuedEthereumHeader,
	) -> Result<(EthereumHeaderId, Vec<Receipt>), Self::Error> {
		self.client
			.transaction_receipts(id, header.header().transactions.clone())
			.await
	}
}

struct SubstrateHeadersTarget {
	/// Substrate node client.
	client: SubstrateRpcClient,
	/// Whether we want to submit signed (true), or unsigned (false) transactions.
	sign_transactions: bool,
	/// Substrate signing params.
	sign_params: SubstrateSigningParams,
}

impl SubstrateHeadersTarget {
	fn new(client: SubstrateRpcClient, sign_transactions: bool, sign_params: SubstrateSigningParams) -> Self {
		Self {
			client,
			sign_transactions,
			sign_params,
		}
	}
}

#[async_trait]
impl TargetClient<EthereumHeadersSyncPipeline> for SubstrateHeadersTarget {
	type Error = RpcError;

	async fn best_header_id(&self) -> Result<EthereumHeaderId, Self::Error> {
		self.client.best_ethereum_block().await
	}

	async fn is_known_header(&self, id: EthereumHeaderId) -> Result<(EthereumHeaderId, bool), Self::Error> {
		Ok((id, self.client.ethereum_header_known(id).await?))
	}

	async fn submit_headers(
		&self,
		headers: Vec<QueuedEthereumHeader>,
	) -> SubmittedHeaders<EthereumHeaderId, Self::Error> {
		let (sign_params, sign_transactions) = (self.sign_params.clone(), self.sign_transactions.clone());
		self.client
			.submit_ethereum_headers(sign_params, headers, sign_transactions)
			.await
	}

	async fn incomplete_headers_ids(&self) -> Result<HashSet<EthereumHeaderId>, Self::Error> {
		Ok(HashSet::new())
	}

	async fn complete_header(&self, id: EthereumHeaderId, _completion: ()) -> Result<EthereumHeaderId, Self::Error> {
		Ok(id)
	}

	async fn requires_extra(&self, header: QueuedEthereumHeader) -> Result<(EthereumHeaderId, bool), Self::Error> {
		// we can minimize number of receipts_check calls by checking header
		// logs bloom here, but it may give us false positives (when authorities
		// source is contract, we never need any logs)
		let id = header.header().id();
		let sub_eth_header = into_substrate_ethereum_header(header.header());
		Ok((id, self.client.ethereum_receipts_required(sub_eth_header).await?))
	}
}

/// Run Ethereum headers synchronization.
pub fn run(params: EthereumSyncParams) -> Result<(), RpcError> {
	let sub_params = params.clone();

	let eth_client = EthereumRpcClient::new(params.eth);
	let sub_client = async_std::task::block_on(async { SubstrateRpcClient::new(sub_params.sub).await })?;

	let sign_sub_transactions = match params.sync_params.target_tx_mode {
		TargetTransactionMode::Signed | TargetTransactionMode::Backup => true,
		TargetTransactionMode::Unsigned => false,
	};

	let source = EthereumHeadersSource::new(eth_client);
	let target = SubstrateHeadersTarget::new(sub_client, sign_sub_transactions, params.sub_sign);

	crate::sync_loop::run(
		source,
		ETHEREUM_TICK_INTERVAL,
		target,
		SUBSTRATE_TICK_INTERVAL,
		params.sync_params,
		futures::future::pending(),
	);

	Ok(())
}
