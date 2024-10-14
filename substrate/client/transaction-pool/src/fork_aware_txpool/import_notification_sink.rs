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

//! Multi view import notification sink. This module provides a unified stream of transactions that
//! have been notified as ready by any of the active views maintained by the transaction pool. It
//! combines streams (`import_notification_stream`) from multiple views into a single stream. Events
//! coming from this stream are dynamically dispatched to many external watchers.

use crate::{fork_aware_txpool::stream_map_util::next_event, LOG_TARGET};
use futures::{
	channel::mpsc::{channel, Receiver as EventStream, Sender as ExternalSink},
	stream::StreamExt,
	Future, FutureExt,
};
use log::trace;
use parking_lot::RwLock;
use sc_utils::mpsc;
use std::{
	collections::HashSet,
	fmt::{self, Debug, Formatter},
	hash::Hash,
	pin::Pin,
	sync::Arc,
};
use tokio_stream::StreamMap;

/// A type alias for a pinned, boxed stream of items of type `I`.
/// This alias is particularly useful for defining the types of the incoming streams from various
/// views, and is intended to build the stream of transaction hashes that become ready.
///
/// Note: generic parameter allows better testing of all types involved.
type StreamOf<I> = Pin<Box<dyn futures::Stream<Item = I> + Send>>;

/// A type alias for a tracing unbounded sender used as the command channel controller.
/// Used to send control commands to the [`AggregatedStreamContext`].
type Controller<T> = mpsc::TracingUnboundedSender<T>;

/// A type alias for a tracing unbounded receiver used as the command channel receiver.
/// Used to receive control commands in the [`AggregatedStreamContext`].
type CommandReceiver<T> = mpsc::TracingUnboundedReceiver<T>;

/// An enum representing commands that can be sent to the multi-sinks context.
///
/// This enum contains variants that encapsulate control commands used to manage multiple streams
/// within the `AggregatedStreamContext`.
enum Command<K, I: Send + Sync> {
	///  Adds a new view with a unique key and a stream of items of type `I`.
	AddView(K, StreamOf<I>),
}

impl<K, I: Send + Sync> Debug for Command<K, I> {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			Command::AddView(..) => write!(f, "AddView"),
		}
	}
}

/// A context used to unfold the single stream of items aggregated from the multiple
/// streams.
///
/// The `AggregatedStreamContext` continuously monitors both the command receiver and the stream
/// map, ensuring new views can be dynamically added and events from any active view can be
/// processed.
struct AggregatedStreamContext<K, I: Send + Sync> {
	/// A map of streams identified by unique keys,
	stream_map: StreamMap<K, StreamOf<I>>,
	/// A receiver for handling control commands, such as adding new views.
	command_receiver: CommandReceiver<Command<K, I>>,
}

impl<K, I> AggregatedStreamContext<K, I>
where
	K: Send + Debug + Unpin + Clone + Default + Hash + Eq + 'static,
	I: Send + Sync + 'static + PartialEq + Eq + Hash + Clone + Debug,
{
	/// Creates a new aggregated stream of items and its command controller.
	///
	/// This function sets up the initial context with an empty stream map. The aggregated output
	/// stream of items (e.g. hashes of transactions that become ready) is unfolded.
	///
	/// It returns a tuple containing the output stream and the command controller, allowing
	/// external components to control this stream.
	fn event_stream() -> (StreamOf<I>, Controller<Command<K, I>>) {
		let (sender, receiver) =
			sc_utils::mpsc::tracing_unbounded::<Command<K, I>>("import-notification-sink", 16);

		let ctx = Self { stream_map: StreamMap::new(), command_receiver: receiver };

		let output_stream = futures::stream::unfold(ctx, |mut ctx| async move {
			loop {
				tokio::select! {
					biased;
					cmd = ctx.command_receiver.next() => {
						match cmd? {
							Command::AddView(key,stream) => {
								trace!(target: LOG_TARGET,"Command::AddView {key:?}");
								ctx.stream_map.insert(key,stream);
							},
						}
					},

					Some(event) = next_event(&mut ctx.stream_map) => {
						trace!(target: LOG_TARGET, "import_notification_sink: select_next_some -> {:?}", event);
						return Some((event.1, ctx));
					}
				}
			}
		})
		.boxed();

		(output_stream, sender)
	}
}

/// A struct that facilitates the relaying notifications of ready transactions from multiple views
/// to many external sinks.
///
/// `MultiViewImportNotificationSink` provides mechanisms to dynamically add new views, filter
/// notifications of imported transactions hashes and relay them to the multiple external sinks.
#[derive(Clone)]
pub struct MultiViewImportNotificationSink<K, I: Send + Sync> {
	/// A controller used to send commands to the internal [`AggregatedStreamContext`].
	controller: Controller<Command<K, I>>,
	/// A vector of the external sinks, each receiving a copy of the merged stream of ready
	/// transaction hashes.
	external_sinks: Arc<RwLock<Vec<ExternalSink<I>>>>,
	/// A set of already notified items, ensuring that each item (transaction hash) is only
	/// sent out once.
	already_notified_items: Arc<RwLock<HashSet<I>>>,
}

/// An asynchronous task responsible for dispatching aggregated import notifications to multiple
/// sinks (created by [`MultiViewImportNotificationSink::event_stream`]).
pub type ImportNotificationTask = Pin<Box<dyn Future<Output = ()> + Send>>;

impl<K, I> MultiViewImportNotificationSink<K, I>
where
	K: 'static + Clone + Send + Debug + Default + Unpin + Eq + Hash,
	I: 'static + Clone + Send + Debug + Sync + PartialEq + Eq + Hash,
{
	/// Creates a new [`MultiViewImportNotificationSink`] along with its associated worker task.
	///
	/// This function initializes the sink and provides the worker task that listens for events from
	/// the aggregated stream, relaying them to the external sinks. The task shall be polled by
	/// caller.
	///
	/// Returns a tuple containing the [`MultiViewImportNotificationSink`] and the
	/// [`ImportNotificationTask`].
	pub fn new_with_worker() -> (MultiViewImportNotificationSink<K, I>, ImportNotificationTask) {
		let (output_stream, controller) = AggregatedStreamContext::<K, I>::event_stream();
		let output_stream_controller = Self {
			controller,
			external_sinks: Default::default(),
			already_notified_items: Default::default(),
		};
		let external_sinks = output_stream_controller.external_sinks.clone();
		let already_notified_items = output_stream_controller.already_notified_items.clone();

		let import_notifcation_task = output_stream
			.for_each(move |event| {
				let external_sinks = external_sinks.clone();
				let already_notified_items = already_notified_items.clone();
				async move {
					if already_notified_items.write().insert(event.clone()) {
						external_sinks.write().retain_mut(|sink| {
							trace!(target: LOG_TARGET, "[{:?}] import_sink_worker sending out imported", event);
							if let Err(e) = sink.try_send(event.clone()) {
								trace!(target: LOG_TARGET, "import_sink_worker sending message failed: {e}");
								false
							} else {
								true
							}
						});
					}
				}
			})
			.boxed();
		(output_stream_controller, import_notifcation_task)
	}

	/// Adds a new stream associated with the view identified by specified key.
	///
	/// The new view's stream is added to the internal aggregated stream context by sending command
	/// to its `command_receiver`.
	pub fn add_view(&self, key: K, view: StreamOf<I>) {
		let _ = self
			.controller
			.unbounded_send(Command::AddView(key.clone(), view))
			.map_err(|e| {
				trace!(target: LOG_TARGET, "add_view {key:?} send message failed: {e}");
			});
	}

	/// Creates and returns a new external stream of ready transactions hashes notifications.
	pub fn event_stream(&self) -> EventStream<I> {
		const CHANNEL_BUFFER_SIZE: usize = 1024;
		let (sender, receiver) = channel(CHANNEL_BUFFER_SIZE);
		self.external_sinks.write().push(sender);
		receiver
	}

	/// Removes specified items from the `already_notified_items` set.
	///
	/// Intended to be called once transactions are finalized.
	pub fn clean_notified_items(&self, items_to_be_removed: &[I]) {
		let mut already_notified_items = self.already_notified_items.write();
		items_to_be_removed.iter().for_each(|i| {
			already_notified_items.remove(i);
		});
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use core::time::Duration;
	use tokio::task::JoinHandle;

	#[derive(Debug, Clone)]
	struct Event<I: Send> {
		delay: u64,
		value: I,
	}

	impl<I: Send> From<(u64, I)> for Event<I> {
		fn from(event: (u64, I)) -> Self {
			Self { delay: event.0, value: event.1 }
		}
	}

	struct View<I: Send + Sync> {
		scenario: Vec<Event<I>>,
		sinks: Arc<RwLock<Vec<ExternalSink<I>>>>,
	}

	impl<I: Send + Sync + 'static + Clone + Debug> View<I> {
		fn new(scenario: Vec<(u64, I)>) -> Self {
			Self {
				scenario: scenario.into_iter().map(Into::into).collect(),
				sinks: Default::default(),
			}
		}

		async fn event_stream(&self) -> EventStream<I> {
			let (sender, receiver) = channel(32);
			self.sinks.write().push(sender);
			receiver
		}

		fn play(&mut self) -> JoinHandle<()> {
			let mut scenario = self.scenario.clone();
			let sinks = self.sinks.clone();
			tokio::spawn(async move {
				loop {
					if scenario.is_empty() {
						for sink in &mut *sinks.write() {
							sink.close_channel();
						}
						break;
					};
					let x = scenario.remove(0);
					tokio::time::sleep(Duration::from_millis(x.delay)).await;
					for sink in &mut *sinks.write() {
						sink.try_send(x.value.clone()).unwrap();
					}
				}
			})
		}
	}

	#[tokio::test]
	async fn deduplicating_works() {
		sp_tracing::try_init_simple();

		let (ctrl, runnable) = MultiViewImportNotificationSink::<u64, i32>::new_with_worker();

		let j0 = tokio::spawn(runnable);

		let stream = ctrl.event_stream();

		let mut v1 = View::new(vec![(0, 1), (0, 2), (0, 3)]);
		let mut v2 = View::new(vec![(0, 1), (0, 2), (0, 6)]);
		let mut v3 = View::new(vec![(0, 1), (0, 2), (0, 3)]);

		let j1 = v1.play();
		let j2 = v2.play();
		let j3 = v3.play();

		let o1 = v1.event_stream().await.boxed();
		let o2 = v2.event_stream().await.boxed();
		let o3 = v3.event_stream().await.boxed();

		ctrl.add_view(1000, o1);
		ctrl.add_view(2000, o2);
		ctrl.add_view(3000, o3);

		let out = stream.take(4).collect::<Vec<_>>().await;
		assert!(out.iter().all(|v| vec![1, 2, 3, 6].contains(v)));
		drop(ctrl);

		futures::future::join_all(vec![j0, j1, j2, j3]).await;
	}

	#[tokio::test]
	async fn dedup_filter_reset_works() {
		sp_tracing::try_init_simple();

		let (ctrl, runnable) = MultiViewImportNotificationSink::<u64, i32>::new_with_worker();

		let j0 = tokio::spawn(runnable);

		let stream = ctrl.event_stream();

		let mut v1 = View::new(vec![(10, 1), (10, 2), (10, 3)]);
		let mut v2 = View::new(vec![(20, 1), (20, 2), (20, 6)]);
		let mut v3 = View::new(vec![(20, 1), (20, 2), (20, 3)]);

		let j1 = v1.play();
		let j2 = v2.play();
		let j3 = v3.play();

		let o1 = v1.event_stream().await.boxed();
		let o2 = v2.event_stream().await.boxed();
		let o3 = v3.event_stream().await.boxed();

		ctrl.add_view(1000, o1);
		ctrl.add_view(2000, o2);

		let j4 = {
			let ctrl = ctrl.clone();
			tokio::spawn(async move {
				tokio::time::sleep(Duration::from_millis(70)).await;
				ctrl.clean_notified_items(&vec![1, 3]);
				ctrl.add_view(3000, o3.boxed());
			})
		};

		let out = stream.take(6).collect::<Vec<_>>().await;
		assert_eq!(out, vec![1, 2, 3, 6, 1, 3]);
		drop(ctrl);

		futures::future::join_all(vec![j0, j1, j2, j3, j4]).await;
	}

	#[tokio::test]
	async fn many_output_streams_are_supported() {
		sp_tracing::try_init_simple();

		let (ctrl, runnable) = MultiViewImportNotificationSink::<u64, i32>::new_with_worker();

		let j0 = tokio::spawn(runnable);

		let stream0 = ctrl.event_stream();
		let stream1 = ctrl.event_stream();

		let mut v1 = View::new(vec![(0, 1), (0, 2), (0, 3)]);
		let mut v2 = View::new(vec![(0, 1), (0, 2), (0, 6)]);
		let mut v3 = View::new(vec![(0, 1), (0, 2), (0, 3)]);

		let j1 = v1.play();
		let j2 = v2.play();
		let j3 = v3.play();

		let o1 = v1.event_stream().await.boxed();
		let o2 = v2.event_stream().await.boxed();
		let o3 = v3.event_stream().await.boxed();

		ctrl.add_view(1000, o1);
		ctrl.add_view(2000, o2);
		ctrl.add_view(3000, o3);

		let out0 = stream0.take(4).collect::<Vec<_>>().await;
		let out1 = stream1.take(4).collect::<Vec<_>>().await;
		assert!(out0.iter().all(|v| vec![1, 2, 3, 6].contains(v)));
		assert!(out1.iter().all(|v| vec![1, 2, 3, 6].contains(v)));
		drop(ctrl);

		futures::future::join_all(vec![j0, j1, j2, j3]).await;
	}
}
