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

//! BLS (Boneh–Lynn–Shacham) Signature along with efficiently verifiable Chaum-Pedersen proof API.
//! Signatures are implemented according to
//! [Efficient Aggregatable BLS Signatures with Chaum-Pedersen Proofs](https://eprint.iacr.org/2022/1611)
//! Hash-to-BLS-curve is using Simplified SWU for AB == 0
//! [RFC 9380](https://datatracker.ietf.org/doc/rfc9380/) Sect 6.6.3.
//! Chaum-Pedersen proof uses the same hash-to-field specified in RFC 9380 for the field of the BLS
//! curve.

use crate::crypto::{
	CryptoType, DeriveError, DeriveJunction, Pair as TraitPair, PublicBytes, SecretStringError,
	SignatureBytes, UncheckedFrom,
};

use sp_std::vec::Vec;

use w3f_bls::{
	DoublePublicKey, DoublePublicKeyScheme, DoubleSignature, EngineBLS, Keypair, Message,
	SecretKey, SerializableToBytes, TinyBLS381,
};

/// BLS-377 specialized types
pub mod bls377 {
	pub use super::{PUBLIC_KEY_SERIALIZED_SIZE, SIGNATURE_SERIALIZED_SIZE};
	use crate::crypto::CryptoTypeId;
	use w3f_bls::TinyBLS377;

	/// An identifier used to match public keys against BLS12-377 keys
	pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"bls7");

	#[doc(hidden)]
	pub type Bls377Tag = TinyBLS377;

	/// BLS12-377 key pair.
	pub type Pair = super::Pair<TinyBLS377>;
	/// BLS12-377 public key.
	pub type Public = super::Public<TinyBLS377>;
	/// BLS12-377 signature.
	pub type Signature = super::Signature<TinyBLS377>;

	impl super::HardJunctionId for TinyBLS377 {
		const ID: &'static str = "BLS12377HDKD";
	}
}

/// BLS-381 specialized types
pub mod bls381 {
	pub use super::{PUBLIC_KEY_SERIALIZED_SIZE, SIGNATURE_SERIALIZED_SIZE};
	use crate::crypto::CryptoTypeId;
	use w3f_bls::TinyBLS381;

	/// An identifier used to match public keys against BLS12-381 keys
	pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"bls8");

	/// BLS12-381 key pair.
	pub type Pair = super::Pair<TinyBLS381>;
	/// BLS12-381 public key.
	pub type Public = super::Public<TinyBLS381>;
	/// BLS12-381 signature.
	pub type Signature = super::Signature<TinyBLS381>;

	impl super::HardJunctionId for TinyBLS381 {
		const ID: &'static str = "BLS12381HDKD";
	}
}

trait BlsBound: EngineBLS + HardJunctionId + Send + Sync + 'static {}

impl<T: EngineBLS + HardJunctionId + Send + Sync + 'static> BlsBound for T {}

/// Secret key serialized size
const SECRET_KEY_SERIALIZED_SIZE: usize =
	<SecretKey<TinyBLS381> as SerializableToBytes>::SERIALIZED_BYTES_SIZE;

/// Public key serialized size
pub const PUBLIC_KEY_SERIALIZED_SIZE: usize =
	<DoublePublicKey<TinyBLS381> as SerializableToBytes>::SERIALIZED_BYTES_SIZE;

/// Signature serialized size
pub const SIGNATURE_SERIALIZED_SIZE: usize =
	<DoubleSignature<TinyBLS381> as SerializableToBytes>::SERIALIZED_BYTES_SIZE;

/// A secret seed.
///
/// It's not called a "secret key" because ring doesn't expose the secret keys
/// of the key pair (yeah, dumb); as such we're forced to remember the seed manually if we
/// will need it later (such as for HDKD).
type Seed = [u8; SECRET_KEY_SERIALIZED_SIZE];

#[doc(hidden)]
pub struct BlsTag;

/// A public key.
pub type Public<SubTag> = PublicBytes<PUBLIC_KEY_SERIALIZED_SIZE, (BlsTag, SubTag)>;

impl<T: BlsBound> CryptoType for Public<T> {
	type Pair = Pair<T>;
}

/// A generic BLS signature.
pub type Signature<SubTag> = SignatureBytes<SIGNATURE_SERIALIZED_SIZE, (BlsTag, SubTag)>;

impl<T: BlsBound> CryptoType for Signature<T> {
	type Pair = Pair<T>;
}

/// A key pair.
pub struct Pair<T: EngineBLS>(Keypair<T>);

impl<T: EngineBLS> Clone for Pair<T> {
	fn clone(&self) -> Self {
		Pair(self.0.clone())
	}
}

trait HardJunctionId {
	const ID: &'static str;
}

/// Derive a single hard junction.
fn derive_hard_junction<T: HardJunctionId>(secret_seed: &Seed, cc: &[u8; 32]) -> Seed {
	use codec::Encode;
	(T::ID, secret_seed, cc).using_encoded(sp_crypto_hashing::blake2_256)
}

impl<T: EngineBLS> Pair<T> {}

impl<T: BlsBound> TraitPair for Pair<T> {
	type Seed = Seed;
	type Public = Public<T>;
	type Signature = Signature<T>;

	fn from_seed_slice(seed_slice: &[u8]) -> Result<Self, SecretStringError> {
		if seed_slice.len() != SECRET_KEY_SERIALIZED_SIZE {
			return Err(SecretStringError::InvalidSeedLength)
		}
		let secret = w3f_bls::SecretKey::from_seed(seed_slice);
		let public = secret.into_public();
		Ok(Pair(w3f_bls::Keypair { secret, public }))
	}

	fn derive<Iter: Iterator<Item = DeriveJunction>>(
		&self,
		path: Iter,
		seed: Option<Seed>,
	) -> Result<(Self, Option<Seed>), DeriveError> {
		let mut acc: [u8; SECRET_KEY_SERIALIZED_SIZE] =
			seed.unwrap_or(self.0.secret.to_bytes().try_into().expect(
				"Secret key serializer returns a vector of SECRET_KEY_SERIALIZED_SIZE size; qed",
			));
		for j in path {
			match j {
				DeriveJunction::Soft(_cc) => return Err(DeriveError::SoftKeyInPath),
				DeriveJunction::Hard(cc) => acc = derive_hard_junction::<T>(&acc, &cc),
			}
		}
		Ok((Self::from_seed(&acc), Some(acc)))
	}

	fn public(&self) -> Self::Public {
		let mut raw = [0u8; PUBLIC_KEY_SERIALIZED_SIZE];
		let pk = DoublePublicKeyScheme::into_double_public_key(&self.0).to_bytes();
		raw.copy_from_slice(pk.as_slice());
		Self::Public::unchecked_from(raw)
	}

	#[cfg(feature = "full_crypto")]
	fn sign(&self, message: &[u8]) -> Self::Signature {
		let mut mutable_self = self.clone();
		let r: [u8; SIGNATURE_SERIALIZED_SIZE] =
			DoublePublicKeyScheme::sign(&mut mutable_self.0, &Message::new(b"", message))
				.to_bytes()
				.try_into()
				.expect("Signature serializer returns vectors of SIGNATURE_SERIALIZED_SIZE size");
		Self::Signature::unchecked_from(r)
	}

	fn verify<M: AsRef<[u8]>>(sig: &Self::Signature, message: M, pubkey: &Self::Public) -> bool {
		let pubkey_array: [u8; PUBLIC_KEY_SERIALIZED_SIZE] =
			match <[u8; PUBLIC_KEY_SERIALIZED_SIZE]>::try_from(pubkey.as_ref()) {
				Ok(pk) => pk,
				Err(_) => return false,
			};
		let public_key = match w3f_bls::double::DoublePublicKey::<T>::from_bytes(&pubkey_array) {
			Ok(pk) => pk,
			Err(_) => return false,
		};

		let sig_array = match sig.0[..].try_into() {
			Ok(s) => s,
			Err(_) => return false,
		};
		let sig = match w3f_bls::double::DoubleSignature::from_bytes(sig_array) {
			Ok(s) => s,
			Err(_) => return false,
		};

		sig.verify(&Message::new(b"", message.as_ref()), &public_key)
	}

	/// Get the seed for this key.
	fn to_raw_vec(&self) -> Vec<u8> {
		self.0
			.secret
			.to_bytes()
			.try_into()
			.expect("Secret key serializer returns a vector of SECRET_KEY_SERIALIZED_SIZE size")
	}
}

impl<T: BlsBound> CryptoType for Pair<T> {
	type Pair = Pair<T>;
}

// Test set exercising the BLS12-377 implementation
#[cfg(test)]
mod tests {
	use super::*;
	#[cfg(feature = "serde")]
	use crate::crypto::Ss58Codec;
	use crate::crypto::DEV_PHRASE;
	use bls377::{Pair, Signature};

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
		let seed = array_bytes::hex2array_unchecked(
			"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
		);
		let pair = Pair::from_seed(&seed);
		// we are using hash-to-field so this is not going to work
		// assert_eq!(pair.seed(), seed);
		let path = vec![DeriveJunction::Hard([0u8; 32])];
		let derived = pair.derive(path.into_iter(), None).ok().unwrap().0;
		assert_eq!(
			derived.to_raw_vec(),
			array_bytes::hex2array_unchecked::<_, 32>(
				"3a0626d095148813cd1642d38254f1cfff7eb8cc1a2fc83b2a135377c3554c12"
			)
		);
	}

	#[test]
	fn test_vector_should_work() {
		let pair = Pair::from_seed(&array_bytes::hex2array_unchecked(
			"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
		));
		let public = pair.public();
		assert_eq!(
			public,
			Public::unchecked_from(array_bytes::hex2array_unchecked(
				"7a84ca8ce4c37c93c95ecee6a3c0c9a7b9c225093cf2f12dc4f69cbfb847ef9424a18f5755d5a742247d386ff2aabb806bcf160eff31293ea9616976628f77266c8a8cc1d8753be04197bd6cdd8c5c87a148f782c4c1568d599b48833fd539001e580cff64bbc71850605433fcd051f3afc3b74819786f815ffb5272030a8d03e5df61e6183f8fd8ea85f26defa83400"
			))
		);
		let message = b"";
		let signature =
	array_bytes::hex2array_unchecked("d1e3013161991e142d8751017d4996209c2ff8a9ee160f373733eda3b4b785ba6edce9f45f87104bbe07aa6aa6eb2780aa705efb2c13d3b317d6409d159d23bdc7cdd5c2a832d1551cf49d811d49c901495e527dbd532e3a462335ce2686009104aba7bc11c5b22be78f3198d2727a0b"
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
			Public::unchecked_from(array_bytes::hex2array_unchecked(
				"7a84ca8ce4c37c93c95ecee6a3c0c9a7b9c225093cf2f12dc4f69cbfb847ef9424a18f5755d5a742247d386ff2aabb806bcf160eff31293ea9616976628f77266c8a8cc1d8753be04197bd6cdd8c5c87a148f782c4c1568d599b48833fd539001e580cff64bbc71850605433fcd051f3afc3b74819786f815ffb5272030a8d03e5df61e6183f8fd8ea85f26defa83400"
			))
		);
		let message = b"";
		let signature =
	array_bytes::hex2array_unchecked("d1e3013161991e142d8751017d4996209c2ff8a9ee160f373733eda3b4b785ba6edce9f45f87104bbe07aa6aa6eb2780aa705efb2c13d3b317d6409d159d23bdc7cdd5c2a832d1551cf49d811d49c901495e527dbd532e3a462335ce2686009104aba7bc11c5b22be78f3198d2727a0b"
	);
		let expected_signature = Signature::unchecked_from(signature);
		println!("signature is {:?}", pair.sign(&message[..]));
		let signature = pair.sign(&message[..]);
		assert!(signature == expected_signature);
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
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let public = pair.public();
		assert_eq!(
			public,
			Public::unchecked_from(
				array_bytes::hex2array_unchecked(
				"754d2f2bbfa67df54d7e0e951979a18a1e0f45948857752cc2bac6bbb0b1d05e8e48bcc453920bf0c4bbd5993212480112a1fb433f04d74af0a8b700d93dc957ab3207f8d071e948f5aca1a7632c00bdf6d06be05b43e2e6216dccc8a5d55a0071cb2313cfd60b7e9114619cd17c06843b352f0b607a99122f6651df8f02e1ad3697bd208e62af047ddd7b942ba80080")
			)
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
	fn password_does_something() {
		let (pair1, phrase, _) = Pair::generate_with_phrase(Some("password"));
		let (pair2, _) = Pair::from_phrase(&phrase, None).unwrap();

		assert_ne!(pair1.public(), pair2.public());
		assert_ne!(pair1.to_raw_vec(), pair2.to_raw_vec());
	}

	#[test]
	fn ss58check_roundtrip_works() {
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let public = pair.public();
		let s = public.to_ss58check();
		println!("Correct: {}", s);
		let cmp = Public::from_ss58check(&s).unwrap();
		assert_eq!(cmp, public);
	}

	#[test]
	fn signature_serialization_works() {
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let message = b"Something important";
		let signature = pair.sign(&message[..]);
		let serialized_signature = serde_json::to_string(&signature).unwrap();
		// Signature is 112 bytes, hexify * 2, so 224  chars + 2 quote chars
		assert_eq!(serialized_signature.len(), 226);
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
}
