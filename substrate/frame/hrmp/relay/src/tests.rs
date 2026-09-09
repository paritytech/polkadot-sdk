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

//! Tests for `pallet-hrmp-relay`.
//!
//! One test per handler lands with the handler it covers. What is here is what can be asserted
//! while the bodies are `todo!()`.

use crate::mock::*;
use frame_support::assert_noop;
use hrmp_primitives::{ChannelId, HrmpRegistry, MessageToRelay, MessageToRelayV1};
use sp_runtime::DispatchError;

const CHANNEL: ChannelId = ChannelId { sender: 2000, recipient: 2001 };

#[test]
fn receive_is_only_for_the_channel_managing_parachain() {
	new_test_ext().execute_with(|| {
		let request = MessageToRelay::V1(MessageToRelayV1::OpenChannel {
			channel: CHANNEL,
			message_id: 0,
			max_capacity: 8,
			max_message_size: 1_024,
		});

		assert_noop!(
			Hrmp::receive(RuntimeOrigin::signed(ALICE), request),
			DispatchError::BadOrigin
		);
		assert!(!MockRegistry::exists(CHANNEL));
		assert!(take_sent().is_empty());
		assert!(hrmp_events().is_empty());
	});
}
