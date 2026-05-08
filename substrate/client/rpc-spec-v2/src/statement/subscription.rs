// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::statement::{
	error::Error,
	event::{NewStatementEntry, SubscribeEvent},
};
use futures::StreamExt;
use jsonrpsee::ConnectionId;
use parking_lot::{Mutex, RwLock};
use sc_rpc::utils::Subscription;
use sc_statement_store::{
	AddFilterError, LiveEventStream, MultiFilterSubscriptionEvent, SubscriptionHandle,
};
use sp_statement_store::{FilterId, OptimizedTopicFilter};
use std::{collections::HashMap, sync::Arc};

use crate::common::connections::{RegisteredConnection, RpcConnections};

pub const MAX_SUBSCRIPTIONS_PER_CONNECTION: usize = 16;

pub(crate) enum AddFilterOutcome {
	Added(FilterId),
	LimitReached,
}

/// Per-subscription state shared between RPC handlers and the subscription task
pub(crate) struct SubscriptionState {
	handle: SubscriptionHandle,
}

impl SubscriptionState {
	fn new(handle: SubscriptionHandle) -> Self {
		Self { handle }
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SubscriptionKey {
	conn_id: ConnectionId,
	sub_id: String,
}

impl SubscriptionKey {
	fn new(conn_id: ConnectionId, sub_id: impl Into<String>) -> Self {
		Self { conn_id, sub_id: sub_id.into() }
	}
}

type SubscriptionStateRef = Arc<Mutex<SubscriptionState>>;
type SubscriptionRegistry = Arc<RwLock<HashMap<SubscriptionKey, SubscriptionStateRef>>>;

/// Long-lived registry owned by the RPC server
#[derive(Clone, Default)]
pub struct StatementSubscriptions {
	rpc_connections: RpcConnections,
	registry: SubscriptionRegistry,
}

impl StatementSubscriptions {
	pub fn new() -> Self {
		Self {
			registry: Arc::new(RwLock::new(HashMap::new())),
			rpc_connections: RpcConnections::new(MAX_SUBSCRIPTIONS_PER_CONNECTION),
		}
	}

	/// Reserves a slot for a new subscription
	pub fn reserve(&self, conn_id: ConnectionId) -> Option<ReservedSubscription> {
		let reserved_token = self.rpc_connections.reserve_space(conn_id)?;
		Some(ReservedSubscription {
			conn_id,
			token: Some(reserved_token),
			registry: self.registry.clone(),
		})
	}

	/// Gets a subscription owned by the connection
	pub fn get(&self, conn_id: ConnectionId, sub_id: &str) -> Option<SubscriptionStateRef> {
		if !self.rpc_connections.contains_identifier(conn_id, sub_id) {
			return None;
		}
		self.registry.read().get(&SubscriptionKey::new(conn_id, sub_id)).cloned()
	}
}

/// Reservation for one pending subscription
pub struct ReservedSubscription {
	conn_id: ConnectionId,
	token: Option<crate::common::connections::ReservedConnection>,
	registry: SubscriptionRegistry,
}

impl ReservedSubscription {
	pub fn register(
		mut self,
		sub_id: String,
		handle: SubscriptionHandle,
	) -> Option<SubscriptionEntry> {
		let token = self.token.take()?;
		let registered = token.register(sub_id.clone())?;
		let state = Arc::new(Mutex::new(SubscriptionState::new(handle)));
		let key = SubscriptionKey::new(self.conn_id, sub_id);
		{
			let mut registry = self.registry.write();
			if registry.contains_key(&key) {
				return None;
			}
			registry.insert(key.clone(), state.clone());
		}
		Some(SubscriptionEntry { key, _registered: registered, registry: self.registry.clone() })
	}
}

/// Registered subscription entry
pub struct SubscriptionEntry {
	key: SubscriptionKey,
	_registered: RegisteredConnection,
	registry: SubscriptionRegistry,
}

impl Drop for SubscriptionEntry {
	fn drop(&mut self) {
		self.registry.write().remove(&self.key);
	}
}

pub(crate) fn add_filter_sync(
	state: &Arc<Mutex<SubscriptionState>>,
	filter: OptimizedTopicFilter,
) -> Result<AddFilterOutcome, Error> {
	let handle = state.lock().handle.clone();
	match handle.add_filter(filter) {
		Ok(filter_id) => Ok(AddFilterOutcome::Added(filter_id)),
		Err(AddFilterError::LimitReached) => Ok(AddFilterOutcome::LimitReached),
		Err(AddFilterError::Store(e)) => {
			Err(Error::InternalError(format!("add_filter failed: {e}")))
		},
	}
}

pub(crate) fn remove_filter_sync(
	state: &Arc<Mutex<SubscriptionState>>,
	filter_id: FilterId,
) -> bool {
	state.lock().handle.remove_filter(filter_id)
}

pub(crate) fn filter_id_to_string(id: FilterId) -> String {
	id.as_u64().to_string()
}
pub(crate) fn parse_filter_id(s: &str) -> Option<FilterId> {
	s.parse::<u64>().ok().map(FilterId::new)
}

pub async fn run_subscription_task(sink: Subscription, mut live_stream: LiveEventStream) {
	while let Some(event) = live_stream.next().await {
		if !send_subscription_event(&sink, event).await {
			return;
		}
	}
}

async fn send_subscription_event(sink: &Subscription, event: MultiFilterSubscriptionEvent) -> bool {
	match event {
		MultiFilterSubscriptionEvent::ReplayStatements { filter_id, statements } => {
			let statements = statements.into_iter().map(sp_core::Bytes).collect();
			send_event(
				sink,
				&SubscribeEvent::ReplayStatements {
					filter_id: filter_id_to_string(filter_id),
					statements,
				},
			)
			.await
		},
		MultiFilterSubscriptionEvent::ReplayDone { filter_id } => {
			send_event(
				sink,
				&SubscribeEvent::ReplayDone { filter_id: filter_id_to_string(filter_id) },
			)
			.await
		},
		MultiFilterSubscriptionEvent::NewStatement(event) => {
			let filter_ids =
				event.matched_filter_ids.into_iter().map(filter_id_to_string).collect();
			send_event(
				sink,
				&SubscribeEvent::NewStatements {
					statements: vec![NewStatementEntry {
						statement: sp_core::Bytes(event.encoded),
						filter_ids,
					}],
				},
			)
			.await
		},
		MultiFilterSubscriptionEvent::Stop => {
			let _ = send_event(sink, &SubscribeEvent::Stop).await;
			false
		},
	}
}

async fn send_event(sink: &Subscription, event: &SubscribeEvent) -> bool {
	match sink.send(event).await {
		Ok(()) => true,
		Err(_) => false,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_statement_store::{MultiFilterSubscriptionApi, Store};
	use std::sync::Arc;

	fn empty_subscription_state() -> Arc<Mutex<SubscriptionState>> {
		let dir = tempfile::tempdir().expect("tempdir");
		let mut db_path: std::path::PathBuf = dir.path().into();
		db_path.push("db");

		type Extrinsic = sp_runtime::OpaqueExtrinsic;
		type Hash = sp_core::H256;
		type Hashing = sp_runtime::traits::BlakeTwo256;
		type BlockNumber = u64;
		type Header = sp_runtime::generic::Header<BlockNumber, Hashing>;
		type Block = sp_runtime::generic::Block<Header, Extrinsic>;
		type MockBackend = sc_client_api::in_mem::Backend<Block>;

		#[derive(Clone)]
		struct TestClient;

		impl sc_client_api::StorageProvider<Block, MockBackend> for TestClient {
			fn storage(
				&self,
				_hash: Hash,
				_key: &sc_client_api::StorageKey,
			) -> sp_blockchain::Result<Option<sc_client_api::StorageData>> {
				use codec::Encode;
				let allowance =
					sp_statement_store::StatementAllowance { max_count: 1000, max_size: 1_000_000 };
				Ok(Some(sc_client_api::StorageData(allowance.encode())))
			}
			fn storage_hash(
				&self,
				_: Hash,
				_: &sc_client_api::StorageKey,
			) -> sp_blockchain::Result<Option<Hash>> {
				unimplemented!()
			}
			fn storage_keys(
				&self,
				_: Hash,
				_: Option<&sc_client_api::StorageKey>,
				_: Option<&sc_client_api::StorageKey>,
			) -> sp_blockchain::Result<
				sc_client_api::backend::KeysIter<
					<MockBackend as sc_client_api::Backend<Block>>::State,
					Block,
				>,
			> {
				unimplemented!()
			}
			fn storage_pairs(
				&self,
				_: Hash,
				_: Option<&sc_client_api::StorageKey>,
				_: Option<&sc_client_api::StorageKey>,
			) -> sp_blockchain::Result<
				sc_client_api::backend::PairsIter<
					<MockBackend as sc_client_api::Backend<Block>>::State,
					Block,
				>,
			> {
				unimplemented!()
			}
			fn child_storage(
				&self,
				_: Hash,
				_: &sc_client_api::ChildInfo,
				_: &sc_client_api::StorageKey,
			) -> sp_blockchain::Result<Option<sc_client_api::StorageData>> {
				unimplemented!()
			}
			fn child_storage_keys(
				&self,
				_: Hash,
				_: sc_client_api::ChildInfo,
				_: Option<&sc_client_api::StorageKey>,
				_: Option<&sc_client_api::StorageKey>,
			) -> sp_blockchain::Result<
				sc_client_api::backend::KeysIter<
					<MockBackend as sc_client_api::Backend<Block>>::State,
					Block,
				>,
			> {
				unimplemented!()
			}
			fn child_storage_hash(
				&self,
				_: Hash,
				_: &sc_client_api::ChildInfo,
				_: &sc_client_api::StorageKey,
			) -> sp_blockchain::Result<Option<Hash>> {
				unimplemented!()
			}
			fn closest_merkle_value(
				&self,
				_: Hash,
				_: &sc_client_api::StorageKey,
			) -> sp_blockchain::Result<Option<sc_client_api::MerkleValue<Hash>>> {
				unimplemented!()
			}
			fn child_closest_merkle_value(
				&self,
				_: Hash,
				_: &sc_client_api::ChildInfo,
				_: &sc_client_api::StorageKey,
			) -> sp_blockchain::Result<Option<sc_client_api::MerkleValue<Hash>>> {
				unimplemented!()
			}
		}

		impl sp_blockchain::HeaderBackend<Block> for TestClient {
			fn header(&self, _: Hash) -> sp_blockchain::Result<Option<Header>> {
				unimplemented!()
			}
			fn info(&self) -> sp_blockchain::Info<Block> {
				let h = sp_core::H256::repeat_byte(1);
				sp_blockchain::Info {
					best_hash: h,
					best_number: 0,
					genesis_hash: Default::default(),
					finalized_hash: h,
					finalized_number: 1,
					finalized_state: None,
					number_leaves: 0,
					block_gap: None,
				}
			}
			fn status(&self, _: Hash) -> sp_blockchain::Result<sp_blockchain::BlockStatus> {
				unimplemented!()
			}
			fn number(&self, _: Hash) -> sp_blockchain::Result<Option<BlockNumber>> {
				unimplemented!()
			}
			fn hash(&self, _: BlockNumber) -> sp_blockchain::Result<Option<Hash>> {
				unimplemented!()
			}
		}

		let store = Arc::new(
			Store::new::<Block, TestClient, MockBackend>(
				&db_path,
				Default::default(),
				Arc::new(TestClient),
				Arc::new(sc_keystore::LocalKeystore::in_memory()),
				None,
				Box::new(sp_core::testing::TaskExecutor::new()),
			)
			.expect("store"),
		);
		std::mem::forget(dir);

		let (handle, _live) = store.create_subscription();
		Arc::new(Mutex::new(SubscriptionState::new(handle)))
	}

	#[test]
	fn subscription_registry_is_scoped_by_connection_id() {
		let subscriptions = StatementSubscriptions::new();
		let conn_a = ConnectionId(1);
		let conn_b = ConnectionId(2);
		let sub_id = "same-subscription-id".to_string();

		let handle_a = empty_subscription_state().lock().handle.clone();
		let handle_b = empty_subscription_state().lock().handle.clone();

		let entry_a = subscriptions
			.reserve(conn_a)
			.unwrap()
			.register(sub_id.clone(), handle_a)
			.unwrap();
		let _entry_b = subscriptions
			.reserve(conn_b)
			.unwrap()
			.register(sub_id.clone(), handle_b)
			.unwrap();

		let state_a = subscriptions.get(conn_a, &sub_id).unwrap();
		let state_b = subscriptions.get(conn_b, &sub_id).unwrap();
		assert!(!Arc::ptr_eq(&state_a, &state_b));

		drop(entry_a);
		assert!(subscriptions.get(conn_a, &sub_id).is_none());
		assert!(subscriptions.get(conn_b, &sub_id).is_some());
	}
}
