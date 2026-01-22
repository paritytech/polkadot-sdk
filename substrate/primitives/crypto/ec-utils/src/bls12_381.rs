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
use ark_ec::{pairing::Pairing, AffineRepr, CurveGroup};
use sp_runtime_interface::{
	pass_by::{AllocateAndReturnByCodec, PassFatPointerAndRead},
	runtime_interface,
};

/// Configuration for *BLS12-381* curve.
pub type Config = ark_bls12_381_ext::Config<HostHooks>;

/// *BLS12-381* pairing friendly curve.
pub type Bls12_381 = ark_bls12_381_ext::Bls12_381<HostHooks>;

/// G1 group configuration.
pub type G1Config = ark_bls12_381_ext::g1::Config<HostHooks>;
/// An element in G1 (affine).
pub type G1Affine = ark_bls12_381_ext::g1::G1Affine<HostHooks>;
/// An element in G1 (projective).
pub type G1Projective = ark_bls12_381_ext::g1::G1Projective<HostHooks>;

/// G2 group configuration.
pub type G2Config = ark_bls12_381_ext::g2::Config<HostHooks>;
/// An element in G2 (affine).
pub type G2Affine = ark_bls12_381_ext::g2::G2Affine<HostHooks>;
/// An element in G2 (projective).
pub type G2Projective = ark_bls12_381_ext::g2::G2Projective<HostHooks>;

/// G1 and G2 scalar field (Fr).
pub type ScalarField = <Bls12_381 as Pairing>::ScalarField;

/// An element in G1 preprocessed for pairing.
pub type G1Prepared = <Bls12_381 as Pairing>::G1Prepared;
/// An element in G2 preprocessed for pairing.
pub type G2Prepared = <Bls12_381 as Pairing>::G2Prepared;
/// Pairing target field.
pub type TargetField = <Bls12_381 as Pairing>::TargetField;

/// Curve hooks jumping into [`host_calls`] host functions.
#[derive(Copy, Clone)]
pub struct HostHooks;

impl CurveHooks for HostHooks {
	fn multi_miller_loop(
		g1: impl Iterator<Item = G1Prepared>,
		g2: impl Iterator<Item = G2Prepared>,
	) -> TargetField {
		host_calls::bls12_381_multi_miller_loop(utils::encode_iter(g1), utils::encode_iter(g2))
			.and_then(|res| utils::decode::<TargetField>(res))
			.unwrap_or_default()
	}

	fn final_exponentiation(target: TargetField) -> TargetField {
		host_calls::bls12_381_final_exponentiation(utils::encode(target))
			.and_then(|res| utils::decode::<TargetField>(res))
			.unwrap_or_default()
	}

	fn msm_g1(bases: &[G1Affine], scalars: &[ScalarField]) -> G1Projective {
		host_calls::bls12_381_msm_g1(utils::encode(bases), utils::encode(scalars))
			.and_then(|res| utils::decode::<G1Affine>(res))
			.unwrap_or_default()
			.into_group()
	}

	fn msm_g2(bases: &[G2Affine], scalars: &[ScalarField]) -> G2Projective {
		host_calls::bls12_381_msm_g2(utils::encode(bases), utils::encode(scalars))
			.and_then(|res| utils::decode::<G2Affine>(res))
			.unwrap_or_default()
			.into_group()
	}

	fn mul_projective_g1(base: &G1Projective, scalar: &[u64]) -> G1Projective {
		host_calls::bls12_381_mul_affine_g1(
			utils::encode(base.into_affine()),
			utils::encode(scalar),
		)
		.and_then(|res| utils::decode::<G1Affine>(res))
		.unwrap_or_default()
		.into_group()
	}

	fn mul_projective_g2(base: &G2Projective, scalar: &[u64]) -> G2Projective {
		host_calls::bls12_381_mul_affine_g2(
			utils::encode(base.into_affine()),
			utils::encode(scalar),
		)
		.and_then(|res| utils::decode::<G2Affine>(res))
		.unwrap_or_default()
		.into_group()
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
	/// Receives encoded:
	/// - `a`: `Vec<G1Affine>`.
	/// - `b`: `Vec<G2Affine>`.
	/// Returns encoded: `TargetField`.
	fn bls12_381_multi_miller_loop(
		a: PassFatPointerAndRead<Vec<u8>>,
		b: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::multi_miller_loop::<ark_bls12_381::Bls12_381>(a, b)
	}

	/// Pairing final exponentiation for *BLS12-381*.
	///
	/// Receives encoded: `TargetField`.
	/// Returns encoded: `TargetField`.
	fn bls12_381_final_exponentiation(
		f: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::final_exponentiation::<ark_bls12_381::Bls12_381>(f)
	}

	/// Multi scalar multiplication on *G1* for *BLS12-381*.
	///
	/// Receives encoded:
	/// - `bases`: `Vec<G1Affine>`.
	/// - `scalars`: `Vec<ScalarField>`.
	/// Returns encoded: `G1Affine`.
	fn bls12_381_msm_g1(
		bases: PassFatPointerAndRead<Vec<u8>>,
		scalars: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::msm_sw::<ark_bls12_381::g1::Config>(bases, scalars)
	}

	/// Multi scalar multiplication on *G2* for *BLS12-381*.
	///
	/// Receives encoded:
	/// - `bases`: `Vec<G2Affine>`.
	/// - `scalars`: `Vec<ScalarField>`.
	/// Returns encoded: `G2Affine`.
	fn bls12_381_msm_g2(
		bases: PassFatPointerAndRead<Vec<u8>>,
		scalars: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::msm_sw::<ark_bls12_381::g2::Config>(bases, scalars)
	}

	/// Affine multiplication on *G1* for *BLS12-381*.
	///
	/// Receives encoded:
	/// - `base`: `G1Affine`.
	/// - `scalar`: `BigInteger`.
	/// Returns encoded: `G1Affine`.
	fn bls12_381_mul_affine_g1(
		base: PassFatPointerAndRead<Vec<u8>>,
		scalar: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::mul_affine_sw::<ark_bls12_381::g1::Config>(base, scalar)
	}

	/// Affine multiplication on *G2* for *BLS12-381*.
	///
	/// Receives encoded:
	/// - `base`: `G2Affine`.
	/// - `scalar`: `BigInteger`.
	/// Returns encoded: `G2Affine`.
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
	use crate::utils::testing::*;

	#[test]
	fn mul_g1_works() {
		mul_test::<G1Affine, ark_bls12_381::G1Affine>();
	}

	#[test]
	fn msm_g1_works() {
		msm_test::<G1Affine, ark_bls12_381::G1Affine>();
	}

	#[test]
	fn mul_g2_works() {
		mul_test::<G2Affine, ark_bls12_381::G2Affine>();
	}

	#[test]
	fn msm_g2_works() {
		msm_test::<G2Affine, ark_bls12_381::G2Affine>();
	}

	#[test]
	fn pairing_works() {
		pairing_test::<Bls12_381, ark_bls12_381::Bls12_381>();
	}
}
