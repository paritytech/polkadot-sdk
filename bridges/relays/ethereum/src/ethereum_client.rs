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

use crate::ethereum_types::{Address, Bytes, EthereumHeaderId, Header, Receipt, TransactionHash, H256, U256, U64};
use crate::substrate_types::{GrandpaJustification, Hash as SubstrateHash, QueuedSubstrateHeader, SubstrateHeaderId};
use crate::sync_types::{HeaderId, MaybeConnectionError};
use crate::{bail_on_arg_error, bail_on_error};
use codec::{Decode, Encode};
use ethabi::FunctionOutputDecoder;
use jsonrpsee::common::Params;
use jsonrpsee::raw::{RawClient, RawClientError};
use jsonrpsee::transport::http::{HttpTransportClient, RequestError};
use parity_crypto::publickey::KeyPair;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{from_value, to_value};
use std::collections::HashSet;

// to encode/decode contract calls
ethabi_contract::use_contract!(bridge_contract, "res/substrate-bridge-abi.json");

/// Proof of hash serialization success.
const HASH_SERIALIZATION_PROOF: &'static str = "hash serialization never fails; qed";
/// Proof of integer serialization success.
const INT_SERIALIZATION_PROOF: &'static str = "integer serialization never fails; qed";
/// Proof of bool serialization success.
const BOOL_SERIALIZATION_PROOF: &'static str = "bool serialization never fails; qed";

/// Ethereum connection params.
#[derive(Debug)]
pub struct EthereumConnectionParams {
	/// Ethereum RPC host.
	pub host: String,
	/// Ethereum RPC port.
	pub port: u16,
}

impl Default for EthereumConnectionParams {
	fn default() -> Self {
		EthereumConnectionParams {
			host: "localhost".into(),
			port: 8545,
		}
	}
}

/// Ethereum signing params.
#[derive(Clone, Debug)]
pub struct EthereumSigningParams {
	/// Ethereum chain id.
	pub chain_id: u64,
	/// Ethereum transactions signer.
	pub signer: KeyPair,
	/// Gas price we agree to pay.
	pub gas_price: U256,
}

impl Default for EthereumSigningParams {
	fn default() -> Self {
		EthereumSigningParams {
			chain_id: 0x11, // Parity dev chain
			// account that has a lot of ether when we run instant seal engine
			// address: 0x00a329c0648769a73afac7f9381e08fb43dbea72
			// secret: 0x4d5db4107d237df6a3d58ee5f70ae63d73d7658d4026f2eefd2f204c81682cb7
			signer: KeyPair::from_secret_slice(
				&hex::decode("4d5db4107d237df6a3d58ee5f70ae63d73d7658d4026f2eefd2f204c81682cb7")
					.expect("secret is hardcoded, thus valid; qed"),
			)
			.expect("secret is hardcoded, thus valid; qed"),
			gas_price: 8_000_000_000u64.into(), // 8 Gwei
		}
	}
}

/// Ethereum client type.
pub type Client = RawClient<HttpTransportClient>;

/// Ethereum contract call request.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct CallRequest {
	/// Contract address.
	pub to: Option<Address>,
	/// Call data.
	pub data: Option<Bytes>,
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
	ResponseParseFailed(String),
	/// We have received header with missing number and hash fields.
	IncompleteHeader,
	/// We have received receipt with missing gas_used field.
	IncompleteReceipt,
	/// Invalid Substrate block number received from Ethereum node.
	InvalidSubstrateBlockNumber,
}

impl MaybeConnectionError for Error {
	fn is_connection_error(&self) -> bool {
		match *self {
			Error::StartRequestFailed(_) | Error::ResponseRetrievalFailed(_) => true,
			_ => false,
		}
	}
}

/// Returns client that is able to call RPCs on Ethereum node.
pub fn client(params: EthereumConnectionParams) -> Client {
	let uri = format!("http://{}:{}", params.host, params.port);
	let transport = HttpTransportClient::new(&uri);
	RawClient::new(transport)
}

/// Retrieve best known block number from Ethereum node.
pub async fn best_block_number(client: Client) -> (Client, Result<u64, Error>) {
	let (client, result) = call_rpc::<U64>(client, "eth_blockNumber", Params::None).await;
	(client, result.map(|x| x.as_u64()))
}

/// Retrieve block header by its number from Ethereum node.
pub async fn header_by_number(client: Client, number: u64) -> (Client, Result<Header, Error>) {
	let (client, header) = call_rpc(
		client,
		"eth_getBlockByNumber",
		Params::Array(vec![
			to_value(U64::from(number)).expect(INT_SERIALIZATION_PROOF),
			to_value(false).expect(BOOL_SERIALIZATION_PROOF),
		]),
	)
	.await;
	(
		client,
		header.and_then(
			|header: Header| match header.number.is_some() && header.hash.is_some() {
				true => Ok(header),
				false => Err(Error::IncompleteHeader),
			},
		),
	)
}

/// Retrieve block header by its hash from Ethereum node.
pub async fn header_by_hash(client: Client, hash: H256) -> (Client, Result<Header, Error>) {
	let (client, header) = call_rpc(
		client,
		"eth_getBlockByHash",
		Params::Array(vec![
			to_value(hash).expect(HASH_SERIALIZATION_PROOF),
			to_value(false).expect(BOOL_SERIALIZATION_PROOF),
		]),
	)
	.await;
	(
		client,
		header.and_then(
			|header: Header| match header.number.is_none() && header.hash.is_none() {
				true => Ok(header),
				false => Err(Error::IncompleteHeader),
			},
		),
	)
}

/// Retrieve transactions receipts for given block.
pub async fn transactions_receipts(
	mut client: Client,
	id: EthereumHeaderId,
	transactions: Vec<H256>,
) -> (Client, Result<(EthereumHeaderId, Vec<Receipt>), Error>) {
	let mut transactions_receipts = Vec::with_capacity(transactions.len());
	for transaction in transactions {
		let (next_client, transaction_receipt) = bail_on_error!(transaction_receipt(client, transaction).await);
		transactions_receipts.push(transaction_receipt);
		client = next_client;
	}
	(client, Ok((id, transactions_receipts)))
}

/// Retrieve transaction receipt by transaction hash.
async fn transaction_receipt(client: Client, hash: H256) -> (Client, Result<Receipt, Error>) {
	let (client, receipt) = call_rpc::<Receipt>(
		client,
		"eth_getTransactionReceipt",
		Params::Array(vec![to_value(hash).expect(HASH_SERIALIZATION_PROOF)]),
	)
	.await;
	(
		client,
		receipt.and_then(|receipt| match receipt.gas_used.is_some() {
			true => Ok(receipt),
			false => Err(Error::IncompleteReceipt),
		}),
	)
}

/// Returns best Substrate block that PoA chain knows of.
pub async fn best_substrate_block(
	client: Client,
	contract_address: Address,
) -> (Client, Result<SubstrateHeaderId, Error>) {
	let (encoded_call, call_decoder) = bridge_contract::functions::best_known_header::call();
	let call_request = bail_on_arg_error!(
		to_value(CallRequest {
			to: Some(contract_address),
			data: Some(encoded_call.into()),
		})
		.map_err(|e| Error::RequestSerialization(e)),
		client
	);
	let (client, call_result) =
		bail_on_error!(call_rpc::<Bytes>(client, "eth_call", Params::Array(vec![call_request]),).await);
	let (number, raw_hash) = match call_decoder.decode(&call_result.0) {
		Ok((raw_number, raw_hash)) => (raw_number, raw_hash),
		Err(error) => return (client, Err(Error::ResponseParseFailed(format!("{}", error)))),
	};
	let hash = match SubstrateHash::decode(&mut &raw_hash[..]) {
		Ok(hash) => hash,
		Err(error) => return (client, Err(Error::ResponseParseFailed(format!("{}", error)))),
	};

	if number != number.low_u32().into() {
		return (client, Err(Error::InvalidSubstrateBlockNumber));
	}

	(client, Ok(HeaderId(number.low_u32(), hash)))
}

/// Returns true if Substrate header is known to Ethereum node.
pub async fn substrate_header_known(
	client: Client,
	contract_address: Address,
	id: SubstrateHeaderId,
) -> (Client, Result<(SubstrateHeaderId, bool), Error>) {
	let (encoded_call, call_decoder) = bridge_contract::functions::is_known_header::call(id.1);
	let call_request = bail_on_arg_error!(
		to_value(CallRequest {
			to: Some(contract_address),
			data: Some(encoded_call.into()),
		})
		.map_err(|e| Error::RequestSerialization(e)),
		client
	);
	let (client, call_result) =
		bail_on_error!(call_rpc::<Bytes>(client, "eth_call", Params::Array(vec![call_request]),).await);
	match call_decoder.decode(&call_result.0) {
		Ok(is_known_block) => (client, Ok((id, is_known_block))),
		Err(error) => (client, Err(Error::ResponseParseFailed(format!("{}", error)))),
	}
}

/// Submits Substrate headers to Ethereum contract.
pub async fn submit_substrate_headers(
	client: Client,
	params: EthereumSigningParams,
	contract_address: Address,
	headers: Vec<QueuedSubstrateHeader>,
) -> (Client, Result<Vec<SubstrateHeaderId>, Error>) {
	let (mut client, mut nonce) =
		bail_on_error!(account_nonce(client, params.signer.address().as_fixed_bytes().into()).await);

	let ids = headers.iter().map(|header| header.id()).collect();
	for header in headers {
		client = bail_on_error!(
			submit_ethereum_transaction(
				client,
				&params,
				Some(contract_address),
				Some(nonce),
				false,
				bridge_contract::functions::import_header::encode_input(header.extract().0.encode(),),
			)
			.await
		)
		.0;

		nonce += 1.into();
	}

	(client, Ok(ids))
}

/// Returns ids of incomplete Substrate headers.
pub async fn incomplete_substrate_headers(
	client: Client,
	contract_address: Address,
) -> (Client, Result<HashSet<SubstrateHeaderId>, Error>) {
	let (encoded_call, call_decoder) = bridge_contract::functions::incomplete_headers::call();
	let call_request = bail_on_arg_error!(
		to_value(CallRequest {
			to: Some(contract_address),
			data: Some(encoded_call.into()),
		})
		.map_err(|e| Error::RequestSerialization(e)),
		client
	);
	let (client, call_result) =
		bail_on_error!(call_rpc::<Bytes>(client, "eth_call", Params::Array(vec![call_request]),).await);
	match call_decoder.decode(&call_result.0) {
		Ok((incomplete_headers_numbers, incomplete_headers_hashes)) => (
			client,
			Ok(incomplete_headers_numbers
				.into_iter()
				.zip(incomplete_headers_hashes)
				.filter_map(|(number, hash)| {
					if number != number.low_u32().into() {
						return None;
					}

					Some(HeaderId(number.low_u32(), hash))
				})
				.collect()),
		),
		Err(error) => (client, Err(Error::ResponseParseFailed(format!("{}", error)))),
	}
}

/// Complete Substrate header.
pub async fn complete_substrate_header(
	client: Client,
	params: EthereumSigningParams,
	contract_address: Address,
	id: SubstrateHeaderId,
	justification: GrandpaJustification,
) -> (Client, Result<SubstrateHeaderId, Error>) {
	let (client, _) = bail_on_error!(
		submit_ethereum_transaction(
			client,
			&params,
			Some(contract_address),
			None,
			false,
			bridge_contract::functions::import_finality_proof::encode_input(id.0, id.1, justification,),
		)
		.await
	);

	(client, Ok(id))
}

/// Deploy bridge contract.
pub async fn deploy_bridge_contract(
	client: Client,
	params: &EthereumSigningParams,
	contract_code: Vec<u8>,
	initial_header: Vec<u8>,
	initial_set_id: u64,
	initial_authorities: Vec<u8>,
) -> (Client, Result<(), Error>) {
	submit_ethereum_transaction(
		client,
		params,
		None,
		None,
		false,
		bridge_contract::constructor(contract_code, initial_header, initial_set_id, initial_authorities),
	)
	.await
}

/// Submit ethereum transaction.
async fn submit_ethereum_transaction(
	client: Client,
	params: &EthereumSigningParams,
	contract_address: Option<Address>,
	nonce: Option<U256>,
	double_gas: bool,
	encoded_call: Vec<u8>,
) -> (Client, Result<(), Error>) {
	let (client, nonce) = match nonce {
		Some(nonce) => (client, nonce),
		None => bail_on_error!(account_nonce(client, params.signer.address().as_fixed_bytes().into()).await),
	};
	let (client, gas) = bail_on_error!(
		estimate_gas(
			client,
			CallRequest {
				to: contract_address,
				data: Some(encoded_call.clone().into()),
			}
		)
		.await
	);
	let raw_transaction = ethereum_tx_sign::RawTransaction {
		nonce,
		to: contract_address,
		value: U256::zero(),
		gas: if double_gas { gas.saturating_mul(2.into()) } else { gas },
		gas_price: params.gas_price,
		data: encoded_call,
	}
	.sign(&params.signer.secret().as_fixed_bytes().into(), &params.chain_id);
	let transaction = bail_on_arg_error!(
		to_value(Bytes(raw_transaction)).map_err(|e| Error::RequestSerialization(e)),
		client
	);
	let (client, _) = bail_on_error!(
		call_rpc::<TransactionHash>(client, "eth_submitTransaction", Params::Array(vec![transaction])).await
	);
	(client, Ok(()))
}

/// Get account nonce.
async fn account_nonce(client: Client, caller_address: Address) -> (Client, Result<U256, Error>) {
	let caller_address = bail_on_arg_error!(
		to_value(caller_address).map_err(|e| Error::RequestSerialization(e)),
		client
	);
	call_rpc(client, "eth_getTransactionCount", Params::Array(vec![caller_address])).await
}

/// Estimate gas usage for call.
async fn estimate_gas(client: Client, call_request: CallRequest) -> (Client, Result<U256, Error>) {
	let call_request = bail_on_arg_error!(
		to_value(call_request).map_err(|e| Error::RequestSerialization(e)),
		client
	);
	call_rpc(client, "eth_estimateGas", Params::Array(vec![call_request])).await
}

/// Calls RPC on Ethereum node.
async fn call_rpc<T: DeserializeOwned>(
	mut client: Client,
	method: &'static str,
	params: Params,
) -> (Client, Result<T, Error>) {
	async fn do_call_rpc<T: DeserializeOwned>(
		client: &mut Client,
		method: &'static str,
		params: Params,
	) -> Result<T, Error> {
		let request_id = client
			.start_request(method, params)
			.await
			.map_err(Error::StartRequestFailed)?;
		// WARN: if there'll be need for executing >1 request at a time, we should avoid
		// calling request_by_id
		let response = client
			.request_by_id(request_id)
			.ok_or(Error::RequestNotFound)?
			.await
			.map_err(Error::ResponseRetrievalFailed)?;
		from_value(response).map_err(|e| Error::ResponseParseFailed(format!("{}", e)))
	}

	let result = do_call_rpc(&mut client, method, params).await;
	(client, result)
}
