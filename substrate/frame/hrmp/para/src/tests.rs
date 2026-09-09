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

//! Tests for `pallet-hrmp-para`.
//!
//! One test per flow lands with the extrinsic it covers. What is here is what can be asserted
//! while the bodies are `todo!()`.

use crate::{mock::*, Channels, Requests};
use frame_support::assert_noop;
use hrmp_primitives::{ChannelId, MessageToPara, MessageToParaV1, ParaRequest, ParaRequestV1};
use sp_runtime::DispatchError;

const CHANNEL: ChannelId = ChannelId { sender: 2000, recipient: 2001 };

#[test]
fn genesis_holds_no_channels_and_no_requests() {
	new_test_ext().execute_with(|| {
		assert_eq!(Requests::<Test>::iter().count(), 0);
		assert_eq!(Channels::<Test>::iter().count(), 0);
		assert!(take_sent().is_empty());
		assert!(hrmp_events().is_empty());
	});
}

#[test]
fn receive_is_only_for_the_relay_chain() {
	new_test_ext().execute_with(|| {
		let report = MessageToPara::V1(MessageToParaV1::OpenChannelResponse {
			channel: CHANNEL,
			message_id: 0,
			outcome: Ok((8, 1_024)),
		});

		assert_noop!(
			Hrmp::receive(RuntimeOrigin::signed(ALICE), report.clone()),
			DispatchError::BadOrigin
		);
		assert_noop!(Hrmp::receive(para_origin(CHANNEL.sender), report), DispatchError::BadOrigin);
	});
}

#[test]
fn receive_request_is_only_for_the_relay_chain() {
	new_test_ext().execute_with(|| {
		let request = ParaRequest::V1(ParaRequestV1::CloseChannel { channel: CHANNEL });

		assert_noop!(
			Hrmp::receive_request(RuntimeOrigin::signed(ALICE), CHANNEL.sender, request.clone()),
			DispatchError::BadOrigin
		);
		assert_noop!(
			Hrmp::receive_request(para_origin(CHANNEL.sender), CHANNEL.sender, request),
			DispatchError::BadOrigin
		);
	});
}
