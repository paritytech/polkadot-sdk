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
	ChannelId, FailureReason, HrmpRegistry, MessageToPara, MessageToParaV1, MessageToRelay,
	MessageToRelayV1, ParaNotification, ParaRequest, ParaRequestV1,
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

const MESSAGE_ID: u64 = 7;
const CAPACITY: u32 = 8;
const MESSAGE_SIZE: u32 = 1_024;

fn open_channel() -> sp_runtime::DispatchResult {
	Hrmp::receive(
		RuntimeOrigin::root(),
		MessageToRelay::V1(MessageToRelayV1::OpenChannel {
			channel: CHANNEL,
			message_id: MESSAGE_ID,
			max_capacity: CAPACITY,
			max_message_size: MESSAGE_SIZE,
		}),
	)
}

fn response(outcome: Result<(u32, u32), FailureReason>) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::OpenChannelResponse {
		channel: CHANNEL,
		message_id: MESSAGE_ID,
		outcome,
	})
}

#[test]
fn open_channel_writes_the_registry_and_answers() {
	new_test_ext().execute_with(|| {
		assert_ok!(open_channel());

		assert!(MockRegistry::exists(CHANNEL));
		assert_eq!(take_sent(), vec![response(Ok((CAPACITY, MESSAGE_SIZE)))]);
		// Both ends are told, since only the relay chain knows the channel now exists.
		assert_eq!(
			take_notified(),
			vec![
				(CHANNEL.sender, ParaNotification::ChannelOpened { channel: CHANNEL }),
				(CHANNEL.recipient, ParaNotification::ChannelOpened { channel: CHANNEL }),
			]
		);
		assert_eq!(
			hrmp_events(),
			vec![Event::ChannelOpened { channel: CHANNEL, message_id: MESSAGE_ID }]
		);
	});
}

#[test]
fn a_refused_open_channel_is_reported_back() {
	new_test_ext().execute_with(|| {
		RegistryRefuses::set(Some(FailureReason::LimitExceeded));

		assert_ok!(open_channel());

		assert!(!MockRegistry::exists(CHANNEL));
		assert_eq!(take_sent(), vec![response(Err(FailureReason::LimitExceeded))]);
		let failure = ParaNotification::ChannelOpenFailure {
			channel: CHANNEL,
			reason: FailureReason::LimitExceeded,
		};
		assert_eq!(
			take_notified(),
			vec![(CHANNEL.sender, failure.clone()), (CHANNEL.recipient, failure)]
		);
		assert_eq!(
			hrmp_events(),
			vec![Event::OpenChannelRejected {
				channel: CHANNEL,
				message_id: MESSAGE_ID,
				reason: FailureReason::LimitExceeded,
			}]
		);
	});
}

#[test]
fn a_refusing_transport_does_not_undo_the_channel() {
	new_test_ext().execute_with(|| {
		SendFails::set(true);

		assert_ok!(open_channel());

		// The relay chain has committed the channel, so the bounced report is only surfaced.
		assert!(MockRegistry::exists(CHANNEL));
		assert!(take_sent().is_empty());
		assert_eq!(take_notified().len(), 2);
		assert_eq!(
			hrmp_events(),
			vec![
				Event::ChannelOpened { channel: CHANNEL, message_id: MESSAGE_ID },
				Event::ReportFailed { para_id: CHANNEL.sender, message_id: MESSAGE_ID },
			]
		);
	});
}

#[test]
fn a_refusing_notify_transport_does_not_undo_the_channel() {
	new_test_ext().execute_with(|| {
		NotifyFails::set(true);

		assert_ok!(open_channel());

		// Same reasoning as a bounced report: the channel is already committed on both chains.
		assert!(MockRegistry::exists(CHANNEL));
		assert!(take_notified().is_empty());
		assert_eq!(take_sent(), vec![response(Ok((CAPACITY, MESSAGE_SIZE)))]);
		assert_eq!(
			hrmp_events(),
			vec![
				Event::ChannelOpened { channel: CHANNEL, message_id: MESSAGE_ID },
				Event::NotifyFailed { para_id: CHANNEL.sender },
				Event::NotifyFailed { para_id: CHANNEL.recipient },
			]
		);
	});
}

/// Two system chains, which is what `OpenSystemChannel` is for.
const SYSTEM_CHANNEL: ChannelId = ChannelId { sender: 1000, recipient: 1001 };

fn open_system_channel() -> sp_runtime::DispatchResult {
	Hrmp::receive(
		RuntimeOrigin::root(),
		MessageToRelay::V1(MessageToRelayV1::OpenSystemChannel {
			channel: SYSTEM_CHANNEL,
			message_id: MESSAGE_ID,
		}),
	)
}

fn system_response(outcome: Result<(u32, u32), FailureReason>) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::OpenChannelResponse {
		channel: SYSTEM_CHANNEL,
		message_id: MESSAGE_ID,
		outcome,
	})
}

#[test]
fn open_system_channel_writes_the_registry_and_answers_with_its_sizes() {
	new_test_ext().execute_with(|| {
		assert_ok!(open_system_channel());

		assert!(MockRegistry::exists(SYSTEM_CHANNEL));
		// The request named no sizes, so the answer is where the parachain learns them.
		assert_eq!(take_sent(), vec![system_response(Ok(SYSTEM_CHANNEL_SIZES))]);
		assert_eq!(
			take_notified(),
			vec![
				(
					SYSTEM_CHANNEL.sender,
					ParaNotification::ChannelOpened { channel: SYSTEM_CHANNEL }
				),
				(
					SYSTEM_CHANNEL.recipient,
					ParaNotification::ChannelOpened { channel: SYSTEM_CHANNEL }
				),
			]
		);
		assert_eq!(
			hrmp_events(),
			vec![Event::ChannelOpened { channel: SYSTEM_CHANNEL, message_id: MESSAGE_ID }]
		);
	});
}

#[test]
fn a_refused_system_channel_is_reported_back() {
	new_test_ext().execute_with(|| {
		RegistryRefuses::set(Some(FailureReason::InvalidPara));

		// A refusal is reported, not raised.
		assert_ok!(open_system_channel());

		assert!(!MockRegistry::exists(SYSTEM_CHANNEL));
		assert_eq!(take_sent(), vec![system_response(Err(FailureReason::InvalidPara))]);
		let failure = ParaNotification::ChannelOpenFailure {
			channel: SYSTEM_CHANNEL,
			reason: FailureReason::InvalidPara,
		};
		assert_eq!(
			take_notified(),
			vec![(SYSTEM_CHANNEL.sender, failure.clone()), (SYSTEM_CHANNEL.recipient, failure)]
		);
		assert_eq!(
			hrmp_events(),
			vec![Event::OpenChannelRejected {
				channel: SYSTEM_CHANNEL,
				message_id: MESSAGE_ID,
				reason: FailureReason::InvalidPara,
			}]
		);
	});
}
