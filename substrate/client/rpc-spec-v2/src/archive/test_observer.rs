// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Request-local observations of real archive subscriptions. Compiled only for unit tests.

use futures::{future::BoxFuture, Future, FutureExt};
use sc_rpc::utils::Subscription;
use std::{
	collections::HashMap,
	sync::{Arc, Condvar, LazyLock, Mutex, Weak},
	task::Poll,
	time::Duration,
};
use tokio::sync::{mpsc, Notify};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point {
	Producer,
	Deduplicate,
	ItemBoundary,
	Item,
	IteratorBoundary,
	Iterator,
	AdvanceBoundary,
	Advance,
	GroupBoundary,
	Group,
	CurrentIteratorBoundary,
	CurrentIterator,
	PreviousIteratorBoundary,
	PreviousIterator,
	DiffStartBoundary,
	DiffAdvanceBoundary,
	DiffAdvance,
	RawNext,
	Unchanged,
	Nonmatching,
	Send,
	PendingSend,
	ProducerExited,
	ProducerJoined,
	ProducerPanicked,
}

#[derive(Default)]
struct State {
	points: Vec<Point>,
	released: bool,
}

pub struct Observer {
	state: Mutex<State>,
	release: Condvar,
	changed: Notify,
	output_ready: Notify,
	gate: Option<(Point, usize)>,
	pub fill_event: Option<&'static str>,
	closed: Mutex<Option<BoxFuture<'static, ()>>>,
}

static OBSERVERS: LazyLock<Mutex<HashMap<usize, Weak<Observer>>>> = LazyLock::new(Default::default);

tokio::task_local! {
	static CURRENT: Option<Arc<Observer>>;
}

pub fn get<C>(client: &Arc<C>) -> Option<Arc<Observer>> {
	OBSERVERS
		.lock()
		.unwrap()
		.get(&(Arc::as_ptr(client) as usize))
		.and_then(Weak::upgrade)
}

pub struct Registration(usize, pub Arc<Observer>);

impl Registration {
	pub fn new<C>(
		client: &Arc<C>,
		gate: Option<(Point, usize)>,
		fill_event: Option<&'static str>,
	) -> Self {
		let observer = Arc::new(Observer {
			state: Mutex::new(State::default()),
			release: Condvar::new(),
			changed: Notify::new(),
			output_ready: Notify::new(),
			gate,
			fill_event,
			closed: Mutex::new(None),
		});
		let key = Arc::as_ptr(client) as usize;
		assert!(OBSERVERS.lock().unwrap().insert(key, Arc::downgrade(&observer)).is_none());
		Self(key, observer)
	}
}

impl Drop for Registration {
	fn drop(&mut self) {
		self.1.release();
		OBSERVERS.lock().unwrap().remove(&self.0);
	}
}

impl Observer {
	pub fn point(&self, point: Point) {
		let mut state = self.state.lock().unwrap();
		state.points.push(point);
		self.changed.notify_one();
		if self.gate == Some((point, state.points.iter().filter(|p| **p == point).count())) {
			let (state, timeout) = self
				.release
				.wait_timeout_while(state, Duration::from_secs(10), |s| !s.released)
				.unwrap();
			drop(state);
			assert!(!timeout.timed_out(), "test did not release {point:?}");
		}
	}

	pub fn release(&self) {
		self.state.lock().unwrap().released = true;
		self.release.notify_all();
		self.output_ready();
	}

	pub fn output_ready(&self) {
		self.output_ready.notify_one();
	}

	pub fn points(&self) -> Vec<Point> {
		self.state.lock().unwrap().points.clone()
	}

	pub async fn wait(&self, point: Point, count: usize) {
		tokio::time::timeout(Duration::from_secs(5), async {
			loop {
				let changed = self.changed.notified();
				if self.points().iter().filter(|p| **p == point).count() >= count {
					return;
				}
				changed.await;
			}
		})
		.await
		.unwrap_or_else(|_| panic!("missing {point:?} #{count}: {:?}", self.points()));
	}

	pub fn channel<T: Send + 'static>(&self, tx: &mpsc::Sender<T>) {
		let tx = tx.downgrade();
		*self.closed.lock().unwrap() = Some(
			async move {
				let tx = tx.upgrade().expect("producer is held at the gate");
				tx.closed().await;
			}
			.boxed(),
		);
	}

	pub async fn receiver_closed(&self) {
		let closed = self.closed.lock().unwrap().take().expect("observed producer channel");
		tokio::time::timeout(Duration::from_secs(5), closed)
			.await
			.expect("forwarder must drop the receiver on cancellation");
	}
}

pub fn point(observer: &Option<Arc<Observer>>, point: Point) {
	if let Some(observer) = observer {
		observer.point(point);
	}
}

pub struct ProducerGuard(pub Option<Arc<Observer>>);

impl Drop for ProducerGuard {
	fn drop(&mut self) {
		point(&self.0, Point::ProducerExited);
	}
}

pub async fn scope<C>(client: Arc<C>, future: impl Future<Output = ()>) {
	CURRENT.scope(get(&client), future).await
}

/// Fill the native jsonrpsee/Tokio output queue, without changing either implementation.
pub async fn prepare_send(sink: &Subscription, event: &impl serde::Serialize) {
	let observer = CURRENT.try_with(Clone::clone).ok().flatten();
	let Some(observer) = observer else { return };
	observer.point(Point::Send);
	if observer.fill_event != serde_json::to_value(event).unwrap()["event"].as_str() {
		return;
	}
	// The subscription response shares this queue. Let the client consume it first.
	tokio::time::timeout(Duration::from_secs(5), observer.output_ready.notified())
		.await
		.expect("test must consume the subscription response before filling the output");
	loop {
		let send = sink.send(&"test queue filler");
		futures::pin_mut!(send);
		let sent = futures::future::poll_fn(|cx| match send.as_mut().poll(cx) {
			Poll::Ready(result) => Poll::Ready(result.is_ok()),
			Poll::Pending => Poll::Ready(false),
		})
		.await;
		if !sent {
			return;
		}
	}
}

/// Report Pending only after polling the actual sink future.
pub async fn sending<T>(send: impl Future<Output = T>) -> T {
	let observer = CURRENT.try_with(Clone::clone).ok().flatten();
	futures::pin_mut!(send);
	let mut reported = false;
	futures::future::poll_fn(|cx| {
		let result = send.as_mut().poll(cx);
		if result.is_pending() && !reported {
			reported = true;
			point(&observer, Point::PendingSend);
		}
		result
	})
	.await
}
