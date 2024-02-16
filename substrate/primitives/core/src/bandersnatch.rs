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

#[cfg(feature = "serde")]
use crate::crypto::Ss58Codec;
use crate::crypto::{
	ByteArray, CryptoType, CryptoTypeId, Derive, Public as TraitPublic, UncheckedFrom, VrfPublic,
};
#[cfg(feature = "full_crypto")]
use crate::crypto::{DeriveError, DeriveJunction, Pair as TraitPair, SecretStringError, VrfSecret};
#[cfg(feature = "serde")]
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
#[cfg(all(not(feature = "std"), feature = "serde"))]
use sp_std::alloc::{format, string::String};

use bandersnatch_vrfs::CanonicalSerialize;
#[cfg(feature = "full_crypto")]
use bandersnatch_vrfs::SecretKey;
use codec::{Decode, Encode, EncodeLike, MaxEncodedLen};
use scale_info::TypeInfo;

use sp_runtime_interface::pass_by::PassByInner;
use sp_std::{vec, vec::Vec};

/// Identifier used to match public keys against bandersnatch-vrf keys.
pub const CRYPTO_ID: CryptoTypeId = CryptoTypeId(*b"band");

/// Context used to produce a plain signature without any VRF input/output.
#[cfg(feature = "full_crypto")]
pub const SIGNING_CTX: &[u8] = b"BandersnatchSigningContext";

#[cfg(feature = "full_crypto")]
const SEED_SERIALIZED_SIZE: usize = 32;

const PUBLIC_SERIALIZED_SIZE: usize = 33;
const SIGNATURE_SERIALIZED_SIZE: usize = 65;
const PREOUT_SERIALIZED_SIZE: usize = 33;

/// Bandersnatch public key.
#[cfg_attr(feature = "full_crypto", derive(Hash))]
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Encode,
	Decode,
	PassByInner,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct Public(pub [u8; PUBLIC_SERIALIZED_SIZE]);

impl UncheckedFrom<[u8; PUBLIC_SERIALIZED_SIZE]> for Public {
	fn unchecked_from(raw: [u8; PUBLIC_SERIALIZED_SIZE]) -> Self {
		Public(raw)
	}
}

impl AsRef<[u8; PUBLIC_SERIALIZED_SIZE]> for Public {
	fn as_ref(&self) -> &[u8; PUBLIC_SERIALIZED_SIZE] {
		&self.0
	}
}

impl AsRef<[u8]> for Public {
	fn as_ref(&self) -> &[u8] {
		&self.0[..]
	}
}

impl AsMut<[u8]> for Public {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0[..]
	}
}

impl TryFrom<&[u8]> for Public {
	type Error = ();

	fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
		if data.len() != PUBLIC_SERIALIZED_SIZE {
			return Err(())
		}
		let mut r = [0u8; PUBLIC_SERIALIZED_SIZE];
		r.copy_from_slice(data);
		Ok(Self::unchecked_from(r))
	}
}

impl ByteArray for Public {
	const LEN: usize = PUBLIC_SERIALIZED_SIZE;
}

impl TraitPublic for Public {}

impl CryptoType for Public {
	#[cfg(feature = "full_crypto")]
	type Pair = Pair;
}

impl Derive for Public {}

impl sp_std::fmt::Debug for Public {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		let s = self.to_ss58check();
		write!(f, "{} ({}...)", crate::hexdisplay::HexDisplay::from(&self.as_ref()), &s[0..8])
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

#[cfg(feature = "serde")]
impl Serialize for Public {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(&self.to_ss58check())
	}
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Public {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		Public::from_ss58check(&String::deserialize(deserializer)?)
			.map_err(|e| de::Error::custom(format!("{:?}", e)))
	}
}

/// Bandersnatch signature.
///
/// The signature is created via the [`VrfSecret::vrf_sign`] using [`SIGNING_CTX`] as transcript
/// `label`.
#[cfg_attr(feature = "full_crypto", derive(Hash))]
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, PassByInner, MaxEncodedLen, TypeInfo)]
pub struct Signature([u8; SIGNATURE_SERIALIZED_SIZE]);

impl UncheckedFrom<[u8; SIGNATURE_SERIALIZED_SIZE]> for Signature {
	fn unchecked_from(raw: [u8; SIGNATURE_SERIALIZED_SIZE]) -> Self {
		Signature(raw)
	}
}

impl AsRef<[u8]> for Signature {
	fn as_ref(&self) -> &[u8] {
		&self.0[..]
	}
}

impl AsMut<[u8]> for Signature {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0[..]
	}
}

impl TryFrom<&[u8]> for Signature {
	type Error = ();

	fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
		if data.len() != SIGNATURE_SERIALIZED_SIZE {
			return Err(())
		}
		let mut r = [0u8; SIGNATURE_SERIALIZED_SIZE];
		r.copy_from_slice(data);
		Ok(Self::unchecked_from(r))
	}
}

impl ByteArray for Signature {
	const LEN: usize = SIGNATURE_SERIALIZED_SIZE;
}

impl CryptoType for Signature {
	#[cfg(feature = "full_crypto")]
	type Pair = Pair;
}

impl sp_std::fmt::Debug for Signature {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "{}", crate::hexdisplay::HexDisplay::from(&self.0))
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

/// The raw secret seed, which can be used to reconstruct the secret [`Pair`].
#[cfg(feature = "full_crypto")]
type Seed = [u8; SEED_SERIALIZED_SIZE];

/// Bandersnatch secret key.
#[cfg(feature = "full_crypto")]
#[derive(Clone)]
pub struct Pair {
	secret: SecretKey,
	seed: Seed,
}

#[cfg(feature = "full_crypto")]
impl Pair {
	/// Get the key seed.
	pub fn seed(&self) -> Seed {
		self.seed
	}
}

#[cfg(feature = "full_crypto")]
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
		let secret = SecretKey::from_seed(&seed);
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
		let public = self.secret.to_public();
		let mut raw = [0; PUBLIC_SERIALIZED_SIZE];
		public
			.serialize_compressed(raw.as_mut_slice())
			.expect("serialization length is constant and checked by test; qed");
		Public::unchecked_from(raw)
	}

	/// Sign a message.
	///
	/// In practice this produce a Schnorr signature of a transcript composed by
	/// the constant label [`SIGNING_CTX`] and `data` without any additional data.
	///
	/// See [`vrf::VrfSignData`] for additional details.
	fn sign(&self, data: &[u8]) -> Signature {
		let data = vrf::VrfSignData::new_unchecked(SIGNING_CTX, &[data], None);
		self.vrf_sign(&data).signature
	}

	fn verify<M: AsRef<[u8]>>(signature: &Signature, data: M, public: &Public) -> bool {
		let data = vrf::VrfSignData::new_unchecked(SIGNING_CTX, &[data.as_ref()], None);
		let signature =
			vrf::VrfSignature { signature: *signature, pre_outputs: vrf::VrfIosVec::default() };
		public.vrf_verify(&data, &signature)
	}

	/// Return a vector filled with the seed (32 bytes).
	fn to_raw_vec(&self) -> Vec<u8> {
		self.seed().to_vec()
	}
}

#[cfg(feature = "full_crypto")]
impl CryptoType for Pair {
	type Pair = Pair;
}

/// Bandersnatch VRF types and operations.
pub mod vrf {
	use super::*;
	use crate::{bounded::BoundedVec, crypto::VrfCrypto, ConstU32};
	use bandersnatch_vrfs::{
		CanonicalDeserialize, CanonicalSerialize, IntoVrfInput, Message, PublicKey,
		ThinVrfSignature, Transcript,
	};

	/// Max number of inputs/pre-outputs which can be handled by the VRF signing procedures.
	///
	/// The number is quite arbitrary and chosen to fulfill the use cases found so far.
	/// If required it can be extended in the future.
	pub const MAX_VRF_IOS: u32 = 3;

	/// Bounded vector used for VRF inputs and pre-outputs.
	///
	/// Can contain at most [`MAX_VRF_IOS`] elements.
	pub type VrfIosVec<T> = BoundedVec<T, ConstU32<MAX_VRF_IOS>>;

	/// VRF input to construct a [`VrfPreOutput`] instance and embeddable in [`VrfSignData`].
	#[derive(Clone, Debug)]
	pub struct VrfInput(pub(super) bandersnatch_vrfs::VrfInput);

	impl VrfInput {
		/// Construct a new VRF input.
		pub fn new(domain: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> Self {
			let msg = Message { domain: domain.as_ref(), message: data.as_ref() };
			VrfInput(msg.into_vrf_input())
		}
	}

	/// VRF pre-output derived from [`VrfInput`] using a [`VrfSecret`].
	///
	/// This object is used to produce an arbitrary number of verifiable pseudo random
	/// bytes and is often called pre-output to emphasize that this is not the actual
	/// output of the VRF but an object capable of generating the output.
	#[derive(Clone, Debug, PartialEq, Eq)]
	pub struct VrfPreOutput(pub(super) bandersnatch_vrfs::VrfPreOut);

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
			let preout =
				bandersnatch_vrfs::VrfPreOut::deserialize_compressed_unchecked(buf.as_slice())
					.map_err(|_| "vrf-preout decode error: bad preout")?;
			Ok(VrfPreOutput(preout))
		}
	}

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
	/// The object contains a transcript and a sequence of [`VrfInput`]s ready to be signed.
	///
	/// The `transcript` summarizes a set of messages which are defining a particular
	/// protocol by automating the Fiat-Shamir transform for challenge generation.
	/// A good explaination of the topic can be found in Merlin [docs](https://merlin.cool/)
	///
	/// The `inputs` is a sequence of [`VrfInput`]s which, during the signing procedure, are
	/// first transformed to [`VrfPreOutput`]s. Both inputs and pre-outputs are then appended to
	/// the transcript before signing the Fiat-Shamir transform result (the challenge).
	///
	/// In practice, as a user, all these technical details can be easily ignored.
	/// What is important to remember is:
	/// - *Transcript* is an object defining the protocol and used to produce the signature. This
	///   object doesn't influence the `VrfPreOutput`s values.
	/// - *Vrf inputs* is some additional data which is used to produce *vrf pre-outputs*. This data
	///   will contribute to the signature as well.
	#[derive(Clone)]
	pub struct VrfSignData {
		/// Associated protocol transcript.
		pub transcript: Transcript,
		/// VRF inputs to be signed.
		pub inputs: VrfIosVec<VrfInput>,
	}

	impl VrfSignData {
		/// Construct a new data to be signed.
		///
		/// Fails if the `inputs` iterator yields more elements than [`MAX_VRF_IOS`]
		///
		/// Refer to [`VrfSignData`] for details about transcript and inputs.
		pub fn new(
			transcript_label: &'static [u8],
			transcript_data: impl IntoIterator<Item = impl AsRef<[u8]>>,
			inputs: impl IntoIterator<Item = VrfInput>,
		) -> Result<Self, ()> {
			let inputs: Vec<VrfInput> = inputs.into_iter().collect();
			if inputs.len() > MAX_VRF_IOS as usize {
				return Err(())
			}
			Ok(Self::new_unchecked(transcript_label, transcript_data, inputs))
		}

		/// Construct a new data to be signed.
		///
		/// At most the first [`MAX_VRF_IOS`] elements of `inputs` are used.
		///
		/// Refer to [`VrfSignData`] for details about transcript and inputs.
		pub fn new_unchecked(
			transcript_label: &'static [u8],
			transcript_data: impl IntoIterator<Item = impl AsRef<[u8]>>,
			inputs: impl IntoIterator<Item = VrfInput>,
		) -> Self {
			let inputs: Vec<VrfInput> = inputs.into_iter().collect();
			let inputs = VrfIosVec::truncate_from(inputs);
			let mut transcript = Transcript::new_labeled(transcript_label);
			transcript_data.into_iter().for_each(|data| transcript.append(data.as_ref()));
			VrfSignData { transcript, inputs }
		}

		/// Append a message to the transcript.
		pub fn push_transcript_data(&mut self, data: &[u8]) {
			self.transcript.append(data);
		}

		/// Tries to append a [`VrfInput`] to the vrf inputs list.
		///
		/// On failure, returns back the [`VrfInput`] parameter.
		pub fn push_vrf_input(&mut self, input: VrfInput) -> Result<(), VrfInput> {
			self.inputs.try_push(input)
		}

		/// Get the challenge associated to the `transcript` contained within the signing data.
		///
		/// Ignores the vrf inputs and outputs.
		pub fn challenge<const N: usize>(&self) -> [u8; N] {
			let mut output = [0; N];
			let mut transcript = self.transcript.clone();
			let mut reader = transcript.challenge(b"bandersnatch challenge");
			reader.read_bytes(&mut output);
			output
		}
	}

	/// VRF signature.
	///
	/// Includes both the transcript `signature` and the `pre-outputs` generated from the
	/// [`VrfSignData::inputs`].
	///
	/// Refer to [`VrfSignData`] for more details.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub struct VrfSignature {
		/// Transcript signature.
		pub signature: Signature,
		/// VRF pre-outputs.
		pub pre_outputs: VrfIosVec<VrfPreOutput>,
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
			const _: () = assert!(MAX_VRF_IOS == 3, "`MAX_VRF_IOS` expected to be 3");
			// Workaround to overcome backend signature generic over the number of IOs.
			match data.inputs.len() {
				0 => self.vrf_sign_gen::<0>(data),
				1 => self.vrf_sign_gen::<1>(data),
				2 => self.vrf_sign_gen::<2>(data),
				3 => self.vrf_sign_gen::<3>(data),
				_ => unreachable!(),
			}
		}

		fn vrf_pre_output(&self, input: &Self::VrfInput) -> Self::VrfPreOutput {
			let pre_output = self.secret.vrf_preout(&input.0);
			VrfPreOutput(pre_output)
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
			const _: () = assert!(MAX_VRF_IOS == 3, "`MAX_VRF_IOS` expected to be 3");
			let pre_outputs_len = signature.pre_outputs.len();
			if pre_outputs_len != data.inputs.len() {
				return false
			}
			// Workaround to overcome backend signature generic over the number of IOs.
			match pre_outputs_len {
				0 => self.vrf_verify_gen::<0>(data, signature),
				1 => self.vrf_verify_gen::<1>(data, signature),
				2 => self.vrf_verify_gen::<2>(data, signature),
				3 => self.vrf_verify_gen::<3>(data, signature),
				_ => unreachable!(),
			}
		}
	}

	#[cfg(feature = "full_crypto")]
	impl Pair {
		fn vrf_sign_gen<const N: usize>(&self, data: &VrfSignData) -> VrfSignature {
			let ios = core::array::from_fn(|i| self.secret.vrf_inout(data.inputs[i].0));

			let thin_signature: ThinVrfSignature<N> =
				self.secret.sign_thin_vrf(data.transcript.clone(), &ios);

			let pre_outputs: Vec<_> =
				thin_signature.preouts.into_iter().map(VrfPreOutput).collect();
			let pre_outputs = VrfIosVec::truncate_from(pre_outputs);

			let mut signature =
				VrfSignature { signature: Signature([0; SIGNATURE_SERIALIZED_SIZE]), pre_outputs };

			thin_signature
				.proof
				.serialize_compressed(signature.signature.0.as_mut_slice())
				.expect("serialization length is constant and checked by test; qed");

			signature
		}

		/// Generate an arbitrary number of bytes from the given `context` and VRF `input`.
		pub fn make_bytes<const N: usize>(
			&self,
			context: &'static [u8],
			input: &VrfInput,
		) -> [u8; N] {
			let transcript = Transcript::new_labeled(context);
			let inout = self.secret.vrf_inout(input.0);
			inout.vrf_output_bytes(transcript)
		}
	}

	impl Public {
		fn vrf_verify_gen<const N: usize>(
			&self,
			data: &VrfSignData,
			signature: &VrfSignature,
		) -> bool {
			let Ok(public) = PublicKey::deserialize_compressed_unchecked(self.as_slice()) else {
				return false
			};

			let preouts: [bandersnatch_vrfs::VrfPreOut; N] =
				core::array::from_fn(|i| signature.pre_outputs[i].0);

			// Deserialize only the proof, the rest has already been deserialized
			// This is another hack used because backend signature type is generic over
			// the number of ios.
			let Ok(proof) = ThinVrfSignature::<0>::deserialize_compressed_unchecked(
				signature.signature.as_ref(),
			)
			.map(|s| s.proof) else {
				return false
			};
			let signature = ThinVrfSignature { proof, preouts };

			let inputs = data.inputs.iter().map(|i| i.0);

			public.verify_thin_vrf(data.transcript.clone(), inputs, &signature).is_ok()
		}
	}

	impl VrfPreOutput {
		/// Generate an arbitrary number of bytes from the given `context` and VRF `input`.
		pub fn make_bytes<const N: usize>(
			&self,
			context: &'static [u8],
			input: &VrfInput,
		) -> [u8; N] {
			let transcript = Transcript::new_labeled(context);
			let inout = bandersnatch_vrfs::VrfInOut { input: input.0, preoutput: self.0 };
			inout.vrf_output_bytes(transcript)
		}
	}
}

/// Bandersnatch Ring-VRF types and operations.
pub mod ring_vrf {
	use super::{vrf::*, *};
	pub use bandersnatch_vrfs::ring::{RingProof, RingProver, RingVerifier, KZG};
	use bandersnatch_vrfs::{ring::VerifierKey, CanonicalDeserialize, PublicKey};

	/// Overhead in the domain size with respect to the supported ring size.
	///
	/// Some bits of the domain are reserved for the zk-proof to work.
	pub const RING_DOMAIN_OVERHEAD: u32 = 257;

	// Max size of serialized ring-vrf context given `domain_len`.
	pub(crate) const fn ring_context_serialized_size(domain_len: u32) -> usize {
		// const G1_POINT_COMPRESSED_SIZE: usize = 48;
		// const G2_POINT_COMPRESSED_SIZE: usize = 96;
		const G1_POINT_UNCOMPRESSED_SIZE: usize = 96;
		const G2_POINT_UNCOMPRESSED_SIZE: usize = 192;
		const OVERHEAD_SIZE: usize = 20;
		const G2_POINTS_NUM: usize = 2;
		let g1_points_num = 3 * domain_len as usize + 1;

		OVERHEAD_SIZE +
			g1_points_num * G1_POINT_UNCOMPRESSED_SIZE +
			G2_POINTS_NUM * G2_POINT_UNCOMPRESSED_SIZE
	}

	pub(crate) const RING_VERIFIER_DATA_SERIALIZED_SIZE: usize = 388;
	pub(crate) const RING_SIGNATURE_SERIALIZED_SIZE: usize = 755;

	/// remove as soon as soon as serialization is implemented by the backend
	pub struct RingVerifierData {
		/// Domain size.
		pub domain_size: u32,
		/// Verifier key.
		pub verifier_key: VerifierKey,
	}

	impl From<RingVerifierData> for RingVerifier {
		fn from(vd: RingVerifierData) -> RingVerifier {
			bandersnatch_vrfs::ring::make_ring_verifier(vd.verifier_key, vd.domain_size as usize)
		}
	}

	impl Encode for RingVerifierData {
		fn encode(&self) -> Vec<u8> {
			const ERR_STR: &str = "serialization length is constant and checked by test; qed";
			let mut buf = [0; RING_VERIFIER_DATA_SERIALIZED_SIZE];
			self.domain_size.serialize_compressed(&mut buf[..4]).expect(ERR_STR);
			self.verifier_key.serialize_compressed(&mut buf[4..]).expect(ERR_STR);
			buf.encode()
		}
	}

	impl Decode for RingVerifierData {
		fn decode<R: codec::Input>(i: &mut R) -> Result<Self, codec::Error> {
			const ERR_STR: &str = "serialization length is constant and checked by test; qed";
			let buf = <[u8; RING_VERIFIER_DATA_SERIALIZED_SIZE]>::decode(i)?;
			let domain_size =
				<u32 as CanonicalDeserialize>::deserialize_compressed_unchecked(&mut &buf[..4])
					.expect(ERR_STR);
			let verifier_key = <bandersnatch_vrfs::ring::VerifierKey as CanonicalDeserialize>::deserialize_compressed_unchecked(&mut &buf[4..]).expect(ERR_STR);

			Ok(RingVerifierData { domain_size, verifier_key })
		}
	}

	impl EncodeLike for RingVerifierData {}

	impl MaxEncodedLen for RingVerifierData {
		fn max_encoded_len() -> usize {
			<[u8; RING_VERIFIER_DATA_SERIALIZED_SIZE]>::max_encoded_len()
		}
	}

	impl TypeInfo for RingVerifierData {
		type Identity = [u8; RING_VERIFIER_DATA_SERIALIZED_SIZE];

		fn type_info() -> scale_info::Type {
			Self::Identity::type_info()
		}
	}

	/// Context used to construct ring prover and verifier.
	///
	/// Generic parameter `D` represents the ring domain size and drives
	/// the max number of supported ring members [`RingContext::max_keyset_size`]
	/// which is equal to `D - RING_DOMAIN_OVERHEAD`.
	#[derive(Clone)]
	pub struct RingContext<const D: u32>(KZG);

	impl<const D: u32> RingContext<D> {
		/// Build an dummy instance for testing purposes.
		pub fn new_testing() -> Self {
			Self(KZG::testing_kzg_setup([0; 32], D))
		}

		/// Get the keyset max size.
		pub fn max_keyset_size(&self) -> usize {
			self.0.max_keyset_size()
		}

		/// Get ring prover for the key at index `public_idx` in the `public_keys` set.
		pub fn prover(&self, public_keys: &[Public], public_idx: usize) -> Option<RingProver> {
			let mut pks = Vec::with_capacity(public_keys.len());
			for public_key in public_keys {
				let pk = PublicKey::deserialize_compressed_unchecked(public_key.as_slice()).ok()?;
				pks.push(pk.0.into());
			}

			let prover_key = self.0.prover_key(pks);
			let ring_prover = self.0.init_ring_prover(prover_key, public_idx);
			Some(ring_prover)
		}

		/// Get ring verifier for the `public_keys` set.
		pub fn verifier(&self, public_keys: &[Public]) -> Option<RingVerifier> {
			let mut pks = Vec::with_capacity(public_keys.len());
			for public_key in public_keys {
				let pk = PublicKey::deserialize_compressed_unchecked(public_key.as_slice()).ok()?;
				pks.push(pk.0.into());
			}

			let verifier_key = self.0.verifier_key(pks);
			let ring_verifier = self.0.init_ring_verifier(verifier_key);
			Some(ring_verifier)
		}

		/// Information required for a lazy construction of a ring verifier.
		pub fn verifier_data(&self, public_keys: &[Public]) -> Option<RingVerifierData> {
			let mut pks = Vec::with_capacity(public_keys.len());
			for public_key in public_keys {
				let pk = PublicKey::deserialize_compressed_unchecked(public_key.as_slice()).ok()?;
				pks.push(pk.0.into());
			}
			Some(RingVerifierData {
				verifier_key: self.0.verifier_key(pks),
				domain_size: self.0.domain_size,
			})
		}
	}

	impl<const D: u32> Encode for RingContext<D> {
		fn encode(&self) -> Vec<u8> {
			let mut buf = vec![0; ring_context_serialized_size(D)];
			self.0
				.serialize_uncompressed(buf.as_mut_slice())
				.expect("serialization length is constant and checked by test; qed");
			buf
		}
	}

	impl<const D: u32> Decode for RingContext<D> {
		fn decode<R: codec::Input>(input: &mut R) -> Result<Self, codec::Error> {
			let mut buf = vec![0; ring_context_serialized_size(D)];
			input.read(&mut buf[..])?;
			let kzg = KZG::deserialize_uncompressed_unchecked(buf.as_slice())
				.map_err(|_| "KZG decode error")?;
			Ok(RingContext(kzg))
		}
	}

	impl<const D: u32> EncodeLike for RingContext<D> {}

	impl<const D: u32> MaxEncodedLen for RingContext<D> {
		fn max_encoded_len() -> usize {
			ring_context_serialized_size(D)
		}
	}

	impl<const D: u32> TypeInfo for RingContext<D> {
		type Identity = Self;

		fn type_info() -> scale_info::Type {
			let path = scale_info::Path::new("RingContext", module_path!());
			let array_type_def = scale_info::TypeDefArray {
				len: ring_context_serialized_size(D) as u32,
				type_param: scale_info::MetaType::new::<u8>(),
			};
			let type_def = scale_info::TypeDef::Array(array_type_def);
			scale_info::Type { path, type_params: Vec::new(), type_def, docs: Vec::new() }
		}
	}

	/// Ring VRF signature.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub struct RingVrfSignature {
		/// Ring signature.
		pub signature: [u8; RING_SIGNATURE_SERIALIZED_SIZE],
		/// VRF pre-outputs.
		pub pre_outputs: VrfIosVec<VrfPreOutput>,
	}

	#[cfg(feature = "full_crypto")]
	impl Pair {
		/// Produce a ring-vrf signature.
		///
		/// The ring signature is verifiable if the public key corresponding to the
		/// signing [`Pair`] is part of the ring from which the [`RingProver`] has
		/// been constructed. If not, the produced signature is just useless.
		pub fn ring_vrf_sign(&self, data: &VrfSignData, prover: &RingProver) -> RingVrfSignature {
			const _: () = assert!(MAX_VRF_IOS == 3, "`MAX_VRF_IOS` expected to be 3");
			// Workaround to overcome backend signature generic over the number of IOs.
			match data.inputs.len() {
				0 => self.ring_vrf_sign_gen::<0>(data, prover),
				1 => self.ring_vrf_sign_gen::<1>(data, prover),
				2 => self.ring_vrf_sign_gen::<2>(data, prover),
				3 => self.ring_vrf_sign_gen::<3>(data, prover),
				_ => unreachable!(),
			}
		}

		fn ring_vrf_sign_gen<const N: usize>(
			&self,
			data: &VrfSignData,
			prover: &RingProver,
		) -> RingVrfSignature {
			let ios = core::array::from_fn(|i| self.secret.vrf_inout(data.inputs[i].0));

			let ring_signature: bandersnatch_vrfs::RingVrfSignature<N> =
				bandersnatch_vrfs::RingProver { ring_prover: prover, secret: &self.secret }
					.sign_ring_vrf(data.transcript.clone(), &ios);

			let pre_outputs: Vec<_> =
				ring_signature.preouts.into_iter().map(VrfPreOutput).collect();
			let pre_outputs = VrfIosVec::truncate_from(pre_outputs);

			let mut signature =
				RingVrfSignature { pre_outputs, signature: [0; RING_SIGNATURE_SERIALIZED_SIZE] };

			ring_signature
				.proof
				.serialize_compressed(signature.signature.as_mut_slice())
				.expect("serialization length is constant and checked by test; qed");

			signature
		}
	}

	impl RingVrfSignature {
		/// Verify a ring-vrf signature.
		///
		/// The signature is verifiable if it has been produced by a member of the ring
		/// from which the [`RingVerifier`] has been constructed.
		pub fn ring_vrf_verify(&self, data: &VrfSignData, verifier: &RingVerifier) -> bool {
			const _: () = assert!(MAX_VRF_IOS == 3, "`MAX_VRF_IOS` expected to be 3");
			let preouts_len = self.pre_outputs.len();
			if preouts_len != data.inputs.len() {
				return false
			}
			// Workaround to overcome backend signature generic over the number of IOs.
			match preouts_len {
				0 => self.ring_vrf_verify_gen::<0>(data, verifier),
				1 => self.ring_vrf_verify_gen::<1>(data, verifier),
				2 => self.ring_vrf_verify_gen::<2>(data, verifier),
				3 => self.ring_vrf_verify_gen::<3>(data, verifier),
				_ => unreachable!(),
			}
		}

		fn ring_vrf_verify_gen<const N: usize>(
			&self,
			data: &VrfSignData,
			verifier: &RingVerifier,
		) -> bool {
			let Ok(vrf_signature) =
				bandersnatch_vrfs::RingVrfSignature::<0>::deserialize_compressed_unchecked(
					self.signature.as_slice(),
				)
			else {
				return false
			};

			let preouts: [bandersnatch_vrfs::VrfPreOut; N] =
				core::array::from_fn(|i| self.pre_outputs[i].0);

			let signature =
				bandersnatch_vrfs::RingVrfSignature { proof: vrf_signature.proof, preouts };

			let inputs = data.inputs.iter().map(|i| i.0);

			bandersnatch_vrfs::RingVerifier(verifier)
				.verify_ring_vrf(data.transcript.clone(), inputs, &signature)
				.is_ok()
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{ring_vrf::*, vrf::*, *};
	use crate::crypto::{VrfPublic, VrfSecret, DEV_PHRASE};

	const DEV_SEED: &[u8; SEED_SERIALIZED_SIZE] = &[0xcb; SEED_SERIALIZED_SIZE];
	const TEST_DOMAIN_SIZE: u32 = 1024;

	type TestRingContext = RingContext<TEST_DOMAIN_SIZE>;

	#[allow(unused)]
	fn b2h(bytes: &[u8]) -> String {
		array_bytes::bytes2hex("", bytes)
	}

	fn h2b(hex: &str) -> Vec<u8> {
		array_bytes::hex2bytes_unchecked(hex)
	}

	#[test]
	fn backend_assumptions_sanity_check() {
		let kzg = KZG::testing_kzg_setup([0; 32], TEST_DOMAIN_SIZE);
		assert_eq!(kzg.max_keyset_size() as u32, TEST_DOMAIN_SIZE - RING_DOMAIN_OVERHEAD);

		assert_eq!(kzg.uncompressed_size(), ring_context_serialized_size(TEST_DOMAIN_SIZE));

		let pks: Vec<_> = (0..16)
			.map(|i| SecretKey::from_seed(&[i as u8; 32]).to_public().0.into())
			.collect();

		let secret = SecretKey::from_seed(&[0u8; 32]);

		let public = secret.to_public();
		assert_eq!(public.compressed_size(), PUBLIC_SERIALIZED_SIZE);

		let input = VrfInput::new(b"foo", &[]);
		let preout = secret.vrf_preout(&input.0);
		assert_eq!(preout.compressed_size(), PREOUT_SERIALIZED_SIZE);

		let verifier_key = kzg.verifier_key(pks.clone());
		assert_eq!(verifier_key.compressed_size() + 4, RING_VERIFIER_DATA_SERIALIZED_SIZE);

		let prover_key = kzg.prover_key(pks);
		let ring_prover = kzg.init_ring_prover(prover_key, 0);

		let data = VrfSignData::new_unchecked(b"mydata", &[b"tdata"], None);

		let thin_signature: bandersnatch_vrfs::ThinVrfSignature<0> =
			secret.sign_thin_vrf(data.transcript.clone(), &[]);
		assert_eq!(thin_signature.compressed_size(), SIGNATURE_SERIALIZED_SIZE);

		let ring_signature: bandersnatch_vrfs::RingVrfSignature<0> =
			bandersnatch_vrfs::RingProver { ring_prover: &ring_prover, secret: &secret }
				.sign_ring_vrf(data.transcript.clone(), &[]);
		assert_eq!(ring_signature.compressed_size(), RING_SIGNATURE_SERIALIZED_SIZE);
	}

	#[test]
	fn max_vrf_ios_bound_respected() {
		let inputs: Vec<_> = (0..MAX_VRF_IOS - 1).map(|_| VrfInput::new(b"", &[])).collect();
		let mut sign_data = VrfSignData::new(b"", &[b""], inputs).unwrap();
		let res = sign_data.push_vrf_input(VrfInput::new(b"", b""));
		assert!(res.is_ok());
		let res = sign_data.push_vrf_input(VrfInput::new(b"", b""));
		assert!(res.is_err());
		let inputs: Vec<_> = (0..MAX_VRF_IOS + 1).map(|_| VrfInput::new(b"", b"")).collect();
		let res = VrfSignData::new(b"mydata", &[b"tdata"], inputs);
		assert!(res.is_err());
	}

	#[test]
	fn derive_works() {
		let pair = Pair::from_string(&format!("{}//Alice//Hard", DEV_PHRASE), None).unwrap();
		let known = h2b("2b340c18b94dc1916979cb83daf3ed4ac106742ddc06afc42cf26be3b18a523f80");
		assert_eq!(pair.public().as_ref(), known);

		// Soft derivation not supported
		let res = Pair::from_string(&format!("{}//Alice/Soft", DEV_PHRASE), None);
		assert!(res.is_err());
	}

	#[test]
	fn generate_with_phrase_should_be_recoverable_with_from_string() {
		let (pair, phrase, seed) = Pair::generate_with_phrase(None);
		let repair_seed = Pair::from_seed_slice(seed.as_ref()).expect("seed slice is valid");
		assert_eq!(pair.public(), repair_seed.public());
		let (repair_phrase, reseed) =
			Pair::from_phrase(phrase.as_ref(), None).expect("seed slice is valid");
		assert_eq!(seed, reseed);
		assert_eq!(pair.public(), repair_phrase.public());
		let repair_string = Pair::from_string(phrase.as_str(), None).expect("seed slice is valid");
		assert_eq!(pair.public(), repair_string.public());
	}

	#[test]
	fn sign_verify() {
		let pair = Pair::from_seed(DEV_SEED);
		let public = pair.public();
		let msg = b"hello";

		let signature = pair.sign(msg);
		assert!(Pair::verify(&signature, msg, &public));
	}

	#[test]
	fn vrf_sign_verify() {
		let pair = Pair::from_seed(DEV_SEED);
		let public = pair.public();

		let i1 = VrfInput::new(b"dom1", b"foo");
		let i2 = VrfInput::new(b"dom2", b"bar");
		let i3 = VrfInput::new(b"dom3", b"baz");

		let data = VrfSignData::new_unchecked(b"mydata", &[b"tdata"], [i1, i2, i3]);

		let signature = pair.vrf_sign(&data);

		assert!(public.vrf_verify(&data, &signature));
	}

	#[test]
	fn vrf_sign_verify_bad_inputs() {
		let pair = Pair::from_seed(DEV_SEED);
		let public = pair.public();

		let i1 = VrfInput::new(b"dom1", b"foo");
		let i2 = VrfInput::new(b"dom2", b"bar");

		let data = VrfSignData::new_unchecked(b"mydata", &[b"aaaa"], [i1.clone(), i2.clone()]);
		let signature = pair.vrf_sign(&data);

		let data = VrfSignData::new_unchecked(b"mydata", &[b"bbb"], [i1, i2.clone()]);
		assert!(!public.vrf_verify(&data, &signature));

		let data = VrfSignData::new_unchecked(b"mydata", &[b"aaa"], [i2]);
		assert!(!public.vrf_verify(&data, &signature));
	}

	#[test]
	fn vrf_make_bytes_matches() {
		let pair = Pair::from_seed(DEV_SEED);

		let i1 = VrfInput::new(b"dom1", b"foo");
		let i2 = VrfInput::new(b"dom2", b"bar");

		let data = VrfSignData::new_unchecked(b"mydata", &[b"tdata"], [i1.clone(), i2.clone()]);
		let signature = pair.vrf_sign(&data);

		let o10 = pair.make_bytes::<32>(b"ctx1", &i1);
		let o11 = signature.pre_outputs[0].make_bytes::<32>(b"ctx1", &i1);
		assert_eq!(o10, o11);

		let o20 = pair.make_bytes::<48>(b"ctx2", &i2);
		let o21 = signature.pre_outputs[1].make_bytes::<48>(b"ctx2", &i2);
		assert_eq!(o20, o21);
	}

	#[test]
	fn encode_decode_vrf_signature() {
		// Transcript data is hashed together and signed.
		// It doesn't contribute to serialized length.
		let pair = Pair::from_seed(DEV_SEED);

		let i1 = VrfInput::new(b"dom1", b"foo");
		let i2 = VrfInput::new(b"dom2", b"bar");

		let data = VrfSignData::new_unchecked(b"mydata", &[b"tdata"], [i1.clone(), i2.clone()]);
		let expected = pair.vrf_sign(&data);

		let bytes = expected.encode();

		let expected_len =
			data.inputs.len() * PREOUT_SERIALIZED_SIZE + SIGNATURE_SERIALIZED_SIZE + 1;
		assert_eq!(bytes.len(), expected_len);

		let decoded = VrfSignature::decode(&mut bytes.as_slice()).unwrap();
		assert_eq!(expected, decoded);

		let data = VrfSignData::new_unchecked(b"mydata", &[b"tdata"], []);
		let expected = pair.vrf_sign(&data);

		let bytes = expected.encode();

		let decoded = VrfSignature::decode(&mut bytes.as_slice()).unwrap();
		assert_eq!(expected, decoded);
	}

	#[test]
	fn ring_vrf_sign_verify() {
		let ring_ctx = TestRingContext::new_testing();

		let mut pks: Vec<_> = (0..16).map(|i| Pair::from_seed(&[i as u8; 32]).public()).collect();
		assert!(pks.len() <= ring_ctx.max_keyset_size());

		let pair = Pair::from_seed(DEV_SEED);

		// Just pick one index to patch with the actual public key
		let prover_idx = 3;
		pks[prover_idx] = pair.public();

		let i1 = VrfInput::new(b"dom1", b"foo");
		let i2 = VrfInput::new(b"dom2", b"bar");
		let i3 = VrfInput::new(b"dom3", b"baz");

		let data = VrfSignData::new_unchecked(b"mydata", &[b"tdata"], [i1, i2, i3]);

		let prover = ring_ctx.prover(&pks, prover_idx).unwrap();
		let signature = pair.ring_vrf_sign(&data, &prover);

		let verifier = ring_ctx.verifier(&pks).unwrap();
		assert!(signature.ring_vrf_verify(&data, &verifier));
	}

	#[test]
	fn ring_vrf_sign_verify_with_out_of_ring_key() {
		let ring_ctx = TestRingContext::new_testing();

		let pks: Vec<_> = (0..16).map(|i| Pair::from_seed(&[i as u8; 32]).public()).collect();
		let pair = Pair::from_seed(DEV_SEED);

		// Just pick one index to patch with the actual public key
		let i1 = VrfInput::new(b"dom1", b"foo");
		let data = VrfSignData::new_unchecked(b"mydata", Some(b"tdata"), Some(i1));

		// pair.public != pks[0]
		let prover = ring_ctx.prover(&pks, 0).unwrap();
		let signature = pair.ring_vrf_sign(&data, &prover);

		let verifier = ring_ctx.verifier(&pks).unwrap();
		assert!(!signature.ring_vrf_verify(&data, &verifier));
	}

	#[test]
	fn ring_vrf_make_bytes_matches() {
		let ring_ctx = TestRingContext::new_testing();

		let mut pks: Vec<_> = (0..16).map(|i| Pair::from_seed(&[i as u8; 32]).public()).collect();
		assert!(pks.len() <= ring_ctx.max_keyset_size());

		let pair = Pair::from_seed(DEV_SEED);

		// Just pick one index to patch with the actual public key
		let prover_idx = 3;
		pks[prover_idx] = pair.public();

		let i1 = VrfInput::new(b"dom1", b"foo");
		let i2 = VrfInput::new(b"dom2", b"bar");
		let data = VrfSignData::new_unchecked(b"mydata", &[b"tdata"], [i1.clone(), i2.clone()]);

		let prover = ring_ctx.prover(&pks, prover_idx).unwrap();
		let signature = pair.ring_vrf_sign(&data, &prover);

		let o10 = pair.make_bytes::<32>(b"ctx1", &i1);
		let o11 = signature.pre_outputs[0].make_bytes::<32>(b"ctx1", &i1);
		assert_eq!(o10, o11);

		let o20 = pair.make_bytes::<48>(b"ctx2", &i2);
		let o21 = signature.pre_outputs[1].make_bytes::<48>(b"ctx2", &i2);
		assert_eq!(o20, o21);
	}

	#[test]
	fn encode_decode_ring_vrf_signature() {
		let ring_ctx = TestRingContext::new_testing();

		let mut pks: Vec<_> = (0..16).map(|i| Pair::from_seed(&[i as u8; 32]).public()).collect();
		assert!(pks.len() <= ring_ctx.max_keyset_size());

		let pair = Pair::from_seed(DEV_SEED);

		// Just pick one...
		let prover_idx = 3;
		pks[prover_idx] = pair.public();

		let i1 = VrfInput::new(b"dom1", b"foo");
		let i2 = VrfInput::new(b"dom2", b"bar");
		let i3 = VrfInput::new(b"dom3", b"baz");

		let data = VrfSignData::new_unchecked(b"mydata", &[b"tdata"], [i1, i2, i3]);

		let prover = ring_ctx.prover(&pks, prover_idx).unwrap();
		let expected = pair.ring_vrf_sign(&data, &prover);

		let bytes = expected.encode();

		let expected_len =
			data.inputs.len() * PREOUT_SERIALIZED_SIZE + RING_SIGNATURE_SERIALIZED_SIZE + 1;
		assert_eq!(bytes.len(), expected_len);

		let decoded = RingVrfSignature::decode(&mut bytes.as_slice()).unwrap();
		assert_eq!(expected, decoded);
	}

	#[test]
	fn encode_decode_ring_vrf_context() {
		let ctx1 = TestRingContext::new_testing();
		let enc1 = ctx1.encode();

		let _ti = <TestRingContext as TypeInfo>::type_info();

		assert_eq!(enc1.len(), ring_context_serialized_size(TEST_DOMAIN_SIZE));
		assert_eq!(enc1.len(), TestRingContext::max_encoded_len());

		let ctx2 = TestRingContext::decode(&mut enc1.as_slice()).unwrap();
		let enc2 = ctx2.encode();

		assert_eq!(enc1, enc2);
	}

	#[test]
	fn encode_decode_verifier_data() {
		let ring_ctx = TestRingContext::new_testing();

		let pks: Vec<_> = (0..16).map(|i| Pair::from_seed(&[i as u8; 32]).public()).collect();
		assert!(pks.len() <= ring_ctx.max_keyset_size());

		let verifier_data = ring_ctx.verifier_data(&pks).unwrap();
		let enc1 = verifier_data.encode();

		assert_eq!(enc1.len(), RING_VERIFIER_DATA_SERIALIZED_SIZE);
		assert_eq!(RingVerifierData::max_encoded_len(), RING_VERIFIER_DATA_SERIALIZED_SIZE);

		let vd2 = RingVerifierData::decode(&mut enc1.as_slice()).unwrap();
		let enc2 = vd2.encode();

		assert_eq!(enc1, enc2);
	}
}
