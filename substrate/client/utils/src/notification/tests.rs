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

use super::*;
use futures::StreamExt;

#[derive(Clone)]
pub struct DummyTracingKey;
impl TracingKeyStr for DummyTracingKey {
	const TRACING_KEY: &'static str = "test_notification_stream";
}

type StringStream = NotificationStream<String, DummyTracingKey>;

#[test]
fn notification_channel_simple() {
	let (sender, stream) = StringStream::channel();

	let test_payload = String::from("test payload");
	let closure_payload = test_payload.clone();

	// Create a future to receive a single notification
	// from the stream and verify its payload.
	let future = stream.subscribe(100_000).take(1).for_each(move |payload| {
		let test_payload = closure_payload.clone();
		async move {
			assert_eq!(payload, test_payload);
		}
	});

	// Send notification.
	let r: std::result::Result<(), ()> = sender.notify(|| Ok(test_payload));
	r.unwrap();

	// Run receiver future.
	tokio_test::block_on(future);
}
