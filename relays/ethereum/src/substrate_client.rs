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

use crate::ethereum_sync_loop::MaybeConnectionError;
use crate::ethereum_types::{Bytes, HeaderId as EthereumHeaderId, QueuedHeader as QueuedEthereumHeader, H256};
use crate::substrate_types::{into_substrate_ethereum_header, into_substrate_ethereum_receipts, TransactionHash};
use codec::{Decode, Encode};
use jsonrpsee::common::Params;
use jsonrpsee::raw::{RawClient, RawClientError};
use jsonrpsee::transport::http::{HttpTransportClient, RequestError};
use serde_json::{from_value, to_value};
use sp_core::crypto::Pair;
use sp_runtime::traits::IdentifyAccount;

/// Substrate client type.
pub struct Client {
	/// Substrate RPC client.
	rpc_client: RawClient<HttpTransportClient>,
	/// Transactions signer.
	signer: sp_core::sr25519::Pair,
	/// Genesis block hash.
	genesis_hash: Option<H256>,
}

/// All possible errors that can occur during interacting with Ethereum node.
#[derive(Debug)]
pub enum Error {
	/// Request start failed.
	StartRequestFailed(RequestError),
	/// Request not found (should never occur?).
	RequestNotFound,
	/// Failed to receive response.
	ResponseRetrievalFailed(RawClientError<RequestError>),
	/// Failed to parse response.
	ResponseParseFailed,
}

impl MaybeConnectionError for Error {
	fn is_connection_error(&self) -> bool {
		match *self {
			Error::StartRequestFailed(_) | Error::ResponseRetrievalFailed(_) => true,
			_ => false,
		}
	}
}

/// Returns client that is able to call RPCs on Substrate node.
pub fn client(uri: &str, signer: sp_core::sr25519::Pair) -> Client {
	let transport = HttpTransportClient::new(uri);
	Client {
		rpc_client: RawClient::new(transport),
		signer,
		genesis_hash: None,
	}
}

/// Returns best Ethereum block that Substrate runtime knows of.
pub async fn best_ethereum_block(client: Client) -> (Client, Result<EthereumHeaderId, Error>) {
	let (client, result) = call_rpc::<(u64, H256)>(
		client,
		"state_call",
		Params::Array(vec![
			to_value("EthereumHeadersApi_best_block").unwrap(),
			to_value("0x").unwrap(),
		]),
	)
	.await;
	(client, result.map(|(num, hash)| EthereumHeaderId(num, hash)))
}

/// Returns true if transactions receipts are required for Ethereum header submission.
pub async fn ethereum_receipts_required(
	client: Client,
	header: QueuedEthereumHeader,
) -> (Client, Result<(EthereumHeaderId, bool), Error>) {
	let id = header.id();
	let header = into_substrate_ethereum_header(header.header());
	let encoded_header = header.encode();
	let (client, receipts_required) = call_rpc(
		client,
		"state_call",
		Params::Array(vec![
			to_value("EthereumHeadersApi_is_import_requires_receipts").unwrap(),
			to_value(Bytes(encoded_header)).unwrap(),
		]),
	)
	.await;
	(
		client,
		receipts_required.map(|receipts_required| (id, receipts_required)),
	)
}

/// Returns true if Ethereum header is known to Substrate runtime.
pub async fn ethereum_header_known(
	client: Client,
	id: EthereumHeaderId,
) -> (Client, Result<(EthereumHeaderId, bool), Error>) {
	// Substrate module could prune old headers. So this fn could return false even
	// if header is synced. And we'll mark corresponding Ethereum header as Orphan.
	//
	// But when we'll read best header from Substrate next time, we will know that
	// there's a better header => this Orphan will either be marked as synced, or
	// eventually pruned.
	let encoded_id = id.1.encode();
	let (client, is_known_block) = call_rpc(
		client,
		"state_call",
		Params::Array(vec![
			to_value("EthereumHeadersApi_is_known_block").unwrap(),
			to_value(Bytes(encoded_id)).unwrap(),
		]),
	)
	.await;
	(client, is_known_block.map(|is_known_block| (id, is_known_block)))
}

/// Submits Ethereum header to Substrate runtime.
pub async fn submit_ethereum_headers(
	client: Client,
	headers: Vec<QueuedEthereumHeader>,
) -> (Client, Result<(TransactionHash, Vec<EthereumHeaderId>), Error>) {
	let ids = headers.iter().map(|header| header.id()).collect();
	let (client, genesis_hash) = match client.genesis_hash {
		Some(genesis_hash) => (client, genesis_hash),
		None => {
			let (mut client, genesis_hash) = block_hash_by_number(client, 0).await;
			let genesis_hash = match genesis_hash {
				Ok(genesis_hash) => genesis_hash,
				Err(err) => return (client, Err(err)),
			};
			client.genesis_hash = Some(genesis_hash);
			(client, genesis_hash)
		}
	};
	let account_id = client.signer.public().as_array_ref().clone().into();
	let (client, nonce) = next_account_index(client, account_id).await;
	let nonce = match nonce {
		Ok(nonce) => nonce,
		Err(err) => return (client, Err(err)),
	};
	let transaction = create_submit_transaction(headers, &client.signer, nonce, genesis_hash);
	let encoded_transaction = transaction.encode();
	let (client, transaction_hash) = call_rpc(
		client,
		"author_submitExtrinsic",
		Params::Array(vec![to_value(Bytes(encoded_transaction)).unwrap()]),
	)
	.await;
	(client, transaction_hash.map(|transaction_hash| (transaction_hash, ids)))
}

/// Get Substrate block hash by its number.
async fn block_hash_by_number(client: Client, number: u64) -> (Client, Result<H256, Error>) {
	call_rpc(
		client,
		"chain_getBlockHash",
		Params::Array(vec![to_value(number).unwrap()]),
	)
	.await
}

/// Get substrate account nonce.
async fn next_account_index(
	client: Client,
	account: node_primitives::AccountId,
) -> (Client, Result<node_primitives::Index, Error>) {
	use sp_core::crypto::Ss58Codec;

	let (client, index) = call_rpc_u64(
		client,
		"system_accountNextIndex",
		Params::Array(vec![to_value(account.to_ss58check()).unwrap()]),
	)
	.await;
	(client, index.map(|index| index as _))
}

/// Calls RPC on Substrate node that returns Bytes.
async fn call_rpc<T: Decode>(mut client: Client, method: &'static str, params: Params) -> (Client, Result<T, Error>) {
	async fn do_call_rpc<T: Decode>(client: &mut Client, method: &'static str, params: Params) -> Result<T, Error> {
		let request_id = client
			.rpc_client
			.start_request(method, params)
			.await
			.map_err(Error::StartRequestFailed)?;
		// WARN: if there'll be need for executing >1 request at a time, we should avoid
		// calling request_by_id
		let response = client
			.rpc_client
			.request_by_id(request_id)
			.ok_or(Error::RequestNotFound)?
			.await
			.map_err(Error::ResponseRetrievalFailed)?;
		let encoded_response: Bytes = from_value(response).map_err(|_| Error::ResponseParseFailed)?;
		Decode::decode(&mut &encoded_response.0[..]).map_err(|_| Error::ResponseParseFailed)
	}

	let result = do_call_rpc(&mut client, method, params).await;
	(client, result)
}

/// Calls RPC on Substrate node that returns u64.
async fn call_rpc_u64(mut client: Client, method: &'static str, params: Params) -> (Client, Result<u64, Error>) {
	async fn do_call_rpc(client: &mut Client, method: &'static str, params: Params) -> Result<u64, Error> {
		let request_id = client
			.rpc_client
			.start_request(method, params)
			.await
			.map_err(Error::StartRequestFailed)?;
		// WARN: if there'll be need for executing >1 request at a time, we should avoid
		// calling request_by_id
		let response = client
			.rpc_client
			.request_by_id(request_id)
			.ok_or(Error::RequestNotFound)?
			.await
			.map_err(Error::ResponseRetrievalFailed)?;
		response.as_u64().ok_or(Error::ResponseParseFailed)
	}

	let result = do_call_rpc(&mut client, method, params).await;
	(client, result)
}

/// Create Substrate transaction for submitting Ethereum header.
fn create_submit_transaction(
	headers: Vec<QueuedEthereumHeader>,
	signer: &sp_core::sr25519::Pair,
	index: node_primitives::Index,
	genesis_hash: H256,
) -> bridge_node_runtime::UncheckedExtrinsic {
	let function = bridge_node_runtime::Call::BridgeEthPoA(bridge_node_runtime::BridgeEthPoACall::import_headers(
		headers
			.into_iter()
			.map(|header| {
				let (header, receipts) = header.extract();
				(
					into_substrate_ethereum_header(&header),
					into_substrate_ethereum_receipts(&receipts),
				)
			})
			.collect(),
	));

	let extra = |i: node_primitives::Index, f: node_primitives::Balance| {
		(
			frame_system::CheckVersion::<bridge_node_runtime::Runtime>::new(),
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
			bridge_node_runtime::VERSION.spec_version as u32,
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

	bridge_node_runtime::UncheckedExtrinsic::new_signed(function, signer.into_account().into(), signature.into(), extra)
}
