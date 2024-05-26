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

use core::marker::PhantomData;

use crate::crypto::{
	ByteArray, CryptoType, DeriveError, DeriveJunction, Pair as PairT, Public as PublicT,
	PublicBytes, SecretStringError, Signature as SignatureT, SignatureBytes, UncheckedFrom,
};

use sp_std::vec::Vec;

/// ECDSA and BLS12-377 paired crypto scheme
#[cfg(feature = "bls-experimental")]
pub mod ecdsa_bls377 {
	use crate::{bls377, crypto::CryptoTypeId, ecdsa};
	#[cfg(feature = "full_crypto")]
	use crate::{
		crypto::{Pair as PairT, UncheckedFrom},
		Hasher,
	};

	/// An identifier used to match public keys against BLS12-377 keys
	pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"ecb7");

	const PUBLIC_KEY_LEN: usize =
		ecdsa::PUBLIC_KEY_SERIALIZED_SIZE + bls377::PUBLIC_KEY_SERIALIZED_SIZE;
	const SIGNATURE_LEN: usize =
		ecdsa::SIGNATURE_SERIALIZED_SIZE + bls377::SIGNATURE_SERIALIZED_SIZE;

	#[doc(hidden)]
	pub struct EcdsaBls377Tag(ecdsa::EcdsaTag, bls377::Bls377Tag);

	impl super::PairedCryptoSubTagBound for EcdsaBls377Tag {}

	/// (ECDSA,BLS12-377) key-pair pair.
	pub type Pair =
		super::Pair<ecdsa::Pair, bls377::Pair, PUBLIC_KEY_LEN, SIGNATURE_LEN, EcdsaBls377Tag>;

	/// (ECDSA,BLS12-377) public key pair.
	pub type Public = super::Public<PUBLIC_KEY_LEN, EcdsaBls377Tag>;

	/// (ECDSA,BLS12-377) signature pair.
	pub type Signature = super::Signature<SIGNATURE_LEN, EcdsaBls377Tag>;

	impl super::CryptoType for Public {
		type Pair = Pair;
	}

	impl super::CryptoType for Signature {
		type Pair = Pair;
	}

	impl super::CryptoType for Pair {
		type Pair = Pair;
	}

	#[cfg(feature = "full_crypto")]
	impl Pair {
		/// Hashes the `message` with the specified [`Hasher`] before signing with the ECDSA secret
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
			let Ok(left_sig) = sig.0[..ecdsa::SIGNATURE_SERIALIZED_SIZE].try_into() else {
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
			bls377::Pair::verify(&right_sig, message, &right_pub)
		}
	}
}

/// Secure seed length.
///
/// Currently only supporting sub-schemes whose seed is a 32-bytes array.
const SECURE_SEED_LEN: usize = 32;

/// A secret seed.
///
/// It's not called a "secret key" because ring doesn't expose the secret keys
/// of the key pair (yeah, dumb); as such we're forced to remember the seed manually if we
/// will need it later (such as for HDKD).
type Seed = [u8; SECURE_SEED_LEN];

#[doc(hidden)]
pub trait PairedCryptoSubTagBound {}
#[doc(hidden)]
pub struct PairedCryptoTag;

/// A public key.
pub type Public<const LEFT_PLUS_RIGHT_LEN: usize, SubTag> =
	PublicBytes<LEFT_PLUS_RIGHT_LEN, (PairedCryptoTag, SubTag)>;

impl<
		LeftPair: PairT,
		RightPair: PairT,
		const LEFT_PLUS_RIGHT_PUBLIC_LEN: usize,
		const SIGNATURE_LEN: usize,
		SubTag: PairedCryptoSubTagBound,
	> From<Pair<LeftPair, RightPair, LEFT_PLUS_RIGHT_PUBLIC_LEN, SIGNATURE_LEN, SubTag>>
	for Public<LEFT_PLUS_RIGHT_PUBLIC_LEN, SubTag>
where
	Pair<LeftPair, RightPair, LEFT_PLUS_RIGHT_PUBLIC_LEN, SIGNATURE_LEN, SubTag>:
		PairT<Public = Public<LEFT_PLUS_RIGHT_PUBLIC_LEN, SubTag>>,
{
	fn from(
		x: Pair<LeftPair, RightPair, LEFT_PLUS_RIGHT_PUBLIC_LEN, SIGNATURE_LEN, SubTag>,
	) -> Self {
		x.public()
	}
}

/// A pair of signatures of different types
pub type Signature<const LEFT_PLUS_RIGHT_LEN: usize, SubTag> =
	SignatureBytes<LEFT_PLUS_RIGHT_LEN, (PairedCryptoTag, SubTag)>;

/// A key pair.
pub struct Pair<
	LeftPair: PairT,
	RightPair: PairT,
	const PUBLIC_KEY_LEN: usize,
	const SIGNATURE_LEN: usize,
	SubTag,
> {
	left: LeftPair,
	right: RightPair,
	_phantom: PhantomData<fn() -> SubTag>,
}

impl<
		LeftPair: PairT + Clone,
		RightPair: PairT + Clone,
		const PUBLIC_KEY_LEN: usize,
		const SIGNATURE_LEN: usize,
		SubTag,
	> Clone for Pair<LeftPair, RightPair, PUBLIC_KEY_LEN, SIGNATURE_LEN, SubTag>
{
	fn clone(&self) -> Self {
		Self { left: self.left.clone(), right: self.right.clone(), _phantom: PhantomData }
	}
}

impl<
		LeftPair: PairT,
		RightPair: PairT,
		const PUBLIC_KEY_LEN: usize,
		const SIGNATURE_LEN: usize,
		SubTag: PairedCryptoSubTagBound,
	> PairT for Pair<LeftPair, RightPair, PUBLIC_KEY_LEN, SIGNATURE_LEN, SubTag>
where
	Pair<LeftPair, RightPair, PUBLIC_KEY_LEN, SIGNATURE_LEN, SubTag>: CryptoType,
	Public<PUBLIC_KEY_LEN, SubTag>: PublicT,
	Signature<SIGNATURE_LEN, SubTag>: SignatureT,
	LeftPair::Seed: From<Seed> + Into<Seed>,
	RightPair::Seed: From<Seed> + Into<Seed>,
{
	type Seed = Seed;
	type Public = Public<PUBLIC_KEY_LEN, SubTag>;
	type Signature = Signature<SIGNATURE_LEN, SubTag>;

	fn from_seed_slice(seed_slice: &[u8]) -> Result<Self, SecretStringError> {
		if seed_slice.len() != SECURE_SEED_LEN {
			return Err(SecretStringError::InvalidSeedLength)
		}
		let left = LeftPair::from_seed_slice(&seed_slice)?;
		let right = RightPair::from_seed_slice(&seed_slice)?;
		Ok(Pair { left, right, _phantom: PhantomData })
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
		let left_path: Vec<_> = path.collect();
		let right_path: Vec<_> = left_path.clone();

		let left = self.left.derive(left_path.into_iter(), seed.map(|s| s.into()))?;
		let right = self.right.derive(right_path.into_iter(), seed.map(|s| s.into()))?;

		let seed = match (left.1, right.1) {
			(Some(l), Some(r)) if l.as_ref() == r.as_ref() => Some(l.into()),
			_ => None,
		};

		Ok((Self { left: left.0, right: right.0, _phantom: PhantomData }, seed))
	}

	fn public(&self) -> Self::Public {
		let mut raw = [0u8; PUBLIC_KEY_LEN];
		let left_pub = self.left.public();
		let right_pub = self.right.public();
		raw[..LeftPair::Public::LEN].copy_from_slice(left_pub.as_ref());
		raw[LeftPair::Public::LEN..].copy_from_slice(right_pub.as_ref());
		Self::Public::unchecked_from(raw)
	}

	#[cfg(feature = "full_crypto")]
	fn sign(&self, message: &[u8]) -> Self::Signature {
		let mut raw: [u8; SIGNATURE_LEN] = [0u8; SIGNATURE_LEN];
		raw[..LeftPair::Signature::LEN].copy_from_slice(self.left.sign(message).as_ref());
		raw[LeftPair::Signature::LEN..].copy_from_slice(self.right.sign(message).as_ref());
		Self::Signature::unchecked_from(raw)
	}

	fn verify<Msg: AsRef<[u8]>>(
		sig: &Self::Signature,
		message: Msg,
		public: &Self::Public,
	) -> bool {
		let Ok(left_pub) = public.0[..LeftPair::Public::LEN].try_into() else { return false };
		let Ok(left_sig) = sig.0[0..LeftPair::Signature::LEN].try_into() else { return false };
		if !LeftPair::verify(&left_sig, message.as_ref(), &left_pub) {
			return false
		}

		let Ok(right_pub) = public.0[LeftPair::Public::LEN..].try_into() else { return false };
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
mod tests {
	use super::*;
	#[cfg(feature = "serde")]
	use crate::crypto::Ss58Codec;
	use crate::{bls377, crypto::DEV_PHRASE, ecdsa, KeccakHasher};
	use codec::{Decode, Encode};
	use ecdsa_bls377::{Pair, Signature};

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
	fn generate_with_phrase_should_be_recoverable_with_from_string() {
		let (pair, phrase, seed) = Pair::generate_with_phrase(None);
		let repair_seed = Pair::from_seed_slice(seed.as_ref()).expect("seed slice is valid");
		assert_eq!(pair.public(), repair_seed.public());
		assert_eq!(pair.to_raw_vec(), repair_seed.to_raw_vec());

		let (repair_phrase, reseed) =
			Pair::from_phrase(phrase.as_ref(), None).expect("seed slice is valid");
		assert_eq!(seed, reseed);
		assert_eq!(pair.public(), repair_phrase.public());
		assert_eq!(pair.to_raw_vec(), repair_seed.to_raw_vec());
		let repair_string = Pair::from_string(phrase.as_str(), None).expect("seed slice is valid");
		assert_eq!(pair.public(), repair_string.public());
		assert_eq!(pair.to_raw_vec(), repair_seed.to_raw_vec());
	}

	#[test]
	fn seed_and_derive_should_work() {
		let seed_for_right_and_left: [u8; SECURE_SEED_LEN] = array_bytes::hex2array_unchecked(
			"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
		);
		let pair = Pair::from_seed(&seed_for_right_and_left);
		// we are using hash-to-field so this is not going to work
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
					array_bytes::hex2array_unchecked("028db55b05db86c0b1786ca49f095d76344c9e6056b2f02701a7e7f3c20aabfd917a84ca8ce4c37c93c95ecee6a3c0c9a7b9c225093cf2f12dc4f69cbfb847ef9424a18f5755d5a742247d386ff2aabb806bcf160eff31293ea9616976628f77266c8a8cc1d8753be04197bd6cdd8c5c87a148f782c4c1568d599b48833fd539001e580cff64bbc71850605433fcd051f3afc3b74819786f815ffb5272030a8d03e5df61e6183f8fd8ea85f26defa83400"
	 ),
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
		assert_ne!(pair1.to_raw_vec(), pair2.to_raw_vec());
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
