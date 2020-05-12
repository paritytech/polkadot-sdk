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

//! Substrate -> Ethereum synchronization.

use crate::ethereum_client::{self, EthereumConnectionParams, EthereumSigningParams};
use crate::ethereum_types::Address;
use crate::substrate_client::{self, SubstrateConnectionParams};
use crate::substrate_types::{
	GrandpaJustification, Hash, Header, Number, QueuedSubstrateHeader, SubstrateHeaderId, SubstrateHeadersSyncPipeline,
};
use crate::sync::{HeadersSyncParams, TargetTransactionMode};
use crate::sync_loop::{OwnedSourceFutureOutput, OwnedTargetFutureOutput, SourceClient, TargetClient};
use crate::sync_types::SourceHeader;
use futures::future::{ready, FutureExt, Ready};
use std::{collections::HashSet, future::Future, pin::Pin, time::Duration};

/// Interval at which we check new Substrate headers when we are synced/almost synced.
const SUBSTRATE_TICK_INTERVAL: Duration = Duration::from_secs(10);
/// Interval at which we check new Ethereum blocks.
const ETHEREUM_TICK_INTERVAL: Duration = Duration::from_secs(5);
/// Max Ethereum headers we want to have in all 'before-submitted' states.
const MAX_FUTURE_HEADERS_TO_DOWNLOAD: usize = 8;
/// Max Ethereum headers count we want to have in 'submitted' state.
const MAX_SUBMITTED_HEADERS: usize = 4;
/// Max depth of in-memory headers in all states. Past this depth they will be forgotten (pruned).
const PRUNE_DEPTH: u32 = 256;

/// Substrate synchronization parameters.
#[derive(Debug)]
pub struct SubstrateSyncParams {
	/// Ethereum connection params.
	pub eth: EthereumConnectionParams,
	/// Ethereum signing params.
	pub eth_sign: EthereumSigningParams,
	/// Ethereum bridge contract address.
	pub eth_contract_address: Address,
	/// Substrate connection params.
	pub sub: SubstrateConnectionParams,
	/// Synchronization parameters.
	pub sync_params: HeadersSyncParams,
}

impl Default for SubstrateSyncParams {
	fn default() -> Self {
		SubstrateSyncParams {
			eth: Default::default(),
			eth_sign: Default::default(),
			// the address 0x731a10897d267e19b34503ad902d0a29173ba4b1 is the address
			// of the contract that is deployed by default signer and 0 nonce
			eth_contract_address: "731a10897d267e19b34503ad902d0a29173ba4b1"
				.parse()
				.expect("address is hardcoded, thus valid; qed"),
			sub: Default::default(),
			sync_params: HeadersSyncParams {
				max_future_headers_to_download: MAX_FUTURE_HEADERS_TO_DOWNLOAD,
				max_headers_in_submitted_status: MAX_SUBMITTED_HEADERS,
				// since we always have single Substrate header in separate Ethereum transaction,
				// all max_**_in_single_submit aren't important here
				max_headers_in_single_submit: 4,
				max_headers_size_in_single_submit: std::usize::MAX,
				prune_depth: PRUNE_DEPTH,
				target_tx_mode: TargetTransactionMode::Signed,
			},
		}
	}
}

/// Substrate client as headers source.
struct SubstrateHeadersSource {
	/// Substrate node client.
	client: substrate_client::Client,
}

type SubstrateFutureOutput<T> = OwnedSourceFutureOutput<SubstrateHeadersSource, SubstrateHeadersSyncPipeline, T>;

impl SourceClient<SubstrateHeadersSyncPipeline> for SubstrateHeadersSource {
	type Error = substrate_client::Error;
	type BestBlockNumberFuture = Pin<Box<dyn Future<Output = SubstrateFutureOutput<Number>>>>;
	type HeaderByHashFuture = Pin<Box<dyn Future<Output = SubstrateFutureOutput<Header>>>>;
	type HeaderByNumberFuture = Pin<Box<dyn Future<Output = SubstrateFutureOutput<Header>>>>;
	type HeaderExtraFuture = Ready<SubstrateFutureOutput<(SubstrateHeaderId, ())>>;
	type HeaderCompletionFuture =
		Pin<Box<dyn Future<Output = SubstrateFutureOutput<(SubstrateHeaderId, Option<GrandpaJustification>)>>>>;

	fn best_block_number(self) -> Self::BestBlockNumberFuture {
		substrate_client::best_header(self.client)
			.map(|(client, result)| (SubstrateHeadersSource { client }, result.map(|header| header.number)))
			.boxed()
	}

	fn header_by_hash(self, hash: Hash) -> Self::HeaderByHashFuture {
		substrate_client::header_by_hash(self.client, hash)
			.map(|(client, result)| (SubstrateHeadersSource { client }, result))
			.boxed()
	}

	fn header_by_number(self, number: Number) -> Self::HeaderByNumberFuture {
		substrate_client::header_by_number(self.client, number)
			.map(|(client, result)| (SubstrateHeadersSource { client }, result))
			.boxed()
	}

	fn header_extra(self, id: SubstrateHeaderId, _header: &Header) -> Self::HeaderExtraFuture {
		ready((self, Ok((id, ()))))
	}

	fn header_completion(self, id: SubstrateHeaderId) -> Self::HeaderCompletionFuture {
		substrate_client::grandpa_justification(self.client, id)
			.map(|(client, result)| (SubstrateHeadersSource { client }, result))
			.boxed()
	}
}

/// Ethereum client as Substrate headers target.
struct EthereumHeadersTarget {
	/// Ethereum node client.
	client: ethereum_client::Client,
	/// Bridge contract address.
	contract: Address,
	/// Ethereum signing params.
	sign_params: EthereumSigningParams,
}

type EthereumFutureOutput<T> = OwnedTargetFutureOutput<EthereumHeadersTarget, SubstrateHeadersSyncPipeline, T>;

impl TargetClient<SubstrateHeadersSyncPipeline> for EthereumHeadersTarget {
	type Error = ethereum_client::Error;
	type BestHeaderIdFuture = Pin<Box<dyn Future<Output = EthereumFutureOutput<SubstrateHeaderId>>>>;
	type IsKnownHeaderFuture = Pin<Box<dyn Future<Output = EthereumFutureOutput<(SubstrateHeaderId, bool)>>>>;
	type RequiresExtraFuture = Ready<EthereumFutureOutput<(SubstrateHeaderId, bool)>>;
	type SubmitHeadersFuture = Pin<Box<dyn Future<Output = EthereumFutureOutput<Vec<SubstrateHeaderId>>>>>;
	type IncompleteHeadersFuture = Pin<Box<dyn Future<Output = EthereumFutureOutput<HashSet<SubstrateHeaderId>>>>>;
	type CompleteHeadersFuture = Pin<Box<dyn Future<Output = EthereumFutureOutput<SubstrateHeaderId>>>>;

	fn best_header_id(self) -> Self::BestHeaderIdFuture {
		let (contract, sign_params) = (self.contract, self.sign_params);
		ethereum_client::best_substrate_block(self.client, contract)
			.map(move |(client, result)| {
				(
					EthereumHeadersTarget {
						client,
						contract,
						sign_params,
					},
					result,
				)
			})
			.boxed()
	}

	fn is_known_header(self, id: SubstrateHeaderId) -> Self::IsKnownHeaderFuture {
		let (contract, sign_params) = (self.contract, self.sign_params);
		ethereum_client::substrate_header_known(self.client, contract, id)
			.map(move |(client, result)| {
				(
					EthereumHeadersTarget {
						client,
						contract,
						sign_params,
					},
					result,
				)
			})
			.boxed()
	}

	fn requires_extra(self, header: &QueuedSubstrateHeader) -> Self::RequiresExtraFuture {
		ready((self, Ok((header.header().id(), false))))
	}

	fn submit_headers(self, headers: Vec<QueuedSubstrateHeader>) -> Self::SubmitHeadersFuture {
		let (contract, sign_params) = (self.contract, self.sign_params);
		ethereum_client::submit_substrate_headers(self.client, sign_params.clone(), contract, headers)
			.map(move |(client, result)| {
				(
					EthereumHeadersTarget {
						client,
						contract,
						sign_params,
					},
					result,
				)
			})
			.boxed()
	}

	fn incomplete_headers_ids(self) -> Self::IncompleteHeadersFuture {
		let (contract, sign_params) = (self.contract, self.sign_params);
		ethereum_client::incomplete_substrate_headers(self.client, contract)
			.map(move |(client, result)| {
				(
					EthereumHeadersTarget {
						client,
						contract,
						sign_params,
					},
					result,
				)
			})
			.boxed()
	}

	fn complete_header(self, id: SubstrateHeaderId, completion: GrandpaJustification) -> Self::CompleteHeadersFuture {
		let (contract, sign_params) = (self.contract, self.sign_params);
		ethereum_client::complete_substrate_header(self.client, sign_params.clone(), contract, id, completion)
			.map(move |(client, result)| {
				(
					EthereumHeadersTarget {
						client,
						contract,
						sign_params,
					},
					result,
				)
			})
			.boxed()
	}
}

/// Run Substrate headers synchronization.
pub fn run(params: SubstrateSyncParams) {
	let eth_client = ethereum_client::client(params.eth);
	let sub_client = substrate_client::client(params.sub);

	crate::sync_loop::run(
		SubstrateHeadersSource { client: sub_client },
		SUBSTRATE_TICK_INTERVAL,
		EthereumHeadersTarget {
			client: eth_client,
			contract: params.eth_contract_address,
			sign_params: params.eth_sign,
		},
		ETHEREUM_TICK_INTERVAL,
		params.sync_params,
	);
}
