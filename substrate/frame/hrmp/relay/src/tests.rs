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

use crate::{mock::*, Error, Event};
use frame_support::{assert_noop, assert_ok};
use hrmp_primitives::{
	ChannelId, HrmpRegistry, MessageToRelay, MessageToRelayV1, ParaNotification, ParaRequest,
	ParaRequestV1,
};
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

#[test]
fn relay_request_forwards_the_asking_paras_id() {
	new_test_ext().execute_with(|| {
		let request = ParaRequest::V1(ParaRequestV1::CloseChannel { channel: CHANNEL });

		assert_ok!(Hrmp::relay_request(para_origin(CHANNEL.sender), request.clone()));

		assert_eq!(take_forwarded(), vec![(CHANNEL.sender, request)]);
		assert_eq!(hrmp_events(), vec![Event::RequestForwarded { para_id: CHANNEL.sender }]);
	});
}

#[test]
fn relay_request_is_only_for_a_parachain() {
	new_test_ext().execute_with(|| {
		let request = ParaRequest::V1(ParaRequestV1::CloseChannel { channel: CHANNEL });

		assert_noop!(
			Hrmp::relay_request(RuntimeOrigin::signed(ALICE), request.clone()),
			DispatchError::BadOrigin
		);
		assert_noop!(Hrmp::relay_request(RuntimeOrigin::root(), request), DispatchError::BadOrigin);
		assert!(take_forwarded().is_empty());
	});
}

#[test]
fn relay_request_fails_if_the_transport_refuses() {
	new_test_ext().execute_with(|| {
		let origin = para_origin(CHANNEL.sender);
		ForwardFails::set(true);

		assert_noop!(
			Hrmp::relay_request(
				origin,
				ParaRequest::V1(ParaRequestV1::CloseChannel { channel: CHANNEL })
			),
			Error::<Test>::ForwardFailed
		);
		assert!(take_forwarded().is_empty());
	});
}

#[test]
fn notify_para_reaches_the_transport() {
	new_test_ext().execute_with(|| {
		let notification = ParaNotification::ChannelAccepted { recipient: CHANNEL.recipient };

		assert_ok!(Hrmp::receive(
			RuntimeOrigin::root(),
			MessageToRelay::V1(MessageToRelayV1::NotifyPara {
				para_id: CHANNEL.sender,
				notification: notification.clone(),
			})
		));

		assert_eq!(take_notified(), vec![(CHANNEL.sender, notification)]);
		assert!(hrmp_events().is_empty());
	});
}

#[test]
fn notify_para_reports_a_refusing_transport() {
	new_test_ext().execute_with(|| {
		NotifyFails::set(true);

		assert_ok!(Hrmp::receive(
			RuntimeOrigin::root(),
			MessageToRelay::V1(MessageToRelayV1::NotifyPara {
				para_id: CHANNEL.sender,
				notification: ParaNotification::ChannelClosing {
					initiator: CHANNEL.recipient,
					sender: CHANNEL.sender,
					recipient: CHANNEL.recipient,
				},
			})
		));

		assert!(take_notified().is_empty());
		assert_eq!(hrmp_events(), vec![Event::NotifyFailed { para_id: CHANNEL.sender }]);
	});
}
