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

//! VRFs backed by [Bandersnatch](https://neuromancer.sk/std/bls/Bandersnatch),
//! an elliptic curve built over BLS12-381 scalar field.
//!
//! The primitive can operate both as a regular VRF or as an anonymized Ring VRF.

#[cfg(feature = "full_crypto")]
use crate::crypto::VrfSecret;
use crate::crypto::{
	ByteArray, CryptoType, CryptoTypeId, DeriveError, DeriveJunction, Pair as TraitPair,
	PublicBytes, SecretStringError, SignatureBytes, UncheckedFrom, VrfPublic,
};
use ark_ec_vrfs::{
	prelude::ark_serialize::{CanonicalDeserialize, CanonicalSerialize},
	ring::RingSuite,
	suites::bandersnatch::edwards as bandersnatch,
};
use bandersnatch::Secret;
use codec::{Decode, DecodeWithMemTracking, Encode, EncodeLike, MaxEncodedLen};
use scale_info::TypeInfo;

use alloc::vec::Vec;

/// Identifier used to match public keys against bandersnatch-vrf keys.
pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"band");

/// Context used to produce a plain signature without any VRF input/output.
pub const SIGNING_CTX: &[u8] = b"BandersnatchSigningContext";

/// The byte length of secret key seed.
pub const SEED_SERIALIZED_SIZE: usize = 32;

/// The byte length of serialized public key.
pub const PUBLIC_SERIALIZED_SIZE: usize = 32;

/// The byte length of serialized signature.
pub const SIGNATURE_SERIALIZED_SIZE: usize = 64;

/// The byte length of serialized pre-output.
pub const PREOUT_SERIALIZED_SIZE: usize = 32;

#[doc(hidden)]
pub struct BandersnatchTag;

/// Bandersnatch public key.
pub type Public = PublicBytes<PUBLIC_SERIALIZED_SIZE, BandersnatchTag>;

impl CryptoType for Public {
	type Pair = Pair;
}

/// Bandersnatch signature.
///
/// The signature is created via [`VrfSecret::vrf_sign`] using [`SIGNING_CTX`] as transcript
/// `label`.
pub type Signature = SignatureBytes<SIGNATURE_SERIALIZED_SIZE, BandersnatchTag>;

impl CryptoType for Signature {
	type Pair = Pair;
}

/// The raw secret seed, which can be used to reconstruct the secret [`Pair`].
type Seed = [u8; SEED_SERIALIZED_SIZE];

/// Bandersnatch secret key.
#[derive(Clone)]
pub struct Pair {
	secret: Secret,
	seed: Seed,
}

impl Pair {
	/// Get the key seed.
	pub fn seed(&self) -> Seed {
		self.seed
	}
}

impl TraitPair for Pair {
	type Seed = Seed;
	type Public = Public;
	type Signature = Signature;

	/// Make a new key pair from secret seed material.
	///
	/// The slice must be 32 bytes long or it will return an error.
	fn from_seed_slice(seed_slice: &[u8]) -> Result<Pair, SecretStringError> {
		if seed_slice.len() != SEED_SERIALIZED_SIZE {
			return Err(SecretStringError::InvalidSeedLength)
		}
		let mut seed = [0; SEED_SERIALIZED_SIZE];
		seed.copy_from_slice(seed_slice);
		let secret = Secret::from_seed(&seed);
		Ok(Pair { secret, seed })
	}

	/// Derive a child key from a series of given (hard) junctions.
	///
	/// Soft junctions are not supported.
	fn derive<Iter: Iterator<Item = DeriveJunction>>(
		&self,
		path: Iter,
		_seed: Option<Seed>,
	) -> Result<(Pair, Option<Seed>), DeriveError> {
		let derive_hard = |seed, cc| -> Seed {
			("bandersnatch-vrf-HDKD", seed, cc).using_encoded(sp_crypto_hashing::blake2_256)
		};

		let mut seed = self.seed();
		for p in path {
			if let DeriveJunction::Hard(cc) = p {
				seed = derive_hard(seed, cc);
			} else {
				return Err(DeriveError::SoftKeyInPath)
			}
		}
		Ok((Self::from_seed(&seed), Some(seed)))
	}

	fn public(&self) -> Public {
		let public = self.secret.public();
		let mut raw = [0; PUBLIC_SERIALIZED_SIZE];
		public
			.serialize_compressed(raw.as_mut_slice())
			.expect("serialization length is constant and checked by test; qed");
		Public::unchecked_from(raw)
	}

	/// Sign a message.
	///
	/// In practice this produce a Schnorr signature.
	#[cfg(feature = "full_crypto")]
	fn sign(&self, data: &[u8]) -> Signature {
		use ark_ec_vrfs::Suite;
		use bandersnatch::BandersnatchSha512Ell2;
		let input = bandersnatch::Input::new(data).unwrap();
		let k = BandersnatchSha512Ell2::nonce(&self.secret.scalar, input);
		let c = BandersnatchSha512Ell2::challenge(&[&self.secret.public.0, &input.0], data);
		let s = k + c * self.secret.scalar;
		let mut raw_signature = [0_u8; SIGNATURE_SERIALIZED_SIZE];
		bandersnatch::IetfProof { c, s }
			.serialize_compressed(&mut raw_signature.as_mut_slice())
			.unwrap();
		Signature::from_raw(raw_signature)
	}

	fn verify<M: AsRef<[u8]>>(signature: &Signature, data: M, public: &Public) -> bool {
		let data = vrf::VrfSignData::new(SIGNING_CTX, data.as_ref());
		let dummy = ark_ec_vrfs::Output(bandersnatch::BandersnatchSha512Ell2::PADDING);
		let signature =
			vrf::VrfSignature { proof: *signature, pre_output: vrf::VrfPreOutput(dummy) };
		public.vrf_verify(&data, &signature)
	}

	/// Return a vector filled with the seed (32 bytes).
	fn to_raw_vec(&self) -> Vec<u8> {
		self.seed().to_vec()
	}
}

impl CryptoType for Pair {
	type Pair = Pair;
}

/// Bandersnatch VRF types and operations.
pub mod vrf {
	use super::*;
	use crate::crypto::VrfCrypto;
	use ark_ec_vrfs::ietf::{Prover, Verifier};

	/// VRF input to construct a [`VrfPreOutput`] instance and embeddable in [`VrfSignData`].
	#[derive(Clone, Debug)]
	pub struct VrfInput(pub(super) bandersnatch::Input);

	impl VrfInput {
		/// Construct a new VRF input.
		pub fn new(data: impl AsRef<[u8]>) -> Self {
			Self(bandersnatch::Input::new(data.as_ref()).expect("Can't fail"))
		}
	}

	/// VRF pre-output derived from [`VrfInput`] using a [`VrfSecret`].
	///
	/// This object is used to produce an arbitrary number of verifiable pseudo random
	/// bytes and is often called pre-output to emphasize that this is not the actual
	/// output of the VRF but an object capable of generating the output.
	#[derive(Clone, Debug)]
	pub struct VrfPreOutput(pub(super) bandersnatch::Output);

	// Workaround until traits are not implemented for newtypes https://github.com/davxy/ark-ec-vrfs/issues/41
	impl PartialEq for VrfPreOutput {
		fn eq(&self, other: &Self) -> bool {
			self.0 .0 == other.0 .0
		}
	}
	impl Eq for VrfPreOutput {}

	impl Encode for VrfPreOutput {
		fn encode(&self) -> Vec<u8> {
			let mut bytes = [0; PREOUT_SERIALIZED_SIZE];
			self.0
				.serialize_compressed(bytes.as_mut_slice())
				.expect("serialization length is constant and checked by test; qed");
			bytes.encode()
		}
	}

	impl Decode for VrfPreOutput {
		fn decode<R: codec::Input>(i: &mut R) -> Result<Self, codec::Error> {
			let buf = <[u8; PREOUT_SERIALIZED_SIZE]>::decode(i)?;
			let preout = bandersnatch::Output::deserialize_compressed_unchecked(buf.as_slice())
				.map_err(|_| "vrf-preout decode error: bad preout")?;
			Ok(VrfPreOutput(preout))
		}
	}

	// `VrfPreOutput` resolves to:
	// ```
	// pub struct Affine<P: TECurveConfig> {
	//     pub x: P::BaseField,
	//     pub y: P::BaseField,
	// }
	// ```
	// where each `P::BaseField` contains a `pub struct BigInt<const N: usize>(pub [u64; N]);`
	// Since none of these structures is allocated on the heap, we don't need any special
	// memory tracking logic. We can simply implement `DecodeWithMemTracking`.
	impl DecodeWithMemTracking for VrfPreOutput {}

	impl EncodeLike for VrfPreOutput {}

	impl MaxEncodedLen for VrfPreOutput {
		fn max_encoded_len() -> usize {
			<[u8; PREOUT_SERIALIZED_SIZE]>::max_encoded_len()
		}
	}

	impl TypeInfo for VrfPreOutput {
		type Identity = [u8; PREOUT_SERIALIZED_SIZE];

		fn type_info() -> scale_info::Type {
			Self::Identity::type_info()
		}
	}

	/// Data to be signed via one of the two provided vrf flavors.
	///
	/// The object contains the VRF input and additional data to be signed together
	/// with the VRF input. Additional data doesn't influence the VRF output.
	///
	/// The `input` is a [`VrfInput`]s which, during the signing procedure, is first mapped
	/// to a [`VrfPreOutput`].
	#[derive(Clone)]
	pub struct VrfSignData {
		/// VRF input.
		pub vrf_input: VrfInput,
		/// Additional data.
		pub aux_data: Vec<u8>,
	}

	impl VrfSignData {
		/// Construct a new data to be signed.
		pub fn new(vrf_input_data: &[u8], aux_data: &[u8]) -> Self {
			Self { vrf_input: VrfInput::new(vrf_input_data), aux_data: aux_data.to_owned() }
		}
	}

	/// VRF signature.
	///
	/// Includes both the VRF proof and the pre-output generated from the [`VrfSignData::input`].
	///
	/// Refer to [`VrfSignData`] for more details.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub struct VrfSignature {
		/// VRF pre-output.
		pub pre_output: VrfPreOutput,
		/// VRF proof.
		pub proof: Signature,
	}

	#[cfg(feature = "full_crypto")]
	impl VrfCrypto for Pair {
		type VrfInput = VrfInput;
		type VrfPreOutput = VrfPreOutput;
		type VrfSignData = VrfSignData;
		type VrfSignature = VrfSignature;
	}

	#[cfg(feature = "full_crypto")]
	impl VrfSecret for Pair {
		fn vrf_sign(&self, data: &Self::VrfSignData) -> Self::VrfSignature {
			let pre_output_impl = self.secret.output(data.vrf_input.0);
			let pre_output = VrfPreOutput(pre_output_impl);
			let proof_impl = self.secret.prove(data.vrf_input.0, pre_output.0, &data.aux_data);
			let mut proof = Signature::default();
			proof_impl
				.serialize_compressed(proof.0.as_mut_slice())
				.expect("serialization length is constant and checked by test; qed");
			VrfSignature { pre_output, proof }
		}

		fn vrf_pre_output(&self, input: &Self::VrfInput) -> Self::VrfPreOutput {
			let pre_output_impl = self.secret.output(input.0);
			VrfPreOutput(pre_output_impl)
		}
	}

	impl VrfCrypto for Public {
		type VrfInput = VrfInput;
		type VrfPreOutput = VrfPreOutput;
		type VrfSignData = VrfSignData;
		type VrfSignature = VrfSignature;
	}

	impl VrfPublic for Public {
		fn vrf_verify(&self, data: &Self::VrfSignData, signature: &Self::VrfSignature) -> bool {
			let Ok(public) =
				bandersnatch::Public::deserialize_compressed_unchecked(self.as_slice())
			else {
				return false
			};

			// Deserialize only the proof, the rest has already been deserialized
			// This is another hack used because backend signature type is generic over
			// the number of ios.
			let Ok(proof) = ark_ec_vrfs::ietf::Proof::deserialize_compressed_unchecked(
				signature.proof.as_slice(),
			) else {
				return false
			};

			public
				.verify(data.vrf_input.0, signature.pre_output.0, &data.aux_data, &proof)
				.is_ok()
		}
	}

	#[cfg(feature = "full_crypto")]
	impl Pair {
		/// Generate VRF output bytes for the given `input`.
		pub fn make_bytes(&self, input: &VrfInput) -> [u8; 32] {
			self.vrf_pre_output(input).make_bytes()
		}
	}

	impl VrfPreOutput {
		/// Generate VRF output bytes.
		pub fn make_bytes(&self) -> [u8; 32] {
			let mut bytes = [0_u8; 32];
			bytes.copy_from_slice(&self.0.hash()[..32]);
			bytes
		}
	}
}

// /// Bandersnatch Ring-VRF types and operations.
// pub mod ring_vrf {
// 	use super::{vrf::*, *};
// 	pub use bandersnatch_vrfs::ring::{RingProof, RingProver, RingVerifier, KZG};
// 	use bandersnatch_vrfs::{ring::VerifierKey, CanonicalDeserialize, PublicKey};

// 	/// Overhead in the domain size with respect to the supported ring size.
// 	///
// 	/// Some bits of the domain are reserved for the zk-proof to work.
// 	pub const RING_DOMAIN_OVERHEAD: u32 = 257;

// 	// Max size of serialized ring-vrf context given `domain_len`.
// 	pub(crate) const fn ring_context_serialized_size(domain_len: u32) -> usize {
// 		// const G1_POINT_COMPRESSED_SIZE: usize = 48;
// 		// const G2_POINT_COMPRESSED_SIZE: usize = 96;
// 		const G1_POINT_UNCOMPRESSED_SIZE: usize = 96;
// 		const G2_POINT_UNCOMPRESSED_SIZE: usize = 192;
// 		const OVERHEAD_SIZE: usize = 20;
// 		const G2_POINTS_NUM: usize = 2;
// 		let g1_points_num = 3 * domain_len as usize + 1;

// 		OVERHEAD_SIZE +
// 			g1_points_num * G1_POINT_UNCOMPRESSED_SIZE +
// 			G2_POINTS_NUM * G2_POINT_UNCOMPRESSED_SIZE
// 	}

// 	pub(crate) const RING_VERIFIER_DATA_SERIALIZED_SIZE: usize = 388;
// 	pub(crate) const RING_SIGNATURE_SERIALIZED_SIZE: usize = 755;

// 	/// remove as soon as soon as serialization is implemented by the backend
// 	pub struct RingVerifierData {
// 		/// Domain size.
// 		pub domain_size: u32,
// 		/// Verifier key.
// 		pub verifier_key: VerifierKey,
// 	}

// 	impl From<RingVerifierData> for RingVerifier {
// 		fn from(vd: RingVerifierData) -> RingVerifier {
// 			bandersnatch_vrfs::ring::make_ring_verifier(vd.verifier_key, vd.domain_size as usize)
// 		}
// 	}

// 	impl Encode for RingVerifierData {
// 		fn encode(&self) -> Vec<u8> {
// 			const ERR_STR: &str = "serialization length is constant and checked by test; qed";
// 			let mut buf = [0; RING_VERIFIER_DATA_SERIALIZED_SIZE];
// 			self.domain_size.serialize_compressed(&mut buf[..4]).expect(ERR_STR);
// 			self.verifier_key.serialize_compressed(&mut buf[4..]).expect(ERR_STR);
// 			buf.encode()
// 		}
// 	}

// 	impl Decode for RingVerifierData {
// 		fn decode<R: codec::Input>(i: &mut R) -> Result<Self, codec::Error> {
// 			const ERR_STR: &str = "serialization length is constant and checked by test; qed";
// 			let buf = <[u8; RING_VERIFIER_DATA_SERIALIZED_SIZE]>::decode(i)?;
// 			let domain_size =
// 				<u32 as CanonicalDeserialize>::deserialize_compressed_unchecked(&mut &buf[..4])
// 					.expect(ERR_STR);
// 			let verifier_key = <bandersnatch_vrfs::ring::VerifierKey as
// CanonicalDeserialize>::deserialize_compressed_unchecked(&mut &buf[4..]).expect(ERR_STR);

// 			Ok(RingVerifierData { domain_size, verifier_key })
// 		}
// 	}

// 	impl EncodeLike for RingVerifierData {}

// 	impl MaxEncodedLen for RingVerifierData {
// 		fn max_encoded_len() -> usize {
// 			<[u8; RING_VERIFIER_DATA_SERIALIZED_SIZE]>::max_encoded_len()
// 		}
// 	}

// 	impl TypeInfo for RingVerifierData {
// 		type Identity = [u8; RING_VERIFIER_DATA_SERIALIZED_SIZE];

// 		fn type_info() -> scale_info::Type {
// 			Self::Identity::type_info()
// 		}
// 	}

// 	/// Context used to construct ring prover and verifier.
// 	///
// 	/// Generic parameter `D` represents the ring domain size and drives
// 	/// the max number of supported ring members [`RingContext::max_keyset_size`]
// 	/// which is equal to `D - RING_DOMAIN_OVERHEAD`.
// 	#[derive(Clone)]
// 	pub struct RingContext<const D: u32>(KZG);

// 	impl<const D: u32> RingContext<D> {
// 		/// Build an dummy instance for testing purposes.
// 		pub fn new_testing() -> Self {
// 			Self(KZG::testing_kzg_setup([0; 32], D))
// 		}

// 		/// Get the keyset max size.
// 		pub fn max_keyset_size(&self) -> usize {
// 			self.0.max_keyset_size()
// 		}

// 		/// Get ring prover for the key at index `public_idx` in the `public_keys` set.
// 		pub fn prover(&self, public_keys: &[Public], public_idx: usize) -> Option<RingProver> {
// 			let mut pks = Vec::with_capacity(public_keys.len());
// 			for public_key in public_keys {
// 				let pk = PublicKey::deserialize_compressed_unchecked(public_key.as_slice()).ok()?;
// 				pks.push(pk.0.into());
// 			}

// 			let prover_key = self.0.prover_key(pks);
// 			let ring_prover = self.0.init_ring_prover(prover_key, public_idx);
// 			Some(ring_prover)
// 		}

// 		/// Get ring verifier for the `public_keys` set.
// 		pub fn verifier(&self, public_keys: &[Public]) -> Option<RingVerifier> {
// 			let mut pks = Vec::with_capacity(public_keys.len());
// 			for public_key in public_keys {
// 				let pk = PublicKey::deserialize_compressed_unchecked(public_key.as_slice()).ok()?;
// 				pks.push(pk.0.into());
// 			}

// 			let verifier_key = self.0.verifier_key(pks);
// 			let ring_verifier = self.0.init_ring_verifier(verifier_key);
// 			Some(ring_verifier)
// 		}

// 		/// Information required for a lazy construction of a ring verifier.
// 		pub fn verifier_data(&self, public_keys: &[Public]) -> Option<RingVerifierData> {
// 			let mut pks = Vec::with_capacity(public_keys.len());
// 			for public_key in public_keys {
// 				let pk = PublicKey::deserialize_compressed_unchecked(public_key.as_slice()).ok()?;
// 				pks.push(pk.0.into());
// 			}
// 			Some(RingVerifierData {
// 				verifier_key: self.0.verifier_key(pks),
// 				domain_size: self.0.domain_size,
// 			})
// 		}
// 	}

// 	impl<const D: u32> Encode for RingContext<D> {
// 		fn encode(&self) -> Vec<u8> {
// 			let mut buf = vec![0; ring_context_serialized_size(D)];
// 			self.0
// 				.serialize_uncompressed(buf.as_mut_slice())
// 				.expect("serialization length is constant and checked by test; qed");
// 			buf
// 		}
// 	}

// 	impl<const D: u32> Decode for RingContext<D> {
// 		fn decode<R: codec::Input>(input: &mut R) -> Result<Self, codec::Error> {
// 			let mut buf = vec![0; ring_context_serialized_size(D)];
// 			input.read(&mut buf[..])?;
// 			let kzg = KZG::deserialize_uncompressed_unchecked(buf.as_slice())
// 				.map_err(|_| "KZG decode error")?;
// 			Ok(RingContext(kzg))
// 		}
// 	}

// 	impl<const D: u32> EncodeLike for RingContext<D> {}

// 	impl<const D: u32> MaxEncodedLen for RingContext<D> {
// 		fn max_encoded_len() -> usize {
// 			ring_context_serialized_size(D)
// 		}
// 	}

// 	impl<const D: u32> TypeInfo for RingContext<D> {
// 		type Identity = Self;

// 		fn type_info() -> scale_info::Type {
// 			let path = scale_info::Path::new("RingContext", module_path!());
// 			let array_type_def = scale_info::TypeDefArray {
// 				len: ring_context_serialized_size(D) as u32,
// 				type_param: scale_info::MetaType::new::<u8>(),
// 			};
// 			let type_def = scale_info::TypeDef::Array(array_type_def);
// 			scale_info::Type { path, type_params: Vec::new(), type_def, docs: Vec::new() }
// 		}
// 	}

// 	/// Ring VRF signature.
// 	#[derive(
// 		Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
// 	)]
// 	pub struct RingVrfSignature {
// 		/// Ring signature.
// 		pub signature: [u8; RING_SIGNATURE_SERIALIZED_SIZE],
// 		/// VRF pre-outputs.
// 		pub pre_outputs: VrfIosVec<VrfPreOutput>,
// 	}

// 	#[cfg(feature = "full_crypto")]
// 	impl Pair {
// 		/// Produce a ring-vrf signature.
// 		///
// 		/// The ring signature is verifiable if the public key corresponding to the
// 		/// signing [`Pair`] is part of the ring from which the [`RingProver`] has
// 		/// been constructed. If not, the produced signature is just useless.
// 		pub fn ring_vrf_sign(&self, data: &VrfSignData, prover: &RingProver) -> RingVrfSignature {
// 			const _: () = assert!(MAX_VRF_IOS == 3, "`MAX_VRF_IOS` expected to be 3");
// 			// Workaround to overcome backend signature generic over the number of IOs.
// 			match data.inputs.len() {
// 				0 => self.ring_vrf_sign_gen::<0>(data, prover),
// 				1 => self.ring_vrf_sign_gen::<1>(data, prover),
// 				2 => self.ring_vrf_sign_gen::<2>(data, prover),
// 				3 => self.ring_vrf_sign_gen::<3>(data, prover),
// 				_ => unreachable!(),
// 			}
// 		}

// 		fn ring_vrf_sign_gen<const N: usize>(
// 			&self,
// 			data: &VrfSignData,
// 			prover: &RingProver,
// 		) -> RingVrfSignature {
// 			let ios = core::array::from_fn(|i| self.secret.vrf_inout(data.inputs[i].0));

// 			let ring_signature: bandersnatch_vrfs::RingVrfSignature<N> =
// 				bandersnatch_vrfs::RingProver { ring_prover: prover, secret: &self.secret }
// 					.sign_ring_vrf(data.transcript.clone(), &ios);

// 			let pre_outputs: Vec<_> =
// 				ring_signature.preouts.into_iter().map(VrfPreOutput).collect();
// 			let pre_outputs = VrfIosVec::truncate_from(pre_outputs);

// 			let mut signature =
// 				RingVrfSignature { pre_outputs, signature: [0; RING_SIGNATURE_SERIALIZED_SIZE] };

// 			ring_signature
// 				.proof
// 				.serialize_compressed(signature.signature.as_mut_slice())
// 				.expect("serialization length is constant and checked by test; qed");

// 			signature
// 		}
// 	}

// 	impl RingVrfSignature {
// 		/// Verify a ring-vrf signature.
// 		///
// 		/// The signature is verifiable if it has been produced by a member of the ring
// 		/// from which the [`RingVerifier`] has been constructed.
// 		pub fn ring_vrf_verify(&self, data: &VrfSignData, verifier: &RingVerifier) -> bool {
// 			const _: () = assert!(MAX_VRF_IOS == 3, "`MAX_VRF_IOS` expected to be 3");
// 			let preouts_len = self.pre_outputs.len();
// 			if preouts_len != data.inputs.len() {
// 				return false
// 			}
// 			// Workaround to overcome backend signature generic over the number of IOs.
// 			match preouts_len {
// 				0 => self.ring_vrf_verify_gen::<0>(data, verifier),
// 				1 => self.ring_vrf_verify_gen::<1>(data, verifier),
// 				2 => self.ring_vrf_verify_gen::<2>(data, verifier),
// 				3 => self.ring_vrf_verify_gen::<3>(data, verifier),
// 				_ => unreachable!(),
// 			}
// 		}

// 		fn ring_vrf_verify_gen<const N: usize>(
// 			&self,
// 			data: &VrfSignData,
// 			verifier: &RingVerifier,
// 		) -> bool {
// 			let Ok(vrf_signature) =
// 				bandersnatch_vrfs::RingVrfSignature::<0>::deserialize_compressed_unchecked(
// 					self.signature.as_slice(),
// 				)
// 			else {
// 				return false
// 			};

// 			let preouts: [bandersnatch_vrfs::VrfPreOut; N] =
// 				core::array::from_fn(|i| self.pre_outputs[i].0);

// 			let signature =
// 				bandersnatch_vrfs::RingVrfSignature { proof: vrf_signature.proof, preouts };

// 			let inputs = data.inputs.iter().map(|i| i.0);

// 			bandersnatch_vrfs::RingVerifier(verifier)
// 				.verify_ring_vrf(data.transcript.clone(), inputs, &signature)
// 				.is_ok()
// 		}
// 	}
// }

#[cfg(test)]
mod tests {
	use super::{vrf::*, *};
	use crate::crypto::{VrfPublic, VrfSecret, DEV_PHRASE};

	const TEST_SEED: &[u8; SEED_SERIALIZED_SIZE] = &[0xcb; SEED_SERIALIZED_SIZE];
	const TEST_DOMAIN_SIZE: u32 = 1024;

	#[allow(unused)]
	fn b2h(bytes: &[u8]) -> String {
		array_bytes::bytes2hex("", bytes)
	}

	fn h2b(hex: &str) -> Vec<u8> {
		array_bytes::hex2bytes_unchecked(hex)
	}

	#[test]
	fn sign_verify() {
		let pair = Pair::from_seed(TEST_SEED);
		let public = pair.public();
		let msg = b"hello";

		let signature = pair.sign(msg);
		assert!(Pair::verify(&signature, msg, &public));
	}
}
