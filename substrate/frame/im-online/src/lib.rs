// This file is part of Substrate.

// Copyright (C) 2019-2020 Parity Technologies (UK) Ltd.
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

//! # I'm online Module
//!
//! If the local node is a validator (i.e. contains an authority key), this module
//! gossips a heartbeat transaction with each new session. The heartbeat functions
//! as a simple mechanism to signal that the node is online in the current era.
//!
//! Received heartbeats are tracked for one era and reset with each new era. The
//! module exposes two public functions to query if a heartbeat has been received
//! in the current era or session.
//!
//! The heartbeat is a signed transaction, which was signed using the session key
//! and includes the recent best block number of the local validators chain as well
//! as the [NetworkState](../../client/offchain/struct.NetworkState.html).
//! It is submitted as an Unsigned Transaction via off-chain workers.
//!
//! - [`im_online::Trait`](./trait.Trait.html)
//! - [`Call`](./enum.Call.html)
//! - [`Module`](./struct.Module.html)
//!
//! ## Interface
//!
//! ### Public Functions
//!
//! - `is_online` - True if the validator sent a heartbeat in the current session.
//!
//! ## Usage
//!
//! ```
//! use frame_support::{decl_module, dispatch};
//! use frame_system::{self as system, ensure_signed};
//! use pallet_im_online::{self as im_online};
//!
//! pub trait Trait: im_online::Trait {}
//!
//! decl_module! {
//! 	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
//! 		#[weight = 0]
//! 		pub fn is_online(origin, authority_index: u32) -> dispatch::DispatchResult {
//! 			let _sender = ensure_signed(origin)?;
//! 			let _is_online = <im_online::Module<T>>::is_online(authority_index);
//! 			Ok(())
//! 		}
//! 	}
//! }
//! # fn main() { }
//! ```
//!
//! ## Dependencies
//!
//! This module depends on the [Session module](../pallet_session/index.html).

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

mod mock;
mod tests;
mod benchmarking;

use sp_application_crypto::RuntimeAppPublic;
use codec::{Encode, Decode};
use sp_core::offchain::OpaqueNetworkState;
use sp_std::prelude::*;
use sp_std::convert::TryInto;
use pallet_session::historical::IdentificationTuple;
use sp_runtime::{
	offchain::storage::StorageValueRef,
	RuntimeDebug,
	traits::{Convert, Member, Saturating, AtLeast32BitUnsigned}, Perbill,
	transaction_validity::{
		TransactionValidity, ValidTransaction, InvalidTransaction, TransactionSource,
		TransactionPriority,
	},
};
use sp_staking::{
	SessionIndex,
	offence::{ReportOffence, Offence, Kind},
};
use frame_support::{
	decl_module, decl_event, decl_storage, Parameter, debug, decl_error,
	traits::Get,
	weights::Weight,
};
use frame_system::{self as system, ensure_none};
use frame_system::offchain::{
	SendTransactionTypes,
	SubmitTransaction,
};

pub mod sr25519 {
	mod app_sr25519 {
		use sp_application_crypto::{app_crypto, key_types::IM_ONLINE, sr25519};
		app_crypto!(sr25519, IM_ONLINE);
	}

	sp_application_crypto::with_pair! {
		/// An i'm online keypair using sr25519 as its crypto.
		pub type AuthorityPair = app_sr25519::Pair;
	}

	/// An i'm online signature using sr25519 as its crypto.
	pub type AuthoritySignature = app_sr25519::Signature;

	/// An i'm online identifier using sr25519 as its crypto.
	pub type AuthorityId = app_sr25519::Public;
}

pub mod ed25519 {
	mod app_ed25519 {
		use sp_application_crypto::{app_crypto, key_types::IM_ONLINE, ed25519};
		app_crypto!(ed25519, IM_ONLINE);
	}

	sp_application_crypto::with_pair! {
		/// An i'm online keypair using ed25519 as its crypto.
		pub type AuthorityPair = app_ed25519::Pair;
	}

	/// An i'm online signature using ed25519 as its crypto.
	pub type AuthoritySignature = app_ed25519::Signature;

	/// An i'm online identifier using ed25519 as its crypto.
	pub type AuthorityId = app_ed25519::Public;
}

const DB_PREFIX: &[u8] = b"parity/im-online-heartbeat/";
/// How many blocks do we wait for heartbeat transaction to be included
/// before sending another one.
const INCLUDE_THRESHOLD: u32 = 3;

/// Status of the offchain worker code.
///
/// This stores the block number at which heartbeat was requested and when the worker
/// has actually managed to produce it.
/// Note we store such status for every `authority_index` separately.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
struct HeartbeatStatus<BlockNumber> {
	/// An index of the session that we are supposed to send heartbeat for.
	pub session_index: SessionIndex,
	/// A block number at which the heartbeat for that session has been actually sent.
	///
	/// It may be 0 in case the sending failed. In such case we should just retry
	/// as soon as possible (i.e. in a worker running for the next block).
	pub sent_at: BlockNumber,
}

impl<BlockNumber: PartialEq + AtLeast32BitUnsigned + Copy> HeartbeatStatus<BlockNumber> {
	/// Returns true if heartbeat has been recently sent.
	///
	/// Parameters:
	/// `session_index` - index of current session.
	/// `now` - block at which the offchain worker is running.
	///
	/// This function will return `true` iff:
	/// 1. the session index is the same (we don't care if it went up or down)
	/// 2. the heartbeat has been sent recently (within the threshold)
	///
	/// The reasoning for 1. is that it's better to send an extra heartbeat than
	/// to stall or not send one in case of a bug.
	fn is_recent(&self, session_index: SessionIndex, now: BlockNumber) -> bool {
		self.session_index == session_index && self.sent_at + INCLUDE_THRESHOLD.into() > now
	}
}

/// Error which may occur while executing the off-chain code.
#[cfg_attr(test, derive(PartialEq))]
enum OffchainErr<BlockNumber> {
	TooEarly(BlockNumber),
	WaitingForInclusion(BlockNumber),
	AlreadyOnline(u32),
	FailedSigning,
	FailedToAcquireLock,
	NetworkState,
	SubmitTransaction,
}

impl<BlockNumber: sp_std::fmt::Debug> sp_std::fmt::Debug for OffchainErr<BlockNumber> {
	fn fmt(&self, fmt: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		match *self {
			OffchainErr::TooEarly(ref block) =>
				write!(fmt, "Too early to send heartbeat, next expected at {:?}", block),
			OffchainErr::WaitingForInclusion(ref block) =>
				write!(fmt, "Heartbeat already sent at {:?}. Waiting for inclusion.", block),
			OffchainErr::AlreadyOnline(auth_idx) =>
				write!(fmt, "Authority {} is already online", auth_idx),
			OffchainErr::FailedSigning => write!(fmt, "Failed to sign heartbeat"),
			OffchainErr::FailedToAcquireLock => write!(fmt, "Failed to acquire lock"),
			OffchainErr::NetworkState => write!(fmt, "Failed to fetch network state"),
			OffchainErr::SubmitTransaction => write!(fmt, "Failed to submit transaction"),
		}
	}
}

pub type AuthIndex = u32;

/// Heartbeat which is sent/received.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Heartbeat<BlockNumber>
	where BlockNumber: PartialEq + Eq + Decode + Encode,
{
	/// Block number at the time heartbeat is created..
	pub block_number: BlockNumber,
	/// A state of local network (peer id and external addresses)
	pub network_state: OpaqueNetworkState,
	/// Index of the current session.
	pub session_index: SessionIndex,
	/// An index of the authority on the list of validators.
	pub authority_index: AuthIndex,
	/// The length of session validator set
	pub validators_len: u32,
}

pub trait Trait: SendTransactionTypes<Call<Self>> + pallet_session::historical::Trait {
	/// The identifier type for an authority.
	type AuthorityId: Member + Parameter + RuntimeAppPublic + Default + Ord;

	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

	/// An expected duration of the session.
	///
	/// This parameter is used to determine the longevity of `heartbeat` transaction
	/// and a rough time when we should start considering sending heartbeats,
	/// since the workers avoids sending them at the very beginning of the session, assuming
	/// there is a chance the authority will produce a block and they won't be necessary.
	type SessionDuration: Get<Self::BlockNumber>;

	/// A type that gives us the ability to submit unresponsiveness offence reports.
	type ReportUnresponsiveness:
		ReportOffence<
			Self::AccountId,
			IdentificationTuple<Self>,
			UnresponsivenessOffence<IdentificationTuple<Self>>,
		>;

	/// A configuration for base priority of unsigned transactions.
	///
	/// This is exposed so that it can be tuned for particular runtime, when
	/// multiple pallets send unsigned transactions.
	type UnsignedPriority: Get<TransactionPriority>;
}

decl_event!(
	pub enum Event<T> where
		<T as Trait>::AuthorityId,
		IdentificationTuple = IdentificationTuple<T>,
	{
		/// A new heartbeat was received from `AuthorityId`
		HeartbeatReceived(AuthorityId),
		/// At the end of the session, no offence was committed.
		AllGood,
		/// At the end of the session, at least one validator was found to be offline.
		SomeOffline(Vec<IdentificationTuple>),
	}
);

decl_storage! {
	trait Store for Module<T: Trait> as ImOnline {
		/// The block number after which it's ok to send heartbeats in current session.
		///
		/// At the beginning of each session we set this to a value that should
		/// fall roughly in the middle of the session duration.
		/// The idea is to first wait for the validators to produce a block
		/// in the current session, so that the heartbeat later on will not be necessary.
		HeartbeatAfter get(fn heartbeat_after): T::BlockNumber;

		/// The current set of keys that may issue a heartbeat.
		Keys get(fn keys): Vec<T::AuthorityId>;

		/// For each session index, we keep a mapping of `AuthIndex` to
		/// `offchain::OpaqueNetworkState`.
		ReceivedHeartbeats get(fn received_heartbeats):
			double_map hasher(twox_64_concat) SessionIndex, hasher(twox_64_concat) AuthIndex
			=> Option<Vec<u8>>;

		/// For each session index, we keep a mapping of `T::ValidatorId` to the
		/// number of blocks authored by the given authority.
		AuthoredBlocks get(fn authored_blocks):
			double_map hasher(twox_64_concat) SessionIndex, hasher(twox_64_concat) T::ValidatorId
			=> u32;
	}
	add_extra_genesis {
		config(keys): Vec<T::AuthorityId>;
		build(|config| Module::<T>::initialize_keys(&config.keys))
	}
}

decl_error! {
	/// Error for the im-online module.
	pub enum Error for Module<T: Trait> {
		/// Non existent public key.
		InvalidKey,
		/// Duplicated heartbeat.
		DuplicatedHeartbeat,
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		type Error = Error<T>;

		fn deposit_event() = default;

		/// # <weight>
		/// - Complexity: `O(K + E)` where K is length of `Keys` and E is length of
		///   `Heartbeat.network_state.external_address`
		///
		///   - `O(K)`: decoding of length `K`
		///   - `O(E)`: decoding/encoding of length `E`
		/// - DbReads: pallet_session `Validators`, pallet_session `CurrentIndex`, `Keys`,
		///   `ReceivedHeartbeats`
		/// - DbWrites: `ReceivedHeartbeats`
		/// # </weight>
		// NOTE: the weight include cost of validate_unsigned as it is part of the cost to import
		// block with such an extrinsic.
		#[weight = (310_000_000 + T::DbWeight::get().reads_writes(4, 1))
			.saturating_add(750_000.saturating_mul(heartbeat.validators_len as Weight))
			.saturating_add(
				1_200_000.saturating_mul(heartbeat.network_state.external_addresses.len() as Weight)
			)
		]
		fn heartbeat(
			origin,
			heartbeat: Heartbeat<T::BlockNumber>,
			// since signature verification is done in `validate_unsigned`
			// we can skip doing it here again.
			_signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
		) {
			ensure_none(origin)?;

			let current_session = <pallet_session::Module<T>>::current_index();
			let exists = <ReceivedHeartbeats>::contains_key(
				&current_session,
				&heartbeat.authority_index
			);
			let keys = Keys::<T>::get();
			let public = keys.get(heartbeat.authority_index as usize);
			if let (false, Some(public)) = (exists, public) {
				Self::deposit_event(Event::<T>::HeartbeatReceived(public.clone()));

				let network_state = heartbeat.network_state.encode();
				<ReceivedHeartbeats>::insert(
					&current_session,
					&heartbeat.authority_index,
					&network_state
				);
			} else if exists {
				Err(Error::<T>::DuplicatedHeartbeat)?
			} else {
				Err(Error::<T>::InvalidKey)?
			}
		}

		// Runs after every block.
		fn offchain_worker(now: T::BlockNumber) {
			// Only send messages if we are a potential validator.
			if sp_io::offchain::is_validator() {
				for res in Self::send_heartbeats(now).into_iter().flatten() {
					if let Err(e) = res {
						debug::debug!(
							target: "imonline",
							"Skipping heartbeat at {:?}: {:?}",
							now,
							e,
						)
					}
				}
			} else {
				debug::trace!(
					target: "imonline",
					"Skipping heartbeat at {:?}. Not a validator.",
					now,
				)
			}
		}
	}
}

type OffchainResult<T, A> = Result<A, OffchainErr<<T as frame_system::Trait>::BlockNumber>>;

/// Keep track of number of authored blocks per authority, uncles are counted as
/// well since they're a valid proof of being online.
impl<T: Trait + pallet_authorship::Trait> pallet_authorship::EventHandler<T::ValidatorId, T::BlockNumber> for Module<T> {
	fn note_author(author: T::ValidatorId) {
		Self::note_authorship(author);
	}

	fn note_uncle(author: T::ValidatorId, _age: T::BlockNumber) {
		Self::note_authorship(author);
	}
}

impl<T: Trait> Module<T> {
	/// Returns `true` if a heartbeat has been received for the authority at
	/// `authority_index` in the authorities series or if the authority has
	/// authored at least one block, during the current session. Otherwise
	/// `false`.
	pub fn is_online(authority_index: AuthIndex) -> bool {
		let current_validators = <pallet_session::Module<T>>::validators();

		if authority_index >= current_validators.len() as u32 {
			return false;
		}

		let authority = &current_validators[authority_index as usize];

		Self::is_online_aux(authority_index, authority)
	}

	fn is_online_aux(authority_index: AuthIndex, authority: &T::ValidatorId) -> bool {
		let current_session = <pallet_session::Module<T>>::current_index();

		<ReceivedHeartbeats>::contains_key(&current_session, &authority_index) ||
			<AuthoredBlocks<T>>::get(
				&current_session,
				authority,
			) != 0
	}

	/// Returns `true` if a heartbeat has been received for the authority at `authority_index` in
	/// the authorities series, during the current session. Otherwise `false`.
	pub fn received_heartbeat_in_current_session(authority_index: AuthIndex) -> bool {
		let current_session = <pallet_session::Module<T>>::current_index();
		<ReceivedHeartbeats>::contains_key(&current_session, &authority_index)
	}

	/// Note that the given authority has authored a block in the current session.
	fn note_authorship(author: T::ValidatorId) {
		let current_session = <pallet_session::Module<T>>::current_index();

		<AuthoredBlocks<T>>::mutate(
			&current_session,
			author,
			|authored| *authored += 1,
		);
	}

	pub(crate) fn send_heartbeats(block_number: T::BlockNumber)
		-> OffchainResult<T, impl Iterator<Item=OffchainResult<T, ()>>>
	{
		let heartbeat_after = <HeartbeatAfter<T>>::get();
		if block_number < heartbeat_after {
			return Err(OffchainErr::TooEarly(heartbeat_after))
		}

		let session_index = <pallet_session::Module<T>>::current_index();
		let validators_len = <pallet_session::Module<T>>::validators().len() as u32;

		Ok(Self::local_authority_keys()
			.map(move |(authority_index, key)|
				Self::send_single_heartbeat(
					authority_index,
					key,
					session_index,
					block_number,
					validators_len,
				)
			))
	}


	fn send_single_heartbeat(
		authority_index: u32,
		key: T::AuthorityId,
		session_index: SessionIndex,
		block_number: T::BlockNumber,
		validators_len: u32,
	) -> OffchainResult<T, ()> {
		// A helper function to prepare heartbeat call.
		let prepare_heartbeat = || -> OffchainResult<T, Call<T>> {
			let network_state = sp_io::offchain::network_state()
				.map_err(|_| OffchainErr::NetworkState)?;
			let heartbeat_data = Heartbeat {
				block_number,
				network_state,
				session_index,
				authority_index,
				validators_len,
			};

			let signature = key.sign(&heartbeat_data.encode()).ok_or(OffchainErr::FailedSigning)?;

			Ok(Call::heartbeat(heartbeat_data, signature))
		};

		if Self::is_online(authority_index) {
			return Err(OffchainErr::AlreadyOnline(authority_index));
		}

		// acquire lock for that authority at current heartbeat to make sure we don't
		// send concurrent heartbeats.
		Self::with_heartbeat_lock(
			authority_index,
			session_index,
			block_number,
			|| {
				let call = prepare_heartbeat()?;
				debug::info!(
					target: "imonline",
					"[index: {:?}] Reporting im-online at block: {:?} (session: {:?}): {:?}",
					authority_index,
					block_number,
					session_index,
					call,
				);

				SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
					.map_err(|_| OffchainErr::SubmitTransaction)?;

				Ok(())
			},
		)
	}

	fn local_authority_keys() -> impl Iterator<Item=(u32, T::AuthorityId)> {
		// on-chain storage
		//
		// At index `idx`:
		// 1. A (ImOnline) public key to be used by a validator at index `idx` to send im-online
		//          heartbeats.
		let authorities = Keys::<T>::get();

		// local keystore
		//
		// All `ImOnline` public (+private) keys currently in the local keystore.
		let mut local_keys = T::AuthorityId::all();

		local_keys.sort();

		authorities.into_iter()
			.enumerate()
			.filter_map(move |(index, authority)| {
				local_keys.binary_search(&authority)
					.ok()
					.map(|location| (index as u32, local_keys[location].clone()))
			})
	}

	fn with_heartbeat_lock<R>(
		authority_index: u32,
		session_index: SessionIndex,
		now: T::BlockNumber,
		f: impl FnOnce() -> OffchainResult<T, R>,
	) -> OffchainResult<T, R> {
		let key = {
			let mut key = DB_PREFIX.to_vec();
			key.extend(authority_index.encode());
			key
		};
		let storage = StorageValueRef::persistent(&key);
		let res = storage.mutate(|status: Option<Option<HeartbeatStatus<T::BlockNumber>>>| {
			// Check if there is already a lock for that particular block.
			// This means that the heartbeat has already been sent, and we are just waiting
			// for it to be included. However if it doesn't get included for INCLUDE_THRESHOLD
			// we will re-send it.
			match status {
				// we are still waiting for inclusion.
				Some(Some(status)) if status.is_recent(session_index, now) => {
					Err(OffchainErr::WaitingForInclusion(status.sent_at))
				},
				// attempt to set new status
				_ => Ok(HeartbeatStatus {
					session_index,
					sent_at: now,
				}),
			}
		})?;

		let mut new_status = res.map_err(|_| OffchainErr::FailedToAcquireLock)?;

		// we got the lock, let's try to send the heartbeat.
		let res = f();

		// clear the lock in case we have failed to send transaction.
		if res.is_err() {
			new_status.sent_at = 0.into();
			storage.set(&new_status);
		}

		res
	}

	fn initialize_keys(keys: &[T::AuthorityId]) {
		if !keys.is_empty() {
			assert!(Keys::<T>::get().is_empty(), "Keys are already initialized!");
			Keys::<T>::put(keys);
		}
	}

	#[cfg(test)]
	fn set_keys(keys: Vec<T::AuthorityId>) {
		Keys::<T>::put(&keys)
	}
}

impl<T: Trait> sp_runtime::BoundToRuntimeAppPublic for Module<T> {
	type Public = T::AuthorityId;
}

impl<T: Trait> pallet_session::OneSessionHandler<T::AccountId> for Module<T> {
	type Key = T::AuthorityId;

	fn on_genesis_session<'a, I: 'a>(validators: I)
		where I: Iterator<Item=(&'a T::AccountId, T::AuthorityId)>
	{
		let keys = validators.map(|x| x.1).collect::<Vec<_>>();
		Self::initialize_keys(&keys);
	}

	fn on_new_session<'a, I: 'a>(_changed: bool, validators: I, _queued_validators: I)
		where I: Iterator<Item=(&'a T::AccountId, T::AuthorityId)>
	{
		// Tell the offchain worker to start making the next session's heartbeats.
		// Since we consider producing blocks as being online,
		// the heartbeat is deferred a bit to prevent spamming.
		let block_number = <frame_system::Module<T>>::block_number();
		let half_session = T::SessionDuration::get() / 2.into();
		<HeartbeatAfter<T>>::put(block_number + half_session);

		// Remember who the authorities are for the new session.
		Keys::<T>::put(validators.map(|x| x.1).collect::<Vec<_>>());
	}

	fn on_before_session_ending() {
		let session_index = <pallet_session::Module<T>>::current_index();
		let keys = Keys::<T>::get();
		let current_validators = <pallet_session::Module<T>>::validators();

		let offenders = current_validators.into_iter().enumerate()
			.filter(|(index, id)|
				!Self::is_online_aux(*index as u32, id)
			).filter_map(|(_, id)|
				T::FullIdentificationOf::convert(id.clone()).map(|full_id| (id, full_id))
			).collect::<Vec<IdentificationTuple<T>>>();

		// Remove all received heartbeats and number of authored blocks from the
		// current session, they have already been processed and won't be needed
		// anymore.
		<ReceivedHeartbeats>::remove_prefix(&<pallet_session::Module<T>>::current_index());
		<AuthoredBlocks<T>>::remove_prefix(&<pallet_session::Module<T>>::current_index());

		if offenders.is_empty() {
			Self::deposit_event(RawEvent::AllGood);
		} else {
			Self::deposit_event(RawEvent::SomeOffline(offenders.clone()));

			let validator_set_count = keys.len() as u32;
			let offence = UnresponsivenessOffence { session_index, validator_set_count, offenders };
			if let Err(e) = T::ReportUnresponsiveness::report_offence(vec![], offence) {
				sp_runtime::print(e);
			}
		}
	}

	fn on_disabled(_i: usize) {
		// ignore
	}
}

/// Invalid transaction custom error. Returned when validators_len field in heartbeat is incorrect.
const INVALID_VALIDATORS_LEN: u8 = 10;

impl<T: Trait> frame_support::unsigned::ValidateUnsigned for Module<T> {
	type Call = Call<T>;

	fn validate_unsigned(
		_source: TransactionSource,
		call: &Self::Call,
	) -> TransactionValidity {
		if let Call::heartbeat(heartbeat, signature) = call {
			if <Module<T>>::is_online(heartbeat.authority_index) {
				// we already received a heartbeat for this authority
				return InvalidTransaction::Stale.into();
			}

			// check if session index from heartbeat is recent
			let current_session = <pallet_session::Module<T>>::current_index();
			if heartbeat.session_index != current_session {
				return InvalidTransaction::Stale.into();
			}

			// verify that the incoming (unverified) pubkey is actually an authority id
			let keys = Keys::<T>::get();
			if keys.len() as u32 != heartbeat.validators_len {
				return InvalidTransaction::Custom(INVALID_VALIDATORS_LEN).into();
			}
			let authority_id = match keys.get(heartbeat.authority_index as usize) {
				Some(id) => id,
				None => return InvalidTransaction::BadProof.into(),
			};

			// check signature (this is expensive so we do it last).
			let signature_valid = heartbeat.using_encoded(|encoded_heartbeat| {
				authority_id.verify(&encoded_heartbeat, &signature)
			});

			if !signature_valid {
				return InvalidTransaction::BadProof.into();
			}

			ValidTransaction::with_tag_prefix("ImOnline")
				.priority(T::UnsignedPriority::get())
				.and_provides((current_session, authority_id))
				.longevity(TryInto::<u64>::try_into(
					T::SessionDuration::get() / 2.into()
				).unwrap_or(64_u64))
				.propagate(true)
				.build()
		} else {
			InvalidTransaction::Call.into()
		}
	}
}

/// An offence that is filed if a validator didn't send a heartbeat message.
#[derive(RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Clone, PartialEq, Eq))]
pub struct UnresponsivenessOffence<Offender> {
	/// The current session index in which we report the unresponsive validators.
	///
	/// It acts as a time measure for unresponsiveness reports and effectively will always point
	/// at the end of the session.
	pub session_index: SessionIndex,
	/// The size of the validator set in current session/era.
	pub validator_set_count: u32,
	/// Authorities that were unresponsive during the current era.
	pub offenders: Vec<Offender>,
}

impl<Offender: Clone> Offence<Offender> for UnresponsivenessOffence<Offender> {
	const ID: Kind = *b"im-online:offlin";
	type TimeSlot = SessionIndex;

	fn offenders(&self) -> Vec<Offender> {
		self.offenders.clone()
	}

	fn session_index(&self) -> SessionIndex {
		self.session_index
	}

	fn validator_set_count(&self) -> u32 {
		self.validator_set_count
	}

	fn time_slot(&self) -> Self::TimeSlot {
		self.session_index
	}

	fn slash_fraction(offenders: u32, validator_set_count: u32) -> Perbill {
		// the formula is min((3 * (k - (n / 10 + 1))) / n, 1) * 0.07
		// basically, 10% can be offline with no slash, but after that, it linearly climbs up to 7%
		// when 13/30 are offline (around 5% when 1/3 are offline).
		if let Some(threshold) = offenders.checked_sub(validator_set_count / 10 + 1) {
			let x = Perbill::from_rational_approximation(3 * threshold, validator_set_count);
			x.saturating_mul(Perbill::from_percent(7))
		} else {
			Perbill::default()
		}
	}
}
