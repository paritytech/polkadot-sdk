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

//! Multi view import notification sink. Combines streams from many views into single stream.

use crate::LOG_TARGET;
use futures::{
	channel::mpsc::{channel, Receiver as EventStream, Sender as ExternalSink},
	stream::{self, Fuse, StreamExt},
	Future, FutureExt,
};
use log::{debug, trace};
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

type StreamOf<I> = Pin<Box<dyn futures::Stream<Item = I> + Send>>;

type Controller<T> = mpsc::TracingUnboundedSender<T>;
type CommandReceiver<T> = mpsc::TracingUnboundedReceiver<T>;

enum Command<K, I: Send + Sync> {
	AddView(K, StreamOf<I>),
}

impl<K, I: Send + Sync> Debug for Command<K, I> {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			Command::AddView(..) => write!(f, "AddView"),
		}
	}
}

struct MultiSinksContext<K, I: Send + Sync> {
	stream_map: Fuse<StreamMap<K, StreamOf<I>>>,
	controller: Fuse<CommandReceiver<Command<K, I>>>,
}

impl<K, I> MultiSinksContext<K, I>
where
	K: Send + Debug + Unpin + Clone + Default + Hash + Eq + 'static,
	I: Send + Sync + 'static + PartialEq + Eq + Hash + Clone + Debug,
{
	fn event_stream() -> (StreamOf<I>, Controller<Command<K, I>>) {
		let (sender, receiver) =
			sc_utils::mpsc::tracing_unbounded::<Command<K, I>>("import-notification-sink", 16);

		let mut stream_map: StreamMap<K, StreamOf<I>> = StreamMap::new();
		//note: do not terminate stream-map if input streams (views) are all done:
		stream_map.insert(Default::default(), stream::pending().boxed());

		let ctx = Self { stream_map: stream_map.fuse(), controller: receiver.fuse() };

		let output_stream = futures::stream::unfold(ctx, |mut ctx| async move {
			loop {
				tokio::select! {
					biased;
					cmd = ctx.controller.next() => {
						match cmd {
							Some(Command::AddView(key,stream)) => {
								debug!(target: LOG_TARGET,"Command::AddView {key:?}");
								ctx.stream_map.get_mut().insert(key,stream);
							},
							//controller sender is terminated, terminate the map as well
							None => { return None }
						}
					},

					event = futures::StreamExt::select_next_some(&mut ctx.stream_map) => {
						trace!(target: LOG_TARGET, "select_next_some -> {:#?}", event);
						return Some((event.1, ctx));
					}
				}
			}
		})
		.boxed();

		(output_stream, sender)
	}
}

#[derive(Clone)]
pub struct MultiViewImportNotificationSink<K, I: Send + Sync> {
	ctrl: Controller<Command<K, I>>,
	external_sinks: Arc<RwLock<Vec<ExternalSink<I>>>>,
	filter: Arc<RwLock<HashSet<I>>>,
}

/// A txpool task driving import notifications.
pub type ImportNotificationTask = Pin<Box<dyn Future<Output = ()> + Send>>;

impl<K, I> MultiViewImportNotificationSink<K, I>
where
	K: 'static + Clone + Send + Debug + Default + Unpin + Eq + Hash,
	I: 'static + Clone + Send + Debug + Sync + PartialEq + Eq + Hash,
{
	pub fn new_with_worker() -> (MultiViewImportNotificationSink<K, I>, ImportNotificationTask) {
		let (output_stream, ctrl) = MultiSinksContext::<K, I>::event_stream();
		let ctrl = Self { ctrl, external_sinks: Default::default(), filter: Default::default() };
		let external_sinks = ctrl.external_sinks.clone();
		let filter = ctrl.filter.clone();

		let f = output_stream
			.for_each(move |event| {
				let external_sinks = external_sinks.clone();
				let filter = filter.clone();
				async move {
					if filter.write().insert(event.clone()) {
						for sink in &mut *external_sinks.write() {
							debug!(target: LOG_TARGET, "[{:?}] import_sink_worker sending out imported", event);
							let _ = sink.try_send(event.clone()).map_err(|e| {
								debug!(target: LOG_TARGET, "import_sink_worker sending message failed: {e}");
							});
						}
					}
				}
			})
			.boxed();
		(ctrl, f)
	}

	pub fn add_view(&self, key: K, view: StreamOf<I>) {
		let _ = self.ctrl.unbounded_send(Command::AddView(key.clone(), view)).map_err(|e| {
			debug!(target: LOG_TARGET, "add_view {key:?} send message failed: {e}");
		});
	}

	pub fn event_stream(&self) -> EventStream<I> {
		const CHANNEL_BUFFER_SIZE: usize = 1024;
		let (sender, receiver) = channel(CHANNEL_BUFFER_SIZE);
		self.external_sinks.write().push(sender);
		receiver
	}

	pub fn clean_filter(&self, items_to_be_removed: &Vec<I>) {
		self.filter.write().retain(|v| !items_to_be_removed.contains(v));
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
				ctrl.clean_filter(&vec![1, 3]);
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
