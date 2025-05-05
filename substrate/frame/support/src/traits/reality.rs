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

//! Traits concerned with modelling reality.

use core::marker::PhantomData;

use codec::{Decode, DecodeWithMemTracking, Encode, FullCodec, MaxEncodedLen};
use frame_support::{CloneNoBound, EqNoBound, Parameter, PartialEqNoBound};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::{traits::Member, BoundedVec, DispatchError, DispatchResult, RuntimeDebug};

/// Identity of personhood.
///
/// This is a persistent identifier for every individual. Regardless of what else the individual
/// changes within the system (such as identity documents, cryptographic keys, etc...) this does not
/// change. As such, it should never be used in application code.
pub type PersonalId = u64;

/// Identifier for a specific application in which we may wish to track individual people.
///
/// NOTE: This MUST remain equivalent to the type `Context` in the crate `verifiable`.
pub type Context = [u8; 32];

/// Identifier for a specific individual within an application context.
///
/// NOTE: This MUST remain equivalent to the type `Alias` in the crate `verifiable`.
pub type Alias = [u8; 32];

/// The type we use to identify different rings.
pub type RingIndex = u32;

/// Data type for arbitrary information handled by the statement oracle.
pub type Data = BoundedVec<u8, ConstU32<32>>;

/// The maximum length of custom statement data.
pub const MAX_STATEMENT_DATA_SIZE: u32 = 256;

/// Data type for custom statement information handled by the statement oracle.
pub type CustomStatement = BoundedVec<u8, ConstU32<MAX_STATEMENT_DATA_SIZE>>;

/// The type used to represent the hash of the evidence used in statements by the statement oracle.
pub type EvidenceHash = [u8; 32];

/// Maximum length of the context passed in the oracle's judgements.
pub const CONTEXT_SIZE: u32 = 64;

/// The type used to represent the context of an oracle's judgement.
pub type JudgementContext = BoundedVec<u8, ConstU32<CONTEXT_SIZE>>;

/// The [`Alias`] type enriched with the originating [`Context`].
#[derive(
	Clone,
	PartialEq,
	Eq,
	RuntimeDebug,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	DecodeWithMemTracking,
)]
pub struct ContextualAlias {
	/// The alias of the person.
	pub alias: Alias,
	/// The context in which this alias was created.
	pub context: Context,
}

/// Trait to recognize people and handle personal id.
///
/// `PersonalId` goes through multiple state: free, reserved, used; a used personal id can belong
/// to a recognized person or a suspended person.
pub trait AddOnlyPeopleTrait {
	type Member: Parameter + MaxEncodedLen;
	/// Reserve a new id for a future person. This id is not recognized, not reserved, and has
	/// never been reserved in the past.
	fn reserve_new_id() -> PersonalId;
	/// Renew a reservation for a personal id. The id is not recognized, but has been reserved in
	/// the past.
	///
	/// An error is returned if the id is used or wasn't reserved before.
	fn renew_id_reservation(personal_id: PersonalId) -> Result<(), DispatchError>;
	/// Cancel the reservation for a personal id
	///
	/// An error is returned if the id wasn't reserved in the first place.
	fn cancel_id_reservation(personal_id: PersonalId) -> Result<(), DispatchError>;
	/// Recognized a person.
	///
	/// The personal id must be reserved or the person must have already been recognized and
	/// suspended in the past.
	/// If recognizing a new person, a key must be provided. If resuming the personhood then no key
	/// must be provided.
	///
	/// An error is returned if:
	/// * `maybe_key` is some and the personal id was not reserved or is used by a recognized or
	///   suspended person.
	/// * `maybe_key` is none and the personal id was not recognized before.
	fn recognize_personhood(
		who: PersonalId,
		maybe_key: Option<Self::Member>,
	) -> Result<(), DispatchError>;
	// All stuff for benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	type Secret;
	#[cfg(feature = "runtime-benchmarks")]
	fn mock_key(who: PersonalId) -> (Self::Member, Self::Secret);
}

/// Trait to recognize and suspend people.
pub trait PeopleTrait: AddOnlyPeopleTrait {
	/// Suspend a set of people. This operation must be called within a mutation session.
	///
	/// An error is returned if:
	/// * a suspended personal id was already suspended.
	/// * a personal id doesn't belong to any person.
	fn suspend_personhood(suspensions: &[PersonalId]) -> DispatchResult;
	/// Start a mutation session for setting people.
	///
	/// An error is returned if the mutation session can be started at the moment. It is expected
	/// to become startable later.
	fn start_people_set_mutation_session() -> DispatchResult;
	/// End a mutation session for setting people.
	///
	/// An error is returned if there is no mutation session ongoing.
	fn end_people_set_mutation_session() -> DispatchResult;
}

impl AddOnlyPeopleTrait for () {
	type Member = ();
	fn reserve_new_id() -> PersonalId {
		0
	}
	fn renew_id_reservation(_: PersonalId) -> Result<(), DispatchError> {
		Ok(())
	}
	fn cancel_id_reservation(_: PersonalId) -> Result<(), DispatchError> {
		Ok(())
	}
	fn recognize_personhood(_: PersonalId, _: Option<Self::Member>) -> Result<(), DispatchError> {
		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	type Secret = PersonalId;
	#[cfg(feature = "runtime-benchmarks")]
	fn mock_key(who: PersonalId) -> (Self::Member, Self::Secret) {
		((), who)
	}
}

impl PeopleTrait for () {
	fn suspend_personhood(_: &[PersonalId]) -> DispatchResult {
		Ok(())
	}
	fn start_people_set_mutation_session() -> DispatchResult {
		Ok(())
	}
	fn end_people_set_mutation_session() -> DispatchResult {
		Ok(())
	}
}

/// Trait to get the total number of active members in a set.
pub trait CountedMembers {
	/// Returns the number of active members in the set.
	fn active_count(&self) -> u32;
}

/// A legitimate verdict on a particular statement.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	RuntimeDebug,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	DecodeWithMemTracking,
)]
pub enum Truth {
	/// The evidence can be taken as a clear indication that the statement is true. Doubt may still
	/// remain but it should be unlikely (no more than 1 chance in 20) that this doubt would be
	/// substantial enough to contravene the evidence.
	True,
	/// The evidence contradicts the statement.
	False,
}

/// Judgement passed on the truth and validity of a statement.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	RuntimeDebug,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	DecodeWithMemTracking,
)]
pub enum Judgement {
	/// A judgement on the truth of a statement.
	Truth(Truth),
	/// The evidence supplied probably (P > 50%) implies contempt for the system. Submitting
	/// evidence which clearly appears to be manipulated or intentionally provides no indication of
	/// truth for the statement would imply this outcome.
	Contempt,
}

impl Judgement {
	pub fn matches_intent(&self, j: Self) -> bool {
		use self::Truth::*;
		use Judgement::*;
		matches!(
			(self, j),
			(Truth(True), Truth(True)) | (Truth(False), Truth(False)) | (Contempt, Contempt)
		)
	}
}

pub mod identity {
	use super::*;

	/// Social platforms that statement oracles support.
	#[derive(
		Clone,
		PartialEq,
		Eq,
		RuntimeDebug,
		Encode,
		Decode,
		MaxEncodedLen,
		TypeInfo,
		DecodeWithMemTracking,
	)]
	pub enum Social {
		Twitter { username: Data },
		Github { username: Data },
	}

	impl Social {
		pub fn eq_platform(&self, other: &Social) -> bool {
			matches!(
				(&self, &other),
				(Social::Twitter { .. }, Social::Twitter { .. }) |
					(Social::Github { .. }, Social::Github { .. })
			)
		}
	}
}

/// A statement upon which a [`StatementOracle`] can provide judgement.
#[derive(Clone, PartialEq, Eq, RuntimeDebug, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum Statement {
	/// Ask for whether evidence exists to confirm that a particular social credential on a
	/// supported platform belongs to a person.
	IdentityCredential { platform: identity::Social, evidence: Data },
	/// Ask for whether a username meets certain standards.
	///
	/// It is up to the oracle to decide upon username validity,
	/// but it may be assumed that a username is considered acceptable if it:
	/// - contains no offensive, discriminatory, or inappropriate content,
	/// - is visually distinct and readable in user interfaces,
	/// - complies with other oracle guidelines.
	UsernameValid { username: Data },
	/// Ask for a custom statement to be judged. The responsibility of correctly interpreting the
	/// encoded bytes falls on the [`StatementOracle`] implementation handling this. If no custom
	/// statements are allowed, the implementation should reject this variant altogether.
	Custom { id: u8, data: CustomStatement },
}

/// Describes the location within the runtime of a callback, along with other type information such
/// as parameters passed into the callback.
#[derive(
	CloneNoBound, PartialEqNoBound, EqNoBound, RuntimeDebug, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
#[scale_info(skip_type_params(Params, RuntimeCall))]
#[codec(mel_bound())]
pub struct Callback<Params, RuntimeCall> {
	pallet_index: u8,
	call_index: u8,
	phantom: PhantomData<(Params, RuntimeCall)>,
}

impl<Params: Encode, RuntimeCall: Decode> Callback<Params, RuntimeCall> {
	pub const fn from_parts(pallet_index: u8, call_index: u8) -> Self {
		Self { pallet_index, call_index, phantom: PhantomData }
	}
	pub fn curry(&self, args: Params) -> Result<RuntimeCall, codec::Error> {
		(self.pallet_index, self.call_index, args).using_encoded(|mut d| Decode::decode(&mut d))
	}
}

/// A provider of wondrous magic: give it a `Statement` and it will tell you if it's true, with
/// some degree of resilience.
///
/// It's asynchronous, so you give it a callback in the form of a `RuntimeCall` stub.
pub trait StatementOracle<RuntimeCall> {
	/// A small piece of data which may be used to identify different ongoing judgements.
	type Ticket: Member + FullCodec + TypeInfo + MaxEncodedLen + Default;

	/// Judge a `statement` and get a Judgement.
	///
	/// We only care about the pallet/call index of `callback`; it must take exactly three
	/// arguments:
	///
	/// - `Self::Ticket`: The ticket which was returned here to identify the judgement.
	/// - `JudgementContext`: The value of `context` which was passed in to this call.
	/// - `Judgement`: The judgement given by the oracle.
	///
	/// It is assumed that all costs associated with this oraclisation have already been paid for
	/// or are absorbed by the system acting in its own interests.
	fn judge_statement(
		statement: Statement,
		context: JudgementContext,
		callback: Callback<(Self::Ticket, JudgementContext, Judgement), RuntimeCall>,
	) -> Result<Self::Ticket, DispatchError>;
}

impl<C> StatementOracle<C> for () {
	type Ticket = ();
	fn judge_statement(
		_: Statement,
		_: JudgementContext,
		_: Callback<(Self::Ticket, JudgementContext, Judgement), C>,
	) -> Result<(), DispatchError> {
		Err(DispatchError::Unavailable)
	}
}
