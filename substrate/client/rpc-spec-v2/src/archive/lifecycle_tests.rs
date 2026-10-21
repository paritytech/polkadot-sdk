// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::{
	test_observer::{Point, Registration},
	Archive, ArchiveApiServer,
};
use crate::{
	common::events::{
		ArchiveStorageDiffItem, ArchiveStorageDiffType, StorageQuery, StorageQueryType,
	},
	hex_string,
};
use jsonrpsee::{core::server::Subscription as RpcSubscription, rpc_params, RpcModule};
use sc_block_builder::BlockBuilderBuilder;
use sc_client_api::ChildInfo;
use sp_consensus::BlockOrigin;
use sp_runtime::traits::Block as BlockT;
use std::{
	sync::{Arc, Mutex},
	time::Duration,
};
use substrate_test_runtime_client::{
	prelude::*, Backend, BlockBuilderExt, Client, ClientBlockImportExt,
};

type Block = substrate_test_runtime_client::runtime::Block;
const KEY: &[u8] = b":mock";
const VALUE: &[u8] = b"value";
const INVALID_HASH: [u8; 32] = [1; 32];

#[derive(Default)]
struct Tasks {
	handles: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Drop for Tasks {
	fn drop(&mut self) {
		for task in self.handles.get_mut().unwrap().drain(..) {
			task.abort();
		}
	}
}

#[derive(Clone, Default)]
struct TrackedExecutor(Arc<Tasks>);

impl sp_core::traits::SpawnNamed for TrackedExecutor {
	fn spawn_blocking(
		&self,
		name: &'static str,
		group: Option<&'static str>,
		future: futures::future::BoxFuture<'static, ()>,
	) {
		self.spawn(name, group, future);
	}

	fn spawn(
		&self,
		_name: &'static str,
		_group: Option<&'static str>,
		future: futures::future::BoxFuture<'static, ()>,
	) {
		self.0.handles.lock().unwrap().push(tokio::spawn(future));
	}
}

impl TrackedExecutor {
	async fn finish(&self) {
		let tasks = std::mem::take(&mut *self.0.handles.lock().unwrap());
		assert_eq!(tasks.len(), 1, "track the entire subscription handler");
		for mut task in tasks {
			let result = tokio::time::timeout(Duration::from_secs(5), &mut task).await;
			if result.is_err() {
				task.abort();
				let _ = task.await;
			}
			result.expect("handler did not finish").expect("handler panicked");
		}
	}
}

type Api = RpcModule<Archive<Backend, Block, Client<Backend>>>;

struct Fixture {
	client: Arc<Client<Backend>>,
	api: Api,
	executor: TrackedExecutor,
	hash: String,
	previous: String,
}

impl Fixture {
	async fn new(request: Request) -> Self {
		let mut builder = TestClientBuilder::new();
		for n in 0..128 {
			builder =
				builder.add_extra_storage(format!(":cancel:{n:03}").into_bytes(), b"old".to_vec());
		}
		for child in [b"one", b"two"] {
			builder = builder.add_extra_child_storage(
				&ChildInfo::new_default(child),
				KEY.to_vec(),
				VALUE.to_vec(),
			);
		}
		let backend = builder.backend();
		let client = Arc::new(builder.build());
		let previous = client.genesis_hash();
		let hash = if matches!(request, Request::DiffResults | Request::DiffNonmatching) {
			let mut builder = BlockBuilderBuilder::new(&*client)
				.on_parent_block(previous)
				.with_parent_block_number(0)
				.build()
				.unwrap();
			for n in 0..128 {
				builder
					.push_storage_change(
						format!(":cancel:{n:03}").into_bytes(),
						Some(b"new".to_vec()),
					)
					.unwrap();
			}
			let block = builder.build().unwrap().block;
			let hash = format!("{:?}", block.hash());
			client.import(BlockOrigin::Own, block).await.unwrap();
			hash
		} else {
			format!("{previous:?}")
		};
		let executor = TrackedExecutor::default();
		let api =
			Archive::new(client.clone(), backend, [0; 32], Arc::new(executor.clone())).into_rpc();
		Self { client, api, executor, hash, previous: format!("{previous:?}") }
	}

	async fn subscribe(&self, request: Request) -> RpcSubscription {
		let query = |key: &[u8], query_type| StorageQuery {
			key: hex_string(&key),
			query_type,
			pagination_start_key: None,
		};
		let diff = |key: &[u8], child: Option<&[u8]>| ArchiveStorageDiffItem {
			key: hex_string(&key),
			return_type: ArchiveStorageDiffType::Value,
			child_trie_key: child.map(|child| hex_string(&child)),
		};
		let params = match request {
			Request::Storage => rpc_params![
				&self.hash,
				vec![
					query(b":absent", StorageQueryType::DescendantsValues),
					query(b":cancel:", StorageQueryType::DescendantsValues),
					query(b":cancel:000", StorageQueryType::Value),
				]
			],
			Request::StorageResults => rpc_params![
				&self.hash,
				vec![query(b":cancel:", StorageQueryType::DescendantsValues)]
			],
			Request::StorageError => rpc_params![
				hex_string(&INVALID_HASH),
				vec![query(b":cancel:", StorageQueryType::DescendantsValues)]
			],
			Request::StorageDone => rpc_params![&self.hash, Vec::<StorageQuery<String>>::new()],
			Request::InvalidQuery => rpc_params![
				&self.hash,
				vec![StorageQuery {
					key: "NOT_HEX".to_string(),
					query_type: StorageQueryType::Value,
					pagination_start_key: None,
				}]
			],
			Request::InvalidChild => {
				rpc_params![&self.hash, Vec::<StorageQuery<String>>::new(), "NOT_HEX"]
			},
			Request::MissingHeader => {
				rpc_params![hex_string(&INVALID_HASH), Vec::<ArchiveStorageDiffItem<String>>::new()]
			},
			Request::DiffUnchanged => {
				rpc_params![&self.hash, vec![diff(b":cancel:", None)], &self.hash]
			},
			Request::DiffNonmatching => {
				rpc_params![&self.hash, vec![diff(b":absent", None)], &self.previous]
			},
			Request::DiffGroups => rpc_params![
				&self.hash,
				vec![diff(b":absent", None), diff(b"", Some(b"one")), diff(b"", Some(b"two"))],
				&self.previous
			],
			Request::DiffResults => {
				rpc_params![&self.hash, vec![diff(b":cancel:", None)], &self.previous]
			},
			Request::DiffError => rpc_params![
				&self.hash,
				vec![ArchiveStorageDiffItem {
					key: "NOT_HEX".to_string(),
					return_type: ArchiveStorageDiffType::Value,
					child_trie_key: None::<String>,
				}],
				&self.previous
			],
			Request::DiffDone => {
				rpc_params![&self.hash, vec![diff(b":absent", None)], &self.hash]
			},
		};
		self.api.subscribe(request.method(), params, 1).await.unwrap()
	}
}

#[derive(Clone, Copy, Debug)]
enum Request {
	Storage,
	StorageResults,
	StorageError,
	StorageDone,
	InvalidQuery,
	InvalidChild,
	MissingHeader,
	DiffUnchanged,
	DiffNonmatching,
	DiffGroups,
	DiffResults,
	DiffError,
	DiffDone,
}

impl Request {
	fn method(self) -> &'static str {
		match self {
			Self::Storage |
			Self::StorageResults |
			Self::StorageError |
			Self::StorageDone |
			Self::InvalidQuery |
			Self::InvalidChild => "archive_v1_storage",
			_ => "archive_v1_storageDiff",
		}
	}

	fn stop(self) -> &'static str {
		if self.method() == "archive_v1_storage" {
			"archive_v1_stopStorage"
		} else {
			"archive_v1_storageDiff_stopStorageDiff"
		}
	}
}

async fn cancel(api: &Api, sub: &mut Option<RpcSubscription>, request: Request, disconnect: bool) {
	if disconnect {
		drop(sub.take());
	} else {
		assert!(api
			.call::<_, bool>(request.stop(), rpc_params![sub.as_ref().unwrap().subscription_id()])
			.await
			.unwrap());
	}
}

fn work(points: Vec<Point>) -> Vec<Point> {
	points
		.into_iter()
		.filter(|point| {
			matches!(
				point,
				Point::Deduplicate |
					Point::Item |
					Point::Iterator |
					Point::Advance |
					Point::Group |
					Point::CurrentIterator |
					Point::PreviousIterator |
					Point::DiffAdvance |
					Point::RawNext |
					Point::Unchanged |
					Point::Nonmatching
			)
		})
		.collect()
}

async fn boundary(request: Request, point: Point, count: usize) {
	for disconnect in [false, true] {
		let fixture = Fixture::new(request).await;
		let registration = Registration::new(&fixture.client, Some((point, count)), None);
		let observer = &registration.1;
		let mut sub = Some(fixture.subscribe(request).await);
		observer.wait(point, count).await;
		let before = work(observer.points());
		assert!(
			!observer.points().contains(&Point::Send),
			"scan must reach the gate without sends"
		);
		cancel(&fixture.api, &mut sub, request, disconnect).await;
		// Do not release the synchronous operation until receiver ownership really ends.
		observer.receiver_closed().await;
		observer.release();
		fixture.executor.finish().await;
		let mut after = work(observer.points());
		// An iterator next() already in progress can finish its current diff calculation.
		if point == Point::RawNext {
			assert!(matches!(after.pop(), Some(Point::Unchanged | Point::Nonmatching)));
		}
		assert_eq!(after, before, "{request:?}, disconnect={disconnect}");
		assert_eq!(observer.points().iter().filter(|p| **p == Point::ProducerExited).count(), 1);
		assert!(observer.points().contains(&Point::ProducerJoined));
		assert!(!observer.points().contains(&Point::ProducerPanicked));
	}
}

macro_rules! boundary_test {
	($name:ident, $request:ident, $point:ident, $count:expr) => {
		#[tokio::test]
		async fn $name() {
			boundary(Request::$request, Point::$point, $count).await;
		}
	};
}

boundary_test!(native_cancel_storage_item, Storage, ItemBoundary, 2);
boundary_test!(native_cancel_storage_iterator, StorageResults, IteratorBoundary, 1);
boundary_test!(native_cancel_storage_advance, StorageResults, AdvanceBoundary, 1);
boundary_test!(native_cancel_diff_producer, DiffUnchanged, Producer, 1);
boundary_test!(native_cancel_diff_current_iterator, DiffUnchanged, CurrentIteratorBoundary, 1);
boundary_test!(native_cancel_diff_previous_iterator, DiffUnchanged, PreviousIteratorBoundary, 1);
boundary_test!(native_cancel_diff_start, DiffUnchanged, DiffStartBoundary, 1);
boundary_test!(native_cancel_diff_unchanged, DiffUnchanged, Unchanged, 2);
boundary_test!(native_cancel_diff_nonmatching, DiffNonmatching, Nonmatching, 2);
boundary_test!(native_cancel_diff_group, DiffGroups, GroupBoundary, 2);
boundary_test!(native_cancel_diff_inflight_next, DiffUnchanged, RawNext, 4);

async fn pending_send(request: Request, event: &'static str, producer: bool) {
	for disconnect in [false, true] {
		let fixture = Fixture::new(request).await;
		let registration = Registration::new(&fixture.client, None, Some(event));
		let observer = &registration.1;
		let mut sub = Some(fixture.subscribe(request).await);
		observer.output_ready();
		observer.wait(Point::PendingSend, 1).await;
		cancel(&fixture.api, &mut sub, request, disconnect).await;
		// Unsubscribe leaves the full output queue alive until after the handler has joined.
		fixture.executor.finish().await;
		assert_eq!(
			observer.points().iter().filter(|p| **p == Point::ProducerExited).count(),
			usize::from(producer),
		);
		assert_eq!(observer.points().contains(&Point::ProducerJoined), producer);
		assert!(!observer.points().contains(&Point::ProducerPanicked));
	}
}

macro_rules! pending_test {
	($name:ident, $request:ident, $event:literal, $producer:expr) => {
		#[tokio::test]
		async fn $name() {
			pending_send(Request::$request, $event, $producer).await;
		}
	};
}

pending_test!(native_pending_storage_result, StorageResults, "storage", true);
pending_test!(native_pending_storage_error, StorageError, "storageError", true);
pending_test!(native_pending_storage_done, StorageDone, "storageDone", true);
pending_test!(native_pending_diff_result, DiffResults, "storageDiff", true);
pending_test!(native_pending_diff_error, DiffError, "storageDiffError", true);
pending_test!(native_pending_diff_done, DiffDone, "storageDiffDone", true);
pending_test!(native_pending_invalid_query, InvalidQuery, "storageError", false);
pending_test!(native_pending_invalid_child, InvalidChild, "storageError", false);
pending_test!(native_pending_missing_header, MissingHeader, "storageDiffError", false);
