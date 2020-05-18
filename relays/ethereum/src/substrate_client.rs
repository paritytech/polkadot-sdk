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
use crate::substrate_types::{
	into_substrate_ethereum_header, into_substrate_ethereum_receipts, GrandpaJustification, Hash,
	Header as SubstrateHeader, Number, SignedBlock as SignedSubstrateBlock, SubstrateHeaderId,
};
use crate::sync_types::{HeaderId, MaybeConnectionError, SourceHeader};
use crate::{bail_on_arg_error, bail_on_error};
use codec::{Decode, Encode};
use jsonrpsee::common::Params;
use jsonrpsee::raw::{RawClient, RawClientError};
use jsonrpsee::transport::http::{HttpTransportClient, RequestError};
use num_traits::Zero;
use serde::de::DeserializeOwned;
use serde_json::{from_value, to_value, Value};
use sp_core::crypto::Pair;
use sp_runtime::traits::IdentifyAccount;

/// Substrate connection params.
#[derive(Debug)]
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
pub struct Client {
	/// Substrate RPC client.
	rpc_client: RawClient<HttpTransportClient>,
	/// Genesis block hash.
	genesis_hash: Option<H256>,
}

/// All possible errors that can occur during interacting with Ethereum node.
#[derive(Debug)]
pub enum Error {
	/// Request start failed.
	StartRequestFailed(RequestError),
	/// Error serializing request.
	RequestSerialization(serde_json::Error),
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
pub fn client(params: SubstrateConnectionParams) -> Client {
	let uri = format!("http://{}:{}", params.host, params.port);
	let transport = HttpTransportClient::new(&uri);
	Client {
		rpc_client: RawClient::new(transport),
		genesis_hash: None,
	}
}

/// Returns best Substrate header.
pub async fn best_header(client: Client) -> (Client, Result<SubstrateHeader, Error>) {
	call_rpc(client, "chain_getHeader", Params::None, rpc_returns_value).await
}

/// Returns Substrate header by hash.
pub async fn header_by_hash(client: Client, hash: Hash) -> (Client, Result<SubstrateHeader, Error>) {
	let hash = bail_on_arg_error!(to_value(hash).map_err(|e| Error::RequestSerialization(e)), client);
	call_rpc(client, "chain_getHeader", Params::Array(vec![hash]), rpc_returns_value).await
}

/// Returns Substrate header by number.
pub async fn header_by_number(client: Client, number: Number) -> (Client, Result<SubstrateHeader, Error>) {
	let (client, hash) = bail_on_error!(block_hash_by_number(client, number).await);
	header_by_hash(client, hash).await
}

/// Returns best Ethereum block that Substrate runtime knows of.
pub async fn best_ethereum_block(client: Client) -> (Client, Result<EthereumHeaderId, Error>) {
	let (client, result) = call_rpc(
		client,
		"state_call",
		Params::Array(vec![
			serde_json::Value::String("EthereumHeadersApi_best_block".into()),
			serde_json::Value::String("0x".into()),
		]),
		rpc_returns_encoded_value,
	)
	.await;
	(client, result.map(|(num, hash)| HeaderId(num, hash)))
}

/// Returns true if transactions receipts are required for Ethereum header submission.
pub async fn ethereum_receipts_required(
	client: Client,
	header: QueuedEthereumHeader,
) -> (Client, Result<(EthereumHeaderId, bool), Error>) {
	let id = header.header().id();
	let header = into_substrate_ethereum_header(header.header());
	let encoded_header = bail_on_arg_error!(
		to_value(Bytes(header.encode())).map_err(|e| Error::RequestSerialization(e)),
		client
	);
	let (client, receipts_required) = call_rpc(
		client,
		"state_call",
		Params::Array(vec![
			serde_json::Value::String("EthereumHeadersApi_is_import_requires_receipts".into()),
			encoded_header,
		]),
		rpc_returns_encoded_value,
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
	let encoded_id = bail_on_arg_error!(
		to_value(Bytes(id.1.encode())).map_err(|e| Error::RequestSerialization(e)),
		client
	);
	let (client, is_known_block) = call_rpc(
		client,
		"state_call",
		Params::Array(vec![
			serde_json::Value::String("EthereumHeadersApi_is_known_block".into()),
			encoded_id,
		]),
		rpc_returns_encoded_value,
	)
	.await;
	(client, is_known_block.map(|is_known_block| (id, is_known_block)))
}

/// Submits Ethereum header to Substrate runtime.
pub async fn submit_ethereum_headers(
	client: Client,
	params: SubstrateSigningParams,
	headers: Vec<QueuedEthereumHeader>,
	sign_transactions: bool,
) -> (Client, Result<Vec<EthereumHeaderId>, Error>) {
	match sign_transactions {
		true => submit_signed_ethereum_headers(client, params, headers).await,
		false => submit_unsigned_ethereum_headers(client, headers).await,
	}
}

/// Submits signed Ethereum header to Substrate runtime.
pub async fn submit_signed_ethereum_headers(
	client: Client,
	params: SubstrateSigningParams,
	headers: Vec<QueuedEthereumHeader>,
) -> (Client, Result<Vec<EthereumHeaderId>, Error>) {
	let ids = headers.iter().map(|header| header.id()).collect();
	let (client, genesis_hash) = match client.genesis_hash {
		Some(genesis_hash) => (client, genesis_hash),
		None => {
			let (mut client, genesis_hash) = bail_on_error!(block_hash_by_number(client, Zero::zero()).await);
			client.genesis_hash = Some(genesis_hash);
			(client, genesis_hash)
		}
	};
	let account_id = params.signer.public().as_array_ref().clone().into();
	let (client, nonce) = bail_on_error!(next_account_index(client, account_id).await);

	let transaction = create_signed_submit_transaction(headers, &params.signer, nonce, genesis_hash);
	let encoded_transaction = bail_on_arg_error!(
		to_value(Bytes(transaction.encode())).map_err(|e| Error::RequestSerialization(e)),
		client
	);
	let (client, _) = bail_on_error!(
		call_rpc(
			client,
			"author_submitExtrinsic",
			Params::Array(vec![encoded_transaction]),
			|_| Ok(()),
		)
		.await
	);

	(client, Ok(ids))
}

/// Submits unsigned Ethereum header to Substrate runtime.
pub async fn submit_unsigned_ethereum_headers(
	mut client: Client,
	headers: Vec<QueuedEthereumHeader>,
) -> (Client, Result<Vec<EthereumHeaderId>, Error>) {
	let ids = headers.iter().map(|header| header.id()).collect();
	for header in headers {
		let transaction = create_unsigned_submit_transaction(header);

		let encoded_transaction = bail_on_arg_error!(
			to_value(Bytes(transaction.encode())).map_err(|e| Error::RequestSerialization(e)),
			client
		);
		let (used_client, _) = bail_on_error!(
			call_rpc(
				client,
				"author_submitExtrinsic",
				Params::Array(vec![encoded_transaction]),
				|_| Ok(()),
			)
			.await
		);

		client = used_client;
	}

	(client, Ok(ids))
}

/// Get GRANDPA justification for given block.
pub async fn grandpa_justification(
	client: Client,
	id: SubstrateHeaderId,
) -> (Client, Result<(SubstrateHeaderId, Option<GrandpaJustification>), Error>) {
	let hash = bail_on_arg_error!(to_value(id.1).map_err(|e| Error::RequestSerialization(e)), client);
	let (client, signed_block) = call_rpc(client, "chain_getBlock", Params::Array(vec![hash]), rpc_returns_value).await;
	(
		client,
		signed_block.map(|signed_block: SignedSubstrateBlock| (id, signed_block.justification)),
	)
}

/// Get GRANDPA authorities set at given block.
pub async fn grandpa_authorities_set(client: Client, block: Hash) -> (Client, Result<Vec<u8>, Error>) {
	let block = bail_on_arg_error!(to_value(block).map_err(|e| Error::RequestSerialization(e)), client);
	call_rpc(
		client,
		"state_call",
		Params::Array(vec![
			serde_json::Value::String("GrandpaApi_grandpa_authorities".into()),
			block,
		]),
		rpc_returns_bytes,
	)
	.await
}

/// Get Substrate block hash by its number.
async fn block_hash_by_number(client: Client, number: Number) -> (Client, Result<Hash, Error>) {
	let number = bail_on_arg_error!(to_value(number).map_err(|e| Error::RequestSerialization(e)), client);
	call_rpc(
		client,
		"chain_getBlockHash",
		Params::Array(vec![number]),
		rpc_returns_value,
	)
	.await
}

/// Get substrate account nonce.
async fn next_account_index(
	client: Client,
	account: node_primitives::AccountId,
) -> (Client, Result<node_primitives::Index, Error>) {
	use sp_core::crypto::Ss58Codec;

	let account = bail_on_arg_error!(
		to_value(account.to_ss58check()).map_err(|e| Error::RequestSerialization(e)),
		client
	);
	let (client, index) = call_rpc(client, "system_accountNextIndex", Params::Array(vec![account]), |v| {
		rpc_returns_value::<u64>(v)
	})
	.await;
	(client, index.map(|index| index as _))
}

/// Calls RPC on Substrate node that returns Bytes.
async fn call_rpc<T>(
	mut client: Client,
	method: &'static str,
	params: Params,
	decode_value: impl Fn(Value) -> Result<T, Error>,
) -> (Client, Result<T, Error>) {
	async fn do_call_rpc<T>(
		client: &mut Client,
		method: &'static str,
		params: Params,
		decode_value: impl Fn(Value) -> Result<T, Error>,
	) -> Result<T, Error> {
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
		decode_value(response)
	}

	let result = do_call_rpc(&mut client, method, params, decode_value).await;
	(client, result)
}

/// Create signed Substrate transaction for submitting Ethereum headers.
fn create_signed_submit_transaction(
	headers: Vec<QueuedEthereumHeader>,
	signer: &sp_core::sr25519::Pair,
	index: node_primitives::Index,
	genesis_hash: H256,
) -> bridge_node_runtime::UncheckedExtrinsic {
	let function =
		bridge_node_runtime::Call::BridgeEthPoA(bridge_node_runtime::BridgeEthPoACall::import_signed_headers(
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

/// Create unsigned Substrate transaction for submitting Ethereum header.
fn create_unsigned_submit_transaction(header: QueuedEthereumHeader) -> bridge_node_runtime::UncheckedExtrinsic {
	let (header, receipts) = header.extract();
	let function =
		bridge_node_runtime::Call::BridgeEthPoA(bridge_node_runtime::BridgeEthPoACall::import_unsigned_header(
			into_substrate_ethereum_header(&header),
			into_substrate_ethereum_receipts(&receipts),
		));

	bridge_node_runtime::UncheckedExtrinsic::new_unsigned(function)
}

/// When RPC method returns encoded value.
fn rpc_returns_encoded_value<T: Decode>(value: Value) -> Result<T, Error> {
	let encoded_response: Bytes = from_value(value).map_err(|_| Error::ResponseParseFailed)?;
	Decode::decode(&mut &encoded_response.0[..]).map_err(|_| Error::ResponseParseFailed)
}

/// When RPC method returns value.
fn rpc_returns_value<T: DeserializeOwned>(value: Value) -> Result<T, Error> {
	from_value(value).map_err(|_| Error::ResponseParseFailed)
}

/// When RPC method returns raw bytes.
fn rpc_returns_bytes(value: Value) -> Result<Vec<u8>, Error> {
	let encoded_response: Bytes = from_value(value).map_err(|_| Error::ResponseParseFailed)?;
	Ok(encoded_response.0)
}
