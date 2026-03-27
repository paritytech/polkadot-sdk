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

//! *Pallas* types and host functions.

use crate::utils::{self, HostcallResult, FAIL_MSG};
use alloc::vec::Vec;
use ark_ec::{AffineRepr, CurveConfig, CurveGroup};
use ark_pallas_ext::CurveHooks;
use sp_runtime_interface::{
	pass_by::{PassFatPointerAndRead, PassFatPointerAndWrite},
	runtime_interface,
};

/// Group configuration.
pub type PallasConfig = ark_pallas_ext::PallasConfig<HostHooks>;
/// Short Weierstrass form point affine representation.
pub type Affine = ark_pallas_ext::Affine<HostHooks>;
/// Short Weierstrass form point projective representation.
pub type Projective = ark_pallas_ext::Projective<HostHooks>;

/// Group scalar field (Fr).
pub type ScalarField = <PallasConfig as CurveConfig>::ScalarField;

/// Curve hooks jumping into [`host_calls`] host functions.
#[derive(Copy, Clone)]
pub struct HostHooks;

impl CurveHooks for HostHooks {
	fn msm(bases: &[Affine], scalars: &[ScalarField]) -> Projective {
		let mut out = utils::buffer_for::<Affine>();
		host_calls::pallas_msm(&utils::encode(bases), &utils::encode(scalars), &mut out)
			.and_then(|_| utils::decode::<Affine>(&out))
			.expect(FAIL_MSG)
			.into_group()
	}

	fn mul_projective(base: &Projective, scalar: &[u64]) -> Projective {
		let mut out = utils::buffer_for::<Affine>();
		host_calls::pallas_mul(&utils::encode(base.into_affine()), &utils::encode(scalar), &mut out)
			.and_then(|_| utils::decode::<Affine>(&out))
			.expect(FAIL_MSG)
			.into_group()
	}
}

/// Interfaces for working with *Arkworks* *Pallas* elliptic curve related types
/// from within the runtime.
///
/// All types are (de-)serialized through the wrapper types from `ark-scale`.
///
/// `ArkScale`'s `Usage` generic parameter is expected to be set to "not-validated"
/// and "not-compressed".
#[runtime_interface]
pub trait HostCalls {
	/// Short Weierstrass multi scalar multiplication for *Pallas*.
	///
	/// Receives encoded:
	/// - `bases`: `Vec<Affine>`.
	/// - `scalars`: `Vec<ScalarField>`.
	/// Writes encoded `Affine` to `out`.
	fn pallas_msm(
		bases: PassFatPointerAndRead<&[u8]>,
		scalars: PassFatPointerAndRead<&[u8]>,
		out: PassFatPointerAndWrite<&mut [u8]>,
	) -> HostcallResult {
		utils::msm_sw::<ark_pallas::PallasConfig>(bases, scalars, out)
	}

	/// Short Weierstrass affine multiplication for *Pallas*.
	///
	/// Receives encoded:
	/// - `base`: `Affine`.
	/// - `scalar`: `BigInteger`.
	/// Writes encoded `Affine` to `out`.
	fn pallas_mul(
		base: PassFatPointerAndRead<&[u8]>,
		scalar: PassFatPointerAndRead<&[u8]>,
		out: PassFatPointerAndWrite<&mut [u8]>,
	) -> HostcallResult {
		utils::mul_sw::<ark_pallas::PallasConfig>(base, scalar, out)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::utils::testing::*;

	#[test]
	fn mul_works() {
		mul_test::<Affine, ark_pallas::Affine>();
	}

	#[test]
	fn msm_works() {
		msm_test::<Affine, ark_pallas::Affine>();
	}
}
