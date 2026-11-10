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

//! Test environment for `pallet-jamkb`.
//!
//! parachain-system and JAM are not part of the runtime. A mocked backend stands in for both.
//! Its state lives in storage, so a call that fails rolls it back, as it would roll back
//! parachain-system's storage.
//!
//! - [`MockSender`] is parachain-system's send queue: it allocates message and operation ids from
//!   distinct ranges and keeps the queued operations, up to [`TransportCapacity`]. A test can make
//!   every send fail with [`SendFailure`]. Per-block message, byte and gas limits are not modelled.
//! - [`Jam`] is the JAM side: the services, their regular and supervisor balances, threshold
//!   balances and `min_memo_gas`. The Parachain Service starts with the whole `CAP`.
//! - [`accumulate`] plays the accumulation of the queued operations: the Parachain Service executes
//!   each `TransferOut` as JAM's `transfer` does (`WHO`, then `LOW`, then `CASH`), credits the
//!   destination at once, writes a `TransferFailed` for each one refused, and parachain-system
//!   reports each outcome to the pallet.
//! - [`fail_next`] reports the oldest queued operation as refused by JAM, without the ledger.
//! - [`return_from`] plays a voluntary return on JAM and [`deliver`] hands a bucket to the pallet
//!   as parachain-system would, rolling it back on `Err`.
//! - [`poll`] runs the pallet's settlement hook.
//! - [`DeliveryFee`] burns [`FEE_PER_MESSAGE`] native units from the payer per message.
//!
//! The test settings ([`TransportCapacity`], [`SendFailure`], [`CurrentTimeslot`]) are thread-local
//! and stay until the test changes them; the backend state is in storage and rolls back with a
//! failed call. [`poll`] and [`build_and_execute`] check the pallet's invariants after they run.

use crate as pallet_jamkb;
pub use crate::*;
use alloc::collections::BTreeMap;
use codec::{Decode, Encode};
use frame_support::{
	assert_ok, derive_impl, parameter_types,
	storage::{unhashed, with_storage_layer},
	traits::{
		fungible,
		tokens::{Fortitude, Precision, Preservation},
		AsEnsureOriginWithArg, Hooks,
	},
	weights::WeightMeter,
	PalletId,
};
use messages::{AccumulateLog, MessageId, TransferError, UpwardMessage};
use sp_runtime::{
	traits::{BlockNumberProvider, IdentityLookup},
	BuildStorage,
};
use transport::{
	ChargeDeliveryFee, IncomingTransfer, IncomingTransfers, JamMessageSender, MessageSendError,
	OnIncomingTransfers, OnJamOutcome, OperationId, SenderId,
};

pub type AccountId = <Test as frame_system::Config>::AccountId;
pub type Balance = <Test as pallet_balances::Config>::Balance;
type Block = frame_system::mocking::MockBlock<Test>;

/// The JAMKB asset id in the test runtime.
pub const JAMKB: u32 = 7;
/// The fixed supply minted at `initialize`.
pub const CAP: Balance = 1_000_000;
/// The least amount a release or redemption accepts.
pub const MIN_TRANSFER: Balance = 10;
/// The gas every deferred transfer carries.
pub const MEMO_GAS: u64 = 5_000;
/// The operations one block settles within `SettlementWeight`.
pub const SETTLEMENTS_PER_BLOCK: u64 = 3;
/// The pallet's sender id at the transport.
pub const JAMKB_SENDER: SenderId = 3;
/// The default number of operations the mock transport queues before it reports `QueueFull`.
pub const DEFAULT_TRANSPORT_CAPACITY: usize = 4;
/// The first message id the mock transport allocates is one above this. It differs from the
/// operation ids so that a test cannot pass by mistaking one for the other.
pub const MESSAGE_ID_BASE: MessageId = 1_000;
/// The first operation id the mock transport allocates is one above this. It differs from the
/// allocation ids for the same reason.
pub const OPERATION_ID_BASE: OperationId = 100;
/// The Parachain Service id.
pub const PARACHAIN_SERVICE: ServiceId = 0;
/// The Parachain Service's threshold balance.
pub const PS_THRESHOLD: Balance = 1_000;
/// A JAM service used as a target. It exists and accepts `MEMO_GAS`.
pub const SERVICE: ServiceId = 42;
/// A service id no JAM service has.
pub const UNKNOWN_SERVICE: ServiceId = 404;
/// A holder account.
pub const ALICE: AccountId = 1;
/// Another account.
pub const BOB: AccountId = 2;
/// A memo.
pub const MEMO: Memo = [0xAB; 128];
/// The native units a signed call pays per message it sends.
pub const FEE_PER_MESSAGE: u64 = 3;
/// The native balance `ALICE` and `BOB` start with.
pub const NATIVE: u64 = 1_000;
/// An account with no native balance, so it cannot hold the non-sufficient JAMKB.
pub const NOBODY: AccountId = 99;
/// The buckets the pallet takes per block.
pub const BUCKETS_PER_BLOCK: u32 = 2;
/// The most transfers in one bucket.
pub const TRANSFERS_PER_BUCKET: u32 = 4;

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeTask,
		RuntimeHoldReason,
		RuntimeFreezeReason
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;
	#[runtime::pallet_index(10)]
	pub type Balances = pallet_balances;
	#[runtime::pallet_index(20)]
	pub type Assets = pallet_assets;
	#[runtime::pallet_index(21)]
	pub type AssetsHolder = pallet_assets_holder;
	#[runtime::pallet_index(30)]
	pub type Jamkb = pallet_jamkb;
}

// Account ids are sixteen bytes so that the pallet's derived accounts stay distinct.
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type DbWeight = MockDbWeight;
	type AccountId = u128;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig as pallet_assets::DefaultConfig)]
impl pallet_assets::Config for Test {
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<u128>>;
	type ForceOrigin = frame_system::EnsureRoot<u128>;
	type Currency = Balances;
	type Holder = AssetsHolder;
}

impl pallet_assets_holder::Config for Test {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeEvent = RuntimeEvent;
}

/// One operation queued at the mock transport.
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct Queued {
	/// The sender that queued it.
	pub sender: SenderId,
	/// The operation id the transport assigned.
	pub id: OperationId,
	/// Its messages.
	pub messages: Vec<UpwardMessage<Balance>>,
}

/// A JAM service as the mock sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Encode, Decode)]
pub struct JamService {
	/// The regular balance.
	pub balance: Balance,
	/// The supervisor balance.
	pub supervisor_balance: Balance,
	/// The balance its state footprint requires; a transfer may not take it below.
	pub threshold: Balance,
	/// The least gas a deferred transfer to it must carry.
	pub min_memo_gas: u64,
}

#[derive(Default, Encode, Decode)]
struct Backend {
	last_message_id: MessageId,
	last_operation_id: OperationId,
	queued: Vec<Queued>,
	services: BTreeMap<ServiceId, JamService>,
}

const BACKEND_KEY: &[u8] = b":mock:jamkb:backend";

/// Runs `f` on the backend state, kept in storage under [`BACKEND_KEY`].
fn with_backend<R>(f: impl FnOnce(&mut Backend) -> R) -> R {
	let mut backend: Backend = unhashed::get(BACKEND_KEY).unwrap_or_default();
	let result = f(&mut backend);
	unhashed::put(BACKEND_KEY, &backend);
	result
}

fn read_backend<R>(f: impl FnOnce(&Backend) -> R) -> R {
	f(&unhashed::get(BACKEND_KEY).unwrap_or_default())
}

/// The stand-in for parachain-system's send queue.
pub struct MockSender;

impl JamMessageSender<Balance> for MockSender {
	fn next_message_id() -> MessageId {
		with_backend(|b| {
			b.last_message_id += 1;
			b.last_message_id
		})
	}

	fn send(
		sender: SenderId,
		messages: Vec<UpwardMessage<Balance>>,
	) -> Result<OperationId, MessageSendError> {
		if let Some(error) = SendFailure::get() {
			return Err(error);
		}
		with_backend(|b| {
			if b.queued.len() >= TransportCapacity::get() {
				return Err(MessageSendError::QueueFull);
			}
			b.last_operation_id += 1;
			let id = b.last_operation_id;
			b.queued.push(Queued { sender, id, messages });
			Ok(id)
		})
	}
}

/// Burns [`FEE_PER_MESSAGE`] native units from the payer per message.
pub struct DeliveryFee;

impl ChargeDeliveryFee<AccountId, Balance> for DeliveryFee {
	fn charge(payer: &AccountId, messages: &[UpwardMessage<Balance>]) -> DispatchResult {
		let fee = FEE_PER_MESSAGE * messages.len() as u64;
		<Balances as fungible::Mutate<AccountId>>::burn_from(
			payer,
			fee,
			Preservation::Preserve,
			Precision::Exact,
			Fortitude::Polite,
		)?;
		Ok(())
	}
}

/// The current timeslot, set by the tests.
pub struct Timeslots;

impl BlockNumberProvider for Timeslots {
	type BlockNumber = Timeslot;

	fn current_block_number() -> Timeslot {
		CurrentTimeslot::get()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_block_number(n: Timeslot) {
		CurrentTimeslot::set(n);
	}
}

/// Records the accounts the pallet maps in pallet-revive.
pub struct RecordingMapping;

impl MapAccount<AccountId> for RecordingMapping {
	fn map(who: &AccountId) -> DispatchResult {
		Mapped::mutate(|m| m.push(*who));
		Ok(())
	}
}

/// The JAM side of the mocked backend.
pub struct Jam;

impl Jam {
	/// The service `id`, if it exists.
	pub fn service(id: ServiceId) -> Option<JamService> {
		read_backend(|b| b.services.get(&id).copied())
	}

	/// The regular balance of `id`, zero for a service that does not exist.
	pub fn balance(id: ServiceId) -> Balance {
		Self::service(id).map_or(0, |s| s.balance)
	}

	/// Creates or replaces the service `id`.
	pub fn set_service(id: ServiceId, service: JamService) {
		with_backend(|b| b.services.insert(id, service));
	}

	/// Sets the regular balance of the existing service `id`.
	pub fn set_balance(id: ServiceId, balance: Balance) {
		with_backend(|b| b.services.get_mut(&id).expect("the service exists").balance = balance);
	}

	/// The supervisor balance of `id`, zero for a service that does not exist.
	pub fn supervisor_balance(id: ServiceId) -> Balance {
		Self::service(id).map_or(0, |s| s.supervisor_balance)
	}

	/// The Parachain Service executes one `TransferOut` as JAM's `transfer` would.
	fn transfer(message: &UpwardMessage<Balance>) -> Option<AccumulateLog> {
		let UpwardMessage::TransferOut {
			source,
			dest,
			amount,
			id,
			deferred,
			source_supervisor_balance,
			dest_supervisor_balance,
		} = message;
		let source = source.unwrap_or(PARACHAIN_SERVICE);
		let error = with_backend(|b| {
			let Some(from) = b.services.get(&source).copied() else {
				return Some(TransferError::UnknownSource);
			};
			let Some(to) = b.services.get(dest).copied() else {
				return Some(TransferError::UnknownDestination);
			};
			let gas = deferred.map_or(0, |(_, gas)| gas);
			if deferred.is_some() && gas < to.min_memo_gas {
				return Some(TransferError::GasBelowDestinationMinimum);
			}
			if *source_supervisor_balance {
				if from.supervisor_balance < *amount {
					return Some(TransferError::InsufficientServiceBalance);
				}
				b.services.get_mut(&source).expect("checked above; qed").supervisor_balance -=
					amount;
			} else {
				if from.balance.checked_sub(*amount).map_or(true, |rest| rest < from.threshold) {
					return Some(TransferError::InsufficientServiceBalance);
				}
				b.services.get_mut(&source).expect("checked above; qed").balance -= amount;
			}
			let to = b.services.get_mut(dest).expect("checked above; qed");
			if *dest_supervisor_balance {
				to.supervisor_balance += amount;
			} else {
				to.balance += amount;
			}
			None
		});
		error.map(|error| AccumulateLog::TransferFailed { id: *id, error })
	}
}

parameter_types! {
	pub const MockDbWeight: frame_support::weights::RuntimeDbWeight =
		frame_support::weights::RuntimeDbWeight { read: 1_000, write: 2_000 };
	pub static TransportCapacity: usize = DEFAULT_TRANSPORT_CAPACITY;
	pub static SendFailure: Option<MessageSendError> = None;
	pub static Mapped: Vec<AccountId> = vec![];
	pub static CurrentTimeslot: Timeslot = 1;
	pub const JamkbAssetId: u32 = JAMKB;
	pub const Cap: Balance = CAP;
	pub const MinTransfer: Balance = MIN_TRANSFER;
	pub const MemoGas: u64 = MEMO_GAS;
	pub SettlementWeight: Weight = settlement_overhead()
		.saturating_add(<() as WeightInfo>::settle().saturating_mul(SETTLEMENTS_PER_BLOCK));
	pub const JamkbSender: SenderId = JAMKB_SENDER;
	pub const JamkbPalletId: PalletId = PalletId(*b"py/jamkb");
	pub const ParachainService: ServiceId = PARACHAIN_SERVICE;
	pub const MaxUnsettledOperations: u32 = 6;
	pub const MaxBucketsPerBlock: u32 = BUCKETS_PER_BLOCK;
	pub const MaxTransfersPerBucket: u32 = TRANSFERS_PER_BUCKET;
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;
	type AssetId = u32;
	type Balance = Balance;
	type Assets = Assets;
	type AssetsHolder = AssetsHolder;
	type MessageSender = MockSender;
	type DeliveryFee = DeliveryFee;
	type TimeslotProvider = Timeslots;
	type SenderId = JamkbSender;
	type JamkbAssetId = JamkbAssetId;
	type Cap = Cap;
	type MinTransfer = MinTransfer;
	type MemoGas = MemoGas;
	type SettlementWeight = SettlementWeight;
	type PalletId = JamkbPalletId;
	type ParachainService = ParachainService;
	type MaxUnsettledOperations = MaxUnsettledOperations;
	type MaxBucketsPerBlock = MaxBucketsPerBlock;
	type MaxTransfersPerBucket = MaxTransfersPerBucket;
	type CustodyMapping = RecordingMapping;
	type GovernanceOrigin = frame_system::EnsureRoot<u128>;
	type WeightInfo = ();
}

/// Builds the test state: JAM holds the Parachain Service with `CAP` and `SERVICE` with
/// nothing. The pallet is not initialized; tests call `initialize` and `attest` themselves.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = RuntimeGenesisConfig {
		balances: pallet_balances::GenesisConfig {
			balances: vec![(ALICE, NATIVE), (BOB, NATIVE)],
			..Default::default()
		},
		..Default::default()
	}
	.build_storage()
	.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| {
		System::set_block_number(1);
		CurrentTimeslot::set(1);
		with_backend(|b| {
			b.last_message_id = MESSAGE_ID_BASE;
			b.last_operation_id = OPERATION_ID_BASE;
		});
		Jam::set_service(
			PARACHAIN_SERVICE,
			JamService {
				balance: CAP,
				supervisor_balance: 0,
				threshold: PS_THRESHOLD,
				min_memo_gas: 0,
			},
		);
		Jam::set_service(SERVICE, JamService { min_memo_gas: MEMO_GAS, ..Default::default() });
	});
	ext
}

/// The operations queued at the transport and not yet accumulated, oldest first.
pub fn sent() -> Vec<Queued> {
	read_backend(|b| b.queued.clone())
}

/// The id of the last message the transport allocated.
pub fn last_message_id() -> MessageId {
	read_backend(|b| b.last_message_id)
}

/// The oldest `count` queued operations are accumulated on JAM at timeslot `at`;
/// parachain-system reports each outcome to its sender. Returns the weight the pallet charged.
pub fn accumulate(count: usize, at: Timeslot) -> Weight {
	let operations: Vec<Queued> = with_backend(|b| b.queued.drain(..count).collect());
	let mut meter = WeightMeter::new();
	for op in operations {
		let entries: Vec<AccumulateLog> = op.messages.iter().filter_map(Jam::transfer).collect();
		<Jamkb as OnJamOutcome>::on_outcome(op.sender, op.id, at, &entries, &mut meter);
	}
	meter.consumed()
}

/// The oldest queued operation is accumulated at timeslot `at` and JAM refuses its transfer with
/// `error`; the ledger is not touched. Returns the operation's id.
pub fn fail_next(at: Timeslot, error: TransferError) -> OperationId {
	let op = with_backend(|b| b.queued.remove(0));
	let entries: Vec<AccumulateLog> = op
		.messages
		.iter()
		.map(|UpwardMessage::TransferOut { id, .. }| AccumulateLog::TransferFailed {
			id: *id,
			error,
		})
		.collect();
	<Jamkb as OnJamOutcome>::on_outcome(op.sender, op.id, at, &entries, &mut WeightMeter::new());
	op.id
}

/// What `on_poll` charges for reading and writing its list when there is work.
pub fn settlement_overhead() -> Weight {
	MockDbWeight::get().reads_writes(1, 1)
}

/// Every queued operation is accumulated on JAM at timeslot `at`.
pub fn accumulate_all(at: Timeslot) -> Weight {
	accumulate(sent().len(), at)
}

/// `source` returns `amount` to the Parachain Service on JAM, to its supervisor balance when
/// `to_supervisor`; the Parachain Service queues it in `incoming_transfers`.
pub fn return_from(
	source: ServiceId,
	amount: Balance,
	memo: Memo,
	to_supervisor: bool,
) -> IncomingTransfer<Balance> {
	with_backend(|b| {
		b.services.get_mut(&source).expect("the source exists").balance -= amount;
		let ps = b.services.get_mut(&PARACHAIN_SERVICE).expect("the Parachain Service exists");
		if to_supervisor {
			ps.supervisor_balance += amount;
		} else {
			ps.balance += amount;
		}
	});
	IncomingTransfer { source, amount, to_supervisor_balance: to_supervisor, memo }
}

/// Hands one bucket to the pallet as parachain-system does: within the capacity, and rolled
/// back on `Err`.
pub fn deliver(bucket: IncomingTransfers<Balance>) -> Result<(), ()> {
	if <Jamkb as OnIncomingTransfers<Balance>>::capacity() == 0 {
		return Err(());
	}
	with_storage_layer(|| {
		<Jamkb as OnIncomingTransfers<Balance>>::on_incoming_transfers(
			bucket,
			&mut WeightMeter::new(),
		)
		.map_err(|()| DispatchError::Other("bucket refused"))
	})
	.map_err(|_| ())
}

/// A voluntary return memo naming `who` as its beneficiary.
pub fn memo_for(who: AccountId) -> Memo {
	let mut memo = [0u8; 128];
	memo[..16].copy_from_slice(&who.to_le_bytes());
	memo
}

/// Runs the settlement hook with the whole block weight available. Returns the weight used.
pub fn poll() -> Weight {
	poll_within(Weight::MAX)
}

/// Runs the settlement hook with `remaining` weight left in the block. Returns the weight used.
pub fn poll_within(remaining: Weight) -> Weight {
	let mut meter = WeightMeter::with_limit(remaining);
	<Jamkb as Hooks<u64>>::on_poll(System::block_number(), &mut meter);
	assert_ok!(Jamkb::do_try_state());
	meter.consumed()
}

/// Runs the settlement hook without checking the invariants, for a test that breaks one on
/// purpose to make a settlement fail.
pub fn poll_unchecked() {
	<Jamkb as Hooks<u64>>::on_poll(System::block_number(), &mut WeightMeter::new());
}

/// Runs `f` on a fresh test state and checks the pallet's invariants afterwards.
pub fn build_and_execute(f: impl FnOnce()) {
	new_test_ext().execute_with(|| {
		f();
		assert_ok!(Jamkb::do_try_state());
	});
}

/// Advances to timeslot `n`.
pub fn run_to_timeslot(n: Timeslot) {
	CurrentTimeslot::set(n);
}

/// The custody account.
pub fn custody() -> AccountId {
	Jamkb::custody_account()
}

/// The released account.
pub fn released() -> AccountId {
	Jamkb::released_account()
}

/// The unclaimed account.
pub fn unclaimed() -> AccountId {
	Jamkb::unclaimed_account()
}

/// The excess account.
pub fn excess() -> AccountId {
	Jamkb::excess_account()
}

/// The native balance of `who`.
pub fn native(who: AccountId) -> u64 {
	Balances::free_balance(who)
}

/// The free JAMKB balance of `who`.
pub fn free(who: AccountId) -> Balance {
	<Assets as frame_support::traits::fungibles::Inspect<AccountId>>::balance(JAMKB, &who)
}

/// The JAMKB held on `who` under the `Releasing` reason.
pub fn held(who: AccountId) -> Balance {
	<AssetsHolder as frame_support::traits::fungibles::InspectHold<AccountId>>::balance_on_hold(
		JAMKB,
		&RuntimeHoldReason::Jamkb(HoldReason::Releasing),
		&who,
	)
}

/// The design's balance invariant once every operation has settled: the Parachain Service's
/// regular balance equals the supply minus the units Asset Hub records as released.
pub fn assert_books_match_jam() {
	assert_eq!(Jam::balance(PARACHAIN_SERVICE), CAP - free(released()));
}

/// The last event emitted.
pub fn last_event() -> RuntimeEvent {
	System::events().pop().expect("an event was emitted").event
}

/// The pallet's events emitted so far, oldest first.
pub fn jamkb_events() -> Vec<Event<Test>> {
	System::events()
		.into_iter()
		.filter_map(|r| match r.event {
			RuntimeEvent::Jamkb(e) => Some(e),
			_ => None,
		})
		.collect()
}
