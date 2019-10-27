// Copyright 2017-2019 Parity Technologies (UK) Ltd.
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

//! Substrate block-author/full-node API.

pub mod error;
pub mod hash;

use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{typed::Subscriber, SubscriptionId};
use primitives::{
	Bytes
};
use self::error::{FutureResult, Result};
use txpool::watcher::Status;

pub use self::gen_client::Client as AuthorClient;

/// Substrate authoring RPC API
#[rpc]
pub trait AuthorApi<Hash, BlockHash> {
	/// RPC metadata
	type Metadata;

	/// Submit hex-encoded extrinsic for inclusion in block.
	#[rpc(name = "author_submitExtrinsic")]
	fn submit_extrinsic(&self, extrinsic: Bytes) -> FutureResult<Hash>;

	/// Insert a key into the keystore.
	#[rpc(name = "author_insertKey")]
	fn insert_key(&self,
		key_type: String,
		suri: String,
		public: Bytes,
	) -> Result<()>;

	/// Generate new session keys and returns the corresponding public keys.
	#[rpc(name = "author_rotateKeys")]
	fn rotate_keys(&self) -> Result<Bytes>;

	/// Returns all pending extrinsics, potentially grouped by sender.
	#[rpc(name = "author_pendingExtrinsics")]
	fn pending_extrinsics(&self) -> Result<Vec<Bytes>>;

	/// Remove given extrinsic from the pool and temporarily ban it to prevent reimporting.
	#[rpc(name = "author_removeExtrinsic")]
	fn remove_extrinsic(&self,
		bytes_or_hash: Vec<hash::ExtrinsicOrHash<Hash>>
	) -> Result<Vec<Hash>>;

	/// Submit an extrinsic to watch.
	#[pubsub(
		subscription = "author_extrinsicUpdate",
		subscribe,
		name = "author_submitAndWatchExtrinsic"
	)]
	fn watch_extrinsic(&self,
		metadata: Self::Metadata,
		subscriber: Subscriber<Status<Hash, BlockHash>>,
		bytes: Bytes
	);

	/// Unsubscribe from extrinsic watching.
	#[pubsub(
		subscription = "author_extrinsicUpdate",
		unsubscribe,
		name = "author_unwatchExtrinsic"
	)]
	fn unwatch_extrinsic(&self,
		metadata: Option<Self::Metadata>,
		id: SubscriptionId
	) -> Result<bool>;
}
