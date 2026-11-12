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

//! Minimal mirror of the Parachain Service messages and log entries the pallet uses.
//!
//! The authoritative definitions live in the Parachain Service design. Only the fields the
//! pallet reads or writes are carried here.

use crate::{Memo, ServiceId};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// The id a `TransferOut` carries, echoed by the Parachain Service in `TransferFailed`.
pub type MessageId = u64;

/// An upward message from Asset Hub to the Parachain Service.
#[derive(Clone, Encode, Decode, PartialEq, Eq, Debug, TypeInfo)]
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
