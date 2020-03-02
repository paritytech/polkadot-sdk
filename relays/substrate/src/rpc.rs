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
