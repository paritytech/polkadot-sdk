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

use crate::{
	crypto::{
		CryptoType, CryptoTypeId, DeriveError, DeriveJunction, Pair as TraitPair, PublicBytes,
		SecretStringError, SignatureBytes,
	},
	proof_of_possession::NonAggregatable,
};

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use k256::{
	ecdsa::{SigningKey as SecretKey, VerifyingKey},
	elliptic_curve::ops::Reduce,
};

#[cfg(feature = "full_crypto")]
type NativeSignature = (k256::ecdsa::Signature, k256::ecdsa::RecoveryId);

/// An identifier used to match public keys against ecdsa keys
pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"ecds");

/// The byte length of public key
pub const PUBLIC_KEY_SERIALIZED_SIZE: usize = 33;

/// The byte length of signature
pub const SIGNATURE_SERIALIZED_SIZE: usize = 65;

/// Returns `true` if the S component of a 65-byte ECDSA signature (R||S||V) is in
/// the low range (s <= N/2), i.e. the signature is canonical.
pub fn is_signature_normalized(sig: &[u8; 65]) -> bool {
	let Ok(parsed) = k256::ecdsa::Signature::try_from(&sig[..64]) else {
		return false;
	};
	parsed.normalize_s().is_none()
}

#[doc(hidden)]
#[derive(Clone)]
pub struct EcdsaTag;

#[doc(hidden)]
#[derive(Clone)]
pub struct EcdsaKeccakTag;

/// The secret seed.
///
/// The raw secret seed, which can be used to create the `Pair`.
type Seed = [u8; 32];

#[doc(hidden)]
pub type GenericPublic<TAG> = PublicBytes<PUBLIC_KEY_SERIALIZED_SIZE, TAG>;

/// The ECDSA compressed public key.
///
/// Uses blake2 during key recovery.
pub type Public = GenericPublic<EcdsaTag>;

/// The ECDSA compressed public key.
///
/// Uses keccak during key recovery.
pub type KeccakPublic = GenericPublic<EcdsaKeccakTag>;

impl<TAG> GenericPublic<TAG> {
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
		let pubkey = VerifyingKey::from_sec1_bytes(&full);
		pubkey.map(|k| k.into()).map_err(|_| ())
	}
}

impl<TAG> PartialEq<[u8; 33]> for GenericPublic<TAG> {
	fn eq(&self, other: &[u8; 33]) -> bool {
		&self.0 == other
	}
}

impl<TAG> From<VerifyingKey> for GenericPublic<TAG> {
	fn from(pubkey: VerifyingKey) -> Self {
		Self::try_from(pubkey.to_encoded_point(true).as_bytes())
			.expect("Valid key is serializable to [u8; 33]. qed.")
	}
}

#[cfg(feature = "full_crypto")]
impl<TAG> From<GenericPair<GenericPublic<TAG>>> for GenericPublic<TAG> {
	fn from(x: GenericPair<GenericPublic<TAG>>) -> Self {
		x.public
	}
}

#[doc(hidden)]
pub type GenericSignature<PUBLIC> = SignatureBytes<SIGNATURE_SERIALIZED_SIZE, PUBLIC>;

/// A signature (a 512-bit value, plus 8 bits for recovery ID).
///
/// Uses blake2 during key recovery.
pub type Signature = GenericSignature<Public>;

/// A signature (a 512-bit value, plus 8 bits for recovery ID).
///
/// Uses keccak during key recovery.
pub type KeccakSignature = GenericSignature<KeccakPublic>;

/// A signature that allows recovering the public key from a message.
pub trait Recover: seal::Sealed {
	/// The public key that will be recovered from the signature.
	type Public;

	/// Recover the public key from this signature and a pre-hashed message.
	fn recover_prehashed(&self, message: &[u8; 32]) -> Option<Self::Public>;

	/// Recover the public key from this signature and a message.
	fn recover<M: AsRef<[u8]>>(&self, message: M) -> Option<Self::Public>;
}

impl<PUBLIC: From<VerifyingKey>> GenericSignature<PUBLIC> {
	/// Recover the public key from this signature and a pre-hashed message.
	pub fn recover_prehashed(&self, message: &[u8; 32]) -> Option<PUBLIC> {
		let rid = k256::ecdsa::RecoveryId::from_byte(self.0[64])?;
		let sig = k256::ecdsa::Signature::from_bytes((&self.0[..64]).into()).ok()?;
		// Recovery is a primitive operation and historically accepted high-S signatures. k256's
		// verifier rejects them, so normalize here and adjust the recovery ID to preserve behavior.
		let (sig, rid) = if let Some(normalized) = sig.normalize_s() {
			(normalized, k256::ecdsa::RecoveryId::new(!rid.is_y_odd(), rid.is_x_reduced()))
		} else {
			(sig, rid)
		};
		VerifyingKey::recover_from_prehash(message, &sig, rid).map(From::from).ok()
	}
}

/// Proof of Possession is the same as Signature.
///
/// Uses blake2 during key recovery.
pub type ProofOfPossession = Signature;

/// Proof of Possession is the same as Signature.
///
/// Uses keccak during key recovery.
pub type KeccakProofOfPossession = KeccakSignature;

impl Signature {
	/// Recover the public key from this signature and a message.
	pub fn recover<M: AsRef<[u8]>>(&self, message: M) -> Option<Public> {
		self.recover_prehashed(&sp_crypto_hashing::blake2_256(message.as_ref()))
	}
}

impl KeccakSignature {
	/// Recover the public key from this signature and a message.
	pub fn recover<M: AsRef<[u8]>>(&self, message: M) -> Option<KeccakPublic> {
		self.recover_prehashed(&sp_crypto_hashing::keccak_256(message.as_ref()))
	}
}

impl Recover for Signature {
	type Public = Public;

	fn recover_prehashed(&self, message: &[u8; 32]) -> Option<Self::Public> {
		self.recover_prehashed(message)
	}

	fn recover<M: AsRef<[u8]>>(&self, message: M) -> Option<Self::Public> {
		self.recover(message)
	}
}

impl Recover for KeccakSignature {
	type Public = KeccakPublic;

	fn recover_prehashed(&self, message: &[u8; 32]) -> Option<Self::Public> {
		self.recover_prehashed(message)
	}

	fn recover<M: AsRef<[u8]>>(&self, message: M) -> Option<Self::Public> {
		self.recover(message)
	}
}

impl<PUBLIC> From<(k256::ecdsa::Signature, k256::ecdsa::RecoveryId)> for GenericSignature<PUBLIC> {
	fn from(recsig: (k256::ecdsa::Signature, k256::ecdsa::RecoveryId)) -> Self {
		let mut r = Self::default();
		r.0[..64].copy_from_slice(&recsig.0.to_bytes());
		r.0[64] = recsig.1.to_byte();
		r
	}
}

/// Derive a single hard junction.
fn derive_hard_junction(secret_seed: &Seed, cc: &[u8; 32]) -> Seed {
	use codec::Encode;
	("Secp256k1HDKD", secret_seed, cc).using_encoded(sp_crypto_hashing::blake2_256)
}

#[derive(Clone)]
#[doc(hidden)]
pub struct GenericPair<PUBLIC> {
	public: PUBLIC,
	secret: SecretKey,
}

/// An ecdsa key pair using the blake2 algorithm for hashing the message.
pub type Pair = GenericPair<Public>;

/// An ecdsa key pair using the keccak algorithm for hashing the message.
pub type KeccakPair = GenericPair<KeccakPublic>;

impl TraitPair for Pair {
	type Public = Public;
	type Seed = Seed;
	type Signature = Signature;
	type ProofOfPossession = ProofOfPossession;

	fn from_seed_slice(seed_slice: &[u8]) -> Result<Self, SecretStringError> {
		Self::from_seed_slice(seed_slice)
	}

	fn derive<Iter: Iterator<Item = DeriveJunction>>(
		&self,
		path: Iter,
		_seed: Option<Seed>,
	) -> Result<(Self, Option<Seed>), DeriveError> {
		self.derive(path)
	}

	fn public(&self) -> Self::Public {
		self.public
	}

	#[cfg(feature = "full_crypto")]
	fn sign(&self, message: &[u8]) -> Self::Signature {
		self.sign(message)
	}

	/// Verify a signature on a message. Returns true if the signature is good.
	fn verify<M: AsRef<[u8]>>(sig: &Self::Signature, message: M, public: &Public) -> bool {
		Self::verify(sig, message, public)
	}

	/// Return a vec filled with raw data.
	fn to_raw_vec(&self) -> Vec<u8> {
		self.to_raw_vec()
	}
}

impl TraitPair for KeccakPair {
	type Public = KeccakPublic;
	type Seed = Seed;
	type Signature = KeccakSignature;
	type ProofOfPossession = KeccakProofOfPossession;

	fn from_seed_slice(seed_slice: &[u8]) -> Result<Self, SecretStringError> {
		Self::from_seed_slice(seed_slice)
	}

	fn derive<Iter: Iterator<Item = DeriveJunction>>(
		&self,
		path: Iter,
		_seed: Option<Seed>,
	) -> Result<(Self, Option<Seed>), DeriveError> {
		self.derive(path)
	}

	fn public(&self) -> Self::Public {
		self.public
	}

	#[cfg(feature = "full_crypto")]
	fn sign(&self, message: &[u8]) -> Self::Signature {
		self.sign(message)
	}

	/// Verify a signature on a message. Returns true if the signature is good.
	fn verify<M: AsRef<[u8]>>(sig: &Self::Signature, message: M, public: &Self::Public) -> bool {
		Self::verify(sig, message, public)
	}

	/// Return a vec filled with raw data.
	fn to_raw_vec(&self) -> Vec<u8> {
		self.to_raw_vec()
	}
}

impl<PUBLIC> GenericPair<PUBLIC>
where
	Self: TraitPair<Seed = Seed, Signature: Recover>,
	<<Self as TraitPair>::Signature as Recover>::Public: PartialEq<PUBLIC>,
	PUBLIC: PartialEq<[u8; 33]>,
{
	/// Get the seed for this key.
	pub fn seed(&self) -> Seed {
		self.secret.to_bytes().into()
	}

	/// Exactly as `from_string` except that if no matches are found then, the the first 32
	/// characters are taken (padded with spaces as necessary) and used as the MiniSecretKey.
	#[cfg(feature = "std")]
	pub fn from_legacy_string(s: &str, password_override: Option<&str>) -> Self {
		Self::from_string(s, password_override).unwrap_or_else(|_| {
			let mut padded_seed: Seed = [b' '; 32];
			let len = s.len().min(32);
			padded_seed[..len].copy_from_slice(&s.as_bytes()[..len]);
			Self::from_seed(&padded_seed)
		})
	}

	/// Verify a signature on a pre-hashed message. Return `true` if the signature is valid
	/// and thus matches the given `public` key.
	pub fn verify_prehashed(
		sig: &<Self as TraitPair>::Signature,
		message: &[u8; 32],
		public: &PUBLIC,
	) -> bool {
		match sig.recover_prehashed(message) {
			Some(actual) => actual == *public,
			None => false,
		}
	}

	/// Verify a signature on a message. Returns true if the signature is good.
	/// Parses the signature with the legacy "overflowing" semantics: `r` and `s` values
	/// greater than or equal to the curve order are reduced modulo the order instead of
	/// being rejected. Backs version 1 of the `ecdsa_verify` host function, so its
	/// behaviour is frozen.
	#[deprecated(note = "please use `verify` instead")]
	pub fn verify_deprecated<M: AsRef<[u8]>>(sig: &Signature, message: M, pubkey: &Public) -> bool {
		// Reduce `r` and `s` modulo the curve order, as the historical
		// `libsecp256k1::Signature::parse_overflowing_slice` did. `recover_prehashed`
		// supplies the rest of the legacy semantics: a raw `0..=3` recovery byte, zero
		// scalars failing recovery and high-S signatures being accepted.
		let mut reduced = sig.0;
		reduced[..32].copy_from_slice(
			&<k256::Scalar as Reduce<k256::U256>>::reduce_bytes(k256::FieldBytes::from_slice(
				&sig.0[..32],
			))
			.to_bytes(),
		);
		reduced[32..64].copy_from_slice(
			&<k256::Scalar as Reduce<k256::U256>>::reduce_bytes(k256::FieldBytes::from_slice(
				&sig.0[32..64],
			))
			.to_bytes(),
		);
		Signature::from_raw(reduced)
			.recover(message)
			.is_some_and(|actual| actual == *pubkey)
	}

	fn derive<Iter: Iterator<Item = DeriveJunction>>(
		&self,
		path: Iter,
	) -> Result<(Self, Option<Seed>), DeriveError> {
		let mut acc = self.seed();
		for j in path {
			match j {
				DeriveJunction::Soft(_cc) => return Err(DeriveError::SoftKeyInPath),
				DeriveJunction::Hard(cc) => acc = derive_hard_junction(&acc, &cc),
			}
		}
		Ok((Self::from_seed(&acc), Some(acc)))
	}

	fn verify<M: AsRef<[u8]>>(
		sig: &<Self as TraitPair>::Signature,
		message: M,
		public: &PUBLIC,
	) -> bool {
		sig.recover(message).map(|actual| actual == *public).unwrap_or_default()
	}

	fn to_raw_vec(&self) -> Vec<u8> {
		self.seed().to_vec()
	}
}

impl<PUBLIC: From<VerifyingKey>> GenericPair<PUBLIC> {
	fn from_seed_slice(seed_slice: &[u8]) -> Result<Self, SecretStringError> {
		let secret =
			SecretKey::from_slice(seed_slice).map_err(|_| SecretStringError::InvalidSeedLength)?;
		Ok(Self { public: VerifyingKey::from(&secret).into(), secret })
	}
}

#[cfg(feature = "full_crypto")]
impl<PUBLIC> GenericPair<PUBLIC>
where
	Self: TraitPair,
	<Self as TraitPair>::Signature: From<NativeSignature>,
{
	/// Sign a pre-hashed message
	pub fn sign_prehashed(&self, message: &[u8; 32]) -> <Self as TraitPair>::Signature {
		let (raw_sig, recovery_id) = self
			.secret
			.sign_prehash_recoverable(message)
			.expect("Signing can't fail when using 32 bytes message hash. qed.");

		// k256 currently returns low-S signatures, but keep the normalization explicit.
		let (normalized_sig, adjusted_v) = if let Some(normalized) = raw_sig.normalize_s() {
			(
				normalized,
				k256::ecdsa::RecoveryId::new(!recovery_id.is_y_odd(), recovery_id.is_x_reduced()),
			)
		} else {
			(raw_sig, recovery_id)
		};
		(normalized_sig, adjusted_v).into()
	}
}

#[cfg(feature = "full_crypto")]
impl Pair
where
	<Self as TraitPair>::Signature: From<NativeSignature>,
{
	fn sign(&self, message: &[u8]) -> Signature {
		self.sign_prehashed(&sp_crypto_hashing::blake2_256(message))
	}
}

#[cfg(feature = "full_crypto")]
impl KeccakPair
where
	<Self as TraitPair>::Signature: From<NativeSignature>,
{
	fn sign(&self, message: &[u8]) -> KeccakSignature {
		self.sign_prehashed(&sp_crypto_hashing::keccak_256(message))
	}
}

impl CryptoType for Public {
	type Pair = Pair;
}

impl CryptoType for KeccakPublic {
	type Pair = KeccakPair;
}

impl CryptoType for Signature {
	type Pair = Pair;
}

impl CryptoType for KeccakSignature {
	type Pair = KeccakPair;
}

impl CryptoType for Pair {
	type Pair = Self;
}

impl CryptoType for KeccakPair {
	type Pair = Self;
}

impl NonAggregatable for Pair {}

mod seal {
	pub trait Sealed {}
	impl Sealed for super::Signature {}
	impl Sealed for super::KeccakSignature {}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		crypto::{
			set_default_ss58_version, PublicError, Ss58AddressFormat, Ss58AddressFormatRegistry,
			Ss58Codec, DEV_PHRASE,
		},
		proof_of_possession::{ProofOfPossessionGenerator, ProofOfPossessionVerifier},
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
		let derived = pair.derive(path.into_iter()).ok().unwrap();
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
	fn generated_pair_should_work_keccak() {
		let (pair, _) = KeccakPair::generate();
		let public = pair.public();
		let message = b"Something important";
		let signature = pair.sign(&message[..]);
		assert!(KeccakPair::verify(&signature, &message[..], &public));
		assert!(!KeccakPair::verify(&signature, b"Something else", &public));
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

		// sign_prehashed always produces a low-S (normalized) signature
		let msg = [0u8; 32];
		let sig1 = pair.sign_prehashed(&msg);
		assert!(
			is_signature_normalized(&sig1.0),
			"sign_prehashed should always produce a low-S signature"
		);

		// sign_prehashed is deterministic
		let sig1_again = pair.sign_prehashed(&msg);
		assert_eq!(sig1, sig1_again, "sign_prehashed should be deterministic");

		// prehashed signature differs from sign() (which blake2-hashes first)
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

	#[test]
	fn good_proof_of_possession_should_work_bad_proof_of_possession_should_fail() {
		let owner = b"owner";
		let not_owner = b"not owner";
		let mut pair = Pair::from_seed(b"12345678901234567890123456789012");
		let other_pair = Pair::from_seed(b"23456789012345678901234567890123");
		let proof_of_possession = pair.generate_proof_of_possession(owner);
		assert!(Pair::verify_proof_of_possession(owner, &proof_of_possession, &pair.public()));
		assert_eq!(
			Pair::verify_proof_of_possession(owner, &proof_of_possession, &other_pair.public()),
			false
		);
		assert!(!Pair::verify_proof_of_possession(not_owner, &proof_of_possession, &pair.public()));
	}

	#[test]
	fn is_signature_normalized_accepts_low_s() {
		// A real low-S signature produced by sign_prehashed
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let msg = sp_crypto_hashing::blake2_256(b"low-s test");
		let sig = pair.sign_prehashed(&msg);
		assert!(is_signature_normalized(&sig.0));
	}

	/// The high-S twin of a signature: `s' = n - s` with the recovery byte parity
	/// flipped. Recovers the same key but is not normalised.
	fn high_s_twin(sig: &Signature) -> Signature {
		use k256::elliptic_curve::PrimeField;
		let s = Option::<k256::Scalar>::from(k256::Scalar::from_repr(
			k256::FieldBytes::clone_from_slice(&sig.0[32..64]),
		))
		.expect("signature carries a valid scalar; qed");
		let mut twin = sig.0;
		twin[32..64].copy_from_slice(&(-s).to_bytes());
		twin[64] ^= 1;
		Signature::from_raw(twin)
	}

	#[test]
	fn is_signature_normalized_rejects_high_s() {
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let msg = sp_crypto_hashing::blake2_256(b"high-s test");
		let sig = pair.sign_prehashed(&msg);
		assert!(!is_signature_normalized(&high_s_twin(&sig).0));
	}

	#[test]
	fn sign_prehashed_produces_low_s() {
		for i in 1..21u8 {
			let seed = [i; 32];
			let pair = Pair::from_seed(&seed);
			let msg = sp_crypto_hashing::blake2_256(&[i]);
			let sig = pair.sign_prehashed(&msg);
			assert!(
				is_signature_normalized(&sig.0),
				"sign_prehashed produced high-S for seed {}",
				i
			);
		}
	}

	#[test]
	fn malleable_signature_is_rejected_by_normalization_check() {
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let msg = sp_crypto_hashing::blake2_256(b"malleable test");
		let sig = pair.sign_prehashed(&msg);
		assert!(is_signature_normalized(&sig.0));

		let malleable_sig = high_s_twin(&sig);
		assert!(
			!is_signature_normalized(&malleable_sig.0),
			"malleable signature should be rejected as high-S"
		);

		// Raw recovery continues to accept high-S signatures. Protocols that require low-S enforce
		// it before calling recovery.
		assert!(Pair::verify_prehashed(&malleable_sig, &msg, &pair.public()));
	}

	/// The historical `libsecp256k1` pipeline behind `verify_deprecated`, returning the
	/// recovered key. Oracle for the differential test below.
	fn libsecp_recover_overflowing(sig: &[u8; 65], msg: &[u8; 32]) -> Option<[u8; 33]> {
		let parsed = libsecp256k1::Signature::parse_overflowing_slice(&sig[..64]).ok()?;
		let rid = libsecp256k1::RecoveryId::parse(sig[64]).ok()?;
		let msg = libsecp256k1::Message::parse(msg);
		Some(libsecp256k1::recover(&msg, &parsed, &rid).ok()?.serialize_compressed())
	}

	/// 32-byte values probing the parsing and recovery boundaries: zero, the curve order
	/// `n` and neighbours, the low-S/high-S boundary at `n >> 1`, `p - n` and neighbour
	/// (the "reduced x" boundary for recovery ids 2/3), the maximum value and
	/// pseudo-random scalars. Kept in sync with its twin in sp-io's tests.
	fn scalar_edge_cases() -> Vec<[u8; 32]> {
		const N: [u8; 32] = [
			0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
			0xff, 0xfe, 0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b, 0xbf, 0xd2, 0x5e, 0x8c,
			0xd0, 0x36, 0x41, 0x41,
		];
		const HALF_N: [u8; 32] = [
			0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
			0xff, 0xff, 0x5d, 0x57, 0x6e, 0x73, 0x57, 0xa4, 0x50, 0x1d, 0xdf, 0xe9, 0x2f, 0x46,
			0x68, 0x1b, 0x20, 0xa0,
		];
		const P_MINUS_N: [u8; 32] = [
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x01, 0x45, 0x51, 0x23, 0x19, 0x50, 0xb7, 0x5f, 0xc4, 0x40, 0x2d, 0xa1, 0x72,
			0x2f, 0xc9, 0xba, 0xee,
		];
		// The last bytes (0x41, 0xa0, 0xee) are away from 0x00/0xff, so the +/-1
		// neighbours only change the last byte.
		let tweak = |mut x: [u8; 32], d: i8| {
			x[31] = x[31].wrapping_add_signed(d);
			x
		};
		let mut one = [0u8; 32];
		one[31] = 1;

		let mut cases = vec![
			[0u8; 32],
			one,
			tweak(N, -1),
			N,
			tweak(N, 1),
			HALF_N,
			tweak(HALF_N, 1),
			P_MINUS_N,
			tweak(P_MINUS_N, -1),
			[0xffu8; 32],
		];
		let mut seed = sp_crypto_hashing::blake2_256(b"ecdsa differential vectors");
		for _ in 0..2 {
			seed = sp_crypto_hashing::blake2_256(&seed);
			cases.push(seed);
		}
		cases
	}

	#[test]
	#[allow(deprecated)]
	fn verify_deprecated_matches_historical_libsecp256k1_implementation() {
		let pair = Pair::from_seed(b"12345678901234567890123456789012");
		let public = pair.public();
		let message = b"differential test message";
		let msg_hash = sp_crypto_hashing::blake2_256(message);

		// True positives: a real signature and its high-S twin, both with the raw
		// recovery byte the legacy verifier expects.
		let sig = pair.sign(message);
		let twin = high_s_twin(&sig);
		assert!(!is_signature_normalized(&twin.0));
		for sig in [&sig, &twin] {
			assert!(Pair::verify_deprecated(sig, message, &public));
			assert_eq!(libsecp_recover_overflowing(&sig.0, &msg_hash), Some(public.0));
		}

		// Boundary sweep: the implementations must agree on every input, and whenever
		// the historical pipeline recovers a key, the new one must accept exactly that
		// key.
		let cases = scalar_edge_cases();
		for r in &cases {
			for s in &cases {
				for v in [0u8, 1, 2, 3, 4, 27] {
					let mut raw = [0u8; 65];
					raw[..32].copy_from_slice(r);
					raw[32..64].copy_from_slice(s);
					raw[64] = v;
					let sig = Signature::from_raw(raw);

					let legacy = libsecp_recover_overflowing(&raw, &msg_hash);
					assert_eq!(
						Pair::verify_deprecated(&sig, message, &public),
						legacy == Some(public.0),
						"implementations diverged for r={r:02x?} s={s:02x?} v={v}",
					);
					if let Some(key) = legacy {
						assert!(
							Pair::verify_deprecated(&sig, message, &Public::from_raw(key)),
							"new implementation rejected the historically recovered key for \
							 r={r:02x?} s={s:02x?} v={v}",
						);
					}
				}
			}
		}
	}
}
