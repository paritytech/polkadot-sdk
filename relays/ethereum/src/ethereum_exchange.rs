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

//! Relaying proofs of PoA -> Substrate exchange transactions.

use crate::ethereum_client::{EthereumConnectionParams, EthereumRpcClient};
use crate::ethereum_types::{
	EthereumHeaderId, Transaction as EthereumTransaction, TransactionHash as EthereumTransactionHash, H256,
};
use crate::exchange::{relay_single_transaction_proof, SourceClient, TargetClient, TransactionProofPipeline};
use crate::rpc::{EthereumRpc, SubstrateRpc};
use crate::rpc_errors::{EthereumNodeError, RpcError};
use crate::substrate_client::{
	SubmitEthereumExchangeTransactionProof, SubstrateConnectionParams, SubstrateRpcClient, SubstrateSigningParams,
};
use crate::sync_types::HeaderId;

use async_trait::async_trait;
use bridge_node_runtime::exchange::EthereumTransactionInclusionProof;
use std::time::Duration;

/// Interval at which we ask Ethereum node for updates.
const ETHEREUM_TICK_INTERVAL: Duration = Duration::from_secs(10);
/// Interval at which we ask Substrate node for updates.
const SUBSTRATE_TICK_INTERVAL: Duration = Duration::from_secs(5);

/// PoA exchange transaction relay params.
#[derive(Debug, Default)]
pub struct EthereumExchangeParams {
	/// Ethereum connection params.
	pub eth: EthereumConnectionParams,
	/// Hash of the Ethereum transaction to relay.
	pub eth_tx_hash: EthereumTransactionHash,
	/// Substrate connection params.
	pub sub: SubstrateConnectionParams,
	/// Substrate signing params.
	pub sub_sign: SubstrateSigningParams,
}

/// Ethereum to Substrate exchange pipeline.
struct EthereumToSubstrateExchange;

impl TransactionProofPipeline for EthereumToSubstrateExchange {
	const SOURCE_NAME: &'static str = "Ethereum";
	const TARGET_NAME: &'static str = "Substrate";

	type BlockHash = H256;
	type BlockNumber = u64;
	type TransactionHash = EthereumTransactionHash;
	type Transaction = EthereumTransaction;
	type TransactionProof = EthereumTransactionInclusionProof;
}

/// Ethereum node as transactions proof source.
struct EthereumTransactionsSource {
	client: EthereumRpcClient,
}

#[async_trait]
impl SourceClient<EthereumToSubstrateExchange> for EthereumTransactionsSource {
	type Error = RpcError;

	async fn tick(&self) {
		async_std::task::sleep(ETHEREUM_TICK_INTERVAL).await;
	}

	async fn transaction(
		&self,
		hash: &EthereumTransactionHash,
	) -> Result<Option<(EthereumHeaderId, EthereumTransaction)>, Self::Error> {
		let eth_tx = match self.client.transaction_by_hash(*hash).await? {
			Some(eth_tx) => eth_tx,
			None => return Ok(None),
		};

		// we need transaction to be mined => check if it is included in the block
		let eth_header_id = match (eth_tx.block_number, eth_tx.block_hash) {
			(Some(block_number), Some(block_hash)) => HeaderId(block_number.as_u64(), block_hash),
			_ => return Ok(None),
		};

		Ok(Some((eth_header_id, eth_tx)))
	}

	async fn transaction_proof(
		&self,
		eth_header_id: &EthereumHeaderId,
		eth_tx: EthereumTransaction,
	) -> Result<EthereumTransactionInclusionProof, Self::Error> {
		const TRANSACTION_HAS_RAW_FIELD_PROOF: &str = "RPC level checks that transactions from Ethereum\
			node are having `raw` field; qed";

		let eth_header = self.client.header_by_hash_with_transactions(eth_header_id.1).await?;
		let eth_relay_tx_hash = eth_tx.hash;
		let mut eth_relay_tx = Some(eth_tx);
		let mut eth_relay_tx_index = None;
		let mut transaction_proof = Vec::with_capacity(eth_header.transactions.len());
		for (index, eth_tx) in eth_header.transactions.into_iter().enumerate() {
			if eth_tx.hash != eth_relay_tx_hash {
				let eth_raw_tx = eth_tx.raw.expect(TRANSACTION_HAS_RAW_FIELD_PROOF);
				transaction_proof.push(eth_raw_tx.0);
			} else {
				let eth_raw_relay_tx = match eth_relay_tx.take() {
					Some(eth_relay_tx) => eth_relay_tx.raw.expect(TRANSACTION_HAS_RAW_FIELD_PROOF),
					None => {
						return Err(
							EthereumNodeError::DuplicateBlockTransaction(*eth_header_id, eth_relay_tx_hash).into(),
						)
					}
				};
				eth_relay_tx_index = Some(index as u64);
				transaction_proof.push(eth_raw_relay_tx.0);
			}
		}

		Ok(EthereumTransactionInclusionProof {
			block: eth_header_id.1,
			index: eth_relay_tx_index.ok_or_else(|| {
				RpcError::from(EthereumNodeError::BlockMissingTransaction(
					*eth_header_id,
					eth_relay_tx_hash,
				))
			})?,
			proof: transaction_proof,
		})
	}
}

/// Substrate node as transactions proof target.
struct SubstrateTransactionsTarget {
	client: SubstrateRpcClient,
	sign_params: SubstrateSigningParams,
}

#[async_trait]
impl TargetClient<EthereumToSubstrateExchange> for SubstrateTransactionsTarget {
	type Error = RpcError;

	async fn tick(&self) {
		async_std::task::sleep(SUBSTRATE_TICK_INTERVAL).await;
	}

	async fn is_header_known(&self, id: &EthereumHeaderId) -> Result<bool, Self::Error> {
		self.client.ethereum_header_known(*id).await
	}

	async fn is_header_finalized(&self, id: &EthereumHeaderId) -> Result<bool, Self::Error> {
		// we check if header is finalized by simple comparison of the header number and
		// number of best finalized PoA header known to Substrate node.
		//
		// this may lead to failure in tx proof import if PoA reorganization has happened
		// after we have checked that our tx has been included into given block
		//
		// the fix is easy, but since this code is mostly developed for demonstration purposes,
		// I'm leaving this KISS-based design here
		let best_finalized_ethereum_block = self.client.best_ethereum_finalized_block().await?;
		Ok(id.0 <= best_finalized_ethereum_block.0)
	}

	async fn submit_transaction_proof(&self, proof: EthereumTransactionInclusionProof) -> Result<(), Self::Error> {
		let sign_params = self.sign_params.clone();
		self.client.submit_exchange_transaction_proof(sign_params, proof).await
	}
}

/// Relay exchange transaction proof to Substrate node.
pub fn run(params: EthereumExchangeParams) {
	let eth_tx_hash = params.eth_tx_hash;
	let mut local_pool = futures::executor::LocalPool::new();

	let result = local_pool.run_until(async move {
		let eth_client = EthereumRpcClient::new(params.eth);
		let sub_client = SubstrateRpcClient::new(params.sub).await?;

		let source = EthereumTransactionsSource { client: eth_client };
		let target = SubstrateTransactionsTarget {
			client: sub_client,
			sign_params: params.sub_sign,
		};

		relay_single_transaction_proof(&source, &target, eth_tx_hash).await
	});

	match result {
		Ok(_) => {
			log::info!(
				target: "bridge",
				"Ethereum transaction {} proof has been successfully submitted to Substrate node",
				eth_tx_hash,
			);
		}
		Err(err) => {
			log::error!(
				target: "bridge",
				"Error submitting Ethereum transaction {} proof to Substrate node: {}",
				eth_tx_hash,
				err,
			);
		}
	}
}
