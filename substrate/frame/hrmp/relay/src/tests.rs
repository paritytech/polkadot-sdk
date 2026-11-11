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

use crate::{
	mock::*, Call, Config, Error, Event, FailedReleases, PendingRequests, PendingSides,
	RequestWindows, WeightInfo, RECEIVE_CALL_INDEX,
};
use frame_support::{
	assert_noop, assert_ok,
	dispatch::GetDispatchInfo,
	traits::{Get, GetCallIndex, Hooks},
	weights::{Weight, WeightMeter},
};
use hrmp_primitives::{
	ChannelId, DepositError as WireDepositError, DepositSide, MessageToPara, MessageToParaV1,
	MessageToRelay, MessageToRelayV1,
};
use polkadot_primitives::{HrmpChannelId, Id as ParaId};
use polkadot_runtime_parachains::{
	hrmp::deposits::{
		ChannelDepositProvider, DepositAction, DepositChange, DepositChangeRequest,
		DepositChangeResponse, DepositError,
	},
	origin as parachains_origin,
};
use sp_runtime::DispatchError;

const INIT: DepositAction =
	DepositAction::InitOpenChannel { max_capacity: 8, max_message_size: 8, max_total_size: 64 };

fn wire(sender: u32, recipient: u32) -> ChannelId {
	ChannelId { sender, recipient }
}

/// A request for the channel `sender → recipient`, each side moving from `current` to `target`.
fn request(
	(sender, recipient): (u32, u32),
	sender_change: Option<(u128, u128)>,
	recipient_change: Option<(u128, u128)>,
	action: DepositAction,
) -> DepositChangeRequest {
	let change = |(current, target)| DepositChange { current, target };
	DepositChangeRequest {
		channel_id: HrmpChannelId { sender: sender.into(), recipient: recipient.into() },
		sender: sender_change.map(change),
		recipient: recipient_change.map(change),
		action,
	}
}

/// A request to release both sides of `sender → recipient`.
fn release_request(channel: (u32, u32)) -> DepositChangeRequest {
	request(channel, Some((10, 0)), Some((10, 0)), DepositAction::Release)
}

fn try_set(request: DepositChangeRequest) -> Result<DepositChangeResponse, DispatchError> {
	<HrmpRelay as ChannelDepositProvider>::try_set_deposit(request)
}

fn para_origin(para: u32) -> RuntimeOrigin {
	parachains_origin::Origin::Parachain(ParaId::from(para)).into()
}

fn holder() -> RuntimeOrigin {
	para_origin(HolderParaId::get().into())
}

fn answer(id: u64, result: Result<(), WireDepositError>) -> MessageToRelay {
	MessageToRelay::V1(MessageToRelayV1::DepositsSet { id, result })
}

fn release_message(channel: (u32, u32)) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::Release {
		channel: wire(channel.0, channel.1),
		sender: true,
		recipient: true,
	})
}

/// The failed releases waiting to be retried, as sent.
fn failed_releases() -> Vec<MessageToPara> {
	FailedReleases::<Test>::get().into_iter().map(Into::into).collect()
}

/// Takes the messages sent so far.
fn sent() -> Vec<MessageToPara> {
	SENT.take()
}

/// Takes the answers handed to `OnDepositSet` so far.
fn answers() -> Vec<(DepositChangeRequest, Result<(), DepositError>)> {
	ANSWERS.take()
}

/// Takes this pallet's events so far.
fn events() -> Vec<Event<Test>> {
	let events = System::events()
		.into_iter()
		.filter_map(|r| match r.event {
			RuntimeEvent::HrmpRelay(e) => Some(e),
			_ => None,
		})
		.collect();
	System::reset_events();
	events
}

/// Queues a release of `channel` while the transport refuses it.
fn queue_release(channel: (u32, u32)) {
	let accept = ACCEPT.replace(Some(0));
	assert_eq!(try_set(release_request(channel)), Ok(DepositChangeResponse::Applied));
	ACCEPT.set(accept);
}

fn poll() {
	HrmpRelay::on_poll(1, &mut WeightMeter::new());
}

/// A request of `sender` to open a channel to `recipient`.
fn init(sender: u32, recipient: u32) -> DepositChangeRequest {
	request((sender, recipient), Some((0, 10)), None, INIT)
}

/// A request to poke both sides of `channel`, made by `by`.
fn poke(channel: (u32, u32), by: Option<u32>) -> DepositChangeRequest {
	let action = DepositAction::PokeChannelDeposits { by: by.map(Into::into) };
	request(channel, Some((10, 20)), Some((10, 20)), action)
}

/// How many requests a para may make in its window.
fn max_requests() -> u32 {
	<Test as Config>::MaxRequestsPerWindow::get()
}

/// The window of `para`: the block it started at and the requests counted in it.
fn window(para: u32) -> Option<(u64, u32)> {
	RequestWindows::<Test>::get(ParaId::from(para))
}

/// Makes `para` use up the requests of its window, opening channels to the paras from 100 on.
fn use_up_requests(para: u32) {
	for i in 0..max_requests() {
		assert_ok!(try_set(init(para, 100 + i)));
	}
	sent();
	events();
}

/// The chain that holds the deposits encodes `receive` with [`RECEIVE_CALL_INDEX`].
#[test]
fn receive_call_index_ok() {
	let message = MessageToRelay::V1(MessageToRelayV1::DepositsSet { id: 0, result: Ok(()) });
	let call = Call::<Test>::receive { message };
	assert_eq!(call.get_call_index(), RECEIVE_CALL_INDEX);
}

/// A request is sent with its targets and waits for its answer, locking its sides.
#[test]
fn request_is_sent_and_pending() {
	new_test_ext(|| {
		let req = request((1, 2), Some((3, 10)), Some((4, 20)), INIT);
		assert_eq!(try_set(req.clone()), Ok(DepositChangeResponse::Pending));

		let message = MessageToParaV1::SetDeposits {
			id: 0,
			channel: wire(1, 2),
			sender: Some(10),
			recipient: Some(20),
		};
		assert_eq!(sent(), vec![MessageToPara::V1(message)]);
		assert_eq!(PendingRequests::<Test>::get(0), Some(req));
		assert_eq!(PendingSides::<Test>::get((wire(1, 2), DepositSide::Sender)), Some(0));
		assert_eq!(PendingSides::<Test>::get((wire(1, 2), DepositSide::Recipient)), Some(0));
		assert_eq!(events(), vec![Event::DepositRequested { id: 0, channel: wire(1, 2) }]);
		assert!(answers().is_empty());

		// The next request takes the next id.
		assert_ok!(try_set(request((1, 3), Some((0, 10)), None, INIT)));
		assert_eq!(events(), vec![Event::DepositRequested { id: 1, channel: wire(1, 3) }]);
	});
}

/// A side that waits for its answer refuses another request, and leaves no trace.
#[test]
fn pending_side_refuses() {
	new_test_ext(|| {
		assert_ok!(try_set(request((1, 2), Some((0, 10)), None, INIT)));
		assert_ok!(try_set(request(
			(1, 2),
			None,
			Some((0, 10)),
			DepositAction::AcceptOpenChannel { max_capacity: 1, max_message_size: 1 }
		)));
		sent();

		let poke = DepositAction::PokeChannelDeposits { by: None };
		for (sender, recipient) in
			[(Some((10, 20)), None), (None, Some((10, 20))), (Some((10, 20)), Some((10, 20)))]
		{
			assert_noop!(
				try_set(request((1, 2), sender, recipient, poke)),
				Error::<Test>::DepositPending
			);
		}
		assert!(sent().is_empty());
	});
}

/// A pending side locks neither the other side, nor the reverse channel, nor another channel.
#[test]
fn pending_side_does_not_block_others() {
	new_test_ext(|| {
		assert_ok!(try_set(request((1, 2), Some((0, 10)), None, INIT)));
		assert_ok!(try_set(request(
			(1, 2),
			None,
			Some((0, 10)),
			DepositAction::AcceptOpenChannel { max_capacity: 1, max_message_size: 1 }
		)));
		assert_ok!(try_set(request((2, 1), Some((0, 10)), None, INIT)));
		assert_ok!(try_set(request((1, 3), Some((0, 10)), None, INIT)));
		assert_eq!(sent().len(), 4);
	});
}

/// A request that changes no side is still sent and answered, and locks nothing.
#[test]
fn request_without_sides() {
	new_test_ext(|| {
		let req = request((1, 2), None, None, DepositAction::PokeChannelDeposits { by: None });
		assert_eq!(try_set(req.clone()), Ok(DepositChangeResponse::Pending));
		assert_eq!(PendingSides::<Test>::iter().count(), 0);

		assert_ok!(HrmpRelay::receive(holder(), answer(0, Ok(()))));
		assert_eq!(answers(), vec![(req, Ok(()))]);
	});
}

/// A request the transport refuses leaves no trace: no id taken, nothing pending, no event.
#[test]
fn send_failure_leaves_no_trace() {
	new_test_ext(|| {
		ACCEPT.set(Some(0));
		assert_noop!(
			try_set(request((1, 2), Some((0, 10)), Some((0, 10)), INIT)),
			Error::<Test>::SendFailed
		);

		// The next request takes id 0.
		ACCEPT.set(None);
		assert_ok!(try_set(request((1, 2), Some((0, 10)), Some((0, 10)), INIT)));
		assert_eq!(events(), vec![Event::DepositRequested { id: 0, channel: wire(1, 2) }]);
	});
}

/// A request must not overtake a release that failed, whatever its channel.
#[test]
fn request_refused_while_releases_failed() {
	new_test_ext(|| {
		queue_release((1, 2));
		assert_noop!(
			try_set(request((1, 2), Some((0, 10)), None, INIT)),
			Error::<Test>::TransportBusy
		);
		assert_noop!(
			try_set(request((3, 4), Some((0, 10)), None, INIT)),
			Error::<Test>::TransportBusy
		);
		assert!(sent().is_empty());

		poll();
		assert_eq!(sent(), vec![release_message((1, 2))]);
		assert_ok!(try_set(request((1, 2), Some((0, 10)), None, INIT)));
	});
}

/// A release is sent at once, applied, and never waits for an answer.
#[test]
fn release_sent_immediately() {
	new_test_ext(|| {
		let req = request((1, 2), Some((10, 0)), None, DepositAction::Release);
		assert_eq!(try_set(req), Ok(DepositChangeResponse::Applied));

		let message =
			MessageToParaV1::Release { channel: wire(1, 2), sender: true, recipient: false };
		assert_eq!(sent(), vec![MessageToPara::V1(message)]);
		assert_eq!(events(), vec![Event::ReleaseSent { channel: wire(1, 2) }]);
		assert_eq!(PendingRequests::<Test>::iter().count(), 0);
		assert_eq!(PendingSides::<Test>::iter().count(), 0);
	});
}

/// A release goes through while a side waits for its answer, and the answer is still taken.
#[test]
fn release_ignores_pending_side() {
	new_test_ext(|| {
		let req = request((1, 2), Some((0, 10)), None, INIT);
		assert_ok!(try_set(req.clone()));
		sent();

		assert_eq!(try_set(release_request((1, 2))), Ok(DepositChangeResponse::Applied));
		assert_eq!(sent(), vec![release_message((1, 2))]);
		assert_eq!(PendingSides::<Test>::get((wire(1, 2), DepositSide::Sender)), Some(0));

		assert_ok!(HrmpRelay::receive(holder(), answer(0, Ok(()))));
		assert_eq!(answers(), vec![(req, Ok(()))]);
	});
}

/// A release the transport refuses waits to be retried.
#[test]
fn release_queued_on_send_failure() {
	new_test_ext(|| {
		queue_release((1, 2));
		assert_eq!(failed_releases(), vec![release_message((1, 2))]);
		assert!(sent().is_empty());
		assert!(events().is_empty());
	});
}

/// A release waits behind those that already failed, even if the transport would take it.
#[test]
fn release_queued_behind_failed_releases() {
	new_test_ext(|| {
		queue_release((1, 2));
		assert_ok!(try_set(release_request((3, 4))));
		assert!(sent().is_empty());
		assert_eq!(failed_releases(), vec![release_message((1, 2)), release_message((3, 4))]);
	});
}

/// A release is dropped when too many failed ones already wait, and still reported as applied.
#[test]
fn release_dropped_when_failed_releases_full() {
	new_test_ext(|| {
		for sender in 0..4 {
			queue_release((sender, 10));
		}
		events();

		ACCEPT.set(Some(0));
		assert_eq!(try_set(release_request((5, 10))), Ok(DepositChangeResponse::Applied));
		assert_eq!(events(), vec![Event::ReleaseDropped { channel: wire(5, 10) }]);
		assert_eq!(FailedReleases::<Test>::decode_len(), Some(4));
	});
}

/// `on_poll` retries the failed releases, oldest first.
#[test]
fn poll_sends_in_order() {
	new_test_ext(|| {
		poll();
		assert!(sent().is_empty());

		for sender in 0..3 {
			queue_release((sender, 10));
		}
		poll();
		let channels = [(0, 10), (1, 10), (2, 10)];
		assert_eq!(sent(), channels.map(release_message).to_vec());
		assert_eq!(
			events(),
			channels.map(|(s, r)| Event::ReleaseSent { channel: wire(s, r) }).to_vec()
		);
		assert!(FailedReleases::<Test>::get().is_empty());
	});
}

/// `on_poll` stops at the first message the transport refuses, and keeps the rest in order.
#[test]
fn poll_stops_at_first_failure() {
	new_test_ext(|| {
		for sender in 0..3 {
			queue_release((sender, 10));
		}
		ACCEPT.set(Some(1));
		poll();
		assert_eq!(sent(), vec![release_message((0, 10))]);
		assert_eq!(failed_releases(), vec![release_message((1, 10)), release_message((2, 10))]);

		// It goes on in a later block.
		ACCEPT.set(None);
		poll();
		assert_eq!(sent(), vec![release_message((1, 10)), release_message((2, 10))]);
	});
}

/// `on_poll` sends no more than its meter has room for.
#[test]
fn poll_respects_weight_meter() {
	new_test_ext(|| {
		for sender in 0..3 {
			queue_release((sender, 10));
		}

		// No room to check the failed releases.
		let mut meter = WeightMeter::with_limit(
			<() as WeightInfo>::retry_failed_releases(0).saturating_sub(Weight::from_parts(1, 0)),
		);
		HrmpRelay::on_poll(1, &mut meter);
		assert_eq!(meter.consumed(), Weight::zero());
		assert!(sent().is_empty());

		// Room for one message.
		let limit =
			<() as WeightInfo>::retry_failed_releases(2).saturating_sub(Weight::from_parts(1, 0));
		let mut meter = WeightMeter::with_limit(limit);
		HrmpRelay::on_poll(1, &mut meter);
		assert_eq!(meter.consumed(), <() as WeightInfo>::retry_failed_releases(1));
		assert_eq!(sent(), vec![release_message((0, 10))]);
		assert_eq!(FailedReleases::<Test>::decode_len(), Some(2));
	});
}

/// Only the para that holds the deposits may answer.
#[test]
fn receive_only_from_holder() {
	new_test_ext(|| {
		assert_ok!(try_set(request((1, 2), Some((0, 10)), None, INIT)));
		for origin in [para_origin(1), RuntimeOrigin::root(), RuntimeOrigin::signed(1)] {
			assert_noop!(HrmpRelay::receive(origin, answer(0, Ok(()))), DispatchError::BadOrigin);
		}
		assert_ok!(HrmpRelay::receive(holder(), answer(0, Ok(()))));
	});
}

/// An answer hands the request back unchanged, and unlocks its sides.
#[test]
fn answer_resumes_request() {
	new_test_ext(|| {
		let req = request((1, 2), Some((3, 10)), Some((4, 20)), INIT);
		assert_ok!(try_set(req.clone()));
		events();

		let result = Err(WireDepositError::InsufficientFunds);
		assert_ok!(HrmpRelay::receive(holder(), answer(0, result)));
		assert_eq!(answers(), vec![(req, Err(DepositError::InsufficientFunds))]);
		assert_eq!(events(), vec![Event::DepositAnswered { id: 0, result }]);
		assert_eq!(PendingRequests::<Test>::iter().count(), 0);
		assert_eq!(PendingSides::<Test>::iter().count(), 0);

		// The sides take requests again.
		assert_ok!(try_set(request((1, 2), Some((3, 10)), Some((4, 20)), INIT)));
	});
}

/// An answer for no pending request is ignored, without failing the message.
#[test]
fn unknown_answer_is_ignored() {
	new_test_ext(|| {
		assert_ok!(HrmpRelay::receive(holder(), answer(7, Ok(()))));
		assert_eq!(events(), vec![Event::UnknownAnswer { id: 7 }]);
		assert!(answers().is_empty());
	});
}

/// A second answer for the same request is ignored.
#[test]
fn duplicate_answer_is_ignored() {
	new_test_ext(|| {
		assert_ok!(try_set(request((1, 2), Some((0, 10)), None, INIT)));
		assert_ok!(HrmpRelay::receive(holder(), answer(0, Ok(()))));
		answers();
		events();

		assert_ok!(HrmpRelay::receive(holder(), answer(0, Ok(()))));
		assert_eq!(events(), vec![Event::UnknownAnswer { id: 0 }]);
		assert!(answers().is_empty());
	});
}

/// Answers may come in any order, each going back to its own request.
#[test]
fn answers_out_of_order() {
	new_test_ext(|| {
		let first = request((1, 2), Some((0, 10)), None, INIT);
		let second = request((3, 4), Some((0, 20)), None, INIT);
		assert_ok!(try_set(first.clone()));
		assert_ok!(try_set(second.clone()));

		assert_ok!(HrmpRelay::receive(holder(), answer(1, Err(WireDepositError::Other))));
		assert_ok!(HrmpRelay::receive(holder(), answer(0, Ok(()))));
		assert_eq!(answers(), vec![(second, Err(DepositError::Other)), (first, Ok(()))]);
	});
}

/// `receive` and `force_answer` are charged the dearest answer a request can get. A release
/// gets no answer, so its weight does not count.
#[test]
fn receive_weight_includes_max_answer_weight() {
	let expected = <() as WeightInfo>::receive() + Weight::from_parts(4, 40);
	let call = Call::<Test>::receive { message: answer(0, Ok(())) };
	assert_eq!(call.get_dispatch_info().call_weight, expected);
	let call = Call::<Test>::force_answer { id: 0, result: Ok(()) };
	assert_eq!(call.get_dispatch_info().call_weight, expected);
}

/// Only governance answers on the parachain's behalf.
#[test]
fn force_answer_requires_root() {
	new_test_ext(|| {
		assert_ok!(try_set(request((1, 2), Some((0, 10)), None, INIT)));
		for origin in [holder(), RuntimeOrigin::signed(1)] {
			assert_noop!(HrmpRelay::force_answer(origin, 0, Ok(())), DispatchError::BadOrigin);
		}
	});
}

/// A forced answer for no pending request is ignored.
#[test]
fn force_answer_unknown() {
	new_test_ext(|| {
		assert_ok!(HrmpRelay::force_answer(RuntimeOrigin::root(), 0, Ok(())));
		assert_eq!(events(), vec![Event::UnknownAnswer { id: 0 }]);
		assert!(answers().is_empty());
	});
}

/// A forced answer acts like the parachain's, which is then ignored if it comes late.
#[test]
fn force_answer_resumes() {
	new_test_ext(|| {
		let req = request((1, 2), Some((0, 10)), None, INIT);
		assert_ok!(try_set(req.clone()));
		events();

		let result = Err(WireDepositError::Other);
		assert_ok!(HrmpRelay::force_answer(RuntimeOrigin::root(), 0, result));
		assert_eq!(answers(), vec![(req, Err(DepositError::Other))]);
		assert_eq!(events(), vec![Event::DepositAnswered { id: 0, result }]);
		assert_eq!(PendingSides::<Test>::iter().count(), 0);

		assert_ok!(HrmpRelay::receive(holder(), answer(0, Ok(()))));
		assert_eq!(events(), vec![Event::UnknownAnswer { id: 0 }]);
		assert!(answers().is_empty());
	});
}

/// A request to open a channel is counted for its sender. One more than it may make in its
/// window is refused and leaves no trace.
#[test]
fn open_requests_are_counted_for_the_sender() {
	new_test_ext(|| {
		for i in 0..max_requests() {
			assert_ok!(try_set(init(1, 2 + i)));
		}
		assert_eq!(window(1), Some((1, max_requests())));
		// The recipient is not counted.
		assert_eq!(window(2), None);
		sent();

		assert_noop!(try_set(init(1, 50)), Error::<Test>::TooManyRequests);
		assert!(sent().is_empty());

		// Another para is not held back, even to open a channel to the first.
		assert_ok!(try_set(init(50, 1)));
		assert_eq!(window(50), Some((1, 1)));
	});
}

/// A request to accept a channel is counted for its recipient, in the same window as the
/// channels it opens.
#[test]
fn accept_requests_are_counted_for_the_recipient() {
	new_test_ext(|| {
		let accept_action =
			DepositAction::AcceptOpenChannel { max_capacity: 8, max_message_size: 8 };
		let accept =
			|sender, recipient| request((sender, recipient), None, Some((0, 10)), accept_action);
		for i in 0..max_requests() {
			assert_ok!(try_set(accept(2 + i, 1)));
		}
		assert_eq!(window(1), Some((1, max_requests())));
		// The sender is not counted.
		assert_eq!(window(2), None);

		assert_noop!(try_set(accept(50, 1)), Error::<Test>::TooManyRequests);
		assert_noop!(try_set(init(1, 50)), Error::<Test>::TooManyRequests);
	});
}

/// A poke is counted for the side that makes it, never for the other side, and for neither
/// when it comes from someone else.
#[test]
fn pokes_are_counted_for_the_side_that_asks() {
	new_test_ext(|| {
		assert_ok!(try_set(poke((1, 2), Some(2))));
		assert_eq!((window(1), window(2)), (None, Some((1, 1))));
		assert_ok!(try_set(poke((3, 4), None)));
		assert_eq!((window(3), window(4)), (None, None));

		// A para that made all its requests cannot poke its channels, but the other side of
		// each can, and so can someone else.
		use_up_requests(5);
		assert_noop!(try_set(poke((5, 6), Some(5))), Error::<Test>::TooManyRequests);
		assert_noop!(try_set(poke((7, 5), Some(5))), Error::<Test>::TooManyRequests);
		assert_ok!(try_set(poke((5, 6), Some(6))));
		assert_ok!(try_set(poke((7, 5), None)));
		assert_eq!(window(5), Some((1, max_requests())));
	});
}

/// A channel opened for both sides at once comes from neither of them, and a release is never
/// refused: they are not counted, and not held back for paras that made all their requests.
#[test]
fn force_open_and_release_are_not_counted() {
	new_test_ext(|| {
		use_up_requests(1);
		use_up_requests(2);

		let force_open = DepositAction::InitAndAcceptOpenChannel {
			max_capacity: 8,
			max_message_size: 8,
			max_total_size: 64,
		};
		assert_eq!(
			try_set(request((1, 2), Some((0, 10)), Some((0, 10)), force_open)),
			Ok(DepositChangeResponse::Pending)
		);
		assert_eq!(try_set(release_request((2, 1))), Ok(DepositChangeResponse::Applied));
		assert_eq!(sent().len(), 2);
		assert_eq!(window(1), Some((1, max_requests())));
		assert_eq!(window(2), Some((1, max_requests())));
	});
}

/// A request refused for another reason is not counted.
#[test]
fn refused_request_is_not_counted() {
	new_test_ext(|| {
		assert_ok!(try_set(init(1, 2)));

		// Its side awaits its answer.
		assert_noop!(try_set(init(1, 2)), Error::<Test>::DepositPending);
		// The transport refuses it.
		ACCEPT.set(Some(0));
		assert_noop!(try_set(init(1, 3)), Error::<Test>::SendFailed);
		ACCEPT.set(None);
		// A release waits for the transport.
		queue_release((8, 9));
		assert_noop!(try_set(init(1, 3)), Error::<Test>::TransportBusy);

		assert_eq!(window(1), Some((1, 1)));
	});
}

/// A window lasts `RequestWindow` blocks from the first request counted in it. The next one
/// starts at the next request, not right when the previous one ends.
#[test]
fn window_restarts_at_the_next_request() {
	new_test_ext(|| {
		let len: u64 = <Test as Config>::RequestWindow::get();
		use_up_requests(1);
		assert_eq!(window(1), Some((1, max_requests())));

		// The window started at block 1: its last block is `len`.
		System::set_block_number(len);
		assert_noop!(try_set(init(1, 200)), Error::<Test>::TooManyRequests);
		System::set_block_number(1 + len);
		assert_ok!(try_set(init(1, 200)));
		assert_eq!(window(1), Some((1 + len, 1)));

		// Long after that window is over, the next request starts a whole new one.
		System::set_block_number(10 * len);
		assert_ok!(try_set(init(1, 300)));
		assert_eq!(window(1), Some((10 * len, 1)));
		for i in 1..max_requests() {
			assert_ok!(try_set(init(1, 300 + i)));
		}
		System::set_block_number(11 * len - 1);
		assert_noop!(try_set(init(1, 400)), Error::<Test>::TooManyRequests);
		System::set_block_number(11 * len);
		assert_ok!(try_set(init(1, 400)));
		assert_eq!(window(1), Some((11 * len, 1)));
	});
}
