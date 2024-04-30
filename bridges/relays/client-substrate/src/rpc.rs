// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! The most generic Substrate node RPC interface.

use async_trait::async_trait;

use crate::{Chain, ChainWithGrandpa, TransactionStatusOf};

use jsonrpsee::{
	core::{client::Subscription, ClientError},
	proc_macros::rpc,
	ws_client::WsClient,
};
use pallet_transaction_payment_rpc_runtime_api::FeeDetails;
use sc_rpc_api::{state::ReadProof, system::Health};
use sp_core::{
	storage::{StorageData, StorageKey},
	Bytes,
};
use sp_rpc::number::NumberOrHex;
use sp_version::RuntimeVersion;

/// RPC methods of Substrate `system` namespace, that we are using.
#[rpc(client, client_bounds(C: Chain), namespace = "system")]
pub(crate) trait SubstrateSystem<C> {
	/// Return node health.
	#[method(name = "health")]
	async fn health(&self) -> RpcResult<Health>;
	/// Return system properties.
	#[method(name = "properties")]
	async fn properties(&self) -> RpcResult<sc_chain_spec::Properties>;
}

/// RPC methods of Substrate `chain` namespace, that we are using.
#[rpc(client, client_bounds(C: Chain), namespace = "chain")]
pub(crate) trait SubstrateChain<C> {
	/// Get block hash by its number.
	#[method(name = "getBlockHash")]
	async fn block_hash(&self, block_number: Option<C::BlockNumber>) -> RpcResult<C::Hash>;
	/// Return block header by its hash.
	#[method(name = "getHeader")]
	async fn header(&self, block_hash: Option<C::Hash>) -> RpcResult<C::Header>;
	/// Return best finalized block hash.
	#[method(name = "getFinalizedHead")]
	async fn finalized_head(&self) -> RpcResult<C::Hash>;
	/// Return signed block (with justifications) by its hash.
	#[method(name = "getBlock")]
	async fn block(&self, block_hash: Option<C::Hash>) -> RpcResult<C::SignedBlock>;
}

/// RPC methods of Substrate `author` namespace, that we are using.
#[rpc(client, client_bounds(C: Chain), namespace = "author")]
pub(crate) trait SubstrateAuthor<C> {
	/// Submit extrinsic to the transaction pool.
	#[method(name = "submitExtrinsic")]
	async fn submit_extrinsic(&self, extrinsic: Bytes) -> RpcResult<C::Hash>;
	/// Return vector of pending extrinsics from the transaction pool.
	#[method(name = "pendingExtrinsics")]
	async fn pending_extrinsics(&self) -> RpcResult<Vec<Bytes>>;
	/// Submit and watch for extrinsic state.
	#[subscription(name = "submitAndWatchExtrinsic", unsubscribe = "unwatchExtrinsic", item = TransactionStatusOf<C>)]
	async fn submit_and_watch_extrinsic(&self, extrinsic: Bytes);
}

/// RPC methods of Substrate `state` namespace, that we are using.
#[rpc(client, client_bounds(C: Chain), namespace = "state")]
pub(crate) trait SubstrateState<C> {
	/// Get current runtime version.
	#[method(name = "getRuntimeVersion")]
	async fn runtime_version(&self) -> RpcResult<RuntimeVersion>;
	/// Call given runtime method.
	#[method(name = "call")]
	async fn call(
		&self,
		method: String,
		data: Bytes,
		at_block: Option<C::Hash>,
	) -> RpcResult<Bytes>;
	/// Get value of the runtime storage.
	#[method(name = "getStorage")]
	async fn storage(
		&self,
		key: StorageKey,
		at_block: Option<C::Hash>,
	) -> RpcResult<Option<StorageData>>;
	/// Get proof of the runtime storage value.
	#[method(name = "getReadProof")]
	async fn prove_storage(
		&self,
		keys: Vec<StorageKey>,
		hash: Option<C::Hash>,
	) -> RpcResult<ReadProof<C::Hash>>;
}

/// RPC methods that we are using for a certain finality gadget.
#[async_trait]
pub trait SubstrateFinalityClient<C: Chain> {
	/// Subscribe to finality justifications.
	async fn subscribe_justifications(
		client: &WsClient,
	) -> Result<Subscription<Bytes>, ClientError>;
}

/// RPC methods of Substrate `grandpa` namespace, that we are using.
#[rpc(client, client_bounds(C: ChainWithGrandpa), namespace = "grandpa")]
pub(crate) trait SubstrateGrandpa<C> {
	/// Subscribe to GRANDPA justifications.
	#[subscription(name = "subscribeJustifications", unsubscribe = "unsubscribeJustifications", item = Bytes)]
	async fn subscribe_justifications(&self);
}

/// RPC finality methods of Substrate `grandpa` namespace, that we are using.
pub struct SubstrateGrandpaFinalityClient;
#[async_trait]
impl<C: ChainWithGrandpa> SubstrateFinalityClient<C> for SubstrateGrandpaFinalityClient {
	async fn subscribe_justifications(
		client: &WsClient,
	) -> Result<Subscription<Bytes>, ClientError> {
		SubstrateGrandpaClient::<C>::subscribe_justifications(client).await
	}
}

// TODO: Use `ChainWithBeefy` instead of `Chain` after #1606 is merged
/// RPC methods of Substrate `beefy` namespace, that we are using.
#[rpc(client, client_bounds(C: Chain), namespace = "beefy")]
pub(crate) trait SubstrateBeefy<C> {
	/// Subscribe to BEEFY justifications.
	#[subscription(name = "subscribeJustifications", unsubscribe = "unsubscribeJustifications", item = Bytes)]
	async fn subscribe_justifications(&self);
}

/// RPC finality methods of Substrate `beefy` namespace, that we are using.
pub struct SubstrateBeefyFinalityClient;
// TODO: Use `ChainWithBeefy` instead of `Chain` after #1606 is merged
#[async_trait]
impl<C: Chain> SubstrateFinalityClient<C> for SubstrateBeefyFinalityClient {
	async fn subscribe_justifications(
		client: &WsClient,
	) -> Result<Subscription<Bytes>, ClientError> {
		SubstrateBeefyClient::<C>::subscribe_justifications(client).await
	}
}

/// RPC methods of Substrate `system` frame pallet, that we are using.
#[rpc(client, client_bounds(C: Chain), namespace = "system")]
pub(crate) trait SubstrateFrameSystem<C> {
	/// Return index of next account transaction.
	#[method(name = "accountNextIndex")]
	async fn account_next_index(&self, account_id: C::AccountId) -> RpcResult<C::Nonce>;
}

/// RPC methods of Substrate `pallet_transaction_payment` frame pallet, that we are using.
#[rpc(client, client_bounds(C: Chain), namespace = "payment")]
pub(crate) trait SubstrateTransactionPayment<C> {
	/// Query transaction fee details.
	#[method(name = "queryFeeDetails")]
	async fn fee_details(
		&self,
		extrinsic: Bytes,
		at_block: Option<C::Hash>,
	) -> RpcResult<FeeDetails<NumberOrHex>>;
}
