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

//! *BLS12-381* types and host functions.

use crate::utils;
use alloc::vec::Vec;
use ark_bls12_381_ext::CurveHooks;
use ark_ec::{pairing::Pairing, CurveConfig, CurveGroup};
use sp_runtime_interface::{
	pass_by::{AllocateAndReturnByCodec, PassFatPointerAndRead},
	runtime_interface,
};

/// First pairing group definitions.
pub mod g1 {
	pub use ark_bls12_381_ext::g1::{BETA, G1_GENERATOR_X, G1_GENERATOR_Y};
	/// Group configuration.
	pub type G1Config = ark_bls12_381_ext::g1::Config<super::HostHooks>;
	/// Short Weierstrass form point affine representation.
	pub type G1Affine = ark_bls12_381_ext::g1::G1Affine<super::HostHooks>;
	/// Short Weierstrass form point projective representation.
	pub type G1Projective = ark_bls12_381_ext::g1::G1Projective<super::HostHooks>;
}

/// Second pairing group definitions.
pub mod g2 {
	pub use ark_bls12_381_ext::g2::{
		G2_GENERATOR_X, G2_GENERATOR_X_C0, G2_GENERATOR_X_C1, G2_GENERATOR_Y, G2_GENERATOR_Y_C0,
		G2_GENERATOR_Y_C1,
	};
	/// Group configuration.
	pub type G2Config = ark_bls12_381_ext::g2::Config<super::HostHooks>;
	/// Short Weierstrass form point affine representation.
	pub type G2Affine = ark_bls12_381_ext::g2::G2Affine<super::HostHooks>;
	/// Short Weierstrass form point projective representation.
	pub type G2Projective = ark_bls12_381_ext::g2::G2Projective<super::HostHooks>;
}

pub use self::{
	g1::{G1Affine, G1Config, G1Projective},
	g2::{G2Affine, G2Config, G2Projective},
};

/// Configuration for *BLS12-381* curve.
pub type Config = ark_bls12_381_ext::Config<HostHooks>;

/// *BLS12-381* definition.
///
/// A generic *BLS12* model specialized with *BLS12-381* configuration.
pub type Bls12_381 = ark_bls12_381_ext::Bls12_381<HostHooks>;

/// G1 and G2 scalar field (Fr).
pub type ScalarField = <G1Config as CurveConfig>::ScalarField;

/// Bls12-381 pairing target field.
pub type TargetField = <Bls12_381 as Pairing>::TargetField;

/// Curve hooks jumping into [`host_calls`] host functions.
#[derive(Copy, Clone)]
pub struct HostHooks;

impl CurveHooks for HostHooks {
	fn multi_miller_loop(
		g1: impl Iterator<Item = <Bls12_381 as Pairing>::G1Prepared>,
		g2: impl Iterator<Item = <Bls12_381 as Pairing>::G2Prepared>,
	) -> <Bls12_381 as Pairing>::TargetField {
		host_calls::bls12_381_multi_miller_loop(utils::encode_iter(g1), utils::encode_iter(g2))
			.and_then(|res| utils::decode(res))
			.unwrap_or_default()
	}

	fn final_exponentiation(target: TargetField) -> TargetField {
		host_calls::bls12_381_final_exponentiation(utils::encode(target))
			.and_then(|res| utils::decode(res))
			.unwrap_or_default()
	}

	fn msm_g1(bases: &[G1Affine], scalars: &[ScalarField]) -> G1Projective {
		host_calls::bls12_381_msm_g1(utils::encode(bases), utils::encode(scalars))
			.and_then(|res| utils::decode::<G1Affine>(res))
			.unwrap_or_default()
			.into()
	}

	fn msm_g2(bases: &[G2Affine], scalars: &[ScalarField]) -> G2Projective {
		host_calls::bls12_381_msm_g2(utils::encode(bases), utils::encode(scalars))
			.and_then(|res| utils::decode::<G2Affine>(res))
			.unwrap_or_default()
			.into()
	}

	fn mul_projective_g1(base: &G1Projective, scalar: &[u64]) -> G1Projective {
		let base = base.into_affine();
		host_calls::bls12_381_mul_affine_g1(utils::encode(base), utils::encode(scalar))
			.and_then(|res| utils::decode::<G1Affine>(res))
			.unwrap_or_default()
			.into()
	}

	fn mul_projective_g2(base: &G2Projective, scalar: &[u64]) -> G2Projective {
		let base = base.into_affine();
		host_calls::bls12_381_mul_affine_g2(utils::encode(base), utils::encode(scalar))
			.and_then(|res| utils::decode::<G2Affine>(res))
			.unwrap_or_default()
			.into()
	}
}

/// Interfaces for working with *Arkworks* *BLS12-381* elliptic curve related types
/// from within the runtime.
///
/// All types are (de-)serialized through the wrapper types from `ark-scale`.
///
/// `ArkScale`'s `Usage` generic parameter is expected to be set to "not-validated"
/// and "not-compressed".
#[runtime_interface]
pub trait HostCalls {
	/// Pairing multi Miller loop for *BLS12-381*.
	///
	/// - Receives encoded:
	///   - `a`: `Vec<G1Affine>`.
	///   - `b`: `Vec<G2Affine>`.
	/// - Returns encoded: `TargetField`.
	fn bls12_381_multi_miller_loop(
		a: PassFatPointerAndRead<Vec<u8>>,
		b: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::multi_miller_loop::<ark_bls12_381::Bls12_381>(a, b)
	}

	/// Pairing final exponentiation for *BLS12-381*.
	///
	/// - Receives encoded: `TargetField`.
	/// - Returns encoded: `TargetField`
	fn bls12_381_final_exponentiation(
		f: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::final_exponentiation::<ark_bls12_381::Bls12_381>(f)
	}

	/// Multi scalar multiplication on *G1* for *BLS12-381*
	///
	/// - Receives encoded:
	///   - `bases`: `Vec<G1Affine>`.
	///   - `scalars`: `Vec<ScalarField>`.
	/// - Returns encoded: `G1Affine`.
	fn bls12_381_msm_g1(
		bases: PassFatPointerAndRead<Vec<u8>>,
		scalars: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::msm_sw::<ark_bls12_381::g1::Config>(bases, scalars)
	}

	/// Multi scalar multiplication on *G2* for *BLS12-381*
	///
	/// - Receives encoded:
	///   - `bases`: `Vec<G2Affine>`.
	///   - `scalars`: `Vec<ScalarField>`.
	/// - Returns encoded: `G2Affine`.
	fn bls12_381_msm_g2(
		bases: PassFatPointerAndRead<Vec<u8>>,
		scalars: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::msm_sw::<ark_bls12_381::g2::Config>(bases, scalars)
	}

	/// Affine multiplication on *G1* for *BLS12-381*.
	///
	/// - Receives encoded:
	///   - `base`: `G1Affine`.
	///   - `scalar`: `BigInteger`.
	/// - Returns encoded: `G1Affine`.
	fn bls12_381_mul_affine_g1(
		base: PassFatPointerAndRead<Vec<u8>>,
		scalar: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::mul_affine_sw::<ark_bls12_381::g1::Config>(base, scalar)
	}

	/// Affine multiplication on *G2* for *BLS12-381*
	///
	/// - Receives encoded:
	///   - `base`: `G2Affine`.
	///   - `scalar`: `BigInteger`.
	/// - Returns encoded: `G2Affine`.
	fn bls12_381_mul_affine_g2(
		base: PassFatPointerAndRead<Vec<u8>>,
		scalar: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::mul_affine_sw::<ark_bls12_381::g2::Config>(base, scalar)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use ark_ec::{AffineRepr, VariableBaseMSM};
	use ark_ff::PrimeField;
	use ark_std::{test_rng, UniformRand};

	fn msm_args<P: AffineRepr>(count: usize) -> (Vec<P>, Vec<P::ScalarField>) {
		let mut rng = test_rng();
		(0..count).map(|_| (P::rand(&mut rng), P::ScalarField::rand(&mut rng))).unzip()
	}

	fn mul_args<P: AffineRepr>() -> (P, P::ScalarField) {
		let (p, s) = msm_args::<P>(1);
		(p[0], s[0])
	}

	fn mul<
		SubAffine: AffineRepr<ScalarField = ScalarField>,
		ArkAffine: AffineRepr<ScalarField = ScalarField>,
	>()
	where
		ArkAffine::Config: ark_ec::short_weierstrass::SWCurveConfig,
	{
		let (p, s) = mul_args::<SubAffine>();

		// This goes implicitly through the hostcall
		let r1 = (p * s).into_affine();

		// This directly calls into arkworks
		let p_enc = utils::encode(p);
		let s_enc = utils::encode(s.into_bigint().as_ref());
		let r2_enc = utils::mul_affine_sw::<ArkAffine::Config>(p_enc, s_enc).unwrap();
		let r2 = utils::decode::<SubAffine>(r2_enc).unwrap();

		assert_eq!(r1, r2);
	}

	#[test]
	fn mul_works_g1() {
		mul::<G1Affine, ark_bls12_381::G1Affine>();
	}

	#[test]
	fn mul_works_g2() {
		mul::<G2Affine, ark_bls12_381::G2Affine>();
	}

	fn msm<
		SubAffine: AffineRepr<ScalarField = ScalarField>,
		ArkAffine: AffineRepr<ScalarField = ScalarField>,
	>()
	where
		ArkAffine::Config: ark_ec::short_weierstrass::SWCurveConfig,
	{
		let (bases, scalars) = msm_args::<SubAffine>(10);

		// This goes implicitly through the hostcall
		let r1 = SubAffine::Group::msm(&bases, &scalars).unwrap().into_affine();

		// This directly calls into arkworks
		let bases_enc = utils::encode(&bases[..]);
		let scalars_enc = utils::encode(&scalars[..]);
		let r2_enc = utils::msm_sw::<ArkAffine::Config>(bases_enc, scalars_enc).unwrap();
		let r2 = utils::decode::<SubAffine>(r2_enc).unwrap();

		assert_eq!(r1, r2);
	}

	#[test]
	fn msm_works_g1() {
		msm::<G1Affine, ark_bls12_381::G1Affine>();
	}

	#[test]
	fn msm_works_g2() {
		msm::<G2Affine, ark_bls12_381::G2Affine>();
	}
}
