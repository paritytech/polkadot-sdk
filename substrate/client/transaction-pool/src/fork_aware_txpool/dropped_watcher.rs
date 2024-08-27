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

//! Multi view pool events listener. Combines streams from many views into single stream.

use crate::{
	graph::{BlockHash, ChainApi, ExtrinsicHash},
	LOG_TARGET,
};
use futures::stream::{self, Fuse, StreamExt};
use log::{debug, info};
use sc_transaction_pool_api::TransactionStatus;
use sc_utils::mpsc;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, HashSet},
	fmt::{self, Debug, Formatter},
	pin::Pin,
};
use tokio_stream::StreamMap;

pub type PoolSingleStreamEvent<C> =
	(ExtrinsicHash<C>, TransactionStatus<BlockHash<C>, ExtrinsicHash<C>>);
type StreamOf<C> = Pin<Box<dyn futures::Stream<Item = PoolSingleStreamEvent<C>> + Send>>;

/// Stream of extrinsic hashes that were dropped by all views or have no references by existing
/// views.
pub(crate) type StreamOfDropped<C> = Pin<Box<dyn futures::Stream<Item = ExtrinsicHash<C>> + Send>>;

type Controller<T> = mpsc::TracingUnboundedSender<T>;
type CommandReceiver<T> = mpsc::TracingUnboundedReceiver<T>;

enum Command<C>
where
	C: ChainApi,
{
	AddView(BlockHash<C>, StreamOf<C>),
	RemoveView(BlockHash<C>),
}

impl<C> Debug for Command<C>
where
	C: ChainApi,
{
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			Command::AddView(..) => write!(f, "AddView"),
			Command::RemoveView(..) => write!(f, "RemoveView"),
		}
	}
}

struct MulitViewDropWatcherContext<C>
where
	C: ChainApi,
{
	stream_map: Fuse<StreamMap<BlockHash<C>, StreamOf<C>>>,
	controller: Fuse<CommandReceiver<Command<C>>>,

	/// For each transaction we keep the HashSet of views that see this transaction as ready or
	/// future.
	/// Once transaction is dropped, dropping view is removed fromt the set.
	transaction_states: HashMap<ExtrinsicHash<C>, HashSet<BlockHash<C>>>,
}

impl<C> MulitViewDropWatcherContext<C>
where
	C: ChainApi + 'static,
	<<C as ChainApi>::Block as BlockT>::Hash: Unpin,
{
	fn handle_event(
		&mut self,
		block_hash: BlockHash<C>,
		event: PoolSingleStreamEvent<C>,
	) -> Option<ExtrinsicHash<C>> {
		info!(
			target: LOG_TARGET,
			"dropped_watcher: got event: views:{:#?}, event: {:?} states: {:?}",
			self.stream_map.get_ref().keys().collect::<Vec<_>>(),
			event,
			self.transaction_states
		);
		let (tx_hash, status) = event;
		match status {
			TransactionStatus::Ready | TransactionStatus::Future => {
				self.transaction_states
					.entry(tx_hash)
					.or_insert(Default::default())
					.insert(block_hash);
			},
			TransactionStatus::Dropped | TransactionStatus::Usurped(_) => {
				let current_views = HashSet::<BlockHash<C>>::from_iter(
					self.stream_map.get_ref().keys().map(Clone::clone),
				);
				if let Some(views_keeping_tx_valid) = self.transaction_states.get_mut(&tx_hash) {
					views_keeping_tx_valid.remove(&block_hash);
					if views_keeping_tx_valid.is_disjoint(&current_views) {
						info!("[{:?}] dropped_watcher: removing tx", tx_hash);
						return Some(tx_hash)
					}
				} else {
					info!("[{:?}] dropped_watcher: removing non tracked tx", tx_hash);
					return Some(tx_hash)
				}
			},
			_ => {},
		};
		None
	}

	fn event_stream() -> (StreamOfDropped<C>, Controller<Command<C>>) {
		//note: 64 allows to avoid warning messages during execution of unit tests.
		const CHANNEL_SIZE: usize = 64;
		let (sender, receiver) = sc_utils::mpsc::tracing_unbounded::<Command<C>>(
			"import-notification-sink",
			CHANNEL_SIZE,
		);

		let mut stream_map: StreamMap<BlockHash<C>, StreamOf<C>> = StreamMap::new();
		//note: do not terminate stream-map if input streams (views) are all done:
		stream_map.insert(Default::default(), stream::pending().boxed());

		let ctx = Self {
			stream_map: stream_map.fuse(),
			controller: receiver.fuse(),
			transaction_states: Default::default(),
		};

		let stream_map = futures::stream::unfold(ctx, |mut ctx| async move {
			loop {
				tokio::select! {
					biased;
					cmd = ctx.controller.next() => {
						match cmd {
							Some(Command::AddView(key,stream)) => {
								debug!(target: LOG_TARGET,"dropped_watcher: Command::AddView {key:?}");
								ctx.stream_map.get_mut().insert(key,stream);
							},
							Some(Command::RemoveView(key)) => {
								debug!(target: LOG_TARGET,"dropped_watcher: Command::RemoveView {key:?}");
								ctx.stream_map.get_mut().remove(&key);
							},
							//controller sender is terminated, terminate the map as well
							None => { return None }
						}
					},

					event = futures::StreamExt::select_next_some(&mut ctx.stream_map) => {
						info!(target: LOG_TARGET, "dropped_watcher: select_next_some -> {:#?}", event);
						if let Some(dropped) = ctx.handle_event(event.0, event.1) {
							info!("dropped_watcher: sending out: {dropped:?}");
							return Some((dropped, ctx));
						}
					}
				}
			}
		})
		.boxed();

		(stream_map, sender)
	}
}

#[derive(Clone)]
/// The controller allowing to manipulate the state of the [`StreamOfDropped`].
pub struct MultiViewDroppedWatcherController<C: ChainApi> {
	ctrl: Controller<Command<C>>,
}

impl<C> MultiViewDroppedWatcherController<C>
where
	C: ChainApi + 'static,
	<<C as ChainApi>::Block as BlockT>::Hash: Unpin,
{
	/// Creates new [`StreamOfDropped`] and its controller.
	pub fn new() -> (MultiViewDroppedWatcherController<C>, StreamOfDropped<C>) {
		let (stream_map, ctrl) = MulitViewDropWatcherContext::<C>::event_stream();
		(Self { ctrl }, stream_map.boxed())
	}

	/// Notifies the [`StreamOfDropped`] that new view was created.
	pub fn add_view(&self, key: BlockHash<C>, view: StreamOf<C>) {
		let _ = self.ctrl.unbounded_send(Command::AddView(key, view)).map_err(|e| {
			debug!(target: LOG_TARGET, "dropped_watcher: add_view {key:?} send message failed: {e}");
		});
	}

	/// Notifies the [`StreamOfDropped`] that the view was destroyed.
	pub fn remove_view(&self, key: BlockHash<C>) {
		let _ = self.ctrl.unbounded_send(Command::RemoveView(key)).map_err(|e| {
			debug!(target: LOG_TARGET, "dropped_watcher: remove_view {key:?} send message failed: {e}");
		});
	}
}

#[cfg(test)]
mod dropped_watcher_tests {
	use super::*;
	use crate::common::tests::TestApi;
	use futures::{stream::pending, FutureExt, StreamExt};
	use sp_core::H256;

	type MultiViewDroppedWatcher = super::MultiViewDroppedWatcherController<TestApi>;

	#[tokio::test]
	async fn test01() {
		sp_tracing::try_init_simple();
		let (watcher, output_stream) = MultiViewDroppedWatcher::new();

		let block_hash = H256::repeat_byte(0x01);
		let tx_hash = H256::repeat_byte(0x0a);

		let view_stream = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Ready),
			(tx_hash, TransactionStatus::Dropped),
		])
		.boxed();

		watcher.add_view(block_hash, view_stream);
		let handle = tokio::spawn(async move { output_stream.take(1).collect::<Vec<_>>().await });
		assert_eq!(handle.await.unwrap(), vec![tx_hash]);
	}

	#[tokio::test]
	async fn test02() {
		sp_tracing::try_init_simple();
		let (watcher, mut output_stream) = MultiViewDroppedWatcher::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let block_hash1 = H256::repeat_byte(0x02);
		let tx_hash = H256::repeat_byte(0x0a);

		let view_stream0 = futures::stream::iter(vec![(tx_hash, TransactionStatus::Future)])
			.chain(pending())
			.boxed();
		let view_stream1 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Ready),
			(tx_hash, TransactionStatus::Dropped),
		])
		.boxed();

		watcher.add_view(block_hash0, view_stream0);

		assert!(output_stream.next().now_or_never().is_none());
		watcher.add_view(block_hash1, view_stream1);
		assert!(output_stream.next().now_or_never().is_none());
	}

	#[tokio::test]
	async fn test03() {
		sp_tracing::try_init_simple();
		let (watcher, output_stream) = MultiViewDroppedWatcher::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let block_hash1 = H256::repeat_byte(0x02);
		let tx_hash0 = H256::repeat_byte(0x0a);
		let tx_hash1 = H256::repeat_byte(0x0b);

		let view_stream0 = futures::stream::iter(vec![(tx_hash0, TransactionStatus::Future)])
			.chain(pending())
			.boxed();
		let view_stream1 = futures::stream::iter(vec![
			(tx_hash1, TransactionStatus::Ready),
			(tx_hash1, TransactionStatus::Dropped),
		])
		.boxed();

		watcher.add_view(block_hash0, view_stream0);
		watcher.add_view(block_hash1, view_stream1);
		let handle = tokio::spawn(async move { output_stream.take(1).collect::<Vec<_>>().await });
		assert_eq!(handle.await.unwrap(), vec![tx_hash1]);
	}

	#[tokio::test]
	async fn test04() {
		sp_tracing::try_init_simple();
		let (watcher, mut output_stream) = MultiViewDroppedWatcher::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let block_hash1 = H256::repeat_byte(0x02);
		let tx_hash = H256::repeat_byte(0x0b);

		let view_stream0 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Future),
			(tx_hash, TransactionStatus::InBlock((block_hash1, 0))),
		])
		.boxed();
		let view_stream1 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Ready),
			(tx_hash, TransactionStatus::Dropped),
		])
		.boxed();

		watcher.add_view(block_hash0, view_stream0);
		assert!(output_stream.next().now_or_never().is_none());

		watcher.add_view(block_hash1, view_stream1);
		let handle = tokio::spawn(async move { output_stream.take(1).collect::<Vec<_>>().await });
		assert_eq!(handle.await.unwrap(), vec![tx_hash]);
	}
}
