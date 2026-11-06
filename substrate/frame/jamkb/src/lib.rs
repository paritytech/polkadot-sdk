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

//! # JAMKB Management Pallet (prototype)
//!
//! Manages the Asset Hub representation of JAMKB, the state-footprint token of JAM, as
//! described in `designs/jamkb-management/jamkb-management.md`.
//!
//! This prototype covers the permanent release and holder redemption flows and the asynchronous
//! operation machinery they rely on. Leases, lease recovery and voluntary returns are not
//! implemented; the types reserve their encodings so that adding them needs no migration.
//!
//! ## Overview
//!
//! The pallet holds the four privileged roles of the JAMKB asset through its custody account and
//! keeps undistributed units there. Units leave Parachain Service control only through an
//! operation: a `TransferOut` message to the Parachain Service on JAM, locked by a hold while in
//! flight and settled once the outcome is reported.
//!
//! The transport to JAM is the responsibility of `pallet-parachain-system`. This crate does not
//! depend on it; the [`transport`] traits are the boundary. The pallet queues each operation's
//! messages through [`transport::JamMessageSender`], and parachain-system reports each
//! operation's outcome, once and in send order, through [`transport::OnJamOutcome`]. The
//! [`messages`] module carries a minimal mirror of the Parachain Service message and log entry
//! shapes.
//!
//! ## Flows
//!
//! - Governance release: `approve_release` holds the amount on custody; `execute_release` sends a
//!   `Delivery` operation; its settlement moves the held units to the `released` account on
//!   Confirmed, or returns the allocation to `Approved` on Failed.
//! - Holder redemption: `redeem` holds the amount on the caller and sends a `Redemption` operation;
//!   its settlement moves the held units to the `released` account on Confirmed, or releases the
//!   hold on Failed.
//! - `grant` moves undistributed units from custody to an Asset Hub account.
//!
//! ## Accounts
//!
//! - `custody`: undistributed units. Once every operation has settled, the Parachain Service's
//!   regular balance equals the supply minus the `released` account (design §2).
//! - `released`: units that left Parachain Service control on JAM.
//! - `custodial`, `excess`: reserved for voluntary returns, unused by this prototype.
//!
//! ## Resolution and settlement
//!
//! An operation is `Requested` from the moment its messages are queued. Its outcome marks it
//! `Failed` when the reported entries hold a `TransferFailed` for its message, and `Confirmed`
//! otherwise. In the next `on_poll`, the resolved operations are settled in the order they were
//! sent, within `SettlementWeight`: a Confirmed operation applies its effect, a Failed one is
//! undone. A settlement that fails is rolled back and left for the permissionless `settle`
//! call; it does not hold up the operations after it.

#![deny(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{vec, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
	defensive,
	pallet_prelude::*,
	storage::with_storage_layer,
	traits::{
		fungibles::{Create, Inspect, InspectHold, Mutate, MutateHold},
		tokens::{Fortitude, Precision, Preservation, Restriction},
		EnsureOrigin,
	},
	weights::WeightMeter,
	PalletId,
};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::traits::{AccountIdConversion, One, Saturating, Zero};

pub use pallet::*;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

/// A JAM service index.
pub type ServiceId = u32;
/// Identifier of an allocation, assigned from 1.
pub type AllocationId = u64;
/// A JAM timeslot.
pub type Timeslot = u32;
/// A JAM transfer memo: 128 octets, as the Gray Paper fixes it.
pub type Memo = [u8; 128];
/// The header hash of the JAM block whose state the balances were checked against. The pallet
/// records it and does not verify it.
pub type Anchor = H256;

/// Minimal mirror of the Parachain Service messages and log entries the pallet uses.
///
/// The authoritative definitions live in the Parachain Service design. Only the fields the
/// pallet reads or writes are carried here.
pub mod messages {
	use super::*;

	/// The id a `TransferOut` carries, echoed by the Parachain Service in `TransferFailed`.
	pub type MessageId = u64;

	/// An upward message from Asset Hub to the Parachain Service.
	#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
	pub enum UpwardMessage<Balance> {
		/// Transfer balance from the Parachain Service to another JAM service.
		TransferOut {
			/// The debited service; `None` is the Parachain Service itself.
			source: Option<ServiceId>,
			/// The credited service.
			dest: ServiceId,
			/// The amount moved.
			amount: Balance,
			/// The id allocated by parachain-system, echoed back in `TransferFailed`.
			id: MessageId,
			/// Debit the source's supervisor balance instead of its regular balance.
			source_supervisor_balance: bool,
			/// Credit the destination's supervisor balance instead of its regular balance.
			dest_supervisor_balance: bool,
			/// The memo and gas of a deferred transfer, which runs the destination's code. A
			/// plain transfer carries `None`.
			deferred: Option<(Memo, u64)>,
		},
	}

	/// Why a JAM `transfer` failed, as reported by the Parachain Service.
	#[derive(
		Clone,
		Copy,
		Encode,
		Decode,
		DecodeWithMemTracking,
		MaxEncodedLen,
		PartialEq,
		Eq,
		Debug,
		TypeInfo,
	)]
	pub enum TransferError {
		/// `source` is not a known service.
		UnknownSource,
		/// `dest` is not a known service.
		UnknownDestination,
		/// The Parachain Service is not `source`'s effective supervisor.
		SourceNotSupervised,
		/// A plain transfer needs the Parachain Service to be `dest`'s effective supervisor.
		DestinationNotSupervised,
		/// The supplied gas is below `dest`'s `min_memo_gas`.
		GasBelowDestinationMinimum,
		/// The `source` service cannot cover `amount` and stay at its threshold balance.
		InsufficientServiceBalance,
	}

	/// An accumulate entry of `parachain_log` the pallet acts on.
	#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
	pub enum AccumulateLog {
		/// A `TransferOut` was rejected by JAM.
		TransferFailed {
			/// The id the `TransferOut` carried.
			id: MessageId,
			/// The JAM error.
			error: TransferError,
		},
	}
}

/// The boundary to the AH→JAM transport in `pallet-parachain-system`.
pub mod transport {
	use super::*;
	use messages::{AccumulateLog, MessageId, UpwardMessage};

	/// Identifies a sender; each sender has its own budget share.
	pub type SenderId = u8;
	/// One atomic batch of upward messages, allocated by parachain-system.
	pub type OperationId = u64;

	/// Why parachain-system refused to queue an operation.
	#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, Debug, TypeInfo)]
	pub enum MessageSendError {
		/// A `(kind, target)` name in the batch is in flight for another operation.
		Conflict,
		/// The batch can never fit one block.
		TooBig,
		/// There is no room in the queue now.
		QueueFull,
	}

	/// Implemented by parachain-system. An operation's messages are sent in one block, in the
	/// order the sender queued them. An operation is in flight from `send` until `on_outcome`.
	pub trait JamMessageSender<Balance> {
		/// Allocates the id for one `TransferOut`. The sender puts it in the message before
		/// `send`.
		fn next_message_id() -> MessageId;
		/// Queues `messages` as one operation and returns its id. On `Err` nothing is queued.
		fn send(
			sender: SenderId,
			messages: Vec<UpwardMessage<Balance>>,
		) -> Result<OperationId, MessageSendError>;
	}

	/// Implemented by each sender. Called by parachain-system.
	pub trait OnJamOutcome<BlockNumber> {
		/// Operation `id` of `sender` was accumulated. Once per operation, in send order.
		/// `sent_at` is the block that carried its messages; `entries` are its `parachain_log`
		/// entries, at most one per message. The sender records the outcome and nothing else.
		/// It cannot fail and charges what it writes to `meter`.
		fn on_outcome(
			sender: SenderId,
			id: OperationId,
			sent_at: BlockNumber,
			entries: &[AccumulateLog],
			meter: &mut WeightMeter,
		);
	}

	/// One transfer the Parachain Service received (Parachain Service design §5.1).
	#[derive(Clone, Encode, Decode, PartialEq, Eq, Debug, TypeInfo)]
	pub struct IncomingTransfer<Balance> {
		/// The sending service.
		pub source: ServiceId,
		/// The amount received.
		pub amount: Balance,
		/// Whether it landed on the Parachain Service's supervisor balance.
		pub to_supervisor_balance: bool,
		/// The memo it carried.
		pub memo: Memo,
	}

	/// One whole `incoming_transfers` bucket, in queue order.
	pub type IncomingTransfers<Balance> = Vec<IncomingTransfer<Balance>>;

	/// Implemented by `pallet-jamkb`. Called by parachain-system.
	pub trait OnIncomingTransfers<Balance> {
		/// How many buckets `on_incoming_transfers` takes this block. Asked before any bucket
		/// is read.
		fn capacity() -> u32;
		/// One whole bucket, booked all or nothing and charged to `meter`. `Err`: the bucket
		/// was not booked; parachain-system rolls the call back and delivers it again later.
		fn on_incoming_transfers(
			transfers: IncomingTransfers<Balance>,
			meter: &mut WeightMeter,
		) -> Result<(), ()>;
	}
}

use messages::{AccumulateLog, MessageId, TransferError, UpwardMessage};
use transport::{
	IncomingTransfers, JamMessageSender, MessageSendError, OnIncomingTransfers, OnJamOutcome,
	OperationId, SenderId,
};

/// Maps an account in pallet-revive, so that contracts can address it. The runtime implements it
/// with pallet-revive's address mapper; `()` maps nothing.
pub trait MapAccount<AccountId> {
	/// Maps `who`.
	fn map(who: &AccountId) -> DispatchResult;
}

impl<AccountId> MapAccount<AccountId> for () {
	fn map(_: &AccountId) -> DispatchResult {
		Ok(())
	}
}

/// Whether an allocation is a lease or a permanent release. Index 0 is reserved for `Lease`.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub enum AllocationMode {
	/// A permanent release to the target's regular balance.
	#[codec(index = 1)]
	Permanent {
		/// The memo the delivery carries, as supplied at `approve_release`.
		memo: Memo,
	},
}

/// Lifecycle of an allocation. Index 3 is reserved for `Reclaiming`.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub enum AllocationState {
	/// Approved; the units are held.
	#[codec(index = 0)]
	Approved,
	/// A delivery is in flight. On Failed, back to `Approved`.
	#[codec(index = 1)]
	Delivering,
	/// The delivery settled Confirmed. Terminal for a permanent release.
	#[codec(index = 2)]
	Delivered,
	/// Cancelled.
	#[codec(index = 4)]
	Closed,
}

/// One approved allocation.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo,
)]
pub struct Allocation<AccountId, Balance, BlockNumber> {
	/// The allocation's id.
	pub id: AllocationId,
	/// Lease or permanent release.
	pub mode: AllocationMode,
	/// The credited JAM service.
	pub target: ServiceId,
	/// The units held for the allocation.
	pub amount: Balance,
	/// Lifecycle state.
	pub state: AllocationState,
	/// The block the delivery was sent in.
	pub delivered_at: Option<BlockNumber>,
	/// The signing account or contract that approved it; `None`: governance is the approver.
	pub approver: Option<AccountId>,
	/// `execute_release` is rejected before this block.
	pub valid_from: BlockNumber,
	/// `execute_release` is rejected from this block on.
	pub expires_at: BlockNumber,
}

/// The data payload of an operation. Index 1 is reserved for `Increase`.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo,
)]
pub enum OperationPayload<AccountId, Balance> {
	/// A permanent release: one deferred `TransferOut` from the Parachain Service's regular
	/// balance to the target's regular balance, with the allocation's memo.
	///
	/// Confirmed: the allocation moves to `Delivered` and the held units move from custody to
	/// the `released` account. Failed: the allocation returns to `Approved`.
	#[codec(index = 0)]
	Delivery {
		/// The allocation delivered.
		allocation: AllocationId,
		/// The units locked for the transfer.
		amount: Balance,
	},
	/// A redemption of holder funds: one deferred `TransferOut` from the Parachain Service's
	/// regular balance to `target`'s regular balance, with the redeemer's memo.
	///
	/// Confirmed: the held units move from `holder` to the `released` account. Failed: the
	/// hold is released.
	#[codec(index = 2)]
	Redemption {
		/// The redeemer; the hold sits on it.
		holder: AccountId,
		/// The credited JAM service.
		target: ServiceId,
		/// The units locked for the transfer.
		amount: Balance,
		/// The memo the transfer carries.
		memo: Memo,
	},
}

/// The success entry of an operation that has one, cut to what the settlement reads. None of
/// the operations of this prototype has one.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub enum SuccessEntry<Balance> {
	/// `ServiceUpgraded`, for a freeze.
	Upgraded {
		/// The code hash the freeze replaced.
		previous_code_hash: H256,
		/// The `min_acc_gas` the freeze replaced.
		previous_min_acc_gas: u64,
		/// The `min_memo_gas` the freeze replaced.
		previous_min_memo_gas: u64,
	},
	/// `ServiceEjected`, for an ejection.
	Ejected {
		/// The amount credited to the Parachain Service.
		credited: Balance,
	},
	/// `ForgetAgainAt`, for a preimage release that needs a second `Forget`.
	ForgetAgainAt {
		/// The timeslot after which the second `Forget` may be sent.
		due: Timeslot,
	},
}

/// The state of an unsettled operation; a settled one is removed.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub enum OperationState<Balance> {
	/// Recorded and queued.
	Requested,
	/// Resolved to success; not settled yet.
	Confirmed(Option<SuccessEntry<Balance>>),
	/// Resolved to failure; not settled yet.
	Failed(FailureReason),
}

/// The failure carried by `OperationFailed`: one variant per failure entry class.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub enum FailureReason {
	/// A `TransferFailed` entry.
	Transfer(TransferError),
}

/// One cross-system operation between Asset Hub and JAM.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo,
)]
pub struct Operation<AccountId, Balance, BlockNumber> {
	/// The id parachain-system assigned.
	pub id: OperationId,
	/// What the operation does.
	pub payload: OperationPayload<AccountId, Balance>,
	/// Lifecycle state.
	pub state: OperationState<Balance>,
	/// The block the messages were sent in; unset until reported.
	pub submitted_at: Option<BlockNumber>,
	/// The id of the operation's `TransferOut`, which a `TransferFailed` echoes.
	pub message_id: MessageId,
}

/// Convenience alias for the allocation record.
pub type AllocationOf<T> =
	Allocation<<T as frame_system::Config>::AccountId, <T as Config>::Balance, BlockNumberFor<T>>;
/// Convenience alias for the operation record.
pub type OperationOf<T> =
	Operation<<T as frame_system::Config>::AccountId, <T as Config>::Balance, BlockNumberFor<T>>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		/// The asset identifier type of the assets pallet holding JAMKB.
		type AssetId: Member + Parameter + MaxEncodedLen + Clone;

		/// The balance type of JAMKB.
		type Balance: frame_support::traits::tokens::Balance;

		/// The assets pallet holding JAMKB.
		type Assets: Inspect<Self::AccountId, AssetId = Self::AssetId, Balance = Self::Balance>
			+ Mutate<Self::AccountId>
			+ Create<Self::AccountId>;

		/// The holder of JAMKB, providing the hold traits for the same asset.
		type AssetsHolder: InspectHold<
				Self::AccountId,
				AssetId = Self::AssetId,
				Balance = Self::Balance,
				Reason = Self::RuntimeHoldReason,
			> + MutateHold<Self::AccountId>;

		/// The AH→JAM transport that queues the pallet's operations.
		type MessageSender: JamMessageSender<Self::Balance>;

		/// The pallet's sender id at the transport.
		#[pallet::constant]
		type SenderId: Get<SenderId>;

		/// The identifier of the JAMKB asset.
		#[pallet::constant]
		type JamkbAssetId: Get<Self::AssetId>;

		/// The fixed JAMKB supply, minted at `initialize`.
		#[pallet::constant]
		type Cap: Get<Self::Balance>;

		/// The least amount `approve_release` and `redeem` accept. At least one unit.
		#[pallet::constant]
		type MinTransfer: Get<Self::Balance>;

		/// The gas every deferred `TransferOut` carries. A destination whose `min_memo_gas` is
		/// above it refuses the transfer.
		#[pallet::constant]
		type MemoGas: Get<u64>;

		/// The weight `on_poll` spends per block settling operations. Covers at least one
		/// settlement.
		#[pallet::constant]
		type SettlementWeight: Get<Weight>;

		/// The pallet's identifier, from which its accounts are derived.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// The service id of the Parachain Service. Rejected as a target.
		#[pallet::constant]
		type ParachainService: Get<ServiceId>;

		/// The most unsettled operations. A call that would exceed it is rejected. Must not be
		/// lowered below the number of unsettled operations by a runtime upgrade.
		#[pallet::constant]
		type MaxUnsettledOperations: Get<u32>;

		/// Maps the custody account in pallet-revive, so contracts can address it.
		type CustodyMapping: MapAccount<Self::AccountId>;

		/// The origin allowed to act as governance.
		type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for the calls.
		type WeightInfo: WeightInfo;
	}

	/// The hold classes. Indices 0 and 2 are reserved for `Leased` and `RecoveryDeposit`.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Units backing outstanding permanent releases and redemptions.
		#[codec(index = 1)]
		Releasing,
	}

	/// The JAM block the attestation refers to. Unset until attested.
	#[pallet::storage]
	pub type Attestation<T: Config> = StorageValue<_, Anchor, OptionQuery>;

	/// The pause flag.
	#[pallet::storage]
	pub type Paused<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// The last allocation id assigned.
	#[pallet::storage]
	pub type LastAllocationId<T: Config> = StorageValue<_, AllocationId, ValueQuery>;

	/// The allocations.
	#[pallet::storage]
	pub type Allocations<T: Config> =
		StorageMap<_, Blake2_128Concat, AllocationId, AllocationOf<T>, OptionQuery>;

	/// The unsettled operations, keyed by the id parachain-system assigned.
	#[pallet::storage]
	pub type Operations<T: Config> =
		CountedStorageMap<_, Blake2_128Concat, OperationId, OperationOf<T>, OptionQuery>;

	/// The unsettled operation of an allocation, if any.
	#[pallet::storage]
	pub type PendingOperationOf<T: Config> =
		StorageMap<_, Blake2_128Concat, AllocationId, OperationId, OptionQuery>;

	/// Operations resolved and not yet tried by `on_poll`, in the order sent. An operation whose
	/// settlement failed is no longer listed; it waits in `Operations` for `settle`.
	#[pallet::storage]
	pub type Resolved<T: Config> =
		StorageValue<_, BoundedVec<OperationId, T::MaxUnsettledOperations>, ValueQuery>;

	/// Events.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The JAMKB asset was created and the cap minted into custody.
		Initialized,
		/// The genesis attestation is recorded.
		Attested {
			/// The JAM block the balances were checked against.
			anchor: Anchor,
			/// The units held by services other than the Parachain Service at `anchor`.
			released: T::Balance,
		},
		/// The pallet is paused.
		Paused,
		/// The pallet is resumed.
		Resumed,
		/// Units moved from custody to an Asset Hub account.
		BudgetGranted {
			/// The receiving account.
			beneficiary: T::AccountId,
			/// The amount moved.
			amount: T::Balance,
		},
		/// A permanent release was approved.
		ReleaseApproved {
			/// The allocation.
			id: AllocationId,
			/// The credited service.
			target: ServiceId,
			/// The amount held.
			amount: T::Balance,
		},
		/// An allocation was cancelled.
		AllocationCancelled {
			/// The allocation.
			id: AllocationId,
		},
		/// An operation was recorded and its messages queued.
		OperationSubmitted {
			/// The operation.
			id: OperationId,
			/// The allocation; `None` for a redemption.
			allocation: Option<AllocationId>,
		},
		/// The operation resolved to success.
		OperationConfirmed {
			/// The operation.
			id: OperationId,
			/// Its success entry, for an operation that has one.
			entry: Option<SuccessEntry<T::Balance>>,
		},
		/// The operation resolved to failure.
		OperationFailed {
			/// The operation.
			id: OperationId,
			/// The Parachain Service failure.
			error: FailureReason,
		},
		/// The settlement of the operation failed; `settle` retries it.
		SettlementFailed {
			/// The operation.
			id: OperationId,
			/// The error the settlement failed with.
			error: DispatchError,
		},
	}

	/// Errors.
	#[pallet::error]
	pub enum Error<T> {
		/// The asset already exists.
		AlreadyInitialized,
		/// The asset does not exist yet.
		NotInitialized,
		/// The attestation is already recorded.
		AlreadyAttested,
		/// The pallet is not attested yet.
		NotAttested,
		/// The pallet is paused.
		Paused,
		/// The target is the Parachain Service.
		TargetIsParachainService,
		/// The amount is zero.
		ZeroAmount,
		/// The beneficiary is one of the pallet's accounts.
		PalletAccount,
		/// The amount is below `MinTransfer`.
		BelowMinTransfer,
		/// The validity window is empty.
		EmptyValidity,
		/// No allocation with this id.
		UnknownAllocation,
		/// No unsettled operation with this id.
		UnknownOperation,
		/// The operation is not resolved yet.
		NotResolved,
		/// The allocation is not in the state the call needs.
		WrongState,
		/// The allocation is not valid yet.
		NotYetValid,
		/// The allocation has expired.
		Expired,
		/// The allocation has not expired yet.
		NotExpired,
		/// The allocation has an unsettled operation.
		UnsettledOperation,
		/// `MaxUnsettledOperations` operations are unsettled.
		TooManyOperations,
		/// The transport refused the operation: no room now, too big for one block, or naming
		/// something already in flight.
		Transport,
	}

	impl<T> From<MessageSendError> for Error<T> {
		fn from(_: MessageSendError) -> Self {
			Error::Transport
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Creates the JAMKB asset, mints the cap into custody and maps the custody account in
		/// pallet-revive.
		///
		/// Origin: governance. The asset is non-sufficient with a minimum balance of one unit;
		/// each pallet account takes a provider reference so that it can hold the asset.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::initialize())]
		pub fn initialize(origin: OriginFor<T>) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			let asset = T::JamkbAssetId::get();
			ensure!(!T::Assets::asset_exists(asset.clone()), Error::<T>::AlreadyInitialized);
			let custody = Self::custody_account();
			for account in Self::pallet_accounts() {
				frame_system::Pallet::<T>::inc_providers(&account);
			}
			T::Assets::create(asset.clone(), custody.clone(), false, One::one())?;
			T::Assets::mint_into(asset, &custody, T::Cap::get())?;
			T::CustodyMapping::map(&custody)?;
			Self::deposit_event(Event::Initialized);
			Ok(())
		}

		/// Records the genesis attestation against JAM state and enables operations.
		///
		/// Origin: governance. `released` is the JAMKB held by services other than the
		/// Parachain Service at `anchor`; it moves from custody to the `released` account.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::attest())]
		pub fn attest(
			origin: OriginFor<T>,
			anchor: Anchor,
			released: T::Balance,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			ensure!(T::Assets::asset_exists(T::JamkbAssetId::get()), Error::<T>::NotInitialized);
			ensure!(Attestation::<T>::get().is_none(), Error::<T>::AlreadyAttested);
			if !released.is_zero() {
				T::Assets::transfer(
					T::JamkbAssetId::get(),
					&Self::custody_account(),
					&Self::released_account(),
					released,
					Preservation::Expendable,
				)?;
			}
			Attestation::<T>::put(anchor);
			Self::deposit_event(Event::Attested { anchor, released });
			Ok(())
		}

		/// Sets the pause flag. While set, a call that creates an allocation, records an
		/// operation or distributes units from custody is rejected. Operations already sent
		/// still settle.
		///
		/// Origin: governance.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::pause())]
		pub fn pause(origin: OriginFor<T>) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_attested()?;
			Paused::<T>::put(true);
			Self::deposit_event(Event::Paused);
			Ok(())
		}

		/// Clears the pause flag.
		///
		/// Origin: governance.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::resume())]
		pub fn resume(origin: OriginFor<T>) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_attested()?;
			Paused::<T>::put(false);
			Self::deposit_event(Event::Resumed);
			Ok(())
		}

		/// Moves `amount` of undistributed units from custody to `beneficiary`.
		///
		/// Origin: governance. Rejected when `beneficiary` is one of the pallet's accounts, whose
		/// balances mirror JAM.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::grant())]
		pub fn grant(
			origin: OriginFor<T>,
			beneficiary: T::AccountId,
			amount: T::Balance,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_active()?;
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			ensure!(!Self::pallet_accounts().contains(&beneficiary), Error::<T>::PalletAccount);
			T::Assets::transfer(
				T::JamkbAssetId::get(),
				&Self::custody_account(),
				&beneficiary,
				amount,
				Preservation::Expendable,
			)?;
			Self::deposit_event(Event::BudgetGranted { beneficiary, amount });
			Ok(())
		}

		/// Approves a permanent release of `amount` to `target` and holds the amount on custody.
		///
		/// Origin: governance, which becomes the approver. `memo` goes to `target` unchanged
		/// with the delivery. `valid_from` `None` means the current block; the allocation
		/// expires `valid_for` blocks after `valid_from`. Rejected when `target` is the
		/// Parachain Service, `amount` is below `MinTransfer` or `valid_for` is zero.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::approve_release())]
		pub fn approve_release(
			origin: OriginFor<T>,
			target: ServiceId,
			amount: T::Balance,
			memo: Memo,
			valid_from: Option<BlockNumberFor<T>>,
			valid_for: BlockNumberFor<T>,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_active()?;
			ensure!(target != T::ParachainService::get(), Error::<T>::TargetIsParachainService);
			ensure!(amount >= T::MinTransfer::get(), Error::<T>::BelowMinTransfer);
			ensure!(!valid_for.is_zero(), Error::<T>::EmptyValidity);
			let valid_from = valid_from.unwrap_or_else(frame_system::Pallet::<T>::block_number);
			let expires_at = valid_from.saturating_add(valid_for);
			T::AssetsHolder::hold(
				T::JamkbAssetId::get(),
				&HoldReason::Releasing.into(),
				&Self::custody_account(),
				amount,
			)?;
			let id = LastAllocationId::<T>::mutate(|last| {
				*last = last.saturating_add(1);
				*last
			});
			Allocations::<T>::insert(
				id,
				Allocation {
					id,
					mode: AllocationMode::Permanent { memo },
					target,
					amount,
					state: AllocationState::Approved,
					delivered_at: None,
					approver: None,
					valid_from,
					expires_at,
				},
			);
			Self::deposit_event(Event::ReleaseApproved { id, target, amount });
			Ok(())
		}

		/// Executes an approved release: sends a `Delivery` operation and moves the allocation
		/// to `Delivering`.
		///
		/// Origin: any signed account. Legal while the allocation is `Approved`, within its
		/// validity window and without an unsettled operation.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::execute_release())]
		pub fn execute_release(origin: OriginFor<T>, id: AllocationId) -> DispatchResult {
			ensure_signed(origin)?;
			Self::ensure_active()?;
			let mut allocation = Allocations::<T>::get(id).ok_or(Error::<T>::UnknownAllocation)?;
			ensure!(allocation.state == AllocationState::Approved, Error::<T>::WrongState);
			ensure!(PendingOperationOf::<T>::get(id).is_none(), Error::<T>::UnsettledOperation);
			let now = frame_system::Pallet::<T>::block_number();
			ensure!(now >= allocation.valid_from, Error::<T>::NotYetValid);
			ensure!(now < allocation.expires_at, Error::<T>::Expired);
			let AllocationMode::Permanent { memo } = allocation.mode;
			let op_id = Self::send_operation(
				OperationPayload::Delivery { allocation: id, amount: allocation.amount },
				allocation.target,
				allocation.amount,
				memo,
			)?;
			allocation.state = AllocationState::Delivering;
			Allocations::<T>::insert(id, allocation);
			PendingOperationOf::<T>::insert(id, op_id);
			Self::deposit_event(Event::OperationSubmitted { id: op_id, allocation: Some(id) });
			Ok(())
		}

		/// Moves `amount` of the caller's own units to `target`: holds the amount on the caller
		/// and sends a `Redemption` operation.
		///
		/// Origin: any signed account. `memo` goes to `target` unchanged. Rejected when `target`
		/// is the Parachain Service or `amount` is below `MinTransfer`.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::redeem())]
		pub fn redeem(
			origin: OriginFor<T>,
			amount: T::Balance,
			target: ServiceId,
			memo: Memo,
		) -> DispatchResult {
			let holder = ensure_signed(origin)?;
			Self::ensure_active()?;
			ensure!(target != T::ParachainService::get(), Error::<T>::TargetIsParachainService);
			ensure!(amount >= T::MinTransfer::get(), Error::<T>::BelowMinTransfer);
			T::AssetsHolder::hold(
				T::JamkbAssetId::get(),
				&HoldReason::Releasing.into(),
				&holder,
				amount,
			)?;
			let op_id = Self::send_operation(
				OperationPayload::Redemption { holder, target, amount, memo },
				target,
				amount,
				memo,
			)?;
			Self::deposit_event(Event::OperationSubmitted { id: op_id, allocation: None });
			Ok(())
		}

		/// Cancels an allocation in `Approved`: the hold is released and the allocation moves
		/// to `Closed`.
		///
		/// Origin: the allocation's approver, or any signed account from `expires_at` on.
		/// Governance is the approver of a permanent release.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::cancel_allocation())]
		pub fn cancel_allocation(origin: OriginFor<T>, id: AllocationId) -> DispatchResult {
			let signer = ensure_signed(origin.clone()).ok();
			let governance = T::GovernanceOrigin::try_origin(origin).is_ok();
			ensure!(governance || signer.is_some(), DispatchError::BadOrigin);
			Self::ensure_attested()?;
			let allocation = Allocations::<T>::get(id).ok_or(Error::<T>::UnknownAllocation)?;
			ensure!(allocation.state == AllocationState::Approved, Error::<T>::WrongState);
			let is_approver = match &allocation.approver {
				None => governance,
				Some(approver) => signer.as_ref() == Some(approver),
			};
			if !is_approver {
				ensure!(signer.is_some(), DispatchError::BadOrigin);
				let now = frame_system::Pallet::<T>::block_number();
				ensure!(now >= allocation.expires_at, Error::<T>::NotExpired);
			}
			T::AssetsHolder::release(
				T::JamkbAssetId::get(),
				&HoldReason::Releasing.into(),
				&Self::custody_account(),
				allocation.amount,
				Precision::Exact,
			)?;
			Allocations::<T>::mutate(id, |a| {
				if let Some(a) = a {
					a.state = AllocationState::Closed;
				}
			});
			Self::deposit_event(Event::AllocationCancelled { id });
			Ok(())
		}

		/// Settles a resolved operation at once, without waiting for `on_poll`. Its use is to
		/// retry a failed settlement.
		///
		/// Origin: any signed account. Rejected unless the operation is resolved.
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::settle())]
		pub fn settle(origin: OriginFor<T>, op_id: OperationId) -> DispatchResult {
			ensure_signed(origin)?;
			Self::ensure_attested()?;
			let operation = Operations::<T>::get(op_id).ok_or(Error::<T>::UnknownOperation)?;
			ensure!(operation.state != OperationState::Requested, Error::<T>::NotResolved);
			Self::apply_settlement(&operation)?;
			Operations::<T>::remove(op_id);
			Resolved::<T>::mutate(|list| list.retain(|id| *id != op_id));
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// The custody account: undistributed DAO units.
		pub fn custody_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// The released account: units that left Parachain Service control.
		pub fn released_account() -> T::AccountId {
			T::PalletId::get().into_sub_account_truncating(b"rlsd")
		}

		/// The custodial account: voluntary returns awaiting their claim.
		pub fn custodial_account() -> T::AccountId {
			T::PalletId::get().into_sub_account_truncating(b"cstd")
		}

		/// The excess account: unattributed inflows.
		pub fn excess_account() -> T::AccountId {
			T::PalletId::get().into_sub_account_truncating(b"xces")
		}

		/// The four pallet accounts.
		pub fn pallet_accounts() -> [T::AccountId; 4] {
			[
				Self::custody_account(),
				Self::released_account(),
				Self::custodial_account(),
				Self::excess_account(),
			]
		}

		/// The allocation with `id`, if any.
		pub fn allocation(id: AllocationId) -> Option<AllocationOf<T>> {
			Allocations::<T>::get(id)
		}

		/// The unsettled operation with `id`, if any.
		pub fn operation(id: OperationId) -> Option<OperationOf<T>> {
			Operations::<T>::get(id)
		}

		fn ensure_attested() -> DispatchResult {
			ensure!(Attestation::<T>::exists(), Error::<T>::NotAttested);
			Ok(())
		}

		fn ensure_active() -> DispatchResult {
			Self::ensure_attested()?;
			ensure!(!Paused::<T>::get(), Error::<T>::Paused);
			Ok(())
		}

		/// Queues one deferred `TransferOut` from the Parachain Service's regular balance to
		/// `dest`'s regular balance as a new operation, and records it as `Requested`. Nothing
		/// after the transport accepts the operation can fail.
		fn send_operation(
			payload: OperationPayload<T::AccountId, T::Balance>,
			dest: ServiceId,
			amount: T::Balance,
			memo: Memo,
		) -> Result<OperationId, DispatchError> {
			ensure!(
				Operations::<T>::count() < T::MaxUnsettledOperations::get(),
				Error::<T>::TooManyOperations
			);
			let message_id = T::MessageSender::next_message_id();
			let message = UpwardMessage::TransferOut {
				source: None,
				dest,
				amount,
				id: message_id,
				source_supervisor_balance: false,
				dest_supervisor_balance: false,
				deferred: Some((memo, T::MemoGas::get())),
			};
			let id = T::MessageSender::send(T::SenderId::get(), vec![message])
				.map_err(Error::<T>::from)?;
			Operations::<T>::insert(
				id,
				Operation {
					id,
					payload,
					state: OperationState::Requested,
					submitted_at: None,
					message_id,
				},
			);
			Ok(id)
		}

		/// Applies the outcome of a resolved operation.
		fn apply_settlement(operation: &OperationOf<T>) -> DispatchResult {
			match operation.state {
				OperationState::Confirmed(_) => Self::confirm(operation),
				OperationState::Failed(_) => Self::undo(operation),
				OperationState::Requested => Err(Error::<T>::NotResolved.into()),
			}
		}

		fn confirm(operation: &OperationOf<T>) -> DispatchResult {
			let asset = T::JamkbAssetId::get();
			let reason: T::RuntimeHoldReason = HoldReason::Releasing.into();
			match &operation.payload {
				OperationPayload::Delivery { allocation, amount } => {
					T::AssetsHolder::transfer_on_hold(
						asset,
						&reason,
						&Self::custody_account(),
						&Self::released_account(),
						*amount,
						Precision::Exact,
						Restriction::Free,
						Fortitude::Polite,
					)?;
					Allocations::<T>::mutate(allocation, |a| {
						if let Some(a) = a {
							a.state = AllocationState::Delivered;
							a.delivered_at = operation.submitted_at;
						}
					});
					PendingOperationOf::<T>::remove(allocation);
				},
				OperationPayload::Redemption { holder, amount, .. } => {
					T::AssetsHolder::transfer_on_hold(
						asset,
						&reason,
						holder,
						&Self::released_account(),
						*amount,
						Precision::Exact,
						Restriction::Free,
						Fortitude::Polite,
					)?;
				},
			}
			Ok(())
		}

		fn undo(operation: &OperationOf<T>) -> DispatchResult {
			match &operation.payload {
				OperationPayload::Delivery { allocation, .. } => {
					Allocations::<T>::mutate(allocation, |a| {
						if let Some(a) = a {
							a.state = AllocationState::Approved;
						}
					});
					PendingOperationOf::<T>::remove(allocation);
				},
				OperationPayload::Redemption { holder, amount, .. } => {
					T::AssetsHolder::release(
						T::JamkbAssetId::get(),
						&HoldReason::Releasing.into(),
						holder,
						*amount,
						Precision::Exact,
					)?;
				},
			}
			Ok(())
		}

		/// Settles `id` in its own storage layer. On failure the layer is rolled back and the
		/// failure is reported.
		fn try_settle(id: OperationId) {
			let Some(operation) = Operations::<T>::get(id) else {
				defensive!("a listed operation is missing");
				return;
			};
			match with_storage_layer(|| Self::apply_settlement(&operation)) {
				Ok(()) => Operations::<T>::remove(id),
				Err(error) => Self::deposit_event(Event::SettlementFailed { id, error }),
			}
		}

		/// Settles the resolved operations in send order, as long as `meter` has room. A
		/// settlement that fails leaves the list and waits for `settle`.
		fn settle_resolved(meter: &mut WeightMeter) {
			if meter.try_consume(T::DbWeight::get().reads(1)).is_err() {
				return;
			}
			let resolved = Resolved::<T>::get();
			if resolved.is_empty() {
				return;
			}
			if meter.try_consume(T::DbWeight::get().writes(1)).is_err() {
				return;
			}
			let each = T::WeightInfo::settle();
			let mut ids = resolved.into_inner().into_iter().peekable();
			while ids.peek().is_some() && meter.try_consume(each).is_ok() {
				let id = ids.next().expect("peeked above; qed");
				Self::try_settle(id);
			}
			// What is left is a tail of a bounded list, so it fits the bound.
			Resolved::<T>::put(BoundedVec::truncate_from(ids.collect::<Vec<_>>()));
		}

		/// Checks the invariants the pallet maintains. Fails when they do not hold.
		#[cfg(any(feature = "try-runtime", test))]
		pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			use alloc::collections::btree_map::BTreeMap;
			let asset = T::JamkbAssetId::get();
			let reason: T::RuntimeHoldReason = HoldReason::Releasing.into();

			if T::Assets::asset_exists(asset.clone()) {
				frame_support::ensure!(
					T::Assets::total_issuance(asset.clone()) == T::Cap::get(),
					"the supply is not the cap"
				);
			}

			// The custody hold backs exactly the approved and delivering allocations.
			let committed = Allocations::<T>::iter_values()
				.filter(|a| {
					matches!(a.state, AllocationState::Approved | AllocationState::Delivering)
				})
				.fold(T::Balance::zero(), |acc, a| acc.saturating_add(a.amount));
			frame_support::ensure!(
				T::AssetsHolder::balance_on_hold(asset.clone(), &reason, &Self::custody_account()) ==
					committed,
				"custody hold does not match the committed allocations"
			);

			// Every holder's hold backs exactly its unsettled redemptions.
			let mut redeeming: BTreeMap<T::AccountId, T::Balance> = BTreeMap::new();
			for op in Operations::<T>::iter_values() {
				if let OperationPayload::Redemption { holder, amount, .. } = op.payload {
					let sum = redeeming.entry(holder).or_insert_with(Zero::zero);
					*sum = sum.saturating_add(amount);
				}
			}
			for (holder, sum) in redeeming {
				frame_support::ensure!(
					T::AssetsHolder::balance_on_hold(asset.clone(), &reason, &holder) == sum,
					"a holder's hold does not match its redemptions"
				);
			}

			// A delivery operation and its delivering allocation point at each other.
			for (allocation, op_id) in PendingOperationOf::<T>::iter() {
				let op = Operations::<T>::get(op_id).ok_or("pending operation is missing")?;
				frame_support::ensure!(
					matches!(op.payload, OperationPayload::Delivery { allocation: a, .. } if a == allocation),
					"pending operation does not belong to the allocation"
				);
			}
			for (id, a) in Allocations::<T>::iter() {
				if a.state == AllocationState::Delivering {
					frame_support::ensure!(
						PendingOperationOf::<T>::contains_key(id),
						"delivering allocation has no pending operation"
					);
				}
			}
			for (id, op) in Operations::<T>::iter() {
				if let OperationPayload::Delivery { allocation, .. } = op.payload {
					frame_support::ensure!(
						PendingOperationOf::<T>::get(allocation) == Some(id),
						"delivery operation is not its allocation's pending operation"
					);
					frame_support::ensure!(
						Allocations::<T>::get(allocation).map(|a| a.state) ==
							Some(AllocationState::Delivering),
						"delivery operation's allocation is not delivering"
					);
				}
			}

			// Only resolved operations are listed, each once; a resolved one that is not listed
			// had its settlement fail. A requested one has no sending block yet.
			frame_support::ensure!(
				Operations::<T>::count() <= T::MaxUnsettledOperations::get(),
				"too many unsettled operations"
			);
			let mut listed: Vec<OperationId> = Resolved::<T>::get().into_inner();
			let listed_count = listed.len();
			listed.sort();
			listed.dedup();
			frame_support::ensure!(listed.len() == listed_count, "operation listed twice");
			for (id, op) in Operations::<T>::iter() {
				let requested = op.state == OperationState::Requested;
				frame_support::ensure!(
					requested == op.submitted_at.is_none(),
					"operation state and sending block disagree"
				);
				frame_support::ensure!(
					!requested || listed.binary_search(&id).is_err(),
					"a requested operation is listed"
				);
			}
			frame_support::ensure!(
				listed.iter().all(|id| Operations::<T>::contains_key(id)),
				"listed operation is missing"
			);
			Ok(())
		}
	}

	impl<T: Config> OnJamOutcome<BlockNumberFor<T>> for Pallet<T> {
		fn on_outcome(
			sender: SenderId,
			id: OperationId,
			sent_at: BlockNumberFor<T>,
			entries: &[AccumulateLog],
			meter: &mut WeightMeter,
		) {
			meter.consume(T::WeightInfo::on_outcome());
			if sender != T::SenderId::get() {
				return;
			}
			let resolved = Operations::<T>::mutate(id, |op| {
				let Some(op) = op else { return false };
				if op.state != OperationState::Requested {
					return false;
				}
				// Only an entry naming this operation's message counts (§5.1).
				let error = entries.iter().find_map(|entry| match entry {
					AccumulateLog::TransferFailed { id, error } if *id == op.message_id => {
						Some(FailureReason::Transfer(*error))
					},
					_ => None,
				});
				op.submitted_at = Some(sent_at);
				match error {
					Some(error) => {
						op.state = OperationState::Failed(error);
						Self::deposit_event(Event::OperationFailed { id, error });
					},
					None => {
						op.state = OperationState::Confirmed(None);
						Self::deposit_event(Event::OperationConfirmed { id, entry: None });
					},
				}
				true
			});
			if resolved && Resolved::<T>::mutate(|list| list.try_push(id)).is_err() {
				// Only if `MaxUnsettledOperations` was lowered below the unsettled operations.
				defensive!("resolved operations exceed their bound; settle it with `settle`");
			}
		}
	}

	impl<T: Config> OnIncomingTransfers<T::Balance> for Pallet<T> {
		/// Voluntary returns are not implemented: no bucket is taken, so every bucket stays on
		/// JAM until they are.
		fn capacity() -> u32 {
			0
		}

		fn on_incoming_transfers(
			_transfers: IncomingTransfers<T::Balance>,
			_meter: &mut WeightMeter,
		) -> Result<(), ()> {
			Err(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_poll(_n: BlockNumberFor<T>, meter: &mut WeightMeter) {
			let budget = T::SettlementWeight::get().min(meter.remaining());
			let mut own = WeightMeter::with_limit(budget);
			Self::settle_resolved(&mut own);
			meter.consume(own.consumed());
		}

		fn integrity_test() {
			assert!(!T::MinTransfer::get().is_zero(), "`MinTransfer` must be at least one unit");
			assert!(T::MaxUnsettledOperations::get() > 0, "`MaxUnsettledOperations` is zero");
			let first =
				T::DbWeight::get().reads_writes(1, 1).saturating_add(T::WeightInfo::settle());
			assert!(
				T::SettlementWeight::get().all_gte(first),
				"`SettlementWeight` must cover at least one settlement"
			);
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}
}
