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

//! *Ed-on-BLS12-381-Bandersnatch* types and host functions.
//!
//! Bandersnatch is an *incomplete* twisted Edwards curve: the HWCD
//! add/double formulas can produce projective points with `z = 0` when fed
//! cofactor-admixed (non-prime-order-subgroup) inputs. Such points have no
//! affine representative, so the standard `(x || y)` FFI channel cannot
//! carry them — arkworks' `From<Projective> for Affine` panics on
//! `z.inverse().unwrap()`.
//!
//! The shared [`utils::mul_te`] / [`utils::msm_te`] helpers detect the
//! degenerate case via [`utils::IntoAffineSafe`] and, instead of attempting
//! to serialize it, write a deterministic non-result: `mul` echoes the
//! input `base`, and `msm` writes `(0, -1)` — a TE point of order 2 that
//! is universally outside any prime-order subgroup. The wire format stays
//! byte-identical to `ArkScale<EdwardsAffine>` — no sentinel bit, no
//! dedicated projective codec.
//!
//! Semantic contract: `mul(base, scalar)` returns `scalar · base` for all
//! subgroup-valid inputs and *may* return `base` itself when the result
//! lands at `z = 0`; `msm(bases, scalars)` returns the correct sum for
//! subgroup-valid bases and *may* return `(0, -1)` on a `z = 0` result.
//! Both fallbacks are designed so that an honest caller's downstream
//! subgroup check on the output will catch the degenerate case (the echo
//! of a non-subgroup `base` stays non-subgroup; `(0, -1)` is never in the
//! prime-order subgroup). Honest callers that subgroup-validate inputs
//! upstream never observe the degenerate branch in the first place.

use crate::utils::{self, HostcallResult, IntoAffineSafe, FAIL_MSG};
use alloc::vec::Vec;
use ark_ec::{AffineRepr, CurveConfig};
use ark_ed_on_bls12_381_bandersnatch_ext::CurveHooks;
use sp_runtime_interface::{
	pass_by::{PassFatPointerAndRead, PassFatPointerAndWrite},
	runtime_interface,
};

/// Group configuration.
pub type BandersnatchConfig = ark_ed_on_bls12_381_bandersnatch_ext::BandersnatchConfig<HostHooks>;

/// Group configuration for Twisted Edwards form (equal to [`BandersnatchConfig`]).
pub type EdwardsConfig = ark_ed_on_bls12_381_bandersnatch_ext::EdwardsConfig<HostHooks>;
/// Twisted Edwards form point affine representation.
pub type EdwardsAffine = ark_ed_on_bls12_381_bandersnatch_ext::EdwardsAffine<HostHooks>;
/// Twisted Edwards form point projective representation.
pub type EdwardsProjective = ark_ed_on_bls12_381_bandersnatch_ext::EdwardsProjective<HostHooks>;

/// Group configuration for Short Weierstrass form (equal to [`BandersnatchConfig`]).
pub type SWConfig = ark_ed_on_bls12_381_bandersnatch_ext::SWConfig<HostHooks>;
/// Short Weierstrass form point affine representation.
pub type SWAffine = ark_ed_on_bls12_381_bandersnatch_ext::SWAffine<HostHooks>;
/// Short Weierstrass form point projective representation.
pub type SWProjective = ark_ed_on_bls12_381_bandersnatch_ext::SWProjective<HostHooks>;

/// Group scalar field (Fr).
pub type ScalarField = <BandersnatchConfig as CurveConfig>::ScalarField;

/// Curve hooks jumping into [`host_calls`] host functions.
#[derive(Copy, Clone)]
pub struct HostHooks;

impl CurveHooks for HostHooks {
	fn msm_te(bases: &[EdwardsAffine], scalars: &[ScalarField]) -> EdwardsProjective {
		let mut out = utils::buffer_for::<EdwardsAffine>();
		host_calls::ed_on_bls12_381_bandersnatch_msm(
			&utils::encode(bases),
			&utils::encode(scalars),
			&mut out,
		)
		.and_then(|_| utils::decode::<EdwardsAffine>(&out))
		.expect(FAIL_MSG)
		.into_group()
	}

	fn mul_projective_te(base: &EdwardsProjective, scalar: &[u64]) -> EdwardsProjective {
		// A `z = 0` projective cannot ride the affine FFI channel —
		// `into_affine()` would panic. `into_affine_safe()` returns `None`
		// in that case; we honor the same "echo input on degenerate"
		// contract the host applies, locally. Honest subgroup-validated
		// callers never produce such a projective.
		let Some(base_aff) = base.into_affine_safe() else {
			return *base;
		};
		let mut out = utils::buffer_for::<EdwardsAffine>();
		host_calls::ed_on_bls12_381_bandersnatch_mul(
			&utils::encode(base_aff),
			&utils::encode(scalar),
			&mut out,
		)
		.and_then(|_| utils::decode::<EdwardsAffine>(&out))
		.expect(FAIL_MSG)
		.into_group()
	}
}

/// Interfaces for working with *Arkworks* *Ed-on-BLS12-381-Bandersnatch* elliptic curve related
/// types from within the runtime.
///
/// All types are (de-)serialized through the wrapper types from `ark-scale`.
///
/// `ArkScale`'s `Usage` generic parameter is expected to be set to "not-validated"
/// and "not-compressed".
///
/// When the projective result of a host call lands at `z = 0` (only reachable
/// via non-subgroup inputs), the host writes a deterministic non-result that
/// stays outside the prime-order subgroup: the input `base` for `mul`, the
/// universal sentinel `(0, -1)` for `msm`.
#[runtime_interface]
pub trait HostCalls {
	/// Twisted Edwards multi scalar multiplication for *Ed-on-BLS12-381-Bandersnatch*.
	///
	/// Receives encoded:
	/// - `bases`: `Vec<EdwardsAffine>`.
	/// - `scalars`: `Vec<ScalarField>`.
	/// Writes encoded: `EdwardsAffine` to `out`.
	fn ed_on_bls12_381_bandersnatch_msm(
		bases: PassFatPointerAndRead<&[u8]>,
		scalars: PassFatPointerAndRead<&[u8]>,
		out: PassFatPointerAndWrite<&mut [u8]>,
	) -> HostcallResult {
		utils::msm_te::<ark_ed_on_bls12_381_bandersnatch::EdwardsConfig>(bases, scalars, out)
	}

	/// Twisted Edwards affine multiplication for *Ed-on-BLS12-381-Bandersnatch*.
	///
	/// Receives encoded:
	/// - `base`: `EdwardsAffine`.
	/// - `scalar`: `BigInteger`.
	/// Writes encoded `EdwardsAffine` to `out`.
	fn ed_on_bls12_381_bandersnatch_mul(
		base: PassFatPointerAndRead<&[u8]>,
		scalar: PassFatPointerAndRead<&[u8]>,
		out: PassFatPointerAndWrite<&mut [u8]>,
	) -> HostcallResult {
		utils::mul_te::<ark_ed_on_bls12_381_bandersnatch::EdwardsConfig>(base, scalar, out)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::utils::testing::*;
	use ark_ec::twisted_edwards::{Projective as TEProjective, TECurveConfig};
	use ark_ed_on_bls12_381_bandersnatch::{EdwardsConfig as RawConfig, Fq, Fr};
	use ark_ff::{AdditiveGroup, Field, PrimeField, Zero};

	#[test]
	fn mul_works() {
		mul_te_test::<EdwardsAffine, ark_ed_on_bls12_381_bandersnatch::EdwardsAffine>();
	}

	#[test]
	fn msm_works() {
		msm_te_test::<EdwardsAffine, ark_ed_on_bls12_381_bandersnatch::EdwardsAffine>();
	}

	#[test]
	fn mul_works_sw() {
		mul_test::<SWAffine, ark_ed_on_bls12_381_bandersnatch::SWAffine>();
	}

	#[test]
	fn msm_works_sw() {
		msm_test::<SWAffine, ark_ed_on_bls12_381_bandersnatch::SWAffine>();
	}

	/// `y = 2` is on the curve but outside the prime-order subgroup
	/// (cofactor admixture). Multiplying it by `Fr::MODULUS` makes the
	/// HWCD arithmetic land at `z = 0`. The host call must not panic and
	/// must echo the input affine bytes back to the runtime.
	#[test]
	fn host_mul_with_z_zero_result_echoes_input() {
		let p_aff = ark_ed_on_bls12_381_bandersnatch::EdwardsAffine::get_point_from_y_unchecked(
			Fq::from(2u64),
			false,
		)
		.unwrap();
		// Sanity: the raw operation does produce z = 0.
		let proj: TEProjective<RawConfig> = p_aff.into_group();
		let raw_res = <RawConfig as TECurveConfig>::mul_projective(&proj, Fr::MODULUS.0.as_ref());
		assert!(raw_res.z.is_zero(), "test precondition: y=2 * Fr::MODULUS must hit z=0");

		// Now exercise the host call: input is the affine point, output buffer
		// should come back equal to the input bytes.
		let scalar_bigint: Vec<u64> = Fr::MODULUS.0.to_vec();
		let input_enc = utils::encode(p_aff);
		let scalar_enc = utils::encode(scalar_bigint);
		let mut out = utils::buffer_for::<EdwardsAffine>();
		host_calls::ed_on_bls12_381_bandersnatch_mul(&input_enc, &scalar_enc, &mut out).unwrap();
		assert_eq!(out, input_enc, "z=0 result must echo input affine bytes");

		// And the runtime-side hook returns the input projective (after the
		// usual affine→projective lift, with z = 1).
		let p_ext: EdwardsProjective =
			EdwardsAffine::get_point_from_y_unchecked(Fq::from(2u64), false)
				.unwrap()
				.into_group();
		let r = <HostHooks as CurveHooks>::mul_projective_te(&p_ext, Fr::MODULUS.0.as_ref());
		assert_eq!(r, p_ext, "hook must return input on degenerate");
	}

	/// `z = 0` input to `mul_projective_te` is short-circuited locally: we
	/// can't serialize it for the host, so we honor the same contract by
	/// returning the input unchanged. The detection now goes through
	/// `IntoAffineSafe::into_affine_safe`, which returns `None` on `z = 0`.
	#[test]
	fn mul_projective_with_z_zero_input_returns_input() {
		use ark_std::{test_rng, UniformRand};
		let mut rng = test_rng();
		let y = Fq::rand(&mut rng);
		let t = Fq::rand(&mut rng);
		let p = EdwardsProjective::new_unchecked(Fq::ZERO, y, t, Fq::ZERO);
		let r = <HostHooks as CurveHooks>::mul_projective_te(&p, &[7u64, 0, 0, 0]);
		assert_eq!(r.x, p.x);
		assert_eq!(r.y, p.y);
		assert_eq!(r.t, p.t);
		assert_eq!(r.z, p.z);
	}

	/// The `msm_te` helper falls back to the universal TE sentinel
	/// `(0, -1)` when the projective result has `z = 0`. The contract
	/// hinges on two properties:
	///   1. `(0, -1)` is on every TE curve (`a·0 + 1 = 1 + 0`), so it encodes as a valid `TEAffine`
	///      and the runtime side can decode it without rejecting "off-curve."
	///   2. `(0, -1)` has order 2, so it is not in any prime-order subgroup — a downstream
	///      `is_in_correct_subgroup_*` check on the helper's output will reject the degenerate case
	///      rather than silently accepting an identity-like value.
	///
	/// We don't drive a real msm to `z = 0` here: msm scalars are `Fr`
	/// (canonicalized mod `r`), so the `Fr::MODULUS` trigger we use for
	/// `mul_projective` can't be reproduced through msm without
	/// curve-specific scalar search. Instead we verify the design
	/// directly: the fallback constant has the required properties and
	/// `IntoAffineSafe` produces `None` for a `z = 0` projective (which
	/// is exactly the branch where `msm_te` substitutes the sentinel).
	#[test]
	fn msm_fallback_is_non_subgroup_te_sentinel() {
		use ark_ed_on_bls12_381_bandersnatch::EdwardsAffine as RawAffine;

		// (1) The fallback constant the helper hardcodes.
		let fallback = RawAffine::new_unchecked(Fq::ZERO, -Fq::ONE);
		assert!(fallback.is_on_curve(), "fallback (0, -1) must lie on the curve");
		assert!(
			!fallback.is_in_correct_subgroup_assuming_on_curve(),
			"fallback (0, -1) must NOT be in the prime-order subgroup",
		);

		// (2) The trait-level z=0 detection that drives the helper into
		// the fallback branch. Use an F-exception shape (X=0, Y!=0, Z=0):
		// arkworks' standard into_affine() panics here; into_affine_safe
		// must return None instead, so msm_te substitutes the sentinel.
		let degenerate = TEProjective::<RawConfig>::new_unchecked(
			Fq::ZERO,        // X = 0
			Fq::from(7u64),  // Y != 0 (F-exception)
			Fq::from(11u64), // arbitrary T
			Fq::ZERO,        // Z = 0
		);
		assert!(
			degenerate.into_affine_safe().is_none(),
			"z=0 projective must map to None via IntoAffineSafe",
		);

		// (3) End-to-end: encode the fallback and decode it back, to
		// confirm the wire bytes the helper would write on the degenerate
		// path round-trip to the same point through the standard
		// `ArkScale<TEAffine>` codec the runtime uses.
		let mut buf = utils::buffer_for::<EdwardsAffine>();
		utils::encode_into(fallback, &mut buf).unwrap();
		let decoded: RawAffine = utils::decode(&buf).unwrap();
		assert_eq!(decoded, fallback);
		assert!(!decoded.is_in_correct_subgroup_assuming_on_curve());
	}
}
