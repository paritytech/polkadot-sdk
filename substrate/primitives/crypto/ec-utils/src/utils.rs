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

//! Generic executions of the operations for *Arkworks* elliptic curves.

// As not all functions are used by each elliptic curve and some elliptic
// curve may be excluded by the build we resort to `#[allow(unused)]` to
// suppress the expected warning.

use alloc::vec::Vec;
use ark_ec::{
	pairing::{MillerLoopOutput, Pairing},
	short_weierstrass::{Affine as SWAffine, SWCurveConfig},
	twisted_edwards::{Affine as TEAffine, TECurveConfig},
	CurveGroup,
};
use ark_scale::{
	ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate},
	scale::{Decode, Encode},
};

// SCALE encoding parameters shared by all the enabled modules
const SCALE_USAGE: u8 = ark_scale::make_usage(Compress::No, Validate::No);
type ArkScale<T> = ark_scale::ArkScale<T, SCALE_USAGE>;

/// Convenience alias for a big integer represented as a sequence of `u64` limbs.
pub type BigInteger = Vec<u64>;

/// Define pairing related types
#[macro_export]
macro_rules! pairing_types {
	($curve:ty) => {
		/// An element in G1 (affine).
		pub type G1Affine = <$curve as ark_ec::pairing::Pairing>::G1Affine;
		/// An element in G1 (projective).
		pub type G1Projective = <$curve as ark_ec::pairing::Pairing>::G1;
		/// An element in G2 (affine).
		pub type G2Affine = <$curve as ark_ec::pairing::Pairing>::G2Affine;
		/// An element in G2 (projective).
		pub type G2Projective = <$curve as ark_ec::pairing::Pairing>::G2;
		/// G1 and G2 scalar field (Fr).
		pub type ScalarField = <$curve as ark_ec::pairing::Pairing>::ScalarField;
		/// Pairing target field.
		pub type TargetField = <$curve as ark_ec::pairing::Pairing>::TargetField;
		/// An element in G1 preprocessed for pairing.
		pub type G1Prepared = <$curve as ark_ec::pairing::Pairing>::G1Prepared;
		/// An element in G2 preprocessed for pairing.
		pub type G2Prepared = <$curve as ark_ec::pairing::Pairing>::G2Prepared;
	};
}

#[inline(always)]
#[allow(unused)]
pub fn encode_iter<T: CanonicalSerialize>(iter: impl Iterator<Item = T>) -> Vec<u8> {
	encode(iter.collect::<Vec<_>>())
}

#[inline(always)]
pub fn encode<T: CanonicalSerialize>(val: T) -> Vec<u8> {
	ArkScale::from(val).encode()
}

#[inline(always)]
pub fn decode<T: CanonicalDeserialize>(buf: Vec<u8>) -> Result<T, ()> {
	ArkScale::<T>::decode(&mut &buf[..]).map_err(|_| ()).map(|v| v.0)
}

#[allow(unused)]
pub fn multi_miller_loop<T: Pairing>(g1: Vec<u8>, g2: Vec<u8>) -> Result<Vec<u8>, ()> {
	let g1 = decode::<Vec<<T as Pairing>::G1Affine>>(g1)?;
	let g2 = decode::<Vec<<T as Pairing>::G2Affine>>(g2)?;
	let res = T::multi_miller_loop(g1, g2);
	Ok(encode(res.0))
}

#[allow(unused)]
pub fn final_exponentiation<T: Pairing>(target: Vec<u8>) -> Result<Vec<u8>, ()> {
	let target = decode::<<T as Pairing>::TargetField>(target)?;
	let res = T::final_exponentiation(MillerLoopOutput(target)).ok_or(())?;
	Ok(encode(res.0))
}

/// Short Weierstrass multi scalar multiplication.
///
/// Expects encoded:
/// - `bases`: `Vec<SWAffine<SWCurveConfig>>`.
/// - `scalars`: `Vec<SWCurveConfig::ScalarField>`.
///
/// Returns encoded: `SWAffine<SWCurveConfig>`.
#[allow(unused)]
pub fn msm_sw<T: SWCurveConfig>(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
	let bases = decode::<Vec<SWAffine<T>>>(bases)?;
	let scalars = decode::<Vec<T::ScalarField>>(scalars)?;
	let res = T::msm(&bases, &scalars).map_err(|_| ())?.into_affine();
	Ok(encode::<SWAffine<T>>(res))
}

/// Short Weierstrass affine multiplication.
///
/// Expects encoded:
/// - `base`: `SWAffine<SWCurveConfig>`.
/// - `scalar`: `BigInteger`.
///
/// Returns encoded: `SWAffine<SWCurveConfig>`.
#[allow(unused)]
pub fn mul_affine_sw<T: SWCurveConfig>(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
	let base = decode::<SWAffine<T>>(base)?;
	let scalar = decode::<BigInteger>(scalar)?;
	let res = T::mul_affine(&base, &scalar).into_affine();
	Ok(encode::<SWAffine<T>>(res))
}

/// Twisted Edwards multi scalar multiplication.
///
/// Expects encoded:
/// - `bases`: `Vec<TEAffine<TECurveConfig>>`.
/// - `scalars`: `Vec<TECurveConfig::ScalarField>`.
///
/// Returns encoded: `TEAffine<TECurveConfig>`.
#[allow(unused)]
pub fn msm_te<T: TECurveConfig>(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
	let bases = decode::<Vec<TEAffine<T>>>(bases)?;
	let scalars = decode::<Vec<T::ScalarField>>(scalars)?;
	let res = T::msm(&bases, &scalars).map_err(|_| ())?.into_affine();
	Ok(encode::<TEAffine<T>>(res))
}

/// Twisted Edwards affine multiplication.
///
/// Expects encoded:
/// - `base`: `TEAffine<TECurveConfig>`.
/// - `scalar`: `BigInteger`.
///
/// Returns encoded: `TEAffine<TECurveConfig>`.
#[allow(unused)]
pub fn mul_affine_te<T: TECurveConfig>(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
	let base = decode::<TEAffine<T>>(base)?;
	let scalar = decode::<BigInteger>(scalar)?;
	let res = T::mul_affine(&base, &scalar).into_affine();
	Ok(encode::<TEAffine<T>>(res))
}

#[cfg(test)]
pub mod testing {
	use super::*;
	use ark_ec::{AffineRepr, VariableBaseMSM};
	use ark_ff::PrimeField;
	use ark_std::{test_rng, UniformRand};

	pub fn msm_args<P: AffineRepr>(count: usize) -> (Vec<P>, Vec<P::ScalarField>) {
		let mut rng = test_rng();
		(0..count).map(|_| (P::rand(&mut rng), P::ScalarField::rand(&mut rng))).unzip()
	}

	pub fn mul_args<P: AffineRepr>() -> (P, P::ScalarField) {
		let (p, s) = msm_args::<P>(1);
		(p[0], s[0])
	}

	pub fn mul<SubAffine: AffineRepr, ArkAffine: AffineRepr<ScalarField = SubAffine::ScalarField>>()
	where
		ArkAffine::Config: ark_ec::short_weierstrass::SWCurveConfig,
	{
		let (p, s) = mul_args::<SubAffine>();

		// This goes implicitly through the hostcall
		let r1 = (p * s).into_affine();

		// This directly calls into arkworks
		let p_enc = encode(p);
		let s_enc = encode(s.into_bigint().as_ref());
		let r2_enc = mul_affine_sw::<ArkAffine::Config>(p_enc, s_enc).unwrap();
		let r2 = decode::<SubAffine>(r2_enc).unwrap();

		assert_eq!(r1, r2);
	}

	pub fn msm<SubAffine, ArkAffine>()
	where
		SubAffine: AffineRepr,
		ArkAffine: AffineRepr<ScalarField = SubAffine::ScalarField>,
		ArkAffine::Config: ark_ec::short_weierstrass::SWCurveConfig,
	{
		let (bases, scalars) = msm_args::<SubAffine>(10);

		// This goes implicitly through the hostcall
		let r1 = SubAffine::Group::msm(&bases, &scalars).unwrap().into_affine();

		// This directly calls into arkworks
		let bases_enc = encode(&bases[..]);
		let scalars_enc = encode(&scalars[..]);
		let r2_enc = msm_sw::<ArkAffine::Config>(bases_enc, scalars_enc).unwrap();
		let r2 = decode::<SubAffine>(r2_enc).unwrap();

		assert_eq!(r1, r2);
	}
}
