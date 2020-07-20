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

use crate::ethereum_types::{Bytes, EthereumHeaderId, QueuedEthereumHeader, H256};
use crate::rpc::{Substrate, SubstrateRpc};
use crate::rpc_errors::RpcError;
use crate::substrate_types::{
	into_substrate_ethereum_header, into_substrate_ethereum_receipts, Hash, Header as SubstrateHeader, Number,
	SignedBlock as SignedSubstrateBlock,
};
use crate::sync_types::{HeaderId, SubmittedHeaders};

use async_trait::async_trait;
use codec::{Decode, Encode};
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient;
use jsonrpsee::Client;
use num_traits::Zero;
use sp_bridge_eth_poa::Header as SubstrateEthereumHeader;
use sp_core::crypto::Pair;
use sp_runtime::traits::IdentifyAccount;
use std::collections::VecDeque;

const ETH_API_IMPORT_REQUIRES_RECEIPTS: &str = "EthereumHeadersApi_is_import_requires_receipts";
const ETH_API_IS_KNOWN_BLOCK: &str = "EthereumHeadersApi_is_known_block";
const ETH_API_BEST_BLOCK: &str = "EthereumHeadersApi_best_block";
const ETH_API_BEST_FINALIZED_BLOCK: &str = "EthereumHeadersApi_finalized_block";
const SUB_API_GRANDPA_AUTHORITIES: &str = "GrandpaApi_grandpa_authorities";

type Result<T> = std::result::Result<T, RpcError>;
type GrandpaAuthorityList = Vec<u8>;

/// Substrate connection params.
#[derive(Debug, Clone)]
pub struct SubstrateConnectionParams {
	/// Substrate RPC host.
	pub host: String,
	/// Substrate RPC port.
	pub port: u16,
}

impl Default for SubstrateConnectionParams {
	fn default() -> Self {
		SubstrateConnectionParams {
			host: "localhost".into(),
			port: 9933,
		}
	}
}

/// Substrate signing params.
#[derive(Clone)]
pub struct SubstrateSigningParams {
	/// Substrate transactions signer.
	pub signer: sp_core::sr25519::Pair,
}

impl std::fmt::Debug for SubstrateSigningParams {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "{}", self.signer.public())
	}
}

impl Default for SubstrateSigningParams {
	fn default() -> Self {
		SubstrateSigningParams {
			signer: sp_keyring::AccountKeyring::Alice.pair(),
		}
	}
}

/// Substrate client type.
pub struct SubstrateRpcClient {
	/// Substrate RPC client.
	client: Client,
	/// Genesis block hash.
	genesis_hash: H256,
}

impl SubstrateRpcClient {
	/// Returns client that is able to call RPCs on Substrate node.
	pub async fn new(params: SubstrateConnectionParams) -> Result<Self> {
		let uri = format!("http://{}:{}", params.host, params.port);
		let transport = HttpTransportClient::new(&uri);
		let raw_client = RawClient::new(transport);
		let client: Client = raw_client.into();

		let number: Number = Zero::zero();
		let genesis_hash = Substrate::chain_get_block_hash(&client, number).await?;

		Ok(Self { client, genesis_hash })
	}
}

#[async_trait]
impl SubstrateRpc for SubstrateRpcClient {
	async fn best_header(&self) -> Result<SubstrateHeader> {
		Ok(Substrate::chain_get_header(&self.client, None).await?)
	}

	async fn get_block(&self, block_hash: Option<Hash>) -> Result<SignedSubstrateBlock> {
		Ok(Substrate::chain_get_block(&self.client, block_hash).await?)
	}

	async fn header_by_hash(&self, block_hash: Hash) -> Result<SubstrateHeader> {
		Ok(Substrate::chain_get_header(&self.client, block_hash).await?)
	}

	async fn block_hash_by_number(&self, number: Number) -> Result<Hash> {
		Ok(Substrate::chain_get_block_hash(&self.client, number).await?)
	}

	async fn header_by_number(&self, block_number: Number) -> Result<SubstrateHeader> {
		let block_hash = Self::block_hash_by_number(self, block_number).await?;
		Ok(Self::header_by_hash(self, block_hash).await?)
	}

	async fn next_account_index(&self, account: node_primitives::AccountId) -> Result<node_primitives::Index> {
		Ok(Substrate::system_account_next_index(&self.client, account).await?)
	}

	async fn best_ethereum_block(&self) -> Result<EthereumHeaderId> {
		let call = ETH_API_BEST_BLOCK.to_string();
		let data = Bytes(Vec::new());

		let encoded_response = Substrate::state_call(&self.client, call, data, None).await?;
		let decoded_response: (u64, sp_bridge_eth_poa::H256) = Decode::decode(&mut &encoded_response.0[..])?;

		let best_header_id = HeaderId(decoded_response.0, decoded_response.1);
		Ok(best_header_id)
	}

	async fn best_ethereum_finalized_block(&self) -> Result<EthereumHeaderId> {
		let call = ETH_API_BEST_FINALIZED_BLOCK.to_string();
		let data = Bytes("0x".into());

		let encoded_response = Substrate::state_call(&self.client, call, data, None).await?;
		let decoded_response: (u64, sp_bridge_eth_poa::H256) = Decode::decode(&mut &encoded_response.0[..])?;

		let best_header_id = HeaderId(decoded_response.0, decoded_response.1);
		Ok(best_header_id)
	}

	async fn ethereum_receipts_required(&self, header: SubstrateEthereumHeader) -> Result<bool> {
		let call = ETH_API_IMPORT_REQUIRES_RECEIPTS.to_string();
		let data = Bytes(header.encode());

		let encoded_response = Substrate::state_call(&self.client, call, data, None).await?;
		let receipts_required: bool = Decode::decode(&mut &encoded_response.0[..])?;

		Ok(receipts_required)
	}

	// The Substrate module could prune old headers. So this function could return false even
	// if header is synced. And we'll mark corresponding Ethereum header as Orphan.
	//
	// But when we read the best header from Substrate next time, we will know that
	// there's a better header. This Orphan will either be marked as synced, or
	// eventually pruned.
	async fn ethereum_header_known(&self, header_id: EthereumHeaderId) -> Result<bool> {
		let call = ETH_API_IS_KNOWN_BLOCK.to_string();
		let data = Bytes(header_id.1.encode());

		let encoded_response = Substrate::state_call(&self.client, call, data, None).await?;
		let is_known_block: bool = Decode::decode(&mut &encoded_response.0[..])?;

		Ok(is_known_block)
	}

	async fn submit_extrinsic(&self, transaction: Bytes) -> Result<Hash> {
		Ok(Substrate::author_submit_extrinsic(&self.client, transaction).await?)
	}

	async fn grandpa_authorities_set(&self, block: Hash) -> Result<GrandpaAuthorityList> {
		let call = SUB_API_GRANDPA_AUTHORITIES.to_string();
		let data = Bytes(block.as_bytes().to_vec());

		let encoded_response = Substrate::state_call(&self.client, call, data, None).await?;
		let authority_list = encoded_response.0;

		Ok(authority_list)
	}
}

/// A trait for RPC calls which are used to submit Ethereum headers to a Substrate
/// runtime. These are typically calls which use a combination of other low-level RPC
/// calls.
#[async_trait]
pub trait SubmitEthereumHeaders: SubstrateRpc {
	/// Submits Ethereum header to Substrate runtime.
	async fn submit_ethereum_headers(
		&self,
		params: SubstrateSigningParams,
		headers: Vec<QueuedEthereumHeader>,
		sign_transactions: bool,
	) -> SubmittedHeaders<EthereumHeaderId, RpcError>;

	/// Submits signed Ethereum header to Substrate runtime.
	async fn submit_signed_ethereum_headers(
		&self,
		params: SubstrateSigningParams,
		headers: Vec<QueuedEthereumHeader>,
	) -> SubmittedHeaders<EthereumHeaderId, RpcError>;

	/// Submits unsigned Ethereum header to Substrate runtime.
	async fn submit_unsigned_ethereum_headers(
		&self,
		headers: Vec<QueuedEthereumHeader>,
	) -> SubmittedHeaders<EthereumHeaderId, RpcError>;
}

#[async_trait]
impl SubmitEthereumHeaders for SubstrateRpcClient {
	async fn submit_ethereum_headers(
		&self,
		params: SubstrateSigningParams,
		headers: Vec<QueuedEthereumHeader>,
		sign_transactions: bool,
	) -> SubmittedHeaders<EthereumHeaderId, RpcError> {
		if sign_transactions {
			self.submit_signed_ethereum_headers(params, headers).await
		} else {
			self.submit_unsigned_ethereum_headers(headers).await
		}
	}

	async fn submit_signed_ethereum_headers(
		&self,
		params: SubstrateSigningParams,
		headers: Vec<QueuedEthereumHeader>,
	) -> SubmittedHeaders<EthereumHeaderId, RpcError> {
		let ids = headers.iter().map(|header| header.id()).collect();
		let submission_result = async {
			let account_id = params.signer.public().as_array_ref().clone().into();
			let nonce = self.next_account_index(account_id).await?;

			let transaction = create_signed_submit_transaction(headers, &params.signer, nonce, self.genesis_hash);
			let _ = self.submit_extrinsic(Bytes(transaction.encode())).await?;
			Ok(())
		}
		.await;

		match submission_result {
			Ok(_) => SubmittedHeaders {
				submitted: ids,
				incomplete: Vec::new(),
				rejected: Vec::new(),
				fatal_error: None,
			},
			Err(error) => SubmittedHeaders {
				submitted: Vec::new(),
				incomplete: Vec::new(),
				rejected: ids,
				fatal_error: Some(error),
			},
		}
	}

	async fn submit_unsigned_ethereum_headers(
		&self,
		headers: Vec<QueuedEthereumHeader>,
	) -> SubmittedHeaders<EthereumHeaderId, RpcError> {
		let mut ids = headers.iter().map(|header| header.id()).collect::<VecDeque<_>>();
		let mut submitted_headers = SubmittedHeaders::default();
		for header in headers {
			let id = ids.pop_front().expect("both collections have same size; qed");
			let transaction = create_unsigned_submit_transaction(header);
			match self.submit_extrinsic(Bytes(transaction.encode())).await {
				Ok(_) => submitted_headers.submitted.push(id),
				Err(error) => {
					submitted_headers.rejected.push(id);
					submitted_headers.rejected.extend(ids);
					submitted_headers.fatal_error = Some(error);
					break;
				}
			}
		}

		submitted_headers
	}
}

/// A trait for RPC calls which are used to submit proof of Ethereum exchange transaction to a
/// Substrate runtime. These are typically calls which use a combination of other low-level RPC
/// calls.
#[async_trait]
pub trait SubmitEthereumExchangeTransactionProof: SubstrateRpc {
	/// Submits Ethereum exchange transaction proof to Substrate runtime.
	async fn submit_exchange_transaction_proof(
		&self,
		params: SubstrateSigningParams,
		proof: bridge_node_runtime::exchange::EthereumTransactionInclusionProof,
	) -> Result<()>;
}

#[async_trait]
impl SubmitEthereumExchangeTransactionProof for SubstrateRpcClient {
	async fn submit_exchange_transaction_proof(
		&self,
		params: SubstrateSigningParams,
		proof: bridge_node_runtime::exchange::EthereumTransactionInclusionProof,
	) -> Result<()> {
		let account_id = params.signer.public().as_array_ref().clone().into();
		let nonce = self.next_account_index(account_id).await?;

		let transaction = create_signed_transaction(
			bridge_node_runtime::Call::BridgeCurrencyExchange(
				bridge_node_runtime::BridgeCurrencyExchangeCall::import_peer_transaction(proof),
			),
			&params.signer,
			nonce,
			self.genesis_hash,
		);
		let _ = self.submit_extrinsic(Bytes(transaction.encode())).await?;
		Ok(())
	}
}

/// Create signed Substrate transaction for submitting Ethereum headers.
fn create_signed_submit_transaction(
	headers: Vec<QueuedEthereumHeader>,
	signer: &sp_core::sr25519::Pair,
	index: node_primitives::Index,
	genesis_hash: H256,
) -> bridge_node_runtime::UncheckedExtrinsic {
	create_signed_transaction(
		bridge_node_runtime::Call::BridgeEthPoA(bridge_node_runtime::BridgeEthPoACall::import_signed_headers(
			headers
				.into_iter()
				.map(|header| {
					(
						into_substrate_ethereum_header(header.header()),
						into_substrate_ethereum_receipts(header.extra()),
					)
				})
				.collect(),
		)),
		signer,
		index,
		genesis_hash,
	)
}

/// Create unsigned Substrate transaction for submitting Ethereum header.
fn create_unsigned_submit_transaction(header: QueuedEthereumHeader) -> bridge_node_runtime::UncheckedExtrinsic {
	let function =
		bridge_node_runtime::Call::BridgeEthPoA(bridge_node_runtime::BridgeEthPoACall::import_unsigned_header(
			into_substrate_ethereum_header(header.header()),
			into_substrate_ethereum_receipts(header.extra()),
		));

	bridge_node_runtime::UncheckedExtrinsic::new_unsigned(function)
}

/// Create signed Substrate transaction.
fn create_signed_transaction(
	function: bridge_node_runtime::Call,
	signer: &sp_core::sr25519::Pair,
	index: node_primitives::Index,
	genesis_hash: H256,
) -> bridge_node_runtime::UncheckedExtrinsic {
	let extra = |i: node_primitives::Index, f: node_primitives::Balance| {
		(
			frame_system::CheckSpecVersion::<bridge_node_runtime::Runtime>::new(),
			frame_system::CheckTxVersion::<bridge_node_runtime::Runtime>::new(),
			frame_system::CheckGenesis::<bridge_node_runtime::Runtime>::new(),
			frame_system::CheckEra::<bridge_node_runtime::Runtime>::from(sp_runtime::generic::Era::Immortal),
			frame_system::CheckNonce::<bridge_node_runtime::Runtime>::from(i),
			frame_system::CheckWeight::<bridge_node_runtime::Runtime>::new(),
			pallet_transaction_payment::ChargeTransactionPayment::<bridge_node_runtime::Runtime>::from(f),
		)
	};
	let raw_payload = bridge_node_runtime::SignedPayload::from_raw(
		function,
		extra(index, 0),
		(
			bridge_node_runtime::VERSION.spec_version,
			bridge_node_runtime::VERSION.transaction_version,
			genesis_hash,
			genesis_hash,
			(),
			(),
			(),
		),
	);
	let signature = raw_payload.using_encoded(|payload| signer.sign(payload));
	let signer: sp_runtime::MultiSigner = signer.public().into();
	let (function, extra, _) = raw_payload.deconstruct();

	bridge_node_runtime::UncheckedExtrinsic::new_signed(function, signer.into_account(), signature.into(), extra)
}
