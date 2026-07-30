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
//! process, replacing the loopback WebSocket connection.
//!
//! Only the transport changes: requests are still serialized to JSON and their SCALE payloads
//! still decoded by subxt. Skipping that needs subxt's `Backend` trait, which is sealed today,
//! so this is the lowest layer currently available to us.

use futures::stream;
use jsonrpsee::{
	core::{server::Methods, traits::ToRpcParams},
	server::RpcModule,
	types::SubscriptionId,
};
use serde_json::value::RawValue;
use std::sync::Arc;
use subxt::rpcs::{
	Error as RpcError, UserError,
	client::{RawRpcFuture, RawRpcSubscription, RpcClientT},
};

/// Same buffer subxt's WebSocket client uses, to keep backpressure behaviour identical.
const SUBSCRIPTION_BUFFER_CAPACITY: usize = 4096;

/// `sc-service` registers this only on the network-facing module, but subxt's backend needs it.
const RPC_METHODS: &str = "rpc_methods";

/// A [`RpcClientT`] that calls into a node's [`RpcModule`] directly.
///
/// ```ignore
/// let rpc_handlers = sc_service::spawn_tasks(params)?;
/// let rpc_client = RpcClient::new(InProcessRpcClient::new(rpc_handlers.handle()));
/// let api = OnlineClient::<SrcChainConfig>::from_rpc_client(rpc_client).await?;
/// ```
#[derive(Clone, Debug)]
pub struct InProcessRpcClient {
	methods: Methods,
	/// `None` when the module answers `rpc_methods` itself.
	rpc_methods_response: Option<Arc<RawValue>>,
	subscription_buffer_capacity: usize,
}

impl InProcessRpcClient {
	/// Wrap the in-memory RPC module of a running node.
	pub fn new(rpc_module: Arc<RpcModule<()>>) -> Self {
		Self::from_methods(rpc_module.as_ref().clone().into())
	}

	/// Wrap an already extracted set of [`Methods`].
	pub fn from_methods(methods: Methods) -> Self {
		let rpc_methods_response = (!methods.method_names().any(|name| name == RPC_METHODS))
			.then(|| render_rpc_methods(&methods));

		Self {
			methods,
			rpc_methods_response,
			subscription_buffer_capacity: SUBSCRIPTION_BUFFER_CAPACITY,
		}
	}

	/// Override how many notifications buffer before the node's subscription task has to wait.
	pub fn with_subscription_buffer_capacity(mut self, capacity: usize) -> Self {
		self.subscription_buffer_capacity = capacity.max(1);
		self
	}
}

impl RpcClientT for InProcessRpcClient {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<RawValue>>,
	) -> RawRpcFuture<'a, Box<RawValue>> {
		Box::pin(async move {
			if method == RPC_METHODS {
				if let Some(response) = &self.rpc_methods_response {
					return Ok(clone_raw_value(response));
				}
			}

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
		_unsub: &'a str,
	) -> RawRpcFuture<'a, RawRpcSubscription> {
		Box::pin(async move {
			let subscription = self
				.methods
				.subscribe(sub, Params(params), self.subscription_buffer_capacity)
				.await
				.map_err(into_rpc_error)?;

			let id = match subscription.subscription_id() {
				SubscriptionId::Str(id) => Some(id.to_string()),
				SubscriptionId::Num(id) => Some(id.to_string()),
			};

			// No explicit unsubscribe: dropping the stream closes the channel the node writes
			// into, which unwinds its subscription task.
			let stream = stream::unfold(subscription, |mut subscription| async move {
				let next = subscription.next::<Box<RawValue>>().await?;
				Some((next.map(|(value, _id)| value).map_err(into_rpc_error), subscription))
			});

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

fn render_rpc_methods(methods: &Methods) -> Arc<RawValue> {
	let mut names = methods.method_names().collect::<Vec<_>>();
	names.push(RPC_METHODS);
	names.sort_unstable();

	serde_json::value::to_raw_value(&serde_json::json!({ "methods": names }))
		.expect("a list of strings is always serializable; qed")
		.into()
}

fn clone_raw_value(value: &RawValue) -> Box<RawValue> {
	RawValue::from_string(value.get().to_owned())
		.expect("the value was parsed from valid JSON; qed")
}

#[cfg(test)]
mod tests {
	use super::*;
	use jsonrpsee::core::{RpcResult, server::SubscriptionMessage};
	use subxt::rpcs::{RpcClient, rpc_params};

	#[derive(serde::Deserialize)]
	struct RpcMethods {
		methods: Vec<String>,
	}

	fn test_module() -> Arc<RpcModule<()>> {
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
		Arc::new(module)
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
	async fn rpc_methods_defers_to_the_module() {
		let mut module = RpcModule::new(());
		module
			.register_method::<RpcResult<serde_json::Value>, _>("rpc_methods", |_, _, _| {
				Ok(serde_json::json!({ "methods": ["only_this"] }))
			})
			.unwrap();

		let client = RpcClient::new(InProcessRpcClient::new(Arc::new(module)));
		let methods: RpcMethods = client.request("rpc_methods", rpc_params![]).await.unwrap();
		assert_eq!(methods.methods, vec!["only_this"]);
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

	/// Nothing sends an unsubscribe call, so the node-side task has to unwind on its own.
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

		let client = RpcClient::new(InProcessRpcClient::new(Arc::new(module)));
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
}
