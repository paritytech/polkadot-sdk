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

use codec::{Decode, Encode, MaxEncodedLen};
use enumflags2::{bitflags, BitFlags};
use scale_info::{build::Variants, Path, Type, TypeInfo};
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

use crate::types::{Data, IdentityFieldProvider, IdentityFields, U64BitFlag};

/// The fields that we use to identify the owner of an account with. Each corresponds to a field
/// in the `IdentityInfo` struct.
#[bitflags]
#[repr(u64)]
#[derive(MaxEncodedLen, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
pub enum IdentityField {
	Display = 1u64 << 0,
	Legal = 1u64 << 1,
	Web = 1u64 << 2,
	Riot = 1u64 << 3,
	Email = 1u64 << 4,
	PgpFingerprint = 1u64 << 5,
	Image = 1u64 << 6,
	Twitter = 1u64 << 7,
}

impl TypeInfo for IdentityField {
	type Identity = Self;

	fn type_info() -> scale_info::Type {
		Type::builder().path(Path::new("IdentityField", module_path!())).variant(
			Variants::new()
				.variant("Display", |v| v.index(0))
				.variant("Legal", |v| v.index(1))
				.variant("Web", |v| v.index(2))
				.variant("Riot", |v| v.index(3))
				.variant("Email", |v| v.index(4))
				.variant("PgpFingerprint", |v| v.index(5))
				.variant("Image", |v| v.index(6))
				.variant("Twitter", |v| v.index(7)),
		)
	}
}

impl Encode for IdentityField {
	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		let x: u8 = match self {
			IdentityField::Display => 0,
			IdentityField::Legal => 1,
			IdentityField::Web => 2,
			IdentityField::Riot => 3,
			IdentityField::Email => 4,
			IdentityField::PgpFingerprint => 5,
			IdentityField::Image => 6,
			IdentityField::Twitter => 7,
		};
		f(&x.encode())
	}
}

impl Decode for IdentityField {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		match u8::decode(input) {
			Ok(0) => Ok(IdentityField::Display),
			Ok(1) => Ok(IdentityField::Legal),
			Ok(2) => Ok(IdentityField::Web),
			Ok(3) => Ok(IdentityField::Riot),
			Ok(4) => Ok(IdentityField::Email),
			Ok(5) => Ok(IdentityField::PgpFingerprint),
			Ok(6) => Ok(IdentityField::Image),
			Ok(7) => Ok(IdentityField::Twitter),
			_ => Err("Invalid IdentityField representation".into()),
		}
	}
}

impl U64BitFlag for IdentityField {}
impl IdentityFieldProvider for IdentityField {}

/// Information concerning the identity of the controller of an account.
///
/// NOTE: This should be stored at the end of the storage item to facilitate the addition of extra
/// fields in a backwards compatible way through a specialized `Decode` impl.
#[derive(Clone, Encode, Decode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
#[codec(mel_bound())]
#[cfg_attr(test, derive(frame_support::Default))]
pub struct IdentityInfo {
	/// A reasonable display name for the controller of the account. This should be whatever it is
	/// that it is typically known as and should not be confusable with other entities, given
	/// reasonable context.
	///
	/// Stored as UTF-8.
	pub display: Data,

	/// The full legal name in the local jurisdiction of the entity. This might be a bit
	/// long-winded.
	///
	/// Stored as UTF-8.
	pub legal: Data,

	/// A representative website held by the controller of the account.
	///
	/// NOTE: `https://` is automatically prepended.
	///
	/// Stored as UTF-8.
	pub web: Data,

	/// The Riot/Matrix handle held by the controller of the account.
	///
	/// Stored as UTF-8.
	pub riot: Data,

	/// The email address of the controller of the account.
	///
	/// Stored as UTF-8.
	pub email: Data,

	/// The PGP/GPG public key of the controller of the account.
	pub pgp_fingerprint: Option<[u8; 20]>,

	/// A graphic image representing the controller of the account. Should be a company,
	/// organization or project logo or a headshot in the case of a human.
	pub image: Data,

	/// The Twitter identity. The leading `@` character may be elided.
	pub twitter: Data,
}

impl IdentityInfo {
	#[allow(unused)]
	pub(crate) fn fields(&self) -> IdentityFields<IdentityField> {
		let mut res = <BitFlags<IdentityField>>::empty();
		if !self.display.is_none() {
			res.insert(IdentityField::Display);
		}
		if !self.legal.is_none() {
			res.insert(IdentityField::Legal);
		}
		if !self.web.is_none() {
			res.insert(IdentityField::Web);
		}
		if !self.riot.is_none() {
			res.insert(IdentityField::Riot);
		}
		if !self.email.is_none() {
			res.insert(IdentityField::Email);
		}
		if self.pgp_fingerprint.is_some() {
			res.insert(IdentityField::PgpFingerprint);
		}
		if !self.image.is_none() {
			res.insert(IdentityField::Image);
		}
		if !self.twitter.is_none() {
			res.insert(IdentityField::Twitter);
		}
		IdentityFields(res)
	}
}
