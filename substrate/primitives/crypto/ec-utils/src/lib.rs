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

//! Elliptic Curves host functions which may be used to handle some of the *Arkworks*
//! computationally expensive operations.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

mod utils;

use sp_runtime_interface::runtime_interface;
use sp_std::vec::Vec;
use utils::*;

/// Interfaces for working with *Arkworks* elliptic curves related types from within the runtime.
///
/// All types are (de-)serialized through the wrapper types from the `ark-scale` trait,
/// with `ark_scale::{ArkScale, ArkScaleProjective}`.
///
/// `ArkScale`'s `Usage` generic parameter is expected to be set to `HOST_CALL`, which is
/// a shortcut for "not-validated" and "not-compressed".
#[runtime_interface]
pub trait EllipticCurves {
	/// Pairing multi Miller loop for BLS12-377.
	///
	/// - Receives encoded:
	///   - `a: ArkScale<Vec<ark_ec::bls12::G1Prepared::<ark_bls12_377::Config>>>`.
	///   - `b: ArkScale<Vec<ark_ec::bls12::G2Prepared::<ark_bls12_377::Config>>>`.
	/// - Returns encoded: ArkScale<MillerLoopOutput<Bls12<ark_bls12_377::Config>>>.
	fn bls12_377_multi_miller_loop(a: Vec<u8>, b: Vec<u8>) -> Result<Vec<u8>, ()> {
		multi_miller_loop::<ark_bls12_377::Bls12_377>(a, b)
	}

	/// Pairing final exponentiation for BLS12-377.
	///
	/// - Receives encoded: `ArkScale<MillerLoopOutput<Bls12<ark_bls12_377::Config>>>`.
	/// - Returns encoded: `ArkScale<PairingOutput<Bls12<ark_bls12_377::Config>>>`.
	fn bls12_377_final_exponentiation(f: Vec<u8>) -> Result<Vec<u8>, ()> {
		final_exponentiation::<ark_bls12_377::Bls12_377>(f)
	}

	/// Projective multiplication on G1 for BLS12-377.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_bls12_377::G1Projective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_377::G1Projective>`.
	fn bls12_377_mul_projective_g1(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_bls12_377::g1::Config>(base, scalar)
	}

	/// Projective multiplication on G2 for BLS12-377.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_bls12_377::G2Projective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_377::G2Projective>`.
	fn bls12_377_mul_projective_g2(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_bls12_377::g2::Config>(base, scalar)
	}

	/// Multi scalar multiplication on G1 for BLS12-377.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_bls12_377::G1Affine]>`.
	///   - `scalars`: `ArkScale<&[ark_bls12_377::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_377::G1Projective>`.
	fn bls12_377_msm_g1(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_bls12_377::g1::Config>(bases, scalars)
	}

	/// Multi scalar multiplication on G2 for BLS12-377.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_bls12_377::G2Affine]>`.
	///   - `scalars`: `ArkScale<&[ark_bls12_377::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_377::G2Projective>`.
	fn bls12_377_msm_g2(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_bls12_377::g2::Config>(bases, scalars)
	}

	/// Pairing multi Miller loop for BLS12-381.
	///
	/// - Receives encoded:
	///   - `a`: `ArkScale<Vec<ark_ec::bls12::G1Prepared::<ark_bls12_381::Config>>>`.
	///   - `b`: `ArkScale<Vec<ark_ec::bls12::G2Prepared::<ark_bls12_381::Config>>>`.
	/// - Returns encoded: ArkScale<MillerLoopOutput<Bls12<ark_bls12_381::Config>>>
	fn bls12_381_multi_miller_loop(a: Vec<u8>, b: Vec<u8>) -> Result<Vec<u8>, ()> {
		multi_miller_loop::<ark_bls12_381::Bls12_381>(a, b)
	}

	/// Pairing final exponentiation for BLS12-381.
	///
	/// - Receives encoded: `ArkScale<MillerLoopOutput<Bls12<ark_bls12_381::Config>>>`.
	/// - Returns encoded: `ArkScale<PairingOutput<Bls12<ark_bls12_381::Config>>>`.
	fn bls12_381_final_exponentiation(f: Vec<u8>) -> Result<Vec<u8>, ()> {
		final_exponentiation::<ark_bls12_381::Bls12_381>(f)
	}

	/// Projective multiplication on G1 for BLS12-381.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_bls12_381::G1Projective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_381::G1Projective>`.
	fn bls12_381_mul_projective_g1(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_bls12_381::g1::Config>(base, scalar)
	}

	/// Projective multiplication on G2 for BLS12-381.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_bls12_381::G2Projective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_381::G2Projective>`.
	fn bls12_381_mul_projective_g2(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_bls12_381::g2::Config>(base, scalar)
	}

	/// Multi scalar multiplication on G1 for BLS12-381.
	///
	/// - Receives encoded:
	///   - bases: `ArkScale<&[ark_bls12_381::G1Affine]>`.
	///   - scalars: `ArkScale<&[ark_bls12_381::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_381::G1Projective>`.
	fn bls12_381_msm_g1(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_bls12_381::g1::Config>(bases, scalars)
	}

	/// Multi scalar multiplication on G2 for BLS12-381.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_bls12_381::G2Affine]>`.
	///   - `scalars`: `ArkScale<&[ark_bls12_381::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_381::G2Projective>`.
	fn bls12_381_msm_g2(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_bls12_381::g2::Config>(bases, scalars)
	}

	/// Pairing multi Miller loop for BW6-761.
	///
	/// - Receives encoded:
	///   - `a`: `ArkScale<Vec<ark_ec::bw6::G1Prepared::<ark_bw6_761::Config>>>`.
	///   - `b`: `ArkScale<Vec<ark_ec::bw6::G2Prepared::<ark_bw6_761::Config>>>`.
	/// - Returns encoded: `ArkScale<MillerLoopOutput<Bls12<ark_bw6_761::Config>>>`.
	fn bw6_761_multi_miller_loop(a: Vec<u8>, b: Vec<u8>) -> Result<Vec<u8>, ()> {
		multi_miller_loop::<ark_bw6_761::BW6_761>(a, b)
	}

	/// Pairing final exponentiation for BW6-761.
	///
	/// - Receives encoded: `ArkScale<MillerLoopOutput<BW6<ark_bw6_761::Config>>>`.
	/// - Returns encoded: `ArkScale<PairingOutput<BW6<ark_bw6_761::Config>>>`.
	fn bw6_761_final_exponentiation(f: Vec<u8>) -> Result<Vec<u8>, ()> {
		final_exponentiation::<ark_bw6_761::BW6_761>(f)
	}

	/// Projective multiplication on G1 for BW6-761.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_bw6_761::G1Projective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bw6_761::G1Projective>`.
	fn bw6_761_mul_projective_g1(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_bw6_761::g1::Config>(base, scalar)
	}

	/// Projective multiplication on G2 for BW6-761.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_bw6_761::G2Projective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bw6_761::G2Projective>`.
	fn bw6_761_mul_projective_g2(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_bw6_761::g2::Config>(base, scalar)
	}

	/// Multi scalar multiplication on G1 for BW6-761.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_bw6_761::G1Affine]>`.
	///   - `scalars`: `ArkScale<&[ark_bw6_761::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bw6_761::G1Projective>`.
	fn bw6_761_msm_g1(bases: Vec<u8>, bigints: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_bw6_761::g1::Config>(bases, bigints)
	}

	/// Multi scalar multiplication on G2 for BW6-761.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_bw6_761::G2Affine]>`.
	///   - `scalars`: `ArkScale<&[ark_bw6_761::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bw6_761::G2Projective>`.
	fn bw6_761_msm_g2(bases: Vec<u8>, bigints: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_bw6_761::g2::Config>(bases, bigints)
	}

	/// Twisted Edwards projective multiplication for Ed-on-BLS12-377.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_ed_on_bls12_377::EdwardsProjective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_ed_on_bls12_377::EdwardsProjective>`.
	fn ed_on_bls12_377_mul_projective(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_te::<ark_ed_on_bls12_377::EdwardsConfig>(base, scalar)
	}

	/// Twisted Edwards multi scalar multiplication for Ed-on-BLS12-377.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_ed_on_bls12_377::EdwardsAffine]>`.
	///   - `scalars`: `ArkScale<&[ark_ed_on_bls12_377::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_ed_on_bls12_377::EdwardsProjective>`.
	fn ed_on_bls12_377_msm(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_te::<ark_ed_on_bls12_377::EdwardsConfig>(bases, scalars)
	}

	/// Short Weierstrass projective multiplication for Ed-on-BLS12-381-Bandersnatch.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::SWProjective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::SWProjective>`.
	fn ed_on_bls12_381_bandersnatch_sw_mul_projective(
		base: Vec<u8>,
		scalar: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_ed_on_bls12_381_bandersnatch::SWConfig>(base, scalar)
	}

	/// Twisted Edwards projective multiplication for Ed-on-BLS12-381-Bandersnatch.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::EdwardsProjective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded:
	///   `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::EdwardsProjective>`.
	fn ed_on_bls12_381_bandersnatch_te_mul_projective(
		base: Vec<u8>,
		scalar: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		mul_projective_te::<ark_ed_on_bls12_381_bandersnatch::EdwardsConfig>(base, scalar)
	}

	/// Short Weierstrass multi scalar multiplication for Ed-on-BLS12-381-Bandersnatch.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_ed_on_bls12_381_bandersnatch::SWAffine]>`.
	///   - `scalars`: `ArkScale<&[ark_ed_on_bls12_381_bandersnatch::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::SWProjective>`.
	fn ed_on_bls12_381_bandersnatch_sw_msm(
		bases: Vec<u8>,
		scalars: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_ed_on_bls12_381_bandersnatch::SWConfig>(bases, scalars)
	}

	/// Twisted Edwards multi scalar multiplication for Ed-on-BLS12-381-Bandersnatch.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::EdwardsProjective>`.
	///   - `scalars`: `ArkScale<&[ark_ed_on_bls12_381_bandersnatch::Fr]>`.
	/// - Returns encoded:
	///   `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::EdwardsProjective>`.
	fn ed_on_bls12_381_bandersnatch_te_msm(
		bases: Vec<u8>,
		scalars: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		msm_te::<ark_ed_on_bls12_381_bandersnatch::EdwardsConfig>(bases, scalars)
	}
}
