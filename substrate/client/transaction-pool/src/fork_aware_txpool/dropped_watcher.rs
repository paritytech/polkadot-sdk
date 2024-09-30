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

//! Multi-view pool dropped events listener provides means to combine streams from multiple pool
//! views into a single event stream. It allows management of dropped transaction events, adding new
//! views, and removing views as needed, ensuring that transactions which are no longer referenced
//! by any view are detected and properly notified.

use crate::{
	fork_aware_txpool::stream_map_util::next_event,
	graph::{BlockHash, ChainApi, ExtrinsicHash},
	LOG_TARGET,
};
use futures::stream::StreamExt;
use log::{debug, trace};
use sc_transaction_pool_api::TransactionStatus;
use sc_utils::mpsc;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{hash_map::Entry, HashMap, HashSet},
	fmt::{self, Debug, Formatter},
	pin::Pin,
};
use tokio_stream::StreamMap;

/// Dropped-logic related event from the single view.
pub type ViewStreamEvent<C> = crate::graph::DroppedByLimitsEvent<ExtrinsicHash<C>, BlockHash<C>>;

/// Dropped-logic stream of events coming from the single view.
type ViewStream<C> = Pin<Box<dyn futures::Stream<Item = ViewStreamEvent<C>> + Send>>;

/// Stream of extrinsic hashes that were dropped by the views and have no references by existing
/// views.
pub(crate) type StreamOfDropped<C> = Pin<Box<dyn futures::Stream<Item = ExtrinsicHash<C>> + Send>>;

/// A type alias for a sender used as the controller of the [`MultiViewDropWatcherContext`].
/// Used to send control commands from the [`MultiViewDroppedWatcherController`] to
/// [`MultiViewDropWatcherContext`].
type Controller<T> = mpsc::TracingUnboundedSender<T>;

/// A type alias for a receiver used as the commands receiver in the
/// [`MultiViewDropWatcherContext`].
type CommandReceiver<T> = mpsc::TracingUnboundedReceiver<T>;

/// Commands to control the instance of dropped transactions stream [`StreamOfDropped`].
enum Command<C>
where
	C: ChainApi,
{
	/// Adds a new stream of dropped-related events originating in a view with a specific block
	/// hash
	AddView(BlockHash<C>, ViewStream<C>),
	/// Removes an existing view's stream associated with a specific block hash.
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

/// Manages the state and logic for handling events related to dropped transactions across multiple
/// views.
///
/// This struct maintains a mapping of active views and their corresponding streams, as well as the
/// state of each transaction with respect to these views.
struct MultiViewDropWatcherContext<C>
where
	C: ChainApi,
{
	/// A map that associates the views identified by corresponding block hashes with their streams
	/// of dropped-related events. This map is used to keep track of active views and their event
	/// streams.
	stream_map: StreamMap<BlockHash<C>, ViewStream<C>>,
	/// A receiver for commands to control the state of the stream, allowing the addition and
	/// removal of views. This is used to dynamically update which views are being tracked.
	command_receiver: CommandReceiver<Command<C>>,

	/// For each transaction hash we keep the set of hashes representing the views that see this
	/// transaction as ready or future.
	/// Once transaction is dropped, dropping view is removed from the set.
	transaction_states: HashMap<ExtrinsicHash<C>, HashSet<BlockHash<C>>>,
}

impl<C> MultiViewDropWatcherContext<C>
where
	C: ChainApi + 'static,
	<<C as ChainApi>::Block as BlockT>::Hash: Unpin,
{
	/// Processes a `ViewStreamEvent` from a specific view and updates the internal state
	/// accordingly.
	///
	/// If the event indicates that a transaction has been dropped and is no longer referenced by
	/// any active views, the transaction hash is returned. Otherwise function returns `None`.
	fn handle_event(
		&mut self,
		block_hash: BlockHash<C>,
		event: ViewStreamEvent<C>,
	) -> Option<ExtrinsicHash<C>> {
		debug!(
			target: LOG_TARGET,
			"dropped_watcher: got event: views:{:#?}, event: {:?} states: {:?}",
			self.stream_map.keys().collect::<Vec<_>>(),
			event,
			self.transaction_states
		);
		let (tx_hash, status) = event;
		match status {
			TransactionStatus::Ready | TransactionStatus::Future => {
				self.transaction_states.entry(tx_hash).or_default().insert(block_hash);
			},
			TransactionStatus::Dropped | TransactionStatus::Usurped(_) => {
				if let Entry::Occupied(mut views_keeping_tx_valid) =
					self.transaction_states.entry(tx_hash)
				{
					views_keeping_tx_valid.get_mut().remove(&block_hash);
					if views_keeping_tx_valid.get().is_empty() ||
						views_keeping_tx_valid
							.get()
							.iter()
							.all(|h| !self.stream_map.contains_key(h))
					{
						debug!("[{:?}] dropped_watcher: removing tx", tx_hash);
						return Some(tx_hash)
					}
				} else {
					// the transaction was never included into any view, so it is being dropped
					// because all views are currently full.
					// we shall give it a chance.
					debug!("[{:?}] dropped_watcher: skipping non tracked tx", tx_hash);
				}
			},
			_ => {},
		};
		None
	}

	/// Creates a new `StreamOfDropped` and its associated event stream controller.
	///
	/// This method initializes the internal structures and unfolds the stream of dropped
	/// transactions. Returns a tuple containing this stream and the controller for managing
	/// this stream.
	fn event_stream() -> (StreamOfDropped<C>, Controller<Command<C>>) {
		//note: 64 allows to avoid warning messages during execution of unit tests.
		const CHANNEL_SIZE: usize = 64;
		let (sender, command_receiver) = sc_utils::mpsc::tracing_unbounded::<Command<C>>(
			"tx-pool-dropped-watcher-cmd-stream",
			CHANNEL_SIZE,
		);

		let ctx = Self {
			stream_map: StreamMap::new(),
			command_receiver,
			transaction_states: Default::default(),
		};

		let stream_map = futures::stream::unfold(ctx, |mut ctx| async move {
			loop {
				tokio::select! {
					biased;
					cmd = ctx.command_receiver.next() => {
						match cmd? {
							Command::AddView(key,stream) => {
								trace!(target: LOG_TARGET,"dropped_watcher: Command::AddView {key:?} {:#?}",ctx.stream_map.keys().collect::<Vec<_>>());
								ctx.stream_map.insert(key,stream);
							},
							Command::RemoveView(key) => {
								trace!(target: LOG_TARGET,"dropped_watcher: Command::RemoveView {key:?} {:#?}",ctx.stream_map.keys().collect::<Vec<_>>());
								ctx.stream_map.remove(&key);
							},
						}
					},

					Some(event) = next_event(&mut ctx.stream_map) => {
						debug!(target: LOG_TARGET, "dropped_watcher: select_next_some -> {:#?} {:#?}", event, ctx.stream_map.keys().collect::<Vec<_>>());
						if let Some(dropped) = ctx.handle_event(event.0, event.1) {
							debug!("dropped_watcher: sending out: {dropped:?}");
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

/// The controller for manipulating the state of the [`StreamOfDropped`].
///
/// This struct provides methods to add and remove streams associated with views to and from the
/// stream.
#[derive(Clone)]
pub struct MultiViewDroppedWatcherController<C: ChainApi> {
	/// A controller allowing to update the state of the associated [`StreamOfDropped`].
	controller: Controller<Command<C>>,
}

impl<C> MultiViewDroppedWatcherController<C>
where
	C: ChainApi + 'static,
	<<C as ChainApi>::Block as BlockT>::Hash: Unpin,
{
	/// Creates new [`StreamOfDropped`] and its controller.
	pub fn new() -> (MultiViewDroppedWatcherController<C>, StreamOfDropped<C>) {
		let (stream_map, ctrl) = MultiViewDropWatcherContext::<C>::event_stream();
		(Self { controller: ctrl }, stream_map.boxed())
	}

	/// Notifies the [`StreamOfDropped`] that new view was created.
	pub fn add_view(&self, key: BlockHash<C>, view: ViewStream<C>) {
		let _ = self.controller.unbounded_send(Command::AddView(key, view)).map_err(|e| {
			trace!(target: LOG_TARGET, "dropped_watcher: add_view {key:?} send message failed: {e}");
		});
	}

	/// Notifies the [`StreamOfDropped`] that the view was destroyed and shall be removed the
	/// stream map.
	pub fn remove_view(&self, key: BlockHash<C>) {
		let _ = self.controller.unbounded_send(Command::RemoveView(key)).map_err(|e| {
			trace!(target: LOG_TARGET, "dropped_watcher: remove_view {key:?} send message failed: {e}");
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

	#[tokio::test]
	async fn test05() {
		sp_tracing::try_init_simple();
		let (watcher, mut output_stream) = MultiViewDroppedWatcher::new();
		assert!(output_stream.next().now_or_never().is_none());

		let block_hash0 = H256::repeat_byte(0x01);
		let block_hash1 = H256::repeat_byte(0x02);
		let tx_hash = H256::repeat_byte(0x0b);

		let view_stream0 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Future),
			(tx_hash, TransactionStatus::InBlock((block_hash1, 0))),
		])
		.boxed();
		watcher.add_view(block_hash0, view_stream0);
		assert!(output_stream.next().now_or_never().is_none());

		let view_stream1 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Ready),
			(tx_hash, TransactionStatus::InBlock((block_hash0, 0))),
		])
		.boxed();

		watcher.add_view(block_hash1, view_stream1);
		assert!(output_stream.next().now_or_never().is_none());
		assert!(output_stream.next().now_or_never().is_none());
		assert!(output_stream.next().now_or_never().is_none());
		assert!(output_stream.next().now_or_never().is_none());
		assert!(output_stream.next().now_or_never().is_none());

		let tx_hash = H256::repeat_byte(0x0c);
		let view_stream2 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Future),
			(tx_hash, TransactionStatus::Dropped),
		])
		.boxed();
		let block_hash2 = H256::repeat_byte(0x03);
		watcher.add_view(block_hash2, view_stream2);
		let handle = tokio::spawn(async move { output_stream.take(1).collect::<Vec<_>>().await });
		assert_eq!(handle.await.unwrap(), vec![tx_hash]);
	}
}
