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

#![doc = include_str!("preimage.md")]

pub mod chunks;
pub mod legacy_request_status;
pub mod request_status;

pub use chunks::{PreimageChunkMigrator, RcPreimageChunk, CHUNK_SIZE};
pub use legacy_request_status::{PreimageLegacyRequestStatusMigrator, RcPreimageLegacyStatusOf};
pub use request_status::{PreimageRequestStatusMigrator, RcPreimageRequestStatusOf};

use crate::*;

pub mod alias {
	use super::*;

	use frame_support::{traits::Currency, Identity};
	use sp_core::ConstU32;

	pub const MAX_SIZE: u32 = 4 * 1024 * 1024;

	/// A type to note whether a preimage is owned by a user or the system.
	// Copied from https://github.com/paritytech/polkadot-sdk/blob/00946b10ab18331f959f5cbced7c433b6132b1cb/substrate/frame/preimage/src/lib.rs#L67-L77
	#[derive(Clone, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen, RuntimeDebug)]
	#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
	pub enum OldRequestStatus<AccountId, Balance> {
		/// The associated preimage has not yet been requested by the system. The given deposit (if
		/// some) is being held until either it becomes requested or the user retracts the
		/// preimage.
		Unrequested { deposit: (AccountId, Balance), len: u32 },
		/// There are a non-zero number of outstanding requests for this hash by this chain. If
		/// there is a preimage registered, then `len` is `Some` and it may be removed iff this
		/// counter becomes zero.
		Requested { deposit: Option<(AccountId, Balance)>, count: u32, len: Option<u32> },
	}

	/// A type to note whether a preimage is owned by a user or the system.
	// Coped from https://github.com/paritytech/polkadot-sdk/blob/00946b10ab18331f959f5cbced7c433b6132b1cb/substrate/frame/preimage/src/lib.rs#L79-L89
	#[derive(Clone, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen, RuntimeDebug)]
	#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
	pub enum RequestStatus<AccountId, Ticket> {
		/// The associated preimage has not yet been requested by the system. The given deposit (if
		/// some) is being held until either it becomes requested or the user retracts the
		/// preimage.
		Unrequested { ticket: (AccountId, Ticket), len: u32 },
		/// There are a non-zero number of outstanding requests for this hash by this chain. If
		/// there is a preimage registered, then `len` is `Some` and it may be removed iff this
		/// counter becomes zero.
		Requested { maybe_ticket: Option<(AccountId, Ticket)>, count: u32, maybe_len: Option<u32> },
	}

	// Coped from https://github.com/paritytech/polkadot-sdk/blob/00946b10ab18331f959f5cbced7c433b6132b1cb/substrate/frame/preimage/src/lib.rs#L91-L93
	pub type BalanceOf<T> = <<T as pallet_preimage::Config>::Currency as Currency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;
	pub type TicketOf<T> = <T as pallet_preimage::Config>::Consideration;

	// Coped from https://github.com/paritytech/polkadot-sdk/blob/00946b10ab18331f959f5cbced7c433b6132b1cb/substrate/frame/preimage/src/lib.rs#L173-L185
	#[frame_support::storage_alias(pallet_name)]
	pub type StatusFor<T: pallet_preimage::Config> = StorageMap<
		pallet_preimage::Pallet<T>,
		Identity,
		H256,
		OldRequestStatus<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
	>;

	#[frame_support::storage_alias(pallet_name)]
	pub type RequestStatusFor<T: pallet_preimage::Config> = StorageMap<
		pallet_preimage::Pallet<T>,
		Identity,
		H256,
		RequestStatus<<T as frame_system::Config>::AccountId, TicketOf<T>>,
	>;

	#[frame_support::storage_alias(pallet_name)]
	pub type PreimageFor<T: pallet_preimage::Config> = StorageMap<
		pallet_preimage::Pallet<T>,
		Identity,
		(H256, u32),
		BoundedVec<u8, ConstU32<MAX_SIZE>>,
	>;
}
