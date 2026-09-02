// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.
//
//! Local keystore implementation

use parking_lot::RwLock;
use sp_application_crypto::{AppCrypto, AppPair, IsWrappedBy};
use sp_core::{
	crypto::{ByteArray, ExposeSecret, KeyTypeId, Pair as CorePair, SecretString, VrfSecret},
	ecdsa, ed25519, sr25519,
};
use sp_keystore::{Error as TraitError, Keystore, KeystorePtr};
use std::{
	collections::HashMap,
	fs::{self, File},
	io::Write,
	path::{Path, PathBuf},
	sync::Arc,
};

/// Conservative upper bound on a single filename component
const MAX_FILENAME_LEN: usize = 255;

/// Length in bytes of the binary key-type prefix stored in every filename.
const KEY_TYPE_PREFIX_LEN: usize = 4;

/// Suffix of filenames that carry `blake2_256(public_key)` instead of the public key itself.
const HASHED_FILENAME_SUFFIX: &str = ".hashed";

sp_keystore::bandersnatch_experimental_enabled! {
use sp_core::bandersnatch;
}

sp_keystore::bls_experimental_enabled! {
use sp_core::{bls381, ecdsa_bls381, KeccakHasher, proof_of_possession::ProofOfPossessionGenerator};
}

use crate::{Error, Result};

/// A local based keystore that is either memory-based or filesystem-based.
pub struct LocalKeystore(RwLock<KeystoreInner>);

impl LocalKeystore {
	/// Create a local keystore from filesystem.
	///
	/// The keystore will be created at `path`. The keystore optionally supports to encrypt/decrypt
	/// the keys in the keystore using `password`.
	///
	/// NOTE: Even when passing a `password`, the keys on disk appear to look like normal secret
	/// uris. However, without having the correct password the secret uri will not generate the
	/// correct private key. See [`SecretUri`](sp_core::crypto::SecretUri) for more information.
	pub fn open<T: Into<PathBuf>>(path: T, password: Option<SecretString>) -> Result<Self> {
		let inner = KeystoreInner::open(path, password)?;
		Ok(Self(RwLock::new(inner)))
	}

	/// Create a local keystore in memory.
	pub fn in_memory() -> Self {
		let inner = KeystoreInner::new_in_memory();
		Self(RwLock::new(inner))
	}

	/// Get a key pair for the given public key.
	///
	/// Returns `Ok(None)` if the key doesn't exist, `Ok(Some(_))` if the key exists and
	/// `Err(_)` when something failed.
	pub fn key_pair<Pair: AppPair>(
		&self,
		public: &<Pair as AppCrypto>::Public,
	) -> Result<Option<Pair>> {
		self.0.read().key_pair::<Pair>(public)
	}

	fn public_keys<T: CorePair>(&self, key_type: KeyTypeId) -> Vec<T::Public> {
		self.0
			.read()
			.raw_public_keys(key_type)
			.map(|v| {
				v.into_iter().filter_map(|k| T::Public::from_slice(k.as_slice()).ok()).collect()
			})
			.unwrap_or_default()
	}

	fn generate_new<T: CorePair>(
		&self,
		key_type: KeyTypeId,
		seed: Option<&str>,
	) -> std::result::Result<T::Public, TraitError> {
		let pair = match seed {
			Some(seed) => self.0.write().insert_ephemeral_from_seed_by_type::<T>(seed, key_type),
			None => self.0.write().generate_by_type::<T>(key_type),
		}
		.map_err(|e| -> TraitError { e.into() })?;
		Ok(pair.public())
	}

	fn sign<T: CorePair>(
		&self,
		key_type: KeyTypeId,
		public: &T::Public,
		msg: &[u8],
	) -> std::result::Result<Option<T::Signature>, TraitError> {
		let signature = self
			.0
			.read()
			.key_pair_by_type::<T>(public, key_type)?
			.map(|pair| pair.sign(msg));
		Ok(signature)
	}

	fn vrf_sign<T: CorePair + VrfSecret>(
		&self,
		key_type: KeyTypeId,
		public: &T::Public,
		data: &T::VrfSignData,
	) -> std::result::Result<Option<T::VrfSignature>, TraitError> {
		let sig = self
			.0
			.read()
			.key_pair_by_type::<T>(public, key_type)?
			.map(|pair| pair.vrf_sign(data));
		Ok(sig)
	}

	fn vrf_pre_output<T: CorePair + VrfSecret>(
		&self,
		key_type: KeyTypeId,
		public: &T::Public,
		input: &T::VrfInput,
	) -> std::result::Result<Option<T::VrfPreOutput>, TraitError> {
		let pre_output = self
			.0
			.read()
			.key_pair_by_type::<T>(public, key_type)?
			.map(|pair| pair.vrf_pre_output(input));
		Ok(pre_output)
	}

	sp_keystore::bls_experimental_enabled! {
		fn generate_proof_of_possession<T: CorePair + ProofOfPossessionGenerator>(
			&self,
			key_type: KeyTypeId,
			public: &T::Public,
			owner: &[u8],
		) -> std::result::Result<Option<T::ProofOfPossession>, TraitError> {
			let proof_of_possession = self
				.0
				.read()
				.key_pair_by_type::<T>(public, key_type)?
				.map(|mut pair| pair.generate_proof_of_possession(owner));
			Ok(proof_of_possession)
		}
	}
}

impl Keystore for LocalKeystore {
	/// Insert a new secret key.
	///
	/// WARNING: if the secret keypair has been manually generated using a password
	/// (e.g. using methods such as [`sp_core::crypto::Pair::from_phrase`]) then such
	/// a password must match the one used to open the keystore via [`LocalKeystore::open`].
	/// If the passwords doesn't match then the inserted key ends up being unusable under
	/// the current keystore instance.
	fn insert(
		&self,
		key_type: KeyTypeId,
		suri: &str,
		public: &[u8],
	) -> std::result::Result<(), ()> {
		self.0.write().insert(key_type, suri, public).map_err(|_| ())
	}

	fn keys(&self, key_type: KeyTypeId) -> std::result::Result<Vec<Vec<u8>>, TraitError> {
		self.0.read().raw_public_keys(key_type).map_err(|e| e.into())
	}

	fn has_keys(&self, public_keys: &[(Vec<u8>, KeyTypeId)]) -> bool {
		public_keys
			.iter()
			.all(|(p, t)| self.0.read().key_phrase_by_type(p, *t).ok().flatten().is_some())
	}

	fn sr25519_public_keys(&self, key_type: KeyTypeId) -> Vec<sr25519::Public> {
		self.public_keys::<sr25519::Pair>(key_type)
	}

	/// Generate a new pair compatible with the 'ed25519' signature scheme.
	///
	/// If `[seed]` is `Some` then the key will be ephemeral and stored in memory.
	fn sr25519_generate_new(
		&self,
		key_type: KeyTypeId,
		seed: Option<&str>,
	) -> std::result::Result<sr25519::Public, TraitError> {
		self.generate_new::<sr25519::Pair>(key_type, seed)
	}

	fn sr25519_sign(
		&self,
		key_type: KeyTypeId,
		public: &sr25519::Public,
		msg: &[u8],
	) -> std::result::Result<Option<sr25519::Signature>, TraitError> {
		self.sign::<sr25519::Pair>(key_type, public, msg)
	}

	fn sr25519_vrf_sign(
		&self,
		key_type: KeyTypeId,
		public: &sr25519::Public,
		data: &sr25519::vrf::VrfSignData,
	) -> std::result::Result<Option<sr25519::vrf::VrfSignature>, TraitError> {
		self.vrf_sign::<sr25519::Pair>(key_type, public, data)
	}

	fn sr25519_vrf_pre_output(
		&self,
		key_type: KeyTypeId,
		public: &sr25519::Public,
		input: &sr25519::vrf::VrfInput,
	) -> std::result::Result<Option<sr25519::vrf::VrfPreOutput>, TraitError> {
		self.vrf_pre_output::<sr25519::Pair>(key_type, public, input)
	}

	fn ed25519_public_keys(&self, key_type: KeyTypeId) -> Vec<ed25519::Public> {
		self.public_keys::<ed25519::Pair>(key_type)
	}

	/// Generate a new pair compatible with the 'sr25519' signature scheme.
	///
	/// If `[seed]` is `Some` then the key will be ephemeral and stored in memory.
	fn ed25519_generate_new(
		&self,
		key_type: KeyTypeId,
		seed: Option<&str>,
	) -> std::result::Result<ed25519::Public, TraitError> {
		self.generate_new::<ed25519::Pair>(key_type, seed)
	}

	fn ed25519_sign(
		&self,
		key_type: KeyTypeId,
		public: &ed25519::Public,
		msg: &[u8],
	) -> std::result::Result<Option<ed25519::Signature>, TraitError> {
		self.sign::<ed25519::Pair>(key_type, public, msg)
	}

	fn ecdsa_public_keys(&self, key_type: KeyTypeId) -> Vec<ecdsa::Public> {
		self.public_keys::<ecdsa::Pair>(key_type)
	}

	/// Generate a new pair compatible with the 'ecdsa' signature scheme.
	///
	/// If `[seed]` is `Some` then the key will be ephemeral and stored in memory.
	fn ecdsa_generate_new(
		&self,
		key_type: KeyTypeId,
		seed: Option<&str>,
	) -> std::result::Result<ecdsa::Public, TraitError> {
		self.generate_new::<ecdsa::Pair>(key_type, seed)
	}

	fn ecdsa_sign(
		&self,
		key_type: KeyTypeId,
		public: &ecdsa::Public,
		msg: &[u8],
	) -> std::result::Result<Option<ecdsa::Signature>, TraitError> {
		self.sign::<ecdsa::Pair>(key_type, public, msg)
	}

	fn ecdsa_sign_prehashed(
		&self,
		key_type: KeyTypeId,
		public: &ecdsa::Public,
		msg: &[u8; 32],
	) -> std::result::Result<Option<ecdsa::Signature>, TraitError> {
		let sig = self
			.0
			.read()
			.key_pair_by_type::<ecdsa::Pair>(public, key_type)?
			.map(|pair| pair.sign_prehashed(msg));
		Ok(sig)
	}

	sp_keystore::bandersnatch_experimental_enabled! {
		fn bandersnatch_public_keys(&self, key_type: KeyTypeId) -> Vec<bandersnatch::Public> {
			self.public_keys::<bandersnatch::Pair>(key_type)
		}

		/// Generate a new pair compatible with the 'bandersnatch' signature scheme.
		///
		/// If `[seed]` is `Some` then the key will be ephemeral and stored in memory.
		fn bandersnatch_generate_new(
			&self,
			key_type: KeyTypeId,
			seed: Option<&str>,
		) -> std::result::Result<bandersnatch::Public, TraitError> {
			self.generate_new::<bandersnatch::Pair>(key_type, seed)
		}

		fn bandersnatch_sign(
			&self,
			key_type: KeyTypeId,
			public: &bandersnatch::Public,
			msg: &[u8],
		) -> std::result::Result<Option<bandersnatch::Signature>, TraitError> {
			self.sign::<bandersnatch::Pair>(key_type, public, msg)
		}

		fn bandersnatch_vrf_sign(
			&self,
			key_type: KeyTypeId,
			public: &bandersnatch::Public,
			data: &bandersnatch::vrf::VrfSignData,
		) -> std::result::Result<Option<bandersnatch::vrf::VrfSignature>, TraitError> {
			self.vrf_sign::<bandersnatch::Pair>(key_type, public, data)
		}

		fn bandersnatch_vrf_pre_output(
			&self,
			key_type: KeyTypeId,
			public: &bandersnatch::Public,
			input: &bandersnatch::vrf::VrfInput,
		) -> std::result::Result<Option<bandersnatch::vrf::VrfPreOutput>, TraitError> {
			self.vrf_pre_output::<bandersnatch::Pair>(key_type, public, input)
		}

		fn bandersnatch_ring_vrf_sign(
			&self,
			key_type: KeyTypeId,
			public: &bandersnatch::Public,
			data: &bandersnatch::vrf::VrfSignData,
			prover: &bandersnatch::ring_vrf::RingProver,
		) -> std::result::Result<Option<bandersnatch::ring_vrf::RingVrfSignature>, TraitError> {
			let sig = self
				.0
				.read()
				.key_pair_by_type::<bandersnatch::Pair>(public, key_type)?
				.map(|pair| pair.ring_vrf_sign(data, prover));
			Ok(sig)
		}
	}

	sp_keystore::bls_experimental_enabled! {
		fn bls381_public_keys(&self, key_type: KeyTypeId) -> Vec<bls381::Public> {
			self.public_keys::<bls381::Pair>(key_type)
		}

		/// Generate a new pair compatible with the 'bls381' signature scheme.
		///
		/// If `[seed]` is `Some` then the key will be ephemeral and stored in memory.
		fn bls381_generate_new(
			&self,
			key_type: KeyTypeId,
			seed: Option<&str>,
		) -> std::result::Result<bls381::Public, TraitError> {
			self.generate_new::<bls381::Pair>(key_type, seed)
		}

		fn bls381_sign(
			&self,
			key_type: KeyTypeId,
			public: &bls381::Public,
			msg: &[u8],
		) -> std::result::Result<Option<bls381::Signature>, TraitError> {
			self.sign::<bls381::Pair>(key_type, public, msg)
		}

		fn bls381_generate_proof_of_possession(
			&self,
			key_type: KeyTypeId,
			public: &bls381::Public,
			owner: &[u8],
		) -> std::result::Result<Option<bls381::ProofOfPossession>, TraitError> {
			self.generate_proof_of_possession::<bls381::Pair>(key_type, public, owner)
		}

		fn ecdsa_bls381_public_keys(&self, key_type: KeyTypeId) -> Vec<ecdsa_bls381::Public> {
			self.public_keys::<ecdsa_bls381::Pair>(key_type)
		}

		/// Generate a new pair of paired-keys compatible with the '(ecdsa,bls381)' signature scheme.
		///
		/// If `[seed]` is `Some` then the key will be ephemeral and stored in memory.
		fn ecdsa_bls381_generate_new(
			&self,
			key_type: KeyTypeId,
			seed: Option<&str>,
		) -> std::result::Result<ecdsa_bls381::Public, TraitError> {
			let pubkey = self.generate_new::<ecdsa_bls381::Pair>(key_type, seed)?;

			let seed = self
				.0
				.read()
				.key_phrase_by_type(pubkey.as_ref(), key_type)
				.map_err(TraitError::from)?
				.ok_or_else(|| {
					TraitError::Other(
						"Phrase for just-generated ecdsa_bls381 key not found in keystore"
							.to_string(),
					)
				})?;

			// This is done to give the keystore access to individual keys, this is necessary to avoid
			// unnecessary host functions for paired keys and re-use host functions implemented for each
			// element of the pair.
			self.generate_new::<ecdsa::Pair>(key_type, Some(&*seed))?;
			self.generate_new::<bls381::Pair>(key_type, Some(&*seed))?;

			Ok(pubkey)
		}

		fn ecdsa_bls381_sign(
			&self,
			key_type: KeyTypeId,
			public: &ecdsa_bls381::Public,
			msg: &[u8],
		) -> std::result::Result<Option<ecdsa_bls381::Signature>, TraitError> {
			self.sign::<ecdsa_bls381::Pair>(key_type, public, msg)
		}

		fn ecdsa_bls381_sign_with_keccak256(
			&self,
			key_type: KeyTypeId,
			public: &ecdsa_bls381::Public,
			msg: &[u8],
		) -> std::result::Result<Option<ecdsa_bls381::Signature>, TraitError> {
			 let sig = self.0
			.read()
			.key_pair_by_type::<ecdsa_bls381::Pair>(public, key_type)?
			.map(|pair| pair.sign_with_hasher::<KeccakHasher>(msg));
			Ok(sig)
		}
	}
}

impl Into<KeystorePtr> for LocalKeystore {
	fn into(self) -> KeystorePtr {
		Arc::new(self)
	}
}

/// A local key store.
///
/// Stores key pairs in a file system store + short lived key pairs in memory.
///
/// Every pair that is being generated by a `seed`, will be placed in memory.
struct KeystoreInner {
	path: Option<PathBuf>,
	/// Map over `(KeyTypeId, Raw public key)` -> `Key phrase/seed`
	additional: HashMap<(KeyTypeId, Vec<u8>), String>,
	password: Option<SecretString>,
}

impl KeystoreInner {
	/// Open the store at the given path.
	///
	/// Optionally takes a password that will be used to encrypt/decrypt the keys.
	fn open<T: Into<PathBuf>>(path: T, password: Option<SecretString>) -> Result<Self> {
		let path = path.into();
		fs::create_dir_all(&path)?;

		Ok(Self { path: Some(path), additional: HashMap::new(), password })
	}

	/// Get the password for this store.
	fn password(&self) -> Option<&str> {
		self.password.as_ref().map(|p| p.expose_secret()).map(|p| p.as_str())
	}

	/// Create a new in-memory store.
	fn new_in_memory() -> Self {
		Self { path: None, additional: HashMap::new(), password: None }
	}

	/// Get the key phrase for the given public key and key type from the in-memory store.
	fn get_additional_pair(&self, public: &[u8], key_type: KeyTypeId) -> Option<&String> {
		let key = (key_type, public.to_vec());
		self.additional.get(&key)
	}

	/// Insert the given public/private key pair with the given key type.
	///
	/// Does not place it into the file system store.
	fn insert_ephemeral_pair<Pair: CorePair>(
		&mut self,
		pair: &Pair,
		seed: &str,
		key_type: KeyTypeId,
	) {
		let key = (key_type, pair.public().to_raw_vec());
		self.additional.insert(key, seed.into());
	}

	/// Insert a new key with anonymous crypto.
	///
	/// Places it into the file system store, if a path is configured.
	fn insert(&self, key_type: KeyTypeId, suri: &str, public: &[u8]) -> Result<()> {
		if let Some(path) = self.key_file_path(public, key_type) {
			Self::write_to_file(path, suri)?;
		}

		Ok(())
	}

	/// Generate a new key.
	///
	/// Places it into the file system store, if a path is configured. Otherwise insert
	/// it into the memory cache only.
	fn generate_by_type<Pair: CorePair>(&mut self, key_type: KeyTypeId) -> Result<Pair> {
		let (pair, phrase, _) = Pair::generate_with_phrase(self.password());
		if let Some(path) = self.key_file_path(pair.public().as_slice(), key_type) {
			Self::write_to_file(path, &phrase)?;
		} else {
			self.insert_ephemeral_pair(&pair, &phrase, key_type);
		}

		Ok(pair)
	}

	/// Write the given `data` to `file`.
	fn write_to_file(file: PathBuf, data: &str) -> Result<()> {
		#[cfg(target_family = "unix")]
		let mut file = {
			use std::os::unix::fs::OpenOptionsExt;
			fs::OpenOptions::new()
				.write(true)
				.create(true)
				.truncate(true)
				.mode(0o600)
				.open(file)?
		};
		#[cfg(not(target_family = "unix"))]
		let mut file = File::create(file)?;

		serde_json::to_writer(&file, data)?;
		file.flush()?;
		Ok(())
	}

	/// Returns `true` if the plain hex-encoded filename for `public` would exceed the
	/// filesystem's filename-length limit, requiring the hashed-filename fallback.
	///
	/// The key-type prefix is always [`KEY_TYPE_PREFIX_LEN`] bytes regardless of the
	/// concrete `KeyTypeId`, so the predicate is a pure function of `public.len()`.
	fn requires_hashed_filename(public: &[u8]) -> bool {
		// 2 hex chars per byte; KEY_TYPE_PREFIX_LEN bytes are always present.
		(KEY_TYPE_PREFIX_LEN + public.len()) * 2 > MAX_FILENAME_LEN
	}

	/// Read the phrase stored in the key file at `path`.
	fn read_phrase(path: &Path) -> Result<String> {
		let file = File::open(path)?;
		serde_json::from_reader(&file).map_err(Into::into)
	}

	sp_keystore::bls_experimental_enabled! {
		/// Recover the public key of a key file with a hashed name.
		///
		/// The name only carries `blake2_256(public_key)`, so the key is re-derived from the
		/// stored `phrase` with every scheme whose public key is long enough to need a hashed
		/// name, and the derivation hashing to `hash` wins. Returns `None` if none does, e.g.
		/// because the file wasn't written by this keystore or the password is wrong.
		fn recover_hashed_public(&self, phrase: &str, hash: &[u8]) -> Option<Vec<u8>> {
			self.public_matching_hash::<bls381::Pair>(phrase, hash)
				.or_else(|| self.public_matching_hash::<ecdsa_bls381::Pair>(phrase, hash))
		}

		/// Derive a `Pair` from `phrase` and return its public key if it hashes to `hash`.
		fn public_matching_hash<Pair: CorePair>(
			&self,
			phrase: &str,
			hash: &[u8],
		) -> Option<Vec<u8>> {
			let public = Pair::from_string(phrase, self.password()).ok()?.public().to_raw_vec();
			(sp_crypto_hashing::blake2_256(&public) == hash).then_some(public)
		}
	}

	/// Without a scheme whose public key needs a hashed name compiled in, nothing can be
	/// recovered: such entries stay usable through lookups by public key but aren't enumerated.
	#[cfg(not(feature = "bls-experimental"))]
	fn recover_hashed_public(&self, _phrase: &str, _hash: &[u8]) -> Option<Vec<u8>> {
		None
	}

	/// Create a new key from seed.
	///
	/// Does not place it into the file system store.
	fn insert_ephemeral_from_seed_by_type<Pair: CorePair>(
		&mut self,
		seed: &str,
		key_type: KeyTypeId,
	) -> Result<Pair> {
		let pair = Pair::from_string(seed, None).map_err(|_| Error::InvalidSeed)?;
		self.insert_ephemeral_pair(&pair, seed, key_type);
		Ok(pair)
	}

	/// Get the key phrase for a given public key and key type.
	fn key_phrase_by_type(&self, public: &[u8], key_type: KeyTypeId) -> Result<Option<String>> {
		if let Some(phrase) = self.get_additional_pair(public, key_type) {
			return Ok(Some(phrase.clone()));
		}

		let path = if let Some(path) = self.key_file_path(public, key_type) {
			path
		} else {
			return Ok(None);
		};

		if path.exists() {
			Self::read_phrase(&path).map(Some)
		} else {
			Ok(None)
		}
	}

	/// Get a key pair for the given public key and key type.
	fn key_pair_by_type<Pair: CorePair>(
		&self,
		public: &Pair::Public,
		key_type: KeyTypeId,
	) -> Result<Option<Pair>> {
		let phrase = if let Some(p) = self.key_phrase_by_type(public.as_slice(), key_type)? {
			p
		} else {
			return Ok(None);
		};

		let pair = Pair::from_string(&phrase, self.password()).map_err(|_| Error::InvalidPhrase)?;

		if &pair.public() == public {
			Ok(Some(pair))
		} else {
			Err(Error::PublicKeyMismatch)
		}
	}

	/// Get the file path for the given public key and key type.
	///
	/// Returns `None` if the keystore only exists in-memory and there isn't any path to provide.
	///
	/// Two file naming schemes are used, the file body being the JSON-encoded phrase in both:
	///
	/// * **Plain** (default and backwards-compatible): `hex(key_type || public_key)`.
	/// * **Hashed**: used when the plain name would exceed [`MAX_FILENAME_LEN`] (notably for BLS381
	///   / ECDSA_BLS381 public keys): `hex(key_type || blake2_256(public_key))` followed by
	///   [`HASHED_FILENAME_SUFFIX`]. The public key is re-derived from the phrase when needed, see
	///   [`Self::recover_hashed_public`].
	fn key_file_path(&self, public: &[u8], key_type: KeyTypeId) -> Option<PathBuf> {
		let mut buf = self.path.as_ref()?.clone();
		let key_type = array_bytes::bytes2hex("", &key_type.0);
		let name = if Self::requires_hashed_filename(public) {
			let hash = array_bytes::bytes2hex("", sp_crypto_hashing::blake2_256(public));
			key_type + &hash + HASHED_FILENAME_SUFFIX
		} else {
			key_type + &array_bytes::bytes2hex("", public)
		};
		buf.push(name);
		Some(buf)
	}

	/// Returns a list of raw public keys filtered by `KeyTypeId`
	fn raw_public_keys(&self, key_type: KeyTypeId) -> Result<Vec<Vec<u8>>> {
		let mut public_keys: Vec<Vec<u8>> = self
			.additional
			.keys()
			.into_iter()
			.filter_map(|k| if k.0 == key_type { Some(k.1.clone()) } else { None })
			.collect();

		if let Some(path) = &self.path {
			for entry in fs::read_dir(&path)? {
				let path = entry?.path();

				// skip directories and non-unicode file names
				let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
				let (hex, hashed) = match name.strip_suffix(HASHED_FILENAME_SUFFIX) {
					Some(hex) => (hex, true),
					None => (name, false),
				};
				// skip files that don't name a key of the requested type
				let payload = match array_bytes::hex2bytes(hex) {
					Ok(bytes)
						if bytes.len() > KEY_TYPE_PREFIX_LEN &&
							bytes[..KEY_TYPE_PREFIX_LEN] == key_type.0 =>
					{
						bytes[KEY_TYPE_PREFIX_LEN..].to_vec()
					},
					_ => continue,
				};
				let public = if hashed {
					// The name only carries a hash of the public key: re-derive it from the phrase.
					match Self::read_phrase(&path)
						.ok()
						.and_then(|phrase| self.recover_hashed_public(&phrase, &payload))
					{
						Some(public) => public,
						None => continue,
					}
				} else {
					payload
				};
				public_keys.push(public);
			}
		}

		Ok(public_keys)
	}

	/// Get a key pair for the given public key.
	///
	/// Returns `Ok(None)` if the key doesn't exist, `Ok(Some(_))` if the key exists or `Err(_)`
	/// when something failed.
	pub fn key_pair<Pair: AppPair>(
		&self,
		public: &<Pair as AppCrypto>::Public,
	) -> Result<Option<Pair>> {
		self.key_pair_by_type::<Pair::Generic>(IsWrappedBy::from_ref(public), Pair::ID)
			.map(|v| v.map(Into::into))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_application_crypto::{ed25519, sr25519, AppPublic};
	use sp_core::{crypto::Ss58Codec, testing::SR25519, Pair};
	use std::{fs, str::FromStr};
	use tempfile::TempDir;

	const TEST_KEY_TYPE: KeyTypeId = KeyTypeId(*b"test");

	impl KeystoreInner {
		fn insert_ephemeral_from_seed<Pair: AppPair>(&mut self, seed: &str) -> Result<Pair> {
			self.insert_ephemeral_from_seed_by_type::<Pair::Generic>(seed, Pair::ID)
				.map(Into::into)
		}

		fn public_keys<Public: AppPublic>(&self) -> Result<Vec<Public>> {
			self.raw_public_keys(Public::ID).map(|v| {
				v.into_iter().filter_map(|k| Public::from_slice(k.as_slice()).ok()).collect()
			})
		}

		fn generate<Pair: AppPair>(&mut self) -> Result<Pair> {
			self.generate_by_type::<Pair::Generic>(Pair::ID).map(Into::into)
		}
	}

	#[test]
	fn basic_store() {
		let temp_dir = TempDir::new().unwrap();
		let mut store = KeystoreInner::open(temp_dir.path(), None).unwrap();

		assert!(store.public_keys::<ed25519::AppPublic>().unwrap().is_empty());

		let key: ed25519::AppPair = store.generate().unwrap();
		let key2: ed25519::AppPair = store.key_pair(&key.public()).unwrap().unwrap();

		assert_eq!(key.public(), key2.public());

		assert_eq!(store.public_keys::<ed25519::AppPublic>().unwrap()[0], key.public());
	}

	#[test]
	fn has_keys_works() {
		let temp_dir = TempDir::new().unwrap();
		let store = LocalKeystore::open(temp_dir.path(), None).unwrap();

		let key: ed25519::AppPair = store.0.write().generate().unwrap();
		let key2 = ed25519::Pair::generate().0;

		assert!(!store.has_keys(&[(key2.public().to_vec(), ed25519::AppPublic::ID)]));

		assert!(!store.has_keys(&[
			(key2.public().to_vec(), ed25519::AppPublic::ID),
			(key.public().to_raw_vec(), ed25519::AppPublic::ID),
		],));

		assert!(store.has_keys(&[(key.public().to_raw_vec(), ed25519::AppPublic::ID)]));
	}

	#[test]
	fn test_insert_ephemeral_from_seed() {
		let temp_dir = TempDir::new().unwrap();
		let mut store = KeystoreInner::open(temp_dir.path(), None).unwrap();

		let pair: ed25519::AppPair = store
			.insert_ephemeral_from_seed(
				"0x3d97c819d68f9bafa7d6e79cb991eebcd77d966c5334c0b94d9e1fa7ad0869dc",
			)
			.unwrap();
		assert_eq!(
			"5DKUrgFqCPV8iAXx9sjy1nyBygQCeiUYRFWurZGhnrn3HJCA",
			pair.public().to_ss58check()
		);

		drop(store);
		let store = KeystoreInner::open(temp_dir.path(), None).unwrap();
		// Keys generated from seed should not be persisted!
		assert!(store.key_pair::<ed25519::AppPair>(&pair.public()).unwrap().is_none());
	}

	#[test]
	fn password_being_used() {
		let password = String::from("password");
		let temp_dir = TempDir::new().unwrap();
		let mut store = KeystoreInner::open(
			temp_dir.path(),
			Some(FromStr::from_str(password.as_str()).unwrap()),
		)
		.unwrap();

		let pair: ed25519::AppPair = store.generate().unwrap();
		assert_eq!(
			pair.public(),
			store.key_pair::<ed25519::AppPair>(&pair.public()).unwrap().unwrap().public(),
		);

		// Without the password the key should not be retrievable
		let store = KeystoreInner::open(temp_dir.path(), None).unwrap();
		assert!(store.key_pair::<ed25519::AppPair>(&pair.public()).is_err());

		let store = KeystoreInner::open(
			temp_dir.path(),
			Some(FromStr::from_str(password.as_str()).unwrap()),
		)
		.unwrap();
		assert_eq!(
			pair.public(),
			store.key_pair::<ed25519::AppPair>(&pair.public()).unwrap().unwrap().public(),
		);
	}

	#[test]
	fn public_keys_are_returned() {
		let temp_dir = TempDir::new().unwrap();
		let mut store = KeystoreInner::open(temp_dir.path(), None).unwrap();

		let mut keys = Vec::new();
		for i in 0..10 {
			keys.push(store.generate::<ed25519::AppPair>().unwrap().public());
			keys.push(
				store
					.insert_ephemeral_from_seed::<ed25519::AppPair>(&format!(
						"0x3d97c819d68f9bafa7d6e79cb991eebcd7{}d966c5334c0b94d9e1fa7ad0869dc",
						i
					))
					.unwrap()
					.public(),
			);
		}

		// Generate a key of a different type
		store.generate::<sr25519::AppPair>().unwrap();

		keys.sort();
		let mut store_pubs = store.public_keys::<ed25519::AppPublic>().unwrap();
		store_pubs.sort();

		assert_eq!(keys, store_pubs);
	}

	#[test]
	fn store_unknown_and_extract_it() {
		let temp_dir = TempDir::new().unwrap();
		let store = KeystoreInner::open(temp_dir.path(), None).unwrap();

		let secret_uri = "//Alice";
		let key_pair = sr25519::AppPair::from_string(secret_uri, None).expect("Generates key pair");

		store
			.insert(SR25519, secret_uri, key_pair.public().as_ref())
			.expect("Inserts unknown key");

		let store_key_pair = store
			.key_pair_by_type::<sr25519::AppPair>(&key_pair.public(), SR25519)
			.expect("Gets key pair from keystore")
			.unwrap();

		assert_eq!(key_pair.public(), store_key_pair.public());
	}

	#[test]
	fn store_ignores_files_with_invalid_name() {
		let temp_dir = TempDir::new().unwrap();
		let store = LocalKeystore::open(temp_dir.path(), None).unwrap();

		let file_name = temp_dir.path().join(array_bytes::bytes2hex("", &SR25519.0[..2]));
		fs::write(file_name, "test").expect("Invalid file is written");

		assert!(store.sr25519_public_keys(SR25519).is_empty());
	}

	#[test]
	fn generate_with_seed_is_not_stored() {
		let temp_dir = TempDir::new().unwrap();
		let store = LocalKeystore::open(temp_dir.path(), None).unwrap();
		let _alice_tmp_key = store.sr25519_generate_new(TEST_KEY_TYPE, Some("//Alice")).unwrap();

		assert_eq!(store.sr25519_public_keys(TEST_KEY_TYPE).len(), 1);

		drop(store);
		let store = LocalKeystore::open(temp_dir.path(), None).unwrap();
		assert_eq!(store.sr25519_public_keys(TEST_KEY_TYPE).len(), 0);
	}

	#[test]
	fn generate_can_be_fetched_in_memory() {
		let store = LocalKeystore::in_memory();
		store.sr25519_generate_new(TEST_KEY_TYPE, Some("//Alice")).unwrap();

		assert_eq!(store.sr25519_public_keys(TEST_KEY_TYPE).len(), 1);
		store.sr25519_generate_new(TEST_KEY_TYPE, None).unwrap();
		assert_eq!(store.sr25519_public_keys(TEST_KEY_TYPE).len(), 2);
	}

	#[test]
	#[cfg(target_family = "unix")]
	fn uses_correct_file_permissions_on_unix() {
		use std::os::unix::fs::PermissionsExt;

		let temp_dir = TempDir::new().unwrap();
		let store = LocalKeystore::open(temp_dir.path(), None).unwrap();

		let public = store.sr25519_generate_new(TEST_KEY_TYPE, None).unwrap();

		let path = store.0.read().key_file_path(public.as_ref(), TEST_KEY_TYPE).unwrap();
		let permissions = File::open(path).unwrap().metadata().unwrap().permissions();

		assert_eq!(0o100600, permissions.mode());
	}

	#[test]
	#[cfg(feature = "bls-experimental")]
	fn ecdsa_bls381_generate_with_none_works() {
		use sp_core::testing::ECDSA_BLS381;

		let store = LocalKeystore::in_memory();
		let ecdsa_bls381_key =
			store.ecdsa_bls381_generate_new(ECDSA_BLS381, None).expect("Cant generate key");

		let ecdsa_keys = store.ecdsa_public_keys(ECDSA_BLS381);
		let bls381_keys = store.bls381_public_keys(ECDSA_BLS381);
		let ecdsa_bls381_keys = store.ecdsa_bls381_public_keys(ECDSA_BLS381);

		assert_eq!(ecdsa_keys.len(), 1);
		assert_eq!(bls381_keys.len(), 1);
		assert_eq!(ecdsa_bls381_keys.len(), 1);

		let ecdsa_key = ecdsa_keys[0];
		let bls381_key = bls381_keys[0];

		let mut combined_key_raw = [0u8; ecdsa_bls381::PUBLIC_KEY_LEN];
		combined_key_raw[..ecdsa::PUBLIC_KEY_SERIALIZED_SIZE].copy_from_slice(ecdsa_key.as_ref());
		combined_key_raw[ecdsa::PUBLIC_KEY_SERIALIZED_SIZE..].copy_from_slice(bls381_key.as_ref());
		let combined_key = ecdsa_bls381::Public::from_raw(combined_key_raw);

		assert_eq!(combined_key, ecdsa_bls381_key);
	}

	#[test]
	#[cfg(feature = "bls-experimental")]
	fn ecdsa_bls381_generate_with_seed_works() {
		use sp_core::testing::ECDSA_BLS381;

		let store = LocalKeystore::in_memory();
		let ecdsa_bls381_key = store
			.ecdsa_bls381_generate_new(ECDSA_BLS381, Some("//Alice"))
			.expect("Cant generate key");

		let ecdsa_keys = store.ecdsa_public_keys(ECDSA_BLS381);
		let bls381_keys = store.bls381_public_keys(ECDSA_BLS381);
		let ecdsa_bls381_keys = store.ecdsa_bls381_public_keys(ECDSA_BLS381);

		assert_eq!(ecdsa_keys.len(), 1);
		assert_eq!(bls381_keys.len(), 1);
		assert_eq!(ecdsa_bls381_keys.len(), 1);

		let ecdsa_key = ecdsa_keys[0];
		let bls381_key = bls381_keys[0];

		let mut combined_key_raw = [0u8; ecdsa_bls381::PUBLIC_KEY_LEN];
		combined_key_raw[..ecdsa::PUBLIC_KEY_SERIALIZED_SIZE].copy_from_slice(ecdsa_key.as_ref());
		combined_key_raw[ecdsa::PUBLIC_KEY_SERIALIZED_SIZE..].copy_from_slice(bls381_key.as_ref());
		let combined_key = ecdsa_bls381::Public::from_raw(combined_key_raw);

		assert_eq!(combined_key, ecdsa_bls381_key);
	}

	#[test]
	#[cfg(feature = "bls-experimental")]
	fn file_backed_keystore_works_for_all_key_types() {
		use sp_core::testing::{BLS381, ECDSA, ECDSA_BLS381, ED25519, SR25519};

		let temp_dir = TempDir::new().unwrap();
		let store = LocalKeystore::open(temp_dir.path(), None).unwrap();
		store.ed25519_generate_new(ED25519, None).expect("ed25519 works");
		store.sr25519_generate_new(SR25519, None).expect("sr25519 works");
		store.ecdsa_generate_new(ECDSA, None).expect("ecdsa works");
		store.bls381_generate_new(BLS381, None).expect("bls381 works");
		store.ecdsa_bls381_generate_new(ECDSA_BLS381, None).expect("ecdsa_bls381 works");
	}

	#[test]
	#[cfg(feature = "bls-experimental")]
	fn ecdsa_bls381_generate_with_none_works_on_disk() {
		use sp_core::testing::ECDSA_BLS381;

		// File-backed counterpart to `ecdsa_bls381_generate_with_none_works`. Triggers
		// the historical panic in the missing-`additional` code path.
		let temp_dir = TempDir::new().unwrap();
		let store = LocalKeystore::open(temp_dir.path(), None).unwrap();
		let ecdsa_bls381_key = store
			.ecdsa_bls381_generate_new(ECDSA_BLS381, None)
			.expect("must not panic on filesystem-backed keystore");

		let ecdsa_keys = store.ecdsa_public_keys(ECDSA_BLS381);
		let bls381_keys = store.bls381_public_keys(ECDSA_BLS381);
		let ecdsa_bls381_keys = store.ecdsa_bls381_public_keys(ECDSA_BLS381);

		assert_eq!(ecdsa_keys.len(), 1);
		assert_eq!(bls381_keys.len(), 1);
		assert_eq!(ecdsa_bls381_keys.len(), 1);

		let ecdsa_key = ecdsa_keys[0];
		let bls381_key = bls381_keys[0];

		let mut combined_key_raw = [0u8; ecdsa_bls381::PUBLIC_KEY_LEN];
		combined_key_raw[..ecdsa::PUBLIC_KEY_SERIALIZED_SIZE].copy_from_slice(ecdsa_key.as_ref());
		combined_key_raw[ecdsa::PUBLIC_KEY_SERIALIZED_SIZE..].copy_from_slice(bls381_key.as_ref());
		let combined_key = ecdsa_bls381::Public::from_raw(combined_key_raw);

		assert_eq!(combined_key, ecdsa_bls381_key);
	}

	#[cfg(feature = "bls-experimental")]
	fn assert_filenames_within_filesystem_limit(dir: &Path) {
		for entry in fs::read_dir(dir).unwrap() {
			let entry = entry.unwrap();
			let name = entry.file_name();
			let len = name.len();
			assert!(
				len <= MAX_FILENAME_LEN,
				"keystore filename `{}` is {} bytes, exceeds the {}-byte filesystem limit",
				name.to_string_lossy(),
				len,
				MAX_FILENAME_LEN,
			);
		}
	}

	#[test]
	#[cfg(feature = "bls-experimental")]
	fn bls_filenames_stay_within_filesystem_limit() {
		use sp_core::testing::{BLS381, ECDSA_BLS381};

		let temp_dir = TempDir::new().unwrap();
		let store = LocalKeystore::open(temp_dir.path(), None).unwrap();
		let bls381 = store.bls381_generate_new(BLS381, None).unwrap();
		let ecdsa_bls381 = store.ecdsa_bls381_generate_new(ECDSA_BLS381, None).unwrap();

		assert_filenames_within_filesystem_limit(temp_dir.path());

		// The public keys must still be enumerable.
		assert_eq!(store.bls381_public_keys(BLS381), vec![bls381]);
		assert_eq!(store.ecdsa_bls381_public_keys(ECDSA_BLS381), vec![ecdsa_bls381]);
	}

	#[test]
	#[cfg(feature = "bls-experimental")]
	fn bls_keys_persist_across_reopens() {
		use sp_core::testing::{BLS381, ECDSA_BLS381};

		let temp_dir = TempDir::new().unwrap();
		let (bls381, ecdsa_bls381) = {
			let store = LocalKeystore::open(temp_dir.path(), None).unwrap();
			(
				store.bls381_generate_new(BLS381, None).unwrap(),
				store.ecdsa_bls381_generate_new(ECDSA_BLS381, None).unwrap(),
			)
		};

		let store = LocalKeystore::open(temp_dir.path(), None).unwrap();
		assert_eq!(store.bls381_public_keys(BLS381), vec![bls381]);
		assert_eq!(store.ecdsa_bls381_public_keys(ECDSA_BLS381), vec![ecdsa_bls381]);

		// Signing after a reopen must work from the on-disk phrase of a hashed-name entry.
		assert!(store.bls381_sign(BLS381, &bls381, b"message").unwrap().is_some());
		assert!(store
			.ecdsa_bls381_sign(ECDSA_BLS381, &ecdsa_bls381, b"message")
			.unwrap()
			.is_some());
	}

	#[test]
	#[cfg(feature = "bls-experimental")]
	fn hashed_entries_are_verified_during_enumeration() {
		use sp_core::testing::BLS381;

		let temp_dir = TempDir::new().unwrap();
		let store = LocalKeystore::open(temp_dir.path(), None).unwrap();
		let legit = store.bls381_generate_new(BLS381, None).unwrap();

		// A hashed name is tied to its key only through the hash, so a file dropped into the
		// keystore directory whose phrase doesn't derive to the hashed public key must not be
		// enumerated as a key.
		let forged_name = format!(
			"{}{}{}",
			array_bytes::bytes2hex("", &BLS381.0),
			array_bytes::bytes2hex("", sp_crypto_hashing::blake2_256(b"unrelated")),
			HASHED_FILENAME_SUFFIX,
		);
		fs::write(temp_dir.path().join(forged_name), serde_json::to_vec("//Eve").unwrap()).unwrap();

		// Nor must a hashed name be mistaken for the plain name of a 32-byte public key.
		let sr25519 = store.sr25519_generate_new(BLS381, None).unwrap();

		assert_eq!(store.bls381_public_keys(BLS381), vec![legit]);
		assert_eq!(store.sr25519_public_keys(BLS381), vec![sr25519]);
	}
}
