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
use codec::Decode;
use futures::StreamExt;
use jsonrpsee::ConnectionId;
use parking_lot::{Mutex, RwLock};
use sc_rpc::utils::Subscription;
use sc_statement_store::{AddFilterError, LiveEventStream, SubscriptionHandle};
use sp_statement_store::{hash_encoded, FilterId, Hash, OptimizedTopicFilter, Statement};
use std::{
	collections::{HashMap, HashSet, VecDeque},
	sync::Arc,
};
use tokio::sync::mpsc;

use crate::common::connections::{RegisteredConnection, RpcConnections};

pub const MAX_SUBSCRIPTIONS_PER_CONNECTION: usize = 16;
const REPLAY_CHUNK_BYTES: usize = 4 * 1024 * 1024;

#[cfg(not(test))]
pub(crate) const PENDING_LIVE_HARD_CAP: usize = 64 * 1024;
#[cfg(test)]
pub(crate) const PENDING_LIVE_HARD_CAP: usize = 4;
#[cfg(not(test))]
pub(crate) const EMITTED_VIA_NEW_HARD_CAP: usize = 64 * 1024;
#[cfg(test)]
pub(crate) const EMITTED_VIA_NEW_HARD_CAP: usize = 4;

pub(crate) enum ControlMessage {
	Wake,
}

pub(crate) enum AddFilterOutcome {
	Added(FilterId),
	LimitReached,
}

struct PendingReplay {
	filter_id: FilterId,
	snapshot: Vec<Vec<u8>>,
}

struct PendingLiveStatement {
	hash: Hash,
	encoded: Vec<u8>,
}

/// Per-subscription state shared between RPC handlers and the subscription task
pub(crate) struct SubscriptionState {
	handle: SubscriptionHandle,
	control_tx: mpsc::UnboundedSender<ControlMessage>,
	replays_in_progress: HashSet<FilterId>,
	cancelled_replays: HashSet<FilterId>,
	pending_replays: VecDeque<PendingReplay>,
	pending_live: VecDeque<PendingLiveStatement>,
	replayed_filter_ids_by_hash: HashMap<Hash, HashSet<FilterId>>,
	new_emitted_hashes: HashSet<Hash>,
	stopped: bool,
}

impl SubscriptionState {
	fn new(handle: SubscriptionHandle, control_tx: mpsc::UnboundedSender<ControlMessage>) -> Self {
		Self {
			handle,
			control_tx,
			replays_in_progress: HashSet::new(),
			cancelled_replays: HashSet::new(),
			pending_replays: VecDeque::new(),
			pending_live: VecDeque::new(),
			replayed_filter_ids_by_hash: HashMap::new(),
			new_emitted_hashes: HashSet::new(),
			stopped: false,
		}
	}

	fn record_filter_added(&mut self, filter_id: FilterId, snapshot: Vec<Vec<u8>>) {
		self.replays_in_progress.insert(filter_id);
		for encoded in &snapshot {
			let hash = hash_encoded(encoded);
			self.replayed_filter_ids_by_hash.entry(hash).or_default().insert(filter_id);
		}
		self.pending_replays.push_back(PendingReplay { filter_id, snapshot });
	}

	fn record_filter_removed(&mut self, filter_id: FilterId) -> bool {
		let was_in_progress = self.replays_in_progress.remove(&filter_id);
		if was_in_progress {
			self.cancelled_replays.insert(filter_id);
		}
		self.replayed_filter_ids_by_hash.retain(|_hash, set| {
			set.remove(&filter_id);
			!set.is_empty()
		});
		was_in_progress
	}

	#[cfg(test)]
	pub(crate) fn fill_pending_live_for_overflow_test(&mut self, filter: FilterId, count: usize) {
		self.replays_in_progress.insert(filter);
		for i in 0..count {
			self.pending_live
				.push_back(PendingLiveStatement { hash: [i as u8; 32], encoded: vec![i as u8] });
		}
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
		control_tx: mpsc::UnboundedSender<ControlMessage>,
	) -> Option<SubscriptionEntry> {
		let token = self.token.take()?;
		let registered = token.register(sub_id.clone())?;
		let state = Arc::new(Mutex::new(SubscriptionState::new(handle, control_tx)));
		let key = SubscriptionKey::new(self.conn_id, sub_id);
		{
			let mut registry = self.registry.write();
			if registry.contains_key(&key) {
				return None;
			}
			registry.insert(key.clone(), state.clone());
		}
		Some(SubscriptionEntry {
			key,
			state,
			_registered: registered,
			registry: self.registry.clone(),
		})
	}
}

/// Registered subscription entry
pub struct SubscriptionEntry {
	key: SubscriptionKey,
	state: SubscriptionStateRef,
	_registered: RegisteredConnection,
	registry: SubscriptionRegistry,
}

impl SubscriptionEntry {
	pub fn state(&self) -> &SubscriptionStateRef {
		&self.state
	}
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
	let (control_tx, filter_id) = {
		let mut subscription = state.lock();
		let handle = subscription.handle.clone();
		match handle.add_filter(filter) {
			Ok((filter_id, snapshot)) => {
				subscription.record_filter_added(filter_id, snapshot);
				(subscription.control_tx.clone(), filter_id)
			},
			Err(AddFilterError::LimitReached) => return Ok(AddFilterOutcome::LimitReached),
			Err(AddFilterError::Store(e)) => {
				return Err(Error::InternalError(format!("add_filter failed: {e}")))
			},
		}
	};
	let _ = control_tx.send(ControlMessage::Wake);
	Ok(AddFilterOutcome::Added(filter_id))
}

pub(crate) fn remove_filter_sync(
	state: &Arc<Mutex<SubscriptionState>>,
	filter_id: FilterId,
) -> bool {
	let (was_present, control_tx) = {
		let mut subscription = state.lock();
		let _ = subscription.handle.remove_filter(filter_id);
		(subscription.record_filter_removed(filter_id), subscription.control_tx.clone())
	};
	let _ = control_tx.send(ControlMessage::Wake);
	was_present
}

pub(crate) fn filter_id_to_string(id: FilterId) -> String {
	id.as_u64().to_string()
}
pub(crate) fn parse_filter_id(s: &str) -> Option<FilterId> {
	s.parse::<u64>().ok().map(FilterId::new)
}

pub async fn run_subscription_task(
	sink: Subscription,
	state: Arc<Mutex<SubscriptionState>>,
	mut live_stream: LiveEventStream,
	mut control_rx: mpsc::UnboundedReceiver<ControlMessage>,
) {
	loop {
		if !drain_pending_replays(&sink, &state).await {
			return;
		}
		if !drain_pending_live(&sink, &state).await {
			return;
		}
		if state.lock().stopped {
			return;
		}

		tokio::select! {
			biased;
			msg = control_rx.recv() => match msg {
				Some(ControlMessage::Wake) => continue,
				None => return,
			},
			event = live_stream.next() => match event {
				Some(event) => {
					if !handle_live_event(&sink, &state, event).await {
						return;
					}
				},
				None => return,
			},
		}
	}
}

#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) enum LiveAction {
	Stop,
	Noop,
	Emit(NewStatementEntry),
}

pub(crate) fn decide_live_action(
	subscription: &mut SubscriptionState,
	event: sp_statement_store::LiveStatementEvent,
) -> LiveAction {
	if subscription.stopped {
		return LiveAction::Noop;
	}

	let matched_filters: HashSet<FilterId> = match Statement::decode(&mut &event.encoded[..]) {
		Ok(stmt) => subscription.handle.matched_filter_ids(&stmt).into_iter().collect(),
		Err(_) => {
			log::warn!(
				target: super::LOG_TARGET,
				"Corrupt statement bytes received on live stream; skipping",
			);
			return LiveAction::Noop;
		},
	};

	if matched_filters.iter().any(|f| subscription.replays_in_progress.contains(f)) {
		if subscription.pending_live.len() >= PENDING_LIVE_HARD_CAP {
			log::warn!(
				target: super::LOG_TARGET,
				"pending_live cap reached on statement subscription; sending stop",
			);
			subscription.stopped = true;
			LiveAction::Stop
		} else {
			subscription
				.pending_live
				.push_back(PendingLiveStatement { hash: event.hash, encoded: event.encoded });
			LiveAction::Noop
		}
	} else {
		compute_new_statements_action(subscription, event.hash, event.encoded, &matched_filters)
	}
}

async fn handle_live_event(
	sink: &Subscription,
	state: &Arc<Mutex<SubscriptionState>>,
	event: sp_statement_store::LiveStatementEvent,
) -> bool {
	let action = {
		let mut subscription = state.lock();
		decide_live_action(&mut subscription, event)
	};

	match action {
		LiveAction::Stop => {
			let _ = send_event(sink, &SubscribeEvent::Stop).await;
			false
		},
		LiveAction::Noop => true,
		LiveAction::Emit(entry) => {
			send_event(sink, &SubscribeEvent::NewStatements { statements: vec![entry] }).await
		},
	}
}

fn compute_new_statements_action(
	subscription: &mut SubscriptionState,
	hash: Hash,
	encoded: Vec<u8>,
	filter_ids: &HashSet<FilterId>,
) -> LiveAction {
	if subscription.new_emitted_hashes.contains(&hash) {
		return LiveAction::Noop;
	}
	let replayed_filter_ids = subscription.replayed_filter_ids_by_hash.get(&hash);
	let filter_ids: Vec<String> = filter_ids
		.iter()
		.filter(|f| replayed_filter_ids.map_or(true, |set| !set.contains(f)))
		.map(|f| filter_id_to_string(*f))
		.collect();
	if filter_ids.is_empty() {
		return LiveAction::Noop;
	}
	if subscription.new_emitted_hashes.len() >= EMITTED_VIA_NEW_HARD_CAP {
		log::warn!(
			target: super::LOG_TARGET,
			"new_emitted_hashes cap reached on statement subscription; sending stop",
		);
		subscription.stopped = true;
		return LiveAction::Stop;
	}
	subscription.new_emitted_hashes.insert(hash);
	LiveAction::Emit(NewStatementEntry { statement: sp_core::Bytes(encoded), filter_ids })
}

async fn drain_pending_replays(sink: &Subscription, state: &Arc<Mutex<SubscriptionState>>) -> bool {
	loop {
		let next = state.lock().pending_replays.pop_front();
		let Some(replay) = next else { return true };
		if !run_replay(sink, state, replay).await {
			return false;
		}
	}
}

async fn run_replay(
	sink: &Subscription,
	state: &Arc<Mutex<SubscriptionState>>,
	replay: PendingReplay,
) -> bool {
	let filter_id = replay.filter_id;
	let filter_id_str = filter_id_to_string(filter_id);
	let mut iter = replay.snapshot.into_iter().peekable();

	while iter.peek().is_some() {
		if state.lock().cancelled_replays.contains(&filter_id) {
			let mut subscription = state.lock();
			subscription.cancelled_replays.remove(&filter_id);
			return true;
		}

		let mut chunk: Vec<sp_core::Bytes> = Vec::new();
		let mut chunk_bytes: usize = 0;
		while let Some(stmt) = iter.peek() {
			let est = stmt.len().saturating_mul(2).saturating_add(8);
			if !chunk.is_empty() && chunk_bytes + est > REPLAY_CHUNK_BYTES {
				break;
			}
			let bytes = iter.next().expect("peek above; qed");
			chunk_bytes = chunk_bytes.saturating_add(est);
			chunk.push(sp_core::Bytes(bytes));
			if chunk_bytes >= REPLAY_CHUNK_BYTES {
				break;
			}
		}
		debug_assert!(!chunk.is_empty(), "loop guard; qed");

		let event = SubscribeEvent::ReplayStatements {
			filter_id: filter_id_str.clone(),
			statements: chunk,
		};
		if !send_event(sink, &event).await {
			return false;
		}
	}

	let cancelled = {
		let mut subscription = state.lock();
		if subscription.cancelled_replays.remove(&filter_id) {
			true
		} else {
			subscription.replays_in_progress.remove(&filter_id);
			false
		}
	};
	if cancelled {
		return true;
	}
	send_event(sink, &SubscribeEvent::ReplayDone { filter_id: filter_id_str }).await
}

async fn drain_pending_live(sink: &Subscription, state: &Arc<Mutex<SubscriptionState>>) -> bool {
	loop {
		let to_send = {
			let mut subscription = state.lock();
			if subscription.stopped {
				return true;
			}
			let mut action = LiveAction::Noop;
			let mut idx = 0usize;
			while idx < subscription.pending_live.len() {
				let entry = &subscription.pending_live[idx];
				let stmt = match Statement::decode(&mut &entry.encoded[..]) {
					Ok(stmt) => stmt,
					Err(_) => {
						subscription.pending_live.remove(idx);
						continue;
					},
				};
				let matched_filters: HashSet<FilterId> =
					subscription.handle.matched_filter_ids(&stmt).into_iter().collect();
				let still_blocked =
					matched_filters.iter().any(|f| subscription.replays_in_progress.contains(f));
				if still_blocked {
					idx += 1;
					continue;
				}
				let popped = subscription.pending_live.remove(idx).expect("idx in range; qed");
				let next_action = compute_new_statements_action(
					&mut subscription,
					popped.hash,
					popped.encoded,
					&matched_filters,
				);
				if !matches!(next_action, LiveAction::Noop) {
					action = next_action;
					break;
				}
			}
			action
		};

		match to_send {
			LiveAction::Emit(entry) => {
				if !send_event(sink, &SubscribeEvent::NewStatements { statements: vec![entry] })
					.await
				{
					return false;
				}
			},
			LiveAction::Stop => {
				let _ = send_event(sink, &SubscribeEvent::Stop).await;
				return false;
			},
			LiveAction::Noop => return true,
		}
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
	use sp_statement_store::OptimizedTopicFilter;
	use std::sync::Arc;
	use tokio::sync::mpsc;

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
		let (tx, _rx) = mpsc::unbounded_channel();
		Arc::new(Mutex::new(SubscriptionState::new(handle, tx)))
	}

	fn dummy_event() -> sp_statement_store::LiveStatementEvent {
		use codec::Encode;
		let encoded = sp_statement_store::Statement::new().encode();
		sp_statement_store::LiveStatementEvent {
			hash: [0xab; 32],
			encoded,
			matched_filter_ids: vec![],
		}
	}

	fn register_any_filter(state: &Arc<Mutex<SubscriptionState>>) -> FilterId {
		let handle = state.lock().handle.clone();
		let (id, _snapshot) = handle.add_filter(OptimizedTopicFilter::Any).expect("add_filter");
		id
	}

	#[test]
	fn subscription_registry_is_scoped_by_connection_id() {
		let subscriptions = StatementSubscriptions::new();
		let conn_a = ConnectionId(1);
		let conn_b = ConnectionId(2);
		let sub_id = "same-subscription-id".to_string();

		let handle_a = empty_subscription_state().lock().handle.clone();
		let handle_b = empty_subscription_state().lock().handle.clone();
		let (tx_a, _rx_a) = mpsc::unbounded_channel();
		let (tx_b, _rx_b) = mpsc::unbounded_channel();

		let entry_a = subscriptions
			.reserve(conn_a)
			.unwrap()
			.register(sub_id.clone(), handle_a, tx_a)
			.unwrap();
		let entry_b = subscriptions
			.reserve(conn_b)
			.unwrap()
			.register(sub_id.clone(), handle_b, tx_b)
			.unwrap();

		let state_a = subscriptions.get(conn_a, &sub_id).unwrap();
		let state_b = subscriptions.get(conn_b, &sub_id).unwrap();
		assert!(Arc::ptr_eq(&state_a, entry_a.state()));
		assert!(Arc::ptr_eq(&state_b, entry_b.state()));
		assert!(!Arc::ptr_eq(&state_a, &state_b));

		drop(entry_a);
		assert!(subscriptions.get(conn_a, &sub_id).is_none());
		assert!(subscriptions.get(conn_b, &sub_id).is_some());
	}

	#[test]
	fn decide_live_action_noop_when_stopped() {
		let state = empty_subscription_state();
		let mut subscription = state.lock();
		subscription.stopped = true;
		assert_eq!(decide_live_action(&mut subscription, dummy_event()), LiveAction::Noop);
	}

	#[test]
	fn decide_live_action_stop_on_overflow() {
		let state = empty_subscription_state();
		let id = register_any_filter(&state);
		let mut subscription = state.lock();
		subscription.fill_pending_live_for_overflow_test(id, PENDING_LIVE_HARD_CAP);
		assert_eq!(decide_live_action(&mut subscription, dummy_event()), LiveAction::Stop);
		assert!(subscription.stopped);
	}

	#[test]
	fn decide_live_action_buffers_when_replay_in_progress_below_cap() {
		let state = empty_subscription_state();
		let id = register_any_filter(&state);
		let mut subscription = state.lock();
		subscription.fill_pending_live_for_overflow_test(id, PENDING_LIVE_HARD_CAP - 1);
		assert_eq!(decide_live_action(&mut subscription, dummy_event()), LiveAction::Noop);
		assert!(!subscription.stopped);
		assert_eq!(subscription.pending_live.len(), PENDING_LIVE_HARD_CAP);
	}

	#[test]
	fn decide_live_action_stop_on_emitted_history_overflow() {
		let state = empty_subscription_state();
		register_any_filter(&state);
		let mut subscription = state.lock();
		for i in 0..EMITTED_VIA_NEW_HARD_CAP {
			subscription.new_emitted_hashes.insert([i as u8; 32]);
		}

		assert_eq!(decide_live_action(&mut subscription, dummy_event()), LiveAction::Stop);
		assert!(subscription.stopped);
		assert_eq!(subscription.new_emitted_hashes.len(), EMITTED_VIA_NEW_HARD_CAP);
	}
}
