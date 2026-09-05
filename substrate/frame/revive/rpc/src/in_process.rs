// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! A [`subxt`] transport that dispatches JSON-RPC calls into a node running in the same
//! process, replacing the loopback WebSocket connection. Only the transport changes; subxt
//! still serializes and decodes as normal. Skipping that needs subxt's sealed `Backend` trait.

use futures::{Stream, stream};
use jsonrpsee::{
	core::{server::Methods, traits::ToRpcParams},
	types::SubscriptionId,
};
use serde_json::value::RawValue;
use std::{
	collections::BTreeSet,
	future::ready,
	pin::Pin,
	task::{Context, Poll},
};
use subxt::rpcs::{
	Error as RpcError, UserError,
	client::{RawRpcFuture, RawRpcSubscription, RpcClientT},
};

/// Matches subxt's WS client: <https://docs.rs/subxt-rpcs/0.50.2/src/subxt_rpcs/client/jsonrpsee_impl.rs.html#107>
const SUBSCRIPTION_BUFFER_CAPACITY: usize = 4096;

/// The `rpc_methods` RPC method's name.
const RPC_METHODS: &str = "rpc_methods";

/// A [`RpcClientT`] that calls into a node's `RpcModule` directly.
///
/// ```ignore
/// let rpc_handlers = sc_service::spawn_tasks(params)?;
/// let methods: Methods = rpc_handlers.handle().as_ref().clone().into();
/// let rpc_client = RpcClient::new(InProcessRpcClient::new(methods));
/// let api = OnlineClient::<SrcChainConfig>::from_rpc_client(rpc_client).await?;
/// ```
#[derive(Clone, Debug)]
pub struct InProcessRpcClient {
	methods: Methods,
	/// The wrapped module never answers `rpc_methods` itself, but `OnlineClient` needs it.
	rpc_methods_response: Box<RawValue>,
}

impl InProcessRpcClient {
	/// Wrap a node's RPC methods, e.g. from `RpcModule` or
	/// [`RpcHandlers::handle`](sc_service::RpcHandlers::handle).
	pub fn new(methods: impl Into<Methods>) -> Self {
		let methods = methods.into();

		// `rpc_methods`'s response shape isn't ours to change.
		let names = methods
			.method_names()
			.chain(std::iter::once(RPC_METHODS))
			.collect::<BTreeSet<_>>();
		let rpc_methods_response =
			serde_json::value::to_raw_value(&serde_json::json!({ "methods": names }))
				.expect("a list of strings is always serializable; qed");

		Self { methods, rpc_methods_response }
	}
}

impl RpcClientT for InProcessRpcClient {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<RawValue>>,
	) -> RawRpcFuture<'a, Box<RawValue>> {
		if method == RPC_METHODS {
			return Box::pin(ready(Ok(self.rpc_methods_response.clone())));
		}

		Box::pin(async move {
			self.methods
				.call::<_, Box<RawValue>>(method, Params(params))
				.await
				.map_err(into_rpc_error)
		})
	}

	fn subscribe_raw<'a>(
		&'a self,
		sub: &'a str,
		params: Option<Box<RawValue>>,
		unsub: &'a str,
	) -> RawRpcFuture<'a, RawRpcSubscription> {
		Box::pin(async move {
			let subscription = self
				.methods
				.subscribe(sub, Params(params), SUBSCRIPTION_BUFFER_CAPACITY)
				.await
				.map_err(into_rpc_error)?;

			let subscription_id = subscription.subscription_id().clone().into_owned();
			let id = match &subscription_id {
				SubscriptionId::Str(id) => Some(id.to_string()),
				SubscriptionId::Num(id) => Some(id.to_string()),
			};

			let stream = stream::unfold(subscription, |mut subscription| async move {
				let next = subscription.next::<Box<RawValue>>().await?;
				Some((next.map(|(value, _id)| value).map_err(into_rpc_error), subscription))
			});

			let stream = UnsubscribeOnDrop {
				stream: Box::pin(stream),
				methods: self.methods.clone(),
				unsub: unsub.to_owned(),
				subscription_id,
			};

			Ok(RawRpcSubscription { stream: Box::pin(stream), id })
		})
	}
}

/// Already serialized parameters, forwarded to jsonrpsee untouched.
struct Params(Option<Box<RawValue>>);

impl ToRpcParams for Params {
	fn to_rpc_params(self) -> Result<Option<Box<RawValue>>, serde_json::Error> {
		Ok(self.0)
	}
}

/// JSON-RPC error objects stay user errors so subxt can act on their codes.
fn into_rpc_error(err: jsonrpsee::core::server::MethodsError) -> RpcError {
	use jsonrpsee::core::server::MethodsError;
	match err {
		MethodsError::JsonRpc(err) => RpcError::User(UserError {
			code: err.code(),
			message: err.message().to_owned(),
			data: err.data().map(|data| data.to_owned()),
		}),
		err => RpcError::Client(Box::new(err)),
	}
}

/// Forwards `sub`'s notifications; on drop, calls `unsub` explicitly rather than relying only
/// on the notification channel closing, matching what a real network transport does.
struct UnsubscribeOnDrop {
	stream: Pin<Box<dyn Stream<Item = Result<Box<RawValue>, RpcError>> + Send>>,
	methods: Methods,
	unsub: String,
	subscription_id: SubscriptionId<'static>,
}

impl Stream for UnsubscribeOnDrop {
	type Item = Result<Box<RawValue>, RpcError>;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		self.stream.as_mut().poll_next(cx)
	}
}

impl Drop for UnsubscribeOnDrop {
	fn drop(&mut self) {
		let methods = self.methods.clone();
		let unsub = std::mem::take(&mut self.unsub);
		let subscription_id = self.subscription_id.clone();
		tokio::spawn(async move {
			let params = serde_json::value::to_raw_value(&[subscription_id]).ok();
			let _ = methods.call::<_, bool>(&unsub, Params(params)).await;
		});
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use jsonrpsee::{
		core::{RpcResult, server::SubscriptionMessage},
		server::RpcModule,
	};
	use subxt::rpcs::{RpcClient, rpc_params};

	#[derive(serde::Deserialize)]
	struct RpcMethods {
		methods: Vec<String>,
	}

	fn test_module() -> RpcModule<()> {
		let mut module = RpcModule::new(());
		module
			.register_method::<RpcResult<u64>, _>("test_echo", |params, _, _| {
				params.one::<u64>().map_err(Into::into)
			})
			.unwrap();
		module
			.register_method::<RpcResult<u64>, _>("test_fail", |_, _, _| {
				Err(jsonrpsee::types::ErrorObjectOwned::owned(
					-32021,
					"nope",
					Some("extra".to_string()),
				))
			})
			.unwrap();
		module
			.register_subscription(
				"test_subscribe",
				"test_item",
				"test_unsubscribe",
				|_, pending, _, _| async move {
					let sink = pending.accept().await?;
					for value in 0u64..3 {
						sink.send(SubscriptionMessage::from_json(&value)?).await?;
					}
					Ok(())
				},
			)
			.unwrap();
		module
	}

	#[tokio::test]
	async fn request_round_trips() {
		let client = RpcClient::new(InProcessRpcClient::new(test_module()));
		let echoed: u64 = client.request("test_echo", rpc_params![42u64]).await.unwrap();
		assert_eq!(echoed, 42);
	}

	#[tokio::test]
	async fn call_errors_surface_as_user_errors() {
		let client = RpcClient::new(InProcessRpcClient::new(test_module()));
		let err = client.request::<u64>("test_fail", rpc_params![]).await.unwrap_err();
		let RpcError::User(err) = err else { panic!("expected a user error, got {err:?}") };
		assert_eq!(err.code, -32021);
		assert_eq!(err.message, "nope");
	}

	#[tokio::test]
	async fn unknown_methods_surface_as_user_errors() {
		let client = RpcClient::new(InProcessRpcClient::new(test_module()));
		let err = client.request::<u64>("test_missing", rpc_params![]).await.unwrap_err();
		let RpcError::User(err) = err else { panic!("expected a user error, got {err:?}") };
		assert_eq!(err.code, jsonrpsee::types::error::METHOD_NOT_FOUND_CODE);
	}

	#[tokio::test]
	async fn rpc_methods_is_synthesized() {
		let client = RpcClient::new(InProcessRpcClient::new(test_module()));
		let methods: RpcMethods = client.request("rpc_methods", rpc_params![]).await.unwrap();
		assert_eq!(
			methods.methods,
			vec!["rpc_methods", "test_echo", "test_fail", "test_subscribe", "test_unsubscribe"]
		);
	}

	#[tokio::test]
	async fn subscription_yields_every_item() {
		let client = RpcClient::new(InProcessRpcClient::new(test_module()));
		let mut subscription = client
			.subscribe::<u64>("test_subscribe", rpc_params![], "test_unsubscribe")
			.await
			.unwrap();

		let mut received = vec![];
		while let Some(item) = subscription.next().await {
			received.push(item.unwrap());
			if received.len() == 3 {
				break;
			}
		}
		assert_eq!(received, vec![0, 1, 2]);
	}

	#[tokio::test]
	async fn dropping_a_subscription_stops_the_node_side_task() {
		let (stopped_tx, mut stopped_rx) = tokio::sync::mpsc::channel::<()>(1);

		let mut module = RpcModule::new(());
		module
			.register_subscription(
				"test_subscribe",
				"test_item",
				"test_unsubscribe",
				move |_, pending, _, _| {
					let stopped_tx = stopped_tx.clone();
					async move {
						let sink = pending.accept().await?;
						while sink.send(SubscriptionMessage::from_json(&0u64)?).await.is_ok() {}
						let _ = stopped_tx.send(()).await;
						Ok(())
					}
				},
			)
			.unwrap();

		let client = RpcClient::new(InProcessRpcClient::new(module));
		let mut subscription = client
			.subscribe::<u64>("test_subscribe", rpc_params![], "test_unsubscribe")
			.await
			.unwrap();
		subscription.next().await.unwrap().unwrap();
		drop(subscription);

		tokio::time::timeout(std::time::Duration::from_secs(5), stopped_rx.recv())
			.await
			.expect("the node-side subscription task should unwind")
			.unwrap();
	}

	/// Only an explicit `unsub` call can end this subscription, proving `UnsubscribeOnDrop`
	/// makes one instead of relying on the notification channel closing.
	#[tokio::test]
	async fn dropping_a_subscription_calls_unsubscribe() {
		let mut module = RpcModule::new(());
		module
			.register_subscription(
				"test_subscribe",
				"test_item",
				"test_unsubscribe",
				move |_, pending, _, _| async move {
					let _sink = pending.accept().await?;
					// Held but never checked, so only an explicit `test_unsubscribe` call (not
					// the notification channel closing) can end this subscription.
					std::future::pending::<()>().await;
					Ok(())
				},
			)
			.unwrap();

		let methods: Methods = module.into();
		let client = RpcClient::new(InProcessRpcClient::new(methods.clone()));
		let subscription = client
			.subscribe::<u64>("test_subscribe", rpc_params![], "test_unsubscribe")
			.await
			.unwrap();
		let sub_id: u64 = subscription.subscription_id().unwrap().parse().unwrap();
		drop(subscription);

		// `UnsubscribeOnDrop` calls `test_unsubscribe` from a spawned task; give it a moment.
		tokio::time::sleep(std::time::Duration::from_millis(200)).await;

		// A second call is destructive, so it doubles as the assertion.
		let params = serde_json::value::to_raw_value(&[sub_id]).ok();
		let still_subscribed: bool =
			methods.call("test_unsubscribe", Params(params)).await.unwrap();
		assert!(!still_subscribed, "drop should have already called `test_unsubscribe`");
	}
}
