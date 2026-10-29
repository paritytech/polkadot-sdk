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
	mock::*, Call, Error, Event, Outbox, OutboxBounds, PendingRequests, PendingSides, WeightInfo,
	RECEIVE_CALL_INDEX,
};
use frame_support::{
	assert_noop, assert_ok,
	dispatch::GetDispatchInfo,
	traits::{GetCallIndex, Hooks},
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

/// The releases waiting in the outbox, oldest first.
fn outbox() -> Vec<MessageToPara> {
	let window = OutboxBounds::<Test>::get();
	(window.head..window.tail).map(|i| Outbox::<Test>::get(i).unwrap()).collect()
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

		let poke = DepositAction::PokeChannelDeposits;
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
		let req = request((1, 2), None, None, DepositAction::PokeChannelDeposits);
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

/// A request must not overtake a release that waits in the outbox, whatever its channel.
#[test]
fn request_refused_while_outbox_not_empty() {
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

/// A release the transport refuses waits in the outbox.
#[test]
fn release_queued_on_send_failure() {
	new_test_ext(|| {
		queue_release((1, 2));
		assert_eq!(outbox(), vec![release_message((1, 2))]);
		assert!(sent().is_empty());
		assert!(events().is_empty());
	});
}

/// A release waits behind those already in the outbox, even if the transport would take it.
#[test]
fn release_queued_behind_outbox() {
	new_test_ext(|| {
		queue_release((1, 2));
		assert_ok!(try_set(release_request((3, 4))));
		assert!(sent().is_empty());
		assert_eq!(outbox(), vec![release_message((1, 2)), release_message((3, 4))]);
	});
}

/// The outbox has no limit: no release is ever dropped, and all are sent once the transport
/// takes them again.
#[test]
fn release_never_dropped() {
	new_test_ext(|| {
		let channels: Vec<_> = (0..1_000).map(|sender| (sender, 10_000)).collect();
		for &channel in &channels {
			queue_release(channel);
		}
		assert!(events().is_empty());
		assert_eq!(outbox().len(), channels.len());

		poll();
		assert_eq!(sent(), channels.iter().copied().map(release_message).collect::<Vec<_>>());
		assert!(outbox().is_empty());
	});
}

/// The outbox keeps its order once its window has moved on.
#[test]
fn outbox_reused_after_flush() {
	new_test_ext(|| {
		queue_release((1, 2));
		queue_release((3, 4));
		poll();
		sent();
		assert_eq!(OutboxBounds::<Test>::get().head, 2);

		queue_release((5, 6));
		queue_release((7, 8));
		assert_eq!(outbox(), vec![release_message((5, 6)), release_message((7, 8))]);
		poll();
		assert_eq!(sent(), vec![release_message((5, 6)), release_message((7, 8))]);
		assert_eq!(Outbox::<Test>::iter_keys().count(), 0);
	});
}

/// `on_poll` sends what waits in the outbox, oldest first.
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
		assert!(outbox().is_empty());
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
		assert_eq!(outbox(), vec![release_message((1, 10)), release_message((2, 10))]);

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

		// No room to check the outbox.
		let mut meter = WeightMeter::with_limit(
			<() as WeightInfo>::flush_outbox(0).saturating_sub(Weight::from_parts(1, 0)),
		);
		HrmpRelay::on_poll(1, &mut meter);
		assert_eq!(meter.consumed(), Weight::zero());
		assert!(sent().is_empty());

		// Room for one message.
		let limit = <() as WeightInfo>::flush_outbox(2).saturating_sub(Weight::from_parts(1, 0));
		let mut meter = WeightMeter::with_limit(limit);
		HrmpRelay::on_poll(1, &mut meter);
		assert_eq!(meter.consumed(), <() as WeightInfo>::flush_outbox(1));
		assert_eq!(sent(), vec![release_message((0, 10))]);
		assert_eq!(outbox().len(), 2);
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

/// Governance can only answer a pending request.
#[test]
fn force_answer_unknown() {
	new_test_ext(|| {
		assert_noop!(
			HrmpRelay::force_answer(RuntimeOrigin::root(), 0, Ok(())),
			Error::<Test>::UnknownRequest
		);
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
