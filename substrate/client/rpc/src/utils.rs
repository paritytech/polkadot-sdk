//! JSON-RPC helpers.
use crate::SubscriptionTaskExecutor;
use futures::{
	future::{self, Either, Fuse, FusedFuture},
	Future, FutureExt, Stream, StreamExt,
};
use jsonrpsee::{PendingSubscriptionSink, SubscriptionMessage, SubscriptionSink};
use sp_runtime::Serialize;
use std::collections::VecDeque;

const DEFAULT_BUF_SIZE: usize = 16;

/// A simple bounded VecDeque.
struct BoundedVecDeque<T> {
	inner: VecDeque<T>,
	max_cap: usize,
}

impl<T> BoundedVecDeque<T> {
	/// Create a new bounded VecDeque.
	fn new() -> Self {
		Self { inner: VecDeque::with_capacity(DEFAULT_BUF_SIZE), max_cap: DEFAULT_BUF_SIZE }
	}

	fn push_back(&mut self, item: T) -> Result<(), ()> {
		if self.inner.len() >= self.max_cap {
			Err(())
		} else {
			self.inner.push_back(item);
			Ok(())
		}
	}

	fn pop_front(&mut self) -> Option<T> {
		self.inner.pop_front()
	}
}

/// Feed items to the subscription from the underlying stream.
///
/// It's possible configure how many items from the stream
/// are allowed to be kept in memory until the subscription is dropped.
///
/// This is needed because the underlying streams in substrate are
/// unbounded.
pub async fn pipe_from_stream<S, T>(pending: PendingSubscriptionSink, mut stream: S)
where
	S: Stream<Item = T> + Unpin + Send + 'static,
	T: Serialize + Send + 'static,
{
	let mut buf = BoundedVecDeque::new();
	let accept_fut = pending.accept();

	futures::pin_mut!(accept_fut);

	// Poll the stream while waiting for the subscription to be accepted
	//
	// If the `max_cap` is exceeded then the subscription is dropped.
	let sink = loop {
		match future::select(accept_fut, stream.next()).await {
			Either::Left((Ok(sink), _)) => break sink,
			Either::Right((Some(msg), f)) => {
				if buf.push_back(msg).is_err() {
					log::warn!(target: "rpc", "Subscription::accept failed buffer limit={} exceed; dropping subscription", buf.max_cap);
					return
				}
				accept_fut = f;
			},
			// The connection was closed or the stream was closed.
			_ => return,
		}
	};

	inner_pipe_from_stream(sink, stream, buf).await
}

async fn inner_pipe_from_stream<S, T>(
	sink: SubscriptionSink,
	mut stream: S,
	mut buf: BoundedVecDeque<T>,
) where
	S: Stream<Item = T> + Unpin + Send + 'static,
	T: Serialize + Send + 'static,
{
	let mut next_fut = Box::pin(Fuse::terminated());
	let mut next_item = stream.next();
	let closed = sink.closed();

	futures::pin_mut!(closed);

	loop {
		if next_fut.is_terminated() {
			if let Some(v) = buf.pop_front() {
				let val = to_sub_message(&sink, &v);
				next_fut.set(async { sink.send(val).await }.fuse());
			}
		}

		match future::select(closed, future::select(next_fut, next_item)).await {
			// Send operation finished.
			Either::Right((Either::Left((_, n)), c)) => {
				next_item = n;
				closed = c;
				next_fut = Box::pin(Fuse::terminated());
			},
			// New item from the stream
			Either::Right((Either::Right((Some(v), n)), c)) => {
				if buf.push_back(v).is_err() {
					log::warn!(target: "rpc", "Subscription buffer limit={} exceed; dropping subscription", buf.max_cap);
					return
				}

				next_fut = n;
				closed = c;
				next_item = stream.next();
			},
			// Stream "finished".
			//
			// Process remaining items and terminate.
			Either::Right((Either::Right((None, pending_fut)), _)) => {
				if pending_fut.await.is_err() {
					return;
				}

				while let Some(v) = buf.pop_front() {
					let val = to_sub_message(&sink, &v);
					if sink.send(val).await.is_err() {
						return;
					}
				}

				return;
			},
			// Subscription was closed.
			Either::Left(_) => return,
		}
	}
}

/// Builds a subscription message.
///
/// # Panics
///
/// This function panics `Serialize` fails and it is treated a bug.
pub fn to_sub_message(sink: &SubscriptionSink, result: &impl Serialize) -> SubscriptionMessage {
	SubscriptionMessage::new(sink.method_name(), sink.subscription_id(), result)
		.expect("Serialize infallible; qed")
}

/// Helper for spawning non-blocking rpc subscription task.
pub fn spawn_subscription_task(
	executor: &SubscriptionTaskExecutor,
	fut: impl Future<Output = ()> + Send + 'static,
) {
	executor.spawn("substrate-rpc-subscription", Some("rpc"), fut.boxed());
}

#[cfg(test)]
mod tests {
	use super::pipe_from_stream;
	use futures::StreamExt;
	use jsonrpsee::{core::EmptyServerParams, RpcModule, Subscription};

	async fn subscribe() -> Subscription {
		let mut module = RpcModule::new(());
		module
			.register_subscription("sub", "my_sub", "unsub", |_, pending, _| async move {
				let stream = futures::stream::iter([0; 16]);
				pipe_from_stream(pending, stream).await;
				Ok(())
			})
			.unwrap();

		module.subscribe("sub", EmptyServerParams::new(), 1).await.unwrap()
	}

	#[tokio::test]
	async fn pipe_from_stream_works() {
		let mut sub = subscribe().await;
		let mut rx = 0;

		while let Some(Ok(_)) = sub.next::<usize>().await {
			rx += 1;
		}

		assert_eq!(rx, 16);
	}

	#[tokio::test]
	async fn pipe_from_stream_is_bounded() {
		let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();

		let mut module = RpcModule::new(tx);
		module
			.register_subscription("sub", "my_sub", "unsub", |_, pending, ctx| async move {
				let stream = futures::stream::iter([0; 32]);
				pipe_from_stream(pending, stream).await;
				_ = ctx.unbounded_send(());
				Ok(())
			})
			.unwrap();

		let mut sub = module.subscribe("sub", EmptyServerParams::new(), 1).await.unwrap();

		// When the 17th item arrives the subscription is dropped
		_ = rx.next().await.unwrap();
		assert!(sub.next::<usize>().await.is_none());
	}

	#[tokio::test]
	async fn wait_for_buffered_items() {
		let mut sub = subscribe().await;

		// HACK: we can't really know when all items have
		// processed by pipe_from_stream and that's
		// why we just sleep long enough here.
		tokio::time::sleep(std::time::Duration::from_secs(6)).await;

		let mut rx = 0;

		while let Some(Ok(_)) = sub.next::<usize>().await {
			rx += 1;
		}

		assert_eq!(rx, 16);
	}
}
