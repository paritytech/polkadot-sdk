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

use crate::{mock::*, Error, Event, PendingReleases};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use hrmp_primitives::{
	ChannelId, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1, ParaRequest,
	ParaRequestV1,
};
use sp_runtime::DispatchError;

fn hold_msg(amount: u128) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::Hold { key: sender_key(), amount })
}

fn release_msg(key: hrmp_primitives::DepositKey, amount: Option<u128>) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::Release { key, amount })
}

fn request(request: ParaRequestV1) -> ParaRequest {
	ParaRequest::V1(request)
}

#[test]
fn relay_request_dispatches_every_request_as_the_asking_para() {
	new_test_ext().execute_with(|| {
		// GIVEN para A acting as itself.
		let para_a = RuntimeOrigin::signed(PARA_A_ACCOUNT);

		// WHEN it sends each of the five requests
		for r in [
			ParaRequestV1::InitOpenChannel {
				recipient: PARA_B,
				proposed_max_capacity: 8,
				proposed_max_message_size: 1_024,
			},
			ParaRequestV1::AcceptOpenChannel { sender: PARA_B },
			ParaRequestV1::CloseChannel { channel: CHANNEL },
			ParaRequestV1::CancelOpenRequest { channel: CHANNEL, open_requests: 3 },
			ParaRequestV1::EstablishChannelWithSystem { target_system_chain: 1_000 },
		] {
			assert_ok!(HrmpRelay::relay_request(para_a.clone(), request(r)));
		}

		// THEN each reaches HRMP as para A, with its arguments.
		assert_eq!(
			HrmpCalls::get(),
			vec![
				HrmpCall::Init { para: PARA_A, recipient: PARA_B, capacity: 8, size: 1_024 },
				HrmpCall::Accept { para: PARA_A, sender: PARA_B },
				HrmpCall::Close { para: PARA_A, channel: CHANNEL },
				HrmpCall::Cancel { para: PARA_A, channel: CHANNEL, open_requests: 3 },
				HrmpCall::WithSystem { para: PARA_A, target: 1_000 },
			]
		);
		assert_eq!(events(), vec![Event::RequestServed { para: PARA_A }; 5]);
	});
}

#[test]
fn relay_request_needs_a_parachain_origin() {
	new_test_ext().execute_with(|| {
		let close = request(ParaRequestV1::CloseChannel { channel: CHANNEL });

		// A plain account, or root, is not a para.
		assert_noop!(
			HrmpRelay::relay_request(RuntimeOrigin::signed(ALICE), close.clone()),
			DispatchError::BadOrigin
		);
		assert_noop!(
			HrmpRelay::relay_request(RuntimeOrigin::root(), close),
			DispatchError::BadOrigin
		);
		assert!(HrmpCalls::get().is_empty());
	});
}

#[test]
fn relay_request_is_refused_while_the_para_is_rationed() {
	new_test_ext().execute_with(|| {
		// GIVEN para A is over its ration.
		Rationed::set(vec![PARA_A]);

		// WHEN it asks, THEN nothing reaches HRMP.
		assert_noop!(
			HrmpRelay::relay_request(
				RuntimeOrigin::signed(PARA_A_ACCOUNT),
				request(ParaRequestV1::CloseChannel { channel: CHANNEL })
			),
			Error::<Test>::RequestRefused
		);
		assert!(HrmpCalls::get().is_empty());

		// AND another para is unaffected.
		assert_ok!(HrmpRelay::relay_request(
			RuntimeOrigin::signed(PARA_B_ACCOUNT),
			request(ParaRequestV1::CloseChannel { channel: CHANNEL })
		));
	});
}

#[test]
fn relay_request_surfaces_the_hrmp_error() {
	new_test_ext().execute_with(|| {
		HrmpFails::set(Some(DispatchError::Other("refused")));

		assert_noop!(
			HrmpRelay::relay_request(
				RuntimeOrigin::signed(PARA_A_ACCOUNT),
				request(ParaRequestV1::AcceptOpenChannel { sender: PARA_B })
			),
			DispatchError::Other("refused")
		);
	});
}

#[test]
fn hold_sends_queued_releases_first() {
	new_test_ext().execute_with(|| {
		// GIVEN two releases queued.
		HrmpRelay::release(sender_key(), None);
		HrmpRelay::release(recipient_key(), Some(40));
		assert!(Sent::get().is_empty());

		// WHEN a hold is sent for a key that was just released
		assert_ok!(HrmpRelay::hold(sender_key(), 100));

		// THEN the parachain sees both releases before the hold.
		assert_eq!(
			Sent::get(),
			vec![
				release_msg(sender_key(), None),
				release_msg(recipient_key(), Some(40)),
				hold_msg(100)
			]
		);
		assert!(PendingReleases::<Test>::get().is_empty());
		assert_eq!(
			events(),
			vec![
				Event::ReleaseSent { key: sender_key(), amount: None },
				Event::ReleaseSent { key: recipient_key(), amount: Some(40) },
				Event::HoldSent { key: sender_key(), amount: 100 },
			]
		);
	});
}

#[test]
fn queued_releases_go_out_at_the_start_of_the_next_block() {
	new_test_ext().execute_with(|| {
		// GIVEN a release queued.
		HrmpRelay::release(sender_key(), None);
		assert!(Sent::get().is_empty());

		// WHEN the next block starts
		HrmpRelay::on_initialize(2);

		// THEN it is sent.
		assert_eq!(Sent::get(), vec![release_msg(sender_key(), None)]);
		assert!(PendingReleases::<Test>::get().is_empty());
	});
}

#[test]
fn a_release_the_transport_refuses_is_dropped_and_reported() {
	new_test_ext().execute_with(|| {
		HrmpRelay::release(sender_key(), None);
		SendFails::set(true);

		HrmpRelay::on_initialize(2);

		assert!(PendingReleases::<Test>::get().is_empty());
		assert_eq!(events(), vec![Event::ReleaseFailed { key: sender_key(), amount: None }]);
	});
}

#[test]
fn a_hold_the_transport_refuses_is_an_error() {
	new_test_ext().execute_with(|| {
		SendFails::set(true);

		assert_eq!(HrmpRelay::hold(sender_key(), 100), Err(()));
		assert!(events().is_empty());
	});
}

#[test]
fn receive_passes_the_hold_answer_on() {
	new_test_ext().execute_with(|| {
		let answer =
			|held| MessageToRelay::V1(MessageToRelayV1::HoldResult { key: sender_key(), held });

		// Only the deposit-holding parachain, or root, may answer.
		assert_noop!(
			HrmpRelay::receive(RuntimeOrigin::signed(ALICE), answer(true)),
			DispatchError::BadOrigin
		);
		assert_noop!(
			HrmpRelay::receive(RuntimeOrigin::signed(PARA_A_ACCOUNT), answer(true)),
			DispatchError::BadOrigin
		);

		assert_ok!(HrmpRelay::receive(RuntimeOrigin::signed(CORETIME), answer(true)));
		assert_ok!(HrmpRelay::receive(RuntimeOrigin::root(), answer(false)));

		assert_eq!(Answers::get(), vec![(sender_key(), true), (sender_key(), false)]);
		assert_eq!(
			events(),
			vec![
				Event::HoldAnswered { key: sender_key(), held: true },
				Event::HoldAnswered { key: sender_key(), held: false },
			]
		);
	});
}

#[test]
fn receive_serves_the_calls_users_make_on_the_deposit_holding_parachain() {
	new_test_ext().execute_with(|| {
		let poke = MessageToRelay::V1(MessageToRelayV1::PokeChannelDeposits { channel: CHANNEL });
		let system = ChannelId { sender: 1_000, recipient: 1_001 };
		let open = MessageToRelay::V1(MessageToRelayV1::EstablishSystemChannel { channel: system });

		// Only the deposit-holding parachain, or root, may ask.
		assert_noop!(
			HrmpRelay::receive(RuntimeOrigin::signed(PARA_A_ACCOUNT), poke.clone()),
			DispatchError::BadOrigin
		);

		assert_ok!(HrmpRelay::receive(RuntimeOrigin::signed(CORETIME), poke));
		assert_ok!(HrmpRelay::receive(RuntimeOrigin::signed(CORETIME), open.clone()));
		assert_eq!(
			HrmpCalls::get(),
			vec![HrmpCall::Poke { channel: CHANNEL }, HrmpCall::SystemChannel { channel: system }]
		);

		// A refusal is the call's error.
		HrmpFails::set(Some(DispatchError::Other("not system")));
		assert_noop!(
			HrmpRelay::receive(RuntimeOrigin::signed(CORETIME), open),
			DispatchError::Other("not system")
		);
	});
}
