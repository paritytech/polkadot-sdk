// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Generic implementation of an unchecked (pre-verification) extrinsic.

use sp_std::{fmt, prelude::*};
use sp_io::hashing::blake2_256;
use codec::{Decode, Encode, EncodeLike, Input, Error};
use crate::{
	traits::{
		self, Member, MaybeDisplay, SignedExtension, Checkable, Extrinsic, ExtrinsicMetadata,
		IdentifyAccount,
	},
	generic::CheckedExtrinsic,
	transaction_validity::{TransactionValidityError, InvalidTransaction},
};

const TRANSACTION_VERSION: u8 = 4;

/// A extrinsic right from the external world. This is unchecked and so
/// can contain a signature.
#[derive(PartialEq, Eq, Clone)]
pub struct UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Extra: SignedExtension
{
	/// The signature, address, number of extrinsics have come before from
	/// the same signer and an era describing the longevity of this transaction,
	/// if this is a signed extrinsic.
	pub signature: Option<(Address, Signature, Extra)>,
	/// The function that should be called.
	pub function: Call,
}

#[cfg(feature = "std")]
impl<Address, Call, Signature, Extra> parity_util_mem::MallocSizeOf
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Extra: SignedExtension
{
	fn size_of(&self, _ops: &mut parity_util_mem::MallocSizeOfOps) -> usize {
		// Instantiated only in runtime.
		0
	}
}

impl<Address, Call, Signature, Extra: SignedExtension>
	UncheckedExtrinsic<Address, Call, Signature, Extra>
{
	/// New instance of a signed extrinsic aka "transaction".
	pub fn new_signed(
		function: Call,
		signed: Address,
		signature: Signature,
		extra: Extra
	) -> Self {
		UncheckedExtrinsic {
			signature: Some((signed, signature, extra)),
			function,
		}
	}

	/// New instance of an unsigned extrinsic aka "inherent".
	pub fn new_unsigned(function: Call) -> Self {
		UncheckedExtrinsic {
			signature: None,
			function,
		}
	}
}

impl<Address, Call, Signature, Extra: SignedExtension> Extrinsic
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
{
	type Call = Call;

	type SignaturePayload = (
		Address,
		Signature,
		Extra,
	);

	fn is_signed(&self) -> Option<bool> {
		Some(self.signature.is_some())
	}

	fn new(function: Call, signed_data: Option<Self::SignaturePayload>) -> Option<Self> {
		Some(if let Some((address, signature, extra)) = signed_data {
			UncheckedExtrinsic::new_signed(function, address, signature, extra)
		} else {
			UncheckedExtrinsic::new_unsigned(function)
		})
	}
}

impl<Address, AccountId, Call, Signature, Extra, Lookup>
	Checkable<Lookup>
for
	UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Address: Member + MaybeDisplay,
	Call: Encode + Member,
	Signature: Member + traits::Verify,
	<Signature as traits::Verify>::Signer: IdentifyAccount<AccountId=AccountId>,
	Extra: SignedExtension<AccountId=AccountId>,
	AccountId: Member + MaybeDisplay,
	Lookup: traits::Lookup<Source=Address, Target=AccountId>,
{
	type Checked = CheckedExtrinsic<AccountId, Call, Extra>;

	fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
		Ok(match self.signature {
			Some((signed, signature, extra)) => {
				let signed = lookup.lookup(signed)?;
				let raw_payload = SignedPayload::new(self.function, extra)?;
				if !raw_payload.using_encoded(|payload| signature.verify(payload, &signed)) {
					return Err(InvalidTransaction::BadProof.into())
				}

				let (function, extra, _) = raw_payload.deconstruct();
				CheckedExtrinsic {
					signed: Some((signed, extra)),
					function,
				}
			}
			None => CheckedExtrinsic {
				signed: None,
				function: self.function,
			},
		})
	}
}

impl<Address, Call, Signature, Extra> ExtrinsicMetadata
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
		where
			Extra: SignedExtension,
{
	const VERSION: u8 = TRANSACTION_VERSION;
	type SignedExtensions = Extra;
}

/// A payload that has been signed for an unchecked extrinsics.
///
/// Note that the payload that we sign to produce unchecked extrinsic signature
/// is going to be different than the `SignaturePayload` - so the thing the extrinsic
/// actually contains.
pub struct SignedPayload<Call, Extra: SignedExtension>((
	Call,
	Extra,
	Extra::AdditionalSigned,
));

impl<Call, Extra> SignedPayload<Call, Extra> where
	Call: Encode,
	Extra: SignedExtension,
{
	/// Create new `SignedPayload`.
	///
	/// This function may fail if `additional_signed` of `Extra` is not available.
	pub fn new(call: Call, extra: Extra) -> Result<Self, TransactionValidityError> {
		let additional_signed = extra.additional_signed()?;
		let raw_payload = (call, extra, additional_signed);
		Ok(Self(raw_payload))
	}

	/// Create new `SignedPayload` from raw components.
	pub fn from_raw(call: Call, extra: Extra, additional_signed: Extra::AdditionalSigned) -> Self {
		Self((call, extra, additional_signed))
	}

	/// Deconstruct the payload into it's components.
	pub fn deconstruct(self) -> (Call, Extra, Extra::AdditionalSigned) {
		self.0
	}
}

impl<Call, Extra> Encode for SignedPayload<Call, Extra> where
	Call: Encode,
	Extra: SignedExtension,
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

impl<Call, Extra> EncodeLike for SignedPayload<Call, Extra>
where
	Call: Encode,
	Extra: SignedExtension,
{}

impl<Address, Call, Signature, Extra> Decode
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Address: Decode,
	Signature: Decode,
	Call: Decode,
	Extra: SignedExtension,
{
	fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
		// This is a little more complicated than usual since the binary format must be compatible
		// with substrate's generic `Vec<u8>` type. Basically this just means accepting that there
		// will be a prefix of vector length (we don't need
		// to use this).
		let _length_do_not_remove_me_see_above: Vec<()> = Decode::decode(input)?;

		let version = input.read_byte()?;

		let is_signed = version & 0b1000_0000 != 0;
		let version = version & 0b0111_1111;
		if version != TRANSACTION_VERSION {
			return Err("Invalid transaction version".into());
		}

		Ok(UncheckedExtrinsic {
			signature: if is_signed { Some(Decode::decode(input)?) } else { None },
			function: Decode::decode(input)?,
		})
	}
}

impl<Address, Call, Signature, Extra> Encode
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Address: Encode,
	Signature: Encode,
	Call: Encode,
	Extra: SignedExtension,
{
	fn encode(&self) -> Vec<u8> {
		super::encode_with_vec_prefix::<Self, _>(|v| {
			// 1 byte version id.
			match self.signature.as_ref() {
				Some(s) => {
					v.push(TRANSACTION_VERSION | 0b1000_0000);
					s.encode_to(v);
				}
				None => {
					v.push(TRANSACTION_VERSION & 0b0111_1111);
				}
			}
			self.function.encode_to(v);
		})
	}
}

impl<Address, Call, Signature, Extra> EncodeLike
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Address: Encode,
	Signature: Encode,
	Call: Encode,
	Extra: SignedExtension,
{}

#[cfg(feature = "std")]
impl<Address: Encode, Signature: Encode, Call: Encode, Extra: SignedExtension> serde::Serialize
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
{
	fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error> where S: ::serde::Serializer {
		self.using_encoded(|bytes| seq.serialize_bytes(bytes))
	}
}

#[cfg(feature = "std")]
impl<'a, Address: Decode, Signature: Decode, Call: Decode, Extra: SignedExtension> serde::Deserialize<'a>
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
{
	fn deserialize<D>(de: D) -> Result<Self, D::Error> where
		D: serde::Deserializer<'a>,
	{
		let r = sp_core::bytes::deserialize(de)?;
		Decode::decode(&mut &r[..])
			.map_err(|e| serde::de::Error::custom(format!("Decode error: {}", e)))
	}
}

impl<Address, Call, Signature, Extra> fmt::Debug
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Address: fmt::Debug,
	Call: fmt::Debug,
	Extra: SignedExtension,
{
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"UncheckedExtrinsic({:?}, {:?})",
			self.signature.as_ref().map(|x| (&x.0, &x.2)),
			self.function,
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_io::hashing::blake2_256;
	use crate::codec::{Encode, Decode};
	use crate::traits::{SignedExtension, IdentifyAccount, IdentityLookup};
	use serde::{Serialize, Deserialize};

	type TestContext = IdentityLookup<u64>;

	#[derive(Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Encode, Decode)]
	pub struct TestSigner(pub u64);
	impl From<u64> for TestSigner { fn from(x: u64) -> Self { Self(x) } }
	impl From<TestSigner> for u64 { fn from(x: TestSigner) -> Self { x.0 } }
	impl IdentifyAccount for TestSigner {
		type AccountId = u64;
		fn into_account(self) -> u64 { self.into() }
	}

	#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Encode, Decode)]
	struct TestSig(u64, Vec<u8>);
	impl traits::Verify for TestSig {
		type Signer = TestSigner;
		fn verify<L: traits::Lazy<[u8]>>(&self, mut msg: L, signer: &u64) -> bool {
			signer == &self.0 && msg.get() == &self.1[..]
		}
	}

	type TestAccountId = u64;
	type TestCall = Vec<u8>;

	const TEST_ACCOUNT: TestAccountId = 0;

	// NOTE: this is demonstration. One can simply use `()` for testing.
	#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, Ord, PartialOrd)]
	struct TestExtra;
	impl SignedExtension for TestExtra {
		const IDENTIFIER: &'static str = "TestExtra";
		type AccountId = u64;
		type Call = ();
		type AdditionalSigned = ();
		type Pre = ();

		fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> { Ok(()) }
	}

	type Ex = UncheckedExtrinsic<TestAccountId, TestCall, TestSig, TestExtra>;
	type CEx = CheckedExtrinsic<TestAccountId, TestCall, TestExtra>;

	#[test]
	fn unsigned_codec_should_work() {
		let ux = Ex::new_unsigned(vec![0u8; 0]);
		let encoded = ux.encode();
		assert_eq!(Ex::decode(&mut &encoded[..]), Ok(ux));
	}

	#[test]
	fn signed_codec_should_work() {
		let ux = Ex::new_signed(
			vec![0u8; 0],
			TEST_ACCOUNT,
			TestSig(TEST_ACCOUNT, (vec![0u8; 0], TestExtra).encode()),
			TestExtra
		);
		let encoded = ux.encode();
		assert_eq!(Ex::decode(&mut &encoded[..]), Ok(ux));
	}

	#[test]
	fn large_signed_codec_should_work() {
		let ux = Ex::new_signed(
			vec![0u8; 0],
			TEST_ACCOUNT,
			TestSig(TEST_ACCOUNT, (vec![0u8; 257], TestExtra)
				.using_encoded(blake2_256)[..].to_owned()),
			TestExtra
		);
		let encoded = ux.encode();
		assert_eq!(Ex::decode(&mut &encoded[..]), Ok(ux));
	}

	#[test]
	fn unsigned_check_should_work() {
		let ux = Ex::new_unsigned(vec![0u8; 0]);
		assert!(!ux.is_signed().unwrap_or(false));
		assert!(<Ex as Checkable<TestContext>>::check(ux, &Default::default()).is_ok());
	}

	#[test]
	fn badly_signed_check_should_fail() {
		let ux = Ex::new_signed(
			vec![0u8; 0],
			TEST_ACCOUNT,
			TestSig(TEST_ACCOUNT, vec![0u8; 0]),
			TestExtra,
		);
		assert!(ux.is_signed().unwrap_or(false));
		assert_eq!(
			<Ex as Checkable<TestContext>>::check(ux, &Default::default()),
			Err(InvalidTransaction::BadProof.into()),
		);
	}

	#[test]
	fn signed_check_should_work() {
		let ux = Ex::new_signed(
			vec![0u8; 0],
			TEST_ACCOUNT,
			TestSig(TEST_ACCOUNT, (vec![0u8; 0], TestExtra).encode()),
			TestExtra,
		);
		assert!(ux.is_signed().unwrap_or(false));
		assert_eq!(
			<Ex as Checkable<TestContext>>::check(ux, &Default::default()),
			Ok(CEx { signed: Some((TEST_ACCOUNT, TestExtra)), function: vec![0u8; 0] }),
		);
	}

	#[test]
	fn encoding_matches_vec() {
		let ex = Ex::new_unsigned(vec![0u8; 0]);
		let encoded = ex.encode();
		let decoded = Ex::decode(&mut encoded.as_slice()).unwrap();
		assert_eq!(decoded, ex);
		let as_vec: Vec<u8> = Decode::decode(&mut encoded.as_slice()).unwrap();
		assert_eq!(as_vec.encode(), encoded);
	}
}
