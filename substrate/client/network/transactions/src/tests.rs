// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::*;
use sc_network::{
	config::MultiaddrWithPeerId, service::traits::MessageSink, Event, Multiaddr, ReputationChange,
};
use sp_runtime::{traits::Hash, OpaqueExtrinsic};
use std::{collections::HashSet, sync::Mutex};

type TestBlock = sp_runtime::testing::Block<OpaqueExtrinsic>;
type TestHash = <TestBlock as BlockT>::Hash;
type TestHandler = TransactionsHandler<TestBlock, TestHash, MockNetwork, MockSync>;
type Reports = Arc<Mutex<Vec<(PeerId, ReputationChange)>>>;

mockall::mock! {
	Network {}

	#[async_trait::async_trait]
	impl NetworkPeers for Network {
		fn set_authorized_peers(&self, peers: HashSet<PeerId>);
		fn set_authorized_only(&self, reserved_only: bool);
		fn add_known_address(&self, peer_id: PeerId, addr: Multiaddr);
		fn report_peer(&self, peer_id: PeerId, cost_benefit: ReputationChange);
		fn peer_reputation(&self, peer_id: &PeerId) -> i32;
		fn disconnect_peer(&self, peer_id: PeerId, protocol: ProtocolName);
		fn accept_unreserved_peers(&self);
		fn deny_unreserved_peers(&self);
		fn add_reserved_peer(&self, peer: MultiaddrWithPeerId) -> Result<(), String>;
		fn remove_reserved_peer(&self, peer_id: PeerId);
		fn set_reserved_peers(&self, protocol: ProtocolName, peers: HashSet<Multiaddr>) -> Result<(), String>;
		fn add_peers_to_reserved_set(&self, protocol: ProtocolName, peers: HashSet<Multiaddr>) -> Result<(), String>;
		fn remove_peers_from_reserved_set(&self, protocol: ProtocolName, peers: Vec<PeerId>) -> Result<(), String>;
		fn sync_num_connected(&self) -> usize;
		fn peer_role(&self, peer_id: PeerId, handshake: Vec<u8>) -> Option<ObservedRole>;
		async fn reserved_peers(&self) -> Result<Vec<PeerId>, ()>;
	}

	impl NetworkEventStream for Network {
		fn event_stream(&self, name: &'static str) -> Pin<Box<dyn Stream<Item = Event> + Send>>;
	}
}

mockall::mock! {
	Sync {}

	impl sp_consensus::SyncOracle for Sync {
		fn is_major_syncing(&self) -> bool;
		fn is_offline(&self) -> bool;
	}

	impl SyncEventStream for Sync {
		fn event_stream(&self, name: &'static str) -> Pin<Box<dyn Stream<Item = SyncEvent> + Send>>;
	}
}

mockall::mock! {
	#[derive(Debug)]
	Notifications {}

	#[async_trait::async_trait]
	impl NotificationService for Notifications {
		async fn open_substream(&mut self, peer: PeerId) -> Result<(), ()>;
		async fn close_substream(&mut self, peer: PeerId) -> Result<(), ()>;
		fn send_sync_notification(&mut self, peer: &PeerId, notification: Vec<u8>);
		async fn send_async_notification(&mut self, peer: &PeerId, notification: Vec<u8>) -> Result<(), error::Error>;
		async fn set_handshake(&mut self, handshake: Vec<u8>) -> Result<(), ()>;
		fn try_set_handshake(&mut self, handshake: Vec<u8>) -> Result<(), ()>;
		async fn next_event(&mut self) -> Option<NotificationEvent>;
		fn clone(&mut self) -> Result<Box<dyn NotificationService>, ()>;
		fn protocol(&self) -> &ProtocolName;
		fn message_sink(&self, peer: &PeerId) -> Option<Box<dyn MessageSink>>;
	}
}

mockall::mock! {
	Pool {}

	impl TransactionPool<TestHash, TestBlock> for Pool {
		fn transactions(&self) -> Vec<(TestHash, Arc<OpaqueExtrinsic>)>;
		fn hash_of(&self, transaction: &OpaqueExtrinsic) -> TestHash;
		fn import(&self, transaction: OpaqueExtrinsic) -> TransactionImportFuture;
		fn on_broadcasted(&self, propagations: HashMap<TestHash, Vec<String>>);
		fn transaction(&self, hash: &TestHash) -> Option<Arc<OpaqueExtrinsic>>;
	}
}

fn new_handler(peers: &[PeerId], import_results: &[TransactionImport]) -> (TestHandler, Reports) {
	let reports = Reports::default();
	let recorded = reports.clone();
	let mut network = MockNetwork::new();
	network.expect_report_peer().returning(move |peer, change| {
		recorded.lock().unwrap().push((peer, change));
	});
	let mut sync = MockSync::new();
	sync.expect_is_major_syncing().return_const(false);
	let mut pool = MockPool::new();
	pool.expect_hash_of().returning(sp_runtime::traits::BlakeTwo256::hash_of);
	let mut sequence = mockall::Sequence::new();
	for &result in import_results {
		pool.expect_import()
			.times(1)
			.in_sequence(&mut sequence)
			.returning(move |_| Box::pin(future::ready(result)));
	}
	let handler = TransactionsHandler {
		protocol_name: "/test/transactions/1".into(),
		propagate_timeout: (Box::pin(stream::pending()) as Pin<Box<dyn Stream<Item = ()> + Send>>)
			.fuse(),
		pending_transactions: FuturesUnordered::new(),
		pending_transactions_peers: HashMap::new(),
		network,
		sync,
		sync_event_stream: (Box::pin(stream::pending())
			as Pin<Box<dyn Stream<Item = SyncEvent> + Send>>)
			.fuse(),
		peers: peers
			.iter()
			.map(|&id| {
				(
					id,
					Peer {
						known_transactions: LruHashSet::new(
							NonZeroUsize::new(MAX_KNOWN_TRANSACTIONS).unwrap(),
						),
						role: ObservedRole::Full,
					},
				)
			})
			.collect(),
		transaction_pool: Arc::new(pool),
		from_controller: tracing_unbounded("transaction-test", 10).1,
		metrics: None,
		notification_service: Box::new(MockNotifications::new()),
	};
	(handler, reports)
}

fn submit(handler: &mut TestHandler, peer: PeerId, transactions: Vec<OpaqueExtrinsic>) {
	handler.handle_notification_event(NotificationEvent::NotificationReceived {
		peer,
		notification: transactions.encode(),
	});
}

fn finish_import(handler: &mut TestHandler) {
	let (hash, result) = handler.pending_transactions.next().now_or_never().unwrap().unwrap();
	let peers = handler.pending_transactions_peers.remove(&hash).unwrap();
	for peer in peers {
		handler.on_handle_transaction_import(peer, result);
	}
	assert!(handler.pending_transactions.is_empty());
	assert!(handler.pending_transactions_peers.is_empty());
}

#[test]
fn duplicate_submissions_apply_import_reputation_once_per_peer() {
	for (result, reputation) in [
		(TransactionImport::NewGood, Some(rep::GOOD_TRANSACTION)),
		(TransactionImport::KnownGood, Some(rep::ANY_TRANSACTION_REFUND)),
		(TransactionImport::Bad, Some(rep::BAD_TRANSACTION)),
		(TransactionImport::None, None),
	] {
		let first = PeerId::random();
		let second = PeerId::random();
		let (mut handler, reports) = new_handler(&[first, second], &[result]);
		let tx = OpaqueExtrinsic::from_blob(vec![7]);
		submit(&mut handler, first, vec![tx.clone(); 5]);

		// A pending import must remain deduplicated even after its hash is evicted from
		// the peer's bounded cache (e.g. by outgoing transaction propagation).
		for i in 0..MAX_KNOWN_TRANSACTIONS {
			let hash = sp_runtime::traits::BlakeTwo256::hash_of(&(i as u64));
			handler.peers.get_mut(&first).unwrap().known_transactions.insert(hash);
		}
		assert!(!handler.peers[&first]
			.known_transactions
			.contains(&sp_runtime::traits::BlakeTwo256::hash_of(&tx)));
		submit(&mut handler, first, vec![tx.clone()]);
		submit(&mut handler, second, vec![tx.clone(); 2]);
		assert_eq!(handler.pending_transactions.len(), 1);
		finish_import(&mut handler);

		let reports = reports.lock().unwrap();
		for peer in [first, second] {
			let outcomes = reports
				.iter()
				.filter(|(id, change)| *id == peer && change.value != rep::ANY_TRANSACTION.value)
				.collect::<Vec<_>>();
			assert_eq!(
				outcomes.len(),
				usize::from(reputation.is_some()),
				"each peer must receive the import outcome at most once"
			);
			if let Some(reputation) = reputation {
				assert_eq!(outcomes[0].1.value, reputation.value);
			}
		}
		// Charge once for each distinct supplier of this pending transaction.
		assert_eq!(
			reports.iter().filter(|(_, c)| c.value == rep::ANY_TRANSACTION.value).count(),
			2
		);
	}
}

#[test]
fn transaction_can_be_retried_after_import_returns_none() {
	let peer = PeerId::random();
	let (mut handler, reports) =
		new_handler(&[peer], &[TransactionImport::None, TransactionImport::NewGood]);
	let tx = OpaqueExtrinsic::from_blob(vec![7]);
	submit(&mut handler, peer, vec![tx.clone()]);
	finish_import(&mut handler);
	// The known-transactions cache must not suppress a retry after an inconclusive import.
	submit(&mut handler, peer, vec![tx]);
	finish_import(&mut handler);
	assert_eq!(
		reports
			.lock()
			.unwrap()
			.iter()
			.filter(|(_, c)| c.value == rep::GOOD_TRANSACTION.value)
			.count(),
		1
	);
}
