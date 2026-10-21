// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Tests with the deposits held on a remote chain, which answers each request later, in any
//! order, and possibly long after the request.

use super::{
	tests::{run_to_block, GenesisConfigBuilder},
	*,
};
use crate::mock::{
	deregister_parachain, new_test_ext, register_parachain_with_balance, ChannelSide, Dmp, Hrmp,
	MockDepositProvider as Remote, Paras, RuntimeEvent as MockEvent, RuntimeOrigin, System, Test,
	TestUsesOnlyStoredVersionWrapper,
};
use frame_support::{assert_noop, assert_ok};

const A: ParaId = ParaId::new(2001);
const B: ParaId = ParaId::new(2002);
const C: ParaId = ParaId::new(2003);
const D: ParaId = ParaId::new(2004);
const SYSTEM_1: ParaId = ParaId::new(1);
const SYSTEM_2: ParaId = ParaId::new(2);

/// What each para holds on the remote chain at first.
const REMOTE_FREE: Balance = 1000;
/// The deposit of each side of a channel, as in the default genesis.
const DEPOSIT: Balance = 100;

const AWAITS_ANSWER: DispatchError =
	DispatchError::Other("a request for this side awaits its answer");

/// Runs `test` with the deposits held on the remote chain, then checks that the storage is
/// consistent and that nothing is held on this chain.
fn new_remote_test_ext(test: impl FnOnce()) {
	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		for para in [SYSTEM_1, SYSTEM_2, A, B, C, D] {
			register_parachain_with_balance(para, 0);
			Remote::set_remote_free(para, REMOTE_FREE);
		}
		Remote::use_remote_chain();
		run_to_block(5, Some(vec![4, 5]));

		test();

		Hrmp::assert_storage_consistency_exhaustive();
		for para in [SYSTEM_1, SYSTEM_2, A, B, C, D] {
			let account = para.into_account_truncating();
			assert_eq!(<Test as Config>::Currency::reserved_balance(&account), 0);
		}
	});
}

fn origin(para: ParaId) -> RuntimeOrigin {
	crate::Origin::Parachain(para).into()
}

fn channel(sender: ParaId, recipient: ParaId) -> HrmpChannelId {
	HrmpChannelId { sender, recipient }
}

fn held(sender: ParaId, recipient: ParaId, side: ChannelSide) -> Balance {
	Remote::remote_held(&channel(sender, recipient), side)
}

fn hrmp_events() -> Vec<Event<Test>> {
	System::events()
		.into_iter()
		.filter_map(|r| match r.event {
			MockEvent::Hrmp(e) => Some(e),
			_ => None,
		})
		.collect()
}

fn has_event(event: Event<Test>) -> bool {
	hrmp_events().contains(&event)
}

fn next_session() {
	let b = System::block_number() + 1;
	run_to_block(b, Some(vec![b]));
	Hrmp::assert_storage_consistency_exhaustive();
}

/// Offboards `para`. Its channels are only cleaned up at the second session, so the storage is
/// not checked at the first.
fn offboard(para: ParaId) {
	deregister_parachain(para);
	let b = System::block_number();
	run_to_block(b + 2, Some(vec![b + 1, b + 2]));
	Hrmp::assert_storage_consistency_exhaustive();
	assert!(!Paras::is_valid_para(para));
}

/// The remote chain processes the requests of the channel, and their answers are delivered,
/// until none is left. The other channels' requests and answers are left waiting.
fn settle_channel(sender: ParaId, recipient: ParaId) {
	let id = channel(sender, recipient);
	loop {
		if let Some(i) = Remote::requests().iter().position(|r| r.channel_id == id) {
			Remote::process_request(i);
		} else if let Some(i) = Remote::answers().iter().position(|(r, _)| r.channel_id == id) {
			Remote::deliver_answer(i);
		} else {
			break;
		}
	}
}

fn deliver_answer_of(sender: ParaId, recipient: ParaId) {
	let id = channel(sender, recipient);
	let i = Remote::answers()
		.iter()
		.position(|(r, _)| r.channel_id == id)
		.expect("an answer");
	Remote::deliver_answer(i);
}

/// Requests and accepts a channel, with the remote chain answering right away. The channel is
/// opened at the next session.
fn request_and_accept(sender: ParaId, recipient: ParaId) {
	assert_ok!(Hrmp::hrmp_init_open_channel(origin(sender), recipient, 2, 8));
	settle_channel(sender, recipient);
	assert_ok!(Hrmp::hrmp_accept_open_channel(origin(recipient), sender));
	settle_channel(sender, recipient);
}

fn set_deposits(sender_deposit: Balance, recipient_deposit: Balance) {
	configuration::ActiveConfig::<Test>::mutate(|c| {
		c.hrmp_sender_deposit = sender_deposit;
		c.hrmp_recipient_deposit = recipient_deposit;
	});
}

#[test]
fn init_and_accept_wait_for_the_remote_chain() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		System::reset_events();

		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		// Nothing is recorded until the remote chain answers.
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());
		assert!(hrmp_events().is_empty());
		assert_eq!(Remote::requests().len(), 1);

		Remote::process_request(0);
		assert_eq!(held(A, B, ChannelSide::Sender), DEPOSIT);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE - DEPOSIT);
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());

		Remote::deliver_answer(0);
		assert_eq!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().sender_deposit, DEPOSIT);
		assert!(has_event(Event::OpenChannelRequested {
			sender: A,
			recipient: B,
			proposed_max_capacity: 2,
			proposed_max_message_size: 8,
		}));

		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		assert!(!HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
		assert!(!has_event(Event::OpenChannelAccepted { sender: A, recipient: B }));

		Remote::settle();
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
		assert!(has_event(Event::OpenChannelAccepted { sender: A, recipient: B }));
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);

		next_session();
		let channel = HrmpChannels::<Test>::get(&id).unwrap();
		assert_eq!((channel.sender_deposit, channel.recipient_deposit), (DEPOSIT, DEPOSIT));
	});
}

#[test]
fn refused_deposit_records_nothing() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		Remote::set_remote_free(A, DEPOSIT - 1);

		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		Remote::settle();
		assert!(has_event(Event::DepositRefused {
			channel_id: id.clone(),
			error: DepositError::CannotHold,
		}));
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(Remote::remote_free(A), DEPOSIT - 1);

		// Once refused, the sender may ask again.
		Remote::set_remote_free(A, DEPOSIT);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		Remote::settle();
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_some());

		System::reset_events();
		Remote::set_remote_free(B, DEPOSIT - 1);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		Remote::settle();
		assert!(has_event(Event::DepositRefused {
			channel_id: id.clone(),
			error: DepositError::CannotHold,
		}));
		assert!(!HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);

		// Once refused, the recipient may accept again.
		Remote::set_remote_free(B, DEPOSIT);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		Remote::settle();
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
	});
}

#[test]
fn side_awaiting_its_answer_refuses_other_requests() {
	new_remote_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		assert_noop!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8), AWAITS_ANSWER);
		assert_noop!(
			Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8),
			AWAITS_ANSWER
		);

		// Processed, but not answered yet.
		Remote::process_requests();
		assert_noop!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8), AWAITS_ANSWER);

		// Other channels are not held back, including the other way round.
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), C, 2, 8));
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(B), A, 2, 8));

		Remote::settle();
		assert_noop!(
			Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8),
			Error::<Test>::OpenHrmpChannelAlreadyRequested
		);
	});
}

#[test]
fn force_open_takes_both_deposits_at_once() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		let opened = Event::HrmpChannelForceOpened {
			sender: A,
			recipient: B,
			proposed_max_capacity: 2,
			proposed_max_message_size: 8,
		};

		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8));
		let deposit = Some(DepositChange { current: 0, target: DEPOSIT });
		assert_eq!(
			Remote::requests(),
			vec![DepositChangeRequest {
				channel_id: id.clone(),
				sender: deposit,
				recipient: deposit,
				action: DepositAction::InitAndAcceptOpenChannel {
					max_capacity: 2,
					max_message_size: 8,
					max_total_size: 16,
				},
			}]
		);
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());
		assert!(!has_event(opened.clone()));

		Remote::settle();
		assert!(has_event(opened));
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
		next_session();
		assert!(HrmpChannels::<Test>::get(&id).is_some());

		// Opened whole or not at all: a recipient short of funds refuses both deposits.
		let id = channel(C, D);
		Remote::set_remote_free(D, DEPOSIT - 1);
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), C, D, 2, 8));
		Remote::settle();
		assert!(has_event(Event::DepositRefused {
			channel_id: id.clone(),
			error: DepositError::CannotHold,
		}));
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());
		assert_eq!(held(C, D, ChannelSide::Sender), 0);
		assert_eq!(Remote::remote_free(C), REMOTE_FREE);
	});
}

#[test]
fn poke_waits_for_the_remote_chain() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		request_and_accept(A, B);
		next_session();

		set_deposits(150, 120);
		System::reset_events();
		let updated = Event::OpenChannelDepositsUpdated { sender: A, recipient: B };
		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), A, B));
		let channel = HrmpChannels::<Test>::get(&id).unwrap();
		assert_eq!((channel.sender_deposit, channel.recipient_deposit), (DEPOSIT, DEPOSIT));
		assert!(!has_event(updated.clone()));

		Remote::settle();
		let channel = HrmpChannels::<Test>::get(&id).unwrap();
		assert_eq!((channel.sender_deposit, channel.recipient_deposit), (150, 120));
		assert!(has_event(updated));
		assert_eq!(held(A, B, ChannelSide::Sender), 150);
		assert_eq!(held(A, B, ChannelSide::Recipient), 120);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE - 150);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE - 120);

		// A refused poke leaves the channel as it is.
		set_deposits(REMOTE_FREE + 1, 120);
		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), A, B));
		Remote::settle();
		assert!(has_event(Event::DepositRefused {
			channel_id: id.clone(),
			error: DepositError::CannotHold,
		}));
		let channel = HrmpChannels::<Test>::get(&id).unwrap();
		assert_eq!((channel.sender_deposit, channel.recipient_deposit), (150, 120));
		assert_eq!(held(A, B, ChannelSide::Sender), 150);
	});
}

#[test]
fn answers_delivered_out_of_order() {
	new_remote_test_ext(|| {
		// The outbound limit is 2. Nothing is recorded before the answers, so all three pass.
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), C, 2, 8));
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), D, 2, 8));
		Remote::process_requests();
		assert_eq!(Remote::remote_free(A), REMOTE_FREE - 3 * DEPOSIT);

		// The answers come back in the reverse order: the first request is the one too many.
		deliver_answer_of(A, D);
		deliver_answer_of(A, C);
		deliver_answer_of(A, B);
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel(A, D)).is_some());
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel(A, C)).is_some());
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel(A, B)).is_none());
		assert!(has_event(Event::DepositReturned {
			channel_id: channel(A, B),
			error: Error::<Test>::OpenHrmpChannelLimitExceeded.into(),
		}));

		// Giving back the deposit goes through the remote chain too.
		assert_eq!(held(A, B, ChannelSide::Sender), DEPOSIT);
		Remote::settle();
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE - 2 * DEPOSIT);
	});
}

#[test]
fn init_answered_after_the_recipient_offboarded() {
	new_remote_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		offboard(B);

		Remote::process_requests();
		Remote::deliver_answers();
		assert!(has_event(Event::DepositReturned {
			channel_id: channel(A, B),
			error: Error::<Test>::OpenHrmpChannelInvalidRecipient.into(),
		}));
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel(A, B)).is_none());

		Remote::settle();
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
	});
}

#[test]
fn init_answered_after_the_sender_offboarded() {
	new_remote_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		offboard(A);

		// The request of a para gone is not recorded, and its deposit is given back.
		Remote::settle();
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel(A, B)).is_none());
		assert!(has_event(Event::DepositReturned {
			channel_id: channel(A, B),
			error: Error::<Test>::OpenHrmpChannelInvalidSender.into(),
		}));
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
	});
}

#[test]
fn force_open_answered_after_the_sender_offboarded() {
	new_remote_test_ext(|| {
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8));
		offboard(A);

		// The request of a para gone is not recorded, and both deposits are given back.
		Remote::settle();
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel(A, B)).is_none());
		assert!(has_event(Event::DepositReturned {
			channel_id: channel(A, B),
			error: Error::<Test>::OpenHrmpChannelInvalidSender.into(),
		}));
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert!(!hrmp_events().iter().any(|e| matches!(e, Event::HrmpChannelForceOpened { .. })));
	});
}

#[test]
fn accept_answered_after_the_sender_canceled() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);

		// The remote chain takes the recipient's deposit, but its answer is late.
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		Remote::process_requests();
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);
		for _ in 0..3 {
			next_session();
		}
		assert!(!HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);

		// Meanwhile the sender cancels and gets its deposit back.
		assert_ok!(Hrmp::hrmp_cancel_open_request(origin(A), id.clone(), 1));
		Remote::process_requests();
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
		// The recipient's deposit stays held, with no record of it here, until the answer.
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);

		Remote::deliver_answers();
		assert!(has_event(Event::DepositReturned {
			channel_id: id.clone(),
			error: Error::<Test>::AcceptHrmpChannelDoesntExist.into(),
		}));
		Remote::settle();
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE);
	});
}

#[test]
fn accept_answered_after_the_sender_offboarded() {
	new_remote_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));

		// The offboarding drops the request and gives back the sender's deposit.
		offboard(A);
		Remote::settle();
		assert!(has_event(Event::DepositReturned {
			channel_id: channel(A, B),
			error: Error::<Test>::AcceptHrmpChannelDoesntExist.into(),
		}));
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE);
	});
}

#[test]
fn force_open_answered_after_the_inbound_limit_filled() {
	new_remote_test_ext(|| {
		// The inbound limit is 2.
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8));
		request_and_accept(C, B);
		request_and_accept(D, B);

		Remote::settle();
		assert!(has_event(Event::DepositReturned {
			channel_id: channel(A, B),
			error: Error::<Test>::AcceptHrmpChannelLimitExceeded.into(),
		}));
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel(A, B)).is_none());
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE - 2 * DEPOSIT);
	});
}

#[test]
fn poke_answered_after_the_channel_closed() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		request_and_accept(A, B);
		next_session();

		set_deposits(150, 150);
		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), A, B));
		assert_ok!(Hrmp::hrmp_close_channel(origin(A), id.clone()));
		for _ in 0..3 {
			next_session();
		}
		assert!(HrmpChannels::<Test>::get(&id).is_none());

		// The remote chain did not process anything yet. The release of the close waits behind
		// the poke.
		assert_eq!(Remote::requests().len(), 2);
		assert_eq!(held(A, B, ChannelSide::Sender), DEPOSIT);
		Remote::process_request(0);
		assert_eq!(held(A, B, ChannelSide::Sender), 150);
		Remote::process_request(0);
		assert_eq!(held(A, B, ChannelSide::Sender), 0);

		// The late poke is refused and gives back what it took, which the close already did.
		System::reset_events();
		Remote::settle();
		assert!(has_event(Event::DepositReturned {
			channel_id: id.clone(),
			error: Error::<Test>::OpenHrmpChannelDoesntExist.into(),
		}));
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE);
	});
}

/// The channel was opened free of deposits, and a poke takes the deposits of a new price while
/// the channel is closed. The close gives back nothing, as the channel recorded nothing, so the
/// late poke gives back what it took.
#[test]
fn poke_answered_after_the_free_channel_closed() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		set_deposits(0, 0);
		request_and_accept(A, B);
		next_session();
		let channel = HrmpChannels::<Test>::get(&id).unwrap();
		assert_eq!((channel.sender_deposit, channel.recipient_deposit), (0, 0));

		set_deposits(DEPOSIT, DEPOSIT);
		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), A, B));
		assert_ok!(Hrmp::hrmp_close_channel(origin(A), id.clone()));
		next_session();
		assert!(HrmpChannels::<Test>::get(&id).is_none());
		// The close had nothing recorded to give back: only the poke was sent.
		assert_eq!(Remote::requests().len(), 1);

		Remote::process_requests();
		assert_eq!(held(A, B, ChannelSide::Sender), DEPOSIT);
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);

		// The late poke is refused and gives back what it took.
		System::reset_events();
		Remote::settle();
		assert!(has_event(Event::DepositReturned {
			channel_id: id.clone(),
			error: Error::<Test>::OpenHrmpChannelDoesntExist.into(),
		}));
		assert!(HrmpChannels::<Test>::get(&id).is_none());
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE);
	});
}

#[test]
fn unanswered_init_holds_back_only_its_side() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		for _ in 0..3 {
			next_session();
		}
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());
		assert_eq!(HrmpOpenChannelRequestCount::<Test>::get(A), 0);

		// The other channels of both paras open meanwhile.
		request_and_accept(A, C);
		request_and_accept(B, A);
		next_session();
		assert!(HrmpChannels::<Test>::get(&channel(A, C)).is_some());
		assert!(HrmpChannels::<Test>::get(&channel(B, A)).is_some());
		assert_noop!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8), AWAITS_ANSWER);

		// The answer comes eventually.
		Remote::settle();
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_some());
		assert_eq!(held(A, B, ChannelSide::Sender), DEPOSIT);
	});
}

#[test]
fn para_offboarded_with_unanswered_requests() {
	new_remote_test_ext(|| {
		request_and_accept(A, B);
		next_session();

		// A poke of a channel of A, and a request to A, are not answered before A is gone.
		set_deposits(150, 150);
		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), A, B));
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(C), A, 2, 8));
		offboard(A);
		assert!(HrmpChannels::<Test>::get(&channel(A, B)).is_none());

		Remote::settle();
		assert!(has_event(Event::DepositReturned {
			channel_id: channel(C, A),
			error: Error::<Test>::OpenHrmpChannelInvalidRecipient.into(),
		}));
		for para in [A, B, C] {
			assert_eq!(Remote::remote_free(para), REMOTE_FREE);
		}
	});
}

#[test]
fn system_channels_need_no_remote_answer() {
	new_remote_test_ext(|| {
		assert_ok!(Hrmp::establish_system_channel(RuntimeOrigin::signed(1), SYSTEM_1, SYSTEM_2));
		assert_ok!(Hrmp::establish_channel_with_system(origin(A), SYSTEM_1));
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), SYSTEM_2, B, 2, 8));
		assert!(Remote::requests().is_empty());

		next_session();
		for (sender, recipient) in
			[(SYSTEM_1, SYSTEM_2), (A, SYSTEM_1), (SYSTEM_1, A), (SYSTEM_2, B)]
		{
			assert!(HrmpChannels::<Test>::get(&channel(sender, recipient)).is_some());
		}
	});
}

#[test]
fn notifications_sent_once_answered() {
	use xcm::latest::{Junction, Location};

	new_remote_test_ext(|| {
		for para in [A, B] {
			TestUsesOnlyStoredVersionWrapper::set_version(
				Location::new(0, [Junction::Parachain(para.into())]),
				None,
			);
		}
		let notifications = |para| Dmp::dmq_contents_do_not_call_in_consensus(para).len();

		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		assert_eq!(notifications(B), 0);
		Remote::settle();
		assert_eq!(notifications(B), 1);

		// A refused accept tells the sender nothing.
		Remote::set_remote_free(B, DEPOSIT - 1);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		Remote::settle();
		assert_eq!(notifications(A), 0);

		Remote::set_remote_free(B, DEPOSIT);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		assert_eq!(notifications(A), 0);
		Remote::settle();
		assert_eq!(notifications(A), 1);
	});
}

#[test]
fn answer_after_the_config_limits_lowered() {
	new_remote_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		configuration::ActiveConfig::<Test>::mutate(|c| c.hrmp_channel_max_capacity = 1);

		Remote::settle();
		assert!(has_event(Event::DepositReturned {
			channel_id: channel(A, B),
			error: Error::<Test>::OpenHrmpChannelCapacityExceedsLimit.into(),
		}));
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel(A, B)).is_none());
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
	});
}

#[test]
fn accept_answered_after_the_recipient_offboarded() {
	new_remote_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));

		offboard(B);
		Remote::settle();
		assert!(has_event(Event::DepositReturned {
			channel_id: channel(A, B),
			error: Error::<Test>::AcceptHrmpChannelDoesntExist.into(),
		}));
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE);
	});
}

#[test]
fn force_open_over_an_existing_request() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);

		// The request is canceled and its deposit given back, before the sender's deposit is
		// taken again.
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8));
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());
		assert_eq!(Remote::requests().len(), 2);

		Remote::settle();
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
		assert_eq!(held(A, B, ChannelSide::Sender), DEPOSIT);
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE - DEPOSIT);
	});
}

#[test]
fn force_open_refused_while_the_recipient_side_awaits() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));

		// Dispatched calls run in a storage layer: the cancel and its release are dropped too.
		assert_noop!(
			frame_support::storage::with_storage_layer(|| {
				Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8)
			}),
			AWAITS_ANSWER
		);
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_some());
		assert_eq!(Remote::requests().len(), 1);

		Remote::settle();
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
	});
}

#[test]
fn zero_deposit_request_does_not_wait_for_a_pending_side() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));

		// Without deposits the provider is not asked, so its pending side does not hold this back.
		set_deposits(0, 0);
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8));
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);

		// The pending request comes back too late, and its deposit is given back.
		Remote::settle();
		assert!(has_event(Event::DepositReturned {
			channel_id: id.clone(),
			error: Error::<Test>::OpenHrmpChannelAlreadyRequested.into(),
		}));
		assert_eq!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().sender_deposit, 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
	});
}

#[test]
fn accept_answered_after_the_deposit_changed() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));

		// The answer comes in a later session, after the price changed.
		set_deposits(DEPOSIT, 150);
		next_session();
		Remote::settle();

		// Taken at the old price, the deposit is given back and the request stays unconfirmed.
		assert!(has_event(Event::DepositReturned {
			channel_id: id.clone(),
			error: Error::<Test>::AcceptHrmpChannelDepositChanged.into(),
		}));
		assert!(!HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE);

		// Accepted again at the new price, the channel records what the remote chain holds.
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		Remote::settle();
		next_session();
		let channel = HrmpChannels::<Test>::get(&id).unwrap();
		assert_eq!(channel.recipient_deposit, 150);
		assert_eq!(held(A, B, ChannelSide::Recipient), 150);
	});
}

#[test]
fn force_open_answered_after_the_deposit_changed() {
	new_remote_test_ext(|| {
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8));

		// The answer comes in a later session, after the price changed.
		set_deposits(DEPOSIT, 150);
		next_session();
		Remote::settle();

		// Both deposits are given back and nothing is recorded.
		assert!(has_event(Event::DepositReturned {
			channel_id: channel(A, B),
			error: Error::<Test>::AcceptHrmpChannelDepositChanged.into(),
		}));
		assert!(HrmpOpenChannelRequests::<Test>::get(&channel(A, B)).is_none());
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE);
	});
}

#[test]
fn refused_force_open_over_an_existing_request() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);

		// The existing request is dropped, and the recipient cannot cover its deposit.
		Remote::set_remote_free(B, DEPOSIT - 1);
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8));
		Remote::settle();
		assert!(has_event(Event::DepositRefused {
			channel_id: id.clone(),
			error: DepositError::CannotHold,
		}));

		// Nothing is recorded and nothing is held.
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());
		assert_eq!(HrmpOpenChannelRequestCount::<Test>::get(A), 0);
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
		assert_eq!(Remote::remote_free(B), DEPOSIT - 1);

		// The sender may ask again.
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_some());
	});
}

/// The sender cancels its request and asks again with other limits while the recipient's accept
/// awaits its answer. The late accept is for the first request: it does not confirm the new one.
#[test]
fn accept_answered_after_the_request_made_anew() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		Remote::process_requests();
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);

		// The new request is answered before the accept.
		assert_ok!(Hrmp::hrmp_cancel_open_request(origin(A), id.clone(), 1));
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 1, 4));
		Remote::process_requests();
		assert_eq!(Remote::answers().len(), 2);
		Remote::deliver_answer(1);
		let request = HrmpOpenChannelRequests::<Test>::get(&id).unwrap();
		assert_eq!((request.max_capacity, request.max_message_size), (1, 4));
		assert!(!request.confirmed);

		// The accept is refused and its deposit is given back.
		Remote::deliver_answer(0);
		assert!(has_event(Event::DepositReturned {
			channel_id: id.clone(),
			error: Error::<Test>::AcceptHrmpChannelRequestChanged.into(),
		}));
		assert!(!HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
		Remote::settle();
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE);

		// The recipient accepts the new request.
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		Remote::settle();
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
		next_session();
		let channel = HrmpChannels::<Test>::get(&id).unwrap();
		assert_eq!((channel.max_capacity, channel.max_message_size), (1, 4));
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);
	});
}

/// Asked again with the same limits, the new request is what the recipient accepted: the late
/// accept confirms it.
#[test]
fn accept_answered_after_the_request_made_anew_alike() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		Remote::process_requests();

		assert_ok!(Hrmp::hrmp_cancel_open_request(origin(A), id.clone(), 1));
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		Remote::process_requests();
		Remote::deliver_answer(1);
		Remote::deliver_answer(0);
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);

		next_session();
		assert!(HrmpChannels::<Test>::get(&id).is_some());
		assert_eq!(held(A, B, ChannelSide::Sender), DEPOSIT);
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);
	});
}

/// The channel is closed and opened anew, free of deposits, while a poke awaits its answer. The
/// late poke does not record deposits the new channel does not hold.
#[test]
fn poke_answered_after_the_channel_opened_anew() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		request_and_accept(A, B);
		next_session();

		set_deposits(150, 150);
		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), A, B));
		assert_ok!(Hrmp::hrmp_close_channel(origin(A), id.clone()));
		next_session();
		assert!(HrmpChannels::<Test>::get(&id).is_none());

		// Without deposits the provider is not asked, so the pending poke does not hold this back.
		set_deposits(0, 0);
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8));
		next_session();
		let channel = HrmpChannels::<Test>::get(&id).unwrap();
		assert_eq!((channel.sender_deposit, channel.recipient_deposit), (0, 0));

		// The remote chain took the poke's deposits, then gave everything back for the close.
		Remote::process_requests();
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);

		// The late poke is refused, and the new channel keeps its records.
		Remote::settle();
		assert!(has_event(Event::DepositReturned {
			channel_id: id.clone(),
			error: Error::<Test>::ChannelDepositsChanged.into(),
		}));
		let channel = HrmpChannels::<Test>::get(&id).unwrap();
		assert_eq!((channel.sender_deposit, channel.recipient_deposit), (0, 0));
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE);
	});
}

/// Asked again with the same limits, but refused: the sender moved its funds meanwhile. Nothing
/// is recorded, and the late accept gives the recipient's deposit back.
#[test]
fn accept_answered_after_the_request_made_anew_refused() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		Remote::process_requests();
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);

		// The sender gets its deposit back, moves everything away and asks again.
		assert_ok!(Hrmp::hrmp_cancel_open_request(origin(A), id.clone(), 1));
		Remote::process_requests();
		assert_eq!(Remote::remote_free(A), REMOTE_FREE);
		Remote::set_remote_free(A, 0);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		Remote::process_requests();
		assert_eq!(held(A, B, ChannelSide::Sender), 0);

		// The refused request is answered first: nothing is recorded.
		Remote::deliver_answer(1);
		assert!(has_event(Event::DepositRefused {
			channel_id: id.clone(),
			error: DepositError::InsufficientFunds,
		}));
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());

		// The accept finds no request and gives its deposit back.
		Remote::deliver_answer(0);
		assert!(has_event(Event::DepositReturned {
			channel_id: id.clone(),
			error: Error::<Test>::AcceptHrmpChannelDoesntExist.into(),
		}));
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());
		Remote::settle();
		assert_eq!(held(A, B, ChannelSide::Recipient), 0);
		assert_eq!(Remote::remote_free(B), REMOTE_FREE);
		assert_eq!(Remote::remote_free(A), 0);
	});
}

/// The sender is offboarded, onboarded again and asks again with the same limits while the
/// recipient's accept awaits its answer. The late accept confirms the new request: the recipient
/// accepted a channel from that para with those limits.
#[test]
fn accept_answered_after_the_sender_offboarded_and_onboarded() {
	new_remote_test_ext(|| {
		let id = channel(A, B);
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		settle_channel(A, B);
		assert_ok!(Hrmp::hrmp_accept_open_channel(origin(B), A));
		Remote::process_requests();

		// The request is removed and the sender's deposit given back. The recipient's stays held.
		offboard(A);
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).is_none());
		Remote::process_requests();
		assert_eq!(held(A, B, ChannelSide::Sender), 0);
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);

		register_parachain_with_balance(A, 0);
		next_session();
		next_session();
		assert!(Paras::is_valid_para(A));
		assert_ok!(Hrmp::hrmp_init_open_channel(origin(A), B, 2, 8));
		Remote::process_requests();
		Remote::deliver_answer(1);
		assert!(!HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);
		Remote::deliver_answer(0);
		assert!(HrmpOpenChannelRequests::<Test>::get(&id).unwrap().confirmed);

		next_session();
		assert!(HrmpChannels::<Test>::get(&id).is_some());
		assert_eq!(held(A, B, ChannelSide::Sender), DEPOSIT);
		assert_eq!(held(A, B, ChannelSide::Recipient), DEPOSIT);
	});
}
