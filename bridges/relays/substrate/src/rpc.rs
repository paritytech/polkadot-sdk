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

use jsonrpsee::core::client::{RawClient, RawClientError, TransportClient};
use node_primitives::{BlockNumber, Hash, Header};
use sp_core::Bytes;
use sp_rpc::number::NumberOrHex;

jsonrpsee::rpc_api! {
	pub SubstrateRPC {
		#[rpc(method = "author_submitExtrinsic", positional_params)]
		fn author_submit_extrinsic(extrinsic: Bytes) -> Hash;

		#[rpc(method = "chain_getFinalizedHead")]
		fn chain_finalized_head() -> Hash;

		#[rpc(method = "chain_getBlockHash", positional_params)]
		fn chain_block_hash(id: Option<NumberOrHex<BlockNumber>>) -> Option<Hash>;

		#[rpc(method = "chain_getHeader", positional_params)]
		fn chain_header(hash: Option<Hash>) -> Option<Header>;

		#[rpc(positional_params)]
		fn state_call(name: String, bytes: Bytes, hash: Option<Hash>) -> Bytes;
	}
}

pub async fn genesis_block_hash<R: TransportClient>(client: &mut RawClient<R>)
	-> Result<Option<Hash>, RawClientError<R::Error>>
{
	SubstrateRPC::chain_block_hash(client, Some(NumberOrHex::Number(0))).await
}
