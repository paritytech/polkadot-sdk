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

use crate::utils::{ArkScale, ArkScaleProjective};
use ark_ec::CurveConfig;
use ark_ed_on_bls12_381_bandersnatch_ext::CurveHooks;
use ark_scale::scale::{Decode, Encode};
use sp_runtime_interface::runtime_interface;
use sp_std::vec::Vec;

/// TODO
#[derive(Copy, Clone)]
pub struct HostHooks;

/// TODO
pub type BandersnatchConfig = ark_ed_on_bls12_381_bandersnatch_ext::BandersnatchConfig<HostHooks>;

/// TODO
pub type EdwardsConfig = ark_ed_on_bls12_381_bandersnatch_ext::EdwardsConfig<HostHooks>;
/// TODO
pub type EdwardsAffine = ark_ed_on_bls12_381_bandersnatch_ext::EdwardsAffine<HostHooks>;
/// TODO
pub type EdwardsProjective = ark_ed_on_bls12_381_bandersnatch_ext::EdwardsProjective<HostHooks>;

/// TODO
pub type SWConfig = ark_ed_on_bls12_381_bandersnatch_ext::SWConfig<HostHooks>;
/// TODO
pub type SWAffine = ark_ed_on_bls12_381_bandersnatch_ext::SWAffine<HostHooks>;
/// TODO
pub type SWProjective = ark_ed_on_bls12_381_bandersnatch_ext::SWProjective<HostHooks>;

impl CurveHooks for HostHooks {
	fn ed_on_bls12_381_bandersnatch_te_msm(
		bases: &[EdwardsAffine],
		scalars: &[<EdwardsConfig as CurveConfig>::ScalarField],
	) -> Result<EdwardsProjective, ()> {
		let bases = ArkScale::from(bases).encode();
		let scalars = ArkScale::from(scalars).encode();

		let res =
			host_calls::ed_on_bls12_381_bandersnatch_te_msm(bases, scalars).unwrap_or_default();
		let res = ArkScaleProjective::<EdwardsProjective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn ed_on_bls12_381_bandersnatch_te_mul_projective(
		base: &EdwardsProjective,
		scalar: &[u64],
	) -> Result<EdwardsProjective, ()> {
		let base = ArkScaleProjective::from(base).encode();
		let scalar = ArkScale::from(scalar).encode();

		let res = host_calls::ed_on_bls12_381_bandersnatch_te_mul_projective(base, scalar)
			.unwrap_or_default();
		let res = ArkScaleProjective::<EdwardsProjective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn ed_on_bls12_381_bandersnatch_sw_msm(
		bases: &[SWAffine],
		scalars: &[<SWConfig as CurveConfig>::ScalarField],
	) -> Result<SWProjective, ()> {
		let bases = ArkScale::from(bases).encode();
		let scalars = ArkScale::from(scalars).encode();

		let res =
			host_calls::ed_on_bls12_381_bandersnatch_sw_msm(bases, scalars).unwrap_or_default();
		let res = ArkScaleProjective::<SWProjective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn ed_on_bls12_381_bandersnatch_sw_mul_projective(
		base: &SWProjective,
		scalar: &[u64],
	) -> Result<SWProjective, ()> {
		let base = ArkScaleProjective::from(base).encode();
		let scalar = ArkScale::from(scalar).encode();

		let res = host_calls::ed_on_bls12_381_bandersnatch_sw_mul_projective(base, scalar)
			.unwrap_or_default();
		let res = ArkScaleProjective::<SWProjective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
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
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::EdwardsProjective>`.
	///   - `scalars`: `ArkScale<&[ark_ed_on_bls12_381_bandersnatch::Fr]>`.
	/// - Returns encoded:
	///   `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::EdwardsProjective>`.
	fn ed_on_bls12_381_bandersnatch_te_msm(
		bases: Vec<u8>,
		scalars: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		crate::utils::msm_te::<ark_ed_on_bls12_381_bandersnatch::EdwardsConfig>(bases, scalars)
	}

	/// Twisted Edwards projective multiplication for *Ed-on-BLS12-381-Bandersnatch*.
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
		crate::utils::mul_projective_te::<ark_ed_on_bls12_381_bandersnatch::EdwardsConfig>(
			base, scalar,
		)
	}

	/// Short Weierstrass multi scalar multiplication for *Ed-on-BLS12-381-Bandersnatch*.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_ed_on_bls12_381_bandersnatch::SWAffine]>`.
	///   - `scalars`: `ArkScale<&[ark_ed_on_bls12_381_bandersnatch::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::SWProjective>`.
	fn ed_on_bls12_381_bandersnatch_sw_msm(
		bases: Vec<u8>,
		scalars: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		crate::utils::msm_sw::<ark_ed_on_bls12_381_bandersnatch::SWConfig>(bases, scalars)
	}

	/// Short Weierstrass projective multiplication for *Ed-on-BLS12-381-Bandersnatch*.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::SWProjective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::SWProjective>`.
	fn ed_on_bls12_381_bandersnatch_sw_mul_projective(
		base: Vec<u8>,
		scalar: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		crate::utils::mul_projective_sw::<ark_ed_on_bls12_381_bandersnatch::SWConfig>(base, scalar)
	}
}
