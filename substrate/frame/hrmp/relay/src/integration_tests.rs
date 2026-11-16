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

//! The relay chain's `hrmp` pallet, this pallet and `pallet-hrmp-para` in one runtime.
//!
//! Messages between the two deposit pallets are queued and delivered on demand, as XCM would
//! deliver them: in a later block than the one that sent them.

use crate::{self as pallet_hrmp_relay, FailedReleases, PendingRequests, SendToPara};
use core::cell::{Cell, RefCell};
use frame_support::{
	assert_noop, assert_ok, derive_impl, parameter_types,
	traits::{fungible::InspectHold, ConstU128, ConstU32, EstimateNextSessionRotation, Hooks},
	weights::{Weight, WeightMeter},
};
use frame_system::EnsureRoot;
use hrmp_primitives::{
	ChannelId, DepositError as WireDepositError, DepositSide, MessageToPara, MessageToRelay,
};
use pallet_hrmp_para::{Deposits, HoldReason, SendToRelay};
use polkadot_primitives::{HrmpChannelId, Id as ParaId};
use polkadot_runtime_parachains::{
	configuration::{self, HostConfiguration},
	dmp,
	hrmp::{self, deposits::DepositError, HrmpChannels, HrmpOpenChannelRequests},
	origin as parachains_origin,
	paras::{self, ParaGenesisArgs, ParaKind},
	shared, Origin,
};
use sp_runtime::{
	traits::{AccountIdConversion, Convert},
	transaction_validity::TransactionPriority,
	BuildStorage, Permill,
};
use std::collections::VecDeque;

type Block = frame_system::mocking::MockBlock<Test>;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type AccountId = u64;
type Balance = u128;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		ParachainsOrigin: parachains_origin,
		Configuration: configuration,
		ParasShared: shared,
		Paras: paras,
		Dmp: dmp,
		Hrmp: hrmp,
		HrmpRelay: pallet_hrmp_relay,
		HrmpPara: pallet_hrmp_para,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type AccountStore = System;
	type RuntimeHoldReason = RuntimeHoldReason;
}

// `paras` submits unsigned transactions.
impl<C> frame_system::offchain::CreateTransactionBase<C> for Test
where
	RuntimeCall: From<C>,
{
	type Extrinsic = UncheckedExtrinsic;
	type RuntimeCall = RuntimeCall;
}

impl<C> frame_system::offchain::CreateBare<C> for Test
where
	RuntimeCall: From<C>,
{
	fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
		UncheckedExtrinsic::new_bare(call)
	}
}

impl parachains_origin::Config for Test {}

impl configuration::Config for Test {
	type WeightInfo = configuration::TestWeightInfo;
}

impl shared::Config for Test {
	type DisabledValidators = ();
}

/// Sessions never rotate: paras are registered at genesis and requests are force-processed.
pub struct NoSessionRotation;
impl EstimateNextSessionRotation<u64> for NoSessionRotation {
	fn average_session_length() -> u64 {
		10
	}

	fn estimate_current_session_progress(_: u64) -> (Option<Permill>, Weight) {
		(None, Weight::zero())
	}

	fn estimate_next_session_rotation(_: u64) -> (Option<u64>, Weight) {
		(None, Weight::zero())
	}
}

parameter_types! {
	pub const ParasUnsignedPriority: TransactionPriority = TransactionPriority::MAX;
	pub const DefaultChannelSizeAndCapacityWithSystem: (u32, u32) = (4, 1);
	pub const HolderParaId: ParaId = ParaId::new(HOLDER);
}

impl paras::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = paras::TestWeightInfo;
	type UnsignedPriority = ParasUnsignedPriority;
	type QueueFootprinter = ();
	type NextSessionRotation = NoSessionRotation;
	type OnNewHead = ();
	type AssignCoretime = ();
	type Fungible = Balances;
	type CooldownRemovalMultiplier = ConstU128<1>;
	type AuthorizeCurrentCodeOrigin = EnsureRoot<AccountId>;
}

impl dmp::Config for Test {
	type WeightInfo = ();
}

impl hrmp::Config for Test {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeEvent = RuntimeEvent;
	type ChannelManager = EnsureRoot<AccountId>;
	type Currency = Balances;
	type DepositProvider = HrmpRelay;
	type DefaultChannelSizeAndCapacityWithSystem = DefaultChannelSizeAndCapacityWithSystem;
	type VersionWrapper = ();
	type WeightInfo = hrmp::TestWeightInfo;
}

thread_local! {
	/// Messages sent to the para that holds the deposits, oldest first.
	static TO_PARA: RefCell<VecDeque<MessageToPara>> = const { RefCell::new(VecDeque::new()) };
	/// Messages sent to the relay chain, oldest first.
	static TO_RELAY: RefCell<VecDeque<MessageToRelay>> = const { RefCell::new(VecDeque::new()) };
	/// Whether the transport to the para refuses every message.
	static TRANSPORT_DOWN: Cell<bool> = const { Cell::new(false) };
}

/// Queues messages until [`deliver`].
pub struct Queue;

impl SendToPara for Queue {
	fn send(message: MessageToPara) -> Result<(), ()> {
		if TRANSPORT_DOWN.get() {
			return Err(());
		}
		TO_PARA.with_borrow_mut(|q| q.push_back(message));
		Ok(())
	}
}

impl SendToRelay for Queue {
	fn send(message: MessageToRelay) -> Result<(), ()> {
		TO_RELAY.with_borrow_mut(|q| q.push_back(message));
		Ok(())
	}
}

impl pallet_hrmp_relay::Config for Test {
	type OnDepositSet = Hrmp;
	type HolderParaId = HolderParaId;
	type SendToPara = Queue;
	type MaxFailedReleases = ConstU32<4>;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

/// A para's sovereign account, as the relay chain derives it.
pub struct SovereignAccount;
impl Convert<u32, AccountId> for SovereignAccount {
	fn convert(para: u32) -> AccountId {
		account(para.into())
	}
}

impl pallet_hrmp_para::Config for Test {
	type Currency = Balances;
	type SendToRelay = Queue;
	type SovereignAccountOf = SovereignAccount;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

/// The para that holds the deposits.
const HOLDER: u32 = 1005;
/// Two funded paras.
const A: ParaId = ParaId::new(2001);
const B: ParaId = ParaId::new(2002);
/// A para with no funds.
const POOR: ParaId = ParaId::new(2003);
/// Two system paras: channels with them are free.
const SYSTEM_1: ParaId = ParaId::new(1000);
const SYSTEM_2: ParaId = ParaId::new(1001);

const ENDOWMENT: Balance = 1_000;
const SENDER_DEPOSIT: Balance = 100;
const RECIPIENT_DEPOSIT: Balance = 100;
/// What every para may open at most, as a sender or as a recipient.
const MAX_CHANNELS: u32 = 2;

fn account(para: ParaId) -> AccountId {
	para.into_account_truncating()
}

/// What `para` holds for channel deposits.
fn on_hold(para: ParaId) -> Balance {
	Balances::balance_on_hold(&HoldReason::ChannelDeposit.into(), &account(para))
}

fn para_origin(para: ParaId) -> RuntimeOrigin {
	Origin::Parachain(para).into()
}

fn channel(sender: ParaId, recipient: ParaId) -> HrmpChannelId {
	HrmpChannelId { sender, recipient }
}

fn wire(sender: ParaId, recipient: ParaId) -> ChannelId {
	ChannelId { sender: sender.into(), recipient: recipient.into() }
}

/// What the para records for `side` of the channel from `sender` to `recipient`.
fn para_deposit(sender: ParaId, recipient: ParaId, side: DepositSide) -> Option<Balance> {
	Deposits::<Test>::get((wire(sender, recipient), side)).map(|d| d.amount)
}

/// Delivers every queued message, in both directions, until nothing is left.
fn deliver() {
	loop {
		if let Some(message) = TO_PARA.with_borrow_mut(|q| q.pop_front()) {
			assert_ok!(HrmpPara::receive(RuntimeOrigin::root(), message));
		} else if let Some(message) = TO_RELAY.with_borrow_mut(|q| q.pop_front()) {
			assert_ok!(HrmpRelay::receive(para_origin(HOLDER.into()), message));
		} else {
			break;
		}
	}
}

fn queued_to_para() -> usize {
	TO_PARA.with_borrow(|q| q.len())
}

fn queued_to_relay() -> usize {
	TO_RELAY.with_borrow(|q| q.len())
}

/// Checks that each depositor has exactly its recorded deposits on hold on the para, as
/// `pallet-hrmp-para`'s `try_state` does.
fn check_para_holds() {
	let mut per_depositor = std::collections::BTreeMap::<AccountId, Balance>::new();
	for (_, deposit) in Deposits::<Test>::iter() {
		assert_ne!(deposit.amount, 0, "a side holding nothing is recorded");
		*per_depositor.entry(deposit.depositor).or_default() += deposit.amount;
	}
	for (depositor, recorded) in per_depositor {
		let on_hold = Balances::balance_on_hold(&HoldReason::ChannelDeposit.into(), &depositor);
		assert_eq!(on_hold, recorded, "a depositor's hold differs from its deposits");
	}
}

/// The `hrmp` pallet's events so far, taken.
fn hrmp_events() -> Vec<hrmp::Event<Test>> {
	let events = System::events()
		.into_iter()
		.filter_map(|r| match r.event {
			RuntimeEvent::Hrmp(e) => Some(e),
			_ => None,
		})
		.collect();
	System::reset_events();
	events
}

/// This pallet's events so far, taken.
fn relay_events() -> Vec<crate::Event<Test>> {
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

/// Opens the channel from `sender` to `recipient` end to end: request, accept, process.
fn open_channel(sender: ParaId, recipient: ParaId) {
	assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(sender), recipient, 2, 8));
	deliver();
	assert_ok!(Hrmp::hrmp_accept_open_channel(para_origin(recipient), sender));
	deliver();
	assert_ok!(Hrmp::force_process_hrmp_open(RuntimeOrigin::root(), 1));
	assert!(HrmpChannels::<Test>::contains_key(channel(sender, recipient)));
}

/// Runs `test` at block 1 with the paras registered and funded, every queue empty and the
/// transport up, then checks that nothing is left undelivered and both pallets' invariants.
fn new_test_ext(test: impl FnOnce()) {
	let config = HostConfiguration {
		max_downward_message_size: 1024,
		hrmp_channel_max_capacity: 2,
		hrmp_channel_max_message_size: 8,
		hrmp_max_parachain_outbound_channels: MAX_CHANNELS,
		hrmp_max_parachain_inbound_channels: MAX_CHANNELS,
		hrmp_max_message_num_per_candidate: 2,
		hrmp_channel_max_total_size: 16,
		hrmp_sender_deposit: SENDER_DEPOSIT,
		hrmp_recipient_deposit: RECIPIENT_DEPOSIT,
		..Default::default()
	};
	let genesis_para = |id| {
		let args = ParaGenesisArgs {
			genesis_head: vec![1].into(),
			validation_code: vec![1].into(),
			para_kind: ParaKind::Parachain,
		};
		(id, args)
	};

	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(account(A), ENDOWMENT), (account(B), ENDOWMENT)],
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.unwrap();
	configuration::GenesisConfig::<Test> { config }
		.assimilate_storage(&mut storage)
		.unwrap();
	paras::GenesisConfig::<Test> {
		paras: [A, B, POOR, SYSTEM_1, SYSTEM_2].map(genesis_para).to_vec(),
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	let mut ext: sp_io::TestExternalities = storage.into();
	ext.execute_with(|| {
		System::set_block_number(1);
		TO_PARA.take();
		TO_RELAY.take();
		TRANSPORT_DOWN.set(false);

		test();

		assert_eq!(queued_to_para(), 0, "messages to the para left undelivered");
		assert_eq!(queued_to_relay(), 0, "messages to the relay left undelivered");
		HrmpRelay::do_try_state().unwrap();
		check_para_holds();
	});
}

/// A channel is requested, accepted and opened, each deposit held on the para by its own side.
#[test]
fn open_channel_holds_deposits_on_para() {
	new_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(A), B, 2, 8));
		// Nothing is recorded until the para answers.
		assert!(HrmpOpenChannelRequests::<Test>::get(channel(A, B)).is_none());
		assert_eq!(PendingRequests::<Test>::iter().count(), 1);
		assert_eq!(on_hold(A), 0);

		deliver();
		assert_eq!(on_hold(A), SENDER_DEPOSIT);
		assert_eq!(para_deposit(A, B, DepositSide::Sender), Some(SENDER_DEPOSIT));
		let request = HrmpOpenChannelRequests::<Test>::get(channel(A, B)).unwrap();
		assert_eq!(request.sender_deposit, SENDER_DEPOSIT);
		assert!(!request.confirmed);
		assert_eq!(PendingRequests::<Test>::iter().count(), 0);

		assert_ok!(Hrmp::hrmp_accept_open_channel(para_origin(B), A));
		deliver();
		assert_eq!(on_hold(B), RECIPIENT_DEPOSIT);
		assert_eq!(para_deposit(A, B, DepositSide::Recipient), Some(RECIPIENT_DEPOSIT));
		assert!(HrmpOpenChannelRequests::<Test>::get(channel(A, B)).unwrap().confirmed);

		assert_ok!(Hrmp::force_process_hrmp_open(RuntimeOrigin::root(), 1));
		let opened = HrmpChannels::<Test>::get(channel(A, B)).unwrap();
		assert_eq!(
			(opened.sender_deposit, opened.recipient_deposit),
			(SENDER_DEPOSIT, RECIPIENT_DEPOSIT)
		);
		// Opening moves nothing: the deposits were taken with the request.
		assert_eq!(queued_to_para(), 0);
	});
}

/// A para that cannot pay gets its request refused: nothing is recorded or held anywhere.
#[test]
fn request_refused_when_sender_cannot_pay() {
	new_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(POOR), B, 2, 8));
		hrmp_events();
		deliver();

		assert_eq!(
			hrmp_events(),
			vec![hrmp::Event::DepositRefused {
				channel_id: channel(POOR, B),
				error: DepositError::InsufficientFunds,
			}]
		);
		assert!(HrmpOpenChannelRequests::<Test>::get(channel(POOR, B)).is_none());
		assert_eq!(PendingRequests::<Test>::iter().count(), 0);
		assert_eq!(on_hold(POOR), 0);
		assert_eq!(Deposits::<Test>::iter().count(), 0);

		// The refusal frees the side for another request.
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(A), B, 2, 8));
		deliver();
		assert_eq!(on_hold(A), SENDER_DEPOSIT);
	});
}

/// While the para has not answered, the same side cannot be requested again.
#[test]
fn side_locked_until_answered() {
	new_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(A), B, 2, 8));
		assert_noop!(
			Hrmp::hrmp_init_open_channel(para_origin(A), B, 2, 8),
			crate::Error::<Test>::DepositPending
		);
		deliver();

		// Answered, it is recorded on the relay chain, which refuses the duplicate itself.
		assert_noop!(
			Hrmp::hrmp_init_open_channel(para_origin(A), B, 2, 8),
			hrmp::Error::<Test>::OpenHrmpChannelAlreadyRequested
		);
	});
}

/// Closing a channel gives both deposits back on the para.
#[test]
fn close_channel_releases_both_deposits() {
	new_test_ext(|| {
		open_channel(A, B);
		assert_eq!((on_hold(A), on_hold(B)), (SENDER_DEPOSIT, RECIPIENT_DEPOSIT));

		assert_ok!(Hrmp::hrmp_close_channel(para_origin(A), channel(A, B)));
		assert_ok!(Hrmp::force_process_hrmp_close(RuntimeOrigin::root(), 1));
		assert!(!HrmpChannels::<Test>::contains_key(channel(A, B)));
		// The release is on its way; the para still holds.
		assert_eq!(queued_to_para(), 1);
		assert_eq!((on_hold(A), on_hold(B)), (SENDER_DEPOSIT, RECIPIENT_DEPOSIT));

		deliver();
		assert_eq!((on_hold(A), on_hold(B)), (0, 0));
		assert_eq!(Deposits::<Test>::iter().count(), 0);
	});
}

/// Canceling a request gives the sender's deposit back on the para.
#[test]
fn cancel_request_releases_sender_deposit() {
	new_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(A), B, 2, 8));
		deliver();
		assert_eq!(on_hold(A), SENDER_DEPOSIT);

		assert_ok!(Hrmp::hrmp_cancel_open_request(para_origin(A), channel(A, B), 1));
		deliver();
		assert!(HrmpOpenChannelRequests::<Test>::get(channel(A, B)).is_none());
		assert_eq!(on_hold(A), 0);
		assert_eq!(Deposits::<Test>::iter().count(), 0);
	});
}

/// Poking an open channel after the prices changed moves each side to the new price.
#[test]
fn poke_moves_deposits_to_new_prices() {
	new_test_ext(|| {
		open_channel(A, B);
		configuration::ActiveConfig::<Test>::mutate(|c| {
			c.hrmp_sender_deposit = 150;
			c.hrmp_recipient_deposit = 50;
		});

		assert_ok!(Hrmp::poke_channel_deposits(RuntimeOrigin::signed(1), A, B));
		// Pending until the para answers.
		let opened = HrmpChannels::<Test>::get(channel(A, B)).unwrap();
		assert_eq!(
			(opened.sender_deposit, opened.recipient_deposit),
			(SENDER_DEPOSIT, RECIPIENT_DEPOSIT)
		);
		hrmp_events();

		deliver();
		assert_eq!((on_hold(A), on_hold(B)), (150, 50));
		let opened = HrmpChannels::<Test>::get(channel(A, B)).unwrap();
		assert_eq!((opened.sender_deposit, opened.recipient_deposit), (150, 50));
		assert_eq!(
			hrmp_events(),
			vec![hrmp::Event::OpenChannelDepositsUpdated { sender: A, recipient: B }]
		);
	});
}

/// Forcing a channel open between two paras takes both deposits in one request, so that it is
/// opened whole or not at all.
#[test]
fn force_open_takes_both_deposits_at_once() {
	new_test_ext(|| {
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, B, 2, 8));
		assert_eq!(queued_to_para(), 1);
		deliver();
		assert_eq!((on_hold(A), on_hold(B)), (SENDER_DEPOSIT, RECIPIENT_DEPOSIT));
		assert!(HrmpOpenChannelRequests::<Test>::get(channel(A, B)).unwrap().confirmed);
		assert_ok!(Hrmp::force_process_hrmp_open(RuntimeOrigin::root(), 1));
		assert!(HrmpChannels::<Test>::contains_key(channel(A, B)));

		// With one side unable to pay, the other is not held either.
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), B, POOR, 2, 8));
		hrmp_events();
		deliver();
		assert_eq!(
			hrmp_events(),
			vec![hrmp::Event::DepositRefused {
				channel_id: channel(B, POOR),
				error: DepositError::InsufficientFunds,
			}]
		);
		assert!(HrmpOpenChannelRequests::<Test>::get(channel(B, POOR)).is_none());
		assert_eq!((on_hold(B), on_hold(POOR)), (RECIPIENT_DEPOSIT, 0));
	});
}

/// Governance answers for a para that does not: the request is refused on the relay chain, and
/// what the para holds once the request reaches it late is given back by governance there.
#[test]
fn governance_answers_and_cleans_up() {
	new_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(A), B, 2, 8));
		let id = PendingRequests::<Test>::iter_keys().next().unwrap();
		hrmp_events();

		let refusal = Err(WireDepositError::Other);
		assert_ok!(HrmpRelay::force_answer(RuntimeOrigin::root(), id, refusal));
		assert_eq!(
			hrmp_events(),
			vec![hrmp::Event::DepositRefused {
				channel_id: channel(A, B),
				error: DepositError::Other,
			}]
		);
		assert_eq!(PendingRequests::<Test>::iter().count(), 0);

		// The request reaches the para late: it holds, but its answer is ignored.
		relay_events();
		deliver();
		assert_eq!(on_hold(A), SENDER_DEPOSIT);
		assert_eq!(relay_events(), vec![crate::Event::UnknownAnswer { id }]);
		assert!(HrmpOpenChannelRequests::<Test>::get(channel(A, B)).is_none());

		// Governance gives it back on the para.
		let side = DepositSide::Sender;
		assert_ok!(HrmpPara::force_set_deposit(RuntimeOrigin::root(), wire(A, B), side, 0));
		assert_eq!(on_hold(A), 0);
		assert_eq!(Deposits::<Test>::iter().count(), 0);
	});
}

/// Channels with the system cost nothing: the para is never involved.
#[test]
fn system_channel_needs_no_deposit() {
	new_test_ext(|| {
		assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, SYSTEM_1, 2, 8));
		assert_ok!(Hrmp::force_process_hrmp_open(RuntimeOrigin::root(), 1));
		let opened = HrmpChannels::<Test>::get(channel(A, SYSTEM_1)).unwrap();
		assert_eq!((opened.sender_deposit, opened.recipient_deposit), (0, 0));
		assert_eq!(queued_to_para(), 0);
		assert_eq!(PendingRequests::<Test>::iter().count(), 0);
		assert_eq!(on_hold(A), 0);
	});
}

/// The relay chain changed while the para was answering: the request can no longer be
/// recorded, and the deposit just taken goes straight back.
#[test]
fn deposit_returned_when_request_no_longer_valid() {
	new_test_ext(|| {
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(A), B, 2, 8));
		// Meanwhile `A` fills its outbound channels for free.
		for system in [SYSTEM_1, SYSTEM_2] {
			assert_ok!(Hrmp::force_open_hrmp_channel(RuntimeOrigin::root(), A, system, 2, 8));
		}
		hrmp_events();

		deliver();
		assert!(matches!(
			hrmp_events().as_slice(),
			[hrmp::Event::DepositReturned { channel_id, .. }] if *channel_id == channel(A, B)
		));
		assert!(HrmpOpenChannelRequests::<Test>::get(channel(A, B)).is_none());
		assert_eq!(on_hold(A), 0);
		assert_eq!(Deposits::<Test>::iter().count(), 0);
	});
}

/// A release the transport refuses waits on the relay chain, blocks other requests, and is
/// retried in a later block.
#[test]
fn release_retried_after_transport_failure() {
	new_test_ext(|| {
		open_channel(A, B);

		TRANSPORT_DOWN.set(true);
		assert_ok!(Hrmp::hrmp_close_channel(para_origin(A), channel(A, B)));
		assert_ok!(Hrmp::force_process_hrmp_close(RuntimeOrigin::root(), 1));
		assert_eq!(FailedReleases::<Test>::decode_len(), Some(1));
		assert_eq!((on_hold(A), on_hold(B)), (SENDER_DEPOSIT, RECIPIENT_DEPOSIT));
		assert_noop!(
			Hrmp::hrmp_init_open_channel(para_origin(B), A, 2, 8),
			crate::Error::<Test>::TransportBusy
		);

		TRANSPORT_DOWN.set(false);
		HrmpRelay::on_poll(2, &mut WeightMeter::new());
		assert!(FailedReleases::<Test>::get().is_empty());
		deliver();
		assert_eq!((on_hold(A), on_hold(B)), (0, 0));

		// Unblocked.
		assert_ok!(Hrmp::hrmp_init_open_channel(para_origin(B), A, 2, 8));
		deliver();
		assert_eq!(on_hold(B), SENDER_DEPOSIT);
	});
}
