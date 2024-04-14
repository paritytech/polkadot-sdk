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

//! Simple ECDSA secp256k1 API.

use crate::crypto::{
	CryptoType, CryptoTypeId, DeriveError, DeriveJunction, Pair as TraitPair, PublicBytes,
	SecretStringError, SignatureBytes,
};

#[cfg(not(feature = "std"))]
use k256::ecdsa::{SigningKey as SecretKey, VerifyingKey};
#[cfg(feature = "std")]
use secp256k1::{
	ecdsa::{RecoverableSignature, RecoveryId},
	Message, PublicKey, SecretKey, SECP256K1,
};
#[cfg(not(feature = "std"))]
use sp_std::vec::Vec;

/// An identifier used to match public keys against ecdsa keys
pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"ecds");

/// The byte length of public key
pub const PUBLIC_KEY_SERIALIZED_SIZE: usize = 33;

/// The byte length of signature
pub const SIGNATURE_SERIALIZED_SIZE: usize = 65;

#[doc(hidden)]
pub struct EcdsaTag;

/// The secret seed.
///
/// The raw secret seed, which can be used to create the `Pair`.
type Seed = [u8; 32];

/// The ECDSA compressed public key.
pub type Public = PublicBytes<PUBLIC_KEY_SERIALIZED_SIZE, EcdsaTag>;

impl Public {
	/// Create a new instance from the given full public key.
	///
	/// This will convert the full public key into the compressed format.
	pub fn from_full(full: &[u8]) -> Result<Self, ()> {
		let mut tagged_full = [0u8; 65];
		let full = if full.len() == 64 {
			// Tag it as uncompressed public key.
			tagged_full[0] = 0x04;
			tagged_full[1..].copy_from_slice(full);
			&tagged_full
		} else {
			full
		};
		#[cfg(feature = "std")]
		let pubkey = PublicKey::from_slice(&full);
		#[cfg(not(feature = "std"))]
		let pubkey = VerifyingKey::from_sec1_bytes(&full);
		pubkey.map(|k| k.into()).map_err(|_| ())
	}
}

#[cfg(feature = "std")]
impl From<PublicKey> for Public {
	fn from(pubkey: PublicKey) -> Self {
		Self::from(pubkey.serialize())
	}
}

#[cfg(not(feature = "std"))]
impl From<VerifyingKey> for Public {
	fn from(pubkey: VerifyingKey) -> Self {
		Self::try_from(&pubkey.to_sec1_bytes()[..])
			.expect("Valid key is serializable to [u8; 33]. qed.")
	}
}

#[cfg(feature = "full_crypto")]
impl From<Pair> for Public {
	fn from(x: Pair) -> Self {
		x.public()
	}
}

/// A signature (a 512-bit value, plus 8 bits for recovery ID).
pub type Signature = SignatureBytes<SIGNATURE_SERIALIZED_SIZE, EcdsaTag>;

impl Signature {
	/// Recover the public key from this signature and a message.
	pub fn recover<M: AsRef<[u8]>>(&self, message: M) -> Option<Public> {
		self.recover_prehashed(&sp_crypto_hashing::blake2_256(message.as_ref()))
	}

	/// Recover the public key from this signature and a pre-hashed message.
	pub fn recover_prehashed(&self, message: &[u8; 32]) -> Option<Public> {
		#[cfg(feature = "std")]
		{
			let rid = RecoveryId::from_i32(self.0[64] as i32).ok()?;
			let sig = RecoverableSignature::from_compact(&self.0[..64], rid).ok()?;
			let message =
				Message::from_digest_slice(message).expect("Message is a 32 bytes hash; qed");
			SECP256K1.recover_ecdsa(&message, &sig).ok().map(Public::from)
		}

		#[cfg(not(feature = "std"))]
		{
			let rid = k256::ecdsa::RecoveryId::from_byte(self.0[64])?;
			let sig = k256::ecdsa::Signature::from_bytes((&self.0[..64]).into()).ok()?;
			VerifyingKey::recover_from_prehash(message, &sig, rid).map(Public::from).ok()
		}
	}
}

#[cfg(not(feature = "std"))]
impl From<(k256::ecdsa::Signature, k256::ecdsa::RecoveryId)> for Signature {
	fn from(recsig: (k256::ecdsa::Signature, k256::ecdsa::RecoveryId)) -> Signature {
		let mut r = Self::default();
		r.0[..64].copy_from_slice(&recsig.0.to_bytes());
		r.0[64] = recsig.1.to_byte();
		r
	}
}

#[cfg(feature = "std")]
impl From<RecoverableSignature> for Signature {
	fn from(recsig: RecoverableSignature) -> Signature {
		let mut r = Self::default();
		let (recid, sig) = recsig.serialize_compact();
		r.0[..64].copy_from_slice(&sig);
		// This is safe due to the limited range of possible valid ids.
		r.0[64] = recid.to_i32() as u8;
		r
	}
}

/// Derive a single hard junction.
fn derive_hard_junction(secret_seed: &Seed, cc: &[u8; 32]) -> Seed {
	use codec::Encode;
	("Secp256k1HDKD", secret_seed, cc).using_encoded(sp_crypto_hashing::blake2_256)
}

/// A key pair.
#[derive(Clone)]
pub struct Pair {
	public: Public,
	secret: SecretKey,
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
		#[cfg(feature = "std")]
		{
			let secret = SecretKey::from_slice(seed_slice)
				.map_err(|_| SecretStringError::InvalidSeedLength)?;
			Ok(Pair { public: PublicKey::from_secret_key(&SECP256K1, &secret).into(), secret })
		}

		#[cfg(not(feature = "std"))]
		{
			let secret = SecretKey::from_slice(seed_slice)
				.map_err(|_| SecretStringError::InvalidSeedLength)?;
			Ok(Pair { public: VerifyingKey::from(&secret).into(), secret })
		}
	}

	/// Derive a child key from a series of given junctions.
	fn derive<Iter: Iterator<Item = DeriveJunction>>(
		&self,
		path: Iter,
		_seed: Option<Seed>,
	) -> Result<(Pair, Option<Seed>), DeriveError> {
		let mut acc = self.seed();
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
		self.public
	}

	/// Sign a message.
	#[cfg(feature = "full_crypto")]
	fn sign(&self, message: &[u8]) -> Signature {
		self.sign_prehashed(&sp_crypto_hashing::blake2_256(message))
	}

	/// Verify a signature on a message. Returns true if the signature is good.
	fn verify<M: AsRef<[u8]>>(sig: &Signature, message: M, public: &Public) -> bool {
		sig.recover(message).map(|actual| actual == *public).unwrap_or_default()
	}

	/// Return a vec filled with raw data.
	fn to_raw_vec(&self) -> Vec<u8> {
		self.seed().to_vec()
	}
}

impl Pair {
	/// Get the seed for this key.
	pub fn seed(&self) -> Seed {
		#[cfg(feature = "std")]
		{
			self.secret.secret_bytes()
		}
		#[cfg(not(feature = "std"))]
		{
			self.secret.to_bytes().into()
		}
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

	/// Sign a pre-hashed message
	#[cfg(feature = "full_crypto")]
	pub fn sign_prehashed(&self, message: &[u8; 32]) -> Signature {
		#[cfg(feature = "std")]
		{
			let message =
				Message::from_digest_slice(message).expect("Message is a 32 bytes hash; qed");
			SECP256K1.sign_ecdsa_recoverable(&message, &self.secret).into()
		}

		#[cfg(not(feature = "std"))]
		{
			// Signing fails only if the `message` number of bytes is less than the field length
			// (unfallible as we're using a fixed message length of 32).
			self.secret
				.sign_prehash_recoverable(message)
				.expect("Signing can't fail when using 32 bytes message hash. qed.")
				.into()
		}
	}

	/// Verify a signature on a pre-hashed message. Return `true` if the signature is valid
	/// and thus matches the given `public` key.
	pub fn verify_prehashed(sig: &Signature, message: &[u8; 32], public: &Public) -> bool {
		match sig.recover_prehashed(message) {
			Some(actual) => actual == *public,
			None => false,
		}
	}

	/// Verify a signature on a message. Returns true if the signature is good.
	/// Parses Signature using parse_overflowing_slice.
	#[deprecated(note = "please use `verify` instead")]
	pub fn verify_deprecated<M: AsRef<[u8]>>(sig: &Signature, message: M, pubkey: &Public) -> bool {
		let message =
			libsecp256k1::Message::parse(&sp_crypto_hashing::blake2_256(message.as_ref()));

		let parse_signature_overflowing = |x: [u8; SIGNATURE_SERIALIZED_SIZE]| {
			let sig = libsecp256k1::Signature::parse_overflowing_slice(&x[..64]).ok()?;
			let rid = libsecp256k1::RecoveryId::parse(x[64]).ok()?;
			Some((sig, rid))
		};

		let (sig, rid) = match parse_signature_overflowing(sig.0) {
			Some(sigri) => sigri,
			_ => return false,
		};
		match libsecp256k1::recover(&message, &sig, &rid) {
			Ok(actual) => pubkey.0 == actual.serialize_compressed(),
			_ => false,
		}
	}
}

// The `secp256k1` backend doesn't implement cleanup for their private keys.
// Currently we should take care of wiping the secret from memory.
// NOTE: this solution is not effective when `Pair` is moved around memory.
// The very same problem affects other cryptographic backends that are just using
// `zeroize`for their secrets.
#[cfg(feature = "std")]
impl Drop for Pair {
	fn drop(&mut self) {
		self.secret.non_secure_erase()
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
mod test {
	use super::*;
	use crate::crypto::{
		set_default_ss58_version, PublicError, Ss58AddressFormat, Ss58AddressFormatRegistry,
		Ss58Codec, DEV_PHRASE,
	};
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
		let derived = pair.derive(path.into_iter(), None).ok().unwrap();
		assert_eq!(
			derived.0.seed(),
			array_bytes::hex2array_unchecked::<_, 32>(
				"b8eefc4937200a8382d00050e050ced2d4ab72cc2ef1b061477afb51564fdd61"
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
			Public::from_full(
				&array_bytes::hex2bytes_unchecked("8db55b05db86c0b1786ca49f095d76344c9e6056b2f02701a7e7f3c20aabfd913ebbe148dd17c56551a52952371071a6c604b3f3abe8f2c8fa742158ea6dd7d4"),
			).unwrap(),
		);
		let message = b"";
		let signature = array_bytes::hex2array_unchecked("3dde91174bd9359027be59a428b8146513df80a2a3c7eda2194f64de04a69ab97b753169e94db6ffd50921a2668a48b94ca11e3d32c1ff19cfe88890aa7e8f3c00");
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
			Public::from_full(
				&array_bytes::hex2bytes_unchecked("8db55b05db86c0b1786ca49f095d76344c9e6056b2f02701a7e7f3c20aabfd913ebbe148dd17c56551a52952371071a6c604b3f3abe8f2c8fa742158ea6dd7d4"),
			).unwrap(),
		);
		let message = b"";
		let signature = array_bytes::hex2array_unchecked("3dde91174bd9359027be59a428b8146513df80a2a3c7eda2194f64de04a69ab97b753169e94db6ffd50921a2668a48b94ca11e3d32c1ff19cfe88890aa7e8f3c00");
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
			Public::from_full(
				&array_bytes::hex2bytes_unchecked("5676109c54b9a16d271abeb4954316a40a32bcce023ac14c8e26e958aa68fba995840f3de562156558efbfdac3f16af0065e5f66795f4dd8262a228ef8c6d813"),
			).unwrap(),
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
	fn generate_with_phrase_should_be_recoverable_with_from_string() {
		let (pair, phrase, seed) = Pair::generate_with_phrase(None);
		let repair_seed = Pair::from_seed_slice(seed.as_ref()).expect("seed slice is valid");
		assert_eq!(pair.public(), repair_seed.public());
		assert_eq!(pair.secret, repair_seed.secret);
		let (repair_phrase, reseed) =
			Pair::from_phrase(phrase.as_ref(), None).expect("seed slice is valid");
		assert_eq!(seed, reseed);
		assert_eq!(pair.public(), repair_phrase.public());
		assert_eq!(pair.secret, repair_phrase.secret);
		let repair_string = Pair::from_string(phrase.as_str(), None).expect("seed slice is valid");
		assert_eq!(pair.public(), repair_string.public());
		assert_eq!(pair.secret, repair_string.secret);
	}

	#[test]
	fn password_does_something() {
		let (pair1, phrase, _) = Pair::generate_with_phrase(Some("password"));
		let (pair2, _) = Pair::from_phrase(&phrase, None).unwrap();

		assert_ne!(pair1.public(), pair2.public());
		assert_ne!(pair1.secret, pair2.secret);
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
	fn ss58check_format_check_works() {
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let public = pair.public();
		let format = Ss58AddressFormatRegistry::Reserved46Account.into();
		let s = public.to_ss58check_with_version(format);
		assert_eq!(Public::from_ss58check_with_version(&s), Err(PublicError::FormatNotAllowed));
	}

	#[test]
	fn ss58check_full_roundtrip_works() {
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let public = pair.public();
		let format = Ss58AddressFormatRegistry::PolkadotAccount.into();
		let s = public.to_ss58check_with_version(format);
		let (k, f) = Public::from_ss58check_with_version(&s).unwrap();
		assert_eq!(k, public);
		assert_eq!(f, format);

		let format = Ss58AddressFormat::custom(64);
		let s = public.to_ss58check_with_version(format);
		let (k, f) = Public::from_ss58check_with_version(&s).unwrap();
		assert_eq!(k, public);
		assert_eq!(f, format);
	}

	#[test]
	fn ss58check_custom_format_works() {
		// We need to run this test in its own process to not interfere with other tests running in
		// parallel and also relying on the ss58 version.
		if std::env::var("RUN_CUSTOM_FORMAT_TEST") == Ok("1".into()) {
			use crate::crypto::Ss58AddressFormat;
			// temp save default format version
			let default_format = crate::crypto::default_ss58_version();
			// set current ss58 version is custom "200" `Ss58AddressFormat::Custom(200)`

			set_default_ss58_version(Ss58AddressFormat::custom(200));
			// custom addr encoded by version 200
			let addr = "4pbsSkWcBaYoFHrKJZp5fDVUKbqSYD9dhZZGvpp3vQ5ysVs5ybV";
			Public::from_ss58check(addr).unwrap();

			set_default_ss58_version(default_format);
			// set current ss58 version to default version
			let addr = "KWAfgC2aRG5UVD6CpbPQXCx4YZZUhvWqqAJE6qcYc9Rtr6g5C";
			Public::from_ss58check(addr).unwrap();

			println!("CUSTOM_FORMAT_SUCCESSFUL");
		} else {
			let executable = std::env::current_exe().unwrap();
			let output = std::process::Command::new(executable)
				.env("RUN_CUSTOM_FORMAT_TEST", "1")
				.args(&["--nocapture", "ss58check_custom_format_works"])
				.output()
				.unwrap();

			let output = String::from_utf8(output.stdout).unwrap();
			assert!(output.contains("CUSTOM_FORMAT_SUCCESSFUL"));
		}
	}

	#[test]
	fn signature_serialization_works() {
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let message = b"Something important";
		let signature = pair.sign(&message[..]);
		let serialized_signature = serde_json::to_string(&signature).unwrap();
		// Signature is 65 bytes, so 130 chars + 2 quote chars
		assert_eq!(serialized_signature.len(), SIGNATURE_SERIALIZED_SIZE * 2 + 2);
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
	fn sign_prehashed_works() {
		let (pair, _, _) = Pair::generate_with_phrase(Some("password"));

		// `msg` shouldn't be mangled
		let msg = [0u8; 32];
		let sig1 = pair.sign_prehashed(&msg);
		let sig2: Signature = {
			#[cfg(feature = "std")]
			{
				let message = Message::from_digest_slice(&msg).unwrap();
				SECP256K1.sign_ecdsa_recoverable(&message, &pair.secret).into()
			}
			#[cfg(not(feature = "std"))]
			{
				pair.secret
					.sign_prehash_recoverable(&msg)
					.expect("signing may not fail (???). qed.")
					.into()
			}
		};
		assert_eq!(sig1, sig2);

		// signature is actually different
		let sig2 = pair.sign(&msg);
		assert_ne!(sig1, sig2);

		// using pre-hashed `msg` works
		let msg = b"this should be hashed";
		let sig1 = pair.sign_prehashed(&sp_crypto_hashing::blake2_256(msg));
		let sig2 = pair.sign(msg);
		assert_eq!(sig1, sig2);
	}

	#[test]
	fn verify_prehashed_works() {
		let (pair, _, _) = Pair::generate_with_phrase(Some("password"));

		// `msg` and `sig` match
		let msg = sp_crypto_hashing::blake2_256(b"this should be hashed");
		let sig = pair.sign_prehashed(&msg);
		assert!(Pair::verify_prehashed(&sig, &msg, &pair.public()));

		// `msg` and `sig` don't match
		let msg = sp_crypto_hashing::blake2_256(b"this is a different message");
		assert!(!Pair::verify_prehashed(&sig, &msg, &pair.public()));
	}

	#[test]
	fn recover_prehashed_works() {
		let (pair, _, _) = Pair::generate_with_phrase(Some("password"));

		// recovered key matches signing key
		let msg = sp_crypto_hashing::blake2_256(b"this should be hashed");
		let sig = pair.sign_prehashed(&msg);
		let key = sig.recover_prehashed(&msg).unwrap();
		assert_eq!(pair.public(), key);

		// recovered key is useable
		assert!(Pair::verify_prehashed(&sig, &msg, &key));

		// recovered key and signing key don't match
		let msg = sp_crypto_hashing::blake2_256(b"this is a different message");
		let key = sig.recover_prehashed(&msg).unwrap();
		assert_ne!(pair.public(), key);
	}
}
