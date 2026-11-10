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

//! The types of `pallet-jamkb`: the JAM primitives it names, its allocation and operation
//! records, and the enums its calls and events carry.

use crate::{
	messages::{MessageId, TransferError},
	transport::{IncomingTransfer, OperationId},
	Config,
};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::{BoundedVec, Get},
	CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::TypeInfo;
use sp_core::H256;

/// A JAM service index.
pub type ServiceId = u32;
/// Identifier of an allocation, assigned from 1.
pub type AllocationId = u64;
/// A JAM timeslot. All times and durations of the pallet are in timeslots.
pub type Timeslot = u32;
/// A JAM transfer memo: 128 octets, as the Gray Paper fixes it.
pub type Memo = [u8; 128];
/// The header hash of the JAM block whose state the balances were checked against. The pallet
/// records it and does not verify it.
pub type Anchor = H256;

/// Whether an allocation is a lease or a permanent release. Index 0 is reserved for `Lease`.
#[derive(Clone, Encode, Decode, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo)]
pub enum AllocationMode {
	/// A permanent release to the target's regular balance.
	#[codec(index = 1)]
	Permanent {
		/// The memo the delivery carries, as supplied at `approve_release`.
		memo: Memo,
	},
}

/// Lifecycle of an allocation. Index 3 is reserved for `Reclaiming`.
#[derive(Clone, Encode, Decode, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo)]
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
#[derive(Clone, Encode, Decode, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo)]
pub struct Allocation<AccountId, Balance> {
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
	/// The timeslot the delivery was accumulated at.
	pub delivered_at: Option<Timeslot>,
	/// Set by a Failed target reclaim of a lease; always `None` for a permanent release.
	pub target_reclaim_from: Option<Timeslot>,
	/// The signing account or contract that approved it; `None`: governance is the approver.
	pub approver: Option<AccountId>,
	/// `execute_release` is rejected before this timeslot.
	pub valid_from: Timeslot,
	/// `execute_release` is rejected from this timeslot on.
	pub expires_at: Timeslot,
}

/// The data payload of an operation. Indices 1 and 3 to 9 are reserved for the lease and
/// recovery operations.
#[derive(
	CloneNoBound, PartialEqNoBound, EqNoBound, DebugNoBound, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
#[scale_info(skip_type_params(MaxTransfers))]
#[codec(mel_bound(AccountId: MaxEncodedLen, Balance: MaxEncodedLen))]
pub enum OperationPayload<
	AccountId: Clone + PartialEq + Eq + core::fmt::Debug,
	Balance: Clone + PartialEq + Eq + core::fmt::Debug,
	MaxTransfers: Get<u32>,
> {
	/// A permanent release: one deferred `TransferOut` from the Parachain Service's regular
	/// balance to the target's regular balance, with the allocation's memo.
	///
	/// Confirmed: the allocation moves to `Delivered`, `delivered_at` is set to the operation's
	/// `accumulated_at`, and the held units move from custody to the `released` account. Failed:
	/// the allocation returns to `Approved`.
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
	/// A move of a bucket's transfers from the Parachain Service's supervisor balance to its
	/// regular balance: one plain `TransferOut` of their sum between the two.
	///
	/// Confirmed: each transfer is booked. Failed: nothing is booked; the units stay in the
	/// `released` account, as they stay on the supervisor balance.
	#[codec(index = 10)]
	Move {
		/// The transfers that landed on the supervisor balance, in queue order.
		transfers: BoundedVec<IncomingTransfer<Balance>, MaxTransfers>,
	},
	/// A refund of an incoming transfer to its source (`dispose_excess`): one deferred
	/// `TransferOut` from the Parachain Service's regular balance to `source`'s regular
	/// balance, with governance's memo.
	///
	/// Confirmed: the held units move from the `excess` account to the `released` account.
	/// Failed: the hold is released and `Excess[source]` restored.
	#[codec(index = 11)]
	Refund {
		/// The refunded service.
		source: ServiceId,
		/// The units locked for the transfer.
		amount: Balance,
		/// The memo the transfer carries.
		memo: Memo,
	},
}

/// The success entry of an operation that has one, cut to what the settlement reads. None of
/// the operations of this prototype has one.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo,
)]
pub enum SuccessEntry<Balance> {
	/// `ServiceEjected`, for an ejection.
	ServiceEjected {
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
#[derive(Clone, Encode, Decode, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo)]
pub enum OperationState<Balance> {
	/// Recorded and queued.
	Requested,
	/// Resolved to success; not settled yet.
	Confirmed(Option<SuccessEntry<Balance>>),
	/// Resolved to failure; not settled yet.
	Failed(FailureReason),
}

/// The failure carried by `OperationFailed`: one variant per failure entry class. The variants
/// of the recovery operations are not implemented.
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
	/// A `TransferFailed { id, error }` entry.
	TransferFailed(TransferError),
}

/// One cross-system operation between Asset Hub and JAM.
#[derive(
	CloneNoBound, PartialEqNoBound, EqNoBound, DebugNoBound, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
#[scale_info(skip_type_params(MaxTransfers))]
#[codec(mel_bound(AccountId: MaxEncodedLen, Balance: MaxEncodedLen))]
pub struct Operation<
	AccountId: Clone + PartialEq + Eq + core::fmt::Debug,
	Balance: Clone + PartialEq + Eq + core::fmt::Debug,
	MaxTransfers: Get<u32>,
> {
	/// The id parachain-system assigned.
	pub id: OperationId,
	/// What the operation does.
	pub payload: OperationPayload<AccountId, Balance, MaxTransfers>,
	/// Lifecycle state.
	pub state: OperationState<Balance>,
	/// The timeslot it was accumulated at; unset until reported.
	pub accumulated_at: Option<Timeslot>,
	/// The service its payload names: the allocation's target for a `Delivery`, the Parachain
	/// Service for a `Move`. The operations of one target settle in send order.
	pub target: ServiceId,
	/// The id of the operation's `TransferOut`, which a `TransferFailed` echoes.
	pub message_id: MessageId,
}

/// The booking class of an uncredited return (design §4.4).
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub enum ReturnClass<AccountId> {
	/// The owner is known; reserved for this beneficiary.
	Unclaimed(AccountId),
	/// The owner is unknown; disposed by governance.
	Excess,
}

/// A pallet account in a `force_transfer`.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub enum PalletAccount<AccountId> {
	/// The `custody` account.
	Custody,
	/// The `released` account.
	Released,
	/// The `unclaimed` account, as `Unclaimed[account]`.
	Unclaimed(AccountId),
	/// The `excess` account, as `Excess[source]`.
	Excess(ServiceId),
}

/// Convenience alias for the allocation record.
pub type AllocationOf<T> =
	Allocation<<T as frame_system::Config>::AccountId, <T as Config>::Balance>;
/// Convenience alias for an operation payload.
pub type OperationPayloadOf<T> = OperationPayload<
	<T as frame_system::Config>::AccountId,
	<T as Config>::Balance,
	<T as Config>::MaxTransfersPerBucket,
>;
/// Convenience alias for the operation record.
pub type OperationOf<T> = Operation<
	<T as frame_system::Config>::AccountId,
	<T as Config>::Balance,
	<T as Config>::MaxTransfersPerBucket,
>;
