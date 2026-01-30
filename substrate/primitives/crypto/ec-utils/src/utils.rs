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

#![allow(unused)]

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

/// Unexpected failure message.
pub const FAIL_MSG: &str = "Unexpected failure, bad arguments, broken host/runtime contract; qed";

// SCALE encoding parameters shared by all the enabled modules
const SCALE_USAGE: u8 = ark_scale::make_usage(Compress::No, Validate::No);
type ArkScale<T> = ark_scale::ArkScale<T, SCALE_USAGE>;

/// Convenience alias for a big integer represented as a sequence of `u64` limbs.
pub type BigInteger = Vec<u64>;

#[inline(always)]
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

/// Pairing multi Miller loop.
///
/// Receives encoded:
/// - `g1`: `Vec<G1Affine>`.
/// - `g2`: `Vec<G2Affine>`.
/// Returns encoded `TargetField`.
pub fn multi_miller_loop<T: Pairing>(g1: Vec<u8>, g2: Vec<u8>) -> Result<Vec<u8>, ()> {
	let g1 = decode::<Vec<<T as Pairing>::G1Affine>>(g1)?;
	let g2 = decode::<Vec<<T as Pairing>::G2Affine>>(g2)?;
	let res = T::multi_miller_loop(g1, g2);
	Ok(encode(res.0))
}

/// Pairing final exponentiation.
///
/// Receives encoded `TargetField`.
/// Returns encoded `TargetField`.
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
/// Returns encoded: `SWAffine<SWCurveConfig>`.
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
/// Returns encoded: `SWAffine<SWCurveConfig>`.
pub fn mul_sw<T: SWCurveConfig>(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
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
/// Returns encoded: `TEAffine<TECurveConfig>`.
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
/// Returns encoded: `TEAffine<TECurveConfig>`.
pub fn mul_te<T: TECurveConfig>(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
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

	fn pairing_args<E: Pairing>() -> (E::G1Affine, E::G2Affine) {
		let mut rng = test_rng();
		(E::G1Affine::rand(&mut rng), E::G2Affine::rand(&mut rng))
	}

	pub fn mul_test<SubAffine, ArkAffine>()
	where
		SubAffine: AffineRepr,
		ArkAffine: AffineRepr<ScalarField = SubAffine::ScalarField>,
		ArkAffine::Config: ark_ec::short_weierstrass::SWCurveConfig,
	{
		let (p, s) = mul_args::<SubAffine>();

		// This goes implicitly through the hostcall
		let r1 = (p * s).into_affine();

		// This directly calls into arkworks
		let p_enc = encode(p);
		let s_enc = encode(s.into_bigint().as_ref());
		let r2_enc = mul_sw::<ArkAffine::Config>(p_enc, s_enc).unwrap();
		let r2 = decode::<SubAffine>(r2_enc).unwrap();

		assert_eq!(r1, r2);
	}

	pub fn msm_test<SubAffine, ArkAffine>()
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

	pub fn mul_te_test<SubAffine, ArkAffine>()
	where
		SubAffine: AffineRepr,
		ArkAffine: AffineRepr<ScalarField = SubAffine::ScalarField>,
		ArkAffine::Config: ark_ec::twisted_edwards::TECurveConfig,
	{
		let (p, s) = mul_args::<SubAffine>();

		// This goes implicitly through the hostcall
		let r1 = (p * s).into_affine();

		// This directly calls into arkworks
		let p_enc = encode(p);
		let s_enc = encode(s.into_bigint().as_ref());
		let r2_enc = mul_te::<ArkAffine::Config>(p_enc, s_enc).unwrap();
		let r2 = decode::<SubAffine>(r2_enc).unwrap();

		assert_eq!(r1, r2);
	}

	pub fn msm_te_test<SubAffine, ArkAffine>()
	where
		SubAffine: AffineRepr,
		ArkAffine: AffineRepr<ScalarField = SubAffine::ScalarField>,
		ArkAffine::Config: ark_ec::twisted_edwards::TECurveConfig,
	{
		let (bases, scalars) = msm_args::<SubAffine>(10);

		// This goes implicitly through the hostcall
		let r1 = SubAffine::Group::msm(&bases, &scalars).unwrap().into_affine();

		// This directly calls into arkworks
		let bases_enc = encode(&bases[..]);
		let scalars_enc = encode(&scalars[..]);
		let r2_enc = msm_te::<ArkAffine::Config>(bases_enc, scalars_enc).unwrap();
		let r2 = decode::<SubAffine>(r2_enc).unwrap();

		assert_eq!(r1, r2);
	}

	pub fn pairing_test<SubPairing, ArkPairing>()
	where
		SubPairing: Pairing,
		ArkPairing: Pairing,
	{
		let (g1, g2) = pairing_args::<SubPairing>();

		// This goes implicitly through the `multi_miller_loop` and `final_exponentiation` hostcalls
		let r1 = SubPairing::pairing(g1, g2).0;

		// Pairing via direct arkworks calls
		let g1_enc = encode(vec![g1]);
		let g2_enc = encode(vec![g2]);
		let r2_enc = multi_miller_loop::<ArkPairing>(g1_enc, g2_enc).unwrap();
		let r2_enc = final_exponentiation::<ArkPairing>(r2_enc).unwrap();
		let r2 = decode::<<SubPairing as Pairing>::TargetField>(r2_enc).unwrap();

		assert_eq!(r1, r2);
	}
}
