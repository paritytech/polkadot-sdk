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

//! The client for the relay chain, intended to be used in AssetHub.
//!
//! The counter-part for this pallet is `pallet-staking-async-ah-client` on the relay chain.
//!
//! This documentation is divided into the following sections:
//!
//! 1. Incoming messages: the messages that we receive from the relay chian.
//! 2. Outgoing messages: the messaged that we sent to the relay chain.
//! 3. Local interfaces: the interfaces that we expose to other pallets in the runtime.
//!
//! ## Incoming Messages
//!
//! All incoming messages are handled via [`Call`]. They are all gated to be dispatched only by the
//! relay chain origin, as per [`Config::RelayChainOrigin`].
//!
//! After potential queuing, they are passed to pallet-staking-async via [`AHStakingInterface`].
//!
//! The calls are:
//!
//! * [`Call::relay_session_report`]: A report from the relay chain, indicating the end of a
//!   session. We allow ourselves to know an implementation detail: **The ending of session `x`
//!   always implies start of session `x+1` and planning of session `x+2`.** This allows us to have
//!   just one message per session.
//!
//! > Note that in the code, due to historical reasons, planning of a new session is called
//! > `new_session`.
//!
//! * [`Call::relay_new_offence_paged`]: A report of one or more offences on the relay chain.
//!
//! ## Outgoing Messages
//!
//! The outgoing messages are expressed in [`SendToRelayChain`].
//!
//! ## Local Interfaces
//!
//! Within this pallet, we need to talk to the staking-async pallet in AH. This is done via
//! [`AHStakingInterface`] trait.
//!
//! The staking pallet in AH has no communication with session pallet whatsoever, therefore its
//! implementation of `SessionManager`, and it associated type `SessionInterface` no longer exists.
//! Moreover, pallet-staking-async no longer has a notion of timestamp locally, and only relies in
//! the timestamp passed in in the `SessionReport`.
//!
//! ## Shared Types
//!
//! Note that a number of types need to be shared between this crate and `ah-client`. For now, as a
//! convention, they are kept in this crate. This can later be decoupled into a shared crate, or
//! `sp-staking`.
//!
//! TODO: the rest should go to staking-async docs.
//!
//! ## Session Change
//!
//! Further details of how the session change works follows. These details are important to how
//! `pallet-staking-async` should rotate sessions/eras going forward.
//!
//! ### Synchronous Model
//!
//! Let's first consider the old school model, when staking and session lived in the same runtime.
//! Assume 3 sessions is one era.
//!
//! The session pallet issues the following events:
//!
//! end_session / start_session / new_session (plan session)
//!
//! * end 0, start 1, plan 2
//! * end 1, start 2, plan 3 (new validator set returned)
//! * end 2, start 3 (new validator set activated), plan 4
//! * end 3, start 4, plan 5
//! * end 4, start 5, plan 6 (ah-client to already return validator set) and so on.
//!
//! Staking should then do the following:
//!
//! * once a request to plan session 3 comes in, it must return a validator set. This is queued
//!   internally in the session pallet, and is enacted later.
//! * at the same time, staking increases its notion of `current_era` by 1. Yet, `active_era` is
//!   intact. This is because the validator elected for era n+1 are not yet active in the session
//!   pallet.
//! * once a request to _start_ session 3 comes in, staking will rotate its `active_era` to also be
//!   incremented to n+1.
//!
//! ### Asynchronous Model
//!
//! Now, if staking lives in AH and the session pallet lives in the relay chain, how will this look
//! like?
//!
//! Staking knows that by the time the relay-chain session index `3` (and later on `6` and so on) is
//! _planned_, it must have already returned a validator set via XCM.
//!
//! conceptually, staking must:
//!
//! - listen to the [`SessionReport`]s coming in, and start a new staking election such that we can
//!   be sure it is delivered to the RC well before the the message for planning session 3 received.
//! - Staking should know that, regardless of the timing, these validators correspond to session 3,
//!   and an upcoming era.
//! - Staking will keep these pending validators internally within its state.
//! - Once the message to start session 3 is received, staking will act upon it locally.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
pub mod weights;

use alloc::{vec, vec::Vec};
use codec::Decode;
#[cfg(feature = "xcm-sender")]
use core::fmt::Display;
#[cfg(feature = "xcm-sender")]
use frame_support::storage::transactional::with_transaction_opaque_err;
use frame_support::{pallet_prelude::*, traits::tokens::Balance as BalanceTrait, weights::Weight};
#[cfg(feature = "xcm-sender")]
use sp_runtime::{traits::Convert, TransactionOutcome};
use sp_runtime::{traits::OpaqueKeys, Perbill};
use sp_staking::SessionIndex;
// XCM imports are only used by the optional XCMSender helper struct for runtimes, not by the
// pallet's public API. The pallet only uses the abstract SendToRelayChain trait.
//
// TODO: Consider relocating `staking-async` pallets to `polkadot/pallets/` or
// `cumulus/pallets/`. These pallets are Polkadot-specific (AH↔RC communication) and leak XCM
// types into FRAME, which historically has been chain-agnostic. Alternatively, the `XCMSender`
// helper could be moved to runtime level, keeping this pallet XCM-agnostic through the
// `SendToRelayChain` trait abstraction.
#[cfg(feature = "xcm-sender")]
use xcm::latest::{
	send_xcm, validate_send, ExecuteXcm, Fungibility::Fungible, Location, SendError, SendXcm, Xcm,
};

/// Export everything needed for the pallet to be used in the runtime.
pub use pallet::*;
pub use weights::WeightInfo;

/// Type alias for balance used in this pallet.
pub type BalanceOf<T> = <T as pallet::Config>::Balance;

const LOG_TARGET: &str = "runtime::staking-async::rc-client";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: $crate::LOG_TARGET,
			concat!("[{:?}] ⬆️ ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

/// Detailed errors for message send operations.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum SendOperationError {
	/// Failed to validate the message before sending.
	ValidationFailed,
	/// Failed to charge delivery fees from the payer.
	ChargeFeesFailed,
	/// Failed to deliver the message to the relay chain.
	DeliveryFailed,
}

/// Error type for [`SendToRelayChain`] operations.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum SendKeysError<Balance> {
	/// Message send operation failed.
	Send(SendOperationError),
	/// Delivery fees exceeded the specified maximum.
	FeesExceededMax {
		/// The required fee amount.
		required: Balance,
		/// The maximum fee the user was willing to pay.
		max: Balance,
	},
}

/// The communication trait of `pallet-staking-async-rc-client` -> `relay-chain`.
///
/// This trait should only encapsulate our _outgoing_ communication to the RC. Any incoming
/// communication comes it directly via our calls.
///
/// In a real runtime, this is implemented via XCM calls, much like how the core-time pallet works.
/// In a test runtime, it can be wired to direct function calls.
///
/// Note: This trait intentionally avoids XCM types in its signature to keep the pallet
/// XCM-agnostic. The implementation details (XCM, direct calls, etc.) are left to the runtime.
pub trait SendToRelayChain {
	/// The validator account ids.
	type AccountId;

	/// The balance type used for fee limits and reporting.
	type Balance: Parameter + Member + Copy;

	/// Send a new validator set report to relay chain.
	#[allow(clippy::result_unit_err)]
	fn validator_set(report: ValidatorSetReport<Self::AccountId>) -> Result<(), ()>;

	/// Send session keys to relay chain for registration.
	///
	/// The keys are forwarded to `pallet-staking-async-ah-client::set_keys_from_ah` on the RC.
	/// Note: proof is validated on AH side, so only validated keys are sent.
	///
	/// The relay chain uses `UnpaidExecution`, so no fees are charged there. Instead, the total
	/// fee (delivery + remote execution cost) is charged on AssetHub.
	///
	/// - `stash`: The validator stash account.
	/// - `keys`: The encoded session keys.
	/// - `max_delivery_and_remote_execution_fee`: Optional maximum total fee the user is willing to
	///   pay. This includes both the XCM delivery fee and the remote execution cost. If the actual
	///   total fee exceeds this, the operation fails with [`SendKeysError::FeesExceededMax`]. Pass
	///   `None` for unlimited (no cap).
	///
	/// Returns the total fees charged on success (delivery + execution).
	fn set_keys(
		stash: Self::AccountId,
		keys: Vec<u8>,
		max_delivery_and_remote_execution_fee: Option<Self::Balance>,
	) -> Result<Self::Balance, SendKeysError<Self::Balance>>;

	/// Send a request to purge session keys on the relay chain.
	///
	/// The request is forwarded to `pallet-staking-async-ah-client::purge_keys_from_ah` on the RC.
	///
	/// The relay chain uses `UnpaidExecution`, so no fees are charged there. Instead, the total
	/// fee (delivery + remote execution cost) is charged on AssetHub.
	///
	/// - `stash`: The validator stash account.
	/// - `max_delivery_and_remote_execution_fee`: Optional maximum total fee the user is willing to
	///   pay. This includes both the XCM delivery fee and the remote execution cost. If the actual
	///   total fee exceeds this, the operation fails with [`SendKeysError::FeesExceededMax`]. Pass
	///   `None` for unlimited (no cap).
	///
	/// Returns the total fees charged on success (delivery + execution).
	fn purge_keys(
		stash: Self::AccountId,
		max_delivery_and_remote_execution_fee: Option<Self::Balance>,
	) -> Result<Self::Balance, SendKeysError<Self::Balance>>;
}

#[cfg(feature = "std")]
impl SendToRelayChain for () {
	type AccountId = u64;
	type Balance = u128;
	fn validator_set(_report: ValidatorSetReport<Self::AccountId>) -> Result<(), ()> {
		unimplemented!();
	}
	fn set_keys(
		_stash: Self::AccountId,
		_keys: Vec<u8>,
		_max_delivery_and_remote_execution_fee: Option<Self::Balance>,
	) -> Result<Self::Balance, SendKeysError<Self::Balance>> {
		unimplemented!();
	}
	fn purge_keys(
		_stash: Self::AccountId,
		_max_delivery_and_remote_execution_fee: Option<Self::Balance>,
	) -> Result<Self::Balance, SendKeysError<Self::Balance>> {
		unimplemented!();
	}
}

/// The interface to communicate to asset hub.
///
/// This trait should only encapsulate our outgoing communications. Any incoming message is handled
/// with `Call`s.
///
/// In a real runtime, this is implemented via XCM calls, much like how the coretime pallet works.
/// In a test runtime, it can be wired to direct function call.
pub trait SendToAssetHub {
	/// The validator account ids.
	type AccountId;

	/// Report a session change to AssetHub.
	///
	/// Returning `Err(())` means the DMP queue is full, and you should try again in the next block.
	#[allow(clippy::result_unit_err)]
	fn relay_session_report(session_report: SessionReport<Self::AccountId>) -> Result<(), ()>;

	#[allow(clippy::result_unit_err)]
	fn relay_new_offence_paged(
		offences: Vec<(SessionIndex, Offence<Self::AccountId>)>,
	) -> Result<(), ()>;
}

/// A no-op implementation of [`SendToAssetHub`].
#[cfg(feature = "std")]
impl SendToAssetHub for () {
	type AccountId = u64;

	fn relay_session_report(_session_report: SessionReport<Self::AccountId>) -> Result<(), ()> {
		unimplemented!();
	}

	fn relay_new_offence_paged(
		_offences: Vec<(SessionIndex, Offence<Self::AccountId>)>,
	) -> Result<(), ()> {
		unimplemented!()
	}
}

#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, TypeInfo)]
/// A report about a new validator set. This is sent from AH -> RC.
pub struct ValidatorSetReport<AccountId> {
	/// The new validator set.
	pub new_validator_set: Vec<AccountId>,
	/// The id of this validator set.
	///
	/// Is an always incrementing identifier for this validator set, the activation of which can be
	/// later pointed to in a `SessionReport`.
	///
	/// Implementation detail: within `pallet-staking-async`, this is always set to the
	/// `planning-era` (aka. `CurrentEra`).
	pub id: u32,
	/// Signal the relay chain that it can prune up to this session, and enough eras have passed.
	///
	/// This can always have a safety buffer. For example, whatever is a sane value, it can be
	/// `value - 5`.
	pub prune_up_to: Option<SessionIndex>,
	/// Same semantics as [`SessionReport::leftover`].
	pub leftover: bool,
}

impl<AccountId: core::fmt::Debug> core::fmt::Debug for ValidatorSetReport<AccountId> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("ValidatorSetReport")
			.field("new_validator_set", &self.new_validator_set)
			.field("id", &self.id)
			.field("prune_up_to", &self.prune_up_to)
			.field("leftover", &self.leftover)
			.finish()
	}
}

impl<AccountId> core::fmt::Display for ValidatorSetReport<AccountId> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("ValidatorSetReport")
			.field("new_validator_set", &self.new_validator_set.len())
			.field("id", &self.id)
			.field("prune_up_to", &self.prune_up_to)
			.field("leftover", &self.leftover)
			.finish()
	}
}

impl<AccountId> ValidatorSetReport<AccountId> {
	/// A new instance of self that is terminal. This is useful when we want to send everything in
	/// one go.
	pub fn new_terminal(
		new_validator_set: Vec<AccountId>,
		id: u32,
		prune_up_to: Option<SessionIndex>,
	) -> Self {
		Self { new_validator_set, id, prune_up_to, leftover: false }
	}

	/// Merge oneself with another instance.
	pub fn merge(mut self, other: Self) -> Result<Self, UnexpectedKind> {
		if self.id != other.id || self.prune_up_to != other.prune_up_to {
			// Must be some bug -- don't merge.
			return Err(UnexpectedKind::ValidatorSetIntegrityFailed);
		}
		self.new_validator_set.extend(other.new_validator_set);
		self.leftover = other.leftover;
		Ok(self)
	}

	/// Split self into chunks of `chunk_size` element.
	pub fn split(self, chunk_size: usize) -> Vec<Self>
	where
		AccountId: Clone,
	{
		let splitted_points = self.new_validator_set.chunks(chunk_size.max(1)).map(|x| x.to_vec());
		let mut parts = splitted_points
			.into_iter()
			.map(|new_validator_set| Self { new_validator_set, leftover: true, ..self })
			.collect::<Vec<_>>();
		if let Some(x) = parts.last_mut() {
			x.leftover = false
		}
		parts
	}
}

/// Message for session keys operations (set or purge) sent from AH -> RC.
///
/// This type is shared between `rc-client` (AssetHub) and `ah-client` (RelayChain).
/// The proof is validated on AH side, so only validated keys are sent to RC.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Debug, TypeInfo)]
pub enum KeysMessage<AccountId> {
	/// Set session keys for a validator.
	SetKeys {
		/// The validator stash account.
		stash: AccountId,
		/// The encoded session keys.
		keys: Vec<u8>,
	},
	/// Purge session keys for a validator.
	PurgeKeys {
		/// The validator stash account.
		stash: AccountId,
	},
}

impl<AccountId> KeysMessage<AccountId> {
	/// Create a new SetKeys message.
	pub fn set_keys(stash: AccountId, keys: Vec<u8>) -> Self {
		Self::SetKeys { stash, keys }
	}

	/// Create a new PurgeKeys message.
	pub fn purge_keys(stash: AccountId) -> Self {
		Self::PurgeKeys { stash }
	}

	/// Get the stash account from the message.
	pub fn stash(&self) -> &AccountId {
		match self {
			Self::SetKeys { stash, .. } | Self::PurgeKeys { stash } => stash,
		}
	}
}

#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, TypeInfo, MaxEncodedLen)]
/// The information that is sent from RC -> AH on session end.
pub struct SessionReport<AccountId> {
	/// The session that is ending.
	///
	/// This always implies start of `end_index + 1`, and planning of `end_index + 2`.
	pub end_index: SessionIndex,
	/// All of the points that validators have accumulated.
	///
	/// This can be either from block authoring, or from parachain consensus, or anything else.
	pub validator_points: Vec<(AccountId, u32)>,
	/// If none, it means no new validator set was activated as a part of this session.
	///
	/// If `Some((timestamp, id))`, it means that the new validator set was activated at the given
	/// timestamp, and the id of the validator set is `id`.
	///
	/// This `id` is what was previously communicated to the RC as a part of
	/// [`ValidatorSetReport::id`].
	pub activation_timestamp: Option<(u64, u32)>,
	/// If this session report is self-contained, then it is false.
	///
	/// If this session report has some leftover, it should not be acted upon until a subsequent
	/// message with `leftover = true` comes in. The client pallets should handle this queuing.
	///
	/// This is in place to future proof us against possibly needing to send multiple rounds of
	/// messages to convey all of the `validator_points`.
	///
	/// Upon processing, this should always be true, and it should be ignored.
	pub leftover: bool,
}

impl<AccountId: core::fmt::Debug> core::fmt::Debug for SessionReport<AccountId> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("SessionReport")
			.field("end_index", &self.end_index)
			.field("validator_points", &self.validator_points)
			.field("activation_timestamp", &self.activation_timestamp)
			.field("leftover", &self.leftover)
			.finish()
	}
}

impl<AccountId> core::fmt::Display for SessionReport<AccountId> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("SessionReport")
			.field("end_index", &self.end_index)
			.field("validator_points", &self.validator_points.len())
			.field("activation_timestamp", &self.activation_timestamp)
			.field("leftover", &self.leftover)
			.finish()
	}
}

impl<AccountId> SessionReport<AccountId> {
	/// A new instance of self that is terminal. This is useful when we want to send everything in
	/// one go.
	pub fn new_terminal(
		end_index: SessionIndex,
		validator_points: Vec<(AccountId, u32)>,
		activation_timestamp: Option<(u64, u32)>,
	) -> Self {
		Self { end_index, validator_points, activation_timestamp, leftover: false }
	}

	/// Merge oneself with another instance.
	pub fn merge(mut self, other: Self) -> Result<Self, UnexpectedKind> {
		if self.end_index != other.end_index ||
			self.activation_timestamp != other.activation_timestamp
		{
			// Must be some bug -- don't merge.
			return Err(UnexpectedKind::SessionReportIntegrityFailed);
		}
		self.validator_points.extend(other.validator_points);
		self.leftover = other.leftover;
		Ok(self)
	}

	/// Split oneself into `count` number of pieces.
	pub fn split(self, chunk_size: usize) -> Vec<Self>
	where
		AccountId: Clone,
	{
		let splitted_points = self.validator_points.chunks(chunk_size.max(1)).map(|x| x.to_vec());
		let mut parts = splitted_points
			.into_iter()
			.map(|validator_points| Self { validator_points, leftover: true, ..self })
			.collect::<Vec<_>>();
		if let Some(x) = parts.last_mut() {
			x.leftover = false
		}
		parts
	}
}

/// A trait to encapsulate messages between RC and AH that can be splitted into smaller chunks.
///
/// Implemented for [`SessionReport`] and [`ValidatorSetReport`].
#[allow(clippy::len_without_is_empty)]
pub trait SplittableMessage: Sized {
	/// Split yourself into pieces of `chunk_size` size.
	fn split_by(self, chunk_size: usize) -> Vec<Self>;

	/// Current length of the message.
	fn len(&self) -> usize;
}

impl<AccountId: Clone> SplittableMessage for SessionReport<AccountId> {
	fn split_by(self, chunk_size: usize) -> Vec<Self> {
		self.split(chunk_size)
	}
	fn len(&self) -> usize {
		self.validator_points.len()
	}
}

impl<AccountId: Clone> SplittableMessage for ValidatorSetReport<AccountId> {
	fn split_by(self, chunk_size: usize) -> Vec<Self> {
		self.split(chunk_size)
	}
	fn len(&self) -> usize {
		self.new_validator_set.len()
	}
}

/// Common utility to send XCM messages that can use [`SplittableMessage`].
///
/// It can be used both in the RC and AH. `Message` is the splittable message type, and `ToXcm`
/// should be configured by the user, converting `message` to a valida `Xcm<()>`. It should utilize
/// the correct call indices, which we only know at the runtime level.
//
// NOTE: to have the pallet fully XCM-agnostic, XCMSender should be moved out (to a new or existing
// XCM helper crate or to runtimes crates directly)
#[cfg(feature = "xcm-sender")]
pub struct XCMSender<Sender, Destination, Message, ToXcm>(
	core::marker::PhantomData<(Sender, Destination, Message, ToXcm)>,
);

#[cfg(feature = "xcm-sender")]
impl<Sender, Destination, Message, ToXcm> XCMSender<Sender, Destination, Message, ToXcm>
where
	Sender: SendXcm,
	Destination: Get<Location>,
	Message: Clone + Encode,
	ToXcm: Convert<Message, Xcm<()>>,
{
	/// Send the message single-shot; no splitting.
	///
	/// Useful for sending messages that are already paged/chunked, so we are sure that they fit in
	/// one message.
	#[allow(clippy::result_unit_err)]
	pub fn send(message: Message) -> Result<(), ()> {
		let xcm = ToXcm::convert(message);
		let dest = Destination::get();
		// send_xcm already calls validate internally
		send_xcm::<Sender>(dest, xcm).map(|_| ()).map_err(|_| ())
	}

	/// Send the message with fee charging and optional max fee limit.
	///
	/// This method validates the XCM message first, calculates the total fee (delivery +
	/// execution), optionally checks if the total exceeds the specified maximum, charges
	/// the total from the payer, and then delivers the message.
	///
	/// The relay chain uses `UnpaidExecution`, so no fees are charged there. Instead, the
	/// total cost (delivery + remote execution) is charged upfront on AssetHub.
	///
	/// - `message`: The message to send
	/// - `payer`: The account paying fees
	/// - `max_delivery_and_remote_execution_fee`: Optional maximum total fee the user is willing to
	///   pay
	/// - `execution_cost`: The relay chain execution cost to include in the total
	///
	/// Generic parameters:
	/// - `XcmExec`: The XCM executor that implements `charge_fees`
	/// - `Call`: The runtime call type (used by XcmExec)
	/// - `AccountId`: The account identifier type
	/// - `AccountToLoc`: Converter from AccountId to XCM Location
	/// - `Balance`: The balance type for fee limits
	///
	/// Returns the total fees charged on success (delivery + execution).
	pub fn send_with_fees<XcmExec, Call, AccountId, AccountToLoc, Balance>(
		message: Message,
		payer: AccountId,
		max_delivery_and_remote_execution_fee: Option<Balance>,
		execution_cost: Balance,
	) -> Result<Balance, SendKeysError<Balance>>
	where
		XcmExec: ExecuteXcm<Call>,
		AccountToLoc: Convert<AccountId, Location>,
		Balance: TryFrom<u128>
			+ Into<u128>
			+ PartialOrd
			+ Copy
			+ Default
			+ core::ops::Add<Output = Balance>,
	{
		let payer_location = AccountToLoc::convert(payer);
		let xcm = ToXcm::convert(message);
		let dest = Destination::get();

		let (ticket, price) = validate_send::<Sender>(dest, xcm).map_err(|e| {
			log::error!(target: LOG_TARGET, "Failed to validate XCM: {:?}", e);
			SendKeysError::Send(SendOperationError::ValidationFailed)
		})?;

		// Extract the delivery fee asset from the price.
		//
		// For parachain→relay chain messages, delivery fees are returned as a single
		// fungible asset. This is based on `ExponentialPrice::price_for_delivery` in
		// `polkadot/runtime/common/src/xcm_sender.rs` which returns `(AssetId, amount).into()`,
		// converting to a single-element `Assets` via `impl<T: Into<Asset>> From<T> for Assets`.
		let fee_asset = price.inner().first().ok_or_else(|| {
			log::error!(target: LOG_TARGET, "Empty price returned from validate_send");
			SendKeysError::Send(SendOperationError::ValidationFailed)
		})?;

		let delivery_fee: Balance = match &fee_asset.fun {
			Fungible(amount) => Balance::try_from(*amount).map_err(|_| {
				log::error!(target: LOG_TARGET, "Failed to convert delivery fee amount");
				SendKeysError::Send(SendOperationError::ValidationFailed)
			})?,
			_ => {
				log::error!(target: LOG_TARGET, "Non-fungible fee asset not supported");
				return Err(SendKeysError::Send(SendOperationError::ValidationFailed));
			},
		};

		// Calculate total fee = delivery + execution
		let total_fee = delivery_fee + execution_cost;

		// Check max fee before charging
		if let Some(max) = max_delivery_and_remote_execution_fee {
			if total_fee > max {
				return Err(SendKeysError::FeesExceededMax { required: total_fee, max });
			}
		}

		// Charge the total fee from the payer using the same asset as delivery fees
		let total_assets = xcm::latest::Assets::from(xcm::latest::Asset {
			id: fee_asset.id.clone(),
			fun: Fungible(total_fee.into()),
		});

		// Wrap fee charging and delivery in a transaction so fees are rolled back if delivery
		// fails. Without this, users would lose fees on transient delivery failures (e.g., queue
		// full).
		match with_transaction_opaque_err(|| {
			if let Err(e) = XcmExec::charge_fees(payer_location, total_assets) {
				log::error!(target: LOG_TARGET, "Failed to charge fees: {:?}", e);
				return TransactionOutcome::Rollback(Err(SendKeysError::Send(
					SendOperationError::ChargeFeesFailed,
				)));
			}

			if let Err(e) = Sender::deliver(ticket) {
				log::error!(target: LOG_TARGET, "Failed to deliver XCM: {:?}", e);
				return TransactionOutcome::Rollback(Err(SendKeysError::Send(
					SendOperationError::DeliveryFailed,
				)));
			}

			TransactionOutcome::Commit(Ok(total_fee))
		}) {
			Ok(inner) => inner,
			// unreachable; `with_transaction_opaque_err` always returns `Ok(inner)`
			Err(_) => Err(SendKeysError::Send(SendOperationError::DeliveryFailed)),
		}
	}
}

#[cfg(feature = "xcm-sender")]
impl<Sender, Destination, Message, ToXcm> XCMSender<Sender, Destination, Message, ToXcm>
where
	Sender: SendXcm,
	Destination: Get<Location>,
	Message: SplittableMessage + Display + Clone + Encode,
	ToXcm: Convert<Message, Xcm<()>>,
{
	/// Safe send method to send a `message`, while validating it and using [`SplittableMessage`] to
	/// split it into smaller pieces if XCM validation fails with `ExceedsMaxMessageSize`. It will
	/// fail on other errors.
	///
	/// Returns `Ok()` if the message was sent using `XCM`, potentially with splitting up to
	/// `maybe_max_step` times, `Err(())` otherwise.
	#[deprecated(
		note = "all staking related VMP messages should fit the single message limits. Should not be used."
	)]
	#[allow(clippy::result_unit_err)]
	pub fn split_then_send(message: Message, maybe_max_steps: Option<u32>) -> Result<(), ()> {
		let message_type_name = core::any::type_name::<Message>();
		let dest = Destination::get();
		let xcms = Self::prepare(message, maybe_max_steps).map_err(|e| {
			log::error!(target: "runtime::staking-async::rc-client", "📨 Failed to split message {}: {:?}", message_type_name, e);
		})?;

		match with_transaction_opaque_err(|| {
			let all_sent = xcms.into_iter().enumerate().try_for_each(|(idx, xcm)| {
				log::debug!(target: "runtime::staking-async::rc-client", "📨 sending {} message index {}, size: {:?}", message_type_name, idx, xcm.encoded_size());
				send_xcm::<Sender>(dest.clone(), xcm).map(|_| {
					log::debug!(target: "runtime::staking-async::rc-client", "📨 Successfully sent {} message part {} to relay chain", message_type_name,  idx);
				}).inspect_err(|e| {
					log::error!(target: "runtime::staking-async::rc-client", "📨 Failed to send {} message to relay chain: {:?}", message_type_name, e);
				})
			});

			match all_sent {
				Ok(()) => TransactionOutcome::Commit(Ok(())),
				Err(send_err) => TransactionOutcome::Rollback(Err(send_err)),
			}
		}) {
			// just like https://doc.rust-lang.org/src/core/result.rs.html#1746 which I cannot use yet because not in 1.89
			Ok(inner) => inner.map_err(|_| ()),
			// unreachable; `with_transaction_opaque_err` always returns `Ok(inner)`
			Err(_) => Err(()),
		}
	}

	fn prepare(message: Message, maybe_max_steps: Option<u32>) -> Result<Vec<Xcm<()>>, SendError> {
		// initial chunk size is the entire thing, so it will be a vector of 1 item.
		let mut chunk_size = message.len();
		let mut steps = 0;

		loop {
			let current_messages = message.clone().split_by(chunk_size);

			// the first message is the heaviest, the last one might be smaller.
			let first_message = if let Some(r) = current_messages.first() {
				r
			} else {
				log::debug!(target: "runtime::staking-async::xcm", "📨 unexpected: no messages to send");
				return Ok(vec![]);
			};

			log::debug!(
				target: "runtime::staking-async::xcm",
				"📨 step: {:?}, chunk_size: {:?}, message_size: {:?}",
				steps,
				chunk_size,
				first_message.encoded_size(),
			);

			let first_xcm = ToXcm::convert(first_message.clone());
			match <Sender as SendXcm>::validate(&mut Some(Destination::get()), &mut Some(first_xcm))
			{
				Ok((_ticket, price)) => {
					log::debug!(target: "runtime::staking-async::xcm", "📨 validated, price: {:?}", price);
					return Ok(current_messages.into_iter().map(ToXcm::convert).collect::<Vec<_>>());
				},
				Err(SendError::ExceedsMaxMessageSize) => {
					log::debug!(target: "runtime::staking-async::xcm", "📨 ExceedsMaxMessageSize -- reducing chunk_size");
					chunk_size = chunk_size.saturating_div(2);
					steps += 1;
					if maybe_max_steps.is_some_and(|max_steps| steps > max_steps) ||
						chunk_size.is_zero()
					{
						log::error!(target: "runtime::staking-async::xcm", "📨 Exceeded max steps or chunk_size = 0");
						return Err(SendError::ExceedsMaxMessageSize);
					} else {
						// try again with the new `chunk_size`
						continue;
					}
				},
				Err(other) => {
					log::error!(target: "runtime::staking-async::xcm", "📨 other error -- cannot send XCM: {:?}", other);
					return Err(other);
				},
			}
		}
	}
}

/// Our communication trait of `pallet-staking-async-rc-client` -> `pallet-staking-async`.
///
/// This is merely a shorthand to avoid tightly-coupling the staking pallet to this pallet. It
/// limits what we can say to `pallet-staking-async` to only these functions.
pub trait AHStakingInterface {
	/// The validator account id type.
	type AccountId;
	/// Maximum number of validators that the staking system may have.
	type MaxValidatorSet: Get<u32>;

	/// New session report from the relay chain.
	fn on_relay_session_report(report: SessionReport<Self::AccountId>) -> Weight;

	/// Return the weight of `on_relay_session_report` call without executing it.
	///
	/// This will return the worst case estimate of the weight. The actual execution will return the
	/// accurate amount.
	fn weigh_on_relay_session_report(report: &SessionReport<Self::AccountId>) -> Weight;

	/// Report one or more offences on the relay chain.
	fn on_new_offences(
		slash_session: SessionIndex,
		offences: Vec<Offence<Self::AccountId>>,
	) -> Weight;

	/// Return the weight of `on_new_offences` call without executing it.
	///
	/// This will return the worst case estimate of the weight. The actual execution will return the
	/// accurate amount.
	fn weigh_on_new_offences(offence_count: u32) -> Weight;

	/// Get the active era's start session index.
	///
	/// Returns the first session index of the currently active era.
	fn active_era_start_session_index() -> SessionIndex;

	/// Check if an account is a registered validator.
	///
	/// Returns true if the account has called `validate()` and is in the `Validators` storage.
	fn is_validator(who: &Self::AccountId) -> bool;
}

/// The communication trait of `pallet-staking-async` -> `pallet-staking-async-rc-client`.
pub trait RcClientInterface {
	/// The validator account ids.
	type AccountId;

	/// Report a new validator set.
	fn validator_set(new_validator_set: Vec<Self::AccountId>, id: u32, prune_up_tp: Option<u32>);
}

/// An offence on the relay chain. Based on [`sp_staking::offence::OffenceDetails`].
#[derive(Encode, Decode, DecodeWithMemTracking, Debug, Clone, PartialEq, TypeInfo)]
pub struct Offence<AccountId> {
	/// The offender.
	pub offender: AccountId,
	/// Those who have reported this offence.
	pub reporters: Vec<AccountId>,
	/// The amount that they should be slashed.
	pub slash_fraction: Perbill,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::{BlockNumberFor, *};

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	/// An incomplete incoming session report that we have not acted upon yet.
	// Note: this can remain unbounded, as the internals of `AHStakingInterface` is benchmarked, and
	// is worst case.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type IncompleteSessionReport<T: Config> =
		StorageValue<_, SessionReport<T::AccountId>, OptionQuery>;

	/// The last session report's `end_index` that we have acted upon.
	///
	/// This allows this pallet to ensure a sequentially increasing sequence of session reports
	/// passed to staking.
	///
	/// Note that with the XCM being the backbone of communication, we have a guarantee on the
	/// ordering of messages. As long as the RC sends session reports in order, we _eventually_
	/// receive them in the same correct order as well.
	#[pallet::storage]
	pub type LastSessionReportEndingIndex<T: Config> = StorageValue<_, SessionIndex, OptionQuery>;

	/// A validator set that is outgoing, and should be sent.
	///
	/// This will be attempted to be sent, possibly on every `on_initialize` call, until it is sent,
	/// or the second value reaches zero, at which point we drop it.
	#[pallet::storage]
	// TODO: for now we know this ValidatorSetReport is at most validator-count * 32, and we don't
	// need its MEL critically.
	#[pallet::unbounded]
	pub type OutgoingValidatorSet<T: Config> =
		StorageValue<_, (ValidatorSetReport<T::AccountId>, u32), OptionQuery>;

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			let mut weight = T::DbWeight::get().reads(1);

			// Early return if no validator set to export
			if !OutgoingValidatorSet::<T>::exists() {
				return weight;
			}

			// Determine if we should export based on session offset
			let should_export = if T::ValidatorSetExportSession::get() == 0 {
				// Immediate export mode
				true
			} else {
				// Check if we've reached the target session offset
				weight.saturating_accrue(T::DbWeight::get().reads(2));

				let last_session_end = LastSessionReportEndingIndex::<T>::get().unwrap_or(0);
				let last_era_ending_index =
					T::AHStakingInterface::active_era_start_session_index().saturating_sub(1);
				let session_offset = last_session_end.saturating_sub(last_era_ending_index);

				session_offset >= T::ValidatorSetExportSession::get()
			};

			if !should_export {
				// validator set buffered until target session offset
				return weight;
			}

			// good time to export the latest elected validator set
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			if let Some((report, retries_left)) = OutgoingValidatorSet::<T>::take() {
				// Export the validator set
				weight.saturating_accrue(T::DbWeight::get().writes(1));
				match T::SendToRelayChain::validator_set(report.clone()) {
					Ok(()) => {
						log::debug!(
							target: LOG_TARGET,
							"Exported validator set to RC for Era: {}",
							report.id,
						);
					},
					Err(()) => {
						log!(error, "Failed to send validator set report to relay chain");
						weight.saturating_accrue(T::DbWeight::get().writes(1));
						Self::deposit_event(Event::<T>::Unexpected(
							UnexpectedKind::ValidatorSetSendFailed,
						));

						if let Some(new_retries_left) = retries_left.checked_sub(One::one()) {
							weight.saturating_accrue(T::DbWeight::get().writes(1));
							OutgoingValidatorSet::<T>::put((report, new_retries_left));
						} else {
							weight.saturating_accrue(T::DbWeight::get().writes(1));
							Self::deposit_event(Event::<T>::Unexpected(
								UnexpectedKind::ValidatorSetDropped,
							));
						}
					},
				}
			} else {
				defensive!("OutgoingValidatorSet checked already, must exist.");
			}

			weight
		}
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// An origin type that allows us to be sure a call is being dispatched by the relay chain.
		///
		/// It be can be configured to something like `Root` or relay chain or similar.
		type RelayChainOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Our communication handle to the local staking pallet.
		type AHStakingInterface: AHStakingInterface<AccountId = Self::AccountId>;

		/// Our communication handle to the relay chain.
		type SendToRelayChain: SendToRelayChain<
			AccountId = Self::AccountId,
			Balance = Self::Balance,
		>;

		/// Maximum number of times that we retry sending a validator set to RC, after which, if
		/// sending still fails, we emit an [`UnexpectedKind::ValidatorSetDropped`] event and drop
		/// it.
		type MaxValidatorSetRetries: Get<u32>;

		/// The end session index within an era post which we export validator set to RC.
		///
		/// This is a 1-indexed session number relative to the era start:
		/// - 0 = export immediately when received from staking pallet
		/// - 1 = export at end of first session of era
		/// - 5 = export at end of 5th session of era (for 6-session eras)
		///
		/// The validator set is placed in `OutgoingValidatorSet` when election completes
		/// in `pallet-staking-async`. The XCM message is sent when BOTH conditions met:
		/// 1. Current session offset >= `ValidatorSetExportSession`
		/// 2. `OutgoingValidatorSet` exists (validator set buffered)
		///
		/// Setting to 0 bypasses the session check and exports immediately.
		///
		/// Example: With `SessionsPerEra=6` and `ValidatorSetExportSession=4`:
		/// - Session 0: Election completes → validator set buffered in `OutgoingValidatorSet`
		/// - Sessions 1-4: Buffered (session offset < 5)
		/// - End of Session 4 and start of Session 5: Export triggered.
		///
		/// Must be < SessionsPerEra.
		type ValidatorSetExportSession: Get<SessionIndex>;

		/// The session keys type that must match the Relay Chain's `pallet_session::Config::Keys`.
		///
		/// This is used to validate session keys on AssetHub before forwarding to RC.
		/// By decoding keys here, we ensure only valid data is sent via XCM, preventing
		/// malicious validators from bloating the XCM queue with garbage.
		///
		/// The type must implement `OpaqueKeys` for ownership proof validation and `Decode`
		/// to verify the keys can be properly decoded.
		type SessionKeys: OpaqueKeys + Decode;

		/// The balance type used for delivery fee limits.
		type Balance: BalanceTrait;

		/// Maximum length of encoded session keys.
		#[pallet::constant]
		type MaxSessionKeysLength: Get<u32>;

		/// Maximum length of the session keys ownership proof.
		#[pallet::constant]
		type MaxSessionKeysProofLength: Get<u32>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Failed to send XCM message to the Relay Chain.
		XcmSendFailed,
		/// The origin account is not a registered validator.
		///
		/// Only accounts that have called `validate()` can set or purge session keys. When called
		/// via a staking proxy, the origin is the delegating account (stash), which must be a
		/// registered validator.
		NotValidator,
		/// The session keys could not be decoded as the expected SessionKeys type.
		InvalidKeys,
		/// Invalid ownership proof for the session keys.
		InvalidProof,
		/// Delivery fees exceeded the specified maximum.
		FeesExceededMax,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A said session report was received.
		SessionReportReceived {
			end_index: SessionIndex,
			activation_timestamp: Option<(u64, u32)>,
			validator_points_counts: u32,
			leftover: bool,
		},
		/// A new offence was reported.
		OffenceReceived { slash_session: SessionIndex, offences_count: u32 },
		/// Fees were charged for a user operation (set_keys or purge_keys).
		///
		/// The fee includes both XCM delivery fee and relay chain execution cost.
		FeesPaid { who: T::AccountId, fees: BalanceOf<T> },
		/// Something occurred that should never happen under normal operation.
		/// Logged as an event for fail-safe observability.
		Unexpected(UnexpectedKind),
	}

	/// Represents unexpected or invariant-breaking conditions encountered during execution.
	///
	/// These variants are emitted as [`Event::Unexpected`] and indicate a defensive check has
	/// failed. While these should never occur under normal operation, they are useful for
	/// diagnosing issues in production or test environments.
	#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, TypeInfo, Debug)]
	pub enum UnexpectedKind {
		/// We could not merge the chunks, and therefore dropped the session report.
		SessionReportIntegrityFailed,
		/// We could not merge the chunks, and therefore dropped the validator set.
		ValidatorSetIntegrityFailed,
		/// The received session index is more than what we expected.
		SessionSkipped,
		/// A session in the past was received. This will not raise any errors, just emit an event
		/// and stop processing the report.
		SessionAlreadyProcessed,
		/// A validator set failed to be sent to RC.
		///
		/// We will store, and retry it for [`Config::MaxValidatorSetRetries`] future blocks.
		ValidatorSetSendFailed,
		/// A validator set was dropped.
		ValidatorSetDropped,
	}

	impl<T: Config> RcClientInterface for Pallet<T> {
		type AccountId = T::AccountId;

		fn validator_set(
			new_validator_set: Vec<Self::AccountId>,
			id: u32,
			prune_up_tp: Option<u32>,
		) {
			let report = ValidatorSetReport::new_terminal(new_validator_set, id, prune_up_tp);
			// just store the report to be outgoing, it will be sent in the next on-init.
			OutgoingValidatorSet::<T>::put((report, T::MaxValidatorSetRetries::get()));
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Called to indicate the start of a new session on the relay chain.
		#[pallet::call_index(0)]
		#[pallet::weight(
			// `LastSessionReportEndingIndex`: rw
			// `IncompleteSessionReport`: rw
			T::DbWeight::get().reads_writes(2, 2) + T::AHStakingInterface::weigh_on_relay_session_report(report)
		)]
		pub fn relay_session_report(
			origin: OriginFor<T>,
			report: SessionReport<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			log!(debug, "Received session report: {}", report);
			T::RelayChainOrigin::ensure_origin_or_root(origin)?;
			let local_weight = T::DbWeight::get().reads_writes(2, 2);

			match LastSessionReportEndingIndex::<T>::get() {
				None => {
					// first session report post genesis, okay.
				},
				Some(last) if report.end_index == last + 1 => {
					// incremental -- good
				},
				Some(last) if report.end_index > last + 1 => {
					// deposit a warning event, but proceed
					Self::deposit_event(Event::Unexpected(UnexpectedKind::SessionSkipped));
					log!(
						warn,
						"Session report end index is more than expected. last_index={:?}, report.index={:?}",
						last,
						report.end_index
					);
				},
				Some(past) => {
					log!(
						error,
						"Session report end index is not valid. last_index={:?}, report.index={:?}",
						past,
						report.end_index
					);
					Self::deposit_event(Event::Unexpected(UnexpectedKind::SessionAlreadyProcessed));
					IncompleteSessionReport::<T>::kill();
					return Ok(Some(local_weight).into());
				},
			}

			Self::deposit_event(Event::SessionReportReceived {
				end_index: report.end_index,
				activation_timestamp: report.activation_timestamp,
				validator_points_counts: report.validator_points.len() as u32,
				leftover: report.leftover,
			});

			// If we have anything previously buffered, then merge it.
			let maybe_new_session_report = match IncompleteSessionReport::<T>::take() {
				Some(old) => old.merge(report.clone()),
				None => Ok(report),
			};

			if let Err(e) = maybe_new_session_report {
				Self::deposit_event(Event::Unexpected(e));
				debug_assert!(
					IncompleteSessionReport::<T>::get().is_none(),
					"we have ::take() it above, we don't want to keep the old data"
				);
				return Ok(().into());
			}
			let new_session_report = maybe_new_session_report.expect("checked above; qed");

			if new_session_report.leftover {
				// this is still not final -- buffer it.
				IncompleteSessionReport::<T>::put(new_session_report);
				Ok(().into())
			} else {
				// this is final, report it.
				LastSessionReportEndingIndex::<T>::put(new_session_report.end_index);

				let weight = T::AHStakingInterface::on_relay_session_report(new_session_report);
				Ok((Some(local_weight + weight)).into())
			}
		}

		#[pallet::call_index(1)]
		#[pallet::weight(
			T::AHStakingInterface::weigh_on_new_offences(offences.len() as u32)
		)]
		pub fn relay_new_offence_paged(
			origin: OriginFor<T>,
			offences: Vec<(SessionIndex, Offence<T::AccountId>)>,
		) -> DispatchResultWithPostInfo {
			T::RelayChainOrigin::ensure_origin_or_root(origin)?;
			log!(info, "Received new page of {} offences", offences.len());

			let mut offences_by_session =
				alloc::collections::BTreeMap::<SessionIndex, Vec<Offence<T::AccountId>>>::new();
			for (session_index, offence) in offences {
				offences_by_session.entry(session_index).or_default().push(offence);
			}

			let mut weight: Weight = Default::default();
			for (slash_session, offences) in offences_by_session {
				Self::deposit_event(Event::OffenceReceived {
					slash_session,
					offences_count: offences.len() as u32,
				});
				let new_weight = T::AHStakingInterface::on_new_offences(slash_session, offences);
				weight.saturating_accrue(new_weight)
			}

			Ok(Some(weight).into())
		}

		/// Set session keys for a validator. Keys are validated on AssetHub and forwarded to RC.
		///
		/// **Validation on AssetHub:**
		/// - Keys are decoded as `T::SessionKeys` to ensure they match RC's expected format.
		/// - Ownership proof is validated using `OpaqueKeys::ownership_proof_is_valid`.
		///
		/// If validation passes, only the validated keys are sent to RC (with empty proof),
		/// since RC trusts AH's validation. This prevents malicious validators from bloating
		/// the XCM queue with garbage data.
		///
		/// This, combined with the enforcement of a high minimum validator bond, makes it
		/// reasonable not to require a deposit.
		///
		/// **Fees:**
		/// The actual cost of this call is higher than what the weight-based fee estimate shows.
		/// In addition to the local transaction weight fee, the stash account is charged an XCM
		/// fee (delivery + RC execution cost) via `XcmExecutor::charge_fees`. The relay chain
		/// uses `UnpaidExecution`, so the full remote cost is charged upfront on AssetHub.
		///
		/// When called via a staking proxy, the proxy pays the transaction weight fee,
		/// while the stash (delegating account) pays the XCM fee.
		///
		/// **Max Fee Limit:**
		/// Users can optionally specify `max_delivery_and_remote_execution_fee` to limit the
		/// delivery + RC execution fee. This does not include the local transaction weight fee. If
		/// the fee exceeds this limit, the operation fails with `FeesExceededMax`. Pass `None` for
		/// unlimited (no cap).
		///
		/// NOTE: unlike the current flow for new validators on RC (bond -> set_keys -> validate),
		/// users on Asset Hub MUST call bond and validate BEFORE calling set_keys. Attempting to
		/// set keys before declaring intent to validate will fail with NotValidator.
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::set_keys())]
		pub fn set_keys(
			origin: OriginFor<T>,
			keys: BoundedVec<u8, T::MaxSessionKeysLength>,
			proof: BoundedVec<u8, T::MaxSessionKeysProofLength>,
			max_delivery_and_remote_execution_fee: Option<BalanceOf<T>>,
		) -> DispatchResult {
			let stash = ensure_signed(origin)?;

			// Only registered validators can set session keys
			ensure!(T::AHStakingInterface::is_validator(&stash), Error::<T>::NotValidator);

			// Validate keys: decode as SessionKeys to ensure correct format
			let session_keys =
				T::SessionKeys::decode(&mut &keys[..]).map_err(|_| Error::<T>::InvalidKeys)?;

			// Validate ownership proof
			ensure!(
				session_keys.ownership_proof_is_valid(&stash.encode(), &proof),
				Error::<T>::InvalidProof
			);

			// Forward validated keys to RC (no proof needed, already validated)
			let fees = T::SendToRelayChain::set_keys(
				stash.clone(),
				keys.into_inner(),
				max_delivery_and_remote_execution_fee,
			)
			.map_err(|e| match e {
				SendKeysError::Send(_) => Error::<T>::XcmSendFailed,
				SendKeysError::FeesExceededMax { .. } => Error::<T>::FeesExceededMax,
			})?;
			Self::deposit_event(Event::FeesPaid { who: stash.clone(), fees });

			log::info!(target: LOG_TARGET, "Session keys validated and set for {stash:?}, forwarded to RC");

			Ok(())
		}

		/// Remove session keys for a validator.
		///
		/// This purges the keys from the Relay Chain.
		///
		/// Unlike `set_keys`, this does not require the caller to be a registered validator.
		/// This is intentional: a validator who has chilled (stopped validating) should still
		/// be able to purge their session keys. This matches the behavior of the original
		/// `pallet-session::purge_keys` which allows anyone to call it.
		///
		/// The Relay Chain will reject the call with `NoKeys` error if the account has no
		/// keys set.
		///
		/// **Fees:**
		/// The actual cost of this call is higher than what the weight-based fee estimate shows.
		/// In addition to the local transaction weight fee, the caller is charged an XCM fee
		/// (delivery + RC execution cost) via `XcmExecutor::charge_fees`. The relay chain uses
		/// `UnpaidExecution`, so the full remote cost is charged upfront on AssetHub.
		///
		/// When called via a staking proxy, the proxy pays the transaction weight fee,
		/// while the delegating account pays the XCM fee.
		///
		/// **Max Fee Limit:**
		/// Users can optionally specify `max_delivery_and_remote_execution_fee` to limit the
		/// delivery + RC execution fee. This does not include the local transaction weight fee. If
		/// the fee exceeds this limit, the operation fails with `FeesExceededMax`. Pass `None` for
		/// unlimited (no cap).
		//
		// TODO: Once we allow setting and purging keys only on AssetHub, we can introduce a state
		// (storage item) to track accounts that have called set_keys. We will also need to perform
		// a migration to populate the state for all validators that have set keys via RC.
		//
		// Note: No deposit is currently held/released, same reason as per set_keys.
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::purge_keys())]
		pub fn purge_keys(
			origin: OriginFor<T>,
			max_delivery_and_remote_execution_fee: Option<BalanceOf<T>>,
		) -> DispatchResult {
			let stash = ensure_signed(origin)?;

			// Forward purge request to RC
			// Note: RC will fail with NoKeys if the account has no keys set
			let fees = T::SendToRelayChain::purge_keys(
				stash.clone(),
				max_delivery_and_remote_execution_fee,
			)
			.map_err(|e| match e {
				SendKeysError::Send(_) => Error::<T>::XcmSendFailed,
				SendKeysError::FeesExceededMax { .. } => Error::<T>::FeesExceededMax,
			})?;
			Self::deposit_event(Event::FeesPaid { who: stash.clone(), fees });

			log::info!(target: LOG_TARGET, "Session keys purged for {stash:?}, forwarded to RC");

			Ok(())
		}
	}
}
