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
		self, Checkable, Extrinsic, ExtrinsicMetadata, IdentifyAccount, MaybeDisplay, Member,
		SignaturePayload, TransactionExtension, AdditionalSigned,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	OpaqueExtrinsic,
};
use codec::{Compact, Decode, Encode, EncodeLike, Error, Input};
use scale_info::{build::Fields, meta_type, Path, StaticTypeInfo, Type, TypeInfo, TypeParameter};
use sp_io::hashing::blake2_256;
#[cfg(all(not(feature = "std"), feature = "serde"))]
use sp_std::alloc::format;
use sp_std::{fmt, prelude::*};

/// Current version of the [`UncheckedExtrinsic`] encoded format.
///
/// This version needs to be bumped if the encoded representation changes.
/// It ensures that if the representation is changed and the format is not known,
/// the decoding fails.
const EXTRINSIC_FORMAT_VERSION: u8 = 4;

/// The `SignaturePayload` of `UncheckedExtrinsic`.
type UncheckedSignaturePayload<Address, Signature, Extensions> = (Address, Signature, Extensions);

impl<Address: TypeInfo, Signature: TypeInfo, Extensions: TypeInfo> SignaturePayload
	for UncheckedSignaturePayload<Address, Signature, Extensions>
{
	type SignatureAddress = Address;
	type Signature = Signature;
	type SignatureExtra = Extensions;
}

/// TODO: docs
#[derive(Eq, PartialEq, Clone, Encode, Decode)]
pub enum Preamble<Address, Signature, Extensions> {
	/// An extrinsic without a signature or any extension. This means it's either an inherent or
	/// an old-school "Unsigned" (we don't use that terminology any more since it's confusable with
	/// the general transaction which is without a signature but does have an extension).
	///
	/// NOTE: In the future, once we remove `ValidateUnsigned`, this will only serve Inherent
	/// extrinsics and thus can be renamed to `Inherent`.
	#[codec(index = 0b00000100)]
	Bare,
	/// An old-school transaction extrinsic which includes a signature of some hard-coded crypto.
	#[codec(index = 0b10000100)]
	Signed(Address, Signature, Extensions),
	/// A new-school transaction extrinsic which does not include a signature.
	#[codec(index = 0b01000100)]
	General(Extensions),
}

impl<Address, Signature, Extensions> Preamble<Address, Signature, Extensions> {
	/// Returns `Some` if this is a signed extrinsic, together with the relevant inner fields.
	pub fn to_signed(self) -> Option<(Address, Signature, Extensions)> {
		match self {
			Self::Signed(a, s, e) => Some((a, s, e)),
			_ => None,
		}
	}
}

impl<Address, Signature, Extensions> fmt::Debug for Preamble<Address, Signature, Extensions> where
	Address: fmt::Debug,
	Extensions: TransactionExtension,
{
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::Bare => write!(f, "Bare"),
			Self::Signed(address, _, extra) => write!(f, "Signed({:?}, {:?})", address, extra),
			Self::General(extra) => write!(f, "General({:?})", extra),
		}
	}
}

/// A extrinsic right from the external world. This is unchecked and so
/// can contain a signature.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct UncheckedExtrinsic<Address, Call, Signature, Extensions>
where
	Extensions: TransactionExtension,
{
	/// Information regarding the type of extrinsic this is (inherent or transaction) as well as
	/// associated extension (`Extensions`) data if it's a transaction and a possible signature.
	pub preamble: Preamble<Address, Signature, Extensions>,
	/// The function that should be called.
	pub function: Call,
}

/// Manual [`TypeInfo`] implementation because of custom encoding. The data is a valid encoded
/// `Vec<u8>`, but requires some logic to extract the signature and payload.
///
/// See [`UncheckedExtrinsic::encode`] and [`UncheckedExtrinsic::decode`].
impl<Address, Call, Signature, Extensions> TypeInfo
	for UncheckedExtrinsic<Address, Call, Signature, Extensions>
where
	Address: StaticTypeInfo,
	Call: StaticTypeInfo,
	Signature: StaticTypeInfo,
	Extensions: TransactionExtension + StaticTypeInfo,
{
	type Identity = UncheckedExtrinsic<Address, Call, Signature, Extensions>;

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
				TypeParameter::new("Extra", Some(meta_type::<Extensions>())),
			])
			.docs(&["UncheckedExtrinsic raw bytes, requires custom decoding routine"])
			// Because of the custom encoding, we can only accurately describe the encoding as an
			// opaque `Vec<u8>`. Downstream consumers will need to manually implement the codec to
			// encode/decode the `signature` and `function` fields.
			.composite(Fields::unnamed().field(|f| f.ty::<Vec<u8>>()))
	}
}

impl<Address, Call, Signature, Extensions: TransactionExtension>
	UncheckedExtrinsic<Address, Call, Signature, Extensions>
{
	/// New instance of a bare (ne unsigned) extrinsic. This could be used for an inherent or an
	/// old-school "unsigned transaction" (which are new being deprecated in favour of general
	/// transactions).
	#[deprecated = "Use new_bare instead"]
	pub fn new_unsigned(function: Call) -> Self {
		Self::new_bare(function)
	}

	/// TODO: docs
	pub fn is_inherent(&self) -> bool {
		matches!(self.preamble, Preamble::Bare)
	}

	/// TODO: docs
	pub fn from_parts(function: Call, preamble: Preamble<Address, Signature, Extensions>) -> Self {
		Self { preamble, function }
	}

	/// New instance of a bare (ne unsigned) extrinsic.
	pub fn new_bare(function: Call) -> Self {
		Self { preamble: Preamble::Bare, function }
	}

	/// New instance of an old-school signed transaction.
	pub fn new_signed(function: Call, signed: Address, signature: Signature, tx_ext: Extensions) -> Self {
		Self { preamble: Preamble::Signed(signed, signature, tx_ext), function }
	}

	/// New instance of an new-school unsigned transaction.
	pub fn new_transaction(function: Call, tx_ext: Extensions) -> Self {
		Self { preamble: Preamble::General(tx_ext), function }
	}
}

// TODO: We can get rid of this trait and just use UncheckedExtrinsic directly.

impl<Address: TypeInfo, Call: TypeInfo, Signature: TypeInfo, Extensions: TransactionExtension + TypeInfo>
	Extrinsic for UncheckedExtrinsic<Address, Call, Signature, Extensions>
{
	type Call = Call;

	type SignaturePayload = UncheckedSignaturePayload<Address, Signature, Extensions>;

	fn is_bare(&self) -> bool {
		matches!(self.preamble, Preamble::Bare)
	}

	fn is_signed(&self) -> Option<bool> {
		Some(matches!(self.preamble, Preamble::Signed(..)))
	}

	fn new(function: Call, signed_data: Option<Self::SignaturePayload>) -> Option<Self> {
		Some(if let Some((address, signature, extra)) = signed_data {
			Self::new_signed(function, address, signature, extra)
		} else {
			Self::new_bare(function)
		})
	}

	fn new_inherent(function: Call) -> Self {
		Self::new_bare(function)
	}
}

impl<LookupSource, AccountId, Call, Signature, Extensions, Lookup> Checkable<Lookup>
	for UncheckedExtrinsic<LookupSource, Call, Signature, Extensions>
where
	LookupSource: Member + MaybeDisplay,
	Call: Encode + Member,
	Signature: Member + traits::Verify,
	<Signature as traits::Verify>::Signer: IdentifyAccount<AccountId = AccountId>,
	Extensions: TransactionExtension + AdditionalSigned,
	AccountId: Member + MaybeDisplay,
	Lookup: traits::Lookup<Source = LookupSource, Target = AccountId>,
{
	type Checked = CheckedExtrinsic<AccountId, Call, Extensions>;

	fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
		Ok(match self.preamble {
			Preamble::Signed(signed, signature, extra) => {
				let signed = lookup.lookup(signed)?;
				let raw_payload = SignedPayload::new(self.function, extra)?;
				if !raw_payload.using_encoded(|payload| signature.verify(payload, &signed)) {
					return Err(InvalidTransaction::BadProof.into())
				}
				let (function, extra, _) = raw_payload.deconstruct();
				CheckedExtrinsic { format: ExtrinsicFormat::Signed(signed, extra), function }
			},
			Preamble::General(extra) => CheckedExtrinsic {
				format: ExtrinsicFormat::General(extra),
				function: self.function,
			},
			Preamble::Bare => CheckedExtrinsic {
				format: ExtrinsicFormat::Bare,
				function: self.function,
			},
		})
	}

	#[cfg(feature = "try-runtime")]
	fn unchecked_into_checked_i_know_what_i_am_doing(
		self,
		lookup: &Lookup,
	) -> Result<Self::Checked, TransactionValidityError> {
		Ok(match self.preamble {
			Preamble::Signed(signed, _, extra) => {
				let signed = lookup.lookup(signed)?;
				CheckedExtrinsic {
					format: ExtrinsicFormat::Signed(signed, extra),
					function: self.function,
				}
			},
			Preamble::General(extra) => CheckedExtrinsic {
				format: ExtrinsicFormat::General(extra),
				function: self.function,
			},
			Preamble::Bare => CheckedExtrinsic {
				format: ExtrinsicFormat::Bare,
				function: self.function,
			},
		})
	}
}

impl<Address, Call, Signature, Extensions> ExtrinsicMetadata
	for UncheckedExtrinsic<Address, Call, Signature, Extensions>
where
	Extensions: TransactionExtension,
{
	const VERSION: u8 = EXTRINSIC_FORMAT_VERSION;
	type Extra = Extensions;
}

impl<Address, Call, Signature, Extensions> Decode for UncheckedExtrinsic<Address, Call, Signature, Extensions>
where
	Address: Decode,
	Signature: Decode,
	Call: Decode,
	Extensions: TransactionExtension,
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

impl<Address, Call, Signature, Extensions> Encode for UncheckedExtrinsic<Address, Call, Signature, Extensions>
where
	Preamble<Address, Signature, Extensions>: Encode,
	Call: Encode,
	Extensions: TransactionExtension,
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

impl<Address, Call, Signature, Extensions> EncodeLike
	for UncheckedExtrinsic<Address, Call, Signature, Extensions>
where
	Address: Encode,
	Signature: Encode,
	Call: Encode,
	Extensions: TransactionExtension,
{
}

#[cfg(feature = "serde")]
impl<Address: Encode, Signature: Encode, Call: Encode, Extensions: TransactionExtension> serde::Serialize
	for UncheckedExtrinsic<Address, Call, Signature, Extensions>
{
	fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error>
	where
		S: ::serde::Serializer,
	{
		self.using_encoded(|bytes| seq.serialize_bytes(bytes))
	}
}

#[cfg(feature = "serde")]
impl<'a, Address: Decode, Signature: Decode, Call: Decode, Extensions: TransactionExtension>
	serde::Deserialize<'a> for UncheckedExtrinsic<Address, Call, Signature, Extensions>
{
	fn deserialize<D>(de: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'a>,
	{
		let r = sp_core::bytes::deserialize(de)?;
		Decode::decode(&mut &r[..])
			.map_err(|e| serde::de::Error::custom(format!("Decode error: {}", e)))
	}
}

/// A payload that has been signed for an unchecked extrinsics.
///
/// Note that the payload that we sign to produce unchecked extrinsic signature
/// is going to be different than the `SignaturePayload` - so the thing the extrinsic
/// actually contains.
pub struct SignedPayload<Call, Extensions: TransactionExtension + AdditionalSigned>((Call, Extensions, <Extensions as AdditionalSigned>::Data));

impl<Call, Extensions> SignedPayload<Call, Extensions>
where
	Call: Encode,
	Extensions: TransactionExtension + AdditionalSigned,
{
	/// Create new `SignedPayload`.
	///
	/// This function may fail if `additional_signed` of `Extensions` is not available.
	pub fn new(call: Call, extra: Extensions) -> Result<Self, TransactionValidityError> {
		let additional_signed = <Extensions as AdditionalSigned>::additional_signed(&extra)?;
		let raw_payload = (call, extra, additional_signed);
		Ok(Self(raw_payload))
	}

	/// Create new `SignedPayload` from raw components.
	pub fn from_raw(call: Call, extra: Extensions, additional_signed: <Extensions as AdditionalSigned>::Data) -> Self {
		Self((call, extra, additional_signed))
	}

	/// Deconstruct the payload into it's components.
	pub fn deconstruct(self) -> (Call, Extensions, <Extensions as AdditionalSigned>::Data) {
		self.0
	}
}

impl<Call, Extensions> Encode for SignedPayload<Call, Extensions>
where
	Call: Encode,
	Extensions: TransactionExtension + AdditionalSigned,
{
	/// Get an encoded version of this payload.
	///
	/// Payloads longer than 256 bytes are going to be `blake2_256`-hashed.
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

impl<Call, Extensions> EncodeLike for SignedPayload<Call, Extensions>
where
	Call: Encode,
	Extensions: TransactionExtension + AdditionalSigned,
{
}

impl<Address, Call, Signature, Extensions> From<UncheckedExtrinsic<Address, Call, Signature, Extensions>>
	for OpaqueExtrinsic
where
	Address: Encode,
	Signature: Encode,
	Call: Encode,
	Extensions: TransactionExtension,
{
	fn from(extrinsic: UncheckedExtrinsic<Address, Call, Signature, Extensions>) -> Self {
		Self::from_bytes(extrinsic.encode().as_slice()).expect(
			"both OpaqueExtrinsic and UncheckedExtrinsic have encoding that is compatible with \
				raw Vec<u8> encoding; qed",
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		codec::{Decode, Encode},
		testing::TestSignature as TestSig,
		traits::{DispatchInfoOf, IdentityLookup, TransactionExtension},
	};
	use sp_io::hashing::blake2_256;

	type TestContext = IdentityLookup<u64>;
	type TestAccountId = u64;
	type TestCall = Vec<u8>;

	const TEST_ACCOUNT: TestAccountId = 0;

	// NOTE: this is demonstration. One can simply use `()` for testing.
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, Ord, PartialOrd, TypeInfo)]
	struct DummyExtension;
	impl AdditionalSigned for DummyExtension {
		type Data = ();
		fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> {
			Ok(())
		}
	}
	impl TransactionExtension for DummyExtension {
		const IDENTIFIER: &'static str = "DummyExtension";
		type Call = ();
		type Val = ();
		type Pre = ();

		fn validate(
			&self,
			who: <Self::Call as traits::Dispatchable>::RuntimeOrigin,
			_call: &Self::Call,
			_info: &DispatchInfoOf<Self::Call>,
			_len: usize,
		) -> Result<
			(crate::transaction_validity::ValidTransaction, Self::Val, <Self::Call as traits::Dispatchable>::RuntimeOrigin),
			TransactionValidityError
		> {
			Ok((Default::default(), (), who))
		}

		fn prepare(
			self,
			_val: (),
			_who: &<Self::Call as traits::Dispatchable>::RuntimeOrigin,
			_call: &Self::Call,
			_info: &DispatchInfoOf<Self::Call>,
			_len: usize,
		) -> Result<(), TransactionValidityError> {
			Ok(())
		}
	}

	type Ex = UncheckedExtrinsic<TestAccountId, TestCall, TestSig, DummyExtension>;
	type CEx = CheckedExtrinsic<TestAccountId, TestCall, DummyExtension>;

	#[test]
	fn unsigned_codec_should_work() {
		let ux = Ex::new_inherent(vec![0u8; 0]);
		let encoded = ux.encode();
		assert_eq!(Ex::decode(&mut &encoded[..]), Ok(ux));
	}

	#[test]
	fn invalid_length_prefix_is_detected() {
		let ux = Ex::new_inherent(vec![0u8; 0]);
		let mut encoded = ux.encode();

		let length = Compact::<u32>::decode(&mut &encoded[..]).unwrap();
		Compact(length.0 + 10).encode_to(&mut &mut encoded[..1]);

		assert_eq!(Ex::decode(&mut &encoded[..]), Err("Invalid length prefix".into()));
	}

	#[test]
	fn transaction_codec_should_work() {
		let ux = Ex::new_transaction(vec![0u8; 0], DummyExtension);
		let encoded = ux.encode();
		assert_eq!(Ex::decode(&mut &encoded[..]), Ok(ux));
	}

	#[test]
	fn signed_codec_should_work() {
		let ux = Ex::new_signed(
			vec![0u8; 0],
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
			vec![0u8; 0],
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
		let ux = Ex::new_inherent(vec![0u8; 0]);
		assert!(ux.is_inherent());
		assert_eq!(
			<Ex as Checkable<TestContext>>::check(ux, &Default::default()),
			Ok(CEx { format: ExtrinsicFormat::Bare, function: vec![0u8; 0] }),
		);
	}

	#[test]
	fn badly_signed_check_should_fail() {
		let ux = Ex::new_signed(
			vec![0u8; 0],
			TEST_ACCOUNT,
			TestSig(TEST_ACCOUNT, vec![0u8; 0]),
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
		let ux = Ex::new_transaction(vec![0u8; 0], DummyExtension);
		assert!(!ux.is_inherent());
		assert_eq!(
			<Ex as Checkable<TestContext>>::check(ux, &Default::default()),
			Ok(CEx { format: ExtrinsicFormat::General(DummyExtension), function: vec![0u8; 0] }),
		);
	}

	#[test]
	fn signed_check_should_work() {
		let ux = Ex::new_signed(
			vec![0u8; 0],
			TEST_ACCOUNT,
			TestSig(TEST_ACCOUNT, (vec![0u8; 0], DummyExtension).encode()),
			DummyExtension,
		);
		assert!(!ux.is_inherent());
		assert_eq!(
			<Ex as Checkable<TestContext>>::check(ux, &Default::default()),
			Ok(CEx { format: ExtrinsicFormat::Signed(TEST_ACCOUNT, DummyExtension), function: vec![0u8; 0] }),
		);
	}

	#[test]
	fn encoding_matches_vec() {
		let ex = Ex::new_inherent(vec![0u8; 0]);
		let encoded = ex.encode();
		let decoded = Ex::decode(&mut encoded.as_slice()).unwrap();
		assert_eq!(decoded, ex);
		let as_vec: Vec<u8> = Decode::decode(&mut encoded.as_slice()).unwrap();
		assert_eq!(as_vec.encode(), encoded);
	}

	#[test]
	fn conversion_to_opaque() {
		let ux = Ex::new_inherent(vec![0u8; 0]);
		let encoded = ux.encode();
		let opaque: OpaqueExtrinsic = ux.into();
		let opaque_encoded = opaque.encode();
		assert_eq!(opaque_encoded, encoded);
	}

	#[test]
	fn large_bad_prefix_should_work() {
		let encoded = (Compact::<u32>::from(u32::MAX), Preamble::<(), (), ()>::Bare).encode();
		assert_eq!(
			Ex::decode(&mut &encoded[..]),
			Err(Error::from("Not enough data to fill buffer"))
		);
	}
}
