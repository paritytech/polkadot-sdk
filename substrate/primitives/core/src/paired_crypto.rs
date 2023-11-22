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

//! API for using a pair of crypto schemes together.

#[cfg(feature = "serde")]
use crate::crypto::Ss58Codec;
use crate::crypto::{ByteArray, CryptoType, Derive, Public as PublicT, UncheckedFrom};
#[cfg(feature = "full_crypto")]
use crate::crypto::{DeriveError, DeriveJunction, Pair as PairT, SecretStringError};

#[cfg(feature = "full_crypto")]
use sp_std::vec::Vec;

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
#[cfg(feature = "serde")]
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
#[cfg(all(not(feature = "std"), feature = "serde"))]
use sp_std::alloc::{format, string::String};

use sp_runtime_interface::pass_by::{self, PassBy, PassByInner};
use sp_std::convert::TryFrom;

/// ECDSA and BLS12-377 paired crypto scheme
#[cfg(feature = "bls-experimental")]
pub mod ecdsa_bls377 {
	#[cfg(feature = "full_crypto")]
	use crate::Hasher;
	use crate::{
		bls377,
		crypto::{CryptoTypeId, Pair as PairT, UncheckedFrom},
		ecdsa,
	};

	/// An identifier used to match public keys against BLS12-377 keys
	pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"ecb7");

	const PUBLIC_KEY_LEN: usize =
		ecdsa::PUBLIC_KEY_SERIALIZED_SIZE + bls377::PUBLIC_KEY_SERIALIZED_SIZE;
	const SIGNATURE_LEN: usize =
		ecdsa::SIGNATURE_SERIALIZED_SIZE + bls377::SIGNATURE_SERIALIZED_SIZE;

	/// (ECDSA,BLS12-377) key-pair pair.
	#[cfg(feature = "full_crypto")]
	pub type Pair = super::Pair<ecdsa::Pair, bls377::Pair, PUBLIC_KEY_LEN, SIGNATURE_LEN>;
	/// (ECDSA,BLS12-377) public key pair.
	pub type Public = super::Public<PUBLIC_KEY_LEN>;
	/// (ECDSA,BLS12-377) signature pair.
	pub type Signature = super::Signature<SIGNATURE_LEN>;

	impl super::CryptoType for Public {
		#[cfg(feature = "full_crypto")]
		type Pair = Pair;
	}

	impl super::CryptoType for Signature {
		#[cfg(feature = "full_crypto")]
		type Pair = Pair;
	}

	#[cfg(feature = "full_crypto")]
	impl super::CryptoType for Pair {
		type Pair = Pair;
	}

	#[cfg(feature = "full_crypto")]
	impl Pair {
		/// Hashes the `message` with the specified [`Hasher`] before signing sith the ECDSA secret
		/// component.
		///
		/// The hasher does not affect the BLS12-377 component. This generates BLS12-377 Signature
		/// according to IETF standard.
		pub fn sign_with_hasher<H>(&self, message: &[u8]) -> Signature
		where
			H: Hasher,
			H::Out: Into<[u8; 32]>,
		{
			let msg_hash = H::hash(message).into();

			let mut raw: [u8; SIGNATURE_LEN] = [0u8; SIGNATURE_LEN];
			raw[..ecdsa::SIGNATURE_SERIALIZED_SIZE]
				.copy_from_slice(self.left.sign_prehashed(&msg_hash).as_ref());
			raw[ecdsa::SIGNATURE_SERIALIZED_SIZE..]
				.copy_from_slice(self.right.sign(message).as_ref());
			<Self as PairT>::Signature::unchecked_from(raw)
		}

		/// Hashes the `message` with the specified [`Hasher`] before verifying with the ECDSA
		/// public component.
		///
		/// The hasher does not affect the the BLS12-377 component. This verifies whether the
		/// BLS12-377 signature was hashed and signed according to IETF standard
		pub fn verify_with_hasher<H>(sig: &Signature, message: &[u8], public: &Public) -> bool
		where
			H: Hasher,
			H::Out: Into<[u8; 32]>,
		{
			let msg_hash = H::hash(message).into();

			let Ok(left_pub) = public.0[..ecdsa::PUBLIC_KEY_SERIALIZED_SIZE].try_into() else {
				return false
			};
			let Ok(left_sig) = sig.0[0..ecdsa::SIGNATURE_SERIALIZED_SIZE].try_into() else {
				return false
			};
			if !ecdsa::Pair::verify_prehashed(&left_sig, &msg_hash, &left_pub) {
				return false
			}

			let Ok(right_pub) = public.0[ecdsa::PUBLIC_KEY_SERIALIZED_SIZE..].try_into() else {
				return false
			};
			let Ok(right_sig) = sig.0[ecdsa::SIGNATURE_SERIALIZED_SIZE..].try_into() else {
				return false
			};
			bls377::Pair::verify(&right_sig, message.as_ref(), &right_pub)
		}
	}
}

/// Secure seed length.
///
/// Currently only supporting sub-schemes whose seed is a 32-bytes array.
#[cfg(feature = "full_crypto")]
const SECURE_SEED_LEN: usize = 32;

/// A secret seed.
///
/// It's not called a "secret key" because ring doesn't expose the secret keys
/// of the key pair (yeah, dumb); as such we're forced to remember the seed manually if we
/// will need it later (such as for HDKD).
#[cfg(feature = "full_crypto")]
type Seed = [u8; SECURE_SEED_LEN];

/// A public key.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq, PartialOrd, Ord)]
pub struct Public<const LEFT_PLUS_RIGHT_LEN: usize>([u8; LEFT_PLUS_RIGHT_LEN]);

#[cfg(feature = "full_crypto")]
impl<const LEFT_PLUS_RIGHT_LEN: usize> sp_std::hash::Hash for Public<LEFT_PLUS_RIGHT_LEN> {
	fn hash<H: sp_std::hash::Hasher>(&self, state: &mut H) {
		self.0.hash(state);
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> ByteArray for Public<LEFT_PLUS_RIGHT_LEN> {
	const LEN: usize = LEFT_PLUS_RIGHT_LEN;
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> TryFrom<&[u8]> for Public<LEFT_PLUS_RIGHT_LEN> {
	type Error = ();

	fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
		if data.len() != LEFT_PLUS_RIGHT_LEN {
			return Err(())
		}
		let mut inner = [0u8; LEFT_PLUS_RIGHT_LEN];
		inner.copy_from_slice(data);
		Ok(Public(inner))
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> AsRef<[u8; LEFT_PLUS_RIGHT_LEN]>
	for Public<LEFT_PLUS_RIGHT_LEN>
{
	fn as_ref(&self) -> &[u8; LEFT_PLUS_RIGHT_LEN] {
		&self.0
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> AsRef<[u8]> for Public<LEFT_PLUS_RIGHT_LEN> {
	fn as_ref(&self) -> &[u8] {
		&self.0[..]
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> AsMut<[u8]> for Public<LEFT_PLUS_RIGHT_LEN> {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0[..]
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> PassByInner for Public<LEFT_PLUS_RIGHT_LEN> {
	type Inner = [u8; LEFT_PLUS_RIGHT_LEN];

	fn into_inner(self) -> Self::Inner {
		self.0
	}

	fn inner(&self) -> &Self::Inner {
		&self.0
	}

	fn from_inner(inner: Self::Inner) -> Self {
		Self(inner)
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> PassBy for Public<LEFT_PLUS_RIGHT_LEN> {
	type PassBy = pass_by::Inner<Self, [u8; LEFT_PLUS_RIGHT_LEN]>;
}

#[cfg(feature = "full_crypto")]
impl<
		LeftPair: PairT,
		RightPair: PairT,
		const LEFT_PLUS_RIGHT_PUBLIC_LEN: usize,
		const SIGNATURE_LEN: usize,
	> From<Pair<LeftPair, RightPair, LEFT_PLUS_RIGHT_PUBLIC_LEN, SIGNATURE_LEN>>
	for Public<LEFT_PLUS_RIGHT_PUBLIC_LEN>
where
	Pair<LeftPair, RightPair, LEFT_PLUS_RIGHT_PUBLIC_LEN, SIGNATURE_LEN>:
		PairT<Public = Public<LEFT_PLUS_RIGHT_PUBLIC_LEN>>,
{
	fn from(x: Pair<LeftPair, RightPair, LEFT_PLUS_RIGHT_PUBLIC_LEN, SIGNATURE_LEN>) -> Self {
		x.public()
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> UncheckedFrom<[u8; LEFT_PLUS_RIGHT_LEN]>
	for Public<LEFT_PLUS_RIGHT_LEN>
{
	fn unchecked_from(data: [u8; LEFT_PLUS_RIGHT_LEN]) -> Self {
		Public(data)
	}
}

#[cfg(feature = "std")]
impl<const LEFT_PLUS_RIGHT_LEN: usize> std::fmt::Display for Public<LEFT_PLUS_RIGHT_LEN>
where
	Public<LEFT_PLUS_RIGHT_LEN>: CryptoType,
{
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "{}", self.to_ss58check())
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> sp_std::fmt::Debug for Public<LEFT_PLUS_RIGHT_LEN>
where
	Public<LEFT_PLUS_RIGHT_LEN>: CryptoType,
	[u8; LEFT_PLUS_RIGHT_LEN]: crate::hexdisplay::AsBytesRef,
{
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		let s = self.to_ss58check();
		write!(f, "{} ({}...)", crate::hexdisplay::HexDisplay::from(&self.0), &s[0..8])
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

#[cfg(feature = "serde")]
impl<const LEFT_PLUS_RIGHT_LEN: usize> Serialize for Public<LEFT_PLUS_RIGHT_LEN>
where
	Public<LEFT_PLUS_RIGHT_LEN>: CryptoType,
{
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_str(&self.to_ss58check())
	}
}

#[cfg(feature = "serde")]
impl<'de, const LEFT_PLUS_RIGHT_LEN: usize> Deserialize<'de> for Public<LEFT_PLUS_RIGHT_LEN>
where
	Public<LEFT_PLUS_RIGHT_LEN>: CryptoType,
{
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		Public::from_ss58check(&String::deserialize(deserializer)?)
			.map_err(|e| de::Error::custom(format!("{:?}", e)))
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> PublicT for Public<LEFT_PLUS_RIGHT_LEN> where
	Public<LEFT_PLUS_RIGHT_LEN>: CryptoType
{
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> Derive for Public<LEFT_PLUS_RIGHT_LEN> {}

/// Trait characterizing a signature which could be used as individual component of an
/// `paired_crypto:Signature` pair.
pub trait SignatureBound: ByteArray {}

impl<T: ByteArray> SignatureBound for T {}

/// A pair of signatures of different types
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
pub struct Signature<const LEFT_PLUS_RIGHT_LEN: usize>([u8; LEFT_PLUS_RIGHT_LEN]);

#[cfg(feature = "full_crypto")]
impl<const LEFT_PLUS_RIGHT_LEN: usize> sp_std::hash::Hash for Signature<LEFT_PLUS_RIGHT_LEN> {
	fn hash<H: sp_std::hash::Hasher>(&self, state: &mut H) {
		self.0.hash(state);
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> ByteArray for Signature<LEFT_PLUS_RIGHT_LEN> {
	const LEN: usize = LEFT_PLUS_RIGHT_LEN;
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> TryFrom<&[u8]> for Signature<LEFT_PLUS_RIGHT_LEN> {
	type Error = ();

	fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
		if data.len() != LEFT_PLUS_RIGHT_LEN {
			return Err(())
		}
		let mut inner = [0u8; LEFT_PLUS_RIGHT_LEN];
		inner.copy_from_slice(data);
		Ok(Signature(inner))
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> AsMut<[u8]> for Signature<LEFT_PLUS_RIGHT_LEN> {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0[..]
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> AsRef<[u8; LEFT_PLUS_RIGHT_LEN]>
	for Signature<LEFT_PLUS_RIGHT_LEN>
{
	fn as_ref(&self) -> &[u8; LEFT_PLUS_RIGHT_LEN] {
		&self.0
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> AsRef<[u8]> for Signature<LEFT_PLUS_RIGHT_LEN> {
	fn as_ref(&self) -> &[u8] {
		&self.0[..]
	}
}

#[cfg(feature = "serde")]
impl<const LEFT_PLUS_RIGHT_LEN: usize> Serialize for Signature<LEFT_PLUS_RIGHT_LEN> {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_str(&array_bytes::bytes2hex("", self))
	}
}

#[cfg(feature = "serde")]
impl<'de, const LEFT_PLUS_RIGHT_LEN: usize> Deserialize<'de> for Signature<LEFT_PLUS_RIGHT_LEN> {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let bytes = array_bytes::hex2bytes(&String::deserialize(deserializer)?)
			.map_err(|e| de::Error::custom(format!("{:?}", e)))?;
		Signature::<LEFT_PLUS_RIGHT_LEN>::try_from(bytes.as_ref()).map_err(|e| {
			de::Error::custom(format!("Error converting deserialized data into signature: {:?}", e))
		})
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> From<Signature<LEFT_PLUS_RIGHT_LEN>>
	for [u8; LEFT_PLUS_RIGHT_LEN]
{
	fn from(signature: Signature<LEFT_PLUS_RIGHT_LEN>) -> [u8; LEFT_PLUS_RIGHT_LEN] {
		signature.0
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> sp_std::fmt::Debug for Signature<LEFT_PLUS_RIGHT_LEN>
where
	[u8; LEFT_PLUS_RIGHT_LEN]: crate::hexdisplay::AsBytesRef,
{
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "{}", crate::hexdisplay::HexDisplay::from(&self.0))
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

impl<const LEFT_PLUS_RIGHT_LEN: usize> UncheckedFrom<[u8; LEFT_PLUS_RIGHT_LEN]>
	for Signature<LEFT_PLUS_RIGHT_LEN>
{
	fn unchecked_from(data: [u8; LEFT_PLUS_RIGHT_LEN]) -> Self {
		Signature(data)
	}
}

/// A key pair.
#[cfg(feature = "full_crypto")]
#[derive(Clone)]
pub struct Pair<
	LeftPair: PairT,
	RightPair: PairT,
	const PUBLIC_KEY_LEN: usize,
	const SIGNATURE_LEN: usize,
> {
	left: LeftPair,
	right: RightPair,
}

#[cfg(feature = "full_crypto")]
impl<
		LeftPair: PairT,
		RightPair: PairT,
		const PUBLIC_KEY_LEN: usize,
		const SIGNATURE_LEN: usize,
	> PairT for Pair<LeftPair, RightPair, PUBLIC_KEY_LEN, SIGNATURE_LEN>
where
	Pair<LeftPair, RightPair, PUBLIC_KEY_LEN, SIGNATURE_LEN>: CryptoType,
	LeftPair::Signature: SignatureBound,
	RightPair::Signature: SignatureBound,
	Public<PUBLIC_KEY_LEN>: CryptoType,
	LeftPair::Seed: From<Seed> + Into<Seed>,
	RightPair::Seed: From<Seed> + Into<Seed>,
{
	type Seed = Seed;
	type Public = Public<PUBLIC_KEY_LEN>;
	type Signature = Signature<SIGNATURE_LEN>;

	fn from_seed_slice(seed_slice: &[u8]) -> Result<Self, SecretStringError> {
		if seed_slice.len() != SECURE_SEED_LEN {
			return Err(SecretStringError::InvalidSeedLength)
		}
		let left = LeftPair::from_seed_slice(&seed_slice)?;
		let right = RightPair::from_seed_slice(&seed_slice)?;
		Ok(Pair { left, right })
	}

	/// Derive a child key from a series of given junctions.
	///
	/// Note: if the `LeftPair` and `RightPair` crypto schemes differ in
	/// seed derivation, `derive` will drop the seed in the return.
	fn derive<Iter: Iterator<Item = DeriveJunction>>(
		&self,
		path: Iter,
		seed: Option<Self::Seed>,
	) -> Result<(Self, Option<Self::Seed>), DeriveError> {
		let path: Vec<_> = path.collect();

		let left = self.left.derive(path.iter().cloned(), seed.map(|s| s.into()))?;
		let right = self.right.derive(path.into_iter(), seed.map(|s| s.into()))?;

		let seed = match (left.1, right.1) {
			(Some(l), Some(r)) if l.as_ref() == r.as_ref() => Some(l.into()),
			_ => None,
		};

		Ok((Self { left: left.0, right: right.0 }, seed))
	}

	fn public(&self) -> Self::Public {
		let mut raw = [0u8; PUBLIC_KEY_LEN];
		let left_pub = self.left.public();
		let right_pub = self.right.public();
		raw[..LeftPair::Public::LEN].copy_from_slice(left_pub.as_ref());
		raw[LeftPair::Public::LEN..].copy_from_slice(right_pub.as_ref());
		Self::Public::unchecked_from(raw)
	}

	fn sign(&self, message: &[u8]) -> Self::Signature {
		let mut raw: [u8; SIGNATURE_LEN] = [0u8; SIGNATURE_LEN];
		raw[..LeftPair::Signature::LEN].copy_from_slice(self.left.sign(message).as_ref());
		raw[LeftPair::Signature::LEN..].copy_from_slice(self.right.sign(message).as_ref());
		Self::Signature::unchecked_from(raw)
	}

	fn verify<M: AsRef<[u8]>>(sig: &Self::Signature, message: M, public: &Self::Public) -> bool {
		let Ok(left_pub) = public.0[..LeftPair::Public::LEN].try_into() else { return false };
		let Ok(left_sig) = sig.0[0..LeftPair::Signature::LEN].try_into() else { return false };
		if !LeftPair::verify(&left_sig, message.as_ref(), &left_pub) {
			return false
		}

		let Ok(right_pub) = public.0[LeftPair::Public::LEN..PUBLIC_KEY_LEN].try_into() else {
			return false
		};
		let Ok(right_sig) = sig.0[LeftPair::Signature::LEN..].try_into() else { return false };
		RightPair::verify(&right_sig, message.as_ref(), &right_pub)
	}

	/// Get the seed/secret key for each key and then concatenate them.
	fn to_raw_vec(&self) -> Vec<u8> {
		let mut raw = self.left.to_raw_vec();
		raw.extend(self.right.to_raw_vec());
		raw
	}
}

// Test set exercising the (ECDSA,BLS12-377) implementation
#[cfg(all(test, feature = "bls-experimental"))]
mod test {
	use super::*;
	use crate::{crypto::DEV_PHRASE, KeccakHasher};
	use ecdsa_bls377::{Pair, Signature};

	use crate::{bls377, ecdsa};

	#[test]
	fn test_length_of_paired_ecdsa_and_bls377_public_key_and_signature_is_correct() {
		assert_eq!(
			<Pair as PairT>::Public::LEN,
			<ecdsa::Pair as PairT>::Public::LEN + <bls377::Pair as PairT>::Public::LEN
		);
		assert_eq!(
			<Pair as PairT>::Signature::LEN,
			<ecdsa::Pair as PairT>::Signature::LEN + <bls377::Pair as PairT>::Signature::LEN
		);
	}

	#[test]
	fn default_phrase_should_be_used() {
		assert_eq!(
			Pair::from_string("//Alice///password", None).unwrap().public(),
			Pair::from_string(&format!("{}//Alice", DEV_PHRASE), Some("password"))
				.unwrap()
				.public(),
		);
	}

	#[test]
	fn seed_and_derive_should_work() {
		let seed_for_right_and_left: [u8; SECURE_SEED_LEN] = array_bytes::hex2array_unchecked(
			"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
		);
		let pair = Pair::from_seed(&seed_for_right_and_left);
		// we are using hash to field so this is not going to work
		// assert_eq!(pair.seed(), seed);
		let path = vec![DeriveJunction::Hard([0u8; 32])];
		let derived = pair.derive(path.into_iter(), None).ok().unwrap().0;
		assert_eq!(
			derived.to_raw_vec(),
			[
				array_bytes::hex2array_unchecked::<&str, SECURE_SEED_LEN>(
					"b8eefc4937200a8382d00050e050ced2d4ab72cc2ef1b061477afb51564fdd61"
				),
				array_bytes::hex2array_unchecked::<&str, SECURE_SEED_LEN>(
					"3a0626d095148813cd1642d38254f1cfff7eb8cc1a2fc83b2a135377c3554c12"
				)
			]
			.concat()
		);
	}

	#[test]
	fn test_vector_should_work() {
		let seed_left_and_right: [u8; SECURE_SEED_LEN] = array_bytes::hex2array_unchecked(
			"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
		);
		let pair = Pair::from_seed(&([seed_left_and_right].concat()[..].try_into().unwrap()));
		let public = pair.public();
		assert_eq!(
					public,
					Public::unchecked_from(
						array_bytes::hex2array_unchecked("028db55b05db86c0b1786ca49f095d76344c9e6056b2f02701a7e7f3c20aabfd917a84ca8ce4c37c93c95ecee6a3c0c9a7b9c225093cf2f12dc4f69cbfb847ef9424a18f5755d5a742247d386ff2aabb806bcf160eff31293ea9616976628f77266c8a8cc1d8753be04197bd6cdd8c5c87a148f782c4c1568d599b48833fd539001e580cff64bbc71850605433fcd051f3afc3b74819786f815ffb5272030a8d03e5df61e6183f8fd8ea85f26defa83400"),
		    		),
		    	);
		let message = b"";
		let signature =
		array_bytes::hex2array_unchecked("3dde91174bd9359027be59a428b8146513df80a2a3c7eda2194f64de04a69ab97b753169e94db6ffd50921a2668a48b94ca11e3d32c1ff19cfe88890aa7e8f3c00d1e3013161991e142d8751017d4996209c2ff8a9ee160f373733eda3b4b785ba6edce9f45f87104bbe07aa6aa6eb2780aa705efb2c13d3b317d6409d159d23bdc7cdd5c2a832d1551cf49d811d49c901495e527dbd532e3a462335ce2686009104aba7bc11c5b22be78f3198d2727a0b"
		);
		let signature = Signature::unchecked_from(signature);
		assert!(pair.sign(&message[..]) == signature);
		assert!(Pair::verify(&signature, &message[..], &public));
	}

	#[test]
	fn test_vector_by_string_should_work() {
		let pair = Pair::from_string(
			"0x9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
			None,
		)
		.unwrap();
		let public = pair.public();
		assert_eq!(
				public,
				Public::unchecked_from(
					array_bytes::hex2array_unchecked("028db55b05db86c0b1786ca49f095d76344c9e6056b2f02701a7e7f3c20aabfd916dc6be608fab3c6bd894a606be86db346cc170db85c733853a371f3db54ae1b12052c0888d472760c81b537572a26f00db865e5963aef8634f9917571c51b538b564b2a9ceda938c8b930969ee3b832448e08e33a79e9ddd28af419a3ce45300f5dbc768b067781f44f3fe05a19e6b07b1c4196151ec3f8ea37e4f89a8963030d2101e931276bb9ebe1f20102239d780"
	 ),
	    		),
	    	);
		let message = b"";
		let signature =
	array_bytes::hex2array_unchecked("3dde91174bd9359027be59a428b8146513df80a2a3c7eda2194f64de04a69ab97b753169e94db6ffd50921a2668a48b94ca11e3d32c1ff19cfe88890aa7e8f3c00bbb395bbdee1a35930912034f5fde3b36df2835a0536c865501b0675776a1d5931a3bea2e66eff73b2546c6af2061a8019223e4ebbbed661b2538e0f5823f2c708eb89c406beca8fcb53a5c13dbc7c0c42e4cf2be2942bba96ea29297915a06bd2b1b979c0e2ac8fd4ec684a6b5d110c"
	);
		let signature = Signature::unchecked_from(signature);
		assert!(pair.sign(&message[..]) == signature);
		assert!(Pair::verify(&signature, &message[..], &public));
	}

	#[test]
	fn generated_pair_should_work() {
		let (pair, _) = Pair::generate();
		let public = pair.public();
		let message = b"Something important";
		let signature = pair.sign(&message[..]);
		assert!(Pair::verify(&signature, &message[..], &public));
		assert!(!Pair::verify(&signature, b"Something else", &public));
	}

	#[test]
	fn seeded_pair_should_work() {
		let pair =
			Pair::from_seed(&(b"12345678901234567890123456789012".as_slice().try_into().unwrap()));
		let public = pair.public();
		assert_eq!(
	    		public,
				Public::unchecked_from(
					array_bytes::hex2array_unchecked("035676109c54b9a16d271abeb4954316a40a32bcce023ac14c8e26e958aa68fba9754d2f2bbfa67df54d7e0e951979a18a1e0f45948857752cc2bac6bbb0b1d05e8e48bcc453920bf0c4bbd5993212480112a1fb433f04d74af0a8b700d93dc957ab3207f8d071e948f5aca1a7632c00bdf6d06be05b43e2e6216dccc8a5d55a0071cb2313cfd60b7e9114619cd17c06843b352f0b607a99122f6651df8f02e1ad3697bd208e62af047ddd7b942ba80080")
	 ),
	    	);
		let message =
	    	array_bytes::hex2bytes_unchecked("2f8c6129d816cf51c374bc7f08c3e63ed156cf78aefb4a6550d97b87997977ee00000000000000000200d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a4500000000000000"
	    );
		let signature = pair.sign(&message[..]);
		println!("Correct signature: {:?}", signature);
		assert!(Pair::verify(&signature, &message[..], &public));
		assert!(!Pair::verify(&signature, "Other message", &public));
	}

	#[test]
	fn generate_with_phrase_recovery_possible() {
		let (pair1, phrase, _) = Pair::generate_with_phrase(None);
		let (pair2, _) = Pair::from_phrase(&phrase, None).unwrap();

		assert_eq!(pair1.public(), pair2.public());
	}

	#[test]
	fn generate_with_password_phrase_recovery_possible() {
		let (pair1, phrase, _) = Pair::generate_with_phrase(Some("password"));
		let (pair2, _) = Pair::from_phrase(&phrase, Some("password")).unwrap();

		assert_eq!(pair1.public(), pair2.public());
	}

	#[test]
	fn password_does_something() {
		let (pair1, phrase, _) = Pair::generate_with_phrase(Some("password"));
		let (pair2, _) = Pair::from_phrase(&phrase, None).unwrap();

		assert_ne!(pair1.public(), pair2.public());
	}

	#[test]
	fn ss58check_roundtrip_works() {
		let pair =
			Pair::from_seed(&(b"12345678901234567890123456789012".as_slice().try_into().unwrap()));
		let public = pair.public();
		let s = public.to_ss58check();
		println!("Correct: {}", s);
		let cmp = Public::from_ss58check(&s).unwrap();
		assert_eq!(cmp, public);
	}

	#[test]
	fn sign_and_verify_with_hasher_works() {
		let pair =
			Pair::from_seed(&(b"12345678901234567890123456789012".as_slice().try_into().unwrap()));
		let message = b"Something important";
		let signature = pair.sign_with_hasher::<KeccakHasher>(&message[..]);

		assert!(Pair::verify_with_hasher::<KeccakHasher>(&signature, &message[..], &pair.public()));
	}

	#[test]
	fn signature_serialization_works() {
		let pair =
			Pair::from_seed(&(b"12345678901234567890123456789012".as_slice().try_into().unwrap()));
		let message = b"Something important";
		let signature = pair.sign(&message[..]);

		let serialized_signature = serde_json::to_string(&signature).unwrap();
		println!("{:?} -- {:}", signature.0, serialized_signature);
		// Signature is 177 bytes, hexify * 2 + 2 quote charsy
		assert_eq!(serialized_signature.len(), 356);
		let signature = serde_json::from_str(&serialized_signature).unwrap();
		assert!(Pair::verify(&signature, &message[..], &pair.public()));
	}

	#[test]
	fn signature_serialization_doesnt_panic() {
		fn deserialize_signature(text: &str) -> Result<Signature, serde_json::error::Error> {
			serde_json::from_str(text)
		}
		assert!(deserialize_signature("Not valid json.").is_err());
		assert!(deserialize_signature("\"Not an actual signature.\"").is_err());
		// Poorly-sized
		assert!(deserialize_signature("\"abc123\"").is_err());
	}

	#[test]
	fn encode_and_decode_public_key_works() {
		let pair =
			Pair::from_seed(&(b"12345678901234567890123456789012".as_slice().try_into().unwrap()));
		let public = pair.public();
		let encoded_public = public.encode();
		let decoded_public = Public::decode(&mut encoded_public.as_slice()).unwrap();
		assert_eq!(public, decoded_public)
	}

	#[test]
	fn encode_and_decode_signature_works() {
		let pair =
			Pair::from_seed(&(b"12345678901234567890123456789012".as_slice().try_into().unwrap()));
		let message = b"Something important";
		let signature = pair.sign(&message[..]);
		let encoded_signature = signature.encode();
		let decoded_signature = Signature::decode(&mut encoded_signature.as_slice()).unwrap();
		assert_eq!(signature, decoded_signature)
	}
}
