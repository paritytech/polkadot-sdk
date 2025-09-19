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

use crate::{
	crypto::{
		CryptoType, DeriveError, DeriveJunction, Pair as TraitPair, PublicBytes, SecretStringError,
		SignatureBytes, UncheckedFrom,
	},
	proof_of_possession::{
		statement_of_ownership, ProofOfPossessionGenerator, ProofOfPossessionVerifier,
	},
};

use alloc::vec::Vec;

use w3f_bls::{
	DoublePublicKey, DoublePublicKeyScheme, DoubleSignature, EngineBLS, Keypair, Message,
	NuggetBLSnCPPoP, ProofOfPossession as BlsProofOfPossession, SecretKey, SerializableToBytes,
	TinyBLS381,
};

#[cfg(feature = "full_crypto")]
use w3f_bls::ProofOfPossessionGenerator as BlsProofOfPossessionGenerator;

/// Required to generate Proof Of Possession
use sha2::Sha256;

/// BLS-377 specialized types
pub mod bls377 {
	pub use super::{
		PROOF_OF_POSSESSION_SERIALIZED_SIZE, PUBLIC_KEY_SERIALIZED_SIZE, SIGNATURE_SERIALIZED_SIZE,
	};
	use crate::crypto::CryptoTypeId;
	pub(crate) use w3f_bls::TinyBLS377 as BlsEngine;

	/// An identifier used to match public keys against BLS12-377 keys
	pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"bls7");

	#[doc(hidden)]
	pub type Bls377Tag = BlsEngine;

	/// BLS12-377 key pair.
	pub type Pair = super::Pair<BlsEngine>;
	/// BLS12-377 public key.
	pub type Public = super::Public<BlsEngine>;
	/// BLS12-377 signature.
	pub type Signature = super::Signature<BlsEngine>;
	/// BLS12-377 Proof Of Possesion.
	pub type ProofOfPossession = super::ProofOfPossession<BlsEngine>;

	impl super::HardJunctionId for BlsEngine {
		const ID: &'static str = "BLS12377HDKD";
	}
}

/// BLS-381 specialized types
pub mod bls381 {
	pub use super::{
		PROOF_OF_POSSESSION_SERIALIZED_SIZE, PUBLIC_KEY_SERIALIZED_SIZE, SIGNATURE_SERIALIZED_SIZE,
	};
	use crate::crypto::CryptoTypeId;
	pub use w3f_bls::TinyBLS381 as BlsEngine;

	/// An identifier used to match public keys against BLS12-381 keys
	pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"bls8");

	#[doc(hidden)]
	pub type Bls381Tag = BlsEngine;

	/// BLS12-381 key pair.
	pub type Pair = super::Pair<BlsEngine>;
	/// BLS12-381 public key.
	pub type Public = super::Public<BlsEngine>;
	/// BLS12-381 signature.
	pub type Signature = super::Signature<BlsEngine>;

	/// BLS12-381 Proof Of Possesion.
	pub type ProofOfPossession = super::ProofOfPossession<BlsEngine>;

	impl super::HardJunctionId for BlsEngine {
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

/// Signature serialized size (for back cert) + Nugget BLS PoP size
pub const PROOF_OF_POSSESSION_SERIALIZED_SIZE: usize = SIGNATURE_SERIALIZED_SIZE +
	<NuggetBLSnCPPoP<TinyBLS381> as SerializableToBytes>::SERIALIZED_BYTES_SIZE;

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

/// A generic BLS ProofOfpossession
pub type ProofOfPossession<SubTag> =
	SignatureBytes<PROOF_OF_POSSESSION_SERIALIZED_SIZE, (BlsTag, SubTag)>;

impl<T: BlsBound> CryptoType for ProofOfPossession<T> {
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
	type ProofOfPossession = ProofOfPossession<T>;

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

impl<T: BlsBound> ProofOfPossessionGenerator for Pair<T> {
	#[cfg(feature = "full_crypto")]
	/// Generate proof of possession for BLS12 curves.
	///
	/// Signs on:
	///  - owner as sort of back cert and proof of ownership to prevent front runner attack
	///  - on its own public key with unique context to prevent rougue key attack on aggregation
	fn generate_proof_of_possession(&mut self, owner: &[u8]) -> Self::ProofOfPossession {
		let proof_of_ownership: [u8; SIGNATURE_SERIALIZED_SIZE] =
			self.sign(statement_of_ownership(owner).as_slice()).to_raw();
		let proof_of_possession: [u8; SIGNATURE_SERIALIZED_SIZE] =
			<Keypair<T> as BlsProofOfPossessionGenerator<
				T,
				Sha256,
				DoublePublicKey<T>,
				NuggetBLSnCPPoP<T>,
			>>::generate_pok(&mut self.0)
			.to_bytes()
			.try_into()
			.expect("NuggetBLSnCPPoP serializer returns vectors of SIGNATURE_SERIALIZED_SIZE size");
		let proof_of_ownership_and_possession: [u8; PROOF_OF_POSSESSION_SERIALIZED_SIZE] =
			[proof_of_ownership, proof_of_possession]
				.concat()
				.try_into()
				.expect("PROOF_OF_POSSESSION_SERIALIZED_SIZE = SIGNATURE_SERIALIZED_SIZE * 2");
		Self::ProofOfPossession::unchecked_from(proof_of_ownership_and_possession)
	}
}

impl<T: BlsBound> ProofOfPossessionVerifier for Pair<T> {
	/// Verify both proof of ownership (back cert) and proof of possession of the private key
	fn verify_proof_of_possession(
		owner: &[u8],
		proof_of_possession: &Self::ProofOfPossession,
		allegedly_possessed_pubkey: &Self::Public,
	) -> bool {
		let Ok(allegedly_possessed_pubkey_as_bls_pubkey) =
			DoublePublicKey::<T>::from_bytes(allegedly_possessed_pubkey.as_ref())
		else {
			return false
		};

		let Ok(proof_of_ownership) = proof_of_possession.0[0..SIGNATURE_SERIALIZED_SIZE].try_into()
		else {
			return false
		};

		if !Self::verify(
			&proof_of_ownership,
			statement_of_ownership(owner).as_slice(),
			allegedly_possessed_pubkey,
		) {
			return false;
		}

		let Ok(proof_of_possession) =
			NuggetBLSnCPPoP::<T>::from_bytes(&proof_of_possession.0[SIGNATURE_SERIALIZED_SIZE..])
		else {
			return false;
		};

		BlsProofOfPossession::<T, Sha256, _>::verify(
			&proof_of_possession,
			&allegedly_possessed_pubkey_as_bls_pubkey,
		)
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
	use bls377::Pair as Bls377Pair;
	use bls381::Pair as Bls381Pair;

	fn default_phrase_should_be_used<E: BlsBound>() {
		assert_eq!(
			Pair::<E>::from_string("//Alice///password", None).unwrap().public(),
			Pair::<E>::from_string(&format!("{}//Alice", DEV_PHRASE), Some("password"))
				.unwrap()
				.public(),
		);
	}

	#[test]
	fn default_phrase_should_be_used_for_bls377() {
		default_phrase_should_be_used::<bls377::BlsEngine>();
	}

	#[test]
	fn default_phrase_should_be_used_for_bls381() {
		default_phrase_should_be_used::<bls381::BlsEngine>();
	}

	fn seed_and_derive_should_work<E: BlsBound>() -> Vec<u8> {
		let seed = array_bytes::hex2array_unchecked(
			"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
		);
		let pair = Pair::<E>::from_seed(&seed);
		// we are using hash-to-field so this is not going to work
		// assert_eq!(pair.seed(), seed);
		let path = vec![DeriveJunction::Hard([0u8; 32])];
		let derived = pair.derive(path.into_iter(), None).ok().unwrap().0;
		println!("derived is: {:?}", array_bytes::bytes2hex("", derived.to_raw_vec()));
		derived.to_raw_vec()
	}

	#[test]
	fn seed_and_derive_should_work_for_bls377() {
		let derived_as_raw_vector = seed_and_derive_should_work::<bls377::BlsEngine>();
		assert_eq!(
			derived_as_raw_vector,
			array_bytes::hex2array_unchecked::<_, 32>(
				"3a0626d095148813cd1642d38254f1cfff7eb8cc1a2fc83b2a135377c3554c12"
			)
		);
	}

	#[test]
	fn seed_and_derive_should_work_for_bls381() {
		let derived_as_raw_vector = seed_and_derive_should_work::<bls381::BlsEngine>();
		assert_eq!(
			derived_as_raw_vector,
			array_bytes::hex2array_unchecked::<_, 32>(
				"bb6ac58be00d3c7ae5608ca64180b5af628e79b58592b6067136bb46255cea27"
			)
		);
	}

	fn test_vector_should_work<E: BlsBound>(
		pair: Pair<E>,
		hex_expected_pub_key: &str,
		hex_expected_signature: &str,
	) {
		let public = pair.public();
		assert_eq!(
			public,
			Public::unchecked_from(array_bytes::hex2array_unchecked(hex_expected_pub_key))
		);
		let message = b"";
		let expected_signature_bytes = array_bytes::hex2array_unchecked(hex_expected_signature);

		let expected_signature = Signature::unchecked_from(expected_signature_bytes);
		let signature = pair.sign(&message[..]);

		assert!(signature == expected_signature);
		assert!(Pair::verify(&signature, &message[..], &public));
	}

	#[test]
	fn test_vector_should_work_for_bls377() {
		let pair = Bls377Pair::from_seed(&array_bytes::hex2array_unchecked(
			"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
		));
		test_vector_should_work(pair,
	    "7a84ca8ce4c37c93c95ecee6a3c0c9a7b9c225093cf2f12dc4f69cbfb847ef9424a18f5755d5a742247d386ff2aabb806bcf160eff31293ea9616976628f77266c8a8cc1d8753be04197bd6cdd8c5c87a148f782c4c1568d599b48833fd539001e580cff64bbc71850605433fcd051f3afc3b74819786f815ffb5272030a8d03e5df61e6183f8fd8ea85f26defa83400",
	    "124571b4bf23083b5d07e720fde0a984d4d592868156ece77487e97a1ba4b29397dbdc454f13e3aed1ad4b6a99af2501c68ab88ec0495f962a4f55c7c460275a8d356cfa344c27778ca4c641bd9a3604ce5c28f9ed566e1d29bf3b5d3591e46ae28be3ece035e8e4db53a40fc5826002"
	    )
	}

	#[test]
	fn test_vector_should_work_for_bls381() {
		let pair = Bls381Pair::from_seed(&array_bytes::hex2array_unchecked(
			"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
		));
		test_vector_should_work(pair,
				    "88ff6c3a32542bc85f2adf1c490a929b7fcee50faeb95af9a036349390e9b3ea7326247c4fc4ebf88050688fd6265de0806284eec09ba0949f5df05dc93a787a14509749f36e4a0981bb748d953435483740907bb5c2fe8ffd97e8509e1a038b05fb08488db628ea0638b8d48c3ddf62ed437edd8b23d5989d6c65820fc70f80fb39b486a3766813e021124aec29a566",
	    "8f4fe16cbb1b7f26ddbfbcde864a3c2f68802fbca5bd59920a135ed7e0f74cd9ba160e61c85e9acee3b4fe277862f226e60ac1958b57ed4487daf4673af420e8bf036ee8169190a927ede2e8eb3d6600633c69b2a84eb017473988fdfde082e150cbef05b77018c1f8ccc06da9e80421"
	    )
	}

	#[test]
	fn test_vector_by_string_should_work_for_bls377() {
		let pair = Bls377Pair::from_string(
			"0x9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
			None,
		)
		.unwrap();
		test_vector_should_work(pair,
	    "7a84ca8ce4c37c93c95ecee6a3c0c9a7b9c225093cf2f12dc4f69cbfb847ef9424a18f5755d5a742247d386ff2aabb806bcf160eff31293ea9616976628f77266c8a8cc1d8753be04197bd6cdd8c5c87a148f782c4c1568d599b48833fd539001e580cff64bbc71850605433fcd051f3afc3b74819786f815ffb5272030a8d03e5df61e6183f8fd8ea85f26defa83400",
	    "124571b4bf23083b5d07e720fde0a984d4d592868156ece77487e97a1ba4b29397dbdc454f13e3aed1ad4b6a99af2501c68ab88ec0495f962a4f55c7c460275a8d356cfa344c27778ca4c641bd9a3604ce5c28f9ed566e1d29bf3b5d3591e46ae28be3ece035e8e4db53a40fc5826002"
	    )
	}

	#[test]
	fn test_vector_by_string_should_work_for_bls381() {
		let pair = Bls381Pair::from_string(
			"0x9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
			None,
		)
		.unwrap();
		test_vector_should_work(pair,
	    "88ff6c3a32542bc85f2adf1c490a929b7fcee50faeb95af9a036349390e9b3ea7326247c4fc4ebf88050688fd6265de0806284eec09ba0949f5df05dc93a787a14509749f36e4a0981bb748d953435483740907bb5c2fe8ffd97e8509e1a038b05fb08488db628ea0638b8d48c3ddf62ed437edd8b23d5989d6c65820fc70f80fb39b486a3766813e021124aec29a566",
	    "8f4fe16cbb1b7f26ddbfbcde864a3c2f68802fbca5bd59920a135ed7e0f74cd9ba160e61c85e9acee3b4fe277862f226e60ac1958b57ed4487daf4673af420e8bf036ee8169190a927ede2e8eb3d6600633c69b2a84eb017473988fdfde082e150cbef05b77018c1f8ccc06da9e80421"
	    )
	}

	fn test_pair<E: BlsBound>(pair: Pair<E>) -> (String, String) {
		let public = pair.public();
		let message = b"Something important";
		let signature = pair.sign(&message[..]);
		assert!(Pair::verify(&signature, &message[..], &public));
		assert!(!Pair::verify(&signature, b"Something else", &public));
		let public_bytes: &[u8] = public.as_ref();
		let signature_bytes: &[u8] = signature.as_ref();
		(array_bytes::bytes2hex("", public_bytes), array_bytes::bytes2hex("", signature_bytes))
	}

	#[test]
	fn generated_pair_should_work_for_bls377() {
		let (pair, _) = Bls377Pair::generate();
		test_pair(pair);
	}

	#[test]
	fn generated_pair_should_work_for_bls381() {
		let (pair, _) = Bls381Pair::generate();
		test_pair(pair);
	}

	#[test]
	fn seeded_pair_should_work_for_bls377() {
		let pair = Bls377Pair::from_seed(b"12345678901234567890123456789012");
		let (public, _) = test_pair(pair);
		assert_eq!(
		    public,
		    "754d2f2bbfa67df54d7e0e951979a18a1e0f45948857752cc2bac6bbb0b1d05e8e48bcc453920bf0c4bbd5993212480112a1fb433f04d74af0a8b700d93dc957ab3207f8d071e948f5aca1a7632c00bdf6d06be05b43e2e6216dccc8a5d55a0071cb2313cfd60b7e9114619cd17c06843b352f0b607a99122f6651df8f02e1ad3697bd208e62af047ddd7b942ba80080"
		);
	}

	#[test]
	fn seeded_pair_should_work_for_bls381() {
		let pair = Bls381Pair::from_seed(b"12345678901234567890123456789012");
		let (public, _) = test_pair(pair);
		assert_eq!(
			public,
		    "abe9554cc2cab7fdc391a4e07ed0f45544cf0fe235babedf553c098d37dd162d9402a0aed95c00ed01349a6017a3d864adcc9756e98b7931aa3526b1511730c9cbacf3cbe781ae5efefdb177b301bca0229a5cf87432251cd31341c9b88aea9501005fa16e814ad31a95fcc396633baf563f6306e982ddec978faa0399ba73c1c1a87fa4791b3f5bbb719c1401b2af37"
		);
	}

	fn test_recover_with_phrase<E: BlsBound>(
		pair: Pair<E>,
		phrase: String,
		password: Option<&str>,
	) {
		let (recovered_pair, _) = Pair::from_phrase(&phrase, password).unwrap();

		assert_eq!(pair.public(), recovered_pair.public());
	}

	#[test]
	fn generate_with_phrase_recovery_possible_for_bls377() {
		let (pair, phrase, _) = Bls377Pair::generate_with_phrase(None);
		test_recover_with_phrase(pair, phrase, None);
	}

	#[test]
	fn generate_with_phrase_recovery_possible_for_bls381() {
		let (pair, phrase, _) = Bls381Pair::generate_with_phrase(None);
		test_recover_with_phrase(pair, phrase, None);
	}

	#[test]
	fn generate_with_password_phrase_recovery_possible_for_bls377() {
		let (pair, phrase, _) = Bls377Pair::generate_with_phrase(Some("password"));
		test_recover_with_phrase(pair, phrase, Some("password"));
	}

	#[test]
	fn generate_with_password_phrase_recovery_possible_for_bls381() {
		let (pair, phrase, _) = Bls381Pair::generate_with_phrase(Some("password"));
		test_recover_with_phrase(pair, phrase, Some("password"));
	}

	fn test_recover_from_seed_and_string<E: BlsBound>(pair: Pair<E>, phrase: String, seed: Seed) {
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
	fn generate_with_phrase_should_be_recoverable_with_from_string_for_bls377() {
		let (pair, phrase, seed) = Bls377Pair::generate_with_phrase(None);
		test_recover_from_seed_and_string(pair, phrase, seed);
	}

	#[test]
	fn generate_with_phrase_should_be_recoverable_with_from_string_for_bls381() {
		let (pair, phrase, seed) = Bls381Pair::generate_with_phrase(None);
		test_recover_from_seed_and_string(pair, phrase, seed);
	}

	fn password_does_something<E: BlsBound>() {
		let (pair1, phrase, _) = Pair::<E>::generate_with_phrase(Some("password"));
		let (pair2, _) = Pair::<E>::from_phrase(&phrase, None).unwrap();

		assert_ne!(pair1.public(), pair2.public());
		assert_ne!(pair1.to_raw_vec(), pair2.to_raw_vec());
	}

	#[test]
	fn password_does_something_for_bls377() {
		password_does_something::<bls377::BlsEngine>();
	}

	#[test]
	fn password_does_something_for_bls381() {
		password_does_something::<bls381::BlsEngine>();
	}

	fn ss58check_roundtrip_works<E: BlsBound>() {
		let pair = Pair::<E>::from_seed(b"12345678901234567890123456789012");
		let public = pair.public();
		let s = public.to_ss58check();
		println!("Correct: {}", s);
		let cmp = Public::from_ss58check(&s).unwrap();
		assert_eq!(cmp, public);
	}

	#[test]
	fn ss58check_roundtrip_works_for_bls377() {
		ss58check_roundtrip_works::<bls377::BlsEngine>();
	}

	#[test]
	fn ss58check_roundtrip_works_for_bls381() {
		ss58check_roundtrip_works::<bls381::BlsEngine>();
	}

	fn signature_serialization_works<E: BlsBound>() {
		let pair = Pair::<E>::from_seed(b"12345678901234567890123456789012");
		let message = b"Something important";
		let signature = pair.sign(&message[..]);
		let serialized_signature = serde_json::to_string(&signature).unwrap();
		// Signature is 112 bytes, hexify * 2, so 224  chars + 2 quote chars
		assert_eq!(serialized_signature.len(), 226);
		let signature = serde_json::from_str(&serialized_signature).unwrap();
		assert!(Pair::<E>::verify(&signature, &message[..], &pair.public()));
	}
	#[test]
	fn signature_serialization_works_for_bls377() {
		signature_serialization_works::<bls377::BlsEngine>();
	}

	#[test]
	fn signature_serialization_works_for_bls381() {
		signature_serialization_works::<bls381::BlsEngine>();
	}

	fn signature_serialization_doesnt_panic<E: BlsBound>() {
		fn deserialize_signature<E: BlsBound>(
			text: &str,
		) -> Result<Signature<E>, serde_json::error::Error> {
			serde_json::from_str(text)
		}
		assert!(deserialize_signature::<E>("Not valid json.").is_err());
		assert!(deserialize_signature::<E>("\"Not an actual signature.\"").is_err());
		// Poorly-sized
		assert!(deserialize_signature::<E>("\"abc123\"").is_err());
	}
	#[test]
	fn signature_serialization_doesnt_panic_for_bls377() {
		signature_serialization_doesnt_panic::<bls377::BlsEngine>();
	}

	#[test]
	fn signature_serialization_doesnt_panic_for_bls381() {
		signature_serialization_doesnt_panic::<bls381::BlsEngine>();
	}

	fn must_generate_proof_of_possession<E: BlsBound>() {
		let mut pair = Pair::<E>::from_seed(b"12345678901234567890123456789012");
		let owner = b"owner";

		pair.generate_proof_of_possession(owner);
	}

	#[test]
	fn must_generate_proof_of_possession_for_bls377() {
		must_generate_proof_of_possession::<bls377::BlsEngine>();
	}

	#[test]
	fn must_generate_proof_of_possession_for_bls381() {
		must_generate_proof_of_possession::<bls381::BlsEngine>();
	}

	fn good_proof_of_possession_must_verify<E: BlsBound>() {
		let mut pair = Pair::<E>::from_seed(b"12345678901234567890123456789012");
		let owner = b"owner";
		let proof_of_possession = pair.generate_proof_of_possession(owner);
		assert!(Pair::<E>::verify_proof_of_possession(owner, &proof_of_possession, &pair.public()));
	}

	#[test]
	fn good_proof_of_possession_must_verify_for_bls377() {
		good_proof_of_possession_must_verify::<bls377::BlsEngine>();
	}

	#[test]
	fn good_proof_of_possession_must_verify_for_bls381() {
		good_proof_of_possession_must_verify::<bls381::BlsEngine>();
	}

	fn proof_of_possession_must_fail_if_prover_does_not_possess_secret_key<E: BlsBound>() {
		let owner = b"owner";
		let not_owner = b"not owner";
		let mut pair = Pair::<E>::from_seed(b"12345678901234567890123456789012");
		let other_pair = Pair::<E>::from_seed(b"23456789012345678901234567890123");
		let proof_of_possession = pair.generate_proof_of_possession(owner);
		assert!(Pair::verify_proof_of_possession(owner, &proof_of_possession, &pair.public()));
		assert_eq!(
			Pair::<E>::verify_proof_of_possession(
				owner,
				&proof_of_possession,
				&other_pair.public()
			),
			false
		);
		assert!(!Pair::verify_proof_of_possession(not_owner, &proof_of_possession, &pair.public()));
	}

	#[test]
	fn proof_of_possession_must_fail_if_prover_does_not_possess_secret_key_for_bls377() {
		proof_of_possession_must_fail_if_prover_does_not_possess_secret_key::<bls377::BlsEngine>();
	}

	#[test]
	fn proof_of_possession_must_fail_if_prover_does_not_possess_secret_key_for_bls381() {
		proof_of_possession_must_fail_if_prover_does_not_possess_secret_key::<bls381::BlsEngine>();
	}
}
