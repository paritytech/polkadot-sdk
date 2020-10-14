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

//! Substrate node client.

use crate::chain::Chain;
use crate::rpc::Substrate;
use crate::{ConnectionParams, Result};

use jsonrpsee::common::DeserializeOwned;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::ws::WsTransportClient;
use jsonrpsee::Client as RpcClient;
use num_traits::Zero;
use sp_core::Bytes;

const SUB_API_GRANDPA_AUTHORITIES: &str = "GrandpaApi_grandpa_authorities";

/// Opaque GRANDPA authorities set.
pub type OpaqueGrandpaAuthoritiesSet = Vec<u8>;

/// Substrate client type.
pub struct Client<C: Chain> {
	/// Substrate RPC client.
	client: RpcClient,
	/// Genesis block hash.
	genesis_hash: C::Hash,
}

impl<C: Chain> std::fmt::Debug for Client<C> {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		fmt.debug_struct("Client")
			.field("genesis_hash", &self.genesis_hash)
			.finish()
	}
}

impl<C: Chain> Client<C> {
	/// Returns client that is able to call RPCs on Substrate node over websocket connection.
	pub async fn new(params: ConnectionParams) -> Result<Self> {
		let uri = format!("ws://{}:{}", params.host, params.port);
		let transport = WsTransportClient::new(&uri).await?;
		let raw_client = RawClient::new(transport);
		let client: RpcClient = raw_client.into();

		let number: C::BlockNumber = Zero::zero();
		let genesis_hash = Substrate::<C, _, _>::chain_get_block_hash(&client, number).await?;

		Ok(Self { client, genesis_hash })
	}
}

impl<C: Chain> Client<C>
where
	C::Header: DeserializeOwned,
	C::Index: DeserializeOwned,
{
	/// Return hash of the genesis block.
	pub fn genesis_hash(&self) -> &C::Hash {
		&self.genesis_hash
	}

	/// Returns the best Substrate header.
	pub async fn best_header(&self) -> Result<C::Header> {
		Ok(Substrate::<C, _, _>::chain_get_header(&self.client, None).await?)
	}

	/// Get a Substrate block from its hash.
	pub async fn get_block(&self, block_hash: Option<C::Hash>) -> Result<C::SignedBlock> {
		Ok(Substrate::<C, _, _>::chain_get_block(&self.client, block_hash).await?)
	}

	/// Get a Substrate header by its hash.
	pub async fn header_by_hash(&self, block_hash: C::Hash) -> Result<C::Header> {
		Ok(Substrate::<C, _, _>::chain_get_header(&self.client, block_hash).await?)
	}

	/// Get a Substrate block hash by its number.
	pub async fn block_hash_by_number(&self, number: C::BlockNumber) -> Result<C::Hash> {
		Ok(Substrate::<C, _, _>::chain_get_block_hash(&self.client, number).await?)
	}

	/// Get a Substrate header by its number.
	pub async fn header_by_number(&self, block_number: C::BlockNumber) -> Result<C::Header> {
		let block_hash = Self::block_hash_by_number(self, block_number).await?;
		Ok(Self::header_by_hash(self, block_hash).await?)
	}

	/// Get the nonce of the given Substrate account.
	///
	/// Note: It's the caller's responsibility to make sure `account` is a valid ss58 address.
	pub async fn next_account_index(&self, account: C::AccountId) -> Result<C::Index> {
		Ok(Substrate::<C, _, _>::system_account_next_index(&self.client, account).await?)
	}

	/// Submit an extrinsic for inclusion in a block.
	///
	/// Note: The given transaction does not need be SCALE encoded beforehand.
	pub async fn submit_extrinsic(&self, transaction: Bytes) -> Result<C::Hash> {
		let tx_hash = Substrate::<C, _, _>::author_submit_extrinsic(&self.client, transaction).await?;
		log::trace!(target: "bridge", "Sent transaction to Substrate node: {:?}", tx_hash);
		Ok(tx_hash)
	}

	/// Get the GRANDPA authority set at given block.
	pub async fn grandpa_authorities_set(&self, block: C::Hash) -> Result<OpaqueGrandpaAuthoritiesSet> {
		let call = SUB_API_GRANDPA_AUTHORITIES.to_string();
		let data = Bytes(Vec::new());

		let encoded_response = Substrate::<C, _, _>::state_call(&self.client, call, data, Some(block)).await?;
		let authority_list = encoded_response.0;

		Ok(authority_list)
	}

	/// Execute runtime call at given block.
	pub async fn state_call(&self, method: String, data: Bytes, at_block: Option<C::Hash>) -> Result<Bytes> {
		Substrate::<C, _, _>::state_call(&self.client, method, data, at_block)
			.await
			.map_err(Into::into)
	}
}
