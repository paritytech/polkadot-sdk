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

use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use enumflags2::{BitFlag, BitFlags, _internal::RawBitFlags};
use frame_support::{traits::Get, BoundedVec, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound};
use scale_info::{build::Fields, meta_type, Path, Type, TypeInfo, TypeParameter};
use sp_runtime::RuntimeDebug;
use sp_std::{fmt::Debug, prelude::*};

/// An identifier for a single name registrar/identity verification service.
pub type RegistrarIndex = u32;

pub trait U64BitFlag: BitFlag + RawBitFlags<Numeric = u64> {}

/// An attestation of a registrar over how accurate some `IdentityInfo` is in describing an account.
///
/// NOTE: Registrars may pay little attention to some fields. Registrars may want to make clear
/// which fields their attestation is relevant for by off-chain means.
#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
pub enum Judgement<Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq>
{
	/// The default value; no opinion is held.
	Unknown,
	/// No judgement is yet in place, but a deposit is reserved as payment for providing one.
	FeePaid(Balance),
	/// The data appears to be reasonably acceptable in terms of its accuracy, however no in depth
	/// checks (such as in-person meetings or formal KYC) have been conducted.
	Reasonable,
	/// The target is known directly by the registrar and the registrar can fully attest to the
	/// the data's accuracy.
	KnownGood,
	/// The data was once good but is currently out of date. There is no malicious intent in the
	/// inaccuracy. This judgement can be removed through updating the data.
	OutOfDate,
	/// The data is imprecise or of sufficiently low-quality to be problematic. It is not
	/// indicative of malicious intent. This judgement can be removed through updating the data.
	LowQuality,
	/// The data is erroneous. This may be indicative of malicious intent. This cannot be removed
	/// except by the registrar.
	Erroneous,
}

impl<Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq>
	Judgement<Balance>
{
	/// Returns `true` if this judgement is indicative of a deposit being currently held. This means
	/// it should not be cleared or replaced except by an operation which utilizes the deposit.
	pub(crate) fn has_deposit(&self) -> bool {
		matches!(self, Judgement::FeePaid(_))
	}

	/// Returns `true` if this judgement is one that should not be generally be replaced outside
	/// of specialized handlers. Examples include "malicious" judgements and deposit-holding
	/// judgements.
	pub(crate) fn is_sticky(&self) -> bool {
		matches!(self, Judgement::FeePaid(_) | Judgement::Erroneous)
	}
}

pub trait IdentityInformationProvider:
	Encode + Decode + MaxEncodedLen + Clone + Debug + Eq + PartialEq + TypeInfo
{
	fn has_identity(&self, fields: u64) -> bool;
}

pub trait IdentityFieldProvider:
	Encode + Decode + MaxEncodedLen + Clone + Debug + Eq + PartialEq + TypeInfo + U64BitFlag
{
}

/// Information concerning the identity of the controller of an account.
///
/// NOTE: This is stored separately primarily to facilitate the addition of extra fields in a
/// backwards compatible way through a specialized `Decode` impl.
#[derive(
	CloneNoBound, Encode, Eq, MaxEncodedLen, PartialEqNoBound, RuntimeDebugNoBound, TypeInfo,
)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(MaxJudgements, MaxAdditionalFields))]
pub struct Registration<
	Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
	MaxJudgements: Get<u32>,
	II: IdentityInformationProvider,
> {
	/// Judgements from the registrars on this identity. Stored ordered by `RegistrarIndex`. There
	/// may be only a single judgement from each registrar.
	pub judgements: BoundedVec<(RegistrarIndex, Judgement<Balance>), MaxJudgements>,

	/// Information on the identity.
	pub info: II,
}

impl<
		Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
		MaxJudgements: Get<u32>,
		II: IdentityInformationProvider,
	> Decode for Registration<Balance, MaxJudgements, II>
{
	fn decode<I: codec::Input>(input: &mut I) -> sp_std::result::Result<Self, codec::Error> {
		let (judgements, info) = Decode::decode(&mut AppendZerosInput::new(input))?;
		Ok(Self { judgements, info })
	}
}

/// Information concerning a registrar.
#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
pub struct RegistrarInfo<
	Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
	AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
	IdField: IdentityFieldProvider,
> {
	/// The account of the registrar.
	pub account: AccountId,

	/// Amount required to be given to the registrar for them to provide judgement.
	pub fee: Balance,

	/// Relevant fields for this registrar. Registrar judgements are limited to attestations on
	/// these fields.
	pub fields: IdentityFields<IdField>,
}

/// Wrapper type for `BitFlags<IdentityField>` that implements `Codec`.
#[derive(Clone, Copy, PartialEq, RuntimeDebug)]
pub struct IdentityFields<IdField: BitFlag>(pub BitFlags<IdField>);

impl<IdField: U64BitFlag> Default for IdentityFields<IdField> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<IdField: U64BitFlag> MaxEncodedLen for IdentityFields<IdField>
where
	IdentityFields<IdField>: Encode,
{
	fn max_encoded_len() -> usize {
		u64::max_encoded_len()
	}
}

impl<IdField: U64BitFlag + PartialEq> Eq for IdentityFields<IdField> {}
impl<IdField: Encode + Decode + U64BitFlag> Encode for IdentityFields<IdField> {
	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		let bits: u64 = self.0.bits();
		bits.using_encoded(f)
	}
}
impl<IdField: Encode + Decode + U64BitFlag> Decode for IdentityFields<IdField> {
	fn decode<I: codec::Input>(input: &mut I) -> sp_std::result::Result<Self, codec::Error> {
		let field = u64::decode(input)?;
		Ok(Self(<BitFlags<IdField>>::from_bits(field).map_err(|_| "invalid value")?))
	}
}
impl<IdField: IdentityFieldProvider> TypeInfo for IdentityFields<IdField> {
	type Identity = Self;

	fn type_info() -> Type {
		Type::builder()
			.path(Path::new("BitFlags", module_path!()))
			.type_params(vec![TypeParameter::new("T", Some(meta_type::<IdField>()))])
			.composite(Fields::unnamed().field(|f| f.ty::<u64>().type_name("IdentityField")))
	}
}
