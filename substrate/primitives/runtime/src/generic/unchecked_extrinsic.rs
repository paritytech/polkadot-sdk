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

//! Generic implementation of an unchecked (pre-verification) extrinsic.

use crate::{
	generic::{CheckedExtrinsic, ExtrinsicFormat},
	traits::{
		self, transaction_extension::TransactionExtension, Checkable, Dispatchable, ExtrinsicLike,
		ExtrinsicMetadata, IdentifyAccount, MaybeDisplay, Member, SignaturePayload,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	OpaqueExtrinsic,
};
#[cfg(all(not(feature = "std"), feature = "serde"))]
use alloc::format;
use alloc::{vec, vec::Vec};
use codec::{Compact, Decode, Encode, EncodeLike, Error, Input};
use core::fmt;
use scale_info::{build::Fields, meta_type, Path, StaticTypeInfo, Type, TypeInfo, TypeParameter};
use sp_io::hashing::blake2_256;
use sp_weights::Weight;

/// Type to represent the version of the [Extension](TransactionExtension) used in this extrinsic.
pub type ExtensionVersion = u8;
/// Type to represent the extrinsic format version which defines an [UncheckedExtrinsic].
pub type ExtrinsicVersion = u8;

/// Current version of the [`UncheckedExtrinsic`] encoded format.
///
/// This version needs to be bumped if the encoded representation changes.
/// It ensures that if the representation is changed and the format is not known,
/// the decoding fails.
pub const EXTRINSIC_FORMAT_VERSION: ExtrinsicVersion = 5;
/// Legacy version of the [`UncheckedExtrinsic`] encoded format.
///
/// This version was used in the signed/unsigned transaction model and is still supported for
/// compatibility reasons. It will be deprecated in favor of v5 extrinsics and an inherent/general
/// transaction model.
pub const LEGACY_EXTRINSIC_FORMAT_VERSION: ExtrinsicVersion = 4;
/// Current version of the [Extension](TransactionExtension) used in this
/// [extrinsic](UncheckedExtrinsic).
///
/// This version needs to be bumped if there are breaking changes to the extension used in the
/// [UncheckedExtrinsic] implementation.
const EXTENSION_VERSION: ExtensionVersion = 0;

/// The `SignaturePayload` of `UncheckedExtrinsic`.
pub type UncheckedSignaturePayload<Address, Signature, Extension> = (Address, Signature, Extension);

impl<Address: TypeInfo, Signature: TypeInfo, Extension: TypeInfo> SignaturePayload
	for UncheckedSignaturePayload<Address, Signature, Extension>
{
	type SignatureAddress = Address;
	type Signature = Signature;
	type SignatureExtra = Extension;
}

/// A "header" for extrinsics leading up to the call itself. Determines the type of extrinsic and
/// holds any necessary specialized data.
#[derive(Eq, PartialEq, Clone)]
pub enum Preamble<Address, Signature, Extension> {
	/// An extrinsic without a signature or any extension. This means it's either an inherent or
	/// an old-school "Unsigned" (we don't use that terminology any more since it's confusable with
	/// the general transaction which is without a signature but does have an extension).
	///
	/// NOTE: In the future, once we remove `ValidateUnsigned`, this will only serve Inherent
	/// extrinsics and thus can be renamed to `Inherent`.
	Bare(ExtrinsicVersion),
	/// An old-school transaction extrinsic which includes a signature of some hard-coded crypto.
	/// Available only on extrinsic version 4.
	Signed(Address, Signature, ExtensionVersion, Extension),
	/// A new-school transaction extrinsic which does not include a signature by default. The
	/// origin authorization, through signatures or other means, is performed by the transaction
	/// extension in this extrinsic. Available starting with extrinsic version 5.
	General(ExtensionVersion, Extension),
}

const VERSION_MASK: u8 = 0b0011_1111;
const TYPE_MASK: u8 = 0b1100_0000;
const BARE_EXTRINSIC: u8 = 0b0000_0000;
const SIGNED_EXTRINSIC: u8 = 0b1000_0000;
const GENERAL_EXTRINSIC: u8 = 0b0100_0000;

impl<Address, Signature, Extension> Decode for Preamble<Address, Signature, Extension>
where
	Address: Decode,
	Signature: Decode,
	Extension: Decode,
{
	fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
		let version_and_type = input.read_byte()?;

		let version = version_and_type & VERSION_MASK;
		let xt_type = version_and_type & TYPE_MASK;

		let preamble = match (version, xt_type) {
			(
				extrinsic_version @ LEGACY_EXTRINSIC_FORMAT_VERSION..=EXTRINSIC_FORMAT_VERSION,
				BARE_EXTRINSIC,
			) => Self::Bare(extrinsic_version),
			(LEGACY_EXTRINSIC_FORMAT_VERSION, SIGNED_EXTRINSIC) => {
				let address = Address::decode(input)?;
				let signature = Signature::decode(input)?;
				let ext = Extension::decode(input)?;
				Self::Signed(address, signature, 0, ext)
			},
			(EXTRINSIC_FORMAT_VERSION, GENERAL_EXTRINSIC) => {
				let ext_version = ExtensionVersion::decode(input)?;
				let ext = Extension::decode(input)?;
				Self::General(ext_version, ext)
			},
			(_, _) => return Err("Invalid transaction version".into()),
		};

		Ok(preamble)
	}
}

impl<Address, Signature, Extension> Encode for Preamble<Address, Signature, Extension>
where
	Address: Encode,
	Signature: Encode,
	Extension: Encode,
{
	fn size_hint(&self) -> usize {
		match &self {
			Preamble::Bare(_) => EXTRINSIC_FORMAT_VERSION.size_hint(),
			Preamble::Signed(address, signature, _, ext) => LEGACY_EXTRINSIC_FORMAT_VERSION
				.size_hint()
				.saturating_add(address.size_hint())
				.saturating_add(signature.size_hint())
				.saturating_add(ext.size_hint()),
			Preamble::General(ext_version, ext) => EXTRINSIC_FORMAT_VERSION
				.size_hint()
				.saturating_add(ext_version.size_hint())
				.saturating_add(ext.size_hint()),
		}
	}

	fn encode_to<T: codec::Output + ?Sized>(&self, dest: &mut T) {
		match &self {
			Preamble::Bare(extrinsic_version) => {
				(extrinsic_version | BARE_EXTRINSIC).encode_to(dest);
			},
			Preamble::Signed(address, signature, _, ext) => {
				(LEGACY_EXTRINSIC_FORMAT_VERSION | SIGNED_EXTRINSIC).encode_to(dest);
				address.encode_to(dest);
				signature.encode_to(dest);
				ext.encode_to(dest);
			},
			Preamble::General(ext_version, ext) => {
				(EXTRINSIC_FORMAT_VERSION | GENERAL_EXTRINSIC).encode_to(dest);
				ext_version.encode_to(dest);
				ext.encode_to(dest);
			},
		}
	}
}

impl<Address, Signature, Extension> Preamble<Address, Signature, Extension> {
	/// Returns `Some` if this is a signed extrinsic, together with the relevant inner fields.
	pub fn to_signed(self) -> Option<(Address, Signature, Extension)> {
		match self {
			Self::Signed(a, s, _, e) => Some((a, s, e)),
			_ => None,
		}
	}
}

impl<Address, Signature, Extension> fmt::Debug for Preamble<Address, Signature, Extension>
where
	Address: fmt::Debug,
	Extension: fmt::Debug,
{
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::Bare(_) => write!(f, "Bare"),
			Self::Signed(address, _, ext_version, tx_ext) =>
				write!(f, "Signed({:?}, {:?}, {:?})", address, ext_version, tx_ext),
			Self::General(ext_version, tx_ext) =>
				write!(f, "General({:?}, {:?})", ext_version, tx_ext),
		}
	}
}

/// An extrinsic right from the external world. This is unchecked and so can contain a signature.
///
/// An extrinsic is formally described as any external data that is originating from the outside of
/// the runtime and fed into the runtime as a part of the block-body.
///
/// Inherents are special types of extrinsics that are placed into the block by the block-builder.
/// They are unsigned because the assertion is that they are "inherently true" by virtue of getting
/// past all validators.
///
/// Transactions are all other statements provided by external entities that the chain deems values
/// and decided to include in the block. This value is typically in the form of fee payment, but it
/// could in principle be any other interaction. Transactions are either signed or unsigned. A
/// sensible transaction pool should ensure that only transactions that are worthwhile are
/// considered for block-building.
#[cfg_attr(all(feature = "std", not(windows)), doc = simple_mermaid::mermaid!("../../docs/mermaid/extrinsics.mmd"))]
/// This type is by no means enforced within Substrate, but given its genericness, it is highly
/// likely that for most use-cases it will suffice. Thus, the encoding of this type will dictate
/// exactly what bytes should be sent to a runtime to transact with it.
///
/// This can be checked using [`Checkable`], yielding a [`CheckedExtrinsic`], which is the
/// counterpart of this type after its signature (and other non-negotiable validity checks) have
/// passed.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct UncheckedExtrinsic<Address, Call, Signature, Extension> {
	/// Information regarding the type of extrinsic this is (inherent or transaction) as well as
	/// associated extension (`Extension`) data if it's a transaction and a possible signature.
	pub preamble: Preamble<Address, Signature, Extension>,
	/// The function that should be called.
	pub function: Call,
}

/// Manual [`TypeInfo`] implementation because of custom encoding. The data is a valid encoded
/// `Vec<u8>`, but requires some logic to extract the signature and payload.
///
/// See [`UncheckedExtrinsic::encode`] and [`UncheckedExtrinsic::decode`].
impl<Address, Call, Signature, Extension> TypeInfo
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Address: StaticTypeInfo,
	Call: StaticTypeInfo,
	Signature: StaticTypeInfo,
	Extension: StaticTypeInfo,
{
	type Identity = UncheckedExtrinsic<Address, Call, Signature, Extension>;

	fn type_info() -> Type {
		Type::builder()
			.path(Path::new("UncheckedExtrinsic", module_path!()))
			// Include the type parameter types, even though they are not used directly in any of
			// the described fields. These type definitions can be used by downstream consumers
			// to help construct the custom decoding from the opaque bytes (see below).
			.type_params(vec![
				TypeParameter::new("Address", Some(meta_type::<Address>())),
				TypeParameter::new("Call", Some(meta_type::<Call>())),
				TypeParameter::new("Signature", Some(meta_type::<Signature>())),
				TypeParameter::new("Extra", Some(meta_type::<Extension>())),
			])
			.docs(&["UncheckedExtrinsic raw bytes, requires custom decoding routine"])
			// Because of the custom encoding, we can only accurately describe the encoding as an
			// opaque `Vec<u8>`. Downstream consumers will need to manually implement the codec to
			// encode/decode the `signature` and `function` fields.
			.composite(Fields::unnamed().field(|f| f.ty::<Vec<u8>>()))
	}
}

impl<Address, Call, Signature, Extension> UncheckedExtrinsic<Address, Call, Signature, Extension> {
	/// New instance of a bare (ne unsigned) extrinsic. This could be used for an inherent or an
	/// old-school "unsigned transaction" (which are new being deprecated in favour of general
	/// transactions).
	#[deprecated = "Use new_bare instead"]
	pub fn new_unsigned(function: Call) -> Self {
		Self::new_bare(function)
	}

	/// Returns `true` if this extrinsic instance is an inherent, `false`` otherwise.
	pub fn is_inherent(&self) -> bool {
		matches!(self.preamble, Preamble::Bare(_))
	}

	/// Returns `true` if this extrinsic instance is an old-school signed transaction, `false`
	/// otherwise.
	pub fn is_signed(&self) -> bool {
		matches!(self.preamble, Preamble::Signed(..))
	}

	/// Create an `UncheckedExtrinsic` from a `Preamble` and the actual `Call`.
	pub fn from_parts(function: Call, preamble: Preamble<Address, Signature, Extension>) -> Self {
		Self { preamble, function }
	}

	/// New instance of a bare (ne unsigned) extrinsic.
	pub fn new_bare(function: Call) -> Self {
		Self { preamble: Preamble::Bare(EXTRINSIC_FORMAT_VERSION), function }
	}

	/// New instance of a bare (ne unsigned) extrinsic on extrinsic format version 4.
	pub fn new_bare_legacy(function: Call) -> Self {
		Self { preamble: Preamble::Bare(LEGACY_EXTRINSIC_FORMAT_VERSION), function }
	}

	/// New instance of an old-school signed transaction on extrinsic format version 4.
	pub fn new_signed(
		function: Call,
		signed: Address,
		signature: Signature,
		tx_ext: Extension,
	) -> Self {
		Self { preamble: Preamble::Signed(signed, signature, 0, tx_ext), function }
	}

	/// New instance of an new-school unsigned transaction.
	pub fn new_transaction(function: Call, tx_ext: Extension) -> Self {
		Self { preamble: Preamble::General(EXTENSION_VERSION, tx_ext), function }
	}
}

impl<Address: TypeInfo, Call: TypeInfo, Signature: TypeInfo, Extension: TypeInfo> ExtrinsicLike
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
{
	fn is_bare(&self) -> bool {
		matches!(self.preamble, Preamble::Bare(_))
	}

	fn is_signed(&self) -> Option<bool> {
		Some(matches!(self.preamble, Preamble::Signed(..)))
	}
}

// TODO: Migrate existing extension pipelines to support current `Signed` transactions as `General`
// transactions by adding an extension to validate signatures, as they are currently validated in
// the `Checkable` implementation for `Signed` transactions.

impl<LookupSource, AccountId, Call, Signature, Extension, Lookup> Checkable<Lookup>
	for UncheckedExtrinsic<LookupSource, Call, Signature, Extension>
where
	LookupSource: Member + MaybeDisplay,
	Call: Encode + Member + Dispatchable,
	Signature: Member + traits::Verify,
	<Signature as traits::Verify>::Signer: IdentifyAccount<AccountId = AccountId>,
	Extension: Encode + TransactionExtension<Call>,
	AccountId: Member + MaybeDisplay,
	Lookup: traits::Lookup<Source = LookupSource, Target = AccountId>,
{
	type Checked = CheckedExtrinsic<AccountId, Call, Extension>;

	fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
		Ok(match self.preamble {
			Preamble::Signed(signed, signature, _, tx_ext) => {
				let signed = lookup.lookup(signed)?;
				// The `Implicit` is "implicitly" included in the payload.
				let raw_payload = SignedPayload::new(self.function, tx_ext)?;
				if !raw_payload.using_encoded(|payload| signature.verify(payload, &signed)) {
					return Err(InvalidTransaction::BadProof.into())
				}
				let (function, tx_ext, _) = raw_payload.deconstruct();
				CheckedExtrinsic { format: ExtrinsicFormat::Signed(signed, tx_ext), function }
			},
			Preamble::General(_, tx_ext) => CheckedExtrinsic {
				format: ExtrinsicFormat::General(tx_ext),
				function: self.function,
			},
			Preamble::Bare(_) =>
				CheckedExtrinsic { format: ExtrinsicFormat::Bare, function: self.function },
		})
	}

	#[cfg(feature = "try-runtime")]
	fn unchecked_into_checked_i_know_what_i_am_doing(
		self,
		lookup: &Lookup,
	) -> Result<Self::Checked, TransactionValidityError> {
		Ok(match self.preamble {
			Preamble::Signed(signed, _, _, extra) => {
				let signed = lookup.lookup(signed)?;
				CheckedExtrinsic {
					format: ExtrinsicFormat::Signed(signed, extra),
					function: self.function,
				}
			},
			Preamble::General(_, extra) => CheckedExtrinsic {
				format: ExtrinsicFormat::General(extra),
				function: self.function,
			},
			Preamble::Bare(_) =>
				CheckedExtrinsic { format: ExtrinsicFormat::Bare, function: self.function },
		})
	}
}

impl<Address, Call: Dispatchable, Signature, Extension: TransactionExtension<Call>>
	ExtrinsicMetadata for UncheckedExtrinsic<Address, Call, Signature, Extension>
{
	// TODO: Expose both version 4 and version 5 in metadata v16.
	const VERSION: u8 = LEGACY_EXTRINSIC_FORMAT_VERSION;
	type TransactionExtensions = Extension;
}

impl<Address, Call: Dispatchable, Signature, Extension: TransactionExtension<Call>>
	UncheckedExtrinsic<Address, Call, Signature, Extension>
{
	/// Returns the weight of the extension of this transaction, if present. If the transaction
	/// doesn't use any extension, the weight returned is equal to zero.
	pub fn extension_weight(&self) -> Weight {
		match &self.preamble {
			Preamble::Bare(_) => Weight::zero(),
			Preamble::Signed(_, _, _, ext) | Preamble::General(_, ext) =>
				ext.weight(&self.function),
		}
	}
}

impl<Address, Call, Signature, Extension> Decode
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Address: Decode,
	Signature: Decode,
	Call: Decode,
	Extension: Decode,
{
	fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
		// This is a little more complicated than usual since the binary format must be compatible
		// with SCALE's generic `Vec<u8>` type. Basically this just means accepting that there
		// will be a prefix of vector length.
		let expected_length: Compact<u32> = Decode::decode(input)?;
		let before_length = input.remaining_len()?;

		let preamble = Decode::decode(input)?;
		let function = Decode::decode(input)?;

		if let Some((before_length, after_length)) =
			input.remaining_len()?.and_then(|a| before_length.map(|b| (b, a)))
		{
			let length = before_length.saturating_sub(after_length);

			if length != expected_length.0 as usize {
				return Err("Invalid length prefix".into())
			}
		}

		Ok(Self { preamble, function })
	}
}

#[docify::export(unchecked_extrinsic_encode_impl)]
impl<Address, Call, Signature, Extension> Encode
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Preamble<Address, Signature, Extension>: Encode,
	Call: Encode,
	Extension: Encode,
{
	fn encode(&self) -> Vec<u8> {
		let mut tmp = self.preamble.encode();
		self.function.encode_to(&mut tmp);

		let compact_len = codec::Compact::<u32>(tmp.len() as u32);

		// Allocate the output buffer with the correct length
		let mut output = Vec::with_capacity(compact_len.size_hint() + tmp.len());

		compact_len.encode_to(&mut output);
		output.extend(tmp);

		output
	}
}

impl<Address, Call, Signature, Extension> EncodeLike
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
where
	Address: Encode,
	Signature: Encode,
	Call: Encode + Dispatchable,
	Extension: TransactionExtension<Call>,
{
}

#[cfg(feature = "serde")]
impl<Address: Encode, Signature: Encode, Call: Encode, Extension: Encode> serde::Serialize
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
{
	fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error>
	where
		S: ::serde::Serializer,
	{
		self.using_encoded(|bytes| seq.serialize_bytes(bytes))
	}
}

#[cfg(feature = "serde")]
impl<'a, Address: Decode, Signature: Decode, Call: Decode, Extension: Decode> serde::Deserialize<'a>
	for UncheckedExtrinsic<Address, Call, Signature, Extension>
{
	fn deserialize<D>(de: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'a>,
	{
		let r = sp_core::bytes::deserialize(de)?;
		Self::decode(&mut &r[..])
			.map_err(|e| serde::de::Error::custom(format!("Decode error: {}", e)))
	}
}

/// A payload that has been signed for an unchecked extrinsics.
///
/// Note that the payload that we sign to produce unchecked extrinsic signature
/// is going to be different than the `SignaturePayload` - so the thing the extrinsic
/// actually contains.
pub struct SignedPayload<Call: Dispatchable, Extension: TransactionExtension<Call>>(
	(Call, Extension, Extension::Implicit),
);

impl<Call, Extension> SignedPayload<Call, Extension>
where
	Call: Encode + Dispatchable,
	Extension: TransactionExtension<Call>,
{
	/// Create new `SignedPayload` for extrinsic format version 4.
	///
	/// This function may fail if `implicit` of `Extension` is not available.
	pub fn new(call: Call, tx_ext: Extension) -> Result<Self, TransactionValidityError> {
		let implicit = Extension::implicit(&tx_ext)?;
		let raw_payload = (call, tx_ext, implicit);
		Ok(Self(raw_payload))
	}

	/// Create new `SignedPayload` from raw components.
	pub fn from_raw(call: Call, tx_ext: Extension, implicit: Extension::Implicit) -> Self {
		Self((call, tx_ext, implicit))
	}

	/// Deconstruct the payload into it's components.
	pub fn deconstruct(self) -> (Call, Extension, Extension::Implicit) {
		self.0
	}
}

impl<Call, Extension> Encode for SignedPayload<Call, Extension>
where
	Call: Encode + Dispatchable,
	Extension: TransactionExtension<Call>,
{
	/// Get an encoded version of this `blake2_256`-hashed payload.
	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		self.0.using_encoded(|payload| {
			if payload.len() > 256 {
				f(&blake2_256(payload)[..])
			} else {
				f(payload)
			}
		})
	}
}

impl<Call, Extension> EncodeLike for SignedPayload<Call, Extension>
where
	Call: Encode + Dispatchable,
	Extension: TransactionExtension<Call>,
{
}

impl<Address, Call, Signature, Extension>
	From<UncheckedExtrinsic<Address, Call, Signature, Extension>> for OpaqueExtrinsic
where
	Address: Encode,
	Signature: Encode,
	Call: Encode,
	Extension: Encode,
{
	fn from(extrinsic: UncheckedExtrinsic<Address, Call, Signature, Extension>) -> Self {
		Self::from_bytes(extrinsic.encode().as_slice()).expect(
			"both OpaqueExtrinsic and UncheckedExtrinsic have encoding that is compatible with \
				raw Vec<u8> encoding; qed",
		)
	}
}

#[cfg(test)]
mod legacy {
	use codec::{Compact, Decode, Encode, EncodeLike, Error, Input};
	use scale_info::{
		build::Fields, meta_type, Path, StaticTypeInfo, Type, TypeInfo, TypeParameter,
	};

	pub type UncheckedSignaturePayloadV4<Address, Signature, Extra> = (Address, Signature, Extra);

	#[derive(PartialEq, Eq, Clone, Debug)]
	pub struct UncheckedExtrinsicV4<Address, Call, Signature, Extra> {
		pub signature: Option<UncheckedSignaturePayloadV4<Address, Signature, Extra>>,
		pub function: Call,
	}

	impl<Address, Call, Signature, Extra> TypeInfo
		for UncheckedExtrinsicV4<Address, Call, Signature, Extra>
	where
		Address: StaticTypeInfo,
		Call: StaticTypeInfo,
		Signature: StaticTypeInfo,
		Extra: StaticTypeInfo,
	{
		type Identity = UncheckedExtrinsicV4<Address, Call, Signature, Extra>;

		fn type_info() -> Type {
			Type::builder()
				.path(Path::new("UncheckedExtrinsic", module_path!()))
				// Include the type parameter types, even though they are not used directly in any
				// of the described fields. These type definitions can be used by downstream
				// consumers to help construct the custom decoding from the opaque bytes (see
				// below).
				.type_params(vec![
					TypeParameter::new("Address", Some(meta_type::<Address>())),
					TypeParameter::new("Call", Some(meta_type::<Call>())),
					TypeParameter::new("Signature", Some(meta_type::<Signature>())),
					TypeParameter::new("Extra", Some(meta_type::<Extra>())),
				])
				.docs(&["OldUncheckedExtrinsic raw bytes, requires custom decoding routine"])
				// Because of the custom encoding, we can only accurately describe the encoding as
				// an opaque `Vec<u8>`. Downstream consumers will need to manually implement the
				// codec to encode/decode the `signature` and `function` fields.
				.composite(Fields::unnamed().field(|f| f.ty::<Vec<u8>>()))
		}
	}

	impl<Address, Call, Signature, Extra> UncheckedExtrinsicV4<Address, Call, Signature, Extra> {
		pub fn new_signed(
			function: Call,
			signed: Address,
			signature: Signature,
			extra: Extra,
		) -> Self {
			Self { signature: Some((signed, signature, extra)), function }
		}

		pub fn new_unsigned(function: Call) -> Self {
			Self { signature: None, function }
		}
	}

	impl<Address, Call, Signature, Extra> Decode
		for UncheckedExtrinsicV4<Address, Call, Signature, Extra>
	where
		Address: Decode,
		Signature: Decode,
		Call: Decode,
		Extra: Decode,
	{
		fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
			// This is a little more complicated than usual since the binary format must be
			// compatible with SCALE's generic `Vec<u8>` type. Basically this just means accepting
			// that there will be a prefix of vector length.
			let expected_length: Compact<u32> = Decode::decode(input)?;
			let before_length = input.remaining_len()?;

			let version = input.read_byte()?;

			let is_signed = version & 0b1000_0000 != 0;
			let version = version & 0b0111_1111;
			if version != 4u8 {
				return Err("Invalid transaction version".into())
			}

			let signature = is_signed.then(|| Decode::decode(input)).transpose()?;
			let function = Decode::decode(input)?;

			if let Some((before_length, after_length)) =
				input.remaining_len()?.and_then(|a| before_length.map(|b| (b, a)))
			{
				let length = before_length.saturating_sub(after_length);

				if length != expected_length.0 as usize {
					return Err("Invalid length prefix".into())
				}
			}

			Ok(Self { signature, function })
		}
	}

	#[docify::export(unchecked_extrinsic_encode_impl)]
	impl<Address, Call, Signature, Extra> Encode
		for UncheckedExtrinsicV4<Address, Call, Signature, Extra>
	where
		Address: Encode,
		Signature: Encode,
		Call: Encode,
		Extra: Encode,
	{
		fn encode(&self) -> Vec<u8> {
			let mut tmp = Vec::with_capacity(sp_std::mem::size_of::<Self>());

			// 1 byte version id.
			match self.signature.as_ref() {
				Some(s) => {
					tmp.push(4u8 | 0b1000_0000);
					s.encode_to(&mut tmp);
				},
				None => {
					tmp.push(4u8 & 0b0111_1111);
				},
			}
			self.function.encode_to(&mut tmp);

			let compact_len = codec::Compact::<u32>(tmp.len() as u32);

			// Allocate the output buffer with the correct length
			let mut output = Vec::with_capacity(compact_len.size_hint() + tmp.len());

			compact_len.encode_to(&mut output);
			output.extend(tmp);

			output
		}
	}

	impl<Address, Call, Signature, Extra> EncodeLike
		for UncheckedExtrinsicV4<Address, Call, Signature, Extra>
	where
		Address: Encode,
		Signature: Encode,
		Call: Encode,
		Extra: Encode,
	{
	}
}

#[cfg(test)]
mod tests {
	use super::{legacy::UncheckedExtrinsicV4, *};
	use crate::{
		codec::{Decode, Encode},
		impl_tx_ext_default,
		testing::TestSignature as TestSig,
		traits::{FakeDispatchable, IdentityLookup, TransactionExtension},
	};
	use sp_io::hashing::blake2_256;

	type TestContext = IdentityLookup<u64>;
	type TestAccountId = u64;
	type TestCall = FakeDispatchable<Vec<u8>>;

	const TEST_ACCOUNT: TestAccountId = 0;

	// NOTE: this is demonstration. One can simply use `()` for testing.
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, Ord, PartialOrd, TypeInfo)]
	struct DummyExtension;
	impl TransactionExtension<TestCall> for DummyExtension {
		const IDENTIFIER: &'static str = "DummyExtension";
		type Implicit = ();
		type Val = ();
		type Pre = ();
		impl_tx_ext_default!(TestCall; weight validate prepare);
	}

	type Ex = UncheckedExtrinsic<TestAccountId, TestCall, TestSig, DummyExtension>;
	type CEx = CheckedExtrinsic<TestAccountId, TestCall, DummyExtension>;

	#[test]
	fn unsigned_codec_should_work() {
		let call: TestCall = vec![0u8; 0].into();
		let ux = Ex::new_bare(call);
		let encoded = ux.encode();
		assert_eq!(Ex::decode(&mut &encoded[..]), Ok(ux));
	}

	#[test]
	fn invalid_length_prefix_is_detected() {
		let ux = Ex::new_bare(vec![0u8; 0].into());
		let mut encoded = ux.encode();

		let length = Compact::<u32>::decode(&mut &encoded[..]).unwrap();
		Compact(length.0 + 10).encode_to(&mut &mut encoded[..1]);

		assert_eq!(Ex::decode(&mut &encoded[..]), Err("Invalid length prefix".into()));
	}

	#[test]
	fn transaction_codec_should_work() {
		let ux = Ex::new_transaction(vec![0u8; 0].into(), DummyExtension);
		let encoded = ux.encode();
		assert_eq!(Ex::decode(&mut &encoded[..]), Ok(ux));
	}

	#[test]
	fn signed_codec_should_work() {
		let ux = Ex::new_signed(
			vec![0u8; 0].into(),
			TEST_ACCOUNT,
			TestSig(TEST_ACCOUNT, (vec![0u8; 0], DummyExtension).encode()),
			DummyExtension,
		);
		let encoded = ux.encode();
		assert_eq!(Ex::decode(&mut &encoded[..]), Ok(ux));
	}

	#[test]
	fn large_signed_codec_should_work() {
		let ux = Ex::new_signed(
			vec![0u8; 0].into(),
			TEST_ACCOUNT,
			TestSig(
				TEST_ACCOUNT,
				(vec![0u8; 257], DummyExtension).using_encoded(blake2_256)[..].to_owned(),
			),
			DummyExtension,
		);
		let encoded = ux.encode();
		assert_eq!(Ex::decode(&mut &encoded[..]), Ok(ux));
	}

	#[test]
	fn unsigned_check_should_work() {
		let ux = Ex::new_bare(vec![0u8; 0].into());
		assert!(ux.is_inherent());
		assert_eq!(
			<Ex as Checkable<TestContext>>::check(ux, &Default::default()),
			Ok(CEx { format: ExtrinsicFormat::Bare, function: vec![0u8; 0].into() }),
		);
	}

	#[test]
	fn badly_signed_check_should_fail() {
		let ux = Ex::new_signed(
			vec![0u8; 0].into(),
			TEST_ACCOUNT,
			TestSig(TEST_ACCOUNT, vec![0u8; 0].into()),
			DummyExtension,
		);
		assert!(!ux.is_inherent());
		assert_eq!(
			<Ex as Checkable<TestContext>>::check(ux, &Default::default()),
			Err(InvalidTransaction::BadProof.into()),
		);
	}

	#[test]
	fn transaction_check_should_work() {
		let ux = Ex::new_transaction(vec![0u8; 0].into(), DummyExtension);
		assert!(!ux.is_inherent());
		assert_eq!(
			<Ex as Checkable<TestContext>>::check(ux, &Default::default()),
			Ok(CEx {
				format: ExtrinsicFormat::General(DummyExtension),
				function: vec![0u8; 0].into()
			}),
		);
	}

	#[test]
	fn signed_check_should_work() {
		let sig_payload = SignedPayload::from_raw(
			FakeDispatchable::from(vec![0u8; 0]),
			DummyExtension,
			DummyExtension.implicit().unwrap(),
		);
		let ux = Ex::new_signed(
			vec![0u8; 0].into(),
			TEST_ACCOUNT,
			TestSig(TEST_ACCOUNT, sig_payload.encode()),
			DummyExtension,
		);
		assert!(!ux.is_inherent());
		assert_eq!(
			<Ex as Checkable<TestContext>>::check(ux, &Default::default()),
			Ok(CEx {
				format: ExtrinsicFormat::Signed(TEST_ACCOUNT, DummyExtension),
				function: vec![0u8; 0].into()
			}),
		);
	}

	#[test]
	fn encoding_matches_vec() {
		let ex = Ex::new_bare(vec![0u8; 0].into());
		let encoded = ex.encode();
		let decoded = Ex::decode(&mut encoded.as_slice()).unwrap();
		assert_eq!(decoded, ex);
		let as_vec: Vec<u8> = Decode::decode(&mut encoded.as_slice()).unwrap();
		assert_eq!(as_vec.encode(), encoded);
	}

	#[test]
	fn conversion_to_opaque() {
		let ux = Ex::new_bare(vec![0u8; 0].into());
		let encoded = ux.encode();
		let opaque: OpaqueExtrinsic = ux.into();
		let opaque_encoded = opaque.encode();
		assert_eq!(opaque_encoded, encoded);
	}

	#[test]
	fn large_bad_prefix_should_work() {
		let encoded = (Compact::<u32>::from(u32::MAX), Preamble::<(), (), ()>::Bare(0)).encode();
		assert!(Ex::decode(&mut &encoded[..]).is_err());
	}

	#[test]
	fn legacy_short_signed_encode_decode() {
		let call: TestCall = vec![0u8; 4].into();
		let signed = TEST_ACCOUNT;
		let extension = DummyExtension;
		let implicit = extension.implicit().unwrap();
		let legacy_signature = TestSig(TEST_ACCOUNT, (&call, &extension, &implicit).encode());

		let old_ux =
			UncheckedExtrinsicV4::<TestAccountId, TestCall, TestSig, DummyExtension>::new_signed(
				call.clone(),
				signed,
				legacy_signature.clone(),
				extension.clone(),
			);

		let encoded_old_ux = old_ux.encode();
		let decoded_old_ux = Ex::decode(&mut &encoded_old_ux[..]).unwrap();

		assert_eq!(decoded_old_ux.function, call);
		assert_eq!(
			decoded_old_ux.preamble,
			Preamble::Signed(signed, legacy_signature.clone(), 0, extension.clone())
		);

		let new_ux =
			Ex::new_signed(call.clone(), signed, legacy_signature.clone(), extension.clone());

		let new_checked = new_ux.check(&IdentityLookup::<TestAccountId>::default()).unwrap();
		let old_checked =
			decoded_old_ux.check(&IdentityLookup::<TestAccountId>::default()).unwrap();
		assert_eq!(new_checked, old_checked);
	}

	#[test]
	fn legacy_long_signed_encode_decode() {
		let call: TestCall = vec![0u8; 257].into();
		let signed = TEST_ACCOUNT;
		let extension = DummyExtension;
		let implicit = extension.implicit().unwrap();
		let signature = TestSig(
			TEST_ACCOUNT,
			blake2_256(&(&call, DummyExtension, &implicit).encode()[..]).to_vec(),
		);

		let old_ux =
			UncheckedExtrinsicV4::<TestAccountId, TestCall, TestSig, DummyExtension>::new_signed(
				call.clone(),
				signed,
				signature.clone(),
				extension.clone(),
			);

		let encoded_old_ux = old_ux.encode();
		let decoded_old_ux = Ex::decode(&mut &encoded_old_ux[..]).unwrap();

		assert_eq!(decoded_old_ux.function, call);
		assert_eq!(
			decoded_old_ux.preamble,
			Preamble::Signed(signed, signature.clone(), 0, extension.clone())
		);

		let new_ux = Ex::new_signed(call.clone(), signed, signature.clone(), extension.clone());

		let new_checked = new_ux.check(&IdentityLookup::<TestAccountId>::default()).unwrap();
		let old_checked =
			decoded_old_ux.check(&IdentityLookup::<TestAccountId>::default()).unwrap();
		assert_eq!(new_checked, old_checked);
	}

	#[test]
	fn legacy_unsigned_encode_decode() {
		let call: TestCall = vec![0u8; 0].into();

		let old_ux =
			UncheckedExtrinsicV4::<TestAccountId, TestCall, TestSig, DummyExtension>::new_unsigned(
				call.clone(),
			);

		let encoded_old_ux = old_ux.encode();
		let decoded_old_ux = Ex::decode(&mut &encoded_old_ux[..]).unwrap();

		assert_eq!(decoded_old_ux.function, call);
		assert_eq!(decoded_old_ux.preamble, Preamble::Bare(LEGACY_EXTRINSIC_FORMAT_VERSION));

		let new_legacy_ux = Ex::new_bare_legacy(call.clone());
		assert_eq!(encoded_old_ux, new_legacy_ux.encode());

		let new_ux = Ex::new_bare(call.clone());
		let encoded_new_ux = new_ux.encode();
		let decoded_new_ux = Ex::decode(&mut &encoded_new_ux[..]).unwrap();
		assert_eq!(new_ux, decoded_new_ux);

		let new_checked = new_ux.check(&IdentityLookup::<TestAccountId>::default()).unwrap();
		let old_checked =
			decoded_old_ux.check(&IdentityLookup::<TestAccountId>::default()).unwrap();
		assert_eq!(new_checked, old_checked);
	}
}
