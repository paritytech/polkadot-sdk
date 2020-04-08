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

use crate::ethereum_client;
use crate::ethereum_types::{EthereumHeaderId, EthereumHeadersSyncPipeline, Header, QueuedEthereumHeader, Receipt};
use crate::substrate_client;
use crate::sync::{HeadersSyncParams, TargetTransactionMode};
use crate::sync_loop::{SourceClient, TargetClient};
use futures::future::FutureExt;
use std::{future::Future, pin::Pin};
pub use web3::types::H256;

/// Interval (in ms) at which we check new Ethereum headers when we are synced/almost synced.
const ETHEREUM_TICK_INTERVAL_MS: u64 = 10_000;
/// Interval (in ms) at which we check new Substrate blocks.
const SUBSTRATE_TICK_INTERVAL_MS: u64 = 5_000;

/// Ethereum synchronization parameters.
pub struct EthereumSyncParams {
	/// Ethereum RPC host.
	pub eth_host: String,
	/// Ethereum RPC port.
	pub eth_port: u16,
	/// Substrate RPC host.
	pub sub_host: String,
	/// Substrate RPC port.
	pub sub_port: u16,
	/// Substrate transactions signer.
	pub sub_signer: sp_core::sr25519::Pair,
	/// Synchronization parameters.
	pub sync_params: HeadersSyncParams,
}

impl std::fmt::Debug for EthereumSyncParams {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct("EthereumSyncParams")
			.field("eth_host", &self.eth_host)
			.field("eth_port", &self.eth_port)
			.field("sub_host", &self.sub_port)
			.field("sub_port", &self.sub_port)
			.field("sync_params", &self.sync_params)
			.finish()
	}
}

impl Default for EthereumSyncParams {
	fn default() -> Self {
		EthereumSyncParams {
			eth_host: "localhost".into(),
			eth_port: 8545,
			sub_host: "localhost".into(),
			sub_port: 9933,
			sub_signer: sp_keyring::AccountKeyring::Alice.pair(),
			sync_params: Default::default(),
		}
	}
}

/// Ethereum client as headers source.
struct EthereumHeadersSource {
	/// Ethereum node client.
	client: ethereum_client::Client,
}

impl SourceClient<EthereumHeadersSyncPipeline> for EthereumHeadersSource {
	type Error = ethereum_client::Error;
	type BestBlockNumberFuture = Pin<Box<dyn Future<Output = (Self, Result<u64, Self::Error>)>>>;
	type HeaderByHashFuture = Pin<Box<dyn Future<Output = (Self, Result<Header, Self::Error>)>>>;
	type HeaderByNumberFuture = Pin<Box<dyn Future<Output = (Self, Result<Header, Self::Error>)>>>;
	type HeaderExtraFuture =
		Pin<Box<dyn Future<Output = (Self, Result<(EthereumHeaderId, Vec<Receipt>), Self::Error>)>>>;

	fn best_block_number(self) -> Self::BestBlockNumberFuture {
		ethereum_client::best_block_number(self.client)
			.map(|(client, result)| (EthereumHeadersSource { client }, result))
			.boxed()
	}

	fn header_by_hash(self, hash: H256) -> Self::HeaderByHashFuture {
		ethereum_client::header_by_hash(self.client, hash)
			.map(|(client, result)| (EthereumHeadersSource { client }, result))
			.boxed()
	}

	fn header_by_number(self, number: u64) -> Self::HeaderByNumberFuture {
		ethereum_client::header_by_number(self.client, number)
			.map(|(client, result)| (EthereumHeadersSource { client }, result))
			.boxed()
	}

	fn header_extra(self, id: EthereumHeaderId, header: &Header) -> Self::HeaderExtraFuture {
		ethereum_client::transactions_receipts(self.client, id, header.transactions.clone())
			.map(|(client, result)| (EthereumHeadersSource { client }, result))
			.boxed()
	}
}

/// Substrate client as Ethereum headers target.
struct SubstrateHeadersTarget {
	/// Substrate node client.
	client: substrate_client::Client,
	/// Substrate transactions signer.
	signer: sp_core::sr25519::Pair,
	/// Whether we want to submit signed (true), or unsigned (false) transactions.
	sign_transactions: bool,
}

impl TargetClient<EthereumHeadersSyncPipeline> for SubstrateHeadersTarget {
	type Error = substrate_client::Error;
	type BestHeaderIdFuture = Pin<Box<dyn Future<Output = (Self, Result<EthereumHeaderId, Self::Error>)>>>;
	type IsKnownHeaderFuture = Pin<Box<dyn Future<Output = (Self, Result<(EthereumHeaderId, bool), Self::Error>)>>>;
	type RequiresExtraFuture = Pin<Box<dyn Future<Output = (Self, Result<(EthereumHeaderId, bool), Self::Error>)>>>;
	type SubmitHeadersFuture = Pin<Box<dyn Future<Output = (Self, Result<Vec<EthereumHeaderId>, Self::Error>)>>>;

	fn best_header_id(self) -> Self::BestHeaderIdFuture {
		let (signer, sign_transactions) = (self.signer, self.sign_transactions);
		substrate_client::best_ethereum_block(self.client)
			.map(move |(client, result)| {
				(
					SubstrateHeadersTarget {
						client,
						signer,
						sign_transactions,
					},
					result,
				)
			})
			.boxed()
	}

	fn is_known_header(self, id: EthereumHeaderId) -> Self::IsKnownHeaderFuture {
		let (signer, sign_transactions) = (self.signer, self.sign_transactions);
		substrate_client::ethereum_header_known(self.client, id)
			.map(move |(client, result)| {
				(
					SubstrateHeadersTarget {
						client,
						signer,
						sign_transactions,
					},
					result,
				)
			})
			.boxed()
	}

	fn requires_extra(self, header: &QueuedEthereumHeader) -> Self::RequiresExtraFuture {
		// we can minimize number of receipts_check calls by checking header
		// logs bloom here, but it may give us false positives (when authorities
		// source is contract, we never need any logs)
		let (signer, sign_transactions) = (self.signer, self.sign_transactions);
		substrate_client::ethereum_receipts_required(self.client, header.clone())
			.map(move |(client, result)| {
				(
					SubstrateHeadersTarget {
						client,
						signer,
						sign_transactions,
					},
					result,
				)
			})
			.boxed()
	}

	fn submit_headers(self, headers: Vec<QueuedEthereumHeader>) -> Self::SubmitHeadersFuture {
		let (signer, sign_transactions) = (self.signer, self.sign_transactions);
		substrate_client::submit_ethereum_headers(self.client, signer.clone(), headers, sign_transactions)
			.map(move |(client, result)| {
				(
					SubstrateHeadersTarget {
						client,
						signer,
						sign_transactions,
					},
					result.map(|(_, submitted_headers)| submitted_headers),
				)
			})
			.boxed()
	}
}

/// Run Ethereum headers synchronization.
pub fn run(params: EthereumSyncParams) {
	let eth_uri = format!("http://{}:{}", params.eth_host, params.eth_port);
	let eth_client = ethereum_client::client(&eth_uri);

	let sub_uri = format!("http://{}:{}", params.sub_host, params.sub_port);
	let sub_client = substrate_client::client(&sub_uri);
	let sub_signer = params.sub_signer;
	let sign_sub_transactions = match params.sync_params.target_tx_mode {
		TargetTransactionMode::Signed | TargetTransactionMode::Backup => true,
		TargetTransactionMode::Unsigned => false,
	};

	crate::sync_loop::run(
		EthereumHeadersSource { client: eth_client },
		ETHEREUM_TICK_INTERVAL_MS,
		SubstrateHeadersTarget {
			client: sub_client,
			signer: sub_signer,
			sign_transactions: sign_sub_transactions,
		},
		SUBSTRATE_TICK_INTERVAL_MS,
		params.sync_params,
	);
}
