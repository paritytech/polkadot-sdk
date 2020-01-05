// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate state API.

pub mod error;

use jsonrpc_core::Result as RpcResult;
use jsonrpc_core::futures::Future;
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{typed::Subscriber, SubscriptionId};
use sp_core::Bytes;
use sp_core::storage::{StorageKey, StorageData, StorageChangeSet};
use sp_version::RuntimeVersion;
use self::error::FutureResult;

pub use self::gen_client::Client as StateClient;

/// Substrate state API
#[rpc]
pub trait StateApi<Hash> {
	/// RPC Metadata
	type Metadata;

	/// Call a contract at a block's state.
	#[rpc(name = "state_call", alias("state_callAt"))]
	fn call(&self, name: String, bytes: Bytes, hash: Option<Hash>) -> FutureResult<Bytes>;

	/// Returns the keys with prefix, leave empty to get all the keys
	#[rpc(name = "state_getKeys")]
	fn storage_keys(&self, prefix: StorageKey, hash: Option<Hash>) -> FutureResult<Vec<StorageKey>>;

	/// Returns a storage entry at a specific block's state.
	#[rpc(name = "state_getStorage", alias("state_getStorageAt"))]
	fn storage(&self, key: StorageKey, hash: Option<Hash>) -> FutureResult<Option<StorageData>>;

	/// Returns the hash of a storage entry at a block's state.
	#[rpc(name = "state_getStorageHash", alias("state_getStorageHashAt"))]
	fn storage_hash(&self, key: StorageKey, hash: Option<Hash>) -> FutureResult<Option<Hash>>;

	/// Returns the size of a storage entry at a block's state.
	#[rpc(name = "state_getStorageSize", alias("state_getStorageSizeAt"))]
	fn storage_size(&self, key: StorageKey, hash: Option<Hash>) -> FutureResult<Option<u64>>;

	/// Returns the keys with prefix from a child storage, leave empty to get all the keys
	#[rpc(name = "state_getChildKeys")]
	fn child_storage_keys(
		&self,
		child_storage_key: StorageKey,
		child_info: StorageKey,
		child_type: u32,
		prefix: StorageKey,
		hash: Option<Hash>
	) -> FutureResult<Vec<StorageKey>>;

	/// Returns a child storage entry at a specific block's state.
	#[rpc(name = "state_getChildStorage")]
	fn child_storage(
		&self,
		child_storage_key: StorageKey,
		child_info: StorageKey,
		child_type: u32,
		key: StorageKey,
		hash: Option<Hash>
	) -> FutureResult<Option<StorageData>>;

	/// Returns the hash of a child storage entry at a block's state.
	#[rpc(name = "state_getChildStorageHash")]
	fn child_storage_hash(
		&self,
		child_storage_key: StorageKey,
		child_info: StorageKey,
		child_type: u32,
		key: StorageKey,
		hash: Option<Hash>
	) -> FutureResult<Option<Hash>>;

	/// Returns the size of a child storage entry at a block's state.
	#[rpc(name = "state_getChildStorageSize")]
	fn child_storage_size(
		&self,
		child_storage_key: StorageKey,
		child_info: StorageKey,
		child_type: u32,
		key: StorageKey,
		hash: Option<Hash>
	) -> FutureResult<Option<u64>>;

	/// Returns the runtime metadata as an opaque blob.
	#[rpc(name = "state_getMetadata")]
	fn metadata(&self, hash: Option<Hash>) -> FutureResult<Bytes>;

	/// Get the runtime version.
	#[rpc(name = "state_getRuntimeVersion", alias("chain_getRuntimeVersion"))]
	fn runtime_version(&self, hash: Option<Hash>) -> FutureResult<RuntimeVersion>;

	/// Query historical storage entries (by key) starting from a block given as the second parameter.
	///
	/// NOTE This first returned result contains the initial state of storage for all keys.
	/// Subsequent values in the vector represent changes to the previous state (diffs).
	#[rpc(name = "state_queryStorage")]
	fn query_storage(
		&self,
		keys: Vec<StorageKey>,
		block: Hash,
		hash: Option<Hash>
	) -> FutureResult<Vec<StorageChangeSet<Hash>>>;

	/// New runtime version subscription
	#[pubsub(
		subscription = "state_runtimeVersion",
		subscribe,
		name = "state_subscribeRuntimeVersion",
		alias("chain_subscribeRuntimeVersion")
	)]
	fn subscribe_runtime_version(&self, metadata: Self::Metadata, subscriber: Subscriber<RuntimeVersion>);

	/// Unsubscribe from runtime version subscription
	#[pubsub(
		subscription = "state_runtimeVersion",
		unsubscribe,
		name = "state_unsubscribeRuntimeVersion",
		alias("chain_unsubscribeRuntimeVersion")
	)]
	fn unsubscribe_runtime_version(&self, metadata: Option<Self::Metadata>, id: SubscriptionId) -> RpcResult<bool>;

	/// New storage subscription
	#[pubsub(subscription = "state_storage", subscribe, name = "state_subscribeStorage")]
	fn subscribe_storage(
		&self, metadata: Self::Metadata, subscriber: Subscriber<StorageChangeSet<Hash>>, keys: Option<Vec<StorageKey>>
	);

	/// Unsubscribe from storage subscription
	#[pubsub(subscription = "state_storage", unsubscribe, name = "state_unsubscribeStorage")]
	fn unsubscribe_storage(
		&self, metadata: Option<Self::Metadata>, id: SubscriptionId
	) -> RpcResult<bool>;
}
