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

//! The boundary to the AH→JAM transport.
//!
//! These traits and types are the interface the design expects from the generic AH→JAM transport,
//! an extension of `pallet-parachain-system` (design §3.4, §7.7). They live in this crate until
//! that pallet exists; they use only the JAM type aliases of the crate root and
//! [`crate::messages`], not the pallet itself.

use crate::{
	messages::{AccumulateLog, MessageId, UpwardMessage},
	Memo, ServiceId, Timeslot,
};
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::DispatchResult, weights::WeightMeter};
use scale_info::TypeInfo;

/// Identifies a sender; each sender has its own budget share.
pub type SenderId = u8;
/// One atomic batch of upward messages, allocated by parachain-system.
pub type OperationId = u64;

/// Why parachain-system refused to queue an operation.
#[derive(Clone, PartialEq, Eq, Debug)]
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

/// Charges the delivery fee of an operation's messages to the signed caller that records it.
/// The runtime implements it with the transport's fee factor, which grows with the queued
/// size; `()` charges nothing.
pub trait ChargeDeliveryFee<AccountId, Balance> {
	/// Charges `payer` for sending `messages`. On `Err` the call that records the operation
	/// fails.
	fn charge(payer: &AccountId, messages: &[UpwardMessage<Balance>]) -> DispatchResult;
}

impl<AccountId, Balance> ChargeDeliveryFee<AccountId, Balance> for () {
	fn charge(_: &AccountId, _: &[UpwardMessage<Balance>]) -> DispatchResult {
		Ok(())
	}
}

/// Implemented by each sender. Called by parachain-system.
pub trait OnJamOutcome {
	/// Operation `id` of `sender` was accumulated. Once per operation, in send order.
	/// `accumulated_at` is the timeslot it was accumulated at; `entries` are its
	/// `parachain_log` entries, at most one per message. A sender gets only the outcomes of
	/// its own operations. It cannot fail and charges what it writes to `meter`.
	fn on_outcome(
		sender: SenderId,
		id: OperationId,
		accumulated_at: Timeslot,
		entries: &[AccumulateLog],
		meter: &mut WeightMeter,
	);
}

/// One transfer the Parachain Service received (Parachain Service design §5.1).
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo,
)]
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
	/// is read. 0 until attested and while paused.
	fn capacity() -> u32;
	/// One whole bucket, in queue order; at most `capacity()` buckets per block. The pallet
	/// books the bucket in the call, all or nothing, and charges it to `meter`. `Ok`: the
	/// bucket is cleaned up and not delivered again. `Err`: `released` cannot cover the
	/// bucket, or its `Move` cannot be recorded; it is not booked. Parachain-system rolls the
	/// call back, delivers no more buckets this block, cleans up only the ones before it, and
	/// delivers it again in a later block.
	fn on_incoming_transfers(
		transfers: IncomingTransfers<Balance>,
		meter: &mut WeightMeter,
	) -> Result<(), ()>;
}
