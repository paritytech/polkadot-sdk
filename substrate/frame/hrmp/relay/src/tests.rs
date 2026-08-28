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

use crate::{mock::*, Error, Event};
use frame_support::{assert_noop, assert_ok};
use hrmp_primitives::{
	ChannelId, FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1,
	Outcome, ParaId,
};
use sp_runtime::{DispatchError, DispatchResult};

/// The message id every test request carries. An arbitrary value: this side only echoes what the
/// parachain sent.
const MSG_ID: u64 = 5;

const CAPACITY: u32 = 8;
const MESSAGE_SIZE: u32 = 1_024;

fn chan(sender: ParaId, recipient: ParaId) -> ChannelId {
	ChannelId { sender, recipient }
}

fn init_msg(channel: ChannelId) -> MessageToRelay {
	MessageToRelay::V1(MessageToRelayV1::InitOpenChannel {
		channel,
		message_id: MSG_ID,
		max_capacity: CAPACITY,
		max_message_size: MESSAGE_SIZE,
	})
}

fn accept_msg(channel: ChannelId) -> MessageToRelay {
	MessageToRelay::V1(MessageToRelayV1::AcceptOpenChannel { channel, message_id: MSG_ID })
}

fn close_msg(channel: ChannelId, initiator: ParaId) -> MessageToRelay {
	MessageToRelay::V1(MessageToRelayV1::CloseChannel {
		channel,
		message_id: MSG_ID,
		initiator,
	})
}

fn cancel_msg(channel: ChannelId) -> MessageToRelay {
	MessageToRelay::V1(MessageToRelayV1::CancelOpenRequest { channel, message_id: MSG_ID })
}

fn open_report(channel: ChannelId, outcome: Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::OpenResponse { channel, message_id: MSG_ID, outcome })
}

fn accept_report(channel: ChannelId, outcome: Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::AcceptResponse { channel, message_id: MSG_ID, outcome })
}

fn close_report(channel: ChannelId, outcome: Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::CloseResponse { channel, message_id: MSG_ID, outcome })
}

fn cancel_report(channel: ChannelId, outcome: Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::CancelResponse { channel, message_id: MSG_ID, outcome })
}

/// Drive a channel all the way to open on the registry.
fn opened(channel: ChannelId) {
	assert_ok!(Hrmp::init_open_channel(RuntimeOrigin::root(), init_msg(channel)));
	assert_ok!(Hrmp::accept_open_channel(RuntimeOrigin::root(), accept_msg(channel)));
	let _ = take_sent();
	let _ = hrmp_events();
}

mod origins {
	use super::*;

	#[test]
	fn users_cannot_reach_any_of_it() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			for call in [
				Hrmp::init_open_channel(RuntimeOrigin::signed(ALICE), init_msg(channel)),
				Hrmp::accept_open_channel(RuntimeOrigin::signed(ALICE), accept_msg(channel)),
				Hrmp::close_channel(RuntimeOrigin::signed(ALICE), close_msg(channel, PARA_A)),
				Hrmp::cancel_open_request(RuntimeOrigin::signed(ALICE), cancel_msg(channel)),
			] {
				assert_noop!(call, DispatchError::BadOrigin);
			}
		});
	}

	#[test]
	fn a_call_refuses_a_message_it_does_not_serve() {
		new_test_ext().execute_with(|| {
			// The wire format is one enum, so each call has to check it got its own variant.
			assert_noop!(
				Hrmp::init_open_channel(
					RuntimeOrigin::root(),
					accept_msg(chan(PARA_A, PARA_B))
				),
				Error::<Test>::UnexpectedMessage
			);
			assert_noop!(
				Hrmp::close_channel(RuntimeOrigin::root(), init_msg(chan(PARA_A, PARA_B))),
				Error::<Test>::UnexpectedMessage
			);
		});
	}
}

mod init_open_channel {
	use super::*;

	#[test]
	fn records_the_request_and_reports_success() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);

			assert_ok!(Hrmp::init_open_channel(RuntimeOrigin::root(), init_msg(channel)));

			assert_eq!(Requests::get(), vec![(channel, false)]);
			assert_eq!(take_sent(), vec![open_report(channel, Ok(()))]);
			assert_eq!(
				hrmp_events(),
				vec![Event::OpenRequested { channel, message_id: MSG_ID }]
			);
		});
	}

	#[test]
	fn a_refusal_is_reported_and_is_not_an_extrinsic_failure() {
		new_test_ext().execute_with(|| {
			// The recipient is not a para this registry knows.
			let channel = chan(PARA_A, PARA_UNKNOWN);

			// Returning `Err` here would roll the report back with everything else, and the
			// parachain would sit on a held deposit waiting for news that never comes.
			assert_ok!(Hrmp::init_open_channel(RuntimeOrigin::root(), init_msg(channel)));

			assert!(Requests::get().is_empty());
			assert_eq!(
				take_sent(),
				vec![open_report(channel, Err(FailureReason::InvalidPara))]
			);
			assert_eq!(
				hrmp_events(),
				vec![Event::OpenRejected {
					channel,
					message_id: MSG_ID,
					reason: FailureReason::InvalidPara,
				}]
			);
		});
	}

	#[test]
	fn a_report_that_cannot_be_sent_does_not_unwind_the_registry() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			SendFails::set(true);

			assert_ok!(Hrmp::init_open_channel(RuntimeOrigin::root(), init_msg(channel)));

			// This chain's own state is already correct; the parachain is the one now out of
			// step, and unwinding here would be strictly worse.
			assert_eq!(Requests::get(), vec![(channel, false)]);
			assert_eq!(
				hrmp_events(),
				vec![
					Event::ReportFailed { channel, message_id: MSG_ID },
					Event::OpenRequested { channel, message_id: MSG_ID },
				]
			);
		});
	}
}

mod accept_open_channel {
	use super::*;

	#[test]
	fn confirms_the_request_and_reports_success() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			assert_ok!(Hrmp::init_open_channel(RuntimeOrigin::root(), init_msg(channel)));
			let _ = take_sent();
			let _ = hrmp_events();

			assert_ok!(Hrmp::accept_open_channel(RuntimeOrigin::root(), accept_msg(channel)));

			assert_eq!(Requests::get(), vec![(channel, true)]);
			assert_eq!(OpenChannels::get(), vec![channel]);
			assert_eq!(take_sent(), vec![accept_report(channel, Ok(()))]);
			assert_eq!(
				hrmp_events(),
				vec![Event::OpenAccepted { channel, message_id: MSG_ID }]
			);
		});
	}

	#[test]
	fn accepting_a_request_that_does_not_exist_is_reported_as_not_found() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);

			assert_ok!(Hrmp::accept_open_channel(RuntimeOrigin::root(), accept_msg(channel)));

			assert_eq!(
				take_sent(),
				vec![accept_report(channel, Err(FailureReason::NotFound))]
			);
		});
	}
}

mod close_channel {
	use super::*;

	#[test]
	fn closes_an_open_channel_and_reports_success() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			opened(channel);

			assert_ok!(Hrmp::close_channel(RuntimeOrigin::root(), close_msg(channel, PARA_B)));

			assert!(OpenChannels::get().is_empty());
			assert_eq!(take_sent(), vec![close_report(channel, Ok(()))]);
			assert_eq!(hrmp_events(), vec![Event::Closed { channel, message_id: MSG_ID }]);
		});
	}

	#[test]
	fn a_closer_that_is_not_a_participant_is_refused() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			opened(channel);

			assert_ok!(Hrmp::close_channel(
				RuntimeOrigin::root(),
				close_msg(channel, PARA_UNKNOWN)
			));

			// The channel survives, and the parachain is told to put it back.
			assert_eq!(OpenChannels::get(), vec![channel]);
			assert_eq!(
				take_sent(),
				vec![close_report(channel, Err(FailureReason::InvalidPara))]
			);
		});
	}

	#[test]
	fn a_registry_that_writes_then_fails_leaves_nothing_behind() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			opened(channel);
			NextFailure::set(Some(FailureReason::Refused));

			assert_ok!(Hrmp::close_channel(RuntimeOrigin::root(), close_msg(channel, PARA_A)));

			// A refusal is reported as "nothing happened", so a partial write must not survive
			// it — otherwise the two chains disagree with no way to notice.
			assert!(!frame_support::storage::unhashed::exists(PARTIAL_WRITE_KEY));
			assert_eq!(OpenChannels::get(), vec![channel]);
			assert_eq!(
				take_sent(),
				vec![close_report(channel, Err(FailureReason::Refused))]
			);
		});
	}
}

mod cancel_open_request {
	use super::*;

	#[test]
	fn drops_an_unconfirmed_request() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			assert_ok!(Hrmp::init_open_channel(RuntimeOrigin::root(), init_msg(channel)));
			let _ = take_sent();
			let _ = hrmp_events();

			assert_ok!(Hrmp::cancel_open_request(RuntimeOrigin::root(), cancel_msg(channel)));

			assert!(Requests::get().is_empty());
			assert_eq!(take_sent(), vec![cancel_report(channel, Ok(()))]);
			assert_eq!(hrmp_events(), vec![Event::Cancelled { channel, message_id: MSG_ID }]);
		});
	}

	#[test]
	fn a_request_already_confirmed_cannot_be_cancelled() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			opened(channel);

			assert_ok!(Hrmp::cancel_open_request(RuntimeOrigin::root(), cancel_msg(channel)));

			// The parachain must keep holding: the channel is real now.
			assert_eq!(
				take_sent(),
				vec![cancel_report(channel, Err(FailureReason::AlreadyExists))]
			);
		});
	}
}

mod establish_system_channel {
	use super::*;

	fn system_msg(channel: ChannelId) -> MessageToRelay {
		MessageToRelay::V1(MessageToRelayV1::EstablishSystemChannel {
			channel,
			message_id: MSG_ID,
		})
	}

	#[test]
	fn opens_both_directions_and_reports_only_locally() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);

			assert_ok!(Hrmp::establish_system_channel(
				RuntimeOrigin::root(),
				system_msg(channel)
			));

			assert_eq!(
				OpenChannels::get(),
				vec![channel, chan(PARA_B, PARA_A)]
			);
			// Nothing goes back: no deposit anywhere depends on the outcome.
			assert_eq!(take_sent(), vec![]);
			assert_eq!(
				hrmp_events(),
				vec![Event::SystemChannelOpened { channel, message_id: MSG_ID }]
			);
		});
	}

	#[test]
	fn a_refusal_is_a_local_event_too() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			NextFailure::set(Some(FailureReason::Refused));

			assert_ok!(Hrmp::establish_system_channel(
				RuntimeOrigin::root(),
				system_msg(channel)
			));

			assert!(!frame_support::storage::unhashed::exists(PARTIAL_WRITE_KEY));
			assert_eq!(take_sent(), vec![]);
			assert_eq!(
				hrmp_events(),
				vec![Event::SystemChannelRejected {
					channel,
					message_id: MSG_ID,
					reason: FailureReason::Refused,
				}]
			);
		});
	}
}

mod contract {
	use super::*;

	/// Every call, the message it serves, and the report it must produce.
	///
	/// The pallet's whole job is "drive the registry, then say what happened", so the thing worth
	/// pinning is that no path can ever skip the saying.
	#[allow(clippy::type_complexity)]
	fn calls() -> Vec<(&'static str, Box<dyn Fn() -> DispatchResult>, MessageToPara)> {
		let channel = chan(PARA_A, PARA_B);
		vec![
			(
				"init_open_channel",
				Box::new(move || {
					Hrmp::init_open_channel(RuntimeOrigin::root(), init_msg(channel))
				}),
				open_report(channel, Ok(())),
			),
			(
				"accept_open_channel",
				Box::new(move || {
					Hrmp::init_open_channel(RuntimeOrigin::root(), init_msg(channel)).unwrap();
					let _ = take_sent();
					Hrmp::accept_open_channel(RuntimeOrigin::root(), accept_msg(channel))
				}),
				accept_report(channel, Ok(())),
			),
			(
				"close_channel",
				Box::new(move || {
					opened(channel);
					Hrmp::close_channel(RuntimeOrigin::root(), close_msg(channel, PARA_A))
				}),
				close_report(channel, Ok(())),
			),
			(
				"cancel_open_request",
				Box::new(move || {
					Hrmp::init_open_channel(RuntimeOrigin::root(), init_msg(channel)).unwrap();
					let _ = take_sent();
					Hrmp::cancel_open_request(RuntimeOrigin::root(), cancel_msg(channel))
				}),
				cancel_report(channel, Ok(())),
			),
		]
	}

	#[test]
	fn every_call_reports_exactly_once_on_success() {
		for (name, run, expected) in calls() {
			new_test_ext().execute_with(|| {
				assert_ok!(run());
				assert_eq!(take_sent(), vec![expected.clone()], "{name} did not report");
			});
		}
	}

	#[test]
	fn a_report_that_cannot_be_sent_never_unwinds_the_registry() {
		for (name, run, _) in calls() {
			new_test_ext().execute_with(|| {
				// The setup steps inside `run` send reports of their own, so the transport is
				// broken for all of them; only the last call's report is the one under test.
				SendFails::set(true);

				// The call still succeeds: this chain's state is already correct, and unwinding
				// it because the report could not go out would be strictly worse.
				assert!(run().is_ok(), "{name} failed when the transport was down");

				let events = hrmp_events();
				assert!(
					events.iter().any(|e| matches!(e, Event::ReportFailed { .. })),
					"{name} did not raise ReportFailed"
				);
			});
		}
	}

	#[test]
	fn every_call_refuses_a_message_it_does_not_serve() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			let wrong = [
				("init", Hrmp::init_open_channel(RuntimeOrigin::root(), accept_msg(channel))),
				("accept", Hrmp::accept_open_channel(RuntimeOrigin::root(), init_msg(channel))),
				("close", Hrmp::close_channel(RuntimeOrigin::root(), cancel_msg(channel))),
				(
					"cancel",
					Hrmp::cancel_open_request(
						RuntimeOrigin::root(),
						close_msg(channel, PARA_A),
					),
				),
				(
					"system",
					Hrmp::establish_system_channel(RuntimeOrigin::root(), init_msg(channel)),
				),
			];
			for (name, result) in wrong {
				assert_eq!(
					result,
					Err(Error::<Test>::UnexpectedMessage.into()),
					"{name} accepted a message it does not serve"
				);
			}
			// A refused message is an extrinsic failure, not a report: nothing was decided, so
			// there is nothing to tell the parachain.
			assert_eq!(take_sent(), vec![]);
		});
	}

	#[test]
	fn the_registry_is_the_only_source_of_truth_about_what_exists() {
		new_test_ext().execute_with(|| {
			use hrmp_primitives::HrmpRegistry;
			let channel = chan(PARA_A, PARA_B);

			assert!(!MockRegistry::exists(channel));

			assert_ok!(Hrmp::init_open_channel(RuntimeOrigin::root(), init_msg(channel)));
			assert!(MockRegistry::exists(channel), "a pending request counts as existing");

			assert_ok!(Hrmp::accept_open_channel(RuntimeOrigin::root(), accept_msg(channel)));
			assert!(MockRegistry::exists(channel));

			assert_ok!(Hrmp::close_channel(RuntimeOrigin::root(), close_msg(channel, PARA_A)));
			assert!(!MockRegistry::exists(channel));
		});
	}

	#[test]
	fn this_pallet_keeps_no_storage_of_its_own() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);
			opened(channel);

			// Deliberate: the relay chain half is a translator, not a second registry. If it ever
			// grows storage, the two chains gain a third thing that can disagree.
			assert!(
				sp_io::storage::next_key(&[]).is_none_or(|k| !k.starts_with(
					&sp_io::hashing::twox_128(b"Hrmp")[..]
				)),
				"pallet-hrmp-relay has grown storage of its own"
			);
		});
	}
}

mod parameters {
	use super::*;

	#[test]
	fn the_registry_gets_the_bounds_the_parachain_asked_for() {
		new_test_ext().execute_with(|| {
			let channel = chan(PARA_A, PARA_B);

			// Zero capacity is refused by the registry, not by this pallet: the live bounds are
			// the relay chain's business, and mirroring them here would let the two drift.
			assert_ok!(Hrmp::init_open_channel(
				RuntimeOrigin::root(),
				MessageToRelay::V1(MessageToRelayV1::InitOpenChannel {
					channel,
					message_id: MSG_ID,
					max_capacity: 0,
					max_message_size: MESSAGE_SIZE,
				})
			));

			assert!(Requests::get().is_empty());
			assert_eq!(
				take_sent(),
				vec![open_report(channel, Err(FailureReason::InvalidParameters))]
			);
		});
	}
}
