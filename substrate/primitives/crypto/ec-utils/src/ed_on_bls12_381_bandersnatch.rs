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
//! carry them: arkworks' `From<Projective> for Affine` panics on
//! `z.inverse().unwrap()`.
//!
//! The shared `utils::mul_te` / `utils::msm_te` helpers detect the
//! degenerate case via `utils::IntoAffineSafe` and return
//! `utils::Error::DegeneratePoint` across the FFI boundary instead of
//! attempting to serialize an unrepresentable point. The runtime-side
//! hooks defined in this module catch that error and substitute the
//! unified `(0, -1)` fallback: a TE point of order 2 that is universally
//! outside any prime-order subgroup, on every TE curve. The wire format
//! stays byte-identical to `ArkScale<EdwardsAffine>`: no sentinel bit, no
//! dedicated projective codec.
//!
//! Semantic contract: both `mul(base, scalar)` and `msm(bases, scalars)`
//! return the mathematically correct result for all subgroup-valid inputs;
//! when the projective result lands at `z = 0` they return `(0, -1)`. An
//! honest caller's downstream subgroup check on the output catches the
//! degenerate case (`(0, -1)` is never in the prime-order subgroup), and
//! callers that subgroup-validate inputs upstream never observe it in the
//! first place.

use crate::utils::{
	self, te_non_subgroup_fallback, Error, HostcallResult, IntoAffineSafe, FAIL_MSG,
};
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

/// The unified `(0, -1)` non-subgroup fallback (lifted to projective). Used
/// by the hooks to substitute a `z = 0` result that cannot ride the affine
/// FFI channel.
#[inline(always)]
fn fallback_projective() -> EdwardsProjective {
	te_non_subgroup_fallback::<EdwardsConfig>().into_group()
}

/// Curve hooks jumping into [`host_calls`] host functions.
#[derive(Copy, Clone)]
pub struct HostHooks;

impl CurveHooks for HostHooks {
	fn msm_te(bases: &[EdwardsAffine], scalars: &[ScalarField]) -> EdwardsProjective {
		let mut out = utils::buffer_for::<EdwardsAffine>();
		match host_calls::ed_on_bls12_381_bandersnatch_msm(
			&utils::encode(bases),
			&utils::encode(scalars),
			&mut out,
		) {
			Ok(()) => utils::decode::<EdwardsAffine>(&out).expect(FAIL_MSG).into_group(),
			// Bandersnatch is incomplete: HWCD on non-subgroup bases can land
			// at `z = 0`. Substitute the unified `(0, -1)` non-subgroup
			// fallback so a downstream subgroup check rejects it.
			Err(Error::DegeneratePoint) => fallback_projective(),
			Err(_) => panic!("{FAIL_MSG}"),
		}
	}

	fn mul_projective_te(base: &EdwardsProjective, scalar: &[u64]) -> EdwardsProjective {
		// A `z = 0` projective cannot ride the affine FFI channel:
		// `into_affine()` would panic. `into_affine_safe()` returns `None`
		// in that case; we honor the same unified `(0, -1)` fallback the
		// host applies on its side, locally. Honest subgroup-validated
		// callers never produce such a projective.
		let Some(base_aff) = base.into_affine_safe() else {
			return fallback_projective();
		};
		let mut out = utils::buffer_for::<EdwardsAffine>();
		match host_calls::ed_on_bls12_381_bandersnatch_mul(
			&utils::encode(base_aff),
			&utils::encode(scalar),
			&mut out,
		) {
			Ok(()) => utils::decode::<EdwardsAffine>(&out).expect(FAIL_MSG).into_group(),
			Err(Error::DegeneratePoint) => fallback_projective(),
			Err(_) => panic!("{FAIL_MSG}"),
		}
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
/// via non-subgroup inputs), the host returns `utils::Error::DegeneratePoint`
/// instead of panicking, and the runtime-side `HostHooks` impl substitutes the
/// unified `(0, -1)` non-subgroup fallback. See the module-level doc for the
/// full contract.
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
	use ark_ec::twisted_edwards::{Affine as TEAffine, Projective as TEProjective, TECurveConfig};
	use ark_ed_on_bls12_381_bandersnatch::{EdwardsConfig as RawConfig, Fq, Fr};
	use ark_ff::{AdditiveGroup, PrimeField, Zero};

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

	/// The cofactor-admixed `y = 2` non-subgroup point used as the
	/// degenerate trigger throughout the tests below. Generic so the same
	/// constructor serves both `RawConfig` (for raw-arithmetic precondition
	/// checks) and `EdwardsConfig<HostHooks>` (the runtime-facing type).
	fn y2_non_subgroup<P: TECurveConfig<BaseField = Fq>>() -> TEAffine<P> {
		TEAffine::<P>::get_point_from_y_unchecked(Fq::from(2u64), false)
			.expect("y=2 must yield a valid TEAffine point")
	}

	#[test]
	fn host_mul_with_z_zero_result_returns_fallback() {
		// Sanity: the raw operation does produce z = 0.
		let proj: TEProjective<RawConfig> = y2_non_subgroup::<RawConfig>().into_group();
		let raw_res = <RawConfig as TECurveConfig>::mul_projective(&proj, Fr::MODULUS.0.as_ref());
		assert!(raw_res.z.is_zero(), "test precondition: y=2 * Fr::MODULUS must hit z=0");

		// The raw host call surfaces the degenerate result as an error
		// (the helper can't represent it on the affine FFI channel).
		let scalar_bigint: Vec<u64> = Fr::MODULUS.0.to_vec();
		let input_enc = utils::encode(y2_non_subgroup::<EdwardsConfig>());
		let scalar_enc = utils::encode(scalar_bigint);
		let mut out = utils::buffer_for::<EdwardsAffine>();
		let err = host_calls::ed_on_bls12_381_bandersnatch_mul(&input_enc, &scalar_enc, &mut out)
			.expect_err("z=0 result must surface as Err(DegeneratePoint)");
		assert_eq!(err, Error::DegeneratePoint);

		// The runtime-side hook catches that error and substitutes the
		// `(0, -1)` fallback (lifted to projective).
		let p_ext: EdwardsProjective = y2_non_subgroup::<EdwardsConfig>().into_group();
		let r = <HostHooks as CurveHooks>::mul_projective_te(&p_ext, Fr::MODULUS.0.as_ref());
		assert_eq!(r, fallback_projective(), "hook must return (0, -1) on degenerate");
	}

	#[test]
	fn mul_projective_with_z_zero_input_returns_fallback() {
		use ark_std::{test_rng, UniformRand};
		let mut rng = test_rng();
		let y = Fq::rand(&mut rng);
		let t = Fq::rand(&mut rng);
		let p = EdwardsProjective::new_unchecked(Fq::ZERO, y, t, Fq::ZERO);
		let r = <HostHooks as CurveHooks>::mul_projective_te(&p, &[7u64, 0, 0, 0]);
		assert_eq!(r, fallback_projective(), "z=0 input must yield (0, -1) projective");
	}

	#[test]
	fn fallback_is_non_subgroup_te_sentinel() {
		use ark_ed_on_bls12_381_bandersnatch::EdwardsAffine as RawAffine;

		// (1) The fallback constant the helper hardcodes.
		let fallback: RawAffine = te_non_subgroup_fallback::<RawConfig>();
		assert!(fallback.is_on_curve(), "fallback (0, -1) must lie on the curve");
		assert!(
			!fallback.is_in_correct_subgroup_assuming_on_curve(),
			"fallback (0, -1) must NOT be in the prime-order subgroup",
		);

		// (2) The trait-level z=0 detection. Use an F-exception shape
		// (X=0, Y!=0, Z=0): arkworks' standard into_affine() panics here;
		// into_affine_safe must return None instead, which is exactly the
		// branch where `msm_te` / `mul_te` return Err(DegeneratePoint) and
		// the hook substitutes the fallback.
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
		// confirm the wire bytes the hook would write on the degenerate
		// path round-trip to the same point through the standard
		// `ArkScale<TEAffine>` codec the runtime uses.
		let mut buf = utils::buffer_for::<EdwardsAffine>();
		utils::encode_into(fallback, &mut buf).unwrap();
		let decoded: RawAffine = utils::decode(&buf).unwrap();
		assert_eq!(decoded, fallback);
		assert!(!decoded.is_in_correct_subgroup_assuming_on_curve());
	}

	#[test]
	fn y2_point_deserialize_checked_vs_unchecked() {
		use ark_scale::ark_serialize::{
			CanonicalDeserialize, CanonicalSerialize, Compress, Validate,
		};

		let p = y2_non_subgroup::<EdwardsConfig>();
		assert!(p.is_on_curve(), "y=2 point must be on curve");
		assert!(
			!p.is_in_correct_subgroup_assuming_on_curve(),
			"y=2 point must NOT be in the prime-order subgroup",
		);

		let mut bytes = Vec::new();
		p.serialize_with_mode(&mut bytes, Compress::No).unwrap();

		// `Validate::No` accepts the non-subgroup point.
		let decoded =
			EdwardsAffine::deserialize_with_mode(&bytes[..], Compress::No, Validate::No).unwrap();
		assert_eq!(decoded, p);

		// `Validate::Yes` over the same bytes rejects it at decode time.
		assert!(
			EdwardsAffine::deserialize_with_mode(&bytes[..], Compress::No, Validate::Yes).is_err(),
			"Validate::Yes must reject non-subgroup point",
		);
	}
}
