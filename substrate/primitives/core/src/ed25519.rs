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

//! Simple Ed25519 API.

use crate::crypto::{
	ByteArray, CryptoType, CryptoTypeId, DeriveError, DeriveJunction, Pair as TraitPair,
	PublicBytes, SecretStringError, SignatureBytes,
};

use ed25519_zebra::{SigningKey, VerificationKey};

use sp_std::vec::Vec;

/// An identifier used to match public keys against ed25519 keys
pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"ed25");

/// The byte length of public key
pub const PUBLIC_KEY_SERIALIZED_SIZE: usize = 32;

/// The byte length of signature
pub const SIGNATURE_SERIALIZED_SIZE: usize = 64;

/// A secret seed. It's not called a "secret key" because ring doesn't expose the secret keys
/// of the key pair (yeah, dumb); as such we're forced to remember the seed manually if we
/// will need it later (such as for HDKD).
type Seed = [u8; 32];

#[doc(hidden)]
pub struct Ed25519Tag;

/// A public key.
pub type Public = PublicBytes<PUBLIC_KEY_SERIALIZED_SIZE, Ed25519Tag>;

/// A signature.
pub type Signature = SignatureBytes<SIGNATURE_SERIALIZED_SIZE, Ed25519Tag>;

/// A key pair.
#[derive(Copy, Clone)]
pub struct Pair {
	public: VerificationKey,
	secret: SigningKey,
}

/// Derive a single hard junction.
fn derive_hard_junction(secret_seed: &Seed, cc: &[u8; 32]) -> Seed {
	use codec::Encode;
	("Ed25519HDKD", secret_seed, cc).using_encoded(sp_crypto_hashing::blake2_256)
}

impl TraitPair for Pair {
	type Public = Public;
	type Seed = Seed;
	type Signature = Signature;

	/// Make a new key pair from secret seed material. The slice must be 32 bytes long or it
	/// will return `None`.
	///
	/// You should never need to use this; generate(), generate_with_phrase
	fn from_seed_slice(seed_slice: &[u8]) -> Result<Pair, SecretStringError> {
		let secret =
			SigningKey::try_from(seed_slice).map_err(|_| SecretStringError::InvalidSeedLength)?;
		let public = VerificationKey::from(&secret);
		Ok(Pair { secret, public })
	}

	/// Derive a child key from a series of given junctions.
	fn derive<Iter: Iterator<Item = DeriveJunction>>(
		&self,
		path: Iter,
		_seed: Option<Seed>,
	) -> Result<(Pair, Option<Seed>), DeriveError> {
		let mut acc = self.secret.into();
		for j in path {
			match j {
				DeriveJunction::Soft(_cc) => return Err(DeriveError::SoftKeyInPath),
				DeriveJunction::Hard(cc) => acc = derive_hard_junction(&acc, &cc),
			}
		}
		Ok((Self::from_seed(&acc), Some(acc)))
	}

	/// Get the public key.
	fn public(&self) -> Public {
		Public::from_raw(self.public.into())
	}

	/// Sign a message.
	#[cfg(feature = "full_crypto")]
	fn sign(&self, message: &[u8]) -> Signature {
		Signature::from_raw(self.secret.sign(message).into())
	}

	/// Verify a signature on a message.
	///
	/// Returns true if the signature is good.
	fn verify<M: AsRef<[u8]>>(sig: &Signature, message: M, public: &Public) -> bool {
		let Ok(public) = VerificationKey::try_from(public.as_slice()) else { return false };
		let Ok(signature) = ed25519_zebra::Signature::try_from(sig.as_ref()) else { return false };
		public.verify(&signature, message.as_ref()).is_ok()
	}

	/// Return a vec filled with raw data.
	fn to_raw_vec(&self) -> Vec<u8> {
		self.seed().to_vec()
	}
}

impl Pair {
	/// Get the seed for this key.
	pub fn seed(&self) -> Seed {
		self.secret.into()
	}

	/// Exactly as `from_string` except that if no matches are found then, the the first 32
	/// characters are taken (padded with spaces as necessary) and used as the MiniSecretKey.
	#[cfg(feature = "std")]
	pub fn from_legacy_string(s: &str, password_override: Option<&str>) -> Pair {
		Self::from_string(s, password_override).unwrap_or_else(|_| {
			let mut padded_seed: Seed = [b' '; 32];
			let len = s.len().min(32);
			padded_seed[..len].copy_from_slice(&s.as_bytes()[..len]);
			Self::from_seed(&padded_seed)
		})
	}
}

impl CryptoType for Public {
	type Pair = Pair;
}

impl CryptoType for Signature {
	type Pair = Pair;
}

impl CryptoType for Pair {
	type Pair = Pair;
}

#[cfg(test)]
mod tests {
	use super::*;
	#[cfg(feature = "serde")]
	use crate::crypto::Ss58Codec;
	use crate::crypto::DEV_PHRASE;
	use serde_json;

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
		assert_eq!(pair.seed(), seed);
		let path = vec![DeriveJunction::Hard([0u8; 32])];
		let derived = pair.derive(path.into_iter(), None).ok().unwrap().0;
		assert_eq!(
			derived.seed(),
			array_bytes::hex2array_unchecked::<_, 32>(
				"ede3354e133f9c8e337ddd6ee5415ed4b4ffe5fc7d21e933f4930a3730e5b21c"
			)
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
	fn test_vector_should_work() {
		let pair = Pair::from_seed(&array_bytes::hex2array_unchecked(
			"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
		));
		let public = pair.public();
		assert_eq!(
			public,
			Public::from_raw(array_bytes::hex2array_unchecked(
				"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
			))
		);
		let message = b"";
		let signature = array_bytes::hex2array_unchecked("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
		let signature = Signature::from_raw(signature);
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
			Public::from_raw(array_bytes::hex2array_unchecked(
				"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
			))
		);
		let message = b"";
		let signature = array_bytes::hex2array_unchecked("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
		let signature = Signature::from_raw(signature);
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
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let public = pair.public();
		assert_eq!(
			public,
			Public::from_raw(array_bytes::hex2array_unchecked(
				"2f8c6129d816cf51c374bc7f08c3e63ed156cf78aefb4a6550d97b87997977ee"
			))
		);
		let message = array_bytes::hex2bytes_unchecked("2f8c6129d816cf51c374bc7f08c3e63ed156cf78aefb4a6550d97b87997977ee00000000000000000200d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a4500000000000000");
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
		// Signature is 64 bytes, so 128 chars + 2 quote chars
		assert_eq!(serialized_signature.len(), 130);
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
