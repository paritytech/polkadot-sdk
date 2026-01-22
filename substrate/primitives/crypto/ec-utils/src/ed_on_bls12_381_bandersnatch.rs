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

//! Elliptic Curves host functions to handle some of the *Arkworks* *Ed-on-BLS12-381-Bandersnatch*
//! computationally expensive operations.

use crate::utils;
use alloc::vec::Vec;
use ark_ec::{CurveConfig, CurveGroup};
use ark_ed_on_bls12_381_bandersnatch_ext::CurveHooks;
use sp_runtime_interface::{
	pass_by::{AllocateAndReturnByCodec, PassFatPointerAndRead},
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
		host_calls::ed_on_bls12_381_bandersnatch_te_msm(
			utils::encode(bases),
			utils::encode(scalars),
		)
		.and_then(|res| utils::decode::<EdwardsAffine>(res))
		.unwrap_or_default()
		.into()
	}

	fn mul_projective_te(base: &EdwardsProjective, scalar: &[u64]) -> EdwardsProjective {
		host_calls::ed_on_bls12_381_bandersnatch_te_mul_affine(
			utils::encode(base.into_affine()),
			utils::encode(scalar),
		)
		.and_then(|res| utils::decode::<EdwardsAffine>(res))
		.unwrap_or_default()
		.into()
	}

	fn msm_sw(bases: &[SWAffine], scalars: &[ScalarField]) -> SWProjective {
		host_calls::ed_on_bls12_381_bandersnatch_sw_msm(
			utils::encode(bases),
			utils::encode(scalars),
		)
		.and_then(|res| utils::decode::<SWAffine>(res))
		.unwrap_or_default()
		.into()
	}

	fn mul_projective_sw(base: &SWProjective, scalar: &[u64]) -> SWProjective {
		host_calls::ed_on_bls12_381_bandersnatch_sw_mul_affine(
			utils::encode(base.into_affine()),
			utils::encode(scalar),
		)
		.and_then(|res| utils::decode::<SWAffine>(res))
		.unwrap_or_default()
		.into()
	}
}

/// Interfaces for working with *Arkworks* *Ed-on-BLS12-381-Bandersnatch* elliptic curve
/// related types from within the runtime.
///
/// All types are (de-)serialized through the wrapper types from the `ark-scale` trait,
/// with `ark_scale::{ArkScale, ArkScaleProjective}`.
///
/// `ArkScale`'s `Usage` generic parameter is expected to be set to "not-validated"
/// and "not-compressed".
#[runtime_interface]
pub trait HostCalls {
	/// Twisted Edwards multi scalar multiplication for *Ed-on-BLS12-381-Bandersnatch*.
	///
	/// Receives encoded:
	/// - `base`: `EdwardsAffine`.
	/// - `scalars`: `Vec<ScalarField>`.
	/// Returns encoded: `EdwardsAffine`.
	fn ed_on_bls12_381_bandersnatch_te_msm(
		bases: PassFatPointerAndRead<Vec<u8>>,
		scalars: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::msm_te::<ark_ed_on_bls12_381_bandersnatch::EdwardsConfig>(bases, scalars)
	}

	/// Twisted Edwards multiplication for *Ed-on-BLS12-381-Bandersnatch*.
	///
	/// Receives encoded:
	///   - `base`: `EdwardsAffine`.
	///   - `scalar`: `BigInteger`.
	/// Returns encoded: `EdwardsAffine`.
	fn ed_on_bls12_381_bandersnatch_te_mul_affine(
		base: PassFatPointerAndRead<Vec<u8>>,
		scalar: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::mul_affine_te::<ark_ed_on_bls12_381_bandersnatch::EdwardsConfig>(base, scalar)
	}

	/// Short Weierstrass multi scalar multiplication for *Ed-on-BLS12-381-Bandersnatch*.
	///
	/// Receives encoded:
	/// - `bases`: `Vec<SWAffine>`.
	/// - `scalars`: `Vec<ScalarField>`.
	/// Returns encoded `SWAffine`.
	fn ed_on_bls12_381_bandersnatch_sw_msm(
		bases: PassFatPointerAndRead<Vec<u8>>,
		scalars: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::msm_sw::<ark_ed_on_bls12_381_bandersnatch::SWConfig>(bases, scalars)
	}

	/// Short Weierstrass projective multiplication for *Ed-on-BLS12-381-Bandersnatch*.
	///
	/// Receives encoded:
	/// - `base`: `SWAffine`.
	/// - `scalar`: `BigInteger`.
	/// Returns encoded `SWAffine`.
	fn ed_on_bls12_381_bandersnatch_sw_mul_affine(
		base: PassFatPointerAndRead<Vec<u8>>,
		scalar: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::mul_affine_sw::<ark_ed_on_bls12_381_bandersnatch::SWConfig>(base, scalar)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::utils::testing::*;

	#[test]
	fn te_mul_works() {
		mul_te_test::<EdwardsAffine, ark_ed_on_bls12_381_bandersnatch::EdwardsAffine>();
	}

	#[test]
	fn te_msm_works() {
		msm_te_test::<EdwardsAffine, ark_ed_on_bls12_381_bandersnatch::EdwardsAffine>();
	}

	#[test]
	fn sw_mul_works() {
		mul_test::<SWAffine, ark_ed_on_bls12_381_bandersnatch::SWAffine>();
	}

	#[test]
	fn sw_msm_works() {
		msm_test::<SWAffine, ark_ed_on_bls12_381_bandersnatch::SWAffine>();
	}
}
